//! Radio: the station list and the radio IPC commands, against the stream
//! player and a local test server (RAD_REQ_001, DEC-048).

mod common;

use audio_player::announcer::Sources;
use audio_player::arbitrator::Arbitrator;
use audio_player::ipc::{handle_full, Radio, Services};
use audio_player::manager::ManagedSources;
use audio_player::radio::{default_stations, slug, validate_url, Station, StationStore};
use audio_player::stream::sink::PacedSink;
use audio_player::stream::{PlayState, Player};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-radio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("radio").join("stations.json")
}

fn test_player() -> Player {
    Player::new(Arc::new(PacedSink::new(4.0)))
}

/// DR's three stations, but streamed by the local test server.
fn local_stations(server: &common::AudioServer, name: &str) -> StationStore {
    let path = temp_path(name);
    let stations: Vec<Station> = default_stations()
        .into_iter()
        .map(|s| Station {
            url: server.url(&format!(
                "/live/tone.mp3?title={}%20live",
                s.name.replace(' ', "%20")
            )),
            ..s
        })
        .collect();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&stations).unwrap()).unwrap();
    StationStore::new(path)
}

fn services(server: &common::AudioServer, name: &str) -> Services {
    Services {
        radio: Some(Arc::new(Radio {
            stations: local_stations(server, name),
            player: test_player(),
            current: StdMutex::new(None),
        })),
        ..Services::default()
    }
}

async fn ipc(services: &Services, request: Value) -> Value {
    handle_full(request, Arc::new(Mutex::new(Arbitrator::new())), services).await
}

