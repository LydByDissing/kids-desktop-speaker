use crate::auth::{TokenStore, TOKENS_FILE};
use crate::messages::{MessageStore, MESSAGES_DIR};
use crate::schedule::{AlarmStore, SleepTimer, ALARMS_FILE};
use crate::setup::Setup;
use std::path::PathBuf;
use std::sync::Arc;

/// Where the backend services' sockets live (IPC protocol, DEC-027).
pub const RUN_DIR: &str = "/run/kds";

#[derive(Clone)]
pub struct AppState {
    pub run_dir: PathBuf,
    /// Recorded voice messages (DEC-037).
    pub messages: Arc<MessageStore>,
    /// API tokens and the spoken setup code (DEC-038).
    pub tokens: Arc<TokenStore>,
    pub setup: Arc<Setup>,
    /// The sleep timer and alarms (DEC-041).
    pub sleep: Arc<SleepTimer>,
    pub alarms: Arc<AlarmStore>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            run_dir: RUN_DIR.into(),
            messages: Arc::new(MessageStore::new(MESSAGES_DIR)),
            tokens: Arc::new(TokenStore::load(TOKENS_FILE)),
            setup: Arc::new(Setup::new()),
            sleep: Arc::new(SleepTimer::default()),
            alarms: Arc::new(AlarmStore::new(ALARMS_FILE)),
        }
    }

    /// Uses sockets in `run_dir` instead of `/run/kds`, and keeps messages in
    /// `run_dir/messages`, for tests.
    pub fn with_run_dir(run_dir: impl Into<PathBuf>) -> Self {
        let run_dir: PathBuf = run_dir.into();
        Self {
            messages: Arc::new(MessageStore::new(run_dir.join("messages"))),
            tokens: Arc::new(TokenStore::load(run_dir.join("api").join("tokens.json"))),
            setup: Arc::new(Setup::new()),
            sleep: Arc::new(SleepTimer::default()),
            alarms: Arc::new(AlarmStore::new(run_dir.join("api").join("alarms.json"))),
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
