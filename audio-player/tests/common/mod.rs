//! A local HTTP server with test audio for the stream player (DEC-048):
//!
//! - `/file/<name>`: a fixture with its length, answering `Range` requests;
//! - `/live/<name>?title=<t>`: the fixture over and over, like a radio
//!   station, with ICY metadata carrying `t` as the stream title;
//! - `/once/<name>`: like a live stream, but it ends after one pass, as a
//!   dropped connection does;
//! - `/page`: an HTML page, not audio; anything else: 404.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Audio bytes between ICY metadata blocks on `/live`.
pub const METAINT: usize = 4000;

pub struct AudioServer {
    pub base: String,
    /// The path of every request, in order.
    pub requests: Arc<Mutex<Vec<String>>>,
}

impl AudioServer {
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn count(&self, prefix: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.starts_with(prefix))
            .count()
    }
}

pub fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("aac") => "audio/aac",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        _ => "application/octet-stream",
    }
}

pub fn start() -> AudioServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let requests: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                let _ = serve(stream, &log);
            });
        }
    });
    AudioServer { base, requests }
}

fn serve(mut stream: TcpStream, log: &Mutex<Vec<String>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut range: Option<u64> = None;
    let mut icy = false;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        let lower = header.to_lowercase();
        if let Some(v) = lower.strip_prefix("range: bytes=") {
            range = v.trim().trim_end_matches('-').parse().ok();
        }
        if lower.starts_with("icy-metadata: 1") {
            icy = true;
        }
    }
    log.lock().unwrap().push(target.clone());
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let title = query
        .strip_prefix("title=")
        .map(|t| t.replace("%20", " "))
        .unwrap_or_default();

    if let Some(name) = path.strip_prefix("/file/") {
        let data = fixture(name);
        let start = range.unwrap_or(0).min(data.len() as u64) as usize;
        let head = if range.is_some() {
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Type: {}\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                content_type(name),
                data.len() - start,
                data.len().saturating_sub(1),
                data.len()
            )
        } else {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                content_type(name),
                data.len()
            )
        };
        stream.write_all(head.as_bytes())?;
        return stream.write_all(&data[start..]);
    }
    let (name, forever) = match (path.strip_prefix("/live/"), path.strip_prefix("/once/")) {
        (Some(n), _) => (n, true),
        (_, Some(n)) => (n, false),
        _ if path == "/page" => {
            let body = "<html><body>Not a stream</body></html>";
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes())?;
            return stream.write_all(body.as_bytes());
        }
        _ => {
            return stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
        }
    };
    let data = fixture(name);
    let mut head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nConnection: close\r\n",
        content_type(name)
    );
    if icy {
        head.push_str(&format!("icy-metaint: {METAINT}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    let meta = {
        let mut block = format!("StreamTitle='{title}';").into_bytes();
        block.resize(block.len().div_ceil(16) * 16, 0);
        let mut out = vec![(block.len() / 16) as u8];
        out.extend(block);
        out
    };
    let mut audio = data.iter().copied().cycle();
    let mut sent = 0usize;
    loop {
        let chunk: Vec<u8> = (&mut audio).take(METAINT).collect();
        stream.write_all(&chunk)?;
        if icy {
            stream.write_all(&meta)?;
        }
        sent += chunk.len();
        if !forever && sent >= data.len() {
            return Ok(());
        }
    }
}

/// Reads everything a reader gives, for tests that check bodies.
pub fn read_all(mut r: impl Read) -> Vec<u8> {
    let mut out = Vec::new();
    r.read_to_end(&mut out).unwrap();
    out
}
