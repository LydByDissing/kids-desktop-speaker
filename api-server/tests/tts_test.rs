//! `POST /api/tts` against a stand-in Audio Player on a real socket.

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
    std::env::temp_dir().join(format!("kds-api-tts-{}-{name}", std::process::id()))
}

/// A stand-in Audio Player that records `announce` requests and answers like the
/// real one: unknown languages and a full queue are errors.
async fn audio_player(dir: &Path) -> Arc<Mutex<Vec<Value>>> {
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let path = dir.join("audio.sock");
    let server_path = path.clone();
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let log = Arc::clone(&log);
            async move {
                if command(&request) != Some("announce") {
                    return error("unknown command");
                }
                log.lock().unwrap().push(request.clone());
                match (request["text"].as_str(), request["lang"].as_str()) {
                    (Some("full"), _) => error("announcement queue full (10)"),
                    (_, Some("fr")) => error("language not enabled: fr"),
                    _ => ok(json!({"position": 1})),
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

async fn post(dir: &Path, body: Value) -> (StatusCode, Value) {
    let response = app(AppState::with_run_dir(dir))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tts")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
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
async fn announcement_is_queued_with_the_audio_player() {
    let dir = run_dir("ok");
    let seen = audio_player(&dir).await;
    let (status, body) = post(&dir, json!({"text": "Hello", "lang": "en-GB"})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body, json!({"position": 1}));
    assert_eq!(
        seen.lock().unwrap()[0],
        json!({"cmd": "announce", "text": "Hello", "lang": "en-GB"})
    );
}

#[tokio::test]
async fn language_is_optional() {
    let dir = run_dir("nolang");
    let seen = audio_player(&dir).await;
    let (status, _) = post(&dir, json!({"text": "Hej"})).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(seen.lock().unwrap()[0]["lang"], Value::Null);
}

#[tokio::test]
async fn empty_text_is_400_without_asking_the_audio_player() {
    let dir = run_dir("empty");
    let seen = audio_player(&dir).await;
    let (status, body) = post(&dir, json!({"text": "  "})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "text must not be empty");
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rejected_language_is_400_and_full_queue_is_429() {
    let dir = run_dir("errors");
    audio_player(&dir).await;
    let (status, body) = post(&dir, json!({"text": "Bonjour", "lang": "fr"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("language not enabled"));
    let (status, _) = post(&dir, json!({"text": "full"})).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn missing_audio_player_is_503() {
    let (status, _) = post(&run_dir("down"), json!({"text": "Hej"})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn malformed_body_is_a_client_error() {
    let (status, _) = post(&run_dir("bad"), json!({"message": "Hej"})).await;
    assert!(status.is_client_error(), "{status}");
}
