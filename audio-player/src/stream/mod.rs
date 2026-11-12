//! The radio and podcast player (RAD_REQ_001, POD_REQ_001, DEC-048), in place
//! of MPD. Each playback runs on its own thread: HTTP in, symphonia decodes
//! (MP3, AAC including .m4a, Ogg Vorbis, FLAC, WAV), aplay out. A live stream
//! that drops reconnects up to [`RECONNECTS`] times; a file ends at its end.

pub mod http;
pub mod icy;
pub mod sink;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use thiserror::Error;

use http::HttpSource;
use icy::Title;
use sink::{AplayFactory, SinkFactory};

/// How long `play_url` waits for a stream to connect and be recognised.
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(20);
/// A live stream that drops is reconnected this many times.
pub const RECONNECTS: u32 = 3;

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("could not play the stream: {0}")]
    Open(String),
    #[error("nothing is playing")]
    NotPlaying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Play,
    Pause,
    Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub state: PlayState,
    /// What the stream reports as playing (ICY title), if anything.
    pub title: Option<String>,
    /// Seconds into the current track, and its length when known (podcasts).
    pub elapsed: Option<f64>,
    pub duration: Option<f64>,
}

impl Status {
    fn stopped() -> Self {
        Status {
            state: PlayState::Stop,
            title: None,
            elapsed: None,
            duration: None,
        }
    }
}

/// One playback, shared between the player and its thread.
struct Session {
    stop: AtomicBool,
    paused: AtomicBool,
    seek_to: Mutex<Option<f64>>,
    status: Mutex<Status>,
    title: Title,
}

impl Session {
    fn set(&self, f: impl FnOnce(&mut Status)) {
        f(&mut self.status.lock().expect("player status"));
    }
}

#[derive(Clone)]
pub struct Player {
    sinks: Arc<dyn SinkFactory>,
    session: Arc<Mutex<Option<Arc<Session>>>>,
}

impl Player {
    pub fn new(sinks: Arc<dyn SinkFactory>) -> Self {
        Player {
            sinks,
            session: Arc::default(),
        }
    }

    /// Plays through `aplay -D <device>`.
    pub fn aplay(device: &str) -> Self {
        Self::new(Arc::new(AplayFactory(device.to_string())))
    }

    fn current(&self) -> Option<Arc<Session>> {
        self.session.lock().expect("player session").clone()
    }

    /// Plays `url`, replacing what plays. Returns once the stream is connected
    /// and recognised, or with the reason it can't be played.
    pub async fn play_url(&self, url: &str) -> Result<(), PlayerError> {
        self.stop_session();
        let session = Arc::new(Session {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            seek_to: Mutex::new(None),
            status: Mutex::new(Status {
                state: PlayState::Play,
                ..Status::stopped()
            }),
            title: Arc::default(),
        });
        *self.session.lock().expect("player session") = Some(Arc::clone(&session));
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let sinks = Arc::clone(&self.sinks);
        let url = url.to_string();
        let thread_session = Arc::clone(&session);
        std::thread::Builder::new()
            .name("kds-player".into())
            .spawn(move || run(&thread_session, &url, &*sinks, ready_tx))
            .map_err(|e| PlayerError::Open(e.to_string()))?;
        let opened = match tokio::time::timeout(OPEN_TIMEOUT, ready_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("the player stopped".into()),
            Err(_) => Err("timed out connecting".into()),
        };
        if let Err(e) = opened {
            session.stop.store(true, Ordering::Relaxed);
            session.set(|s| *s = Status::stopped());
            return Err(PlayerError::Open(e));
        }
        Ok(())
    }

    /// Jumps to `seconds` into a file (podcasts); live streams ignore it.
    pub async fn seek(&self, seconds: f64) -> Result<(), PlayerError> {
        let session = self.current().ok_or(PlayerError::NotPlaying)?;
        *session.seek_to.lock().expect("player seek") = Some(seconds.max(0.0));
        session.set(|s| s.elapsed = Some(seconds.max(0.0)));
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), PlayerError> {
        self.stop_session();
        Ok(())
    }

    fn stop_session(&self) {
        if let Some(session) = self.session.lock().expect("player session").take() {
            session.stop.store(true, Ordering::Relaxed);
        }
    }

    pub async fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        let session = self.current().ok_or(PlayerError::NotPlaying)?;
        session.paused.store(paused, Ordering::Relaxed);
        session.set(|s| {
            if s.state != PlayState::Stop {
                s.state = if paused {
                    PlayState::Pause
                } else {
                    PlayState::Play
                };
            }
        });
        Ok(())
    }

    pub async fn status(&self) -> Result<Status, PlayerError> {
        Ok(self
            .current()
            .map(|s| s.status.lock().expect("player status").clone())
            .unwrap_or_else(Status::stopped))
    }
}

/// How one connection ended.
enum Ended {
    Stopped,
    /// A file played to its end.
    Finished,
    Failed(String),
}

type Ready = Option<tokio::sync::oneshot::Sender<Result<(), String>>>;

