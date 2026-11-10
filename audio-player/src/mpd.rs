//! A small client for MPD's text protocol (DEC_005, RAD_REQ_001).
//!
//! Each call opens a connection, sends one command list and reads the reply. MPD
//! greets with `OK MPD <version>`; a reply is `key: value` lines ended by `OK`,
//! or an `ACK [...] {cmd} message` line on error.

use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Error)]
pub enum MpdError {
    #[error("mpd unreachable: {0}")]
    Io(#[from] std::io::Error),
    #[error("mpd timed out")]
    Timeout,
    #[error("mpd: {0}")]
    Ack(String),
    #[error("mpd: unexpected reply: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Play,
    Pause,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state: PlayState,
    /// What the stream reports as playing (ICY title), if anything.
    pub title: Option<String>,
}

/// Quotes an argument: wrapped in double quotes, with `"` and `\` escaped.
pub fn quote(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    for c in arg.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

#[derive(Debug, Clone)]
pub struct Mpd {
    pub addr: String,
}

impl Mpd {
    pub fn new(addr: impl Into<String>) -> Self {
        Mpd { addr: addr.into() }
    }

    /// Sends `commands` as one command list and returns the reply's key/value pairs.
    pub async fn run(&self, commands: &[String]) -> Result<Vec<(String, String)>, MpdError> {
        tokio::time::timeout(TIMEOUT, self.run_inner(commands))
            .await
            .map_err(|_| MpdError::Timeout)?
    }

    async fn run_inner(&self, commands: &[String]) -> Result<Vec<(String, String)>, MpdError> {
        let stream = TcpStream::connect(&self.addr).await?;
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let greeting = lines.next_line().await?.unwrap_or_default();
        if !greeting.starts_with("OK MPD ") {
            return Err(MpdError::Protocol(greeting));
        }
        let mut request = String::new();
        if commands.len() > 1 {
            request.push_str("command_list_begin\n");
        }
        for c in commands {
            if c.contains('\n') {
                return Err(MpdError::Protocol("newline in command".into()));
            }
            request.push_str(c);
            request.push('\n');
        }
        if commands.len() > 1 {
            request.push_str("command_list_end\n");
        }
        write.write_all(request.as_bytes()).await?;
        let mut pairs = Vec::new();
        while let Some(line) = lines.next_line().await? {
            if line == "OK" {
                return Ok(pairs);
            }
            if let Some(ack) = line.strip_prefix("ACK ") {
                let message = ack.split_once("} ").map(|(_, m)| m).unwrap_or(ack);
                return Err(MpdError::Ack(message.to_string()));
            }
            if let Some((k, v)) = line.split_once(": ") {
                pairs.push((k.to_string(), v.to_string()));
            }
        }
        Err(MpdError::Protocol("connection closed".into()))
    }

    /// Replaces the queue with `url` and plays it.
    pub async fn play_url(&self, url: &str) -> Result<(), MpdError> {
        self.run(&["clear".into(), format!("add {}", quote(url)), "play".into()])
            .await
            .map(|_| ())
    }

    pub async fn stop(&self) -> Result<(), MpdError> {
        self.run(&["stop".into()]).await.map(|_| ())
    }

    pub async fn set_paused(&self, paused: bool) -> Result<(), MpdError> {
        self.run(&[format!("pause {}", u8::from(paused))])
            .await
            .map(|_| ())
    }

    pub async fn status(&self) -> Result<Status, MpdError> {
        let pairs = self.run(&["status".into(), "currentsong".into()]).await?;
        let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let state = match get("state").as_deref() {
            Some("play") => PlayState::Play,
            Some("pause") => PlayState::Pause,
            _ => PlayState::Stop,
        };
        Ok(Status {
            state,
            title: get("Title"),
        })
    }
}
