use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("utf8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    connected: bool,
    controller: Option<String>,
}

/// Serves Unix socket at the given path.
/// Handles messages like {"cmd":"status"} → {"connected":false,"controller":null}
pub async fn serve(
    sock_path: &str,
    connected: Arc<std::sync::RwLock<bool>>,
) -> Result<(), IpcError> {
    // Ensure the directory exists
    if let Some(parent) = Path::new(sock_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove existing socket if present
    if Path::new(sock_path).exists() {
        std::fs::remove_file(sock_path)?;
    }

    let listener = UnixListener::bind(sock_path)?;

    let mut join_set: JoinSet<Result<(), IpcError>> = JoinSet::new();

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let connected_clone = Arc::clone(&connected);
                join_set.spawn(handle_connection(stream, connected_clone));
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    connected: Arc<std::sync::RwLock<bool>>,
) -> Result<(), IpcError> {
    let mut buffer = [0u8; 1024];

    match stream.read(&mut buffer).await? {
        0 => Ok(()), // Connection closed
        n => {
            let request_str = std::str::from_utf8(&buffer[..n])?;

            if let Ok(req) = serde_json::from_str::<serde_json::Value>(request_str) {
                let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("");

                let is_connected = connected
                    .read()
                    .map(|g| *g)
                    .map_err(|_| IpcError::Io(std::io::Error::other("state poisoned")))?;

                let response = match cmd {
                    "status" => StatusResponse {
                        connected: is_connected,
                        controller: None,
                    },
                    _ => StatusResponse {
                        connected: false,
                        controller: None,
                    },
                };

                let response_json = serde_json::to_string(&response)?;
                stream.write_all(response_json.as_bytes()).await?;
                stream.write_all(b"\n").await?;
            }

            Ok(())
        }
    }
}
