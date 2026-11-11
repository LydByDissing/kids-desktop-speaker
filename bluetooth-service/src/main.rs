use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::task::JoinSet;

use bluetooth_service::agent::{Agent, AGENT_PATH};
use bluetooth_service::bluez;
use bluetooth_service::controller::Controller;
use bluetooth_service::events::AvrcpEvent;
use bluetooth_service::ipc::{self, Context};
use bluetooth_service::pairing::PairingWindow;

/// Connects to the system bus with the pairing agent already served, so BlueZ
/// can use it as soon as it is registered (BT_REQ_002). Returns None without a
/// system bus; pairing is then unavailable but the rest of the service runs.
async fn connect_agent(window: Arc<Mutex<PairingWindow>>) -> Option<zbus::Connection> {
    let agent = Agent {
        window,
        bluez: "org.bluez".into(),
    };
    // Served from the builder, so the object server is running before BlueZ
    // can send the agent anything.
    let bus = match zbus::connection::Builder::system().and_then(|b| b.serve_at(AGENT_PATH, agent))
    {
        Ok(builder) => builder.build().await,
        Err(e) => Err(e),
    };
    match bus {
        Ok(bus) => Some(bus),
        Err(e) => {
            eprintln!("bluetooth-service: no system bus, pairing disabled: {e}");
            None
        }
    }
}

/// Registers the agent with BlueZ, retrying until BlueZ is up. Runs in the
/// background: the IPC server must answer even when BlueZ never comes up.
async fn register_until_done(bus: zbus::Connection) {
    let mut delay = Duration::from_secs(1);
    loop {
        match bluez::register_agent(&bus, "org.bluez", AGENT_PATH).await {
            Ok(()) => {
                println!("bluetooth-service: pairing agent registered");
                return;
            }
            Err(e) => {
                eprintln!(
                    "bluetooth-service: registering the agent failed ({e}); retrying in {delay:?}"
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(60));
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AvrcpEvent>(32);
    let connected = Arc::new(RwLock::new(false));
    let window = Arc::new(Mutex::new(PairingWindow::default()));
    let mut join_set: JoinSet<()> = JoinSet::new();

    let mut ctx = Context::new(Arc::clone(&connected), Arc::clone(&window));
    ctx.bus = connect_agent(Arc::clone(&window)).await;
    if let Some(bus) = ctx.bus.clone() {
        join_set.spawn(register_until_done(bus.clone()));
        // Tell the Audio Player when a phone starts or stops streaming, so only
        // one source plays at a time (DEC-039).
        join_set.spawn(async move {
            let report = |active: bool| async move {
                let state = if active { "active" } else { "inactive" };
                let request = serde_json::json!({"cmd": "source_event", "source": "bluetooth", "state": state});
                if let Err(e) = kds_ipc::request("/run/kds/audio.sock", &request).await {
                    eprintln!("bluetooth-service: could not tell the Audio Player: {e}");
                }
            };
            if let Err(e) = bluez::watch_transports(&bus, "org.bluez", report).await {
                eprintln!("bluetooth-service: watching audio transports failed: {e}");
            }
        });
    }
    join_set.spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/bt.sock", ctx).await {
            eprintln!("ipc error: {e}");
        }
    });

    // A Bluetooth remote's buttons (RMT_REQ_001, DEC-042).
    join_set.spawn(bluetooth_service::input::watch_remotes(
        "/sys/class/input".into(),
        "/dev/input".into(),
        Duration::from_secs(5),
        |action| async move {
            let request = serde_json::json!({"cmd": "remote", "action": action.name()});
            if let Err(e) = kds_ipc::request("/run/kds/audio.sock", &request).await {
                eprintln!(
                    "bluetooth-service: could not pass on {}: {e}",
                    action.name()
                );
            }
        },
    ));

    join_set.spawn(async move {
        while let Some(_event) = rx.recv().await {
            let _ = connected.write().map(|mut c| *c = true);
        }
    });

    join_set.spawn(async move {
        let mut controller = Controller::new();
        if let Err(e) = controller.run(tx).await {
            eprintln!("controller error: {e}");
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = async { while join_set.join_next().await.is_some() {} } => {}
    }
    Ok(())
}
