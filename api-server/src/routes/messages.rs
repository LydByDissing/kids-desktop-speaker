//! Recorded voice messages (MSG_REQ_001-003, DEC-037).

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::SystemTime;

use crate::routes::services::send;
use crate::state::{AppState, Service};

type Reply = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

fn fail(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message.into() })))
}

/// A store error: an unknown message is 404, anything else 400.
fn store_error(message: String) -> (StatusCode, Json<Value>) {
    let status = if message.starts_with("no message") {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::BAD_REQUEST
    };
    fail(status, message)
}

/// Asks the Audio Player to play a message; on success records that it was
/// played (which restarts an unnamed message's 90 days, MSG_REQ_003).
async fn play(state: &AppState, id: &str) -> Result<Value, (StatusCode, Json<Value>)> {
    let response = send(
        state,
        Service::AudioPlayer,
        &json!({"cmd": "play_message", "id": id}),
    )
    .await?;
    let _ = state.messages.mark_played(id, SystemTime::now());
    Ok(response)
}

#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    /// Play the message at once (the default, MSG_REQ_001).
    pub play: Option<bool>,
    pub name: Option<String>,
}

/// Stores an uploaded WAV recording and, unless `?play=false`, plays it. The
/// message is kept even if it can't be played now.
pub async fn upload(
    State(state): State<AppState>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Reply {
    let message = state
        .messages
        .save(&body, q.name.as_deref(), SystemTime::now())
        .map_err(|e| fail(StatusCode::BAD_REQUEST, e))?;
    let mut reply = json!({ "message": message, "played": false });
    if q.play.unwrap_or(true) {
        match play(&state, &message.id).await {
            Ok(r) => {
                reply["played"] = json!(true);
                reply["position"] = r["position"].clone();
            }
            Err((_, Json(e))) => reply["play_error"] = e["error"].clone(),
        }
    }
    Ok((StatusCode::CREATED, Json(reply)))
}

pub async fn list(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "messages": state.messages.list() }))
}

/// The recording itself, so the Web UI can play it back in the browser.
pub async fn audio(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state
        .messages
        .audio_path(&id)
        .and_then(|p| std::fs::read(p).map_err(|e| e.to_string()))
    {
        Ok(bytes) => ([(header::CONTENT_TYPE, "audio/wav")], bytes).into_response(),
        Err(e) => store_error(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct Rename {
    /// The new name; null or empty removes it.
    pub name: Option<String>,
}

pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Rename>,
) -> Reply {
    let message = state
        .messages
        .rename(&id, body.name.as_deref())
        .map_err(store_error)?;
    Ok((StatusCode::OK, Json(json!({ "message": message }))))
}

pub async fn replay(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    state.messages.get(&id).map_err(store_error)?;
    let response = play(&state, &id).await?;
    Ok((StatusCode::OK, Json(response)))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    state.messages.delete(&id).map_err(store_error)?;
    Ok((StatusCode::OK, Json(json!({}))))
}
