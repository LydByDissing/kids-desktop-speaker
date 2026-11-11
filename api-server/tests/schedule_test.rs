//! The sleep timer and alarms (SLP_REQ_001, ALM_REQ_001, DEC-041).

use api_server::local_app as app;
use api_server::schedule::{fire, fires_between, run_timer, Alarm, AlarmInput, AlarmStore};
use api_server::state::AppState;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::NaiveDate;
use http_body_util::BodyExt;
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;

fn at(day: u32, h: u32, m: u32, s: u32) -> chrono::NaiveDateTime {
    // November 2026: the 9th is a Monday.
    NaiveDate::from_ymd_opt(2026, 11, day)
        .unwrap()
        .and_hms_opt(h, m, s)
        .unwrap()
}

fn alarm(time: &str, days: &[u8]) -> Alarm {
    Alarm {
        id: "a".into(),
        name: None,
        time: time.into(),
        days: days.to_vec(),
        station: "dr-p3".into(),
        volume: 60,
        enabled: true,
    }
}

#[test]
fn an_alarm_goes_off_once_when_its_time_passes() {
    let a = alarm("07:00", &[]);
    assert!(fires_between(&a, at(10, 6, 59, 50), at(10, 7, 0, 5)));
    assert!(
        !fires_between(&a, at(10, 7, 0, 5), at(10, 7, 0, 20)),
        "not again"
    );
    assert!(
        !fires_between(&a, at(10, 6, 59, 0), at(10, 6, 59, 59)),
        "not before"
    );
    let mut off = a.clone();
    off.enabled = false;
    assert!(!fires_between(&off, at(10, 6, 59, 50), at(10, 7, 0, 5)));
}

#[test]
fn alarms_repeat_on_their_days() {
    let weekdays = alarm("07:00", &[1, 2, 3, 4, 5]);
    assert!(
        fires_between(&weekdays, at(9, 6, 59, 50), at(9, 7, 0, 5)),
        "Monday"
    );
    assert!(
        !fires_between(&weekdays, at(14, 6, 59, 50), at(14, 7, 0, 5)),
        "not Saturday"
    );
    let midnight = alarm("00:00", &[2]);
    assert!(
        fires_between(&midnight, at(9, 23, 59, 50), at(10, 0, 0, 5)),
        "Tuesday's midnight, crossed on Monday night"
    );
}

#[test]
fn a_clock_jump_does_not_fire_old_alarms() {
    // The clock is set after boot and jumps from 06:00 to 07:01.
    assert!(!fires_between(
        &alarm("06:30", &[]),
        at(10, 6, 0, 0),
        at(10, 7, 1, 0)
    ));
    assert!(
        fires_between(&alarm("07:00", &[]), at(10, 6, 0, 0), at(10, 7, 1, 0)),
        "within the 2-minute catch-up"
    );
}

