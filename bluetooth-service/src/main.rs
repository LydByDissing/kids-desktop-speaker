use std::sync::Arc;
use tokio::task::JoinSet;

use bluetooth_service::controller::Controller;
use bluetooth_service::events::AvrcpEvent;
use bluetooth_service::ipc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AvrcpEvent>(32);
    let connected = Arc::new(std::sync::RwLock::new(false));
    let mut join_set: JoinSet<()> = JoinSet::new();

    let ipc_connected = Arc::clone(&connected);
    join_set.spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/bt.sock", ipc_connected).await {
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
