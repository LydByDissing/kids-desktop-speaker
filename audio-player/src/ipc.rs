//! The Audio Player's IPC server: `status`, `mute`, `announce` and the radio
//! commands (IPC protocol, DEC-027, DEC-033, RAD_REQ_001).

use crate::announcer::{AnnounceQueue, Announcement};
use crate::arbitrator::{Arbitrator, Source};
use crate::radio::StationStore;
use crate::stream::{PlayState, Player};
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Internet radio: the station list and the player that plays it.
pub struct Radio {
    pub stations: StationStore,
    pub player: Player,
    /// The station last started, for status.
    pub current: std::sync::Mutex<Option<String>>,
}

/// The optional parts of the Audio Player the IPC commands reach.
#[derive(Clone, Default)]
pub struct Services {
    pub announcements: Option<AnnounceQueue>,
    pub radio: Option<Arc<Radio>>,
    /// Where the API Server keeps recorded voice messages (DEC-037).
    pub messages_dir: Option<std::path::PathBuf>,
    /// True while an announcement or message plays.
    pub announcing: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Stops displaced sources (DEC-039).
    pub stopper: Option<Arc<dyn crate::switcher::Stopper>>,
    /// Podcast feeds and episodes (POD_REQ_001/002).
    pub podcasts: Option<Arc<crate::podcasts::Podcasts>>,
    /// The device volume and the limit (VOL_REQ_001).
    pub volume: Option<Arc<crate::volume::Volume>>,
}

/// A percentage argument, 0-100.
fn percent_arg(request: &Value, name: &str) -> Result<u8, String> {
    match request.get(name).and_then(Value::as_u64) {
        Some(v) if v <= 100 => Ok(v as u8),
        _ => Err(format!("{name} must be a whole number from 0 to 100")),
    }
}

/// A message id names a file in the messages directory, so it may only hold
/// letters, digits and dashes: nothing that can reach another file.
pub fn valid_message_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub async fn serve(
    sock_path: &str,
    arb: Arc<Mutex<Arbitrator>>,
    services: Services,
) -> std::io::Result<()> {
    kds_ipc::serve(sock_path, move |request| {
        let arb = Arc::clone(&arb);
        let services = services.clone();
        async move { handle_full(request, arb, &services).await }
    })
    .await
}

fn source_name(source: &Source) -> &'static str {
    source.name()
}

pub async fn handle(request: Value, arb: Arc<Mutex<Arbitrator>>) -> Value {
    handle_with(request, arb, None).await
}

pub async fn handle_with(
    request: Value,
    arb: Arc<Mutex<Arbitrator>>,
    announcements: Option<&AnnounceQueue>,
) -> Value {
    let services = Services {
        announcements: announcements.cloned(),
        ..Services::default()
    };
    handle_full(request, arb, &services).await
}

fn str_arg<'a>(request: &'a Value, name: &str) -> Result<&'a str, String> {
    match request.get(name) {
        Some(Value::String(s)) => Ok(s),
        Some(_) => Err(format!("{name} must be a string")),
        None => Err(format!("missing {name}")),
    }
}

async fn radio_status(radio: &Radio) -> Value {
    let station = radio.current.lock().ok().and_then(|c| c.clone());
    match radio.player.status().await {
        Ok(status) => {
            let state = match status.state {
                PlayState::Play => "playing",
                PlayState::Pause => "paused",
                PlayState::Stop => "stopped",
            };
            let station = if status.state == PlayState::Stop {
                None
            } else {
                station
            };
            json!({ "state": state, "station": station, "title": status.title })
        }
        Err(e) => json!({ "state": "unavailable", "error": e.to_string() }),
    }
}

