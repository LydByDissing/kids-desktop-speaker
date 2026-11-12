//! Podcasts (POD_REQ_001/002): feed parsing, the store, positions, and the IPC
//! commands against a local feed server and the stream player (DEC-048).

mod common;

use audio_player::arbitrator::{Arbitrator, Source};
use audio_player::ipc::{handle_full, Services};
use audio_player::podcasts::{
    parse_duration, parse_feed, short_id, PodcastStore, Podcasts, MAX_EPISODES,
};
use audio_player::stream::sink::PacedSink;
use audio_player::stream::Player;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title>  Børneradio  </title>
    <item>
      <title>Afsnit 2</title>
      <guid>ep-2</guid>
      <pubDate>Tue, 10 Nov 2026 07:00:00 +0100</pubDate>
      <itunes:duration>01:02:03</itunes:duration>
      <enclosure url="http://example.com/ep2.mp3" type="audio/mpeg" length="1"/>
    </item>
    <item>
      <title>No audio</title>
    </item>
    <item>
      <title>Local file</title>
      <enclosure url="file:///etc/passwd" type="audio/mpeg"/>
    </item>
    <item>
      <title>Afsnit 1</title>
      <itunes:duration>754</itunes:duration>
      <enclosure url="https://example.com/ep1.mp3" type="audio/mpeg"/>
    </item>
  </channel>
</rss>"#;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-pod-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn feeds_are_parsed_newest_first_with_audio_only() {
    let (title, episodes) = parse_feed(FEED).unwrap();
    assert_eq!(title, "Børneradio");
    let titles: Vec<_> = episodes.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Afsnit 2", "Afsnit 1"],
        "items without http(s) audio are skipped"
    );
    assert_eq!(episodes[0].id, short_id("ep-2"));
    assert_eq!(
        episodes[1].id,
        short_id("https://example.com/ep1.mp3"),
        "no guid: the URL"
    );
    assert_eq!(episodes[0].duration_s, Some(3723));
    assert_eq!(episodes[1].duration_s, Some(754));
    assert_eq!(
        episodes[0].published.as_deref(),
        Some("Tue, 10 Nov 2026 07:00:00 +0100")
    );
    assert!(parse_feed("<html/>").is_err());
    assert!(parse_feed("not xml").is_err());
}

#[test]
fn long_feeds_are_capped() {
    let items: String = (0..80)
        .map(|i| format!("<item><title>E{i}</title><enclosure url=\"http://x/{i}.mp3\"/></item>"))
        .collect();
    let (_, episodes) = parse_feed(&format!(
        "<rss><channel><title>T</title>{items}</channel></rss>"
    ))
    .unwrap();
    assert_eq!(episodes.len(), MAX_EPISODES);
    assert_eq!(episodes[0].title, "E0");
}

#[test]
fn durations() {
    assert_eq!(parse_duration("90"), Some(90));
    assert_eq!(parse_duration("12:34"), Some(754));
    assert_eq!(parse_duration("1:02:03"), Some(3723));
    assert_eq!(parse_duration("soon"), None);
}

#[test]
fn feeds_and_positions_are_kept() {
    let store = PodcastStore::new(temp_dir("store"));
    let feed = store
        .add("http://example.com/feed.xml", "Børneradio")
        .unwrap();
    assert_eq!(
        store.add("http://example.com/feed.xml", "again").unwrap(),
        feed,
        "no duplicates"
    );
    assert!(store.add("file:///etc/passwd", "x").is_err());
    assert_eq!(store.feeds().unwrap().len(), 1);

    store.save_position("e1", 125.0, Some(3600.0)).unwrap();
    assert_eq!(store.positions().get("e1"), Some(&125.0));
    store.save_position("e1", 3590.0, Some(3600.0)).unwrap();
    assert_eq!(
        store.positions().get("e1"),
        None,
        "near the end it starts over next time"
    );
    store.remove(&feed.id).unwrap();
    assert!(store.remove(&feed.id).is_err());
}

/// Serves FEED over HTTP for any request, with episode 2's audio on `audio`.
async fn feed_server(audio: &common::AudioServer) -> String {
    let feed = FEED.replace(
        "http://example.com/ep2.mp3",
        &audio.url("/file/episode.mp3"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = listener.accept().await.unwrap();
            let feed = feed.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let body = feed.as_bytes();
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/rss+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(body).await;
            });
        }
    });
    format!("http://{addr}/feed.xml")
}

