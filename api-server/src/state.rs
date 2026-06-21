use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Airplay,
    Spotify,
    Radio,
}

#[derive(Clone)]
pub struct AppState {
    pub vol_limit: Arc<RwLock<u8>>,
    pub active_source: Arc<RwLock<Option<Source>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            vol_limit: Arc::new(RwLock::new(75)),
            active_source: Arc::new(RwLock::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
