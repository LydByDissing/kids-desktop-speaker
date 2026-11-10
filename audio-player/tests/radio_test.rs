//! Radio: the MPD client, the station list and the radio IPC commands, against a
//! fake MPD on a local TCP port (RAD_REQ_001).

use audio_player::announcer::Sources;
use audio_player::arbitrator::Arbitrator;
use audio_player::ipc::{handle_full, Radio, Services};
use audio_player::manager::ManagedSources;
use audio_player::mpd::{quote, Mpd, MpdError, PlayState};
use audio_player::radio::{default_stations, slug, validate_url, StationStore};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// A fake MPD: keeps a play state, records every command, and answers
/// `status`/`currentsong`. `add` of a URL containing "broken" fails like MPD.
struct FakeMpd {
    addr: String,
    log: Arc<StdMutex<Vec<String>>>,
    state: Arc<StdMutex<String>>,
}

async fn fake_mpd() -> FakeMpd {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let log: Arc<StdMutex<Vec<String>>> = Arc::default();
    let state = Arc::new(StdMutex::new("stop".to_string()));
    let (l, st) = (Arc::clone(&log), Arc::clone(&state));
    tokio::spawn(async move {
        loop {
            let (sock, _) = listener.accept().await.unwrap();
            let (l, st) = (Arc::clone(&l), Arc::clone(&st));
            tokio::spawn(async move {
                let (r, mut w) = sock.into_split();
                w.write_all(b"OK MPD 0.23.5\n").await.unwrap();
                let mut lines = BufReader::new(r).lines();
                let mut reply = String::new();
                let mut in_list = false;
                while let Ok(Some(line)) = lines.next_line().await {
                    match line.as_str() {
                        "command_list_begin" => {
                            in_list = true;
                            continue;
                        }
                        "command_list_end" => in_list = false,
                        cmd => {
                            l.lock().unwrap().push(cmd.to_string());
                            let mut s = st.lock().unwrap();
                            match cmd.split(' ').next().unwrap() {
                                "play" => *s = "play".into(),
                                "stop" => *s = "stop".into(),
                                "pause" => {
                                    *s = if cmd.ends_with('1') { "pause" } else { "play" }.into()
                                }
                                "status" => reply.push_str(&format!("volume: 100\nstate: {s}\n")),
                                "currentsong" if *s != "stop" => {
                                    reply.push_str("file: http://x\nTitle: Go' Morgen P3\n")
                                }
                                "add" if cmd.contains("broken") => {
                                    reply = "ACK [50@1] {add} No such directory\n".into();
                                }
                                _ => {}
                            }
                        }
                    }
                    if !in_list {
                        if !reply.starts_with("ACK") {
                            reply.push_str("OK\n");
                        }
                        w.write_all(reply.as_bytes()).await.unwrap();
                        return;
                    }
                }
            });
        }
    });
    FakeMpd { addr, log, state }
}

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-radio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("radio").join("stations.json")
}

fn services(mpd: &FakeMpd, name: &str) -> Services {
    Services {
        announcements: None,
        radio: Some(Arc::new(Radio {
            stations: StationStore::new(temp_path(name)),
            mpd: Mpd::new(&mpd.addr),
            current: StdMutex::new(None),
        })),
    }
}

async fn ipc(services: &Services, request: Value) -> Value {
    handle_full(request, Arc::new(Mutex::new(Arbitrator::new())), services).await
}

#[test]
fn arguments_are_quoted_and_escaped() {
    assert_eq!(quote("http://a/b"), "\"http://a/b\"");
    assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
}

#[tokio::test]
async fn play_url_replaces_the_queue_and_plays() {
    let mpd = fake_mpd().await;
    Mpd::new(&mpd.addr)
        .play_url("http://live-icy.dr.dk/A/A05H.mp3")
        .await
        .unwrap();
    assert_eq!(
        *mpd.log.lock().unwrap(),
        vec!["clear", "add \"http://live-icy.dr.dk/A/A05H.mp3\"", "play"]
    );
    assert_eq!(*mpd.state.lock().unwrap(), "play");
}

#[tokio::test]
async fn status_reports_state_and_stream_title() {
    let mpd = fake_mpd().await;
    let client = Mpd::new(&mpd.addr);
    assert_eq!(client.status().await.unwrap().state, PlayState::Stop);
    client.play_url("http://x").await.unwrap();
    let status = client.status().await.unwrap();
    assert_eq!(status.state, PlayState::Play);
    assert_eq!(status.title.as_deref(), Some("Go' Morgen P3"));
}

#[tokio::test]
async fn mpd_errors_are_reported() {
    let mpd = fake_mpd().await;
    let err = Mpd::new(&mpd.addr)
        .play_url("http://broken")
        .await
        .unwrap_err();
    assert!(
        matches!(&err, MpdError::Ack(m) if m == "No such directory"),
        "{err}"
    );
    let err = Mpd::new("127.0.0.1:1").status().await.unwrap_err();
    assert!(matches!(err, MpdError::Io(_)), "{err}");
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
    let mpd = fake_mpd().await;
    let s = services(&mpd, "ipc-play");
    let r = ipc(&s, json!({"cmd": "radio_play", "id": "dr-p3"})).await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["station"]["name"], "DR P3");
    let status = ipc(&s, json!({"cmd": "status"})).await;
    assert_eq!(
        status["radio"],
        json!({"state": "playing", "station": "dr-p3", "title": "Go' Morgen P3"})
    );
    let r = ipc(&s, json!({"cmd": "radio_stop"})).await;
    assert_eq!(r, json!({"ok": true}));
    let status = ipc(&s, json!({"cmd": "status"})).await;
    assert_eq!(status["radio"]["state"], "stopped");
    assert_eq!(status["radio"]["station"], Value::Null);
}

#[tokio::test]
async fn ipc_manages_the_station_list() {
    let mpd = fake_mpd().await;
    let s = services(&mpd, "ipc-list");
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
async fn status_survives_mpd_being_down() {
    let s = Services {
        announcements: None,
        radio: Some(Arc::new(Radio {
            stations: StationStore::new(temp_path("down")),
            mpd: Mpd::new("127.0.0.1:1"),
            current: StdMutex::new(None),
        })),
    };
    let status = ipc(&s, json!({"cmd": "status"})).await;
    assert_eq!(status["ok"], true);
    assert_eq!(status["radio"]["state"], "unavailable");
}

#[tokio::test]
async fn announcements_pause_and_resume_playing_radio() {
    let mpd = fake_mpd().await;
    let client = Mpd::new(&mpd.addr);
    client.play_url("http://x").await.unwrap();
    let mut sources = ManagedSources {
        managers: vec![],
        mpd: Some(client.clone()),
    };
    let paused = sources.pause_all().await;
    assert_eq!(paused, vec!["mpd".to_string()]);
    assert_eq!(*mpd.state.lock().unwrap(), "pause");
    sources.resume(&paused).await;
    assert_eq!(*mpd.state.lock().unwrap(), "play");
}

#[tokio::test]
async fn announcements_leave_stopped_radio_alone() {
    let mpd = fake_mpd().await;
    let mut sources = ManagedSources {
        managers: vec![],
        mpd: Some(Mpd::new(&mpd.addr)),
    };
    assert!(sources.pause_all().await.is_empty());
    assert!(!mpd
        .log
        .lock()
        .unwrap()
        .iter()
        .any(|c| c.starts_with("pause")));
}
