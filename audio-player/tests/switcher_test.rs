//! One source at a time (AUD_REQ_003, DEC-039).

use audio_player::announcer::Sources;
use audio_player::arbitrator::{Arbitrator, Source};
use audio_player::ipc::{handle_full, Services};
use audio_player::manager::{ManagedSources, Manager};
use audio_player::switcher::{event_state, source_active, BoxFuture, BtClient, Stopper};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Default)]
struct FakeStopper {
    stopped: StdMutex<Vec<Source>>,
}

impl Stopper for FakeStopper {
    fn stop(&self, source: Source) -> BoxFuture<'_, Result<(), String>> {
        self.stopped.lock().unwrap().push(source);
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn receiver_events_map_to_playing_or_not() {
    assert_eq!(event_state(Source::Airplay, "active"), Some(true));
    assert_eq!(event_state(Source::Airplay, "inactive"), Some(false));
    assert_eq!(event_state(Source::Spotify, "playing"), Some(true));
    for e in ["paused", "stopped", "session_disconnected"] {
        assert_eq!(event_state(Source::Spotify, e), Some(false), "{e}");
    }
    for e in ["volume_set", "changed", "loading", "preloading"] {
        assert_eq!(event_state(Source::Spotify, e), None, "{e}");
    }
    assert_eq!(event_state(Source::Bluetooth, "idle"), Some(false));
    assert_eq!(event_state(Source::Bluetooth, "pending"), None);
}

#[tokio::test]
async fn the_newest_source_wins_and_the_others_stop() {
    let arb = Mutex::new(Arbitrator::new());
    let stopper = FakeStopper::default();
    assert!(source_active(&arb, &stopper, Source::Airplay)
        .await
        .is_empty());
    assert_eq!(
        source_active(&arb, &stopper, Source::Radio).await,
        vec![Source::Airplay]
    );
    assert_eq!(arb.lock().await.active(), Some(Source::Radio));
    assert_eq!(
        source_active(&arb, &stopper, Source::Bluetooth).await,
        vec![Source::Radio]
    );
    assert_eq!(
        *stopper.stopped.lock().unwrap(),
        vec![Source::Airplay, Source::Radio]
    );
    // The same source again displaces nothing.
    assert!(source_active(&arb, &stopper, Source::Bluetooth)
        .await
        .is_empty());
}

fn services(stopper: &Arc<FakeStopper>) -> Services {
    Services {
        stopper: Some(Arc::clone(stopper) as Arc<dyn Stopper>),
        ..Services::default()
    }
}

#[tokio::test]
async fn source_events_over_ipc() {
    let stopper = Arc::new(FakeStopper::default());
    let s = services(&stopper);
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let event = |source: &str, state: &str| json!({"cmd": "source_event", "source": source, "state": state});

    let r = handle_full(event("airplay", "active"), Arc::clone(&arb), &s).await;
    assert_eq!(r, json!({"ok": true, "displaced": []}));
    let r = handle_full(event("spotify", "playing"), Arc::clone(&arb), &s).await;
    assert_eq!(r, json!({"ok": true, "displaced": ["airplay"]}));
    let status = handle_full(json!({"cmd": "status"}), Arc::clone(&arb), &s).await;
    assert_eq!(status["active"], "spotify");
    let r = handle_full(event("spotify", "paused"), Arc::clone(&arb), &s).await;
    assert_eq!(r, json!({"ok": true}));
    assert_eq!(arb.lock().await.active(), None);
    // Events that change nothing are accepted and ignored.
    let r = handle_full(event("spotify", "volume_set"), Arc::clone(&arb), &s).await;
    assert_eq!(r, json!({"ok": true}));
    for (request, message) in [
        (
            json!({"cmd": "source_event", "source": "cassette", "state": "active"}),
            "unknown source: cassette",
        ),
        (
            json!({"cmd": "source_event", "state": "active"}),
            "source_event needs source",
        ),
        (
            json!({"cmd": "source_event", "source": "radio"}),
            "source_event needs state",
        ),
    ] {
        assert_eq!(
            handle_full(request.clone(), Arc::clone(&arb), &s).await,
            json!({"ok": false, "error": message}),
            "{request}"
        );
    }
}

// ---- AVRCP during announcements ----

/// A stand-in Bluetooth Service: answers avrcp pause/play, or refuses pause
/// when `playing` is false, and records the actions.
async fn bt_service(dir: &Path, playing: bool) -> (PathBuf, Arc<StdMutex<Vec<String>>>) {
    let log: Arc<StdMutex<Vec<String>>> = Arc::default();
    let path = dir.join("bt.sock");
    let server_path = path.clone();
    let l = Arc::clone(&log);
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let l = Arc::clone(&l);
            async move {
                let action = request["action"].as_str().unwrap_or_default().to_string();
                l.lock().unwrap().push(action.clone());
                if action == "pause" && !playing {
                    kds_ipc::error("nothing is playing")
                } else {
                    kds_ipc::ok(Value::Null)
                }
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
    (path, log)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-switch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A stand-in for bluealsa-aplay: a sleeping process under that name.
async fn fake_aplay() -> Manager {
    let mut m = Manager::new("bluealsa-aplay", "/bin/sleep", vec!["30".into()]);
    assert!(m.start_if_available().await);
    m
}

fn proc_state(pid: u32) -> char {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    stat.rsplit(')')
        .next()
        .unwrap()
        .trim_start()
        .chars()
        .next()
        .unwrap()
}

#[tokio::test]
async fn announcements_pause_a_playing_phone_over_avrcp() {
    let (socket, log) = bt_service(&temp_dir("avrcp-play"), true).await;
    let aplay = fake_aplay().await;
    let pid = aplay.pid().unwrap();
    let mut sources = ManagedSources {
        managers: vec![aplay],
        player: None,
        bt: Some(BtClient { socket }),
    };
    let paused = sources.pause_all().await;
    assert_eq!(paused, vec!["avrcp".to_string()]);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        proc_state(pid),
        'S',
        "bluealsa-aplay keeps running; the phone pauses"
    );
    sources.resume(&paused).await;
    assert_eq!(*log.lock().unwrap(), vec!["pause", "play"]);
    sources.stop_all().await.unwrap();
}

#[tokio::test]
async fn without_avrcp_the_player_is_frozen_instead() {
    let (socket, log) = bt_service(&temp_dir("avrcp-refused"), false).await;
    let aplay = fake_aplay().await;
    let pid = aplay.pid().unwrap();
    let mut sources = ManagedSources {
        managers: vec![aplay],
        player: None,
        bt: Some(BtClient { socket }),
    };
    let paused = sources.pause_all().await;
    assert_eq!(paused, vec!["bluealsa-aplay".to_string()]);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(proc_state(pid), 'T');
    sources.resume(&paused).await;
    assert_eq!(
        *log.lock().unwrap(),
        vec!["pause"],
        "no play is sent when nothing was paused over AVRCP"
    );
    sources.stop_all().await.unwrap();
}

#[tokio::test]
async fn restarting_a_receiver_ends_its_session() {
    let mut m = Manager::new("shairport-sync", "/bin/sleep", vec!["30".into()]);
    assert!(m.start_if_available().await);
    let before = m.pid().unwrap();
    let mut sources = ManagedSources {
        managers: vec![m],
        player: None,
        bt: None,
    };
    sources.restart("shairport-sync").await.unwrap();
    let after = sources.managers[0].pid().unwrap();
    assert_ne!(before, after, "a new process");
    assert!(sources
        .restart("nope")
        .await
        .unwrap_err()
        .contains("no source"));
    sources.stop_all().await.unwrap();
}

#[tokio::test]
async fn stop_all_stops_every_playing_source() {
    let stopper = Arc::new(FakeStopper::default());
    let s = services(&stopper);
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    arb.lock().await.source_started(Source::Bluetooth);
    let r = handle_full(json!({"cmd": "stop_all"}), Arc::clone(&arb), &s).await;
    assert_eq!(r, json!({"ok": true, "stopped": ["bluetooth"]}));
    assert_eq!(*stopper.stopped.lock().unwrap(), vec![Source::Bluetooth]);
    assert_eq!(arb.lock().await.active(), None);
    let r = handle_full(json!({"cmd": "stop_all"}), Arc::clone(&arb), &s).await;
    assert_eq!(
        r,
        json!({"ok": true, "stopped": []}),
        "nothing playing: nothing to stop"
    );
}
