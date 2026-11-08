use bluetooth_service::ipc::handle;
use serde_json::json;
use std::sync::{Arc, RwLock};

#[test]
fn status_reports_connection() {
    let connected = Arc::new(RwLock::new(true));
    let response = handle(json!({"cmd": "status"}), connected);
    assert_eq!(
        response,
        json!({"ok": true, "connected": true, "controller": null})
    );
}

#[test]
fn unknown_command_is_an_error() {
    let response = handle(json!({"cmd": "pair"}), Arc::new(RwLock::new(false)));
    assert_eq!(
        response,
        json!({"ok": false, "error": "unknown command: pair"})
    );
}
