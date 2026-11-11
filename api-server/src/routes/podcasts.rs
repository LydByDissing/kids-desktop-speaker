//! Podcasts (POD_REQ_001/002), served by the Audio Player.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use crate::routes::services::send_within;
use crate::state::{AppState, Service};

type Reply = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

/// Feeds are fetched from the internet, so the Audio Player gets 30 s.
const FETCH_LIMIT: Duration = Duration::from_secs(30);

/// Sends a podcast command. An unknown feed or episode is 404, an unreachable
/// feed or MPD 502, anything else the Audio Player refuses 400.
async fn podcast(state: &AppState, request: Value, success: StatusCode) -> Reply {
    match send_within(state, Service::AudioPlayer, &request, FETCH_LIMIT).await {
        Ok(response) => Ok((success, Json(response))),
        Err((StatusCode::BAD_GATEWAY, Json(error))) => {
            let full = error["error"].as_str().unwrap_or_default();
            let message = full.split_once(": ").map(|(_, m)| m).unwrap_or(full);
            let status = if message.starts_with("no podcast") || message.starts_with("no episode") {
                StatusCode::NOT_FOUND
            } else if message.starts_with("cannot fetch")
                || message.starts_with("the feed answered")
                || message.starts_with("mpd")
                || message == "podcasts unavailable"
            {
                StatusCode::BAD_GATEWAY
            } else {
                StatusCode::BAD_REQUEST
            };
            Err((status, Json(error)))
        }
        Err(other) => Err(other),
    }
}

pub async fn list(State(state): State<AppState>) -> Reply {
    podcast(&state, json!({"cmd": "podcast_feeds"}), StatusCode::OK).await
}

#[derive(Debug, Deserialize)]
pub struct NewFeed {
    pub url: String,
}

pub async fn add(State(state): State<AppState>, Json(body): Json<NewFeed>) -> Reply {
    podcast(
        &state,
        json!({"cmd": "podcast_add", "url": body.url}),
        StatusCode::CREATED,
    )
    .await
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    podcast(
        &state,
        json!({"cmd": "podcast_remove", "id": id}),
        StatusCode::OK,
    )
    .await
}

pub async fn episodes(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    podcast(
        &state,
        json!({"cmd": "podcast_episodes", "id": id}),
        StatusCode::OK,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct Play {
    pub feed: String,
    pub episode: String,
}

pub async fn play(State(state): State<AppState>, Json(body): Json<Play>) -> Reply {
    let request = json!({"cmd": "podcast_play", "feed": body.feed, "episode": body.episode});
    podcast(&state, request, StatusCode::OK).await
}

pub async fn stop(State(state): State<AppState>) -> Reply {
    podcast(&state, json!({"cmd": "podcast_stop"}), StatusCode::OK).await
}
