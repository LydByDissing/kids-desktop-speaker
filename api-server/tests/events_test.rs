//! Live updates at /api/events (UI_REQ_004, DEC-046), against a stand-in
//! Audio Player.

use api_server::app;
use api_server::events::{same, watch};
use api_server::state::AppState;
use api_server::transport::Transport;
use axum::body::{Body, BodyDataStream};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use axum::{Extension, Router};
use futures_util::StreamExt;
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;

fn run_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-api-events-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// What the stand-in Audio Player holds.
#[derive(Default)]
struct Speaker {
    volume: u64,
    station: Option<String>,
}

/// A stand-in Audio Player with a volume and a radio that can play.
async fn audio_player(dir: &Path) -> Arc<Mutex<Speaker>> {
    let speaker = Arc::new(Mutex::new(Speaker {
        volume: 60,
        station: None,
    }));
    let path = dir.join("audio.sock");
    let server_path = path.clone();
    let sp = Arc::clone(&speaker);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let sp = Arc::clone(&sp);
            async move {
                let mut s = sp.lock().unwrap();
                match command(&request) {
                    Some("status") => {
                        let playing = s.station.is_some();
                        ok(json!({
                            "active": if playing { json!("radio") } else { Value::Null },
                            "radio": {
                                "state": if playing { "playing" } else { "stopped" },
                                "station": s.station,
                                "title": null
                            }
                        }))
                    }
                    Some("set_volume") => {
                        s.volume = request["volume"].as_u64().unwrap().min(75);
                        ok(json!({"volume": s.volume, "limit": 75}))
                    }
                    Some("volume") => ok(json!({"volume": s.volume, "limit": 75})),
                    _ => error("unknown command"),
                }
            }
        })
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            return speaker;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("stand-in audio player did not start");
}

fn net(state: AppState) -> Router {
    app(state)
        .layer(Extension(Transport::Https))
        .layer(MockConnectInfo(SocketAddr::from(([192, 168, 1, 50], 5000))))
}

fn request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json");
    builder
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap()
}

/// Reads server-sent events from a response body.
struct Events {
    body: BodyDataStream,
    buffer: String,
}

impl Events {
    /// The next event as (topic, data), or None if none arrives in `within`.
    async fn next(&mut self, within: Duration) -> Option<(String, Value)> {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let frame: String = self.buffer.drain(..end + 2).collect();
                let field = |name: &str| {
                    frame
                        .lines()
                        .find_map(|l| l.strip_prefix(name).map(str::to_string))
                };
                // Keep-alive comments have no event.
                if let (Some(topic), Some(data)) = (field("event: "), field("data: ")) {
                    return Some((topic, serde_json::from_str(&data).unwrap()));
                }
                continue;
            }
            let chunk = tokio::time::timeout_at(deadline, self.body.next())
                .await
                .ok()??
                .unwrap();
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }

    /// The next event of `topic`, skipping others.
    async fn next_of(&mut self, topic: &str, within: Duration) -> Option<Value> {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let (t, data) = self.next(left).await?;
            if t == topic {
                return Some(data);
            }
        }
    }
}

