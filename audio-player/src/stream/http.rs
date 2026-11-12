//! An HTTP stream as a seekable source for the decoder (DEC-048). Live radio
//! has no length and can't seek; a podcast file has one, and a seek asks for
//! the rest from that byte (`Range`). A stalled connection fails after
//! [`READ_TIMEOUT`], so the player can reconnect instead of hanging.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::icy::{IcyReader, Title};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// A forward seek this short reads and drops the bytes instead of reconnecting.
const SKIP_LIMIT: u64 = 256 * 1024;

/// The response body as a blocking reader, driven by the source's own runtime.
struct Body {
    rt: Arc<tokio::runtime::Runtime>,
    response: reqwest::Response,
    chunk: bytes::Bytes,
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.chunk.is_empty() {
            match self.rt.block_on(self.response.chunk()) {
                Ok(Some(chunk)) => self.chunk = chunk,
                Ok(None) => return Ok(0),
                Err(e) => return Err(io::Error::new(io::ErrorKind::TimedOut, e)),
            }
        }
        let n = buf.len().min(self.chunk.len());
        buf[..n].copy_from_slice(&self.chunk[..n]);
        self.chunk = self.chunk.split_off(n);
        Ok(n)
    }
}

pub struct HttpSource {
    url: String,
    client: reqwest::Client,
    rt: Arc<tokio::runtime::Runtime>,
    // MediaSource needs Sync; the reader is only used from one thread.
    reader: Mutex<Box<dyn Read + Send>>,
    pos: u64,
    len: Option<u64>,
    content_type: Option<String>,
    title: Title,
}

impl HttpSource {
    /// Connects to `url`. Stream titles (ICY metadata) go to `title`. `rt` is
    /// the playback thread's runtime, which the caller has entered.
    pub fn open(url: &str, title: Title, rt: Arc<tokio::runtime::Runtime>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .user_agent("kids-desktop-speaker")
            .build()
            .map_err(|e| e.to_string())?;
        let mut source = HttpSource {
            url: url.to_string(),
            client,
            rt,
            reader: Mutex::new(Box::new(io::empty())),
            pos: 0,
            len: None,
            content_type: None,
            title,
        };
        source.request(0)?;
        Ok(source)
    }

    /// The server's content type, such as `audio/mpeg`.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// (Re)connects, reading from byte `start`.
    fn request(&mut self, start: u64) -> Result<(), String> {
        let mut request = self.client.get(&self.url);
        if start == 0 {
            request = request.header("Icy-MetaData", "1");
        } else {
            request = request.header("Range", format!("bytes={start}-"));
        }
        let response = self
            .rt
            .block_on(request.send())
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("{}: {e}", self.url))?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let partial = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let metaint = header("icy-metaint").and_then(|v| v.trim().parse::<usize>().ok());
        if start == 0 {
            self.content_type = header("content-type")
                .map(|t| t.split(';').next().unwrap_or("").trim().to_lowercase());
            self.len = response.content_length();
        } else if partial {
            // "bytes 1000-1999/2000": the total is after the slash.
            self.len = header("content-range")
                .and_then(|r| r.rsplit('/').next()?.parse().ok())
                .or(self.len);
        }
        let body = Body {
            rt: Arc::clone(&self.rt),
            response,
            chunk: bytes::Bytes::new(),
        };
        let mut reader: Box<dyn Read + Send> = match metaint {
            Some(n) if n > 0 => Box::new(IcyReader::new(body, n, Arc::clone(&self.title))),
            _ => Box::new(body),
        };
        let mut pos = if partial { start } else { 0 };
        // The server ignored the range: read up to where we wanted to be.
        if pos < start {
            io::copy(&mut (&mut reader).take(start - pos), &mut io::sink())
                .map_err(|e| e.to_string())?;
            pos = start;
        }
        self.reader = Mutex::new(reader);
        self.pos = pos;
        Ok(())
    }
}

impl Read for HttpSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.get_mut().expect("http reader").read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpSource {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            SeekFrom::End(d) => self.len.and_then(|len| len.checked_add_signed(d)),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek out of range"))?;
        if target == self.pos {
            return Ok(target);
        }
        if self.len.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "a live stream can't seek",
            ));
        }
        if target > self.pos && target - self.pos <= SKIP_LIMIT {
            let skip = target - self.pos;
            let copied = io::copy(&mut self.take(skip), &mut io::sink())?;
            if copied < skip {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "seek past end",
                ));
            }
            return Ok(self.pos);
        }
        self.request(target).map_err(io::Error::other)?;
        Ok(self.pos)
    }
}

impl symphonia::core::io::MediaSource for HttpSource {
    fn is_seekable(&self) -> bool {
        self.len.is_some()
    }

    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}
