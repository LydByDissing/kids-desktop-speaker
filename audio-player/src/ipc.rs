//! The Audio Player's IPC server: `status` and `mute` (IPC protocol, DEC-027).

use crate::arbitrator::{Arbitrator, Source};
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn serve(sock_path: &str, arb: Arc<Mutex<Arbitrator>>) -> std::io::Result<()> {
    kds_ipc::serve(sock_path, move |request| {
        let arb = Arc::clone(&arb);
        async move { handle(request, arb).await }
    })
    .await
}

fn source_name(source: &Source) -> &'static str {
    match source {
        Source::Airplay => "airplay",
        Source::Spotify => "spotify",
    }
}

pub async fn handle(request: Value, arb: Arc<Mutex<Arbitrator>>) -> Value {
    match command(&request) {
        Some("status") => {
            let arb = arb.lock().await;
            ok(json!({ "active": arb.active().as_ref().map(source_name) }))
        }
        Some("mute") => {
            let source = match request.get("source").and_then(Value::as_str) {
                Some("airplay") => Source::Airplay,
                Some("spotify") => Source::Spotify,
                Some(other) => return error(format!("unknown source: {other}")),
                None => return error("mute needs source"),
            };
            arb.lock().await.source_stopped(source);
            ok(Value::Null)
        }
        Some(other) => error(format!("unknown command: {other}")),
        None => error("missing cmd"),
    }
}
