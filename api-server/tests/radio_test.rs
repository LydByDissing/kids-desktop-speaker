//! The radio API against a stand-in Audio Player on a real socket (RAD_REQ_001).

use api_server::local_app as app;
use api_server::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;

fn run_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("kds-api-radio-{}-{name}", std::process::id()))
}

/// Answers the radio commands like the Audio Player, and records each request.
async fn audio_player(dir: &Path, radio_state: &'static str) -> Arc<Mutex<Vec<Value>>> {
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let path = dir.join("audio.sock");
    let server_path = path.clone();
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let log = Arc::clone(&log);
            async move {
                log.lock().unwrap().push(request.clone());
                let id = request["id"].as_str().unwrap_or_default().to_string();
                match command(&request) {
                    Some("status") => ok(json!({"active": null,
                        "radio": {"state": radio_state, "station": "dr-p3", "title": "Go' Morgen P3"}})),
                    Some("radio_stations") => ok(json!({"stations": [{"id": "dr-p3", "name": "DR P3", "url": "http://x"}]})),
                    Some("radio_add") if request["url"].as_str().unwrap().starts_with("http") =>
                        ok(json!({"station": {"id": "radio-4", "name": request["name"], "url": request["url"]}})),
                    Some("radio_add") => error("url must start with http:// or https://"),
                    Some("radio_play" | "radio_remove") if id == "dr-p3" => ok(json!({"station": {"id": "dr-p3"}})),
                    Some("radio_play" | "radio_remove") => error(format!("no station {id}")),
                    Some("radio_stop") => error("mpd unreachable: connection refused"),
                    _ => error("unknown command"),
                }
            }
        })
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("stand-in audio player did not start");
}

async fn call(dir: &Path, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if body.is_some() {
        request = request.header("content-type", "application/json");
    }
    let body = body
        .map(|b| Body::from(b.to_string()))
        .unwrap_or_else(Body::empty);
    let response = app(AppState::with_run_dir(dir))
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn stations_are_listed_added_and_removed() {
    let dir = run_dir("list");
    let seen = audio_player(&dir, "stopped").await;
    let (status, body) = call(&dir, "GET", "/api/radio/stations", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stations"][0]["name"], "DR P3");

    let (status, body) = call(
        &dir,
        "POST",
        "/api/radio/stations",
        Some(json!({"name": "Radio 4", "url": "http://example.com/r4"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["station"]["id"], "radio-4");

    let (status, _) = call(&dir, "DELETE", "/api/radio/stations/dr-p3", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &json!({"cmd": "radio_remove", "id": "dr-p3"})
    );
}

#[tokio::test]
async fn bad_station_urls_are_400_and_unknown_stations_404() {
    let dir = run_dir("errors");
    audio_player(&dir, "stopped").await;
    let (status, body) = call(
        &dir,
        "POST",
        "/api/radio/stations",
        Some(json!({"name": "X", "url": "file:///etc/passwd"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("http"));
    let (status, _) = call(&dir, "POST", "/api/radio/play", Some(json!({"id": "nope"}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&dir, "DELETE", "/api/radio/stations/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn play_starts_a_station_and_mpd_failures_are_502() {
    let dir = run_dir("play");
    let seen = audio_player(&dir, "stopped").await;
    let (status, _) = call(
        &dir,
        "POST",
        "/api/radio/play",
        Some(json!({"id": "dr-p3"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &json!({"cmd": "radio_play", "id": "dr-p3"})
    );
    let (status, _) = call(&dir, "POST", "/api/radio/stop", None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn playing_radio_shows_as_the_active_source() {
    let dir = run_dir("source");
    audio_player(&dir, "playing").await;
    let (status, body) = call(&dir, "GET", "/api/audio/source", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], "radio");
    assert_eq!(body["sources"]["radio"]["state"], "playing");
    assert_eq!(body["sources"]["radio"]["station"], "dr-p3");
    assert_eq!(body["sources"]["radio"]["title"], "Go' Morgen P3");
}

#[tokio::test]
async fn radio_without_the_audio_player_is_503() {
    let (status, _) = call(&run_dir("down"), "GET", "/api/radio/stations", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
