use audio_player::announcer::{
    ding, AnnounceError, AnnounceQueue, Announcement, Announcer, Chimes, Lang, Output, Sources,
    Voices, QUEUE_MAX,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use tts_engine::{Lookup, Synthesizer, TtsCache, TtsError, DEFAULT_TTL};

type Log = Arc<StdMutex<Vec<String>>>;

struct FakeOutput {
    log: Log,
    fail_on: Option<&'static str>,
}

impl Output for FakeOutput {
    async fn play(&self, path: &Path) -> Result<(), String> {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let label = if name.starts_with("chime-") {
            name
        } else {
            "speech".into()
        };
        self.log.lock().unwrap().push(format!("play {label}"));
        if self.fail_on.is_some_and(|f| label.contains(f)) {
            return Err("device busy".into());
        }
        Ok(())
    }
}

struct FakeSources {
    log: Log,
    running: Vec<String>,
}

impl Sources for FakeSources {
    async fn pause_all(&mut self) -> Vec<String> {
        self.log
            .lock()
            .unwrap()
            .push(format!("pause {:?}", self.running));
        self.running.clone()
    }
    async fn resume(&mut self, names: &[String]) {
        self.log.lock().unwrap().push(format!("resume {names:?}"));
    }
}

struct FakeSynth {
    log: Log,
    fail: bool,
}

impl Synthesizer for FakeSynth {
    fn id(&self) -> String {
        "fake".into()
    }
    fn synthesize(&self, text: &str, output: &Path) -> Result<(), TtsError> {
        self.log.lock().unwrap().push(format!("synthesize {text}"));
        if self.fail {
            return Err(TtsError::NoAudio("fake".into()));
        }
        std::fs::write(output, b"RIFF fake speech")?;
        Ok(())
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-announcer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn announcer(
    name: &str,
    running: &[&str],
    fail_on: Option<&'static str>,
    synth_fails: bool,
) -> (Announcer<FakeOutput, FakeSources>, Log) {
    let dir = temp_dir(name);
    let log: Log = Arc::default();
    let mut voices: Voices = Voices::new();
    voices.insert(
        Lang::Da,
        Arc::new(FakeSynth {
            log: Arc::clone(&log),
            fail: synth_fails,
        }),
    );
    let announcer = Announcer {
        output: FakeOutput {
            log: Arc::clone(&log),
            fail_on,
        },
        sources: Arc::new(Mutex::new(FakeSources {
            log: Arc::clone(&log),
            running: running.iter().map(|s| s.to_string()).collect(),
        })),
        cache: Arc::new(TtsCache::new(dir.join("cache"), DEFAULT_TTL).unwrap()),
        voices,
        chimes: Chimes::write(&dir.join("chimes")).unwrap(),
    };
    (announcer, log)
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

#[tokio::test]
async fn announcement_pauses_chimes_speaks_chimes_and_resumes() {
    let (a, log) = announcer("order", &["shairport-sync"], None, false);
    let lookup = a
        .speak(&Announcement::new("Aftensmaden er klar", None).unwrap())
        .await
        .unwrap();
    assert_eq!(lookup, Lookup::Miss);
    assert_eq!(
        entries(&log),
        vec![
            "synthesize Aftensmaden er klar",
            "pause [\"shairport-sync\"]",
            "play chime-in.wav",
            "play speech",
            "play chime-out.wav",
            "resume [\"shairport-sync\"]",
        ]
    );
}

#[tokio::test]
async fn repeated_announcement_comes_from_the_cache() {
    let (a, log) = announcer("cache", &[], None, false);
    let msg = Announcement::new("Godnat", None).unwrap();
    assert_eq!(a.speak(&msg).await.unwrap(), Lookup::Miss);
    assert_eq!(a.speak(&msg).await.unwrap(), Lookup::Hit);
    let synths = entries(&log)
        .iter()
        .filter(|e| e.starts_with("synthesize"))
        .count();
    assert_eq!(synths, 1);
}

#[tokio::test]
async fn sources_resume_even_when_playback_fails() {
    let (a, log) = announcer("fail", &["shairport-sync"], Some("speech"), false);
    let err = a
        .speak(&Announcement::new("Hej", None).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, AnnounceError::Playback(_)), "{err}");
    assert_eq!(entries(&log).last().unwrap(), "resume [\"shairport-sync\"]");
    assert!(!entries(&log).contains(&"play chime-out.wav".to_string()));
}

#[tokio::test]
async fn failed_synthesis_pauses_nothing() {
    let (a, log) = announcer("synthfail", &["shairport-sync"], None, true);
    let err = a
        .speak(&Announcement::new("Hej", None).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, AnnounceError::Synthesis(_)), "{err}");
    assert_eq!(entries(&log), vec!["synthesize Hej"]);
}

#[tokio::test]
async fn language_without_a_voice_is_an_error() {
    let (a, log) = announcer("novoice", &[], None, false);
    let err = a
        .speak(&Announcement::new("Hello", Some("en-GB")).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, AnnounceError::NoVoice(Lang::En)));
    assert!(entries(&log).is_empty());
}

#[test]
fn languages_parse_from_bcp47_tags() {
    assert_eq!(Lang::parse(None), Ok(Lang::Da));
    for tag in ["da", "da-DK", "DA_dk"] {
        assert_eq!(Lang::parse(Some(tag)), Ok(Lang::Da), "{tag}");
    }
    for tag in ["en", "en-GB", "en-US"] {
        assert_eq!(Lang::parse(Some(tag)), Ok(Lang::En), "{tag}");
    }
    assert!(Lang::parse(Some("de-DE"))
        .unwrap_err()
        .contains("not enabled"));
    assert_eq!(Lang::Da.espeak_voice(), "da");
    assert_eq!(Lang::En.espeak_voice(), "en");
}

#[test]
fn empty_text_is_rejected_and_text_is_trimmed() {
    assert!(Announcement::new("   ", None).is_err());
    assert_eq!(Announcement::new("  Hej  ", None).unwrap().text, "Hej");
}

#[tokio::test]
async fn queue_reports_positions_and_refuses_when_full() {
    let (queue, _rx) = AnnounceQueue::new();
    for expected in 1..=QUEUE_MAX {
        let pos = queue.submit(Announcement::new("x", None).unwrap()).unwrap();
        assert_eq!(pos, expected);
    }
    let err = queue
        .submit(Announcement::new("x", None).unwrap())
        .unwrap_err();
    assert!(err.contains("full"), "{err}");
}

#[tokio::test]
async fn closed_queue_reports_unavailable() {
    let (queue, rx) = AnnounceQueue::new();
    drop(rx);
    let err = queue
        .submit(Announcement::new("x", None).unwrap())
        .unwrap_err();
    assert!(err.contains("unavailable"), "{err}");
}

#[tokio::test]
async fn run_speaks_queued_announcements_in_order() {
    let (a, log) = announcer("run", &[], None, false);
    let (queue, rx) = AnnounceQueue::new();
    queue
        .submit(Announcement::new("en", None).unwrap())
        .unwrap();
    queue
        .submit(Announcement::new("to", None).unwrap())
        .unwrap();
    drop(queue);
    a.run(rx).await;
    let spoken: Vec<_> = entries(&log)
        .into_iter()
        .filter(|e| e.starts_with("synthesize"))
        .collect();
    assert_eq!(spoken, vec!["synthesize en", "synthesize to"]);
}

#[test]
fn chimes_are_valid_48k_mono_wavs() {
    let dir = temp_dir("chimes");
    let chimes = Chimes::write(&dir).unwrap();
    for path in [&chimes.enter, &chimes.exit] {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1, "mono");
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        let data_len = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 44 + data_len);
        // Short enough not to drag: well under a second.
        assert!(data_len / 2 < 48_000, "chime is {} samples", data_len / 2);
    }
}

#[test]
fn ding_does_not_clip_and_starts_softly() {
    let samples = ding(&[784.0, 1046.5]);
    let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
    assert!(peak < i16::MAX as u16, "clipped");
    assert!(peak > 3000, "too quiet: {peak}");
    assert!(samples[0].abs() < 100, "starts with a click");
    assert!(samples.last().unwrap().abs() < 300, "ends abruptly");
}
