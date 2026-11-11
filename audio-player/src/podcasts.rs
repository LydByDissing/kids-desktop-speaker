//! Podcasts (POD_REQ_001/002): RSS feeds on the data partition, episodes played
//! through MPD, and how far each episode got.

use crate::radio::validate_url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PODCASTS_DIR: &str = "/data/podcasts";
/// Episodes listed per feed, newest first, so a reply fits the IPC limit.
pub const MAX_EPISODES: usize = 50;
/// The largest feed fetched.
const MAX_FEED_BYTES: usize = 5 * 1024 * 1024;
/// An episode played to within this of its end starts over next time.
pub const FINISHED_MARGIN_S: f64 = 60.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feed {
    pub id: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Episode {
    pub id: String,
    pub title: String,
    /// As the feed gives it (RFC 2822), for display.
    pub published: Option<String>,
    pub duration_s: Option<u64>,
    pub url: String,
    /// Where listening stopped, if it did part-way.
    pub position_s: Option<f64>,
}

/// A stable short id: FNV-1a of `text`, as 16 hex characters.
pub fn short_id(text: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Parses "HH:MM:SS", "MM:SS" or plain seconds (itunes:duration).
pub fn parse_duration(text: &str) -> Option<u64> {
    let parts: Vec<u64> = text
        .trim()
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [s] => Some(*s),
        [m, s] => Some(m * 60 + s),
        [h, m, s] => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

fn clip(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().take(max).collect()
}

/// The feed's title and its latest episodes (those with an http(s) enclosure).
pub fn parse_feed(xml: &str) -> Result<(String, Vec<Episode>), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("not a valid feed: {e}"))?;
    let channel = doc
        .descendants()
        .find(|n| n.has_tag_name("channel"))
        .ok_or("not an RSS feed (no channel)")?;
    let child_text = |node: roxmltree::Node, name: &str| {
        node.children()
            .find(|c| c.tag_name().name() == name)
            .and_then(|c| c.text())
            .map(str::to_string)
    };
    let title = child_text(channel, "title")
        .map(|t| clip(&t, 100))
        .unwrap_or_else(|| "Podcast".into());
    let mut episodes = Vec::new();
    for item in channel.children().filter(|c| c.has_tag_name("item")) {
        let Some(url) = item
            .children()
            .find(|c| c.has_tag_name("enclosure"))
            .and_then(|e| e.attribute("url"))
            .map(str::to_string)
        else {
            continue;
        };
        if validate_url(&url).is_err() {
            continue;
        }
        let guid = child_text(item, "guid").unwrap_or_else(|| url.clone());
        episodes.push(Episode {
            id: short_id(&guid),
            title: child_text(item, "title")
                .map(|t| clip(&t, 150))
                .unwrap_or_else(|| "Episode".into()),
            published: child_text(item, "pubDate").map(|t| clip(&t, 40)),
            duration_s: child_text(item, "duration").and_then(|d| parse_duration(&d)),
            url,
            position_s: None,
        });
        if episodes.len() == MAX_EPISODES {
            break;
        }
    }
    Ok((title, episodes))
}

/// Fetches a feed over http(s), at most 5 MB, within 20 s.
pub async fn fetch(url: &str) -> Result<String, String> {
    validate_url(url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("kids-desktop-speaker")
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("cannot fetch the feed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("the feed answered {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|n| n as usize > MAX_FEED_BYTES)
    {
        return Err("the feed is too large".into());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("cannot fetch the feed: {e}"))?;
    if bytes.len() > MAX_FEED_BYTES {
        return Err("the feed is too large".into());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "the feed is not UTF-8".into())
}

pub struct PodcastStore {
    dir: PathBuf,
}

impl PodcastStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        PodcastStore { dir: dir.into() }
    }

    fn read<T: serde::de::DeserializeOwned + Default>(&self, name: &str) -> Result<T, String> {
        match std::fs::read(self.dir.join(name)) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("{name} is damaged: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(format!("cannot read {name}: {e}")),
        }
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("cannot save {name}: {e}"))?;
        let path = self.dir.join(name);
        let temp = path.with_extension("tmp");
        let json = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
        std::fs::write(&temp, json)
            .and_then(|()| std::fs::rename(&temp, &path))
            .map_err(|e| format!("cannot save {name}: {e}"))
    }

    pub fn feeds(&self) -> Result<Vec<Feed>, String> {
        self.read("feeds.json")
    }

    pub fn feed(&self, id: &str) -> Result<Feed, String> {
        self.feeds()?
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| format!("no podcast {id}"))
    }

    pub fn add(&self, url: &str, title: &str) -> Result<Feed, String> {
        validate_url(url)?;
        let mut feeds = self.feeds()?;
        if let Some(existing) = feeds.iter().find(|f| f.url == url) {
            return Ok(existing.clone());
        }
        if feeds.len() >= 50 {
            return Err("too many podcasts (50)".into());
        }
        let feed = Feed {
            id: short_id(url),
            title: clip(title, 100),
            url: url.to_string(),
        };
        feeds.push(feed.clone());
        self.write("feeds.json", &feeds)?;
        Ok(feed)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut feeds = self.feeds()?;
        let before = feeds.len();
        feeds.retain(|f| f.id != id);
        if feeds.len() == before {
            return Err(format!("no podcast {id}"));
        }
        self.write("feeds.json", &feeds)
    }

    pub fn positions(&self) -> HashMap<String, f64> {
        self.read("positions.json").unwrap_or_default()
    }

    /// Records how far an episode got; near its end it is cleared, so the
    /// episode starts over next time (POD_REQ_002).
    pub fn save_position(
        &self,
        episode: &str,
        elapsed: f64,
        duration: Option<f64>,
    ) -> Result<(), String> {
        let mut positions = self.positions();
        let finished = duration.is_some_and(|d| d > 0.0 && elapsed >= d - FINISHED_MARGIN_S);
        if finished || elapsed < 1.0 {
            positions.remove(episode);
        } else {
            positions.insert(episode.to_string(), (elapsed * 10.0).round() / 10.0);
        }
        self.write("positions.json", &positions)
    }

    /// Adds the saved positions to fetched episodes.
    pub fn with_positions(&self, mut episodes: Vec<Episode>) -> Vec<Episode> {
        let positions = self.positions();
        for e in &mut episodes {
            e.position_s = positions.get(&e.id).copied();
        }
        episodes
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Podcasts as the IPC commands and the position saver see them.
pub struct Podcasts {
    pub store: PodcastStore,
    pub mpd: crate::mpd::Mpd,
    /// The episode playing now: (feed id, episode id).
    pub current: std::sync::Mutex<Option<(String, String)>>,
}

impl Podcasts {
    pub fn current(&self) -> Option<(String, String)> {
        self.current.lock().ok().and_then(|c| c.clone())
    }

    fn set_current(&self, value: Option<(String, String)>) {
        if let Ok(mut c) = self.current.lock() {
            *c = value;
        }
    }

    /// Saves how far the current episode got (while it plays or as it stops).
    pub async fn save_current(&self) {
        let Some((_, episode)) = self.current() else {
            return;
        };
        if let Ok(status) = self.mpd.status().await {
            if let Some(elapsed) = status.elapsed {
                let _ = self.store.save_position(&episode, elapsed, status.duration);
            }
        }
    }

    /// Plays an episode from where it was left (POD_REQ_002).
    pub async fn play(&self, feed_id: &str, episode_id: &str) -> Result<Episode, String> {
        let feed = self.store.feed(feed_id)?;
        let (_, episodes) = parse_feed(&fetch(&feed.url).await?)?;
        let episode = self
            .store
            .with_positions(episodes)
            .into_iter()
            .find(|e| e.id == episode_id)
            .ok_or_else(|| format!("no episode {episode_id}"))?;
        self.save_current().await;
        self.mpd
            .play_url(&episode.url)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(position) = episode.position_s {
            // MPD seeks once the stream is open; give it a few tries.
            for _ in 0..20 {
                if self.mpd.seek(position).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
        self.set_current(Some((feed.id, episode.id.clone())));
        Ok(episode)
    }

    /// Saves the position and stops.
    pub async fn stop(&self) -> Result<(), String> {
        self.save_current().await;
        self.set_current(None);
        self.mpd.stop().await.map_err(|e| e.to_string())
    }

    /// Forgets the current episode after saving its position, when something
    /// else takes over MPD (a station, or another source).
    pub async fn release(&self) {
        self.save_current().await;
        self.set_current(None);
    }
}
