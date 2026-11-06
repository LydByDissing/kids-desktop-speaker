pub mod config;
pub mod installer;
pub mod ipc;
pub mod release;
pub mod state;
pub mod version;

pub use config::OtaConfig;
pub use installer::InstallerError;
pub use release::{LatestUpdate, Manifest, ReleaseError};
pub use state::{OtaState, OtaStatus, SharedStatus};
