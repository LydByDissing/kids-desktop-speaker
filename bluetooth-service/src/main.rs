use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::task::JoinSet;

use bluetooth_service::agent::{Agent, AGENT_PATH};
use bluetooth_service::bluez;
use bluetooth_service::controller::Controller;
use bluetooth_service::events::AvrcpEvent;
use bluetooth_service::ipc::{self, Context};
use bluetooth_service::pairing::PairingWindow;

/// Serves the pairing agent on the system bus and registers it with BlueZ,
/// retrying until BlueZ is up (BT_REQ_002). Returns the connection, which must
/// stay open for the agent to keep working.
async fn start_agent(window: Arc<Mutex<PairingWindow>>) -> Option<zbus::Connection> {
    let bus = match zbus::Connection::system().await {
        Ok(bus) => bus,
        Err(e) => {
            eprintln!("bluetooth-service: no system bus, pairing disabled: {e}");
            return None;
        }
    };
    let agent = Agent {
        window,
        bus: bus.clone(),
        bluez: "org.bluez".into(),
    };
    if let Err(e) = bus.object_server().at(AGENT_PATH, agent).await {
        eprintln!("bluetooth-service: could not serve the agent: {e}");
        return None;
    }
    let mut delay = Duration::from_secs(1);
    loop {
        match bluez::register_agent(&bus, "org.bluez", AGENT_PATH).await {
            Ok(()) => {
                println!("bluetooth-service: pairing agent registered");
                return Some(bus);
            }
            Err(e) => {
                eprintln!(
                    "bluetooth-service: registering the agent failed ({e}); retrying in {delay:?}"
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(30));
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
    ctx.bus = start_agent(Arc::clone(&window)).await;
    join_set.spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/bt.sock", ctx).await {
            eprintln!("ipc error: {e}");
        }
    });

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
