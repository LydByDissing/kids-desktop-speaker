//! The device volume and the limit (VOL_REQ_001, FEAT_REQ_007, DEC-040).

use audio_player::arbitrator::Arbitrator;
use audio_player::ipc::{handle_full, Services};
use audio_player::volume::{
    from_sender, sender_fraction, to_db, Camilla, Settings, Volume, VolumeStore,
};
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
        (75, 75),
        "the volume starts at the limit (FEAT_REQ_007: 75 %, DEC-040)"
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

#[test]
fn each_sender_reports_its_volume_its_own_way() {
    let close = |a: Option<f64>, b: f64| (a.unwrap() - b).abs() < 1e-9;
    // AirPlay: dB from -30 to 0, -144 for mute.
    assert!(close(sender_fraction("airplay", "0.00"), 1.0));
    assert!(close(sender_fraction("airplay", "-15.00"), 0.5));
    assert!(close(sender_fraction("airplay", "-144.00"), 0.0));
    // Spotify: 0-65535; Bluetooth AVRCP: 0-127.
    assert!(close(sender_fraction("spotify", "65535"), 1.0));
    assert!(close(
        sender_fraction("spotify", "32768"),
        32768.0 / 65535.0
    ));
    assert!(close(sender_fraction("bluetooth", "127"), 1.0));
    assert!(close(sender_fraction("bluetooth", "0"), 0.0));
    assert!(close(sender_fraction("bluetooth", "300"), 1.0), "clamped");
    assert_eq!(sender_fraction("radio", "10"), None);
    assert_eq!(sender_fraction("airplay", "loud"), None);
    assert_eq!(sender_fraction("spotify", "NaN"), None);
}

#[test]
fn a_senders_whole_range_maps_onto_zero_to_the_limit() {
    assert_eq!(
        from_sender(1.0, 75),
        75,
        "all the way up plays at the limit"
    );
    assert_eq!(from_sender(0.5, 75), 38);
    assert_eq!(from_sender(0.0, 75), 0);
    // Many increments: a Bluetooth phone's 128 steps give distinct volumes.
    let distinct: std::collections::BTreeSet<u8> = (0..=127)
        .map(|v| from_sender(f64::from(v) / 127.0, 100))
        .collect();
    assert!(distinct.len() > 95, "{}", distinct.len());
}

#[tokio::test]
async fn a_phone_changing_its_volume_sets_the_speaker_volume() {
    let (addr, log) = fake_camilla().await;
    let v = volume(&addr, "sender");
    let services = Services {
        volume: Some(Arc::clone(&v)),
        ..Services::default()
    };
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let call = |request: Value| handle_full(request, Arc::clone(&arb), &services);

    // Bluetooth at 64 of 127, limit 75: about half of 75.
    let r = call(json!({"cmd": "source_volume", "source": "bluetooth", "value": 64})).await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["volume"], 38);
    assert_eq!(v.store.load().volume, 38, "kept for the next source");
    assert!(log
        .lock()
        .unwrap()
        .contains(&json!({"SetVolume": -60.0 * (1.0 - 0.38)})));
    // AirPlay all the way up: the limit.
    let r = call(json!({"cmd": "source_volume", "source": "airplay", "value": "0.00"})).await;
    assert_eq!(r["volume"], 75);
    for bad in [
        json!({"cmd": "source_volume", "source": "airplay"}),
        json!({"cmd": "source_volume", "source": "radio", "value": "5"}),
        json!({"cmd": "source_volume", "source": "spotify", "value": "loud"}),
    ] {
        assert_eq!(call(bad.clone()).await["ok"], false, "{bad}");
    }
}
