use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OtaConfig {
    pub repo_owner: String,
    pub repo_name: String,
    pub check_interval: Duration,
    pub version_file: String,
    pub api_base: String,
    /// Local time range in which a pending update may reboot the device.
    pub reboot_window: crate::reboot::Window,
    /// Wait before rebooting, rechecking activity at the end.
    pub reboot_grace: Duration,
    /// The command that reboots the device. Tests replace it.
    pub reboot_command: String,
    /// The Audio Player's socket, asked whether audio is playing.
    pub audio_socket: String,
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
            reboot_window: crate::reboot::Window::parse(&env_or(
                "OTA_REBOOT_WINDOW",
                "03:00-05:00",
            ))
            .unwrap_or_default(),
            reboot_grace: Duration::from_secs(
                std::env::var("OTA_REBOOT_GRACE_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
            ),
            reboot_command: env_or("OTA_REBOOT_COMMAND", "systemctl reboot"),
            audio_socket: env_or("OTA_AUDIO_SOCKET", "/run/kds/audio.sock"),
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

const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60);

/// A failed check is retried soon, because the network or clock may not be ready
/// yet just after boot. The device has no battery clock, and TLS needs the time.
pub fn next_check_delay(check_succeeded: bool, interval: Duration) -> Duration {
    if check_succeeded {
        interval
    } else {
        interval.min(RETRY_AFTER_FAILURE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_waits_the_full_interval() {
        let hour = Duration::from_secs(3600);
        assert_eq!(next_check_delay(true, hour), hour);
    }

    #[test]
    fn failure_retries_sooner() {
        let hour = Duration::from_secs(3600);
        assert_eq!(next_check_delay(false, hour), RETRY_AFTER_FAILURE);
    }

    #[test]
    fn failure_never_waits_longer_than_the_interval() {
        let short = Duration::from_secs(15);
        assert_eq!(next_check_delay(false, short), short);
    }
}
