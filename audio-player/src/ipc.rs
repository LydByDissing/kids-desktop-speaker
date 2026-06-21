use crate::arbitrator::{Arbitrator, Source};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown source: {0}")]
    UnknownSource(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
enum Request {
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "mute")]
    Mute { source: String },
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    active: Option<String>,
}

#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
}

pub async fn serve(sock_path: &str, arb: Arc<Mutex<Arbitrator>>) -> Result<(), IpcError> {
    let _ = std::fs::remove_file(sock_path);
    let listener = UnixListener::bind(sock_path)?;
    let mut set: JoinSet<()> = JoinSet::new();

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (socket, _) = result?;
                let arb_clone = Arc::clone(&arb);
                set.spawn(async move {
                    if let Err(e) = handle_connection(socket, arb_clone).await {
                        eprintln!("ipc connection error: {e}");
                    }
                });
            }
            Some(_) = set.join_next() => {}
        }
    }
}

async fn handle_connection(
    socket: tokio::net::UnixStream,
    arb: Arc<Mutex<Arbitrator>>,
) -> Result<(), IpcError> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    reader.read_line(&mut line).await?;
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }

    let request: Request = serde_json::from_str(line)?;

    let response = match request {
        Request::Status => {
            let arb = arb.lock().await;
            let active = arb.active().map(|src| match src {
                Source::Airplay => "airplay".to_string(),
                Source::Spotify => "spotify".to_string(),
            });
            serde_json::to_string(&StatusResponse { active })?
        }
        Request::Mute { source } => {
            let src = match source.as_str() {
                "airplay" => Source::Airplay,
                "spotify" => Source::Spotify,
                other => return Err(IpcError::UnknownSource(other.to_string())),
            };
            let mut arb = arb.lock().await;
            arb.source_stopped(src);
            serde_json::to_string(&OkResponse { ok: true })?
        }
    };

    writer.write_all(response.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}
