use crate::state::{OtaState, SharedStatus};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InstallerError {
    #[error("download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("rauc failed: exit {0}")]
    RaucFailed(i32),
}

const BUNDLE_PATH: &str = "/tmp/update.raucb";

pub async fn install(url: &str, status: SharedStatus) -> Result<(), InstallerError> {
    status.set(OtaState::Downloading).await;
    let response = reqwest::get(url).await?;
    let bytes = response.bytes().await?;
    tokio::fs::write(BUNDLE_PATH, bytes).await?;

    status.set(OtaState::Installing).await;
    let result = tokio::process::Command::new("rauc")
        .arg("install")
        .arg(BUNDLE_PATH)
        .status()
        .await;

    let _ = tokio::fs::remove_file(BUNDLE_PATH).await;

    let output = result?;
    if output.success() {
        status.set(OtaState::Done).await;
        Ok(())
    } else {
        let exit_code = output.code().unwrap_or(-1);
        status.set(OtaState::Error(format!("rauc exit {exit_code}"))).await;
        Err(InstallerError::RaucFailed(exit_code))
    }
}