/// Podcasts on a player whose output runs 4× faster than real time.
async fn podcasts_with(name: &str) -> (Services, Arc<Podcasts>) {
    let podcasts = Arc::new(Podcasts {
        store: PodcastStore::new(temp_dir(name)),
        player: Player::new(Arc::new(PacedSink::new(4.0))),
        current: StdMutex::new(None),
    });
    let services = Services {
        podcasts: Some(Arc::clone(&podcasts)),
        ..Services::default()
    };
    (services, podcasts)
}

async fn elapsed(podcasts: &Podcasts) -> f64 {
    podcasts
        .player
        .status()
        .await
        .unwrap()
        .elapsed
        .unwrap_or(0.0)
}

async fn ipc(s: &Services, arb: &Arc<Mutex<Arbitrator>>, request: Value) -> Value {
    handle_full(request, Arc::clone(arb), s).await
}

#[tokio::test]
async fn feeds_are_added_by_url_and_list_their_episodes() {
    let audio = common::start();
    let url = feed_server(&audio).await;
    let (s, _) = podcasts_with("ipc-add").await;
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let r = ipc(&s, &arb, json!({"cmd": "podcast_add", "url": url})).await;
    assert_eq!(r["feed"]["title"], "Børneradio", "{r}");
    let id = r["feed"]["id"].as_str().unwrap().to_string();
    let r = ipc(&s, &arb, json!({"cmd": "podcast_episodes", "id": id})).await;
    assert_eq!(r["episodes"].as_array().unwrap().len(), 2);
    assert_eq!(r["episodes"][0]["title"], "Afsnit 2");
    let r = ipc(&s, &arb, json!({"cmd": "podcast_feeds"})).await;
    assert_eq!(r["feeds"][0]["id"], id.as_str());
    assert_eq!(
        ipc(&s, &arb, json!({"cmd": "podcast_remove", "id": id})).await,
        json!({"ok": true})
    );
    let r = ipc(
        &s,
        &arb,
        json!({"cmd": "podcast_add", "url": "file:///etc/passwd"}),
    )
    .await;
    assert_eq!(r["ok"], false);
}

#[tokio::test]
async fn an_episode_resumes_where_it_stopped() {
    let audio = common::start();
    let url = feed_server(&audio).await;
    let (s, podcasts) = podcasts_with("ipc-play").await;
    let arb = Arc::new(Mutex::new(Arbitrator::new()));
    let feed = ipc(&s, &arb, json!({"cmd": "podcast_add", "url": url})).await["feed"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let episode = short_id("ep-2");

    let r = ipc(
        &s,
        &arb,
        json!({"cmd": "podcast_play", "feed": feed, "episode": episode}),
    )
    .await;
    assert_eq!(r["episode"]["url"], audio.url("/file/episode.mp3"), "{r}");
    assert_eq!(
        arb.lock().await.active(),
        Some(Source::Radio),
        "podcasts are the radio source"
    );
    let status = ipc(&s, &arb, json!({"cmd": "status"})).await;
    assert_eq!(status["podcast"], json!({"feed": feed, "episode": episode}));
    // Listen a while (4× fast), then stop: that saves the position.
    for _ in 0..200 {
        if elapsed(&podcasts).await >= 4.0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        ipc(&s, &arb, json!({"cmd": "podcast_stop"})).await,
        json!({"ok": true})
    );
    let saved = *podcasts.store.positions().get(&episode).expect("saved");
    assert!(saved >= 4.0, "{saved}");
    assert_eq!(arb.lock().await.active(), None);

    // Playing it again seeks to where it stopped: from the start, 4× fast,
    // it would take a second to get there.
    let r = ipc(
        &s,
        &arb,
        json!({"cmd": "podcast_play", "feed": feed, "episode": episode}),
    )
    .await;
    assert_eq!(r["episode"]["position_s"], saved);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let now = elapsed(&podcasts).await;
    assert!(now >= saved - 0.5, "resumed at {now}, saved {saved}");
    let r = ipc(
        &s,
        &arb,
        json!({"cmd": "podcast_play", "feed": feed, "episode": "nope"}),
    )
    .await;
    assert_eq!(r, json!({"ok": false, "error": "no episode nope"}));
}
