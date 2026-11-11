use axum::{extract::State, http::StatusCode, response::Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{ensure_admin, Principal};
use crate::routes::services::{query, send, ApiResult};
use crate::state::{AppState, Service};
use crate::transport::Transport;

/// The active source, from the Audio Player. Sources it doesn't report are idle.
pub async fn get_source(State(state): State<AppState>) -> ApiResult {
    let status = query(&state, Service::AudioPlayer, "status").await?;
    // Radio plays through MPD, which reports its own state (RAD_REQ_001).
    let radio_playing = status["radio"]["state"] == "playing";
    let active = status
        .get("active")
        .and_then(Value::as_str)
        .or(radio_playing.then_some("radio"));
    let source_state = |name: &str| {
        if active == Some(name) {
            "playing"
        } else {
            "idle"
        }
    };
    Ok(Json(json!({
        "active": active,
        "sources": {
            "airplay": { "state": source_state("airplay") },
            "spotify": { "state": source_state("spotify") },
            "radio":   {
                "state": source_state("radio"),
                "station": status["radio"]["station"],
                "title": status["radio"]["title"]
            }
        }
    })))
}

/// Sends a volume command; the Audio Player's refusals are the client's
/// request failing (422).
async fn volume_command(state: &AppState, request: Value) -> ApiResult {
    match send(state, Service::AudioPlayer, &request).await {
        Ok(reply) => Ok(Json(reply)),
        Err((StatusCode::BAD_GATEWAY, body)) => Err((StatusCode::UNPROCESSABLE_ENTITY, body)),
        Err(other) => Err(other),
    }
}

/// The device volume and the limit (VOL_REQ_001).
pub async fn get_volume(State(state): State<AppState>) -> ApiResult {
    volume_command(&state, json!({"cmd": "volume"})).await
}

#[derive(Deserialize)]
pub struct VolumeBody {
    pub volume: Value,
}

/// Sets the device volume; above the limit it is capped (anyone with a token).
pub async fn set_volume(State(state): State<AppState>, Json(body): Json<VolumeBody>) -> ApiResult {
    volume_command(&state, json!({"cmd": "set_volume", "volume": body.volume})).await
}

pub async fn get_volume_limit(State(state): State<AppState>) -> ApiResult {
    volume_command(&state, json!({"cmd": "volume"})).await
}

#[derive(Deserialize)]
pub struct VolumeLimitBody {
    pub limit: Value,
}

/// Sets the maximum volume: a parental control, so an admin token is needed.
pub async fn set_volume_limit(
    State(state): State<AppState>,
    principal: Option<axum::Extension<Principal>>,
    transport: Option<axum::Extension<Transport>>,
    Json(body): Json<VolumeLimitBody>,
) -> ApiResult {
    ensure_admin(
        principal.as_ref().map(|p| &p.0),
        transport.as_ref().map(|t| &t.0),
    )?;
    volume_command(&state, json!({"cmd": "set_limit", "limit": body.limit})).await
}