async fn handle_radio(
    cmd: &str,
    request: &Value,
    radio: &Radio,
    arb: &Mutex<Arbitrator>,
    stopper: Option<&Arc<dyn crate::switcher::Stopper>>,
) -> Value {
    let result: Result<Value, String> = async {
        match cmd {
            "radio_stations" => Ok(json!({ "stations": radio.stations.list()? })),
            "radio_add" => {
                let station = radio
                    .stations
                    .add(str_arg(request, "name")?, str_arg(request, "url")?)?;
                Ok(json!({ "station": station }))
            }
            "radio_remove" => {
                radio.stations.remove(str_arg(request, "id")?)?;
                Ok(Value::Null)
            }
            "radio_play" => {
                let station = radio.stations.get(str_arg(request, "id")?)?;
                radio
                    .player
                    .play_url(&station.url)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Ok(mut current) = radio.current.lock() {
                    *current = Some(station.id.clone());
                }
                // The radio is now the newest source (DEC-039).
                match stopper {
                    Some(stopper) => {
                        crate::switcher::source_active(arb, stopper.as_ref(), Source::Radio).await;
                    }
                    None => {
                        arb.lock().await.source_started(Source::Radio);
                    }
                }
                Ok(json!({ "station": station }))
            }
            "radio_stop" => {
                radio.player.stop().await.map_err(|e| e.to_string())?;
                if let Ok(mut current) = radio.current.lock() {
                    *current = None;
                }
                arb.lock().await.source_stopped(Source::Radio);
                Ok(Value::Null)
            }
            _ => Ok(json!({ "radio": radio_status(radio).await })),
        }
    }
    .await;
    match result {
        Ok(data) => ok(data),
        Err(e) => error(e),
    }
}

async fn handle_podcast(
    cmd: &str,
    request: &Value,
    podcasts: &crate::podcasts::Podcasts,
    radio: Option<&Radio>,
    arb: &Mutex<Arbitrator>,
    stopper: Option<&Arc<dyn crate::switcher::Stopper>>,
) -> Value {
    use crate::podcasts::{fetch, parse_feed};
    let result: Result<Value, String> = async {
        match cmd {
            "podcast_feeds" => Ok(json!({ "feeds": podcasts.store.feeds()? })),
            "podcast_add" => {
                let url = str_arg(request, "url")?;
                let (title, _) = parse_feed(&fetch(url).await?)?;
                Ok(json!({ "feed": podcasts.store.add(url, &title)? }))
            }
            "podcast_remove" => {
                podcasts.store.remove(str_arg(request, "id")?)?;
                Ok(Value::Null)
            }
            "podcast_episodes" => {
                let feed = podcasts.store.feed(str_arg(request, "id")?)?;
                let (_, episodes) = parse_feed(&fetch(&feed.url).await?)?;
                Ok(json!({ "feed": feed, "episodes": podcasts.store.with_positions(episodes) }))
            }
            "podcast_play" => {
                let episode = podcasts
                    .play(str_arg(request, "feed")?, str_arg(request, "episode")?)
                    .await?;
                // A station is no longer playing; podcasts are the radio source.
                if let Some(radio) = radio {
                    if let Ok(mut current) = radio.current.lock() {
                        *current = None;
                    }
                }
                match stopper {
                    Some(stopper) => {
                        crate::switcher::source_active(arb, stopper.as_ref(), Source::Radio).await;
                    }
                    None => {
                        arb.lock().await.source_started(Source::Radio);
                    }
                }
                Ok(json!({ "episode": episode }))
            }
            _ => {
                podcasts.stop().await?;
                arb.lock().await.source_stopped(Source::Radio);
                Ok(Value::Null)
            }
        }
    }
    .await;
    match result {
        Ok(data) => ok(data),
        Err(e) => error(e),
    }
}

