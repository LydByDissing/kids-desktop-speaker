//! The device volume and the volume limit (VOL_REQ_001, FEAT_REQ_007,
//! DEC-040), applied as the codec's analog line-out gain: the amplifier on the
//! HAT (DEC-050). CamillaDSP stays at 0 dB, so the DAC always gets the whole
//! digital signal. Without the codec (no HAT, the VM) CamillaDSP's volume
//! takes over, on the same curve.

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub const VOLUME_FILE: &str = "/data/audio/volume.json";
/// CamillaDSP's websocket, on localhost only (DEC-036).
pub const CAMILLA_ADDR: &str = "127.0.0.1:1234";
/// 1 % is this many dB below full; 100 % is 0 dB: the line-out's whole range.
pub const RANGE_DB: f64 = 63.0;
/// The DA7213's "Lineout Volume": steps 0-63, one dB each, 48 is 0 dB.
pub const LINEOUT_MAX: u8 = 63;
/// The step 100 % plays at: the amplifier's full output (voicing may lower it).
pub const LINEOUT_FULL: u8 = 63;

/// The line-out step for a volume, or None for 0 % (muted).
pub fn lineout_step(percent: u8) -> Option<u8> {
    to_db(percent).map(|db| {
        (f64::from(LINEOUT_FULL) + db)
            .round()
            .clamp(0.0, f64::from(LINEOUT_MAX)) as u8
    })
}

/// CamillaDSP's loudness filter (DEC-051): it lifts the bass as the volume
/// drops below `reference` by lowering the mids, by up to `max_boost` dB at
/// 20 dB below. The Audio Player mirrors the volume into fader Aux1 for it and
/// makes the mid cut up with the amplifier's analog gain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    pub reference: f64,
    pub max_boost: f64,
}

impl Loudness {
    /// How much the mids are lowered at `db`, as CamillaDSP computes it.
    pub fn mid_cut(&self, db: f64) -> f64 {
        ((self.reference - db) / 20.0).clamp(0.0, 1.0) * self.max_boost
    }

    /// The loudness filter in a CamillaDSP config (GetConfigJson), if any.
    pub fn from_config(config: &Value) -> Option<Self> {
        config["filters"].as_object()?.values().find_map(|filter| {
            (filter["type"] == "Loudness" && filter["parameters"]["attenuate_mid"] == true).then(
                || {
                    let p = &filter["parameters"];
                    let boost = |key: &str| p[key].as_f64().unwrap_or(0.0);
                    Loudness {
                        reference: p["reference_level"].as_f64().unwrap_or(0.0),
                        max_boost: boost("low_boost").max(boost("high_boost")),
                    }
                },
            )
        })
    }
}

/// Where the bass fill curve is kept (DEC-053).
pub const BASS_FILE: &str = "/data/audio/bass.json";

/// How much the bass fill (CamillaDSP's `bass_boost`, and the bass
/// compressor's view of it, `boosted_view`) lifts the bass (DEC-053),
/// counted back from 100 %: `top_db` at the amplifier's full gain, the most
/// bass the amplifier and driver can play, and 1 dB more for each dB the gain
/// is lower, up to the box correction `max_db`. The bass (gain plus fill)
/// never plays louder than at 100 %, so no volume asks more of the system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BassFill {
    pub max_db: f64,
    pub top_db: f64,
}

impl Default for BassFill {
    /// The same fill at every volume: the config's static +6 dB.
    fn default() -> Self {
        BassFill {
            max_db: 6.0,
            top_db: 6.0,
        }
    }
}

impl BassFill {
    /// The fill in dB at `gain_db`, the amplifier's gain below full (0 or
    /// less; see [`analog_gain`]).
    pub fn at(&self, gain_db: f64) -> f64 {
        (self.top_db - gain_db.min(0.0)).min(self.max_db)
    }

    /// The gain in dB below full where the fill starts to fall.
    pub fn knee_db(&self) -> f64 {
        self.top_db - self.max_db
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, db) in [("max_db", self.max_db), ("top_db", self.top_db)] {
            if !(0.0..=24.0).contains(&db) {
                return Err(format!("{name} must be from 0 to 24 dB"));
            }
        }
        if self.top_db > self.max_db {
            return Err("top_db can't be more than max_db".into());
        }
        Ok(())
    }
}

/// The amplifier's gain in dB below full at `percent` (None for 0 %): the
/// volume plus the loudness filter's mid cut it makes up (DEC-050/051).
pub fn analog_gain(percent: u8, loudness: Option<Loudness>) -> Option<f64> {
    to_db(percent).map(|db| (db + loudness.map(|l| l.mid_cut(db)).unwrap_or(0.0)).min(0.0))
}

