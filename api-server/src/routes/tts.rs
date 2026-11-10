//! `POST /api/tts`: speak a message on the speaker (TTS_REQ_002, DEC-033).

use axum::{extract::State, http::StatusCode, response::Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::services::send;
use crate::state::{AppState, Service};

#[derive(Debug, Deserialize)]
pub struct TtsRequest {
    pub text: String,
    /// A BCP 47 tag such as `da-DK` or `en-GB`; Danish when absent (TTS_REQ_003).
    pub lang: Option<String>,
}

/// Queues an announcement with the Audio Player. Answers 202 with the queue
/// position, 400 for a request the Audio Player rejects (empty text, language
/// not enabled), 429 when the queue is full, and 503 when the Audio Player is
/// not running.
pub async fn post_tts(
    State(state): State<AppState>,
    Json(body): Json<TtsRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    if body.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "text must not be empty" })),
        ));
    }
    let request = json!({ "cmd": "announce", "text": body.text, "lang": body.lang });
    match send(&state, Service::AudioPlayer, &request).await {
        Ok(response) => Ok((StatusCode::ACCEPTED, Json(response))),
        Err((StatusCode::BAD_GATEWAY, Json(error))) => {
            let message = error["error"].as_str().unwrap_or_default();
            let status = if message.contains("queue full") {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::BAD_REQUEST
            };
            Err((status, Json(error)))
        }
        Err(other) => Err(other),
    }
}
