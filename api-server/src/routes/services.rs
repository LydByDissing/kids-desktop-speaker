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
    match kds_ipc::request(state.socket(service), &json!({ "cmd": cmd })).await {
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

pub async fn get_bluetooth_status(State(state): State<AppState>) -> ApiResult {
    query(&state, Service::Bluetooth, "status").await.map(Json)
}