/// The bass fill curve, kept in a file, and the value last sent.
#[derive(Default)]
pub struct Bass {
    pub file: Option<PathBuf>,
    pub curve: std::sync::Mutex<BassFill>,
    pub sent: std::sync::Mutex<Option<f64>>,
}

impl Bass {
    /// The curve saved in `file`, or the default.
    pub fn load(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let curve = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<BassFill>(&b).ok())
            .filter(|c| c.validate().is_ok())
            .unwrap_or_default();
        Bass {
            file: Some(file),
            curve: std::sync::Mutex::new(curve),
            sent: Default::default(),
        }
    }

    pub fn curve(&self) -> BassFill {
        self.curve.lock().map(|c| *c).unwrap_or_default()
    }

    fn save(&self, curve: &BassFill) -> Result<(), String> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot save the bass fill: {e}"))?;
        }
        let temp = file.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(curve).map_err(|e| e.to_string())?;
        std::fs::write(&temp, json)
            .and_then(|()| std::fs::rename(&temp, file))
            .map_err(|e| format!("cannot save the bass fill: {e}"))
    }
}

/// CamillaDSP's fader that only the loudness filter reads.
pub const LOUDNESS_FADER: usize = 1;

/// The codec's line-out gain, set with amixer.
#[derive(Debug, Clone)]
pub struct Codec {
    pub program: String,
    pub card: String,
    pub control: String,
    /// The codec's soft mute, which ramps the DAC down and up without clicks.
    pub mute_control: String,
}

impl Codec {
    /// The IQaudIO Pi-Codec Zero (/etc/asound.conf, kds-hat-init).
    pub fn iqaudio() -> Self {
        Codec {
            program: "amixer".into(),
            card: "IQaudIOCODEC".into(),
            control: "Lineout Volume".into(),
            mute_control: "DAC Soft Mute Switch".into(),
        }
    }

    pub async fn set(&self, step: u8) -> Result<(), String> {
        self.cset(&self.control, &step.to_string()).await
    }

    /// Mutes or unmutes in hardware (0 %, DEC-050).
    pub async fn set_muted(&self, muted: bool) -> Result<(), String> {
        self.cset(&self.mute_control, if muted { "on" } else { "off" })
            .await
    }

    async fn cset(&self, control: &str, value: &str) -> Result<(), String> {
        let status = tokio::process::Command::new(&self.program)
            .args(["-q", "-c", &self.card, "cset"])
            .arg(format!("name={control}"))
            .arg(value)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map_err(|e| format!("{}: {e}", self.program))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "{} could not set {control}: {status}",
                self.program
            ))
        }
    }
}

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

    /// Sets CamillaDSP's volume: muted at 0 %, otherwise the level in dB.
    pub async fn apply(&self, percent: u8) -> Result<(), String> {
        self.apply_db(to_db(percent)).await
    }

    async fn apply_db(&self, db: Option<f64>) -> Result<(), String> {
        let commands = match db {
            None => vec![json!({"SetMute": true})],
            Some(db) => vec![
                json!({"SetVolume": (db * 10.0).round() / 10.0}),
                json!({"SetMute": false}),
            ],
        };
        self.send(&commands).await.map(|_| ())
    }

    /// Sets the bass fill on the running config (DEC-053): the fill itself
    /// and the bass compressor's view of it. A config without them (a
    /// listening variant) is left alone.
    pub async fn set_bass_fill(&self, db: f64) -> Result<(), String> {
        let db = (db * 10.0).round() / 10.0;
        let patch = json!({"PatchConfig": {"filters": {
            "bass_boost": {"parameters": {"gain": db}},
            "boosted_view": {"parameters": {"gain": db}},
        }}});
        self.send(&[patch]).await.map(|_| ())
    }

    /// The running config's loudness filter, if it has one.
    pub async fn loudness(&self) -> Result<Option<Loudness>, String> {
        let replies = self.send(&[json!("GetConfigJson")]).await?;
        let text = replies
            .first()
            .and_then(|r| r["GetConfigJson"]["value"].as_str())
            .ok_or("CamillaDSP sent no config")?;
        let config: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Ok(Loudness::from_config(&config))
    }
}

