//! The podcast API against a stand-in Audio Player (POD_REQ_001/002).

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
    std::env::temp_dir().join(format!("kds-api-pod-{}-{name}", std::process::id()))
}

async fn audio_player(dir: &Path) -> Arc<Mutex<Vec<Value>>> {
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let path = dir.join("audio.sock");
    let server_path = path.clone();
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let log = Arc::clone(&log);
            async move {
                log.lock().unwrap().push(request.clone());
                let id = request["id"]
                    .as_str()
                    .or(request["feed"].as_str())
                    .unwrap_or_default()
                    .to_string();
                match command(&request) {
                    Some("podcast_feeds") => ok(json!({"feeds": []})),
                    Some("podcast_add") if request["url"] == "http://down.example/feed" => {
                        error("cannot fetch the feed: connection refused")
                    }
                    Some("podcast_add") => ok(json!({"feed": {"id": "f1", "title": "Børneradio"}})),
                    Some("podcast_episodes" | "podcast_remove" | "podcast_play") if id != "f1" => {
                        error(format!("no podcast {id}"))
                    }
                    Some("podcast_episodes") => ok(json!({"episodes": []})),
                    Some("podcast_remove" | "podcast_stop") => ok(Value::Null),
                    Some("podcast_play") => ok(json!({"episode": {"id": request["episode"]}})),
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
async fn podcasts_are_added_listed_played_and_removed() {
    let dir = run_dir("ok");
    let seen = audio_player(&dir).await;
    assert_eq!(
        call(&dir, "GET", "/api/podcasts", None).await.0,
        StatusCode::OK
    );
    let (status, body) = call(
        &dir,
        "POST",
        "/api/podcasts",
        Some(json!({"url": "http://example.com/feed"})),
    )
    .await;
    assert_eq!(
        (status, body["feed"]["id"].clone()),
        (StatusCode::CREATED, json!("f1"))
    );
    assert_eq!(
        call(&dir, "GET", "/api/podcasts/f1/episodes", None).await.0,
        StatusCode::OK
    );
    let (status, _) = call(
        &dir,
        "POST",
        "/api/podcasts/play",
        Some(json!({"feed": "f1", "episode": "e1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &json!({"cmd": "podcast_play", "feed": "f1", "episode": "e1"})
    );
    assert_eq!(
        call(&dir, "POST", "/api/podcasts/stop", None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&dir, "DELETE", "/api/podcasts/f1", None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn unknown_feeds_are_404_and_unreachable_feeds_502() {
    let dir = run_dir("errors");
    audio_player(&dir).await;
    assert_eq!(
        call(&dir, "GET", "/api/podcasts/nope/episodes", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&dir, "DELETE", "/api/podcasts/nope", None).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _) = call(
        &dir,
        "POST",
        "/api/podcasts",
        Some(json!({"url": "http://down.example/feed"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}
