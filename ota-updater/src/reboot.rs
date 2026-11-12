//! When a pending update reboots the device (OTA_REQ_006, OTA_REQ_007). Rules
//! from the OTA Updater design: only inside the maintenance window, never while
//! audio plays, and only after a grace period. A forced reboot skips all three.

use crate::config::OtaConfig;
use crate::state::SharedStatus;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// A local time range, in minutes after midnight. It may wrap past midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: u16,
    pub end: u16,
}

impl Default for Window {
    /// 03:00 to 05:00, from the OTA Updater design.
    fn default() -> Self {
        Window {
            start: 180,
            end: 300,
        }
    }
}

fn parse_time(s: &str) -> Option<u16> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

impl Window {
    /// Parses "HH:MM-HH:MM".
    pub fn parse(s: &str) -> Option<Window> {
        let (start, end) = s.split_once('-')?;
        let window = Window {
            start: parse_time(start)?,
            end: parse_time(end)?,
        };
        (window.start != window.end).then_some(window)
    }

    pub fn contains(&self, minute: u16) -> bool {
        if self.start < self.end {
            (self.start..self.end).contains(&minute)
        } else {
            minute >= self.start || minute < self.end
        }
    }
}

/// Whether a reboot may happen now.
pub fn may_reboot(pending: bool, in_window: bool, audio_playing: bool) -> bool {
    pending && in_window && !audio_playing
}

fn local_minute() -> u16 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    (now.hour() * 60 + now.minute()) as u16
}

/// Whether the Audio Player reports an active source. If it can't be reached,
/// nothing can be playing through it.
pub async fn audio_playing(socket: &str) -> bool {
    match kds_ipc::request(socket, &serde_json::json!({"cmd": "status"})).await {
        Ok(status) => status.get("active").is_some_and(|a| !a.is_null()),
        Err(_) => false,
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Waits out the grace period before a reboot, showing when it will happen in
/// the status (`reboot_at`) meanwhile. Returns true if `reboot_now` cut it
/// short.
pub async fn grace_period(status: &SharedStatus, grace: Duration, reboot_now: &Notify) -> bool {
    status
        .set_reboot_at(Some(unix_now() + grace.as_secs()))
        .await;
    let forced = tokio::select! {
        _ = reboot_now.notified() => true,
        _ = tokio::time::sleep(grace) => false,
    };
    status.set_reboot_at(None).await;
    forced
}

async fn run_reboot(command: &str) {
    println!("ota: rebooting to activate the update ({command})");
    let mut parts = command.split_whitespace();
    let Some(program) = parts.next() else { return };
    match tokio::process::Command::new(program)
        .args(parts)
        .status()
        .await
    {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("ota: reboot command failed: {s}"),
        Err(e) => eprintln!("ota: reboot command failed: {e}"),
    }
}

/// Reboots for a pending update when the rules allow, or at once when
/// `reboot_now` is notified.
pub async fn reboot_loop(cfg: OtaConfig, status: SharedStatus, reboot_now: Arc<Notify>) {
    loop {
        tokio::select! {
            _ = reboot_now.notified() => {
                println!("ota: reboot requested");
                run_reboot(&cfg.reboot_command).await;
                continue;
            }
            _ = tokio::time::sleep(Duration::from_secs(60)) => {}
        }
        let pending = status.get().await.pending.is_some();
        let allowed =
            |playing| may_reboot(pending, cfg.reboot_window.contains(local_minute()), playing);
        if !allowed(audio_playing(&cfg.audio_socket).await) {
            continue;
        }
        println!(
            "ota: rebooting in {} s unless audio starts",
            cfg.reboot_grace.as_secs()
        );
        let forced = grace_period(&status, cfg.reboot_grace, &reboot_now).await;
        if !forced && !allowed(audio_playing(&cfg.audio_socket).await) {
            println!("ota: reboot postponed");
            continue;
        }
        run_reboot(&cfg.reboot_command).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows() {
        assert_eq!(
            Window::parse("03:00-05:00"),
            Some(Window {
                start: 180,
                end: 300
            })
        );
        assert_eq!(
            Window::parse(" 23:30 - 01:15 "),
            Some(Window {
                start: 1410,
                end: 75
            })
        );
        assert_eq!(Window::parse("25:00-05:00"), None);
        assert_eq!(Window::parse("03:00"), None);
        assert_eq!(Window::parse("03:00-03:00"), None);
    }

    #[test]
    fn window_contains_its_start_but_not_its_end() {
        let w = Window::default();
        assert!(!w.contains(179));
        assert!(w.contains(180));
        assert!(w.contains(299));
        assert!(!w.contains(300));
    }

    #[test]
    fn window_wraps_past_midnight() {
        let w = Window::parse("23:00-02:00").unwrap();
        assert!(w.contains(23 * 60 + 30));
        assert!(w.contains(30));
        assert!(!w.contains(12 * 60));
        assert!(!w.contains(2 * 60));
    }

    #[test]
    fn reboots_only_when_pending_in_window_and_quiet() {
        assert!(may_reboot(true, true, false));
        assert!(!may_reboot(false, true, false));
        assert!(!may_reboot(true, false, false));
        assert!(!may_reboot(true, true, true));
    }

    #[tokio::test]
    async fn the_grace_period_shows_when_the_reboot_happens() {
        let status = SharedStatus::new();
        let notify = Arc::new(Notify::new());
        let (s, n) = (status.clone(), Arc::clone(&notify));
        let wait =
            tokio::spawn(async move { grace_period(&s, Duration::from_secs(300), &n).await });
        // Wait until the grace period has started.
        let mut at = None;
        for _ in 0..100 {
            at = status.get().await.reboot_at;
            if at.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let at = at.expect("reboot_at is set during the grace period");
        assert!((unix_now() + 295..=unix_now() + 300).contains(&at), "{at}");
        notify.notify_one();
        assert!(wait.await.unwrap(), "reboot now cuts it short");
        assert_eq!(status.get().await.reboot_at, None, "and clears it");
    }

    #[tokio::test]
    async fn after_the_grace_period_reboot_at_is_cleared() {
        let status = SharedStatus::new();
        let forced = grace_period(&status, Duration::from_millis(20), &Notify::new()).await;
        assert!(!forced);
        assert_eq!(status.get().await.reboot_at, None);
    }

    #[tokio::test]
    async fn unreachable_audio_player_counts_as_quiet() {
        assert!(!audio_playing("/nonexistent/audio.sock").await);
    }
}
