//! The API Server against stand-in services on real sockets (IPC_REQ_004).

use api_server::app;
use api_server::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tower::ServiceExt;

fn run_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("kds-api-{}-{name}", std::process::id()))
}

/// Starts a stand-in service that answers `status` with `fields`.
async fn stand_in(dir: &Path, socket: &str, fields: Value) {
    let path = dir.join(socket);
    let server_path = path.clone();
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let fields = fields.clone();
            async move {
                match command(&request) {
                    Some("status") => ok(fields),
                    Some(other) => error(format!("unknown command: {other}")),
                    None => error("missing cmd"),
                }
            }
        })
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("stand-in service did not start");
}

async fn get(dir: &Path, uri: &str) -> (StatusCode, Value) {
    let response = app(AppState::with_run_dir(dir))
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn audio_source_comes_from_the_audio_player() {
    let dir = run_dir("audio");
    stand_in(&dir, "audio.sock", json!({"active": "spotify"})).await;
    let (status, body) = get(&dir, "/api/audio/source").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], "spotify");
    assert_eq!(body["sources"]["spotify"]["state"], "playing");
    assert_eq!(body["sources"]["airplay"]["state"], "idle");
}

#[tokio::test]
async fn ota_status_comes_from_the_ota_updater() {
    let dir = run_dir("ota");
    stand_in(
        &dir,
        "ota.sock",
        json!({"state": "idle", "detail": null, "version": "v0.1.0"}),
    )
    .await;
    let (status, body) = get(&dir, "/api/ota/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"state": "idle", "detail": null, "version": "v0.1.0"})
    );
}

#[tokio::test]
async fn bluetooth_status_comes_from_the_bluetooth_service() {
    let dir = run_dir("bt");
    stand_in(
        &dir,
        "bt.sock",
        json!({"connected": true, "controller": null}),
    )
    .await;
    let (status, body) = get(&dir, "/api/bluetooth/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["connected"], true);
}

#[tokio::test]
async fn missing_services_are_503() {
    let dir = run_dir("none");
    for uri in [
        "/api/audio/source",
        "/api/ota/status",
        "/api/bluetooth/status",
    ] {
        let (status, body) = get(&dir, uri).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert!(
            body["error"].as_str().unwrap().contains("unavailable"),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn a_service_error_is_502() {
    let dir = run_dir("broken");
    let path = dir.join("ota.sock");
    let server_path = path.clone();
    tokio::spawn(async move {
        kds_ipc::serve(server_path, |_request: Value| async {
            error("state unavailable")
        })
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, body) = get(&dir, "/api/ota/status").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "ota-updater: state unavailable");
}

#[tokio::test]
async fn ota_check_and_reboot_are_forwarded() {
    let dir = run_dir("ota-post");
    let path = dir.join("ota.sock");
    let server_path = path.clone();
    tokio::spawn(async move {
        kds_ipc::serve(server_path, |request: Value| async move {
            match command(&request) {
                Some("check") | Some("reboot") => ok(json!({"accepted": command(&request)})),
                _ => error("unexpected"),
            }
        })
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    for (uri, cmd) in [("/api/ota/check", "check"), ("/api/ota/reboot", "reboot")] {
        let response = app(AppState::with_run_dir(&dir))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["accepted"], cmd);
    }
}
