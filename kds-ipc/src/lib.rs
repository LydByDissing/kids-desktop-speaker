//! The speaker's IPC protocol: newline-delimited JSON over Unix sockets, one
//! request and one response per connection. See the IPC protocol page in the
//! architecture docs and DEC-027.

use serde_json::{json, Map, Value};
use std::future::Future;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Longest message accepted, in bytes, including the newline (IPC_REQ_003).
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
/// How long a client waits to connect.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a whole exchange may take, for clients and servers.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("message longer than {MAX_MESSAGE_BYTES} bytes")]
    TooLong,
    #[error("timed out")]
    Timeout,
    #[error("connection closed without a response")]
    Closed,
    #[error("service error: {0}")]
    Remote(String),
}

/// A successful response: `{"ok": true}` plus the fields of `data`, if it's an object.
pub fn ok(data: Value) -> Value {
    let mut map = match data {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    map.insert("ok".into(), Value::Bool(true));
    Value::Object(map)
}

/// A failed response: `{"ok": false, "error": message}`.
pub fn error(message: impl std::fmt::Display) -> Value {
    json!({ "ok": false, "error": message.to_string() })
}

/// The request's command, from its `cmd` field.
pub fn command(request: &Value) -> Option<&str> {
    request.get("cmd").and_then(Value::as_str)
}

/// Binds a server socket, creating its directory and replacing a stale socket,
/// so services work regardless of start order (IPC_REQ_003).
pub fn bind(path: impl AsRef<Path>) -> std::io::Result<UnixListener> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    UnixListener::bind(path)
}

/// Reads one newline-terminated message, refusing anything over the size limit.
async fn read_message<R: AsyncRead + Unpin>(reader: R) -> Result<Option<Vec<u8>>, IpcError> {
    let mut limited = BufReader::new(reader.take(MAX_MESSAGE_BYTES as u64 + 1));
    let mut buf = Vec::new();
    let n = limited.read_until(b'\n', &mut buf).await?;
    if n == 0 {
        return Ok(None);
    }
    if buf.len() > MAX_MESSAGE_BYTES || (buf.last() != Some(&b'\n') && n > MAX_MESSAGE_BYTES) {
        return Err(IpcError::TooLong);
    }
    Ok(Some(buf))
}

/// Discards the rest of an oversized message, so the client finishes sending and
/// reads the error instead of getting a reset. Bounded, so a client can't keep a
/// server reading forever.
async fn drain_line(stream: &mut UnixStream) {
    const DRAIN_LIMIT: usize = 16 * MAX_MESSAGE_BYTES;
    let mut buf = [0u8; 8192];
    let mut drained = 0;
    while drained < DRAIN_LIMIT {
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                drained += n;
                if buf[..n].contains(&b'\n') {
                    return;
                }
            }
        }
    }
}

async fn write_message(stream: &mut UnixStream, message: &Value) -> Result<(), IpcError> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    stream.write_all(&line).await?;
    stream.shutdown().await?;
    Ok(())
}

/// Turns one raw message into a validated request, or the error to send back.
fn parse_request(raw: &[u8]) -> Result<Value, Value> {
    let request: Value =
        serde_json::from_slice(raw).map_err(|e| error(format!("invalid JSON: {e}")))?;
    if command(&request).is_none() {
        return Err(error("missing cmd"));
    }
    Ok(request)
}

/// Serves requests on `path` until an accept fails. `handler` maps each request to
/// its response, and should use [`ok`] and [`error`]. Malformed, oversized or
/// command-less requests are answered with an error without reaching the handler.
pub async fn serve<F, Fut>(path: impl AsRef<Path>, handler: F) -> std::io::Result<()>
where
    F: Fn(Value) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Value> + Send,
{
    let listener = bind(path)?;
    loop {
        let (stream, _) = listener.accept().await?;
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(e) =
                tokio::time::timeout(REQUEST_TIMEOUT, handle_connection(stream, handler)).await
            {
                eprintln!("ipc: connection timed out: {e}");
            }
        });
    }
}

async fn handle_connection<F, Fut>(mut stream: UnixStream, handler: F)
where
    F: Fn(Value) -> Fut,
    Fut: Future<Output = Value>,
{
    let response = match read_message(&mut stream).await {
        Ok(None) => return,
        Ok(Some(raw)) => match parse_request(&raw) {
            Ok(request) => handler(request).await,
            Err(response) => response,
        },
        Err(IpcError::TooLong) => {
            drain_line(&mut stream).await;
            error(IpcError::TooLong)
        }
        Err(e) => error(e),
    };
    if let Err(e) = write_message(&mut stream, &response).await {
        eprintln!("ipc: failed to send response: {e}");
    }
}

/// Sends one request and returns the response's fields. A response with
/// `ok: false` becomes [`IpcError::Remote`].
pub async fn request(path: impl AsRef<Path>, request: &Value) -> Result<Value, IpcError> {
    request_with_timeouts(path, request, CONNECT_TIMEOUT, REQUEST_TIMEOUT).await
}

pub async fn request_with_timeouts(
    path: impl AsRef<Path>,
    request: &Value,
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<Value, IpcError> {
    let mut stream = tokio::time::timeout(connect_timeout, UnixStream::connect(path.as_ref()))
        .await
        .map_err(|_| IpcError::Timeout)??;
    let exchange = async {
        let mut line = serde_json::to_vec(request)?;
        line.push(b'\n');
        stream.write_all(&line).await?;
        let raw = read_message(&mut stream).await?.ok_or(IpcError::Closed)?;
        let response: Value = serde_json::from_slice(&raw)?;
        Ok::<Value, IpcError>(response)
    };
    let response = tokio::time::timeout(request_timeout, exchange)
        .await
        .map_err(|_| IpcError::Timeout)??;
    match response.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(response),
        _ => Err(IpcError::Remote(
            response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("response without ok")
                .to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_merges_data_fields() {
        assert_eq!(
            ok(json!({"state": "idle"})),
            json!({"ok": true, "state": "idle"})
        );
        assert_eq!(ok(Value::Null), json!({"ok": true}));
    }

    #[test]
    fn error_has_message() {
        assert_eq!(error("nope"), json!({"ok": false, "error": "nope"}));
    }

    #[test]
    fn parse_rejects_bad_json_and_missing_cmd() {
        assert!(parse_request(b"{not json").unwrap_err()["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid JSON"));
        assert_eq!(
            parse_request(br#"{"x":1}"#).unwrap_err(),
            error("missing cmd")
        );
        assert_eq!(
            command(&parse_request(br#"{"cmd":"status"}"#).unwrap()),
            Some("status")
        );
    }
}
