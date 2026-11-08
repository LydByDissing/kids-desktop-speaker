//! The Bluetooth Service's IPC server: `status` (IPC protocol, DEC-027).

use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};

pub async fn serve(sock_path: &str, connected: Arc<RwLock<bool>>) -> std::io::Result<()> {
    kds_ipc::serve(sock_path, move |request| {
        let connected = Arc::clone(&connected);
        async move { handle(request, connected) }
    })
    .await
}

pub fn handle(request: Value, connected: Arc<RwLock<bool>>) -> Value {
    match command(&request) {
        Some("status") => match connected.read() {
            Ok(is_connected) => ok(json!({ "connected": *is_connected, "controller": null })),
            Err(_) => error("bluetooth state unavailable"),
        },
        Some(other) => error(format!("unknown command: {other}")),
        None => error("missing cmd"),
    }
}
