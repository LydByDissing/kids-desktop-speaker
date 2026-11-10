//! Announcements (TTS_REQ_002, TTS_REQ_003, DEC-033): synthesise the speech, pause
//! the running sources, play an "in" chime, the speech and an "out" chime, then
//! resume the sources.

use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};
use tts_engine::{Engine, Lookup, Synthesizer, TtsCache, TtsError};

/// At most this many announcements wait to be spoken.
pub const QUEUE_MAX: usize = 10;

/// The languages the speaker can speak (TTS_REQ_003). Danish is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Da,
    En,
}

impl Lang {
    /// Parses a BCP 47 tag such as `da-DK` or `en-GB`. No tag means Danish.
    pub fn parse(tag: Option<&str>) -> Result<Lang, String> {
        let Some(tag) = tag else { return Ok(Lang::Da) };
        let primary = tag
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match primary.as_str() {
            "da" => Ok(Lang::Da),
            "en" => Ok(Lang::En),
            _ => Err(format!("language not enabled: {tag}")),
        }
    }

    /// The espeak-ng voice for this language (DEC-030).
    pub fn espeak_voice(self) -> &'static str {
        match self {
            Lang::Da => "da",
            Lang::En => "en",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub text: String,
    pub lang: Lang,
}

impl Announcement {
    pub fn new(text: &str, lang: Option<&str>) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("announce needs text".into());
        }
        Ok(Announcement {
            text: text.to_string(),
            lang: Lang::parse(lang)?,
        })
    }
}

