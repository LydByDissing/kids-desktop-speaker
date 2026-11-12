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
            .unwrap_or(WEEK.as_secs());
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

/// Checks run at boot and then this often (OTA_REQ_011): many devices
/// shouldn't load the release server.
pub const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60);
/// A failed check is retried this many times before the next scheduled check.
pub const RETRIES: u32 = 3;

/// The failures in a row after a check: a success, or a failure after the
/// last retry, starts over with the next scheduled check.
pub fn failures_after(check_succeeded: bool, failures_in_a_row: u32) -> u32 {
    if check_succeeded || failures_in_a_row >= RETRIES {
        0
    } else {
        failures_in_a_row + 1
    }
}

/// How long to wait after a check, given the failures in a row so far. A failed
/// check is retried soon, because the network or clock may not be ready yet
/// just after boot (the device has no battery clock, and TLS needs the time):
/// after 1, 2 and 4 minutes. After that it waits for the next scheduled check
/// (OTA_REQ_011, KDS-eio).
pub fn next_check_delay(failures_in_a_row: u32, interval: Duration) -> Duration {
    match failures_in_a_row {
        0 => interval,
        n if n <= RETRIES => interval.min(RETRY_AFTER_FAILURE * (1 << (n - 1))),
        _ => interval,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_waits_the_full_interval() {
        assert_eq!(next_check_delay(0, WEEK), WEEK);
    }

    #[test]
    fn a_first_failure_retries_after_a_minute() {
        assert_eq!(next_check_delay(1, WEEK), RETRY_AFTER_FAILURE);
    }

    #[test]
    fn three_retries_then_the_next_scheduled_check() {
        let minutes: Vec<u64> = (1..=5)
            .map(|n| next_check_delay(n, WEEK).as_secs() / 60)
            .collect();
        assert_eq!(minutes, [1, 2, 4, 7 * 24 * 60, 7 * 24 * 60]);
        assert_eq!(next_check_delay(u32::MAX, WEEK), WEEK, "no overflow");
    }

    #[test]
    fn a_boot_check_and_three_retries_then_a_week() {
        // Every check fails: the boot check, 3 retries, then the weekly one.
        let mut failures = 0;
        let mut waits = Vec::new();
        for _ in 0..5 {
            failures = failures_after(false, failures);
            waits.push(next_check_delay(failures, WEEK).as_secs() / 60);
        }
        assert_eq!(waits, [1, 2, 4, 7 * 24 * 60, 1]);
        assert_eq!(failures_after(true, 2), 0, "a success starts over");
    }

    #[test]
    fn the_default_is_a_weekly_check() {
        std::env::remove_var("OTA_CHECK_INTERVAL_SECS");
        assert_eq!(OtaConfig::from_env().check_interval, WEEK);
    }

    #[test]
    fn failure_never_waits_longer_than_the_interval() {
        let short = Duration::from_secs(15);
        assert_eq!(next_check_delay(1, short), short);
    }
}
