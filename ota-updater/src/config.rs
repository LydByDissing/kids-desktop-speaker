use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OtaConfig {
    pub repo_owner: String,
    pub repo_name: String,
    pub check_interval: Duration,
    pub version_file: String,
    pub api_base: String,
}

impl OtaConfig {
    pub fn from_env() -> Self {
        let interval_secs = std::env::var("OTA_CHECK_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(3600);
        OtaConfig {
            repo_owner: env_or("OTA_REPO_OWNER", "LydByDissing"),
            repo_name: env_or("OTA_REPO_NAME", "kids-desktop-speaker"),
            check_interval: Duration::from_secs(interval_secs),
            version_file: env_or("OTA_VERSION_FILE", "/etc/kds-version"),
            api_base: env_or("OTA_API_BASE", "https://api.github.com"),
        }
    }
}

impl Default for OtaConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
