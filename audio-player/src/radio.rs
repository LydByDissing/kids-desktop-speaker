//! Internet radio stations, kept on the data partition (RAD_REQ_001).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STATIONS_FILE: &str = "/data/radio/stations.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub url: String,
}

/// The stations a new speaker starts with: DR's national channels.
pub fn default_stations() -> Vec<Station> {
    [
        ("dr-p1", "DR P1", "http://live-icy.dr.dk/A/A03H.mp3"),
        ("dr-p2", "DR P2", "http://live-icy.dr.dk/A/A04H.mp3"),
        ("dr-p3", "DR P3", "http://live-icy.dr.dk/A/A05H.mp3"),
    ]
    .iter()
    .map(|(id, name, url)| Station {
        id: id.to_string(),
        name: name.to_string(),
        url: url.to_string(),
    })
    .collect()
}

/// Checks a stream URL: http or https with a host, no whitespace (RAD_REQ_001).
/// MPD would otherwise also accept local files and other URL schemes.
pub fn validate_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or("url must start with http:// or https://")?;
    if url.len() > 2048 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("url is not valid".into());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        return Err("url has no host".into());
    }
    Ok(())
}

/// Makes an id from a name: lowercase letters and digits joined by dashes.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "station".into()
    } else {
        out
    }
}

pub struct StationStore {
    path: PathBuf,
}

impl StationStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        StationStore { path: path.into() }
    }

    /// The saved stations, or the defaults if nothing is saved yet.
    pub fn list(&self) -> Result<Vec<Station>, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("station list is damaged: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(default_stations()),
            Err(e) => Err(format!("cannot read stations: {e}")),
        }
    }

    pub fn get(&self, id: &str) -> Result<Station, String> {
        self.list()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("no station {id}"))
    }

    /// Adds a station and returns it. Its id comes from the name, made unique.
    pub fn add(&self, name: &str, url: &str) -> Result<Station, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err("name must be 1 to 100 characters".into());
        }
        validate_url(url)?;
        let mut stations = self.list()?;
        if stations.len() >= 100 {
            return Err("too many stations (100)".into());
        }
        let base = slug(name);
        let mut id = base.clone();
        let mut n = 2;
        while stations.iter().any(|s| s.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        let station = Station {
            id,
            name: name.to_string(),
            url: url.to_string(),
        };
        stations.push(station.clone());
        self.save(&stations)?;
        Ok(station)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut stations = self.list()?;
        let before = stations.len();
        stations.retain(|s| s.id != id);
        if stations.len() == before {
            return Err(format!("no station {id}"));
        }
        self.save(&stations)
    }

    /// Writes the list atomically: a temporary file renamed over the old one.
    fn save(&self, stations: &[Station]) -> Result<(), String> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot save stations: {e}"))?;
        let temp = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(stations).map_err(|e| e.to_string())?;
        std::fs::write(&temp, json)
            .and_then(|()| std::fs::rename(&temp, &self.path))
            .map_err(|e| format!("cannot save stations: {e}"))
    }
}
