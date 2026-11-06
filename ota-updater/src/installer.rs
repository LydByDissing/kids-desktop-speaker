use crate::state::{OtaState, SharedStatus};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tokio::io::AsyncWriteExt;

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
    #[error("download incomplete: expected {expected} bytes, got {actual}")]
    Incomplete { expected: u64, actual: u64 },
}

impl InstallerError {
    /// Whether the release itself is bad, rather than this attempt failing. A full
    /// download with the wrong checksum, or a bundle RAUC rejects, fails the same
    /// way every time, so it shouldn't be downloaded again. A short download is a
    /// network problem and is worth retrying.
    pub fn is_bad_release(&self) -> bool {
        matches!(
            self,
            InstallerError::ChecksumMismatch { .. } | InstallerError::RaucFailed(_)
        )
    }
}

const DEFAULT_BUNDLE_PATH: &str = "/data/ota/staging/bundle.raucb";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

fn bundle_path() -> PathBuf {
    std::env::var_os("OTA_BUNDLE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_BUNDLE_PATH))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn check_digest(actual: String, expected: &str) -> Result<(), InstallerError> {
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(InstallerError::ChecksumMismatch {
            expected: expected.trim().to_string(),
            actual,
        })
    }
}

pub fn verify_checksum(bytes: &[u8], expected: &str) -> Result<(), InstallerError> {
    check_digest(sha256_hex(bytes), expected)
}

/// Streams the bundle to `path`, hashing as it goes so the bundle is never held
/// in RAM. On any failure the partial file is removed.
pub async fn download_verified(
    url: &str,
    expected_sha256: &str,
    expected_size: Option<u64>,
    path: &Path,
) -> Result<(), InstallerError> {
    download_with_read_timeout(url, expected_sha256, expected_size, path, READ_TIMEOUT).await
}

/// A slow download may take as long as it needs, but a stalled one must fail, or
/// weak Wi-Fi could block updates until the next restart.
pub async fn download_with_read_timeout(
    url: &str,
    expected_sha256: &str,
    expected_size: Option<u64>,
    path: &Path,
    read_timeout: Duration,
) -> Result<(), InstallerError> {
    let result = stream_to_file(url, expected_sha256, expected_size, path, read_timeout).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(path).await;
    }
    result
}

async fn stream_to_file(
    url: &str,
    expected_sha256: &str,
    expected_size: Option<u64>,
    path: &Path,
    read_timeout: Duration,
) -> Result<(), InstallerError> {
    let http = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(read_timeout)
        .build()?;
    let mut response = http.get(url).send().await?.error_for_status()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .await?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
    }
    file.flush().await?;
    // Check the size first, so a cut-off download counts as a network failure and
    // not as a bad release.
    if let Some(expected) = expected_size {
        if received != expected {
            return Err(InstallerError::Incomplete {
                expected,
                actual: received,
            });
        }
    }
    check_digest(hex::encode(hasher.finalize()), expected_sha256)
}

pub async fn install_verified(
    url: &str,
    expected_sha256: &str,
    expected_size: Option<u64>,
    status: SharedStatus,
) -> Result<(), InstallerError> {
    let path = bundle_path();
    status.set(OtaState::Downloading).await;
    let result = match download_verified(url, expected_sha256, expected_size, &path).await {
        Ok(()) => {
            status.set(OtaState::Installing).await;
            let installed = rauc_install(&path).await;
            let _ = tokio::fs::remove_file(&path).await;
            installed
        }
        Err(e) => Err(e),
    };

    match &result {
        Ok(()) => status.set(OtaState::Done).await,
        Err(e) => status.set(OtaState::Error(e.to_string())).await,
    }
    result
}

async fn rauc_install(path: &Path) -> Result<(), InstallerError> {
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
    fn bad_release_errors_are_classified() {
        let mismatch = InstallerError::ChecksumMismatch {
            expected: "a".into(),
            actual: "b".into(),
        };
        assert!(mismatch.is_bad_release());
        assert!(InstallerError::RaucFailed(1).is_bad_release());
        let short = InstallerError::Incomplete {
            expected: 10,
            actual: 4,
        };
        assert!(!short.is_bad_release());
    }

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
