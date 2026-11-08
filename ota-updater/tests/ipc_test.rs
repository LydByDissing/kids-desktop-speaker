use ota_updater::ipc::{handle, Controls};
use ota_updater::{OtaState, SharedStatus};
use serde_json::json;

#[tokio::test]
async fn status_reports_state_and_version() {
    let status = SharedStatus::new();
    status.set_version("v1.2.3".into()).await;
    let response = handle(json!({"cmd": "status"}), status, &Controls::default()).await;
    assert_eq!(
        response,
        json!({"ok": true, "state": "idle", "detail": null, "version": "v1.2.3", "pending": null})
    );
}

#[tokio::test]
async fn status_separates_error_detail() {
    let status = SharedStatus::new();
    status
        .set(OtaState::Error("checksum mismatch".into()))
        .await;
    let response = handle(json!({"cmd": "status"}), status, &Controls::default()).await;
    assert_eq!(response["state"], "error");
    assert_eq!(response["detail"], "checksum mismatch");
}

#[tokio::test]
async fn install_requires_url_and_sha256() {
    let response = handle(
        json!({"cmd": "install", "url": "x"}),
        SharedStatus::new(),
        &Controls::default(),
    )
    .await;
    assert_eq!(
        response,
        json!({"ok": false, "error": "install needs url and sha256"})
    );
}

#[tokio::test]
async fn install_is_refused_while_one_is_running() {
    let status = SharedStatus::new();
    status.set(OtaState::Downloading).await;
    let response = handle(
        json!({"cmd": "install", "url": "x", "sha256": "y"}),
        status,
        &Controls::default(),
    )
    .await;
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"], "an install is already in progress");
}

#[tokio::test]
async fn unknown_command_is_an_error() {
    let response = handle(
        json!({"cmd": "format"}),
        SharedStatus::new(),
        &Controls::default(),
    )
    .await;
    assert_eq!(
        response,
        json!({"ok": false, "error": "unknown command: format"})
    );
}

#[tokio::test]
async fn status_reports_a_pending_update() {
    let status = SharedStatus::new();
    status.set_pending("v1.3.0".into()).await;
    let response = handle(json!({"cmd": "status"}), status, &Controls::default()).await;
    assert_eq!(response["pending"], "v1.3.0");
}

#[tokio::test]
async fn check_and_reboot_signal_the_background_loops() {
    let controls = Controls::default();
    for (cmd, notify) in [
        ("check", controls.check_now.clone()),
        ("reboot", controls.reboot_now.clone()),
    ] {
        let response = handle(json!({"cmd": cmd}), SharedStatus::new(), &controls).await;
        assert_eq!(response, json!({"ok": true}), "{cmd}");
        tokio::time::timeout(std::time::Duration::from_secs(1), notify.notified())
            .await
            .unwrap_or_else(|_| panic!("{cmd} did not signal"));
    }
}
