//! The device volume and the volume limit, applied as CamillaDSP's main volume
//! (VOL_REQ_001, FEAT_REQ_007, DEC-040).

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub const VOLUME_FILE: &str = "/data/audio/volume.json";
/// CamillaDSP's websocket, on localhost only (DEC-036).
pub const CAMILLA_ADDR: &str = "127.0.0.1:1234";
/// 1 % is this many dB below full; 100 % is 0 dB.
pub const RANGE_DB: f64 = 60.0;

/// The level in dB for a volume in percent, or None for 0 % (muted).
pub fn to_db(percent: u8) -> Option<f64> {
    match percent.min(100) {
        0 => None,
        p => Some(-RANGE_DB * (1.0 - p as f64 / 100.0)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub volume: u8,
    pub limit: u8,
}

impl Default for Settings {
    fn default() -> Self {
        // The volume starts at the limit (DEC-040).
        Settings {
            volume: 75,
            limit: 75,
        }
    }
}

impl Settings {
    /// Sets the volume, capped at the limit. Returns the volume set.
    pub fn set_volume(&mut self, volume: u8) -> u8 {
        self.volume = volume.min(100).min(self.limit);
        self.volume
    }

    /// Sets the limit; a volume above it comes down to it.
    pub fn set_limit(&mut self, limit: u8) {
        self.limit = limit.min(100);
        self.volume = self.volume.min(self.limit);
    }
}

/// Settings kept in a file on the data partition.
pub struct VolumeStore {
    path: PathBuf,
}

impl VolumeStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        VolumeStore { path: path.into() }
    }

    pub fn load(&self) -> Settings {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .map(|s| {
                let mut s = s;
                s.set_limit(s.limit);
                s
            })
            .unwrap_or_default()
    }

    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        let dir = self.path.parent().map(PathBuf::from).unwrap_or_default();
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot save the volume: {e}"))?;
        let temp = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&temp, json)
            .and_then(|()| std::fs::rename(&temp, &self.path))
            .map_err(|e| format!("cannot save the volume: {e}"))
    }
}

/// A client for CamillaDSP's websocket.
#[derive(Debug, Clone)]
pub struct Camilla {
    pub addr: String,
}

impl Camilla {
    /// Sends commands in order on one connection; returns the replies.
    pub async fn send(&self, commands: &[Value]) -> Result<Vec<Value>, String> {
        let url = format!("ws://{}", self.addr);
        let work = async {
            let (mut ws, _) = tokio_tungstenite::connect_async(url.as_str())
                .await
                .map_err(|e| format!("CamillaDSP unreachable: {e}"))?;
            let mut replies = Vec::new();
            for command in commands {
                ws.send(Message::Text(command.to_string().into()))
                    .await
                    .map_err(|e| e.to_string())?;
                loop {
                    match ws.next().await {
                        Some(Ok(Message::Text(text))) => {
                            let reply: Value =
                                serde_json::from_str(text.as_str()).map_err(|e| e.to_string())?;
                            let result = reply
                                .as_object()
                                .and_then(|o| o.values().next())
                                .and_then(|v| v["result"].as_str());
                            if result != Some("Ok") {
                                return Err(format!("CamillaDSP refused {command}: {reply}"));
                            }
                            replies.push(reply);
                            break;
                        }
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => return Err(e.to_string()),
                        None => return Err("CamillaDSP closed the connection".into()),
                    }
                }
            }
            let _ = ws.close(None).await;
            Ok(replies)
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| "CamillaDSP did not answer".to_string())?
    }

    /// Applies a volume: muted at 0 %, otherwise the level in dB.
    pub async fn apply(&self, percent: u8) -> Result<(), String> {
        let commands = match to_db(percent) {
            None => vec![json!({"SetMute": true})],
            Some(db) => vec![
                json!({"SetVolume": (db * 10.0).round() / 10.0}),
                json!({"SetMute": false}),
            ],
        };
        self.send(&commands).await.map(|_| ())
    }
}

/// The volume as the IPC commands see it.
pub struct Volume {
    pub store: VolumeStore,
    pub camilla: Camilla,
    pub settings: std::sync::Mutex<Settings>,
}

impl Volume {
    pub fn current(&self) -> Settings {
        self.settings.lock().map(|s| *s).unwrap_or_default()
    }

    fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut settings = self.settings.lock().map_err(|_| "volume unavailable")?;
        change(&mut settings);
        self.store.save(&settings)?;
        Ok(*settings)
    }

    /// Sets the volume (capped at the limit), saves it and applies it. A
    /// CamillaDSP that is down is reported but the setting is kept.
    pub async fn set_volume(&self, volume: u8) -> Result<(Settings, Option<String>), String> {
        let settings = self.update(|s| {
            s.set_volume(volume);
        })?;
        Ok((settings, self.camilla.apply(settings.volume).await.err()))
    }

    pub async fn set_limit(&self, limit: u8) -> Result<(Settings, Option<String>), String> {
        let settings = self.update(|s| s.set_limit(limit))?;
        Ok((settings, self.camilla.apply(settings.volume).await.err()))
    }

    /// Applies the saved volume, retrying until CamillaDSP answers.
    pub async fn apply_when_ready(&self) {
        let mut delay = Duration::from_secs(1);
        loop {
            match self.camilla.apply(self.current().volume).await {
                Ok(()) => return,
                Err(_) => {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(30));
                }
            }
        }
    }
}

/// How loud a sender (a phone over AirPlay, Spotify or Bluetooth) asks to be,
/// from 0.0 to 1.0, from the value its receiver reports: AirPlay's volume in
/// dB (-30 to 0, -144 for mute), Spotify's 0-65535, Bluetooth's AVRCP 0-127.
pub fn sender_fraction(source: &str, raw: &str) -> Option<f64> {
    let value: f64 = raw.trim().parse().ok().filter(|v: &f64| v.is_finite())?;
    let fraction = match source {
        "airplay" if value <= -30.0 => 0.0,
        "airplay" => (value + 30.0) / 30.0,
        "spotify" => value / 65535.0,
        "bluetooth" => value / 127.0,
        _ => return None,
    };
    Some(fraction.clamp(0.0, 1.0))
}

/// The device volume for a sender's volume: its whole range maps onto 0 to
/// the limit, so a phone turned all the way up plays at the limit and every
/// step of its slider counts (DEC-049).
pub fn from_sender(fraction: f64, limit: u8) -> u8 {
    (fraction.clamp(0.0, 1.0) * f64::from(limit)).round() as u8
}

/// The JSON a volume reply carries.
pub fn describe(settings: &Settings) -> Value {
    json!({
        "volume": settings.volume,
        "limit": settings.limit,
        "db": to_db(settings.volume).map(|d| (d * 10.0).round() / 10.0),
    })
}
