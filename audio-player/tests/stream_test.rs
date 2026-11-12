//! The stream player (DEC-048) against a local HTTP server: formats, live
//! streams with titles, pause, seek, stop, failures and reconnecting.

mod common;

use audio_player::stream::sink::PacedSink;
use audio_player::stream::{PlayState, Player, PlayerError, Status};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// A player whose output keeps time `speed` times faster than real time.
fn player(speed: f64) -> (Player, PacedSink) {
    let sink = PacedSink::new(speed);
    (Player::new(Arc::new(sink.clone())), sink)
}

async fn wait_for(player: &Player, what: &str, check: impl Fn(&Status) -> bool) -> Status {
    for _ in 0..200 {
        let status = player.status().await.unwrap();
        if check(&status) {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}: {:?}", player.status().await);
}

#[tokio::test]
async fn every_format_plays_to_its_end() {
    let server = common::start();
    for name in ["tone.mp3", "tone.m4a", "tone.aac", "tone.ogg", "tone.flac"] {
        let (player, sink) = player(40.0);
        player
            .play_url(&server.url(&format!("/file/{name}")))
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let playing = player.status().await.unwrap();
        assert_eq!(playing.state, PlayState::Play, "{name}");
        // A 4 s file at 40× ends after about 0.1 s.
        wait_for(&player, name, |s| s.state == PlayState::Stop).await;
        let frames = sink.frames.load(Ordering::Relaxed);
        let rate = if name.ends_with(".ogg") || name.ends_with(".flac") {
            22_050
        } else {
            44_100
        };
        let seconds = frames as f64 / rate as f64;
        assert!(
            (3.5..=4.6).contains(&seconds),
            "{name}: {seconds:.2} s played"
        );
    }
}

#[tokio::test]
async fn a_file_knows_its_length_and_how_far_it_got() {
    let server = common::start();
    let (player, _) = player(10.0);
    player
        .play_url(&server.url("/file/long.mp3"))
        .await
        .unwrap();
    let status = wait_for(&player, "elapsed", |s| s.elapsed.unwrap_or(0.0) > 1.0).await;
    let duration = status.duration.expect("a file has a duration");
    assert!((29.5..=30.5).contains(&duration), "{duration}");
    assert_eq!(status.title, None);
}

#[tokio::test]
async fn a_live_stream_reports_its_title() {
    let server = common::start();
    let (player, sink) = player(4.0);
    player
        .play_url(&server.url("/live/tone.mp3?title=Go%20Morgen%20P3"))
        .await
        .unwrap();
    let status = wait_for(&player, "the title", |s| s.title.is_some()).await;
    assert_eq!(status.title.as_deref(), Some("Go Morgen P3"));
    assert_eq!(status.state, PlayState::Play);
    assert_eq!(status.duration, None, "live has no length");
    assert!(sink.frames.load(Ordering::Relaxed) > 0);
    player.stop().await.unwrap();
}

#[tokio::test]
async fn pausing_stops_the_output_and_resuming_continues() {
    let server = common::start();
    let (player, sink) = player(4.0);
    player
        .play_url(&server.url("/live/tone.mp3"))
        .await
        .unwrap();
    wait_for(&player, "playing", |s| s.elapsed.is_some()).await;
    player.set_paused(true).await.unwrap();
    assert_eq!(player.status().await.unwrap().state, PlayState::Pause);
    tokio::time::sleep(Duration::from_millis(150)).await;
    let paused_at = sink.frames.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        sink.frames.load(Ordering::Relaxed),
        paused_at,
        "silent while paused"
    );
    player.set_paused(false).await.unwrap();
    assert_eq!(player.status().await.unwrap().state, PlayState::Play);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        sink.frames.load(Ordering::Relaxed) > paused_at,
        "plays again"
    );
    player.stop().await.unwrap();
}

#[tokio::test]
async fn seeking_jumps_into_a_file() {
    let server = common::start();
    for name in ["long.mp3", "tone.m4a"] {
        let (player, _) = player(1.0);
        player
            .play_url(&server.url(&format!("/file/{name}")))
            .await
            .unwrap();
        let to = if name == "long.mp3" { 20.0 } else { 2.5 };
        player.seek(to).await.unwrap();
        let status = wait_for(&player, "the seek", |s| {
            s.elapsed.is_some_and(|e| e >= to - 0.5)
        })
        .await;
        assert!(status.elapsed.unwrap() < to + 2.0, "{name}: {status:?}");
        player.stop().await.unwrap();
    }
    // The .m4a keeps its index at the end: reading it, and seeking, asked the
    // server for ranges. A short skip (long.mp3) just reads ahead.
    assert!(server.count("/file/tone.m4a") >= 2, "{:?}", server.requests);
}

#[tokio::test]
async fn stopping_ends_the_output() {
    let server = common::start();
    let (player, sink) = player(4.0);
    player
        .play_url(&server.url("/live/tone.mp3"))
        .await
        .unwrap();
    wait_for(&player, "playing", |s| s.elapsed.is_some()).await;
    player.stop().await.unwrap();
    assert_eq!(player.status().await.unwrap().state, PlayState::Stop);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let stopped_at = sink.frames.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(sink.frames.load(Ordering::Relaxed), stopped_at);
    assert!(matches!(
        player.set_paused(true).await,
        Err(PlayerError::NotPlaying)
    ));
}

#[tokio::test]
async fn streams_that_cannot_play_are_refused_with_a_reason() {
    let server = common::start();
    let (player, _) = player(4.0);
    for (path, reason) in [("/missing.mp3", "404"), ("/page", "unsupported")] {
        let err = player.play_url(&server.url(path)).await.unwrap_err();
        assert!(err.to_string().contains(reason), "{path}: {err}");
        assert_eq!(player.status().await.unwrap().state, PlayState::Stop);
    }
    let err = player
        .play_url("http://127.0.0.1:1/x.mp3")
        .await
        .unwrap_err();
    assert!(matches!(err, PlayerError::Open(_)), "{err}");
}

#[tokio::test]
async fn a_live_stream_that_drops_reconnects() {
    let server = common::start();
    let (player, _) = player(40.0);
    player
        .play_url(&server.url("/once/tone.mp3"))
        .await
        .unwrap();
    // Each pass ends after the file; the player reconnects up to 3 times
    // (after 1 and 2 s), then stops.
    for _ in 0..80 {
        if server.count("/once/") >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(server.count("/once/") >= 3, "{:?}", server.requests);
    player.stop().await.unwrap();
}

#[tokio::test]
async fn playing_something_else_replaces_what_plays() {
    let server = common::start();
    let (player, _) = player(4.0);
    player
        .play_url(&server.url("/live/tone.mp3?title=First"))
        .await
        .unwrap();
    wait_for(&player, "first title", |s| {
        s.title.as_deref() == Some("First")
    })
    .await;
    player
        .play_url(&server.url("/live/tone.mp3?title=Second"))
        .await
        .unwrap();
    let status = wait_for(&player, "second title", |s| s.title.is_some()).await;
    assert_eq!(status.title.as_deref(), Some("Second"));
    player.stop().await.unwrap();
}