fn input(time: &str, volume: u8) -> AlarmInput {
    serde_json::from_value(
        json!({"time": time, "days": [5, 1, 1], "station": "dr-p3", "volume": volume}),
    )
    .unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-sched-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn alarms_are_kept_and_checked() {
    let store = AlarmStore::new(temp_dir("store").join("alarms.json"));
    let a = store.add(input("07:00", 60)).unwrap();
    assert_eq!(
        (a.id.as_str(), a.days.clone()),
        ("alarm-1", vec![1, 5]),
        "days sorted, duplicates dropped"
    );
    let b = store.add(input("08:15", 40)).unwrap();
    assert_eq!(b.id, "alarm-2");
    store.update(&a.id, input("06:45", 70)).unwrap();
    assert_eq!(store.list().unwrap()[0].time, "06:45");
    store.remove(&b.id).unwrap();
    assert!(store.remove(&b.id).unwrap_err().starts_with("no alarm"));
    for bad in [
        input("25:00", 50),
        input("7am", 50),
        input("07:00", 0),
        input("07:00", 101),
    ] {
        assert!(store.add(bad).is_err());
    }
    let bad_day: AlarmInput =
        serde_json::from_value(json!({"time": "07:00", "days": [8], "station": "x", "volume": 50}))
            .unwrap();
    assert!(store.add(bad_day).is_err());
}

/// A stand-in Audio Player: records commands; radio_play fails for "offline".
async fn audio_player(dir: &Path) -> Arc<Mutex<Vec<Value>>> {
    let log: Arc<Mutex<Vec<Value>>> = Arc::default();
    let path = dir.join("audio.sock");
    let server_path = path.clone();
    let l = Arc::clone(&log);
    let volume = Arc::new(Mutex::new(60u64));
    tokio::spawn(async move {
        kds_ipc::serve(server_path, move |request: Value| {
            let (l, volume) = (Arc::clone(&l), Arc::clone(&volume));
            async move {
                l.lock().unwrap().push(request.clone());
                match command(&request) {
                    Some("set_volume") => {
                        *volume.lock().unwrap() = request["volume"].as_u64().unwrap();
                        ok(json!({}))
                    }
                    Some("volume") => ok(json!({"volume": *volume.lock().unwrap()})),
                    Some("radio_play") if request["id"] == "offline" => error("mpd unreachable"),
                    Some("radio_play" | "announce" | "stop_all") => ok(json!({})),
                    _ => error("unknown command"),
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
    log
}

fn commands(log: &Arc<Mutex<Vec<Value>>>) -> Vec<String> {
    log.lock()
        .unwrap()
        .iter()
        .map(|r| {
            let cmd = r["cmd"].as_str().unwrap().to_string();
            match cmd.as_str() {
                "set_volume" => format!("volume {}", r["volume"]),
                "radio_play" => format!("radio {}", r["id"].as_str().unwrap()),
                "announce" => format!("say {}", r["text"].as_str().unwrap()),
                _ => cmd,
            }
        })
        .collect()
}

#[tokio::test]
async fn an_alarm_starts_quietly_and_rises() {
    let dir = temp_dir("fire");
    let log = audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    fire(&state, &alarm("07:00", &[]), Duration::from_millis(120)).await;
    let c = commands(&log);
    assert_eq!(
        &c[..2],
        &["volume 30", "radio dr-p3"],
        "half the alarm's volume first"
    );
    assert_eq!(
        c.last().unwrap(),
        "volume 60",
        "rises to the alarm's volume"
    );
    assert_eq!(c.iter().filter(|x| x.starts_with("volume")).count(), 13);
}

#[tokio::test]
async fn without_the_station_the_speaker_says_good_morning() {
    let dir = temp_dir("fallback");
    let log = audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let mut a = alarm("07:00", &[]);
    a.station = "offline".into();
    fire(&state, &a, Duration::from_millis(120)).await;
    assert_eq!(
        commands(&log),
        vec!["volume 30", "radio offline", "volume 60", "say Godmorgen"]
    );
}

#[tokio::test]
async fn the_sleep_timer_fades_stops_and_restores() {
    let dir = temp_dir("timer");
    let log = audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let generation = state.sleep.start(1);
    run_timer(
        state.clone(),
        generation,
        Duration::from_millis(10),
        Duration::from_millis(100),
    )
    .await;
    let c = commands(&log);
    assert_eq!(c[0], "volume");
    assert_eq!(c[1], "volume 54");
    assert!(c.contains(&"volume 0".to_string()));
    assert_eq!(
        &c[c.len() - 2..],
        &["stop_all", "volume 60"],
        "stops, then restores the setting"
    );
    assert!(state.sleep.remaining().is_none(), "finished");
}

#[tokio::test]
async fn a_cancelled_timer_does_nothing() {
    let dir = temp_dir("cancel");
    let log = audio_player(&dir).await;
    let state = AppState::with_run_dir(&dir);
    let generation = state.sleep.start(1);
    assert!(state.sleep.cancel());
    run_timer(
        state.clone(),
        generation,
        Duration::from_millis(10),
        Duration::from_millis(50),
    )
    .await;
    assert!(commands(&log).is_empty());
    let replaced = state.sleep.start(5);
    let newer = state.sleep.start(10);
    assert!(
        !state.sleep.is_current(replaced) && state.sleep.is_current(newer),
        "a new timer replaces the old one"
    );
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
async fn timer_and_alarm_routes() {
    let dir = temp_dir("http");
    let (status, _) = call(&dir, "POST", "/api/timer", Some(json!({"minutes": 0}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(&dir, "POST", "/api/timer", Some(json!({"minutes": 241}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = call(
        &dir,
        "POST",
        "/api/alarms",
        Some(json!({"time": "07:00", "station": "dr-p3", "volume": 50})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["alarm"]["days"], json!([]));
    let (_, body) = call(&dir, "GET", "/api/alarms", None).await;
    assert_eq!(body["alarms"][0]["id"], "alarm-1");
    let (status, _) = call(
        &dir,
        "PUT",
        "/api/alarms/alarm-1",
        Some(json!({"time": "06:30", "station": "dr-p1", "volume": 40, "enabled": false})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &dir,
        "PUT",
        "/api/alarms/nope",
        Some(json!({"time": "06:30", "station": "x", "volume": 40})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &dir,
        "POST",
        "/api/alarms",
        Some(json!({"time": "late", "station": "x", "volume": 40})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        call(&dir, "DELETE", "/api/alarms/alarm-1", None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_running_timer_can_be_read_and_cancelled() {
    let dir = temp_dir("http-timer");
    let state = AppState::with_run_dir(&dir);
    let router = || app(state.clone());
    let send = |method: &'static str, body: Option<Value>| {
        let mut r = Request::builder().method(method).uri("/api/timer");
        if body.is_some() {
            r = r.header("content-type", "application/json");
        }
        r.body(
            body.map(|b| Body::from(b.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap()
    };
    let body_of = |response: axum::response::Response| async move {
        serde_json::from_slice::<Value>(&response.into_body().collect().await.unwrap().to_bytes())
            .unwrap()
    };
    let r = body_of(
        router()
            .oneshot(send("POST", Some(json!({"minutes": 30}))))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        (r["running"].clone(), r["minutes"].clone()),
        (json!(true), json!(30))
    );
    assert!(r["remaining_s"].as_u64().unwrap() > 1790);
    let r = body_of(router().oneshot(send("GET", None)).await.unwrap()).await;
    assert_eq!(r["running"], true);
    let r = body_of(router().oneshot(send("DELETE", None)).await.unwrap()).await;
    assert_eq!(r, json!({"running": false}));
}
