use audio_player::arbitrator::{Arbitrator, Source};
use audio_player::ipc::handle;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

fn arbitrator() -> Arc<Mutex<Arbitrator>> {
    Arc::new(Mutex::new(Arbitrator::new()))
}

#[tokio::test]
async fn status_reports_no_active_source_when_idle() {
    let response = handle(json!({"cmd": "status"}), arbitrator()).await;
    assert_eq!(response, json!({"ok": true, "active": null}));
}

#[tokio::test]
async fn status_reports_the_active_source() {
    let arb = arbitrator();
    arb.lock().await.source_started(Source::Spotify);
    let response = handle(json!({"cmd": "status"}), arb).await;
    assert_eq!(response["active"], "spotify");
}

#[tokio::test]
async fn mute_stops_the_source() {
    let arb = arbitrator();
    arb.lock().await.source_started(Source::Airplay);
    let response = handle(
        json!({"cmd": "mute", "source": "airplay"}),
        Arc::clone(&arb),
    )
    .await;
    assert_eq!(response, json!({"ok": true}));
    assert_eq!(arb.lock().await.active(), None);
}

#[tokio::test]
async fn mute_rejects_unknown_and_missing_sources() {
    let unknown = handle(json!({"cmd": "mute", "source": "radio"}), arbitrator()).await;
    assert_eq!(
        unknown,
        json!({"ok": false, "error": "unknown source: radio"})
    );
    let missing = handle(json!({"cmd": "mute"}), arbitrator()).await;
    assert_eq!(missing, json!({"ok": false, "error": "mute needs source"}));
}
