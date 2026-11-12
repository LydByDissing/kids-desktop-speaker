use ota_updater::config::{failures_after, next_check_delay, OtaConfig};
use ota_updater::ipc::Controls;
use ota_updater::{installer, ipc, reboot, release, version, SharedStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("ota-updater: starting");

    let cfg = OtaConfig::from_env();
    let status = SharedStatus::new();

    if let Some(v) = version::read_current(&cfg.version_file) {
        status.set_version(v).await;
    }

    let controls = Controls::default();
    let status_ipc = status.clone();
    let controls_ipc = controls.clone();
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc::serve("/run/kds/ota.sock", status_ipc, controls_ipc).await {
            eprintln!("IPC server error: {e}");
        }
    });

    let reboot_handle = tokio::spawn(reboot::reboot_loop(
        cfg.clone(),
        status.clone(),
        controls.reboot_now.clone(),
    ));

    let status_check = status.clone();
    let check_now = controls.check_now.clone();
    let check_handle = tokio::spawn(async move {
        check_loop(cfg, status_check, check_now).await;
    });

    tokio::signal::ctrl_c().await?;
    println!("ota-updater: shutting down");
    ipc_handle.abort();
    reboot_handle.abort();
    check_handle.abort();
    println!("ota-updater: stopped");
    Ok(())
}

/// What the check loop remembers between checks.
#[derive(Default)]
struct CheckState {
    /// A version already installed and waiting for a reboot.
    pending: Option<String>,
    /// The version and checksum of a release that failed verification.
    failed: Option<(String, String)>,
}

async fn check_loop(
    cfg: OtaConfig,
    status: SharedStatus,
    check_now: std::sync::Arc<tokio::sync::Notify>,
) {
    let mut state = CheckState::default();
    let mut failures = 0u32;
    loop {
        // At boot, then weekly; a failed check is retried 3 times before the
        // next scheduled one (OTA_REQ_011).
        failures = failures_after(check_once(&cfg, &status, &mut state).await, failures);
        // A `check` command cuts the wait short.
        tokio::select! {
            _ = tokio::time::sleep(next_check_delay(failures, cfg.check_interval)) => {}
            _ = check_now.notified() => println!("ota: check requested"),
        }
    }
}

/// Returns whether the check completed, so a failure can be retried soon.
async fn check_once(cfg: &OtaConfig, status: &SharedStatus, state: &mut CheckState) -> bool {
    let current = version::read_current(&cfg.version_file).unwrap_or_else(|| "0.0.0".to_string());
    let update = match release::fetch_latest(cfg).await {
        Ok(Some(update)) => update,
        Ok(None) => {
            println!("ota: no release published yet (current {current})");
            return true;
        }
        Err(e) => {
            eprintln!("ota: release check failed: {:#}", anyhow::Error::new(e));
            return false;
        }
    };
    let candidate = update.manifest.version;
    let sha256 = update.manifest.sha256;
    if !version::should_install(&candidate, &current, state.pending.as_deref()) {
        println!("ota: nothing to install (current {current}, latest {candidate})");
        return true;
    }
    let failed = state.failed.as_ref().map(|(v, s)| (v.as_str(), s.as_str()));
    if version::is_known_bad(&candidate, &sha256, failed) {
        println!("ota: skipping {candidate}, which failed verification earlier");
        return true;
    }
    println!("ota: newer release {candidate} available (current {current})");
    let size = Some(update.manifest.size);
    match installer::install_verified(&update.bundle_url, &sha256, size, status.clone()).await {
        // The reported version stays the running one until the device reboots.
        Ok(()) => {
            println!("ota: installed {candidate}, active after reboot");
            status.set_pending(candidate.clone()).await;
            state.pending = Some(candidate);
            true
        }
        Err(e) => {
            if e.is_bad_release() {
                state.failed = Some((candidate, sha256));
            }
            eprintln!("ota: install failed: {:#}", anyhow::Error::new(e));
            false
        }
    }
}
