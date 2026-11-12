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
    assert_eq!(
        response,
        json!({"ok": true, "active": null, "radio": null, "podcast": null, "announcing": false})
    );
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
    let unknown = handle(json!({"cmd": "mute", "source": "cassette"}), arbitrator()).await;
    assert_eq!(
        unknown,
        json!({"ok": false, "error": "unknown source: cassette"})
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
        let Some(audio_player::announcer::Item::Speech(queued)) = rx.recv().await else {
            panic!("expected speech");
        };
        assert_eq!(queued.text, "Hello");
        assert_eq!(queued.lang, audio_player::announcer::Lang::En);
        assert!(!queued.slow, "normal speed unless asked");
    }

    #[tokio::test]
    async fn announce_can_ask_to_be_read_slowly() {
        let (queue, mut rx) = AnnounceQueue::new();
        let r = handle_with(
            json!({"cmd": "announce", "text": "4, 7, 1", "slow": true}),
            arb(),
            Some(&queue),
        )
        .await;
        assert_eq!(r, json!({"ok": true, "position": 1}));
        let Some(audio_player::announcer::Item::Speech(queued)) = rx.recv().await else {
            panic!("expected speech");
        };
        assert!(queued.slow);
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
            (
                json!({"cmd": "announce", "text": "Hej", "slow": "yes"}),
                "slow must be true or false",
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

mod play_message {
    use audio_player::announcer::{AnnounceQueue, Item};
    use audio_player::arbitrator::Arbitrator;
    use audio_player::ipc::{handle_full, valid_message_id, Services};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn messages_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kds-msgs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn message_ids_cannot_reach_other_files() {
        assert!(valid_message_id("20261110-224500-ab12"));
        for bad in ["", "../etc/passwd", "a/b", "x.wav", "a b", &"x".repeat(65)] {
            assert!(!valid_message_id(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn play_message_queues_the_recording() {
        let dir = messages_dir("ok");
        std::fs::write(dir.join("m1.wav"), b"RIFF").unwrap();
        let (queue, mut rx) = AnnounceQueue::new();
        let services = Services {
            announcements: Some(queue),
            messages_dir: Some(dir.clone()),
            ..Services::default()
        };
        let arb = Arc::new(Mutex::new(Arbitrator::new()));
        let r = handle_full(json!({"cmd": "play_message", "id": "m1"}), arb, &services).await;
        assert_eq!(r, json!({"ok": true, "position": 1}));
        assert_eq!(rx.recv().await, Some(Item::Recording(dir.join("m1.wav"))));
    }

    #[tokio::test]
    async fn play_message_rejects_bad_and_missing_ids() {
        let dir = messages_dir("bad");
        let (queue, _rx) = AnnounceQueue::new();
        let services = Services {
            announcements: Some(queue),
            messages_dir: Some(dir),
            ..Services::default()
        };
        for (request, message) in [
            (
                json!({"cmd": "play_message", "id": "../../etc/passwd"}),
                "invalid message id",
            ),
            (
                json!({"cmd": "play_message", "id": "nope"}),
                "no message nope",
            ),
            (json!({"cmd": "play_message"}), "missing id"),
        ] {
            let arb = Arc::new(Mutex::new(Arbitrator::new()));
            assert_eq!(
                handle_full(request.clone(), arb, &services).await,
                json!({"ok": false, "error": message}),
                "{request}"
            );
        }
        let arb = Arc::new(Mutex::new(Arbitrator::new()));
        let r = handle_full(
            json!({"cmd": "play_message", "id": "x"}),
            arb,
            &Services::default(),
        )
        .await;
        assert_eq!(r, json!({"ok": false, "error": "messages unavailable"}));
    }
}

#[tokio::test]
async fn status_reports_when_an_announcement_plays_or_waits() {
    use audio_player::announcer::{AnnounceQueue, Announcement};
    use audio_player::ipc::{handle_full, Services};
    use std::sync::atomic::{AtomicBool, Ordering};
    let (queue, _rx) = AnnounceQueue::new();
    let busy = Arc::new(AtomicBool::new(false));
    let services = Services {
        announcements: Some(queue.clone()),
        announcing: Some(Arc::clone(&busy)),
        ..Services::default()
    };
    let status = || async {
        handle_full(json!({"cmd": "status"}), arbitrator(), &services).await["announcing"].clone()
    };
    assert_eq!(status().await, json!(false));
    busy.store(true, Ordering::SeqCst);
    assert_eq!(status().await, json!(true), "playing");
    busy.store(false, Ordering::SeqCst);
    queue.submit(Announcement::new("x", None).unwrap()).unwrap();
    assert_eq!(status().await, json!(true), "waiting in the queue");
}
