use ota_updater::SharedStatus;
use ota_updater::ipc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("ota-updater: starting");

    // Create the shared status
    let status = SharedStatus::new();

    // Spawn IPC server task
    let status_clone = status.clone();
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/ota.sock", status_clone).await {
            eprintln!("IPC server error: {}", e);
        }
    });

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;

    println!("ota-updater: shutting down");

    // Cancel the IPC task
    ipc_handle.abort();

    println!("ota-updater: stopped");

    Ok(())
}