/// A Bluetooth remote's button (RMT_REQ_001, DEC-042).
async fn handle_remote(action: &str, arb: &Arc<Mutex<Arbitrator>>, services: &Services) -> Value {
    match action {
        "volume_up" | "volume_down" => {
            let Some(volume) = &services.volume else {
                return error("volume unavailable");
            };
            let current = volume.current().volume;
            let next = if action == "volume_up" {
                current.saturating_add(5).min(100)
            } else {
                current.saturating_sub(5)
            };
            match volume.set_volume(next).await {
                Ok((settings, _)) => ok(crate::volume::describe(&settings)),
                Err(e) => error(e),
            }
        }
        "play_pause" => {
            let Some(radio) = &services.radio else {
                return error("radio unavailable");
            };
            let state = radio.player.status().await.map(|s| s.state);
            match state {
                Ok(PlayState::Play) => match radio.player.set_paused(true).await {
                    Ok(()) => ok(json!({ "radio": "paused" })),
                    Err(e) => error(e),
                },
                Ok(PlayState::Pause) => match radio.player.set_paused(false).await {
                    Ok(()) => ok(json!({ "radio": "playing" })),
                    Err(e) => error(e),
                },
                _ => {
                    // The player is idle. Another source playing can't be paused
                    // from here; otherwise start the first station.
                    if let Some(active) = arb.lock().await.active() {
                        return ok(json!({ "ignored": active.name() }));
                    }
                    let Some(first) = radio
                        .stations
                        .list()
                        .ok()
                        .and_then(|l| l.into_iter().next())
                    else {
                        return error("no stations");
                    };
                    handle_radio(
                        "radio_play",
                        &json!({ "id": first.id }),
                        radio,
                        arb,
                        services.stopper.as_ref(),
                    )
                    .await
                }
            }
        }
        "next" | "previous" => {
            let Some(radio) = &services.radio else {
                return error("radio unavailable");
            };
            let current = radio.current.lock().ok().and_then(|c| c.clone());
            let (Some(current), Ok(stations)) = (current, radio.stations.list()) else {
                return ok(json!({ "ignored": "no station playing" }));
            };
            let Some(i) = stations.iter().position(|s| s.id == current) else {
                return ok(json!({ "ignored": "no station playing" }));
            };
            let n = stations.len();
            let j = if action == "next" {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            };
            handle_radio(
                "radio_play",
                &json!({ "id": stations[j].id }),
                radio,
                arb,
                services.stopper.as_ref(),
            )
            .await
        }
        other => error(format!("unknown action: {other}")),
    }
}

