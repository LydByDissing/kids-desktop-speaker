//! The pairing agent and pairing window against a fake BlueZ on a private D-Bus
//! (BT_REQ_002). Skipped when dbus-daemon is not installed.

use bluetooth_service::agent::{Agent, AGENT_PATH};
use bluetooth_service::bluez;
use bluetooth_service::ipc::{handle, Context};
use bluetooth_service::pairing::{PairingWindow, MAX_WINDOW};
use serde_json::json;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use zbus::zvariant::OwnedObjectPath;
use zbus::{connection, interface, Connection};

// The tests run on a multi-threaded runtime, as the service does: the agent
// calls back into BlueZ while answering BlueZ, and on a single-threaded test
// runtime that reply is never scheduled.

/// Every D-Bus round trip in these tests must finish within this, so a rare
/// hang fails with the name of the call instead of stalling CI.
const CALL_LIMIT: Duration = Duration::from_secs(10);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(CALL_LIMIT, fut)
        .await
        .unwrap_or_else(|_| panic!("{what} did not finish within {CALL_LIMIT:?}"))
}

const DEVICE: &str = "/org/bluez/hci0/dev_CC_25_EF_BE_9A_85";

struct Bus {
    daemon: Child,
    address: String,
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
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
    Some(Bus {
        daemon,
        address: address.trim().to_string(),
    })
}

#[derive(Default)]
struct FakeAgentManager {
    registered: Vec<(String, String)>,
    default: Option<String>,
}

#[interface(name = "org.bluez.AgentManager1")]
impl FakeAgentManager {
    fn register_agent(&mut self, agent: OwnedObjectPath, capability: String) {
        self.registered.push((agent.to_string(), capability));
    }
    fn request_default_agent(&mut self, agent: OwnedObjectPath) {
        self.default = Some(agent.to_string());
    }
}

#[derive(Default)]
struct FakeAdapter {
    pairable: bool,
    discoverable: bool,
    pairable_timeout: u32,
    discoverable_timeout: u32,
}

#[interface(name = "org.bluez.Adapter1")]
impl FakeAdapter {
    #[zbus(property)]
    fn pairable(&self) -> bool {
        self.pairable
    }
    #[zbus(property)]
    fn set_pairable(&mut self, v: bool) {
        self.pairable = v;
    }
    #[zbus(property)]
    fn discoverable(&self) -> bool {
        self.discoverable
    }
    #[zbus(property)]
    fn set_discoverable(&mut self, v: bool) {
        self.discoverable = v;
    }
    #[zbus(property)]
    fn pairable_timeout(&self) -> u32 {
        self.pairable_timeout
    }
    #[zbus(property)]
    fn set_pairable_timeout(&mut self, v: u32) {
        self.pairable_timeout = v;
    }
    #[zbus(property)]
    fn discoverable_timeout(&self) -> u32 {
        self.discoverable_timeout
    }
    #[zbus(property)]
    fn set_discoverable_timeout(&mut self, v: u32) {
        self.discoverable_timeout = v;
    }
}

#[derive(Default)]
struct FakeDevice {
    trusted: bool,
}

#[interface(name = "org.bluez.Device1")]
impl FakeDevice {
    #[zbus(property)]
    fn trusted(&self) -> bool {
        self.trusted
    }
    #[zbus(property)]
    fn set_trusted(&mut self, v: bool) {
        self.trusted = v;
    }
}

struct Setup {
    _bus: Bus,
    /// The fake BlueZ's connection: it calls the agent like BlueZ would.
    bluez: Connection,
    /// The speaker's connection, serving the agent.
    speaker: Connection,
    window: Arc<Mutex<PairingWindow>>,
}

async fn setup() -> Option<Setup> {
    let Some(bus) = private_bus() else {
        eprintln!("skipping: dbus-daemon not available");
        return None;
    };
    let bluez = connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .serve_at("/org/bluez", FakeAgentManager::default())
        .unwrap()
        .serve_at(bluez::ADAPTER_PATH, FakeAdapter::default())
        .unwrap()
        .serve_at(DEVICE, FakeDevice::default())
        .unwrap()
        .build()
        .await
        .unwrap();
    let window = Arc::new(Mutex::new(PairingWindow::default()));
    let speaker = connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let agent = Agent {
        window: Arc::clone(&window),
        bus: speaker.clone(),
        bluez: "org.bluez".into(),
    };
    speaker.object_server().at(AGENT_PATH, agent).await.unwrap();
    Some(Setup {
        _bus: bus,
        bluez,
        speaker,
        window,
    })
}

