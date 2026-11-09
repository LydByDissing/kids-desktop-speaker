//! A cache of synthesised speech on the data partition (TTS_REQ_001). Entries are
//! keyed by engine, voice and text, refreshed on each use, and deleted 90 days
//! after their last use.

use crate::synth::{Synthesizer, TtsError};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Entries unused for this long are deleted.
pub const DEFAULT_TTL: Duration = Duration::from_secs(90 * 24 * 60 * 60);
/// Temporary files older than this were abandoned by a crash.
const STALE_TEMP: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// Served from the cache.
    Hit,
    /// Synthesised now, and cached.
    Miss,
}

pub struct TtsCache {
    dir: PathBuf,
    ttl: Duration,
}

/// Collapses runs of whitespace and trims, so trivially different spacing shares
/// an entry. Case is kept: it can change how text is spoken.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl TtsCache {
    pub fn new(dir: impl Into<PathBuf>, ttl: Duration) -> std::io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(TtsCache { dir, ttl })
    }

    /// The cache key for `text` spoken by the synthesizer identified by `synth_id`.
    pub fn key(synth_id: &str, text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(synth_id.as_bytes());
        hasher.update([0u8]);
        hasher.update(normalize(text).as_bytes());
        hex::encode(hasher.finalize())
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.wav"))
    }

    /// Returns a WAV file of `text` spoken by `synth`, from the cache when possible.
    pub fn get_or_synthesize(
        &self,
        synth: &dyn Synthesizer,
        text: &str,
    ) -> Result<(PathBuf, Lookup), TtsError> {
        let key = Self::key(&synth.id(), text);
        let path = self.path(&key);
        if fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            // Mark it as used now, which restarts its 90 days.
            fs::File::options()
                .write(true)
                .open(&path)?
                .set_modified(SystemTime::now())?;
            return Ok((path, Lookup::Hit));
        }

        let temp = self.dir.join(format!(".{key}.{}.tmp", std::process::id()));
        let result = synth.synthesize(&normalize(text), &temp).and_then(|()| {
            if fs::metadata(&temp).map(|m| m.len()).unwrap_or(0) == 0 {
                return Err(TtsError::NoAudio(synth.id()));
            }
            // A completed file appears under its final name in one step.
            fs::rename(&temp, &path)?;
            Ok(())
        });
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result.map(|()| (path, Lookup::Miss))
    }

    /// Deletes entries unused for longer than the TTL, and temporary files left by
    /// a crash. Returns how many files were deleted.
    pub fn evict_expired(&self, now: SystemTime) -> std::io::Result<usize> {
        let mut removed = 0;
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let limit = if name.ends_with(".wav") {
                self.ttl
            } else if name.ends_with(".tmp") {
                STALE_TEMP
            } else {
                continue;
            };
            let age = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok());
            if age.is_some_and(|age| age > limit) && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_ignores_spacing_but_not_case_or_voice() {
        let a = TtsCache::key("flite:slt", "Good  morning!\n");
        assert_eq!(a, TtsCache::key("flite:slt", " Good morning!"));
        assert_ne!(a, TtsCache::key("flite:slt", "good morning!"));
        assert_ne!(a, TtsCache::key("flite:awb", "Good morning!"));
    }

    #[test]
    fn key_separates_engine_id_from_text() {
        // Without a separator, "ab"+"c" and "a"+"bc" would collide.
        assert_ne!(TtsCache::key("ab", "c"), TtsCache::key("a", "bc"));
    }

    #[test]
    fn default_ttl_is_90_days() {
        assert_eq!(DEFAULT_TTL.as_secs(), 90 * 86_400);
    }
}
