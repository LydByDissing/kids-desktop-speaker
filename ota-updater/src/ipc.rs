//! The OTA Updater's IPC server: `status` and `install` (IPC protocol, DEC-027).

use crate::installer::install_verified;
use crate::state::{OtaState, SharedStatus};
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Notify;

/// Signals from IPC commands to the updater's background loops.
#[derive(Clone, Default)]
pub struct Controls {
    /// Check for an update now instead of waiting for the next check.
    pub check_now: Arc<Notify>,
    /// Reboot now, skipping the maintenance window (OTA_REQ_007).
    pub reboot_now: Arc<Notify>,
}

pub async fn serve(
    sock_path: &str,
    status: SharedStatus,
    controls: Controls,
) -> std::io::Result<()> {
    kds_ipc::serve(sock_path, move |request| {
        let status = status.clone();
        let controls = controls.clone();
        async move { handle(request, status, &controls).await }
    })
    .await
}

/// Answers one request. `install` starts the install in the background and
/// returns at once; progress shows in `status`.
pub async fn handle(request: Value, status: SharedStatus, controls: &Controls) -> Value {
    match command(&request) {
        Some("status") => {
            let current = status.get().await;
            let (state, detail) = match &current.state {
                OtaState::Idle => ("idle", None),
                OtaState::Downloading => ("downloading", None),
                OtaState::Installing => ("installing", None),
                OtaState::Done => ("done", None),
                OtaState::Error(message) => ("error", Some(message.clone())),
            };
            ok(json!({
                "state": state,
                "detail": detail,
                "version": current.version,
                "pending": current.pending,
            }))
        }
        Some("install") => {
            let (Some(url), Some(sha256)) = (
                request.get("url").and_then(Value::as_str),
                request.get("sha256").and_then(Value::as_str),
            ) else {
                return error("install needs url and sha256");
            };
            if matches!(
                status.get().await.state,
                OtaState::Downloading | OtaState::Installing
            ) {
                return error("an install is already in progress");
            }
            let (url, sha256) = (url.to_string(), sha256.to_string());
            tokio::spawn(async move {
                if let Err(e) = install_verified(&url, &sha256, None, status).await {
                    eprintln!("ota: install failed: {e}");
                }
            });
            ok(Value::Null)
        }
        Some("check") => {
            controls.check_now.notify_one();
            ok(Value::Null)
        }
        Some("reboot") => {
            controls.reboot_now.notify_one();
            ok(Value::Null)
        }
        Some(other) => error(format!("unknown command: {other}")),
        None => error("missing cmd"),
    }
}