pub async fn handle_full(
    request: Value,
    arb: Arc<Mutex<Arbitrator>>,
    services: &Services,
) -> Value {
    let announcements = services.announcements.as_ref();
    match command(&request) {
        Some("status") => {
            let active = arb.lock().await.active().as_ref().map(source_name);
            let radio = match &services.radio {
                Some(radio) => radio_status(radio).await,
                None => Value::Null,
            };
            let playing = services
                .announcing
                .as_ref()
                .is_some_and(|b| b.load(std::sync::atomic::Ordering::SeqCst));
            let waiting = announcements.map(AnnounceQueue::waiting).unwrap_or(0);
            let podcast = services
                .podcasts
                .as_ref()
                .and_then(|p| p.current())
                .map(|(feed, episode)| json!({ "feed": feed, "episode": episode }));
            ok(
                json!({ "active": active, "radio": radio, "podcast": podcast, "announcing": playing || waiting > 0 }),
            )
        }
        Some(
            cmd @ ("radio_stations" | "radio_add" | "radio_remove" | "radio_play" | "radio_stop"
            | "radio_status"),
        ) => match &services.radio {
            Some(radio) => {
                handle_radio(cmd, &request, radio, &arb, services.stopper.as_ref()).await
            }
            None => error("radio unavailable"),
        },
        Some(
            cmd @ ("podcast_feeds" | "podcast_add" | "podcast_remove" | "podcast_episodes"
            | "podcast_play" | "podcast_stop"),
        ) => match &services.podcasts {
            Some(podcasts) => {
                handle_podcast(
                    cmd,
                    &request,
                    podcasts,
                    services.radio.as_deref(),
                    &arb,
                    services.stopper.as_ref(),
                )
                .await
            }
            None => error("podcasts unavailable"),
        },
        Some("source_volume") => {
            // A phone changed its volume: that is the speaker's one volume, so
            // the next source plays at it too (DEC-049).
            let Some(volume) = &services.volume else {
                return error("volume unavailable");
            };
            let source = request.get("source").and_then(Value::as_str).unwrap_or("");
            let raw = match request.get("value") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                _ => return error("source_volume needs value"),
            };
            let Some(fraction) = crate::volume::sender_fraction(source, &raw) else {
                return error(format!("cannot read a {source} volume from {raw:?}"));
            };
            let target = crate::volume::from_sender(fraction, volume.current().limit);
            match volume.set_volume(target).await {
                Ok((settings, _)) => ok(crate::volume::describe(&settings)),
                Err(e) => error(e),
            }
        }
        Some(cmd @ ("volume" | "set_volume" | "set_limit")) => {
            let Some(volume) = &services.volume else {
                return error("volume unavailable");
            };
            let applied = match cmd {
                "set_volume" => match percent_arg(&request, "volume") {
                    Ok(v) => volume.set_volume(v).await,
                    Err(e) => return error(e),
                },
                "set_limit" => match percent_arg(&request, "limit") {
                    Ok(l) => volume.set_limit(l).await,
                    Err(e) => return error(e),
                },
                _ => Ok((volume.current(), None)),
            };
            match applied {
                Ok((settings, warning)) => {
                    let mut reply = crate::volume::describe(&settings);
                    if let Some(w) = warning {
                        reply["warning"] = json!(w);
                    }
                    ok(reply)
                }
                Err(e) => error(e),
            }
        }
        Some("remote") => {
            let action = request
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            handle_remote(action, &arb, services).await
        }
        Some("stop_all") => {
            // The sleep timer (SLP_REQ_001): stop whatever plays, the way each
            // source allows (DEC-039), and the player in any case.
            let playing: Vec<Source> = {
                let a = arb.lock().await;
                Source::ALL
                    .into_iter()
                    .filter(|s| a.state_of(s) == crate::arbitrator::SourceState::Playing)
                    .collect()
            };
            let mut stopped = Vec::new();
            if let Some(stopper) = &services.stopper {
                for source in &playing {
                    match stopper.stop(*source).await {
                        Ok(()) => stopped.push(source.name()),
                        Err(e) => eprintln!("audio-player: could not stop {}: {e}", source.name()),
                    }
                }
            }
            if !playing.contains(&Source::Radio) {
                if let Some(radio) = &services.radio {
                    let _ = radio.player.stop().await;
                }
            }
            let mut a = arb.lock().await;
            for source in Source::ALL {
                a.source_stopped(source);
            }
            ok(json!({ "stopped": stopped }))
        }
        Some("source_event") => {
            let source = match request.get("source").and_then(Value::as_str) {
                Some(name) => match Source::parse(name) {
                    Some(s) => s,
                    None => return error(format!("unknown source: {name}")),
                },
                None => return error("source_event needs source"),
            };
            let Some(event) = request.get("state").and_then(Value::as_str) else {
                return error("source_event needs state");
            };
            match crate::switcher::event_state(source, event) {
                Some(true) => {
                    let displaced = match &services.stopper {
                        Some(stopper) => {
                            crate::switcher::source_active(&arb, stopper.as_ref(), source).await
                        }
                        None => {
                            arb.lock().await.source_started(source);
                            Vec::new()
                        }
                    };
                    let names: Vec<&str> = displaced.iter().map(|s| s.name()).collect();
                    ok(json!({ "displaced": names }))
                }
                Some(false) => {
                    arb.lock().await.source_stopped(source);
                    ok(Value::Null)
                }
                None => ok(Value::Null),
            }
        }
        Some("mute") => {
            let source = match request.get("source").and_then(Value::as_str) {
                Some(name) => match Source::parse(name) {
                    Some(s) => s,
                    None => return error(format!("unknown source: {name}")),
                },
                None => return error("mute needs source"),
            };
            arb.lock().await.source_stopped(source);
            ok(Value::Null)
        }
        Some("announce") => {
            let Some(queue) = announcements else {
                return error("announcements unavailable");
            };
            let text = match request.get("text") {
                Some(Value::String(text)) => text,
                Some(_) => return error("text must be a string"),
                None => return error("announce needs text"),
            };
            let lang = match request.get("lang") {
                None | Some(Value::Null) => None,
                Some(Value::String(lang)) => Some(lang.as_str()),
                Some(_) => return error("lang must be a string"),
            };
            let slow = match request.get("slow") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(slow)) => *slow,
                Some(_) => return error("slow must be true or false"),
            };
            let announcement =
                Announcement::new(text, lang).map(|a| if slow { a.slow() } else { a });
            match announcement.and_then(|a| queue.submit(a)) {
                Ok(position) => ok(json!({ "position": position })),
                Err(e) => error(e),
            }
        }
        Some("play_message") => {
            let (Some(queue), Some(dir)) = (announcements, &services.messages_dir) else {
                return error("messages unavailable");
            };
            let id = match str_arg(&request, "id") {
                Ok(id) if valid_message_id(id) => id,
                Ok(_) => return error("invalid message id"),
                Err(e) => return error(e),
            };
            let path = dir.join(format!("{id}.wav"));
            if !path.is_file() {
                return error(format!("no message {id}"));
            }
            match queue.submit_recording(path) {
                Ok(position) => ok(json!({ "position": position })),
                Err(e) => error(e),
            }
        }
        Some(other) => error(format!("unknown command: {other}")),
        None => error("missing cmd"),
    }
}
