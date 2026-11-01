use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command as TokioCommand;

use crate::events::{parse_bluetoothctl_line, AvrcpEvent};

#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    #[error("spawn failed: {0}")]
    Spawn(std::io::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Controller {
    #[allow(dead_code)]
    addr: Option<String>, // paired controller MAC, None = not yet connected
}

impl Controller {
    pub fn new() -> Self {
        Controller { addr: None }
    }

    /// Runs bluetoothctl in a loop, reads stdout line by line.
    /// For each line: call parse_bluetoothctl_line; if Some(Ok(ev)) send on tx.
    /// If process exits, sleep 5s and restart (reconnect loop).
    pub async fn run(
        &mut self,
        tx: tokio::sync::mpsc::Sender<AvrcpEvent>,
    ) -> Result<(), ControllerError> {
        loop {
            match self.run_once(&tx).await {
                Ok(_) => {
                    // Process exited cleanly
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(e) => {
                    // Log error and retry after sleep
                    eprintln!("Controller error: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn run_once(
        &mut self,
        tx: &tokio::sync::mpsc::Sender<AvrcpEvent>,
    ) -> Result<(), ControllerError> {
        // Spawn bluetoothctl process
        let mut child = TokioCommand::new("bluetoothctl")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(ControllerError::Spawn)?;

        let stdout = child.stdout.take().ok_or_else(|| {
            ControllerError::Io(std::io::Error::other("failed to capture stdout"))
        })?;

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            if let Some(result) = parse_bluetoothctl_line(&line) {
                match result {
                    Ok(event) => {
                        // Send the event
                        let _ = tx.send(event).await;
                    }
                    Err(e) => {
                        eprintln!("Parse error: {}", e);
                    }
                }
            }
        }

        // Wait for child to exit
        let _ = child.wait().await;

        Ok(())
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}
