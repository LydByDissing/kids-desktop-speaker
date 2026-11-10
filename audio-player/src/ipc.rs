//! The Audio Player's IPC server: `status`, `mute` and `announce` (IPC protocol,
//! DEC-027, DEC-033).

use crate::announcer::{AnnounceQueue, Announcement};
use crate::arbitrator::{Arbitrator, Source};
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn serve(
    sock_path: &str,
    arb: Arc<Mutex<Arbitrator>>,
    announcements: AnnounceQueue,
) -> std::io::Result<()> {
    kds_ipc::serve(sock_path, move |request| {
        let arb = Arc::clone(&arb);
        let announcements = announcements.clone();
        async move { handle_with(request, arb, Some(&announcements)).await }
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
    handle_with(request, arb, None).await
}

pub async fn handle_with(
    request: Value,
    arb: Arc<Mutex<Arbitrator>>,
    announcements: Option<&AnnounceQueue>,
) -> Value {
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
        Some("announce") => {
            let Some(queue) = announcements else {
                return error("announcements unavailable");
            };
            let text = match request.get("text") {
                Some(Value::String(text)) => text,
                Some(_) => return error("text must be a string"),
                None => return error("announce needs text"),
            };
            let lang = match request.get("lang") {
                None | Some(Value::Null) => None,
                Some(Value::String(lang)) => Some(lang.as_str()),
                Some(_) => return error("lang must be a string"),
            };
            match Announcement::new(text, lang).and_then(|a| queue.submit(a)) {
                Ok(position) => ok(json!({ "position": position })),
                Err(e) => error(e),
            }
        }
        Some(other) => error(format!("unknown command: {other}")),
        None => error("missing cmd"),
    }
}
