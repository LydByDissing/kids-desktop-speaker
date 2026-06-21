use crate::installer::install;
use crate::state::SharedStatus;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
enum Request {
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "install")]
    Install { url: String },
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    state: String,
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
}

pub async fn serve(sock_path: &str, status: SharedStatus) -> Result<(), IpcError> {
    let _ = std::fs::remove_file(sock_path);
    let listener = UnixListener::bind(sock_path)?;
    let mut conn_set: JoinSet<()> = JoinSet::new();
    let mut install_set: JoinSet<()> = JoinSet::new();
    let (install_tx, mut install_rx) = mpsc::channel::<String>(4);

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (socket, _) = result?;
                let status_clone = status.clone();
                let tx = install_tx.clone();
                conn_set.spawn(async move {
                    if let Err(e) = handle_connection(socket, status_clone, tx).await {
                        eprintln!("ipc connection error: {e}");
                    }
                });
            }
            Some(_) = conn_set.join_next() => {}
            Some(url) = install_rx.recv() => {
                let status_clone = status.clone();
                install_set.spawn(async move {
                    if let Err(e) = install(&url, status_clone).await {
                        eprintln!("install failed: {e}");
                    }
                });
            }
            Some(_) = install_set.join_next() => {}
        }
    }
}

async fn handle_connection(
    socket: tokio::net::UnixStream,
    status: SharedStatus,
    install_tx: mpsc::Sender<String>,
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
            let current_status = status.get().await;
            let state_str = match &current_status.state {
                crate::state::OtaState::Idle => "idle".to_string(),
                crate::state::OtaState::Downloading => "downloading".to_string(),
                crate::state::OtaState::Installing => "installing".to_string(),
                crate::state::OtaState::Done => "done".to_string(),
                crate::state::OtaState::Error(msg) => format!("error: {msg}"),
            };
            serde_json::to_string(&StatusResponse {
                state: state_str,
                version: current_status.version,
            })?
        }
        Request::Install { url } => {
            let _ = install_tx.send(url).await;
            serde_json::to_string(&OkResponse { ok: true })?
        }
    };

    writer.write_all(response.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}
