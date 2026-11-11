//! Recorded voice messages (MSG_REQ_001-003): WAV checks, the store, expiry and
//! the HTTP routes against a stand-in Audio Player.

use api_server::local_app as app;
use api_server::messages::{wav_info, MessageStore, MAX_BYTES, UNNAMED_TTL};
use api_server::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tower::ServiceExt;

/// A WAV file: `extra` chunks go between fmt and data.
fn wav(rate: u32, channels: u16, bits: u16, seconds: f64, extra: &[u8]) -> Vec<u8> {
    let data_len = (rate as f64 * seconds) as u32 * channels as u32 * (bits as u32 / 8);
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + extra.len() as u32 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * channels as u32 * bits as u32 / 8).to_le_bytes());
    v.extend_from_slice(&(channels * bits / 8).to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v.extend_from_slice(extra);
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    v.resize(v.len() + data_len as usize, 0);
    v
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-messages-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn accepted_recordings() {
    let info = wav_info(&wav(24_000, 1, 16, 2.0, &[])).unwrap();
    assert_eq!((info.rate, info.channels), (24_000, 1));
    assert!((info.duration_s - 2.0).abs() < 0.01);
    assert!(
        wav_info(&wav(48_000, 2, 16, 30.0, &[])).is_ok(),
        "30 s of 48 kHz stereo"
    );
    // A LIST chunk before the audio, as some encoders write.
    let list = [b"LIST".as_slice(), &4u32.to_le_bytes(), b"INFO"].concat();
    assert!(wav_info(&wav(16_000, 1, 16, 1.0, &list)).is_ok());
    // A streaming recorder may leave the data length unset.
    let mut streamed = wav(24_000, 1, 16, 1.0, &[]);
    streamed[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!((wav_info(&streamed).unwrap().duration_s - 1.0).abs() < 0.01);
}

#[test]
fn rejected_recordings() {
    for (bytes, why) in [
        (b"OggS not a wav".to_vec(), "not a WAV"),
        (wav(24_000, 1, 8, 1.0, &[]), "16-bit"),
        (wav(24_000, 3, 16, 1.0, &[]), "mono or stereo"),
        (wav(96_000, 1, 16, 1.0, &[]), "8-48 kHz"),
        (wav(24_000, 1, 16, 31.0, &[]), "limit is 30 s"),
        (wav(24_000, 1, 16, 0.0, &[]), "empty"),
    ] {
        let err = wav_info(&bytes).unwrap_err();
        assert!(err.contains(why), "{why}: {err}");
    }
}

#[test]
fn messages_are_stored_listed_renamed_and_deleted() {
    let store = MessageStore::new(temp_dir("store"));
    let t0 = SystemTime::now();
    let first = store.save(&wav(24_000, 1, 16, 2.0, &[]), None, t0).unwrap();
    let second = store
        .save(
            &wav(24_000, 1, 16, 3.5, &[]),
            Some("  Godnat  "),
            t0 + Duration::from_secs(60),
        )
        .unwrap();
    assert_eq!(second.name.as_deref(), Some("Godnat"));
    assert_eq!(second.duration_s, 3.5);
    let ids: Vec<_> = store.list().into_iter().map(|m| m.id).collect();
    assert_eq!(
        ids,
        vec![second.id.clone(), first.id.clone()],
        "newest first"
    );

    assert_eq!(
        store
            .rename(&first.id, Some("Mad"))
            .unwrap()
            .name
            .as_deref(),
        Some("Mad")
    );
    assert_eq!(
        store.rename(&first.id, Some("")).unwrap().name,
        None,
        "an empty name removes it"
    );
    assert!(store.rename(&first.id, Some(&"x".repeat(101))).is_err());
    store.delete(&first.id).unwrap();
    assert!(store.get(&first.id).unwrap_err().starts_with("no message"));
    assert_eq!(store.list().len(), 1);
    assert!(store.audio_path("../../etc/passwd").is_err());
}

#[test]
fn unnamed_messages_expire_named_ones_stay() {
    let dir = temp_dir("evict");
    let store = MessageStore::new(&dir);
    let long_ago = SystemTime::now() - UNNAMED_TTL - Duration::from_secs(3600);
    let old_unnamed = store
        .save(&wav(8_000, 1, 16, 1.0, &[]), None, long_ago)
        .unwrap();
    let old_named = store
        .save(&wav(8_000, 1, 16, 1.0, &[]), Some("Keep"), long_ago)
        .unwrap();
    let replayed = store
        .save(&wav(8_000, 1, 16, 1.0, &[]), None, long_ago)
        .unwrap();
    store.mark_played(&replayed.id, SystemTime::now()).unwrap();
    // A file from an interrupted save, long ago, and one being written now.
    let stale = dir.join("stale.wav.tmp");
    std::fs::write(&stale, b"x").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    let fresh = dir.join("fresh.wav.tmp");
    std::fs::write(&fresh, b"x").unwrap();

    assert_eq!(store.evict(SystemTime::now()), 1);
    let left: Vec<_> = store.list().into_iter().map(|m| m.id).collect();
    assert!(!left.contains(&old_unnamed.id));
    assert!(left.contains(&old_named.id), "named messages are kept");
    assert!(left.contains(&replayed.id), "playing restarts the 90 days");
    assert!(!stale.exists() && fresh.exists());
}

// ---- HTTP ----

/// A stand-in Audio Player that records play_message requests.
async fn audio_player(run: &Path) -> Arc<Mutex<Vec<Value>>> {
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let path = run.join("audio.sock");
    let server_path = path.clone();
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let log = Arc::clone(&log);
            async move {
                log.lock().unwrap().push(request.clone());
                match command(&request) {
                    Some("play_message") => ok(json!({"position": 1})),
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

async fn call(
    run: &Path,
    method: &str,
    uri: &str,
    body: Body,
    json_body: bool,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(uri);
    if json_body {
        request = request.header("content-type", "application/json");
    }
    let response = app(AppState::with_run_dir(run))
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
}

fn as_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn an_upload_is_stored_and_played_at_once() {
    let run = temp_dir("http-upload");
    let seen = audio_player(&run).await;
    let (status, body) = call(
        &run,
        "POST",
        "/api/messages",
        Body::from(wav(24_000, 1, 16, 2.0, &[])),
        false,
    )
    .await;
    let body = as_json(&body);
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["played"], true);
    let id = body["message"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        seen.lock().unwrap()[0],
        json!({"cmd": "play_message", "id": id})
    );

    let (status, list) = call(&run, "GET", "/api/messages", Body::empty(), false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(as_json(&list)["messages"][0]["id"], id.as_str());
    let (status, audio) = call(
        &run,
        "GET",
        &format!("/api/messages/{id}/audio"),
        Body::empty(),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&audio[0..4], b"RIFF");
}

#[tokio::test]
async fn play_false_only_stores_and_a_missing_player_still_keeps_it() {
    let run = temp_dir("http-noplay");
    let (status, body) = call(
        &run,
        "POST",
        "/api/messages?play=false&name=Hej",
        Body::from(wav(24_000, 1, 16, 1.0, &[])),
        false,
    )
    .await;
    let body = as_json(&body);
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        (body["played"].clone(), body["message"]["name"].clone()),
        (json!(false), json!("Hej"))
    );
    // No Audio Player is running here: the message is kept, the error reported.
    let (status, body) = call(
        &run,
        "POST",
        "/api/messages",
        Body::from(wav(24_000, 1, 16, 1.0, &[])),
        false,
    )
    .await;
    let body = as_json(&body);
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["played"], false);
    assert!(body["play_error"].as_str().unwrap().contains("unavailable"));
    let (_, list) = call(&run, "GET", "/api/messages", Body::empty(), false).await;
    assert_eq!(as_json(&list)["messages"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn bad_uploads_are_refused() {
    let run = temp_dir("http-bad");
    let (status, body) = call(
        &run,
        "POST",
        "/api/messages",
        Body::from(wav(24_000, 1, 16, 40.0, &[])),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(as_json(&body)["error"].as_str().unwrap().contains("30 s"));
    let (status, _) = call(
        &run,
        "POST",
        "/api/messages",
        Body::from(vec![0u8; MAX_BYTES + 1]),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn messages_can_be_renamed_replayed_and_deleted() {
    let run = temp_dir("http-manage");
    let seen = audio_player(&run).await;
    let (_, body) = call(
        &run,
        "POST",
        "/api/messages?play=false",
        Body::from(wav(24_000, 1, 16, 1.0, &[])),
        false,
    )
    .await;
    let id = as_json(&body)["message"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = call(
        &run,
        "PATCH",
        &format!("/api/messages/{id}"),
        Body::from(r#"{"name": "Mormor"}"#),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(as_json(&body)["message"]["name"], "Mormor");
    let (status, _) = call(
        &run,
        "POST",
        &format!("/api/messages/{id}/play"),
        Body::empty(),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(seen.lock().unwrap().len(), 1);
    let (status, _) = call(
        &run,
        "DELETE",
        &format!("/api/messages/{id}"),
        Body::empty(),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for (method, uri) in [
        ("POST", format!("/api/messages/{id}/play")),
        ("DELETE", format!("/api/messages/{id}")),
        ("GET", format!("/api/messages/{id}/audio")),
    ] {
        let (status, _) = call(&run, method, &uri, Body::empty(), false).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}
