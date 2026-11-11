//! Live state from the backend services over IPC (IPC_REQ_004).

use axum::{extract::State, http::StatusCode, response::Json};
use kds_ipc::IpcError;
use serde_json::{json, Value};

use crate::state::{AppState, Service};

pub type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

/// Sends `cmd` to a service and returns its response without the `ok` field. A
/// service that can't be reached is a 503; a service that answers with an error
/// is a 502.
pub async fn query(
    state: &AppState,
    service: Service,
    cmd: &str,
) -> Result<Value, (StatusCode, Json<Value>)> {
    send(state, service, &json!({ "cmd": cmd })).await
}

/// Sends a full request (a `cmd` plus its arguments) to a service, with the same
/// error mapping as [`query`].
pub async fn send(
    state: &AppState,
    service: Service,
    request: &Value,
) -> Result<Value, (StatusCode, Json<Value>)> {
    send_within(state, service, request, std::time::Duration::from_secs(5)).await
}

/// As [`send`], allowing the service `limit` to answer: for commands that fetch
/// from the internet, such as a podcast feed.
pub async fn send_within(
    state: &AppState,
    service: Service,
    request: &Value,
    limit: std::time::Duration,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let reply = kds_ipc::request_with_timeouts(
        state.socket(service),
        request,
        std::time::Duration::from_secs(2),
        limit,
    )
    .await;
    match reply {
        Ok(mut response) => {
            if let Some(map) = response.as_object_mut() {
                map.remove("ok");
            }
            Ok(response)
        }
        Err(IpcError::Remote(message)) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("{}: {message}", service.name()) })),
        )),
        Err(e) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": format!("{} unavailable: {e}", service.name()) })),
        )),
    }
}

pub async fn get_ota_status(State(state): State<AppState>) -> ApiResult {
    query(&state, Service::OtaUpdater, "status").await.map(Json)
}

/// Asks the OTA Updater to check for an update now (OTA_REQ_007).
pub async fn post_ota_check(State(state): State<AppState>) -> ApiResult {
    query(&state, Service::OtaUpdater, "check").await.map(Json)
}

/// Asks the OTA Updater to reboot now, skipping the maintenance window (OTA_REQ_007).
pub async fn post_ota_reboot(State(state): State<AppState>) -> ApiResult {
    query(&state, Service::OtaUpdater, "reboot").await.map(Json)
}

pub async fn get_bluetooth_status(State(state): State<AppState>) -> ApiResult {
    query(&state, Service::Bluetooth, "status").await.map(Json)
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct PairRequest {
    /// How long the pairing window stays open; 180 s when absent (BT_REQ_002).
    pub seconds: Option<u64>,
}

/// Opens a Bluetooth pairing window, so a phone can pair (BT_REQ_002). The body
/// is optional.
pub async fn post_bluetooth_pair(
    State(state): State<AppState>,
    body: Option<Json<PairRequest>>,
) -> ApiResult {
    let seconds = body.and_then(|Json(b)| b.seconds);
    send(
        &state,
        Service::Bluetooth,
        &json!({ "cmd": "pair", "seconds": seconds }),
    )
    .await
    .map(Json)
}
