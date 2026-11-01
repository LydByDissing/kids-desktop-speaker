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
    pub version: Option<String>, // last installed version tag
}

#[derive(Clone)]
pub struct SharedStatus(pub Arc<RwLock<OtaStatus>>);

impl SharedStatus {
    pub fn new() -> Self {
        SharedStatus(Arc::new(RwLock::new(OtaStatus {
            state: OtaState::Idle,
            version: None,
        })))
    }

    pub async fn get(&self) -> OtaStatus {
        let status = self.0.read().await;
        OtaStatus {
            state: status.state.clone(),
            version: status.version.clone(),
        }
    }

    pub async fn set(&self, s: OtaState) {
        let mut status = self.0.write().await;
        status.state = s;
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
