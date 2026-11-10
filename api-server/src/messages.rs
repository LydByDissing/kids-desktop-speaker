//! Recorded voice messages (MSG_REQ_001-003, DEC-037): stored on the data
//! partition by the API Server, played by the Audio Player.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MESSAGES_DIR: &str = "/data/messages";
/// The longest message, plus a little slack for the encoder (MSG_REQ_001).
pub const MAX_SECONDS: f64 = 30.5;
/// The largest upload: 30 s of 48 kHz 16-bit stereo, plus headers.
pub const MAX_BYTES: usize = 6 * 1024 * 1024;
/// Unnamed messages expire this long after they were last played (MSG_REQ_003).
pub const UNNAMED_TTL: Duration = Duration::from_secs(90 * 24 * 3600);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    /// Seconds since the Unix epoch.
    pub recorded: u64,
    pub name: Option<String>,
    pub duration_s: f64,
    /// Seconds since the Unix epoch; the recording time until it is replayed.
    pub last_played: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WavInfo {
    pub rate: u32,
    pub channels: u16,
    pub duration_s: f64,
}

/// Checks that `bytes` is a PCM WAV the speaker accepts: 16-bit, mono or
/// stereo, 8-48 kHz, at most [`MAX_SECONDS`] long.
pub fn wav_info(bytes: &[u8]) -> Result<WavInfo, String> {
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let (mut fmt, mut data_len) = (None, None);
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32_at(pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            if len < 16 || body + 16 > bytes.len() {
                return Err("damaged WAV format chunk".into());
            }
            fmt = Some((
                u16_at(body),
                u16_at(body + 2),
                u32_at(body + 4),
                u16_at(body + 14),
            ));
        } else if id == b"data" {
            // Recorders that stream may leave the length unset; use what is there.
            data_len = Some(len.min(bytes.len() - body));
            break;
        }
        pos = body + len + (len & 1);
    }
    let (format, channels, rate, bits) = fmt.ok_or("WAV has no format chunk")?;
    let data_len = data_len.ok_or("WAV has no audio data")?;
    if format != 1 || bits != 16 {
        return Err("WAV must be 16-bit PCM".into());
    }
    if !(1..=2).contains(&channels) {
        return Err("WAV must be mono or stereo".into());
    }
    if !(8_000..=48_000).contains(&rate) {
        return Err("WAV sample rate must be 8-48 kHz".into());
    }
    let duration_s = data_len as f64 / (rate as f64 * channels as f64 * 2.0);
    if duration_s <= 0.0 {
        return Err("WAV is empty".into());
    }
    if duration_s > MAX_SECONDS {
        return Err(format!("message is {duration_s:.1} s; the limit is 30 s"));
    }
    Ok(WavInfo {
        rate,
        channels,
        duration_s,
    })
}

fn now_secs(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn clean_name(name: Option<&str>) -> Result<Option<String>, String> {
    match name.map(str::trim) {
        None | Some("") => Ok(None),
        Some(n) if n.chars().count() > 100 => Err("name must be at most 100 characters".into()),
        Some(n) if n.chars().any(char::is_control) => {
            Err("name must not contain control characters".into())
        }
        Some(n) => Ok(Some(n.to_string())),
    }
}

/// A message id: when it was recorded (sortable) and 4 random bytes. Only
/// letters, digits and dashes, as the Audio Player requires.
fn new_id(now: SystemTime) -> String {
    use std::io::Read;
    let mut random = [0u8; 4];
    let filled = fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut random));
    if filled.is_err() {
        random = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
            .to_le_bytes();
    }
    let hex: String = random.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{hex}", now_secs(now))
}

pub struct MessageStore {
    dir: PathBuf,
}

impl MessageStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        MessageStore { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 64
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn audio_path(&self, id: &str) -> Result<PathBuf, String> {
        if !Self::valid_id(id) {
            return Err(format!("no message {id}"));
        }
        let path = self.dir.join(format!("{id}.wav"));
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!("no message {id}"))
        }
    }

    fn write_meta(&self, message: &Message) -> Result<(), String> {
        let path = self.meta_path(&message.id);
        let temp = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(message).map_err(|e| e.to_string())?;
        fs::write(&temp, json)
            .and_then(|()| fs::rename(&temp, &path))
            .map_err(|e| format!("cannot save message: {e}"))
    }

    /// Stores a validated recording and returns its metadata.
    pub fn save(&self, wav: &[u8], name: Option<&str>, now: SystemTime) -> Result<Message, String> {
        let info = wav_info(wav)?;
        let name = clean_name(name)?;
        fs::create_dir_all(&self.dir).map_err(|e| format!("cannot save message: {e}"))?;
        let id = new_id(now);
        let audio = self.dir.join(format!("{id}.wav"));
        let temp = audio.with_extension("wav.tmp");
        fs::write(&temp, wav)
            .and_then(|()| fs::rename(&temp, &audio))
            .map_err(|e| format!("cannot save message: {e}"))?;
        let message = Message {
            id,
            recorded: now_secs(now),
            name,
            duration_s: (info.duration_s * 10.0).round() / 10.0,
            last_played: now_secs(now),
        };
        if let Err(e) = self.write_meta(&message) {
            let _ = fs::remove_file(&audio);
            return Err(e);
        }
        Ok(message)
    }

    pub fn get(&self, id: &str) -> Result<Message, String> {
        self.audio_path(id)?;
        let bytes = fs::read(self.meta_path(id)).map_err(|_| format!("no message {id}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("message {id} is damaged: {e}"))
    }

    /// Every message, newest first. Damaged entries are skipped.
    pub fn list(&self) -> Vec<Message> {
        let mut messages: Vec<Message> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let id = name.strip_suffix(".json")?;
                self.get(id).ok()
            })
            .collect();
        messages.sort_by(|a, b| b.recorded.cmp(&a.recorded).then(b.id.cmp(&a.id)));
        messages
    }

    /// Names or renames a message; an empty name removes it (MSG_REQ_002).
    pub fn rename(&self, id: &str, name: Option<&str>) -> Result<Message, String> {
        let mut message = self.get(id)?;
        message.name = clean_name(name)?;
        self.write_meta(&message)?;
        Ok(message)
    }

    pub fn mark_played(&self, id: &str, now: SystemTime) -> Result<(), String> {
        let mut message = self.get(id)?;
        message.last_played = now_secs(now);
        self.write_meta(&message)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let audio = self.audio_path(id)?;
        fs::remove_file(audio).map_err(|e| format!("cannot delete message: {e}"))?;
        let _ = fs::remove_file(self.meta_path(id));
        Ok(())
    }

    /// Removes unnamed messages not played for [`UNNAMED_TTL`] (MSG_REQ_003),
    /// and files left behind by an interrupted save (over an hour old, so a save
    /// in progress is never touched). Returns how many messages were removed.
    pub fn evict(&self, now: SystemTime) -> usize {
        let cutoff = now_secs(now).saturating_sub(UNNAMED_TTL.as_secs());
        let mut removed = 0;
        for message in self.list() {
            if message.name.is_none()
                && message.last_played < cutoff
                && self.delete(&message.id).is_ok()
            {
                removed += 1;
            }
        }
        for entry in fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let orphan_audio = name
                .strip_suffix(".wav")
                .is_some_and(|id| !self.meta_path(id).exists());
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| now.duration_since(t).unwrap_or_default() > Duration::from_secs(3600))
                .unwrap_or(false);
            if old && (name.ends_with(".tmp") || orphan_audio) {
                let _ = fs::remove_file(entry.path());
            }
        }
        removed
    }
}
