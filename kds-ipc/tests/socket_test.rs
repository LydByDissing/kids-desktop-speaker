use kds_ipc::{error, ok, request, request_with_timeouts, serve, IpcError, MAX_MESSAGE_BYTES};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

fn socket_path(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("kds-ipc-{}-{name}", std::process::id()))
        .join("nested")
        .join("test.sock")
}

async fn start(name: &str) -> PathBuf {
    let path = socket_path(name);
    let server_path = path.clone();
    tokio::spawn(async move {
        serve(server_path, |req: Value| async move {
            match kds_ipc::command(&req) {
                Some("status") => ok(json!({"state": "idle"})),
                Some("slow") => {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    ok(Value::Null)
                }
                Some(other) => error(format!("unknown command: {other}")),
                None => unreachable!("serve validates cmd"),
            }
        })
        .await
        .unwrap();
    });
    // Wait until the server accepts connections. The file alone isn't enough: a
    // stale socket may already exist.
    for _ in 0..100 {
        if UnixStream::connect(&path).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    path
}

async fn raw_exchange(path: &PathBuf, bytes: &[u8]) -> Value {
    let mut stream = UnixStream::connect(path).await.unwrap();
    stream.write_all(bytes).await.unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).await.unwrap();
    serde_json::from_str(out.trim()).unwrap()
}

#[tokio::test]
async fn round_trip_and_creates_socket_directory() {
    let path = start("roundtrip").await;
    let response = request(&path, &json!({"cmd": "status"})).await.unwrap();
    assert_eq!(response, json!({"ok": true, "state": "idle"}));
}

#[tokio::test]
async fn unknown_command_is_a_remote_error() {
    let path = start("unknown").await;
    let err = request(&path, &json!({"cmd": "reboot"})).await.unwrap_err();
    assert!(matches!(err, IpcError::Remote(m) if m == "unknown command: reboot"));
}

#[tokio::test]
async fn malformed_and_commandless_requests_get_errors() {
    let path = start("malformed").await;
    let bad = raw_exchange(&path, b"{not json\n").await;
    assert_eq!(bad["ok"], false);
    assert!(bad["error"].as_str().unwrap().starts_with("invalid JSON"));
    let missing = raw_exchange(&path, b"{\"x\":1}\n").await;
    assert_eq!(missing, json!({"ok": false, "error": "missing cmd"}));
}

#[tokio::test]
async fn oversized_request_is_rejected() {
    let path = start("oversized").await;
    let mut big = vec![b'a'; MAX_MESSAGE_BYTES + 10];
    big.push(b'\n');
    let response = raw_exchange(&path, &big).await;
    assert_eq!(response["ok"], false);
    assert!(response["error"].as_str().unwrap().contains("longer than"));
}

#[tokio::test]
async fn slow_service_times_out() {
    let path = start("slow").await;
    let err = request_with_timeouts(
        &path,
        &json!({"cmd": "slow"}),
        Duration::from_secs(1),
        Duration::from_millis(300),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, IpcError::Timeout));
}

#[tokio::test]
async fn missing_service_is_an_io_error() {
    let err = request(socket_path("absent"), &json!({"cmd": "status"}))
        .await
        .unwrap_err();
    assert!(matches!(err, IpcError::Io(_)));
}

#[tokio::test]
async fn stale_socket_is_replaced() {
    let path = socket_path("stale");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"stale").unwrap();
    let started = start("stale").await;
    assert_eq!(started, path);
    let response = request(&path, &json!({"cmd": "status"})).await.unwrap();
    assert_eq!(response["ok"], true);
}
