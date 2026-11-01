use ota_updater::config::OtaConfig;
use ota_updater::{installer, ipc, release, version, SharedStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("ota-updater: starting");

    let cfg = OtaConfig::from_env();
    let status = SharedStatus::new();

    if let Some(v) = version::read_current(&cfg.version_file) {
        status.set_version(v).await;
    }

    let status_ipc = status.clone();
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/ota.sock", status_ipc).await {
            eprintln!("IPC server error: {e}");
        }
    });

    let status_check = status.clone();
    let check_handle = tokio::spawn(async move {
        check_loop(cfg, status_check).await;
    });

    tokio::signal::ctrl_c().await?;
    println!("ota-updater: shutting down");
    ipc_handle.abort();
    check_handle.abort();
    println!("ota-updater: stopped");
    Ok(())
}

async fn check_loop(cfg: OtaConfig, status: SharedStatus) {
    let mut ticker = tokio::time::interval(cfg.check_interval);
    let mut pending = None;
    loop {
        ticker.tick().await;
        check_once(&cfg, &status, &mut pending).await;
    }
}

async fn check_once(cfg: &OtaConfig, status: &SharedStatus, pending: &mut Option<String>) {
    let current = version::read_current(&cfg.version_file).unwrap_or_else(|| "0.0.0".to_string());
    let update = match release::fetch_latest(cfg).await {
        Ok(update) => update,
        Err(e) => {
            eprintln!("ota: release check failed: {e}");
            return;
        }
    };
    let candidate = update.manifest.version;
    if !version::should_install(&candidate, &current, pending.as_deref()) {
        println!("ota: nothing to install (current {current}, latest {candidate})");
        return;
    }
    println!("ota: newer release {candidate} available (current {current})");
    match installer::install_verified(&update.bundle_url, &update.manifest.sha256, status.clone())
        .await
    {
        // The reported version stays the running one until the device reboots.
        Ok(()) => {
            println!("ota: installed {candidate}, active after reboot");
            *pending = Some(candidate);
        }
        Err(e) => eprintln!("ota: install failed: {e}"),
    }
}
