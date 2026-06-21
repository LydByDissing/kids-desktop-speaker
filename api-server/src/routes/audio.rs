use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn get_source(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let guard = state
        .active_source
        .read()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"state poisoned"}))))?;
    let active_str: Option<&str> = guard.as_ref().map(|s| match s {
        crate::state::Source::Airplay => "airplay",
        crate::state::Source::Spotify => "spotify",
        crate::state::Source::Radio => "radio",
    });
    Ok(Json(json!({
        "active": active_str,
        "sources": {
            "airplay": { "state": "idle" },
            "spotify": { "state": "idle" },
            "radio":   { "state": "idle" }
        }
    })))
}

pub async fn get_volume_limit(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let guard = state
        .vol_limit
        .read()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"state poisoned"}))))?;
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
