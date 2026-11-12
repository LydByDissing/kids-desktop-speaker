//! Where decoded audio goes: `aplay` on the shared "speaker" device, the same
//! way announcements play (DEC-033, DEC-048). The device converts any rate for
//! the HAT, and aplay blocks while its buffer is full, which paces decoding.

use std::io::{self, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Plays interleaved 16-bit samples.
pub trait Sink: Send {
    /// Prepares for `rate` Hz and `channels`; a change reopens the output.
    fn open(&mut self, rate: u32, channels: usize) -> io::Result<()>;
    fn write(&mut self, samples: &[i16]) -> io::Result<()>;
    /// Releases the output (on pause and stop).
    fn close(&mut self);
}

/// Makes a sink for each playback.
pub trait SinkFactory: Send + Sync {
    fn make(&self) -> Box<dyn Sink>;
}

/// `aplay -D <device>` reading raw samples from a pipe.
pub struct Aplay {
    device: String,
    child: Option<Child>,
    format: (u32, usize),
}

impl Aplay {
    pub fn new(device: &str) -> Self {
        Aplay {
            device: device.to_string(),
            child: None,
            format: (0, 0),
        }
    }
}

impl Sink for Aplay {
    fn open(&mut self, rate: u32, channels: usize) -> io::Result<()> {
        if self.child.is_some() && self.format == (rate, channels) {
            return Ok(());
        }
        self.close();
        let child = Command::new("aplay")
            .args(["-q", "-D", &self.device, "-t", "raw", "-f", "S16_LE"])
            .args(["-r", &rate.to_string(), "-c", &channels.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()?;
        self.child = Some(child);
        self.format = (rate, channels);
        Ok(())
    }

    fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let stdin = self
            .child
            .as_mut()
            .and_then(|c| c.stdin.as_mut())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "aplay is not open"))?;
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        stdin.write_all(&bytes)
    }

    fn close(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Aplay {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct AplayFactory(pub String);

impl SinkFactory for AplayFactory {
    fn make(&self) -> Box<dyn Sink> {
        Box::new(Aplay::new(&self.0))
    }
}

/// For tests: counts frames and keeps time like a sound card, `speed` times
/// faster than real time.
#[derive(Clone)]
pub struct PacedSink {
    pub frames: Arc<AtomicU64>,
    pub opened: Arc<AtomicU64>,
    speed: f64,
    format: (u32, usize),
}

impl PacedSink {
    pub fn new(speed: f64) -> Self {
        PacedSink {
            frames: Arc::default(),
            opened: Arc::default(),
            speed,
            format: (0, 0),
        }
    }
}

impl Sink for PacedSink {
    fn open(&mut self, rate: u32, channels: usize) -> io::Result<()> {
        if self.format != (rate, channels) {
            self.opened.fetch_add(1, Ordering::Relaxed);
        }
        self.format = (rate, channels);
        Ok(())
    }

    fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let (rate, channels) = self.format;
        let frames = (samples.len() / channels.max(1)) as u64;
        self.frames.fetch_add(frames, Ordering::Relaxed);
        let seconds = frames as f64 / rate.max(1) as f64 / self.speed;
        std::thread::sleep(Duration::from_secs_f64(seconds));
        Ok(())
    }

    fn close(&mut self) {
        self.format = (0, 0);
    }
}

impl SinkFactory for PacedSink {
    fn make(&self) -> Box<dyn Sink> {
        Box::new(PacedSink {
            format: (0, 0),
            ..self.clone()
        })
    }
}
