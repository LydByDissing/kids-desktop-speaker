use crate::config::OtaConfig;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub bundle: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub released_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset_url(&self, name: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.as_str())
    }
}

#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("asset not found in release: {0}")]
    AssetNotFound(String),
}

pub struct LatestUpdate {
    pub manifest: Manifest,
    pub bundle_url: String,
}

pub fn parse_release(json: &str) -> Result<Release, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn parse_manifest(json: &str) -> Result<Manifest, serde_json::Error> {
    serde_json::from_str(json)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("kds-ota-updater")
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("reqwest client")
}

/// The latest release's update, or None when the repository has no release
/// yet: GitHub answers 404, also when there are only pre-releases. That is a
/// normal answer, not a failed check (KDS-eio).
pub async fn fetch_latest(cfg: &OtaConfig) -> Result<Option<LatestUpdate>, ReleaseError> {
    let http = client();
    let url = format!(
        "{}/repos/{}/{}/releases/latest",
        cfg.api_base, cfg.repo_owner, cfg.repo_name
    );
    let response = http.get(&url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let release: Release = response.error_for_status()?.json().await?;
    resolve_update(&http, release).await.map(Some)
}

async fn resolve_update(
    http: &reqwest::Client,
    release: Release,
) -> Result<LatestUpdate, ReleaseError> {
    let manifest_url = release
        .asset_url("manifest.json")
        .ok_or_else(|| ReleaseError::AssetNotFound("manifest.json".into()))?
        .to_string();
    let manifest: Manifest = http
        .get(&manifest_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let bundle_url = release
        .asset_url(&manifest.bundle)
        .ok_or_else(|| ReleaseError::AssetNotFound(manifest.bundle.clone()))?
        .to_string();
    Ok(LatestUpdate {
        manifest,
        bundle_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_JSON: &str = r#"{
        "tag_name": "v1.2.3",
        "assets": [
            {"name": "image.raucb", "browser_download_url": "https://example.test/image.raucb"},
            {"name": "manifest.json", "browser_download_url": "https://example.test/manifest.json"}
        ]
    }"#;

    const MANIFEST_JSON: &str = r#"{
        "version": "v1.2.3",
        "bundle": "image.raucb",
        "sha256": "abc123",
        "size": 5310,
        "released_at": "2026-06-28T00:00:00Z"
    }"#;

    #[test]
    fn parses_release_and_finds_assets() {
        let r = parse_release(RELEASE_JSON).unwrap();
        assert_eq!(r.tag_name, "v1.2.3");
        assert_eq!(
            r.asset_url("manifest.json"),
            Some("https://example.test/manifest.json")
        );
        assert_eq!(
            r.asset_url("image.raucb"),
            Some("https://example.test/image.raucb")
        );
        assert_eq!(r.asset_url("missing"), None);
    }

    #[test]
    fn parses_manifest_matching_gen_manifest_schema() {
        let m = parse_manifest(MANIFEST_JSON).unwrap();
        assert_eq!(m.version, "v1.2.3");
        assert_eq!(m.bundle, "image.raucb");
        assert_eq!(m.sha256, "abc123");
        assert_eq!(m.size, 5310);
        assert_eq!(m.released_at.as_deref(), Some("2026-06-28T00:00:00Z"));
    }

    #[test]
    fn manifest_without_released_at_is_ok() {
        let m = parse_manifest(r#"{"version":"v1","bundle":"b","sha256":"s","size":1}"#).unwrap();
        assert_eq!(m.released_at, None);
    }
}
