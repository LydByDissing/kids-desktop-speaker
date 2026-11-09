use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};
use tts_engine::{Lookup, Synthesizer, TtsCache, TtsError, DEFAULT_TTL};

/// A stand-in engine that counts how often it synthesises.
struct Counting {
    voice: &'static str,
    calls: AtomicUsize,
    fail: bool,
    empty: bool,
}

impl Counting {
    fn new(voice: &'static str) -> Self {
        Counting {
            voice,
            calls: AtomicUsize::new(0),
            fail: false,
            empty: false,
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Synthesizer for Counting {
    fn id(&self) -> String {
        format!("test:{}", self.voice)
    }
    fn synthesize(&self, text: &str, output: &Path) -> Result<(), TtsError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            std::fs::write(output, b"partial")?;
            return Err(TtsError::Engine {
                engine: self.id(),
                detail: "boom".into(),
            });
        }
        let body: &[u8] = if self.empty { b"" } else { text.as_bytes() };
        std::fs::write(output, body)?;
        Ok(())
    }
}

fn cache(name: &str) -> TtsCache {
    let dir = std::env::temp_dir().join(format!("kds-tts-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    TtsCache::new(dir, DEFAULT_TTL).unwrap()
}

fn age(path: &PathBuf, days: u64) {
    let when = SystemTime::now() - Duration::from_secs(days * 86_400);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

fn files(cache: &TtsCache) -> usize {
    std::fs::read_dir(cache.dir()).unwrap().count()
}

#[test]
fn repeated_text_is_synthesised_once() {
    let cache = cache("repeat");
    let engine = Counting::new("a");
    let (first, l1) = cache.get_or_synthesize(&engine, "Time for bed").unwrap();
    let (second, l2) = cache.get_or_synthesize(&engine, "Time  for bed ").unwrap();
    assert_eq!((l1, l2), (Lookup::Miss, Lookup::Hit));
    assert_eq!(first, second);
    assert_eq!(engine.calls(), 1);
    assert_eq!(std::fs::read(&first).unwrap(), b"Time for bed");
}

#[test]
fn different_voices_have_separate_entries() {
    let cache = cache("voices");
    let (a, b) = (Counting::new("a"), Counting::new("b"));
    let (pa, _) = cache.get_or_synthesize(&a, "Hello").unwrap();
    let (pb, lb) = cache.get_or_synthesize(&b, "Hello").unwrap();
    assert_ne!(pa, pb);
    assert_eq!(lb, Lookup::Miss);
}

#[test]
fn a_hit_restarts_the_entry_ttl() {
    let cache = cache("touch");
    let engine = Counting::new("a");
    let (path, _) = cache.get_or_synthesize(&engine, "Dinner is ready").unwrap();
    age(&path, 89);
    cache.get_or_synthesize(&engine, "Dinner is ready").unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert!(SystemTime::now().duration_since(modified).unwrap() < Duration::from_secs(60));
    // 89 days later than the original use, but just used: it must survive.
    assert_eq!(cache.evict_expired(SystemTime::now()).unwrap(), 0);
    assert!(path.exists());
}

#[test]
fn entries_unused_for_90_days_are_removed() {
    let cache = cache("expire");
    let engine = Counting::new("a");
    let (old, _) = cache.get_or_synthesize(&engine, "Old message").unwrap();
    let (recent, _) = cache.get_or_synthesize(&engine, "Recent message").unwrap();
    age(&old, 91);
    age(&recent, 89);
    assert_eq!(cache.evict_expired(SystemTime::now()).unwrap(), 1);
    assert!(!old.exists());
    assert!(recent.exists());
    // An expired entry is synthesised again on its next use.
    let (_, lookup) = cache.get_or_synthesize(&engine, "Old message").unwrap();
    assert_eq!(lookup, Lookup::Miss);
}

#[test]
fn a_failed_synthesis_leaves_nothing_behind() {
    let cache = cache("fail");
    let mut engine = Counting::new("a");
    engine.fail = true;
    assert!(cache.get_or_synthesize(&engine, "Will fail").is_err());
    assert_eq!(files(&cache), 0);
    // Not cached as a success: the next attempt synthesises again.
    assert!(cache.get_or_synthesize(&engine, "Will fail").is_err());
    assert_eq!(engine.calls(), 2);
}

#[test]
fn empty_audio_is_an_error_and_not_cached() {
    let cache = cache("empty");
    let mut engine = Counting::new("a");
    engine.empty = true;
    assert!(matches!(
        cache.get_or_synthesize(&engine, "Silent"),
        Err(TtsError::NoAudio(_))
    ));
    assert_eq!(files(&cache), 0);
}

#[test]
fn stale_temporary_files_are_removed() {
    let cache = cache("stale");
    let stale = cache.dir().join(".abc.123.tmp");
    let fresh = cache.dir().join(".def.123.tmp");
    std::fs::write(&stale, b"x").unwrap();
    std::fs::write(&fresh, b"x").unwrap();
    age(&stale, 2);
    assert_eq!(cache.evict_expired(SystemTime::now()).unwrap(), 1);
    assert!(!stale.exists());
    assert!(
        fresh.exists(),
        "a synthesis may still be writing a recent temporary file"
    );
}
