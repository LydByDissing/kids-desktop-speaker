//! The device volume and the limit (VOL_REQ_001, FEAT_REQ_007, DEC-040).

use audio_player::arbitrator::Arbitrator;
use audio_player::ipc::{handle_full, Services};
use audio_player::volume::{to_db, Camilla, Settings, Volume, VolumeStore};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-vol-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_curve_is_even_in_db() {
    assert_eq!(to_db(0), None, "0 % mutes");
    assert_eq!(to_db(100), Some(0.0));
    assert_eq!(to_db(50), Some(-30.0));
    assert_eq!(to_db(75), Some(-15.0));
    assert!((to_db(1).unwrap() + 59.4).abs() < 1e-9);
    assert_eq!(to_db(200), Some(0.0), "above 100 counts as 100");
}

#[test]
fn the_limit_caps_the_volume() {
    let mut s = Settings::default();
    assert_eq!(
        (s.volume, s.limit),
        (40, 75),
        "defaults (FEAT_REQ_007: 75 %)"
    );
    assert_eq!(
        s.set_volume(90),
        75,
        "asking for more than the limit gives the limit"
    );
    s.set_limit(50);
    assert_eq!(s.volume, 50, "lowering the limit lowers a higher volume");
    s.set_limit(80);
    assert_eq!(s.volume, 50, "raising the limit leaves the volume");
}

#[test]
fn settings_are_kept_on_disk() {
    let dir = temp_dir("store");
    let store = VolumeStore::new(dir.join("audio/volume.json"));
    assert_eq!(store.load(), Settings::default(), "nothing saved yet");
    store
        .save(&Settings {
            volume: 30,
            limit: 60,
        })
        .unwrap();
    assert_eq!(
        store.load(),
        Settings {
            volume: 30,
            limit: 60
        }
    );
    std::fs::write(
        dir.join("audio/volume.json"),
        br#"{"volume": 90, "limit": 60}"#,
    )
    .unwrap();
    assert_eq!(store.load().volume, 60, "a file above the limit is capped");
    std::fs::write(dir.join("audio/volume.json"), b"{damaged").unwrap();
    assert_eq!(store.load(), Settings::default());
}

/// A fake CamillaDSP websocket that records commands and answers Ok.
async fn fake_camilla() -> (String, Arc<StdMutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let log: Arc<StdMutex<Vec<Value>>> = Arc::default();
    let l = Arc::clone(&log);
    tokio::spawn(async move {
        loop {
            let (sock, _) = listener.accept().await.unwrap();
            let l = Arc::clone(&l);
            tokio::spawn(async move {
                let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
                while let Some(Ok(Message::Text(text))) = ws.next().await {
                    let command: Value = serde_json::from_str(text.as_str()).unwrap();
                    let name = command.as_object().unwrap().keys().next().unwrap().clone();
                    l.lock().unwrap().push(command);
                    let reply = json!({ name: { "result": "Ok" } });
                    ws.send(Message::Text(reply.to_string().into()))
                        .await
                        .unwrap();
                }
            });
        }
    });
    (addr, log)
}

fn volume(addr: &str, dir: &str) -> Arc<Volume> {
    let store = VolumeStore::new(temp_dir(dir).join("volume.json"));
    Arc::new(Volume {
        settings: StdMutex::new(store.load()),
        store,
        camilla: Camilla { addr: addr.into() },
    })
}

#[tokio::test]
async fn camilla_receives_the_level_or_a_mute() {
    let (addr, log) = fake_camilla().await;
    let camilla = Camilla { addr };
    camilla.apply(50).await.unwrap();
    camilla.apply(0).await.unwrap();
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            json!({"SetVolume": -30.0}),
            json!({"SetMute": false}),
            json!({"SetMute": true})
        ]
    );
}

#[tokio::test]
async fn volume_over_ipc_is_capped_saved_and_applied() {
    let (addr, log) = fake_camilla().await;
    let v = volume(&addr, "ipc");
    let services = Services {
        volume: Some(Arc::clone(&v)),
        ..Services::default()
    };
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let call = |request: Value| handle_full(request, Arc::clone(&arb), &services);

    let r = call(json!({"cmd": "set_volume", "volume": 90})).await;
    assert_eq!(
        r,
        json!({"ok": true, "volume": 75, "limit": 75, "db": -15.0})
    );
    let r = call(json!({"cmd": "set_limit", "limit": 50})).await;
    assert_eq!(r["volume"], 50);
    assert_eq!(
        log.lock().unwrap().last().unwrap(),
        &json!({"SetMute": false})
    );
    assert!(log.lock().unwrap().contains(&json!({"SetVolume": -30.0})));
    assert_eq!(
        v.store.load(),
        Settings {
            volume: 50,
            limit: 50
        },
        "saved"
    );
    let r = call(json!({"cmd": "volume"})).await;
    assert_eq!(
        r,
        json!({"ok": true, "volume": 50, "limit": 50, "db": -30.0})
    );
    for bad in [
        json!({"cmd": "set_volume", "volume": 150}),
        json!({"cmd": "set_limit"}),
        json!({"cmd": "set_volume", "volume": "x"}),
    ] {
        assert_eq!(call(bad.clone()).await["ok"], false, "{bad}");
    }
}

#[tokio::test]
async fn without_camilla_the_setting_is_kept_with_a_warning() {
    let v = volume("127.0.0.1:1", "down");
    let services = Services {
        volume: Some(Arc::clone(&v)),
        ..Services::default()
    };
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let r = handle_full(json!({"cmd": "set_volume", "volume": 30}), arb, &services).await;
    assert_eq!(r["ok"], true);
    assert_eq!(r["volume"], 30);
    assert!(r["warning"].as_str().unwrap().contains("CamillaDSP"));
    assert_eq!(v.store.load().volume, 30);
}

#[tokio::test]
async fn remote_volume_buttons_step_by_five_within_the_limit() {
    let (addr, _) = fake_camilla().await;
    let v = volume(&addr, "remote");
    v.set_volume(65).await.unwrap();
    let services = Services {
        volume: Some(Arc::clone(&v)),
        ..Services::default()
    };
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let press = |a: &str| {
        handle_full(
            json!({"cmd": "remote", "action": a}),
            Arc::clone(&arb),
            &services,
        )
    };
    assert_eq!(press("volume_up").await["volume"], 70);
    assert_eq!(press("volume_up").await["volume"], 75);
    assert_eq!(press("volume_up").await["volume"], 75, "not past the limit");
    assert_eq!(press("volume_down").await["volume"], 70);
}
