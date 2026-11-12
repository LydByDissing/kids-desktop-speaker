use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OtaState {
    Idle,
    Downloading,
    Installing,
    Done,
    Error(String),
}

pub struct OtaStatus {
    pub state: OtaState,
    pub version: Option<String>, // running version
    /// An installed version waiting for a reboot.
    pub pending: Option<String>,
    /// While a reboot waits out its grace period: when it happens, in Unix
    /// seconds (OTA_REQ_010, DEC-046).
    pub reboot_at: Option<u64>,
}

#[derive(Clone)]
pub struct SharedStatus(pub Arc<RwLock<OtaStatus>>);

impl SharedStatus {
    pub fn new() -> Self {
        SharedStatus(Arc::new(RwLock::new(OtaStatus {
            state: OtaState::Idle,
            version: None,
            pending: None,
            reboot_at: None,
        })))
    }

    pub async fn get(&self) -> OtaStatus {
        let status = self.0.read().await;
        OtaStatus {
            state: status.state.clone(),
            version: status.version.clone(),
            pending: status.pending.clone(),
            reboot_at: status.reboot_at,
        }
    }

    pub async fn set(&self, s: OtaState) {
        let mut status = self.0.write().await;
        status.state = s;
    }
    pub async fn set_pending(&self, v: String) {
        let mut status = self.0.write().await;
        status.pending = Some(v);
    }
    pub async fn set_reboot_at(&self, at: Option<u64>) {
        let mut status = self.0.write().await;
        status.reboot_at = at;
    }
    pub async fn set_version(&self, v: String) {
        let mut status = self.0.write().await;
        status.version = Some(v);
    }
}

impl Default for SharedStatus {
    fn default() -> Self {
        Self::new()
    }
}