impl Setup {
    /// Calls the agent as BlueZ would; Err holds the D-Bus error name.
    async fn call_agent<B>(&self, method: &str, body: &B) -> Result<(), String>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let agent = self.speaker.unique_name().unwrap().to_string();
        let call = self.bluez.call_method(
            Some(agent.as_str()),
            AGENT_PATH,
            Some("org.bluez.Agent1"),
            method,
            body,
        );
        match within(method, call).await {
            Ok(_) => Ok(()),
            Err(zbus::Error::MethodError(name, _, _)) => Err(name.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn trusted(&self) -> bool {
        let iface = self
            .bluez
            .object_server()
            .interface::<_, FakeDevice>(DEVICE)
            .await
            .unwrap();
        let trusted = iface.get().await.trusted;
        trusted
    }

    fn open_window(&self) {
        self.window
            .lock()
            .unwrap()
            .open(Instant::now(), Duration::from_secs(180));
    }

    fn context(&self) -> Context {
        let mut ctx = Context::new(Arc::new(RwLock::new(false)), Arc::clone(&self.window));
        ctx.bus = Some(self.speaker.clone());
        ctx
    }
}

fn device() -> OwnedObjectPath {
    OwnedObjectPath::try_from(DEVICE).unwrap()
}

const AVRCP: &str = "0000110e-0000-1000-8000-00805f9b34fb";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_registers_as_the_default_no_input_agent() {
    let Some(s) = setup().await else { return };
    within(
        "RegisterAgent",
        bluez::register_agent(&s.speaker, "org.bluez", AGENT_PATH),
    )
    .await
    .unwrap();
    let manager = s
        .bluez
        .object_server()
        .interface::<_, FakeAgentManager>("/org/bluez")
        .await
        .unwrap();
    let manager = manager.get().await;
    assert_eq!(
        manager.registered,
        vec![(AGENT_PATH.to_string(), "NoInputNoOutput".to_string())]
    );
    assert_eq!(manager.default.as_deref(), Some(AGENT_PATH));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pairing_is_rejected_while_the_window_is_closed() {
    let Some(s) = setup().await else { return };
    let r = s
        .call_agent("RequestConfirmation", &(device(), 123456u32))
        .await;
    assert_eq!(r, Err("org.bluez.Error.Rejected".to_string()));
    assert!(!s.trusted().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pairing_during_the_window_is_accepted_and_trusted() {
    let Some(s) = setup().await else { return };
    s.open_window();
    assert_eq!(
        s.call_agent("RequestConfirmation", &(device(), 123456u32))
            .await,
        Ok(())
    );
    assert!(s.trusted().await, "a paired device must be trusted at once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trusted_device_is_authorised_after_the_window_closes() {
    let Some(s) = setup().await else { return };
    s.open_window();
    s.call_agent("RequestAuthorization", &(device(),))
        .await
        .unwrap();
    s.window.lock().unwrap().close();
    assert_eq!(
        s.call_agent("AuthorizeService", &(device(), AVRCP)).await,
        Ok(())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_device_is_not_authorised_outside_the_window() {
    let Some(s) = setup().await else { return };
    assert_eq!(
        s.call_agent("AuthorizeService", &(device(), AVRCP)).await,
        Err("org.bluez.Error.Rejected".to_string())
    );
    assert_eq!(
        s.call_agent("RequestPinCode", &(device(),)).await,
        Err("org.bluez.Error.Rejected".to_string())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pair_opens_the_adapter_and_the_window() {
    let Some(s) = setup().await else { return };
    let ctx = s.context();
    let r = within("pair", handle(json!({"cmd": "pair"}), &ctx)).await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["pairing"]["open"], true);
    let adapter = s
        .bluez
        .object_server()
        .interface::<_, FakeAdapter>(bluez::ADAPTER_PATH)
        .await
        .unwrap();
    let a = adapter.get().await;
    assert!(a.pairable && a.discoverable);
    assert_eq!((a.pairable_timeout, a.discoverable_timeout), (180, 180));
    drop(a);

    let status = handle(json!({"cmd": "status"}), &ctx).await;
    assert_eq!(status["pairing"]["open"], true);
    assert!(status["pairing"]["remaining_s"].as_u64().unwrap() > 170);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pair_caps_the_window_and_rejects_bad_durations() {
    let Some(s) = setup().await else { return };
    let ctx = s.context();
    let r = within(
        "pair",
        handle(json!({"cmd": "pair", "seconds": 99999}), &ctx),
    )
    .await;
    assert!(r["pairing"]["remaining_s"].as_u64().unwrap() <= MAX_WINDOW.as_secs());
    for bad in [json!(0), json!(-5), json!("60")] {
        let r = handle(json!({"cmd": "pair", "seconds": bad}), &ctx).await;
        assert_eq!(
            r,
            json!({"ok": false, "error": "seconds must be a positive integer"}),
            "{bad}"
        );
    }
}
