//! AVRCP control and the transport watcher against a fake BlueZ on a private
//! D-Bus (DEC-039). Skipped when dbus-daemon is not installed. Multi-threaded,
//! as the service runs.

use bluetooth_service::bluez::{avrcp, players, watch_transports, TransportEvent};
use bluetooth_service::ipc::{handle, Context};
use bluetooth_service::pairing::PairingWindow;
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use zbus::zvariant::Value;
use zbus::{connection, interface, Connection};

const PLAYER: &str = "/org/bluez/hci0/dev_CC_25_EF_BE_9A_85/player0";
const TRANSPORT: &str = "/org/bluez/hci0/dev_CC_25_EF_BE_9A_85/fd0";

struct Bus(Child, String);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn private_bus() -> Option<Bus> {
    let mut daemon = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut address = String::new();
    BufReader::new(daemon.stdout.take()?)
        .read_line(&mut address)
        .ok()?;
    Some(Bus(daemon, address.trim().to_string()))
}

struct FakePlayer {
    status: String,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

#[interface(name = "org.bluez.MediaPlayer1")]
impl FakePlayer {
    fn pause(&mut self) {
        self.calls.lock().unwrap().push("Pause");
        self.status = "paused".into();
    }
    fn play(&mut self) {
        self.calls.lock().unwrap().push("Play");
        self.status = "playing".into();
    }
    #[zbus(property)]
    fn status(&self) -> String {
        self.status.clone()
    }
}

struct Setup {
    _bus: Bus,
    bluez: Connection,
    speaker: Connection,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

async fn setup(status: Option<&str>) -> Option<Setup> {
    let Some(bus) = private_bus() else {
        eprintln!("skipping: dbus-daemon not available");
        return None;
    };
    let calls: Arc<Mutex<Vec<&'static str>>> = Arc::default();
    let mut builder = connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .serve_at("/", zbus::fdo::ObjectManager)
        .unwrap();
    if let Some(status) = status {
        builder = builder
            .serve_at(
                PLAYER,
                FakePlayer {
                    status: status.into(),
                    calls: Arc::clone(&calls),
                },
            )
            .unwrap();
    }
    let bluez = builder.build().await.unwrap();
    let speaker = connection::Builder::address(bus.1.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    Some(Setup {
        _bus: bus,
        bluez,
        speaker,
        calls,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn players_are_found_with_their_status() {
    let Some(s) = setup(Some("playing")).await else {
        return;
    };
    let found = players(&s.speaker, "org.bluez").await.unwrap();
    assert_eq!(found, vec![(PLAYER.to_string(), "playing".to_string())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_playing_phone_is_paused_and_resumed() {
    let Some(s) = setup(Some("playing")).await else {
        return;
    };
    avrcp(&s.speaker, "org.bluez", "pause").await.unwrap();
    avrcp(&s.speaker, "org.bluez", "play").await.unwrap();
    assert_eq!(*s.calls.lock().unwrap(), vec!["Pause", "Play"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_that_is_not_playing_is_not_paused() {
    let Some(s) = setup(Some("paused")).await else {
        return;
    };
    let err = avrcp(&s.speaker, "org.bluez", "pause").await.unwrap_err();
    assert_eq!(err, "nothing is playing");
    assert!(s.calls.lock().unwrap().is_empty());
    let Some(none) = setup(None).await else {
        return;
    };
    assert_eq!(
        avrcp(&none.speaker, "org.bluez", "play").await.unwrap_err(),
        "no phone connected"
    );
    assert!(avrcp(&none.speaker, "org.bluez", "skip")
        .await
        .unwrap_err()
        .contains("unknown action"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn avrcp_over_ipc() {
    let Some(s) = setup(Some("playing")).await else {
        return;
    };
    let mut ctx = Context::new(
        Arc::new(RwLock::new(false)),
        Arc::new(Mutex::new(PairingWindow::default())),
    );
    ctx.bus = Some(s.speaker.clone());
    assert_eq!(
        handle(json!({"cmd": "avrcp", "action": "pause"}), &ctx).await,
        json!({"ok": true})
    );
    assert_eq!(
        handle(json!({"cmd": "avrcp", "action": "pause"}), &ctx).await,
        json!({"ok": false, "error": "nothing is playing"})
    );
    assert_eq!(
        handle(json!({"cmd": "avrcp"}), &ctx).await,
        json!({"ok": false, "error": "avrcp needs action"})
    );
}

async fn emit_state(bluez: &Connection, state: &str) {
    emit_change(bluez, "State", Value::from(state)).await;
}

async fn emit_change(bluez: &Connection, name: &str, value: Value<'_>) {
    let mut changed: HashMap<&str, Value> = HashMap::new();
    changed.insert(name, value);
    bluez
        .emit_signal(
            None::<&str>,
            TRANSPORT,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &("org.bluez.MediaTransport1", changed, Vec::<String>::new()),
        )
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streaming_and_the_phones_volume_are_reported() {
    let Some(s) = setup(None).await else { return };
    let seen: Arc<Mutex<Vec<TransportEvent>>> = Arc::default();
    let log = Arc::clone(&seen);
    let speaker = s.speaker.clone();
    tokio::spawn(async move {
        let _ = watch_transports(&speaker, "org.bluez", move |active| {
            let log = Arc::clone(&log);
            async move { log.lock().unwrap().push(active) }
        })
        .await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    for state in ["pending", "active"] {
        emit_state(&s.bluez, state).await;
    }
    // The phone's volume buttons (AVRCP absolute volume, DEC-049).
    emit_change(&s.bluez, "Volume", Value::from(90u16)).await;
    emit_state(&s.bluez, "idle").await;
    for _ in 0..50 {
        if seen.lock().unwrap().len() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            TransportEvent::Streaming(true),
            TransportEvent::Volume(90),
            TransportEvent::Streaming(false)
        ],
        "pending is not reported"
    );
}