#[derive(Debug, Error)]
pub enum AnnounceError {
    #[error("no voice for {0:?}")]
    NoVoice(Lang),
    #[error("synthesis failed: {0}")]
    Synthesis(#[from] TtsError),
    #[error("synthesis task failed: {0}")]
    Task(String),
    #[error("playback failed: {0}")]
    Playback(String),
}

/// Plays a WAV file to the end.
pub trait Output: Send + Sync {
    fn play(&self, path: &Path) -> impl Future<Output = Result<(), String>> + Send;
}

/// Plays through an ALSA device with `aplay`.
pub struct Aplay {
    pub device: String,
}

impl Output for Aplay {
    async fn play(&self, path: &Path) -> Result<(), String> {
        let out = tokio::process::Command::new("aplay")
            .args(["-q", "-D", &self.device])
            .arg(path)
            .output()
            .await
            .map_err(|e| format!("aplay: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "aplay {}: {}",
                path.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }
}

/// The sources an announcement pauses.
pub trait Sources: Send {
    /// Pauses every running source and returns their names.
    fn pause_all(&mut self) -> impl Future<Output = Vec<String>> + Send;
    /// Resumes the named sources.
    fn resume(&mut self, names: &[String]) -> impl Future<Output = ()> + Send;
}

/// The "in" and "out" chimes, written once as WAV files.
#[derive(Debug, Clone)]
pub struct Chimes {
    pub enter: PathBuf,
    pub exit: PathBuf,
}

impl Chimes {
    /// Writes the chimes into `dir`: a rising two-note ding before the
    /// announcement and a falling one after it.
    pub fn write(dir: &Path) -> std::io::Result<Chimes> {
        std::fs::create_dir_all(dir)?;
        let chimes = Chimes {
            enter: dir.join("chime-in.wav"),
            exit: dir.join("chime-out.wav"),
        };
        write_wav(&chimes.enter, &ding(&[784.0, 1046.5]))?;
        write_wav(&chimes.exit, &ding(&[1046.5, 784.0]))?;
        Ok(chimes)
    }
}

pub const CHIME_RATE: u32 = 48_000;

/// Two soft bell-like notes: each starts at once and decays exponentially.
pub fn ding(notes: &[f32]) -> Vec<i16> {
    let note_len = (CHIME_RATE as f32 * 0.18) as usize;
    let tail = (CHIME_RATE as f32 * 0.25) as usize;
    let total = note_len * notes.len() + tail;
    let mut samples = vec![0f32; total];
    for (n, freq) in notes.iter().enumerate() {
        let start = n * note_len;
        for (i, s) in samples[start..].iter_mut().enumerate() {
            let t = i as f32 / CHIME_RATE as f32;
            let attack = (t / 0.005).min(1.0);
            let env = attack * (-t * 9.0).exp();
            let tone = (2.0 * std::f32::consts::PI * freq * t).sin()
                + 0.3 * (4.0 * std::f32::consts::PI * freq * t).sin();
            *s += 0.3 * env * tone;
        }
    }
    samples
        .iter()
        .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

/// Writes 16-bit mono PCM at [`CHIME_RATE`] as a WAV file.
pub fn write_wav(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&CHIME_RATE.to_le_bytes())?;
    f.write_all(&(CHIME_RATE * 2).to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    f.flush()
}

pub type Voices = HashMap<Lang, Arc<dyn Synthesizer>>;

/// The espeak-ng voices for Danish and English (DEC-030).
pub fn espeak_voices() -> Voices {
    let mut voices: Voices = HashMap::new();
    for lang in [Lang::Da, Lang::En] {
        voices.insert(
            lang,
            Arc::new(Engine::EspeakNg {
                voice: lang.espeak_voice().into(),
            }),
        );
    }
    voices
}

pub struct Announcer<O: Output, S: Sources> {
    pub output: O,
    pub sources: Arc<Mutex<S>>,
    pub cache: Arc<TtsCache>,
    pub voices: Voices,
    pub chimes: Chimes,
}

impl<O: Output, S: Sources> Announcer<O, S> {
    /// Speaks one announcement. The speech is synthesised before anything is
    /// paused, and the paused sources are resumed even if playback fails.
    pub async fn speak(&self, announcement: &Announcement) -> Result<Lookup, AnnounceError> {
        let synth = self
            .voices
            .get(&announcement.lang)
            .cloned()
            .ok_or(AnnounceError::NoVoice(announcement.lang))?;
        let cache = Arc::clone(&self.cache);
        let text = announcement.text.clone();
        let (speech, lookup) =
            tokio::task::spawn_blocking(move || cache.get_or_synthesize(&*synth, &text))
                .await
                .map_err(|e| AnnounceError::Task(e.to_string()))??;

        self.between_chimes(&speech).await?;
        Ok(lookup)
    }

    /// Plays a recorded voice message the same way (MSG_REQ_001, DEC-037).
    pub async fn play_recording(&self, path: &Path) -> Result<(), AnnounceError> {
        if !path.is_file() {
            return Err(AnnounceError::Playback(format!(
                "{} is missing",
                path.display()
            )));
        }
        self.between_chimes(path).await
    }

    /// Pauses the sources, plays the in chime, `clip` and the out chime, and
    /// resumes the sources even if playback fails.
    async fn between_chimes(&self, clip: &Path) -> Result<(), AnnounceError> {
        let mut sources = self.sources.lock().await;
        let paused = sources.pause_all().await;
        let played = async {
            self.output.play(&self.chimes.enter).await?;
            self.output.play(clip).await?;
            self.output.play(&self.chimes.exit).await
        }
        .await;
        sources.resume(&paused).await;
        played.map_err(AnnounceError::Playback)
    }

    /// Plays queued announcements and messages one at a time until the queue
    /// closes.
    pub async fn run(self, mut queue: mpsc::Receiver<Item>) {
        while let Some(item) = queue.recv().await {
            match item {
                Item::Speech(announcement) => match self.speak(&announcement).await {
                    Ok(lookup) => println!(
                        "audio-player: announced ({:?}, {} chars, cache {:?})",
                        announcement.lang,
                        announcement.text.chars().count(),
                        lookup
                    ),
                    Err(e) => eprintln!("audio-player: announcement failed: {e}"),
                },
                Item::Recording(path) => match self.play_recording(&path).await {
                    Ok(()) => println!("audio-player: played message {}", path.display()),
                    Err(e) => eprintln!("audio-player: message failed: {e}"),
                },
            }
        }
    }
}

/// What the announcement queue carries: speech to synthesise, or a recorded
/// voice message to play (DEC-037).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Speech(Announcement),
    Recording(PathBuf),
}

/// The sending side of the announcement queue, used by the IPC server.
#[derive(Clone)]
pub struct AnnounceQueue {
    tx: mpsc::Sender<Item>,
}

impl AnnounceQueue {
    pub fn new() -> (AnnounceQueue, mpsc::Receiver<Item>) {
        let (tx, rx) = mpsc::channel(QUEUE_MAX);
        (AnnounceQueue { tx }, rx)
    }

    /// Queues an announcement and returns its position (1 = next).
    pub fn submit(&self, announcement: Announcement) -> Result<usize, String> {
        self.submit_item(Item::Speech(announcement))
    }

    /// Queues a recorded voice message and returns its position.
    pub fn submit_recording(&self, path: PathBuf) -> Result<usize, String> {
        self.submit_item(Item::Recording(path))
    }

    fn submit_item(&self, item: Item) -> Result<usize, String> {
        self.tx.try_send(item).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => format!("announcement queue full ({QUEUE_MAX})"),
            mpsc::error::TrySendError::Closed(_) => "announcements unavailable".to_string(),
        })?;
        Ok(QUEUE_MAX - self.tx.capacity())
    }
}
