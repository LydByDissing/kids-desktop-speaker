use audio_player::manager::Manager;

#[tokio::test]
async fn missing_source_is_not_fatal() {
    let mut manager = Manager::new("missing", "/nonexistent/source", vec![]);
    assert!(!manager.start_if_available().await);
    assert!(manager.stop().await.is_ok());
}

#[tokio::test]
async fn available_source_starts() {
    let mut manager = Manager::new("sleep", "/bin/sleep", vec!["30".to_string()]);
    assert!(manager.start_if_available().await);
    assert!(manager.stop().await.is_ok());
}
