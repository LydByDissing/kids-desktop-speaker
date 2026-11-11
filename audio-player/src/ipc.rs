//! The Audio Player's IPC server: `status`, `mute`, `announce` and the radio
//! commands (IPC protocol, DEC-027, DEC-033, RAD_REQ_001).

use crate::announcer::{AnnounceQueue, Announcement};
use crate::arbitrator::{Arbitrator, Source};
use crate::mpd::{Mpd, PlayState};
use crate::radio::StationStore;
use kds_ipc::{command, error, ok};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Internet radio: the station list and the MPD that plays it.
pub struct Radio {
    pub stations: StationStore,
    pub mpd: Mpd,
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
    match source {
        Source::Airplay => "airplay",
        Source::Spotify => "spotify",
    }
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
    match radio.mpd.status().await {
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

async fn handle_radio(cmd: &str, request: &Value, radio: &Radio) -> Value {
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
                    .mpd
                    .play_url(&station.url)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Ok(mut current) = radio.current.lock() {
                    *current = Some(station.id.clone());
                }
                Ok(json!({ "station": station }))
            }
            "radio_stop" => {
                radio.mpd.stop().await.map_err(|e| e.to_string())?;
                if let Ok(mut current) = radio.current.lock() {
                    *current = None;
                }
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
            ok(json!({ "active": active, "radio": radio, "announcing": playing || waiting > 0 }))
        }
        Some(
            cmd @ ("radio_stations" | "radio_add" | "radio_remove" | "radio_play" | "radio_stop"
            | "radio_status"),
        ) => match &services.radio {
            Some(radio) => handle_radio(cmd, &request, radio).await,
            None => error("radio unavailable"),
        },
        Some("mute") => {
            let source = match request.get("source").and_then(Value::as_str) {
                Some("airplay") => Source::Airplay,
                Some("spotify") => Source::Spotify,
                Some(other) => return error(format!("unknown source: {other}")),
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
            match Announcement::new(text, lang).and_then(|a| queue.submit(a)) {
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
