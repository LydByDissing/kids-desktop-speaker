use crate::auth::{TokenStore, TOKENS_FILE};
use crate::messages::{MessageStore, MESSAGES_DIR};
use crate::setup::Setup;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Where the backend services' sockets live (IPC protocol, DEC-027).
pub const RUN_DIR: &str = "/run/kds";

#[derive(Clone)]
pub struct AppState {
    pub vol_limit: Arc<RwLock<u8>>,
    pub run_dir: PathBuf,
    /// Recorded voice messages (DEC-037).
    pub messages: Arc<MessageStore>,
    /// API tokens and the spoken setup code (DEC-038).
    pub tokens: Arc<TokenStore>,
    pub setup: Arc<Setup>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            vol_limit: Arc::new(RwLock::new(75)),
            run_dir: RUN_DIR.into(),
            messages: Arc::new(MessageStore::new(MESSAGES_DIR)),
            tokens: Arc::new(TokenStore::load(TOKENS_FILE)),
            setup: Arc::new(Setup::new()),
        }
    }

    /// Uses sockets in `run_dir` instead of `/run/kds`, and keeps messages in
    /// `run_dir/messages`, for tests.
    pub fn with_run_dir(run_dir: impl Into<PathBuf>) -> Self {
        let run_dir: PathBuf = run_dir.into();
        Self {
            vol_limit: Arc::new(RwLock::new(75)),
            messages: Arc::new(MessageStore::new(run_dir.join("messages"))),
            tokens: Arc::new(TokenStore::load(run_dir.join("api").join("tokens.json"))),
            setup: Arc::new(Setup::new()),
            run_dir,
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
