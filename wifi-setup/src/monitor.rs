//! When to open the setup access point (NET_ONB_001/002, DEC-044).

use std::time::{Duration, Instant};

/// Checks run this often.
pub const CHECK_EVERY: Duration = Duration::from_secs(30);
/// At boot the network gets this long before failures count.
pub const GRACE: Duration = Duration::from_secs(120);
/// This many failed checks in a row open the access point.
pub const MAX_FAILURES: u32 = 5;
/// With a saved network, the access point closes after this long and the
/// speaker tries the network again.
pub const AP_TIME: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Connected, or still giving the network time.
    Wait,
    /// Open the access point (for this long, or until set up if None).
    OpenAp(Option<Duration>),
}

pub struct Monitor {
    started: Instant,
    failures: u32,
}

impl Monitor {
    pub fn new(started: Instant) -> Self {
        Monitor {
            started,
            failures: 0,
        }
    }

    /// One check: whether a network is saved and whether the speaker is
    /// connected to it now.
    pub fn check(&mut self, now: Instant, saved: bool, connected: bool) -> Decision {
        if !saved {
            self.failures = 0;
            return Decision::OpenAp(None);
        }
        if connected {
            self.failures = 0;
            return Decision::Wait;
        }
        if now.duration_since(self.started) < GRACE {
            return Decision::Wait;
        }
        self.failures += 1;
        if self.failures >= MAX_FAILURES {
            self.failures = 0;
            return Decision::OpenAp(Some(AP_TIME));
        }
        Decision::Wait
    }

    /// After the access point closes, the network gets a fresh grace period.
    pub fn restart(&mut self, now: Instant) {
        self.started = now;
        self.failures = 0;
    }
}