/// Waits until `check` holds for the radio part of `status`.
async fn wait_radio(s: &Services, check: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..200 {
        let status = ipc(s, json!({"cmd": "status"})).await;
        if check(&status["radio"]) {
            return status["radio"].clone();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out: {}", ipc(s, json!({"cmd": "status"})).await);
}

async fn player_state(player: &Player) -> PlayState {
    player.status().await.unwrap().state
}

#[test]
fn only_http_and_https_urls_are_accepted() {
    for ok in [
        "http://live-icy.dr.dk/A/A05H.mp3",
        "https://example.com/stream?x=1",
    ] {
        assert!(validate_url(ok).is_ok(), "{ok}");
    }
    for bad in [
        "file:///etc/passwd",
        "ftp://x/y",
        "http://",
        "http:///path",
        "https://a b",
        "x",
    ] {
        assert!(validate_url(bad).is_err(), "{bad}");
    }
}

#[test]
fn station_ids_come_from_names() {
    assert_eq!(slug("DR P3"), "dr-p3");
    // Non-ASCII letters are dropped from ids; the name keeps them.
    assert_eq!(slug("  Radio Æble! "), "radio-ble");
    assert_eq!(slug("!!!"), "station");
}

#[test]
fn store_starts_with_defaults_then_saves_changes() {
    let store = StationStore::new(temp_path("store"));
    assert_eq!(store.list().unwrap(), default_stations());
    let added = store.add("Radio 4", "http://example.com/r4").unwrap();
    assert_eq!(added.id, "radio-4");
    let again = store.add("Radio 4", "http://example.com/other").unwrap();
    assert_eq!(again.id, "radio-4-2", "ids are unique");
    store.remove("dr-p2").unwrap();
    let ids: Vec<_> = store.list().unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(ids, vec!["dr-p1", "dr-p3", "radio-4", "radio-4-2"]);
    assert!(store.remove("dr-p2").is_err());
    assert!(store.add("", "http://x/y").is_err());
    assert!(store.add("Bad", "file:///etc/passwd").is_err());
}

#[test]
fn a_damaged_station_file_is_reported_not_replaced() {
    let path = temp_path("damaged");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{not json").unwrap();
    let store = StationStore::new(&path);
    assert!(store.list().unwrap_err().contains("damaged"));
    assert!(store.add("X", "http://x/y").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
}

#[tokio::test]
async fn ipc_plays_a_station_and_reports_it() {
    let server = common::start();
    let s = services(&server, "ipc-play");
    let r = ipc(&s, json!({"cmd": "radio_play", "id": "dr-p3"})).await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["station"]["name"], "DR P3");
    let radio = wait_radio(&s, |r| r["title"].is_string()).await;
    assert_eq!(
        radio,
        json!({"state": "playing", "station": "dr-p3", "title": "DR P3 live"})
    );
    let r = ipc(&s, json!({"cmd": "radio_stop"})).await;
    assert_eq!(r, json!({"ok": true}));
    let status = ipc(&s, json!({"cmd": "status"})).await;
    assert_eq!(status["radio"]["state"], "stopped");
    assert_eq!(status["radio"]["station"], Value::Null);
}

#[tokio::test]
async fn ipc_manages_the_station_list() {
    let server = common::start();
    let s = services(&server, "ipc-list");
    let r = ipc(
        &s,
        json!({"cmd": "radio_add", "name": "Radio 4", "url": "http://example.com/r4"}),
    )
    .await;
    assert_eq!(r["station"]["id"], "radio-4");
    let r = ipc(&s, json!({"cmd": "radio_stations"})).await;
    assert_eq!(r["stations"].as_array().unwrap().len(), 4);
    assert_eq!(
        ipc(&s, json!({"cmd": "radio_remove", "id": "radio-4"})).await,
        json!({"ok": true})
    );
    for (request, message) in [
        (
            json!({"cmd": "radio_play", "id": "nope"}),
            "no station nope",
        ),
        (json!({"cmd": "radio_play"}), "missing id"),
        (
            json!({"cmd": "radio_add", "name": "X", "url": "file:///etc/passwd"}),
            "url must start with http:// or https://",
        ),
        (
            json!({"cmd": "radio_add", "name": 5, "url": "http://x/y"}),
            "name must be a string",
        ),
    ] {
        assert_eq!(
            ipc(&s, request.clone()).await,
            json!({"ok": false, "error": message}),
            "{request}"
        );
    }
}

#[tokio::test]
async fn radio_commands_without_radio_are_unavailable() {
    let r = ipc(&Services::default(), json!({"cmd": "radio_stations"})).await;
    assert_eq!(r, json!({"ok": false, "error": "radio unavailable"}));
}

#[tokio::test]
async fn a_station_that_cannot_play_is_reported() {
    let server = common::start();
    let s = services(&server, "broken");
    let r = ipc(
        &s,
        json!({"cmd": "radio_add", "name": "Gone", "url": server.url("/missing.mp3")}),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    let r = ipc(&s, json!({"cmd": "radio_play", "id": "gone"})).await;
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("404"), "{r}");
    let status = ipc(&s, json!({"cmd": "status"})).await;
    assert_eq!(status["radio"]["state"], "stopped");
}

#[tokio::test]
async fn announcements_pause_and_resume_playing_radio() {
    let server = common::start();
    let player = test_player();
    player
        .play_url(&server.url("/live/tone.mp3"))
        .await
        .unwrap();
    let mut sources = ManagedSources {
        managers: vec![],
        player: Some(player.clone()),
        bt: None,
    };
    let paused = sources.pause_all().await;
    assert_eq!(paused, vec!["player".to_string()]);
    assert_eq!(player_state(&player).await, PlayState::Pause);
    sources.resume(&paused).await;
    assert_eq!(player_state(&player).await, PlayState::Play);
    player.stop().await.unwrap();
}

#[tokio::test]
async fn announcements_leave_stopped_radio_alone() {
    let player = test_player();
    let mut sources = ManagedSources {
        managers: vec![],
        player: Some(player.clone()),
        bt: None,
    };
    assert!(sources.pause_all().await.is_empty());
    assert_eq!(player_state(&player).await, PlayState::Stop);
}

#[tokio::test]
async fn playing_the_radio_displaces_the_active_source() {
    use audio_player::arbitrator::Source;
    use audio_player::switcher::{BoxFuture, Stopper};
    #[derive(Default)]
    struct Recorder(StdMutex<Vec<Source>>);
    impl Stopper for Recorder {
        fn stop(&self, source: Source) -> BoxFuture<'_, Result<(), String>> {
            self.0.lock().unwrap().push(source);
            Box::pin(async { Ok(()) })
        }
    }
    let server = common::start();
    let recorder = Arc::new(Recorder::default());
    let mut s = services(&server, "displace");
    s.stopper = Some(Arc::clone(&recorder) as Arc<dyn Stopper>);
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    arb.lock().await.source_started(Source::Airplay);
    let r = handle_full(
        json!({"cmd": "radio_play", "id": "dr-p3"}),
        Arc::clone(&arb),
        &s,
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(*recorder.0.lock().unwrap(), vec![Source::Airplay]);
    assert_eq!(arb.lock().await.active(), Some(Source::Radio));
    handle_full(json!({"cmd": "radio_stop"}), Arc::clone(&arb), &s).await;
    assert_eq!(arb.lock().await.active(), None);
}

#[tokio::test]
async fn a_remote_starts_pauses_and_steps_through_stations() {
    let server = common::start();
    let s = services(&server, "remote");
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let remote = |action: &str| {
        handle_full(
            json!({"cmd": "remote", "action": action}),
            Arc::clone(&arb),
            &s,
        )
    };
    let current = || s.radio.as_ref().unwrap().current.lock().unwrap().clone();

    let r = remote("play_pause").await;
    assert_eq!(
        r["station"]["id"], "dr-p1",
        "nothing playing: the first station {r}"
    );
    assert_eq!(
        remote("play_pause").await,
        json!({"ok": true, "radio": "paused"})
    );
    let player = s.radio.as_ref().unwrap().player.clone();
    assert_eq!(player_state(&player).await, PlayState::Pause);
    assert_eq!(
        remote("play_pause").await,
        json!({"ok": true, "radio": "playing"})
    );
    remote("next").await;
    assert_eq!(current().as_deref(), Some("dr-p2"));
    remote("previous").await;
    remote("previous").await;
    assert_eq!(current().as_deref(), Some("dr-p3"), "wraps around");
    assert_eq!(remote("rewind").await["ok"], false);
}
