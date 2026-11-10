use bluetooth_service::ipc::{handle, Context};
use bluetooth_service::pairing::{PairingWindow, DEFAULT_WINDOW};
use serde_json::json;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

fn context(connected: bool) -> Context {
    Context::new(
        Arc::new(RwLock::new(connected)),
        Arc::new(Mutex::new(PairingWindow::default())),
    )
}

#[tokio::test]
async fn status_reports_connection_and_a_closed_window() {
    let response = handle(json!({"cmd": "status"}), &context(true)).await;
    assert_eq!(
        response,
        json!({"ok": true, "connected": true, "controller": null,
               "pairing": {"open": false, "remaining_s": 0}})
    );
}

#[tokio::test]
async fn pair_without_bluez_is_unavailable() {
    let ctx = context(false);
    let response = handle(json!({"cmd": "pair"}), &ctx).await;
    assert_eq!(
        response,
        json!({"ok": false, "error": "bluetooth unavailable"})
    );
    assert!(!ctx.window.lock().unwrap().is_open(Instant::now()));
}

#[tokio::test]
async fn unknown_command_is_an_error() {
    let response = handle(json!({"cmd": "reboot"}), &context(false)).await;
    assert_eq!(
        response,
        json!({"ok": false, "error": "unknown command: reboot"})
    );
}

#[test]
fn window_opens_closes_and_expires() {
    let mut w = PairingWindow::default();
    let t0 = Instant::now();
    assert!(!w.is_open(t0));
    assert_eq!(w.open(t0, DEFAULT_WINDOW), Duration::from_secs(180));
    assert!(w.is_open(t0 + Duration::from_secs(179)));
    assert!(!w.is_open(t0 + Duration::from_secs(180)));
    assert_eq!(
        w.remaining(t0 + Duration::from_secs(60)),
        Duration::from_secs(120)
    );
    w.close();
    assert!(!w.is_open(t0));
    assert_eq!(w.remaining(t0), Duration::ZERO);
}
