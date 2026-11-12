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

/// How long to wait after a check. A failed check is retried soon, because the
/// network or clock may not be ready yet just after boot (the device has no
/// battery clock, and TLS needs the time). Failures in a row back off, 1, 2,
/// 4 … minutes, up to the interval, so an outage or GitHub's rate limit
/// doesn't make the speaker ask every minute (KDS-eio).
pub fn next_check_delay(failures_in_a_row: u32, interval: Duration) -> Duration {
    if failures_in_a_row == 0 {
        return interval;
    }
    let factor = 1u32 << (failures_in_a_row - 1).min(16);
    interval.min(RETRY_AFTER_FAILURE.saturating_mul(factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_waits_the_full_interval() {
        let hour = Duration::from_secs(3600);
        assert_eq!(next_check_delay(0, hour), hour);
    }

    #[test]
    fn a_first_failure_retries_after_a_minute() {
        let hour = Duration::from_secs(3600);
        assert_eq!(next_check_delay(1, hour), RETRY_AFTER_FAILURE);
    }

    #[test]
    fn failures_in_a_row_back_off_up_to_the_interval() {
        let hour = Duration::from_secs(3600);
        let minutes: Vec<u64> = (1..=8)
            .map(|n| next_check_delay(n, hour).as_secs() / 60)
            .collect();
        assert_eq!(minutes, [1, 2, 4, 8, 16, 32, 60, 60]);
        assert_eq!(next_check_delay(1000, hour), hour, "no overflow");
    }

    #[test]
    fn failure_never_waits_longer_than_the_interval() {
        let short = Duration::from_secs(15);
        assert_eq!(next_check_delay(1, short), short);
    }
}
