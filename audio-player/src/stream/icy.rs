//! Shoutcast/Icecast stream titles (RAD_REQ_001). With `Icy-MetaData: 1` the
//! server puts a metadata block after every `icy-metaint` bytes of audio: one
//! length byte (×16), then text such as `StreamTitle='Go' Morgen P3';`. The
//! reader passes the audio through and keeps the latest title.

use std::io::{self, Read};
use std::sync::{Arc, Mutex};

pub type Title = Arc<Mutex<Option<String>>>;

pub struct IcyReader<R> {
    inner: R,
    metaint: usize,
    /// Audio bytes left before the next metadata block.
    left: usize,
    title: Title,
}

impl<R: Read> IcyReader<R> {
    pub fn new(inner: R, metaint: usize, title: Title) -> Self {
        IcyReader {
            inner,
            metaint,
            left: metaint,
            title,
        }
    }

    fn read_metadata(&mut self) -> io::Result<()> {
        let mut len = [0u8; 1];
        self.inner.read_exact(&mut len)?;
        let mut block = vec![0u8; len[0] as usize * 16];
        self.inner.read_exact(&mut block)?;
        if let Some(title) = stream_title(&block) {
            *self.title.lock().expect("icy title") = Some(title).filter(|t| !t.is_empty());
        }
        Ok(())
    }
}

impl<R: Read> Read for IcyReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            self.read_metadata()?;
            self.left = self.metaint;
        }
        let want = buf.len().min(self.left);
        let n = self.inner.read(&mut buf[..want])?;
        self.left -= n;
        Ok(n)
    }
}

/// The `StreamTitle` in a metadata block, if it has one. Stations send UTF-8 or
/// Latin-1; both are understood.
pub fn stream_title(block: &[u8]) -> Option<String> {
    let text = match std::str::from_utf8(block) {
        Ok(s) => s.to_string(),
        Err(_) => block.iter().map(|&b| b as char).collect(),
    };
    let text = text.trim_end_matches('\0');
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let end = rest.find("';").unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream with `metaint` audio bytes between metadata blocks.
    fn stream(chunks: &[(&[u8], &str)], metaint: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for (audio, meta) in chunks {
            assert_eq!(audio.len(), metaint);
            out.extend_from_slice(audio);
            let mut block = meta.as_bytes().to_vec();
            block.resize(block.len().div_ceil(16) * 16, 0);
            out.push((block.len() / 16) as u8);
            out.extend_from_slice(&block);
        }
        out
    }

    #[test]
    fn passes_the_audio_through_and_keeps_the_latest_title() {
        let data = stream(
            &[
                (b"abcd", "StreamTitle='Go'' Morgen P3';StreamUrl='';"),
                (b"efgh", ""),
                (b"ijkl", "StreamTitle='Nyheder';"),
            ],
            4,
        );
        let title: Title = Arc::default();
        let mut reader = IcyReader::new(&data[..], 4, Arc::clone(&title));
        let mut audio = Vec::new();
        let mut buf = [0u8; 3];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => audio.extend_from_slice(&buf[..n]),
            }
            if audio.len() == 8 {
                // An empty block keeps the title.
                assert_eq!(title.lock().unwrap().as_deref(), Some("Go'' Morgen P3"));
            }
        }
        assert_eq!(audio, b"abcdefghijkl");
        assert_eq!(title.lock().unwrap().as_deref(), Some("Nyheder"));
    }

    #[test]
    fn reads_latin1_and_ignores_blocks_without_a_title() {
        assert_eq!(
            stream_title(b"StreamTitle='B\xf8rneradio';\0\0").as_deref(),
            Some("Børneradio")
        );
        assert_eq!(stream_title(b"StreamUrl='x';"), None);
        assert_eq!(stream_title(b"StreamTitle='';").as_deref(), Some(""));
    }
}
