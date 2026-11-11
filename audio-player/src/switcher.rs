//! One source at a time (AUD_REQ_001, AUD_REQ_003, DEC-039): when a source
//! starts playing, the newest wins and the others are stopped.

use crate::arbitrator::{Arbitrator, Source, SourceState};
use crate::ipc::Radio;
use crate::manager::ManagedSources;
use serde_json::json;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Stops a source that another source displaced.
pub trait Stopper: Send + Sync {
    fn stop(&self, source: Source) -> BoxFuture<'_, Result<(), String>>;
}

/// What a receiver's event means: Some(true) when it starts playing,
/// Some(false) when it stops, None for events that change neither.
pub fn event_state(source: Source, event: &str) -> Option<bool> {
    match (source, event) {
        (_, "active") => Some(true),
        (_, "inactive") => Some(false),
        // librespot's --onevent PLAYER_EVENT values.
        (Source::Spotify, "playing") => Some(true),
        (Source::Spotify, "paused" | "stopped" | "session_disconnected" | "unavailable") => {
            Some(false)
        }
        // BlueZ MediaTransport1.State.
        (Source::Bluetooth, "idle") => Some(false),
        _ => None,
    }
}

/// Marks `source` as playing and stops every source it displaced. Returns the
/// displaced sources. The arbitrator is not held while sources stop.
pub async fn source_active(
    arb: &Mutex<Arbitrator>,
    stopper: &dyn Stopper,
    source: Source,
) -> Vec<Source> {
    let displaced: Vec<Source> = arb
        .lock()
        .await
        .source_started(source)
        .into_iter()
        .filter(|(s, state)| *s != source && *state == SourceState::Paused)
        .map(|(s, _)| s)
        .collect();
    for other in &displaced {
        match stopper.stop(*other).await {
            Ok(()) => println!(
                "audio-player: {} started; stopped {}",
                source.name(),
                other.name()
            ),
            Err(e) => eprintln!("audio-player: could not stop {}: {e}", other.name()),
        }
    }
    displaced
}

/// Talks to the Bluetooth Service's IPC socket.
#[derive(Debug, Clone)]
pub struct BtClient {
    pub socket: PathBuf,
}

impl BtClient {
    /// Sends an AVRCP command ("pause" or "play") to the connected phone.
    pub async fn avrcp(&self, action: &str) -> Result<(), String> {
        kds_ipc::request(&self.socket, &json!({"cmd": "avrcp", "action": action}))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Stops sources on the device: radio stops, AirPlay and Spotify sessions end
/// by restarting the receiver, and a Bluetooth phone is paused over AVRCP.
pub struct DeviceStopper {
    pub sources: Arc<Mutex<ManagedSources>>,
    pub radio: Option<Arc<Radio>>,
    pub podcasts: Option<Arc<crate::podcasts::Podcasts>>,
    pub bt: BtClient,
}

impl Stopper for DeviceStopper {
    fn stop(&self, source: Source) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            match source {
                Source::Radio => {
                    // A podcast episode keeps its position (POD_REQ_002).
                    if let Some(podcasts) = &self.podcasts {
                        podcasts.release().await;
                    }
                    let radio = self.radio.as_ref().ok_or("radio unavailable")?;
                    radio.mpd.stop().await.map_err(|e| e.to_string())?;
                    if let Ok(mut current) = radio.current.lock() {
                        *current = None;
                    }
                    Ok(())
                }
                Source::Airplay => self.sources.lock().await.restart("shairport-sync").await,
                Source::Spotify => self.sources.lock().await.restart("librespot").await,
                Source::Bluetooth => self.bt.avrcp("pause").await,
            }
        })
    }
}
