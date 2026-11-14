//! The device volume and the limit (VOL_REQ_001, FEAT_REQ_007, DEC-040).

use audio_player::arbitrator::Arbitrator;
use audio_player::ipc::{handle_full, Services};
use audio_player::volume::{
    analog_gain, from_sender, lineout_step, sender_fraction, to_db, Bass, BassFill, Camilla, Codec,
    Loudness, Settings, Volume, VolumeStore, LINEOUT_FULL,
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
    assert_eq!(to_db(50), Some(-31.5));
    assert_eq!(to_db(75), Some(-15.75));
    assert!((to_db(1).unwrap() + 62.37).abs() < 1e-9);
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

/// The commands without the bass fill patches (DEC-053), which have their own
/// tests.
fn without_patches(log: &[Value]) -> Vec<Value> {
    log.iter()
        .filter(|c| c.get("PatchConfig").is_none())
        .cloned()
        .collect()
}

/// A fake CamillaDSP websocket that records the volume commands and answers
/// Ok. Its config has no loudness filter.
async fn fake_camilla() -> (String, Arc<StdMutex<Vec<Value>>>) {
    fake_camilla_with(json!({"filters": {}})).await
}

/// The same with `config` as the running config (GetConfigJson). Fader
/// commands are recorded too.
async fn fake_camilla_with(config: Value) -> (String, Arc<StdMutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let log: Arc<StdMutex<Vec<Value>>> = Arc::default();
    let l = Arc::clone(&log);
    tokio::spawn(async move {
        loop {
            let (sock, _) = listener.accept().await.unwrap();
            let l = Arc::clone(&l);
            let config = config.clone();
            tokio::spawn(async move {
                let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
                while let Some(Ok(Message::Text(text))) = ws.next().await {
                    let command: Value = serde_json::from_str(text.as_str()).unwrap();
                    let reply = match command.as_str() {
                        Some(name) => {
                            json!({ name: { "result": "Ok", "value": config.to_string() } })
                        }
                        None => {
                            let name = command.as_object().unwrap().keys().next().unwrap().clone();
                            l.lock().unwrap().push(command);
                            json!({ name: { "result": "Ok" } })
                        }
                    };
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
        codec: None,
        loudness: Default::default(),
        codec_muted: Default::default(),
        bass: Default::default(),
    })
}

#[tokio::test]
async fn camilla_receives_the_level_or_a_mute() {
    let (addr, log) = fake_camilla().await;
    let camilla = Camilla { addr };
    camilla.apply(50).await.unwrap();
    camilla.apply(0).await.unwrap();
    assert_eq!(
        without_patches(&log.lock().unwrap()),
        vec![
            json!({"SetVolume": -31.5}),
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
        json!({"ok": true, "volume": 75, "limit": 75, "db": -15.8})
    );
    let r = call(json!({"cmd": "set_limit", "limit": 50})).await;
    assert_eq!(r["volume"], 50);
    assert_eq!(
        log.lock().unwrap().last().unwrap(),
        &json!({"SetMute": false})
    );
    assert!(log.lock().unwrap().contains(&json!({"SetVolume": -31.5})));
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
        json!({"ok": true, "volume": 50, "limit": 50, "db": -31.5})
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
    assert!(log.lock().unwrap().contains(&json!({"SetVolume": -39.1})));
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

#[test]
fn the_volume_maps_onto_the_amplifiers_steps() {
    assert_eq!(lineout_step(0), None, "0 % mutes");
    assert_eq!(
        lineout_step(100),
        Some(LINEOUT_FULL),
        "100 % is full output"
    );
    assert_eq!(lineout_step(50), Some(32));
    assert_eq!(lineout_step(1), Some(1));
    // One dB per step: neighbouring volumes are at most one step apart.
    for p in 1..100u8 {
        let (a, b) = (lineout_step(p).unwrap(), lineout_step(p + 1).unwrap());
        assert!(b >= a && b - a <= 1, "{p}: {a} -> {b}");
    }
}

/// An amixer that records its arguments, or fails.
fn fake_amixer(dir: &str, works: bool) -> (Codec, PathBuf) {
    let dir = temp_dir(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("amixer.log");
    let script = dir.join("amixer");
    let body = if works {
        format!("#!/bin/sh\necho \"$@\" >> {}\n", log.display())
    } else {
        "#!/bin/sh\nexit 1\n".to_string()
    };
    std::fs::write(&script, body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let codec = Codec {
        program: script.display().to_string(),
        ..Codec::iqaudio()
    };
    (codec, log)
}

#[tokio::test]
async fn the_volume_is_set_on_the_amplifier_with_camilla_at_0_db() {
    let (addr, log) = fake_camilla().await;
    let (codec, amixer_log) = fake_amixer("codec", true);
    let store = VolumeStore::new(temp_dir("codec-store").join("volume.json"));
    let v = Volume {
        settings: StdMutex::new(store.load()),
        store,
        camilla: Camilla { addr },
        codec: Some(codec),
        loudness: Default::default(),
        codec_muted: Default::default(),
        bass: Default::default(),
    };
    v.set_volume(50).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(&amixer_log).unwrap(),
        "-q -c IQaudIOCODEC cset name=Lineout Volume 32\n\
         -q -c IQaudIOCODEC cset name=DAC Soft Mute Switch off\n",
        "the first change also makes sure the codec isn't muted"
    );
    assert_eq!(
        without_patches(&log.lock().unwrap()),
        vec![json!({"SetVolume": 0.0}), json!({"SetMute": false})],
        "the DAC gets the whole signal"
    );
    // The next step is one amixer call: the mute state is known.
    v.set_volume(60).await.unwrap();
    let steps = std::fs::read_to_string(&amixer_log).unwrap();
    assert!(
        steps.ends_with("Mute Switch off\n-q -c IQaudIOCODEC cset name=Lineout Volume 38\n"),
        "{steps}"
    );
    // 0 % is the codec's soft mute, not CamillaDSP's.
    v.set_volume(0).await.unwrap();
    let steps = std::fs::read_to_string(&amixer_log).unwrap();
    assert!(
        steps.ends_with("cset name=DAC Soft Mute Switch on\n"),
        "{steps}"
    );
    assert_eq!(
        log.lock().unwrap().last().unwrap(),
        &json!({"SetMute": false})
    );
    // Back up: unmuted again.
    v.set_volume(40).await.unwrap();
    let steps = std::fs::read_to_string(&amixer_log).unwrap();
    assert!(
        steps.ends_with(
            "Lineout Volume 25\n-q -c IQaudIOCODEC cset name=DAC Soft Mute Switch off\n"
        ),
        "{steps}"
    );
}

#[tokio::test]
async fn without_the_codec_camilla_plays_the_volume() {
    let (addr, log) = fake_camilla().await;
    let (codec, _) = fake_amixer("nocodec", false);
    let store = VolumeStore::new(temp_dir("nocodec-store").join("volume.json"));
    let v = Volume {
        settings: StdMutex::new(store.load()),
        store,
        camilla: Camilla { addr },
        codec: Some(codec),
        loudness: Default::default(),
        codec_muted: Default::default(),
        bass: Default::default(),
    };
    v.set_volume(50).await.unwrap();
    assert_eq!(
        without_patches(&log.lock().unwrap()),
        vec![json!({"SetVolume": -31.5}), json!({"SetMute": false})]
    );
}

/// The loudness filter as ci/device/camilladsp.yml has it.
fn config_with_loudness() -> Value {
    json!({"filters": {
        "loudness": {"type": "Loudness", "parameters": {
            "fader": "Aux1", "reference_level": -12.0, "low_boost": 8.0,
            "high_boost": 2.0, "attenuate_mid": true}},
        "subsonic": {"type": "BiquadCombo", "parameters": {}}
    }})
}

#[test]
fn the_mid_cut_follows_camillas_formula() {
    let l = Loudness::from_config(&config_with_loudness()).unwrap();
    assert_eq!(
        l,
        Loudness {
            reference: -12.0,
            max_boost: 8.0
        }
    );
    assert_eq!(l.mid_cut(-6.0), 0.0, "above the reference: nothing");
    assert_eq!(l.mid_cut(-22.0), 4.0, "10 dB below: half");
    assert_eq!(l.mid_cut(-50.0), 8.0, "20 dB below and more: all of it");
    assert_eq!(Loudness::from_config(&json!({"filters": {}})), None);
}

#[tokio::test]
async fn loudness_follows_the_volume_and_the_amplifier_makes_up_the_mids() {
    let (addr, log) = fake_camilla_with(config_with_loudness()).await;
    let (codec, amixer_log) = fake_amixer("loudness", true);
    let store = VolumeStore::new(temp_dir("loudness-store").join("volume.json"));
    let v = Volume {
        settings: StdMutex::new(store.load()),
        store,
        camilla: Camilla { addr },
        codec: Some(codec),
        loudness: Default::default(),
        codec_muted: Default::default(),
        bass: Default::default(),
    };
    // 50 %: -31.5 dB, 19.5 dB below the reference: the mids are cut 7.8 dB,
    // so the amplifier is at 63 - 31.5 + 7.8 = 39.3, step 39 (32 without).
    v.set_volume(50).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(&amixer_log).unwrap(),
        "-q -c IQaudIOCODEC cset name=Lineout Volume 39\n\
         -q -c IQaudIOCODEC cset name=DAC Soft Mute Switch off\n"
    );
    assert_eq!(
        without_patches(&log.lock().unwrap()),
        vec![
            json!({"SetFaderExternalVolume": [1, -31.5]}),
            json!({"SetVolume": 0.0}),
            json!({"SetMute": false})
        ],
        "the loudness fader mirrors the volume; the main volume stays at 0 dB"
    );
    // 90 % (limit raised): -6.3 dB, above the reference, no cut: the plain
    // mapping, 63 - 6.3 = step 57.
    v.set_limit(100).await.unwrap();
    v.set_volume(90).await.unwrap();
    let last = std::fs::read_to_string(&amixer_log).unwrap();
    assert!(last.ends_with("Lineout Volume 57\n"), "{last}");
}

#[test]
fn the_bass_fill_counts_back_from_the_top() {
    let curve = BassFill {
        max_db: 18.0,
        top_db: 6.0,
    };
    assert_eq!(curve.at(0.0), 6.0, "at full gain: what the system can play");
    assert_eq!(curve.at(-6.0), 12.0, "1 dB more for each dB lower");
    assert_eq!(curve.at(-12.0), 18.0, "the box correction at the knee");
    assert_eq!(curve.knee_db(), -12.0);
    assert_eq!(curve.at(-40.0), 18.0, "and no more below it");
    assert_eq!(curve.at(3.0), 6.0, "no gain above full");
    assert_eq!(
        BassFill::default().at(-30.0),
        6.0,
        "by default the static +6 dB"
    );
    // The bass never plays louder than at 100 %, the loudness filter's
    // make-up gain included.
    let loudness = Loudness::from_config(&config_with_loudness());
    assert_eq!(analog_gain(100, loudness), Some(0.0));
    assert_eq!(analog_gain(0, loudness), None);
    for percent in 1..=100 {
        let gain = analog_gain(percent, loudness).unwrap();
        assert!(gain + curve.at(gain) <= 6.0 + 1e-9, "{percent} %");
    }
    // 70 %: -18.9 dB, raised 2.76 dB for the mid cut: -16.14 dB.
    let deep = BassFill {
        max_db: 24.0,
        top_db: 6.0,
    };
    let fill = deep.at(analog_gain(70, loudness).unwrap());
    assert!((fill - 22.14).abs() < 1e-9, "{fill}");

    assert!(curve.validate().is_ok());
    for bad in [
        BassFill {
            max_db: 30.0,
            ..curve
        },
        BassFill {
            top_db: -1.0,
            ..curve
        },
        BassFill {
            max_db: 6.0,
            top_db: 12.0,
        },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
}

fn patches(log: &[Value]) -> Vec<f64> {
    log.iter()
        .filter_map(|c| c["PatchConfig"]["filters"]["bass_boost"]["parameters"]["gain"].as_f64())
        .collect()
}

#[tokio::test]
async fn the_bass_fill_follows_the_volume() {
    let (addr, log) = fake_camilla_with(config_with_loudness()).await;
    let dir = temp_dir("bass");
    let store = VolumeStore::new(dir.join("volume.json"));
    let v = Volume {
        settings: StdMutex::new(store.load()),
        store,
        camilla: Camilla { addr },
        codec: None,
        loudness: Default::default(),
        codec_muted: Default::default(),
        bass: Bass::load(dir.join("bass.json")),
    };
    v.set_limit(100).await.unwrap();
    let curve = BassFill {
        max_db: 18.0,
        top_db: 6.0,
    };
    v.set_bass_fill(curve).await.unwrap();
    v.set_volume(40).await.unwrap(); // the same +18 dB: nothing sent
    v.set_volume(80).await.unwrap(); // -12.6 dB (-12.36 with the make-up): still +18
    v.set_volume(90).await.unwrap(); // -6.3 dB: 6 + 6.3
    v.set_volume(100).await.unwrap();
    // The default curve's +6 dB when the limit was raised, then +18 at 75 %
    // when the curve was set, then 90 and 100 %.
    assert_eq!(patches(&log.lock().unwrap()), vec![6.0, 18.0, 12.3, 6.0]);
    // Both the fill and the bass compressor's view of it are patched.
    let last = log
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|c| c.get("PatchConfig").is_some())
        .cloned()
        .unwrap();
    assert_eq!(
        last["PatchConfig"]["filters"]["boosted_view"]["parameters"]["gain"],
        6.0
    );
    // The curve is kept for the next start.
    assert_eq!(Bass::load(dir.join("bass.json")).curve(), curve);
    assert!(v
        .set_bass_fill(BassFill {
            top_db: 20.0,
            ..curve
        })
        .await
        .is_err());
    assert_eq!(v.bass_fill(), curve, "a bad curve changes nothing");
}

#[tokio::test]
async fn the_bass_fill_over_ipc() {
    let (addr, log) = fake_camilla().await;
    let v = volume(&addr, "bass-ipc");
    let services = Services {
        volume: Some(Arc::clone(&v)),
        ..Services::default()
    };
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let call = |request: Value| handle_full(request, Arc::clone(&arb), &services);

    let r = call(json!({"cmd": "bass_fill"})).await;
    assert_eq!(r, json!({"ok": true, "max_db": 6.0, "top_db": 6.0}));
    let r = call(json!({"cmd": "set_bass_fill", "max_db": 18.0, "top_db": 6.0})).await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["max_db"], 18.0);
    assert!(
        !patches(&log.lock().unwrap()).is_empty(),
        "applied at the current volume"
    );
    for bad in [
        json!({"cmd": "set_bass_fill", "max_db": 40.0, "top_db": 6.0}),
        json!({"cmd": "set_bass_fill", "max_db": 18.0}),
    ] {
        assert_eq!(call(bad.clone()).await["ok"], false, "{bad}");
    }
}