fn run(
    session: &Session,
    url: &str,
    sinks: &dyn SinkFactory,
    ready_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    let mut ready: Ready = Some(ready_tx);
    // HTTP runs on this thread's own runtime, entered for the whole playback so
    // connections are made and dropped inside it.
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            if let Some(tx) = ready.take() {
                let _ = tx.send(Err(e.to_string()));
            }
            return;
        }
    };
    let _context = rt.enter();
    let mut reconnects = 0;
    let mut live = false;
    loop {
        match play_once(session, url, sinks, &rt, &mut ready, &mut live) {
            Ended::Stopped | Ended::Finished => break,
            Ended::Failed(e) => {
                if let Some(tx) = ready.take() {
                    let _ = tx.send(Err(e));
                    break;
                }
                if !live || reconnects >= RECONNECTS {
                    eprintln!("audio-player: stream ended: {e}");
                    break;
                }
                reconnects += 1;
                eprintln!(
                    "audio-player: stream dropped ({e}), reconnecting ({reconnects}/{RECONNECTS})"
                );
                // 1, 2, 4 s, waking early for a stop.
                for _ in 0..(10 << (reconnects - 1)) {
                    if session.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    if !session.stop.load(Ordering::Relaxed) {
        session.set(|s| s.state = PlayState::Stop);
    }
}

/// A hint for the format from the content type or the URL's extension.
fn hint(url: &str, content_type: Option<&str>) -> Hint {
    let mut hint = Hint::new();
    let by_type = match content_type.unwrap_or("") {
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/aac" | "audio/aacp" | "audio/x-aac" => Some("aac"),
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => Some("m4a"),
        "audio/ogg" | "application/ogg" | "audio/vorbis" => Some("ogg"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        _ => None,
    };
    if let Some(t) = content_type.filter(|t| !t.is_empty()) {
        hint.mime_type(t);
    }
    let path = url.split(['?', '#']).next().unwrap_or("");
    let by_url = path
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|e| e.len() <= 4);
    if let Some(ext) = by_type.or(by_url) {
        hint.with_extension(ext);
    }
    hint
}

fn play_once(
    session: &Session,
    url: &str,
    sinks: &dyn SinkFactory,
    rt: &Arc<tokio::runtime::Runtime>,
    ready: &mut Ready,
    live: &mut bool,
) -> Ended {
    let source = match HttpSource::open(url, Arc::clone(&session.title), Arc::clone(rt)) {
        Ok(s) => s,
        Err(e) => return Ended::Failed(e),
    };
    let hint = hint(url, source.content_type());
    let seekable = symphonia::core::io::MediaSource::is_seekable(&source);
    *live = !seekable;
    let stream = MediaSourceStream::new(Box::new(source), Default::default());
    let probed = match symphonia::default::get_probe().format(
        &hint,
        stream,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) {
        Ok(p) => p,
        Err(e) => return Ended::Failed(format!("unsupported stream: {e}")),
    };
    let mut format = probed.format;
    let Some(track) = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    else {
        return Ended::Failed("no audio in the stream".into());
    };
    let track_id = track.id;
    let params = track.codec_params.clone();
    let mut decoder =
        match symphonia::default::get_codecs().make(&params, &DecoderOptions::default()) {
            Ok(d) => d,
            Err(e) => return Ended::Failed(format!("unsupported audio: {e}")),
        };
    let time_base = params.time_base;
    let seconds = |ts: u64| {
        time_base.map(|tb| {
            let t = tb.calc_time(ts);
            t.seconds as f64 + t.frac
        })
    };
    // A live stream may carry a file's header (frame counts) but has no end.
    let duration = params.n_frames.and_then(seconds).filter(|_| seekable);
    session.set(|s| s.duration = duration);
    if let Some(tx) = ready.take() {
        let _ = tx.send(Ok(()));
    }

    let mut sink = sinks.make();
    let mut samples: Option<SampleBuffer<i16>> = None;
    let ended = loop {
        if session.stop.load(Ordering::Relaxed) {
            break Ended::Stopped;
        }
        if let Some(to) = session.seek_to.lock().expect("player seek").take() {
            if seekable {
                let target = SeekTo::Time {
                    time: Time::from(to),
                    track_id: Some(track_id),
                };
                match format.seek(SeekMode::Coarse, target) {
                    Ok(_) => decoder.reset(),
                    Err(e) => eprintln!("audio-player: seek to {to:.0} s failed: {e}"),
                }
            }
        }
        if session.paused.load(Ordering::Relaxed) {
            sink.close();
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break if seekable {
                    Ended::Finished
                } else {
                    Ended::Failed("the stream ended".into())
                };
            }
            Err(SymError::ResetRequired) => {
                match symphonia::default::get_codecs().make(&params, &DecoderOptions::default()) {
                    Ok(d) => decoder = d,
                    Err(e) => break Ended::Failed(e.to_string()),
                }
                continue;
            }
            Err(e) => break Ended::Failed(e.to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged frame is skipped, as players do.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => break Ended::Failed(e.to_string()),
        };
        let spec = *decoded.spec();
        let buffer = match &mut samples {
            Some(b) if b.capacity() >= decoded.capacity() * spec.channels.count() => b,
            _ => samples.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
        };
        buffer.copy_interleaved_ref(decoded);
        if let Err(e) = sink
            .open(spec.rate, spec.channels.count())
            .and_then(|()| sink.write(buffer.samples()))
        {
            break Ended::Failed(format!("audio output: {e}"));
        }
        let elapsed = seconds(packet.ts());
        let title = session.title.lock().expect("icy title").clone();
        session.set(|s| {
            if !session.stop.load(Ordering::Relaxed) {
                s.elapsed = elapsed;
                s.title = title;
            }
        });
    };
    sink.close();
    ended
}
