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
    assert_eq!(response, json!({"ok": true, "active": null, "radio": null}));
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

mod announce {
    use audio_player::announcer::AnnounceQueue;
    use audio_player::arbitrator::Arbitrator;
    use audio_player::ipc::{handle, handle_with};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn arb() -> Arc<Mutex<Arbitrator>> {
        Arc::new(Mutex::new(Arbitrator::new()))
    }

    #[tokio::test]
    async fn announce_queues_and_reports_the_position() {
        let (queue, mut rx) = AnnounceQueue::new();
        let r = handle_with(
            json!({"cmd": "announce", "text": "Hello", "lang": "en-GB"}),
            arb(),
            Some(&queue),
        )
        .await;
        assert_eq!(r, json!({"ok": true, "position": 1}));
        let queued = rx.recv().await.unwrap();
        assert_eq!(queued.text, "Hello");
        assert_eq!(queued.lang, audio_player::announcer::Lang::En);
    }

    #[tokio::test]
    async fn announce_rejects_bad_requests() {
        let (queue, _rx) = AnnounceQueue::new();
        for (request, message) in [
            (json!({"cmd": "announce"}), "announce needs text"),
            (
                json!({"cmd": "announce", "text": 5}),
                "text must be a string",
            ),
            (
                json!({"cmd": "announce", "text": " "}),
                "announce needs text",
            ),
            (
                json!({"cmd": "announce", "text": "Hej", "lang": 1}),
                "lang must be a string",
            ),
            (
                json!({"cmd": "announce", "text": "Hej", "lang": "fr"}),
                "language not enabled: fr",
            ),
        ] {
            let r = handle_with(request.clone(), arb(), Some(&queue)).await;
            assert_eq!(r, json!({"ok": false, "error": message}), "{request}");
        }
    }

    #[tokio::test]
    async fn announce_without_an_announcer_is_unavailable() {
        let r = handle(json!({"cmd": "announce", "text": "Hej"}), arb()).await;
        assert_eq!(
            r,
            json!({"ok": false, "error": "announcements unavailable"})
        );
    }
}
