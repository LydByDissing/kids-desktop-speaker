//! Internet radio (RAD_REQ_001): the station list and playback, served by the
//! Audio Player.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::services::send;
use crate::state::{AppState, Service};

type RadioResult = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

/// Sends a radio command. The Audio Player's own errors (an unknown station, a
/// bad URL, MPD not playing) are the client's request failing: 400, or 404 for
/// a station that doesn't exist.
async fn radio(state: &AppState, request: Value, success: StatusCode) -> RadioResult {
    match send(state, Service::AudioPlayer, &request).await {
        Ok(response) => Ok((success, Json(response))),
        Err((StatusCode::BAD_GATEWAY, Json(error))) => {
            // The message is "audio-player: <the Audio Player's error>".
            let full = error["error"].as_str().unwrap_or_default();
            let message = full.split_once(": ").map(|(_, m)| m).unwrap_or(full);
            let status = if message.starts_with("no station") {
                StatusCode::NOT_FOUND
            } else if message.starts_with("mpd") || message == "radio unavailable" {
                StatusCode::BAD_GATEWAY
            } else {
                StatusCode::BAD_REQUEST
            };
            Err((status, Json(error)))
        }
        Err(other) => Err(other),
    }
}

pub async fn get_stations(State(state): State<AppState>) -> RadioResult {
    radio(&state, json!({"cmd": "radio_stations"}), StatusCode::OK).await
}

#[derive(Debug, Deserialize)]
pub struct NewStation {
    pub name: String,
    pub url: String,
}

pub async fn add_station(
    State(state): State<AppState>,
    Json(body): Json<NewStation>,
) -> RadioResult {
    let request = json!({"cmd": "radio_add", "name": body.name, "url": body.url});
    radio(&state, request, StatusCode::CREATED).await
}

pub async fn remove_station(State(state): State<AppState>, Path(id): Path<String>) -> RadioResult {
    radio(
        &state,
        json!({"cmd": "radio_remove", "id": id}),
        StatusCode::OK,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct PlayRequest {
    pub id: String,
}

pub async fn play(State(state): State<AppState>, Json(body): Json<PlayRequest>) -> RadioResult {
    radio(
        &state,
        json!({"cmd": "radio_play", "id": body.id}),
        StatusCode::OK,
    )
    .await
}

pub async fn stop(State(state): State<AppState>) -> RadioResult {
    radio(&state, json!({"cmd": "radio_stop"}), StatusCode::OK).await
}
