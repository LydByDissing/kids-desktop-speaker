use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Where the backend services' sockets live (IPC protocol, DEC-027).
pub const RUN_DIR: &str = "/run/kds";

#[derive(Clone)]
pub struct AppState {
    pub vol_limit: Arc<RwLock<u8>>,
    pub run_dir: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        Self::with_run_dir(RUN_DIR)
    }

    /// Uses sockets in `run_dir` instead of `/run/kds`, for tests.
    pub fn with_run_dir(run_dir: impl Into<PathBuf>) -> Self {
        Self {
            vol_limit: Arc::new(RwLock::new(75)),
            run_dir: run_dir.into(),
        }
    }

    pub fn socket(&self, service: Service) -> PathBuf {
        self.run_dir.join(service.socket_name())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// The backend services the API Server queries.
#[derive(Clone, Copy, Debug)]
pub enum Service {
    AudioPlayer,
    OtaUpdater,
    Bluetooth,
}

impl Service {
    pub fn socket_name(self) -> &'static str {
        match self {
            Service::AudioPlayer => "audio.sock",
            Service::OtaUpdater => "ota.sock",
            Service::Bluetooth => "bt.sock",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Service::AudioPlayer => "audio-player",
            Service::OtaUpdater => "ota-updater",
            Service::Bluetooth => "bluetooth-service",
        }
    }
}
