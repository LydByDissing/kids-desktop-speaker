use axum::{extract::State, http::StatusCode, response::Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::services::{query, ApiResult};
use crate::state::{AppState, Service};

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

pub async fn get_volume_limit(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let guard = state.vol_limit.read().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"state poisoned"})),
        )
    })?;
    Ok(Json(json!({ "limit": *guard })))
}

#[derive(Deserialize)]
pub struct VolumeLimitBody {
    pub limit: u8,
}

pub async fn set_volume_limit(
    State(state): State<AppState>,
    Json(body): Json<VolumeLimitBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if body.limit > 100 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "limit must be between 0 and 100" })),
        ));
    }
    match state.vol_limit.write() {
        Ok(mut guard) => {
            *guard = body.limit;
            Ok(Json(json!({ "limit": body.limit })))
        }
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal state error" })),
        )),
    }
}