/// The volume as the IPC commands see it.
pub struct Volume {
    pub store: VolumeStore,
    pub camilla: Camilla,
    /// The amplifier's gain; None plays the volume through CamillaDSP.
    pub codec: Option<Codec>,
    /// The loudness filter, read from CamillaDSP once it answers.
    pub loudness: std::sync::Mutex<Option<Option<Loudness>>>,
    /// Whether the codec's soft mute is on, once set: it is only changed
    /// when that changes, so a volume step is a single amixer call.
    pub codec_muted: std::sync::Mutex<Option<bool>>,
    /// The bass fill that follows the volume (DEC-053).
    pub bass: Bass,
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
        Ok((settings, self.apply(settings.volume).await.err()))
    }

    /// The loudness filter, asking CamillaDSP the first time.
    async fn loudness(&self) -> Option<Loudness> {
        if let Some(known) = *self.loudness.lock().ok()? {
            return known;
        }
        let found = self.camilla.loudness().await.ok()?;
        if let Ok(mut cached) = self.loudness.lock() {
            *cached = Some(found);
        }
        found
    }

    /// Applies a volume: on the amplifier, with CamillaDSP at 0 dB, or on
    /// CamillaDSP when the codec can't be set. 0 % mutes CamillaDSP. The
    /// loudness filter's mid cut is made up with the same gain (DEC-051).
    pub async fn apply(&self, percent: u8) -> Result<(), String> {
        let loudness = self.loudness().await;
        let makeup = |db: f64| loudness.map(|l| l.mid_cut(db)).unwrap_or(0.0);
        let step = to_db(percent).map(|db| {
            (f64::from(LINEOUT_FULL) + db + makeup(db))
                .round()
                .clamp(0.0, f64::from(LINEOUT_MAX)) as u8
        });
        let on_codec = match (&self.codec, step) {
            (Some(codec), Some(step)) => match codec.set(step).await {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("audio-player: {e}; using CamillaDSP's volume");
                    false
                }
            },
            _ => false,
        };
        // 0 % is the codec's soft mute, which ramps without clicks;
        // CamillaDSP mutes only when the codec can't.
        let muted_on_codec = match &self.codec {
            Some(codec) if step.is_none() || on_codec => {
                let mute = step.is_none();
                let known = self.codec_muted.lock().ok().and_then(|m| *m);
                if known == Some(mute) {
                    true
                } else {
                    match codec.set_muted(mute).await {
                        Ok(()) => {
                            if let Ok(mut m) = self.codec_muted.lock() {
                                *m = Some(mute);
                            }
                            true
                        }
                        Err(e) => {
                            eprintln!("audio-player: {e}; muting with CamillaDSP");
                            false
                        }
                    }
                }
            }
            _ => false,
        };
        if let Some(db) = to_db(percent) {
            if loudness.is_some() {
                // Only the loudness filter reads this fader. "External" sets the
                // fader's current value too: no Volume filter ramps it.
                let fader =
                    json!({"SetFaderExternalVolume": [LOUDNESS_FADER, (db * 10.0).round() / 10.0]});
                self.camilla.send(&[fader]).await?;
            }
        }
        let db = match to_db(percent) {
            None if muted_on_codec => Some(0.0),
            None => None,
            Some(_) if on_codec => Some(0.0),
            Some(db) => Some((db + makeup(db)).min(0.0)),
        };
        self.camilla.apply_db(db).await?;
        if let Some(gain) = analog_gain(percent, loudness) {
            self.apply_bass_fill(gain).await;
        }
        Ok(())
    }

    /// Sends the bass fill for the amplifier's `gain` when it differs from
    /// the last sent.
    async fn apply_bass_fill(&self, gain: f64) {
        let fill = self.bass.curve().at(gain);
        let last = self.bass.sent.lock().ok().and_then(|s| *s);
        if last.is_some_and(|l| (l - fill).abs() < 0.05) {
            return;
        }
        match self.camilla.set_bass_fill(fill).await {
            Ok(()) => {
                if let Ok(mut sent) = self.bass.sent.lock() {
                    *sent = Some(fill);
                }
            }
            // A listening variant without the fill filters, or CamillaDSP down.
            Err(e) => eprintln!("audio-player: bass fill not applied: {e}"),
        }
    }

    /// The bass fill curve (DEC-053).
    pub fn bass_fill(&self) -> BassFill {
        self.bass.curve()
    }

    /// Changes the bass fill curve, saves it and applies it at the current
    /// volume.
    pub async fn set_bass_fill(&self, curve: BassFill) -> Result<BassFill, String> {
        curve.validate()?;
        self.bass.save(&curve)?;
        if let Ok(mut c) = self.bass.curve.lock() {
            *c = curve;
        }
        if let Ok(mut sent) = self.bass.sent.lock() {
            *sent = None;
        }
        let loudness = self.loudness().await;
        if let Some(gain) = analog_gain(self.current().volume, loudness) {
            self.apply_bass_fill(gain).await;
        }
        Ok(curve)
    }

    pub async fn set_limit(&self, limit: u8) -> Result<(Settings, Option<String>), String> {
        let settings = self.update(|s| s.set_limit(limit))?;
        Ok((settings, self.apply(settings.volume).await.err()))
    }

    /// Applies the saved volume, retrying until CamillaDSP answers.
    pub async fn apply_when_ready(&self) {
        let mut delay = Duration::from_secs(1);
        loop {
            match self.apply(self.current().volume).await {
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