async fn subscribe(router: Router, token: &str) -> Events {
    let response = router
        .oneshot(request("GET", "/api/events", token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/event-stream",
        "server-sent events"
    );
    Events {
        body: response.into_body().into_data_stream(),
        buffer: String::new(),
    }
}

const SOON: Duration = Duration::from_secs(2);

#[tokio::test]
async fn a_subscriber_first_gets_every_topic_that_answers() {
    let dir = run_dir("first");
    audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let (_, token) = state.tokens.create("Idas iPad", false).unwrap();
    let mut events = subscribe(net(state), &token).await;
    let mut topics = Vec::new();
    while let Some((topic, data)) = events.next(Duration::from_millis(500)).await {
        if topic == "volume" {
            assert_eq!(data, json!({"volume": 60, "limit": 75}));
        }
        topics.push(topic);
    }
    // The stand-ins for Bluetooth and the OTA Updater aren't running.
    assert_eq!(
        topics,
        ["source", "volume", "timer"],
        "any token may subscribe"
    );
}

#[tokio::test]
async fn the_stream_needs_a_token() {
    let dir = run_dir("token");
    let response = net(AppState::with_run_dir(&dir))
        .oneshot(Request::get("/api/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_change_through_the_api_reaches_subscribers_at_once() {
    let dir = run_dir("api");
    audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let (_, token) = state.tokens.create("phone", false).unwrap();
    // A long interval: only the nudge after the change can deliver it in time.
    tokio::spawn(watch(state.clone(), Duration::from_secs(60)));
    let mut events = subscribe(net(state.clone()), &token).await;
    assert!(events.next_of("timer", SOON).await.is_some());

    let response = net(state.clone())
        .oneshot(request(
            "POST",
            "/api/audio/volume",
            &token,
            Some(json!({"volume": 30})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let volume = events
        .next_of("volume", SOON)
        .await
        .expect("a volume event");
    assert_eq!(volume["volume"], 30);

    net(state)
        .oneshot(request(
            "POST",
            "/api/timer",
            &token,
            Some(json!({"minutes": 15})),
        ))
        .await
        .unwrap();
    let timer = events.next_of("timer", SOON).await.expect("a timer event");
    assert_eq!(timer["running"], true);
    assert_eq!(timer["minutes"], 15);
}

#[tokio::test]
async fn a_change_elsewhere_arrives_within_the_interval() {
    let dir = run_dir("elsewhere");
    let speaker = audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let (_, token) = state.tokens.create("phone", false).unwrap();
    tokio::spawn(watch(state.clone(), Duration::from_millis(200)));
    let mut events = subscribe(net(state), &token).await;
    let first = events.next_of("source", SOON).await.unwrap();
    assert_eq!(first["active"], Value::Null);

    // The radio starts without the API, as an alarm or a remote would.
    speaker.lock().unwrap().station = Some("dr-p3".into());
    let source = events
        .next_of("source", SOON)
        .await
        .expect("a source event");
    assert_eq!(source["active"], "radio");
    assert_eq!(source["sources"]["radio"]["station"], "dr-p3");
}

#[tokio::test]
async fn nothing_is_sent_while_nothing_changes() {
    let dir = run_dir("quiet");
    audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let (_, token) = state.tokens.create("phone", false).unwrap();
    state.sleep.start(30);
    tokio::spawn(watch(state.clone(), Duration::from_millis(100)));
    let mut events = subscribe(net(state), &token).await;
    while events.next(Duration::from_millis(300)).await.is_some() {}
    // The sleep timer ticks, but the client counts it down itself.
    assert_eq!(events.next(Duration::from_millis(1500)).await, None);
}

#[test]
fn countdowns_are_the_same_unless_restarted() {
    let at = |s: u64| json!({"running": true, "minutes": 30, "remaining_s": s});
    let ten = Duration::from_secs(10);
    assert!(same(&at(1800), &at(1790), ten), "ticked as expected");
    assert!(same(&at(1800), &at(1788), ten), "a little late is fine");
    assert!(!same(&at(1790), &at(1800), ten), "restarted");
    assert!(!same(&at(1800), &at(1700), ten), "jumped");
    assert!(!same(&at(1800), &json!({"running": false}), Duration::ZERO));
    let pairing = |open: bool, s: u64| json!({"connected": false, "pairing": {"open": open, "remaining_s": s}});
    assert!(same(
        &pairing(true, 180),
        &pairing(true, 178),
        Duration::from_secs(2)
    ));
    assert!(!same(
        &pairing(true, 180),
        &pairing(false, 0),
        Duration::from_secs(2)
    ));
    assert!(!same(
        &json!({"volume": 60}),
        &json!({"volume": 61}),
        Duration::ZERO
    ));
}
