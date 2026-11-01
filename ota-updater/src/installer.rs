use crate::state::{OtaState, SharedStatus};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InstallerError {
    #[error("download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("rauc failed: exit {0}")]
    RaucFailed(i32),
}

const BUNDLE_PATH: &str = "/tmp/update.raucb";

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn verify_checksum(bytes: &[u8], expected: &str) -> Result<(), InstallerError> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(InstallerError::ChecksumMismatch {
            expected: expected.trim().to_string(),
            actual,
        })
    }
}

pub async fn install_verified(
    url: &str,
    expected_sha256: &str,
    status: SharedStatus,
) -> Result<(), InstallerError> {
    status.set(OtaState::Downloading).await;
    let bytes = match download(url).await {
        Ok(b) => b,
        Err(e) => {
            status.set(OtaState::Error(e.to_string())).await;
            return Err(e);
        }
    };

    if let Err(e) = verify_checksum(&bytes, expected_sha256) {
        status.set(OtaState::Error(e.to_string())).await;
        return Err(e);
    }

    tokio::fs::write(BUNDLE_PATH, &bytes).await?;
    status.set(OtaState::Installing).await;
    let result = rauc_install(BUNDLE_PATH).await;
    let _ = tokio::fs::remove_file(BUNDLE_PATH).await;

    match result {
        Ok(()) => {
            status.set(OtaState::Done).await;
            Ok(())
        }
        Err(e) => {
            status.set(OtaState::Error(e.to_string())).await;
            Err(e)
        }
    }
}

async fn download(url: &str) -> Result<Vec<u8>, InstallerError> {
    let response = reqwest::get(url).await?.error_for_status()?;
    Ok(response.bytes().await?.to_vec())
}

async fn rauc_install(path: &str) -> Result<(), InstallerError> {
    let output = tokio::process::Command::new("rauc")
        .arg("install")
        .arg(path)
        .status()
        .await?;
    if output.success() {
        Ok(())
    } else {
        Err(InstallerError::RaucFailed(output.code().unwrap_or(-1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_accepts_matching_checksum_case_insensitive() {
        let data = b"kids-desktop-speaker bundle";
        let sum = sha256_hex(data);
        assert!(verify_checksum(data, &sum).is_ok());
        assert!(verify_checksum(data, &sum.to_uppercase()).is_ok());
        assert!(verify_checksum(data, &format!("  {sum}  ")).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_bundle() {
        let sum = sha256_hex(b"original");
        let err = verify_checksum(b"tampered", &sum).unwrap_err();
        assert!(matches!(err, InstallerError::ChecksumMismatch { .. }));
    }
}
