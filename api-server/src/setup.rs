//! The spoken setup code (AUTH_REQ_003, DEC-038): the speaker reads a 6-digit
//! code aloud, and whoever types it in gets an admin token.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::auth::constant_time_eq;

/// A code is valid this long, unless the speaker keeps reading it.
pub const CODE_LIFETIME: Duration = Duration::from_secs(600);
/// A new code can be requested this often at most.
pub const REQUEST_INTERVAL: Duration = Duration::from_secs(60);
pub const MAX_ATTEMPTS: u32 = 5;

#[derive(Debug, PartialEq, Eq)]
pub enum CheckError {
    /// No code is active: ask for one.
    NoCode,
    /// Wrong; this many attempts are left (0 means the code is gone).
    Wrong(u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    /// Asked too soon; try again after this long.
    TooSoon(Duration),
}

struct Code {
    digits: String,
    issued: Instant,
    attempts_left: u32,
}

#[derive(Default)]
pub struct Setup {
    code: Mutex<Option<Code>>,
    last_request: Mutex<Option<Instant>>,
}

/// Six random digits, without the bias of a plain modulo.
pub fn random_code() -> String {
    use std::io::Read;
    let mut f =
        std::fs::File::open("/dev/urandom").expect("the system's random source must be readable");
    loop {
        let mut b = [0u8; 4];
        f.read_exact(&mut b)
            .expect("the system's random source must be readable");
        let n = u32::from_le_bytes(b);
        // 4294000000 is the largest multiple of 1000000 below 2^32.
        if n < 4_294_000_000 {
            return format!("{:06}", n % 1_000_000);
        }
    }
}

/// What the speaker says: the code twice, digit by digit, in Danish.
pub fn spoken(code: &str) -> String {
    let digits: Vec<String> = code.chars().map(String::from).collect();
    let read = digits.join(", ");
    format!("Opsætningskode: {read}. Jeg gentager: {read}.")
}

impl Setup {
    pub fn new() -> Self {
        Self::default()
    }

    /// The active code, making one if there is none (or it expired). `keep`
    /// restarts its lifetime: while the speaker keeps reading a code, it stays
    /// valid.
    pub fn current_or_new(&self, now: Instant, keep: bool) -> String {
        let mut code = self.code.lock().expect("setup state");
        let expired = code
            .as_ref()
            .is_none_or(|c| now.duration_since(c.issued) >= CODE_LIFETIME);
        if expired {
            *code = Some(Code {
                digits: random_code(),
                issued: now,
                attempts_left: MAX_ATTEMPTS,
            });
        } else if keep {
            if let Some(c) = code.as_mut() {
                c.issued = now;
            }
        }
        code.as_ref().map(|c| c.digits.clone()).unwrap_or_default()
    }

    /// A fresh code on request (POST /api/setup/start), at most once a minute.
    pub fn request_new(&self, now: Instant) -> Result<String, StartError> {
        let mut last = self.last_request.lock().expect("setup state");
        if let Some(t) = *last {
            let since = now.duration_since(t);
            if since < REQUEST_INTERVAL {
                return Err(StartError::TooSoon(REQUEST_INTERVAL - since));
            }
        }
        *last = Some(now);
        let digits = random_code();
        *self.code.lock().expect("setup state") = Some(Code {
            digits: digits.clone(),
            issued: now,
            attempts_left: MAX_ATTEMPTS,
        });
        Ok(digits)
    }

    /// Checks a typed code. A correct code is used up; after the last wrong
    /// attempt the code is gone.
    pub fn check(&self, typed: &str, now: Instant) -> Result<(), CheckError> {
        let mut slot = self.code.lock().expect("setup state");
        let Some(code) = slot.as_mut() else {
            return Err(CheckError::NoCode);
        };
        if now.duration_since(code.issued) >= CODE_LIFETIME {
            *slot = None;
            return Err(CheckError::NoCode);
        }
        if constant_time_eq(typed.trim(), &code.digits) {
            *slot = None;
            return Ok(());
        }
        code.attempts_left -= 1;
        let left = code.attempts_left;
        if left == 0 {
            *slot = None;
        }
        Err(CheckError::Wrong(left))
    }
}
