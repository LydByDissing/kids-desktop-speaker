//! The OTA Updater's IPC server: `status` and `install` (IPC protocol, DEC-027).

use crate::installer::install_verified;
use crate::state::{OtaState, SharedStatus};
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};

pub async fn serve(sock_path: &str, status: SharedStatus) -> std::io::Result<()> {
    kds_ipc::serve(sock_path, move |request| {
        let status = status.clone();
        async move { handle(request, status).await }
    })
    .await
}

/// Answers one request. `install` starts the install in the background and
/// returns at once; progress shows in `status`.
pub async fn handle(request: Value, status: SharedStatus) -> Value {
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
            ok(json!({ "state": state, "detail": detail, "version": current.version }))
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
        Some(other) => error(format!("unknown command: {other}")),
        None => error("missing cmd"),
    }
}
