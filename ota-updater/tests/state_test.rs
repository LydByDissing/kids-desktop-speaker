use ota_updater::{OtaState, SharedStatus};

#[tokio::test]
async fn test_initial_state_is_idle() {
    let status = SharedStatus::new();
    let s = status.get().await;
    assert_eq!(s.state, OtaState::Idle);
    assert_eq!(s.version, None);
}

#[tokio::test]
async fn test_set_downloading() {
    let status = SharedStatus::new();
    status.set(OtaState::Downloading).await;
    let s = status.get().await;
    assert_eq!(s.state, OtaState::Downloading);
}

#[tokio::test]
async fn test_set_installing() {
    let status = SharedStatus::new();
    status.set(OtaState::Installing).await;
    let s = status.get().await;
    assert_eq!(s.state, OtaState::Installing);
}

#[tokio::test]
async fn test_set_done() {
    let status = SharedStatus::new();
    status.set(OtaState::Done).await;
    let s = status.get().await;
    assert_eq!(s.state, OtaState::Done);
}

#[tokio::test]
async fn test_set_error_stores_message() {
    let status = SharedStatus::new();
    let error_msg = "rauc exit 1";
    status.set(OtaState::Error(error_msg.to_string())).await;
    let s = status.get().await;
    assert_eq!(s.state, OtaState::Error("rauc exit 1".to_string()));
}

#[test]
fn deliberate_failure_for_ci_check() {
    assert_eq!(1, 2);
}
