//! The pairing window (BT_REQ_002): new devices may pair only while it is open.

use std::time::{Duration, Instant};

/// How long a pairing window stays open unless asked otherwise.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(180);
/// The longest window a request may ask for.
pub const MAX_WINDOW: Duration = Duration::from_secs(600);

#[derive(Debug, Default, Clone, Copy)]
pub struct PairingWindow {
    until: Option<Instant>,
}

impl PairingWindow {
    /// Opens the window for `duration` (capped at [`MAX_WINDOW`]) from `now`.
    pub fn open(&mut self, now: Instant, duration: Duration) -> Duration {
        let duration = duration.min(MAX_WINDOW);
        self.until = Some(now + duration);
        duration
    }

    pub fn close(&mut self) {
        self.until = None;
    }

    pub fn is_open(&self, now: Instant) -> bool {
        self.until.is_some_and(|until| now < until)
    }

    /// Time left, or zero when closed.
    pub fn remaining(&self, now: Instant) -> Duration {
        self.until
            .map(|until| until.saturating_duration_since(now))
            .unwrap_or_default()
    }
}
