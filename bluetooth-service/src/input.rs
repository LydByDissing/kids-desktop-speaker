//! A Bluetooth remote's buttons, read from the Linux input device BlueZ makes
//! for it (RMT_REQ_001, DEC-042).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;

/// `struct input_event`: two longs of time, then u16 type, u16 code, i32 value.
/// 16 bytes on the Pi's 32-bit ARM, 24 on a 64-bit machine.
pub const EVENT_SIZE: usize = if cfg!(target_pointer_width = "64") {
    24
} else {
    16
};
const EV_KEY: u16 = 1;
/// The Linux bus type of devices BlueZ creates for remotes.
const BUS_BLUETOOTH: &str = "0005";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAction {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
}

impl RemoteAction {
    pub fn name(self) -> &'static str {
        match self {
            RemoteAction::PlayPause => "play_pause",
            RemoteAction::Next => "next",
            RemoteAction::Previous => "previous",
            RemoteAction::VolumeUp => "volume_up",
            RemoteAction::VolumeDown => "volume_down",
        }
    }
}

/// Splits one raw event into (type, code, value).
pub fn parse_event(raw: &[u8]) -> Option<(u16, u16, i32)> {
    if raw.len() != EVENT_SIZE {
        return None;
    }
    let at = EVENT_SIZE - 8;
    let kind = u16::from_le_bytes([raw[at], raw[at + 1]]);
    let code = u16::from_le_bytes([raw[at + 2], raw[at + 3]]);
    let value = i32::from_le_bytes([raw[at + 4], raw[at + 5], raw[at + 6], raw[at + 7]]);
    Some((kind, code, value))
}

/// What a key event means. A press acts; a held key repeats for volume only.
pub fn action_for(kind: u16, code: u16, value: i32) -> Option<RemoteAction> {
    if kind != EV_KEY {
        return None;
    }
    let action = match code {
        164 | 200 | 201 | 207 | 119 => RemoteAction::PlayPause, // PLAYPAUSE, PLAYCD, PAUSECD, PLAY, PAUSE
        163 => RemoteAction::Next,                              // NEXTSONG
        165 => RemoteAction::Previous,                          // PREVIOUSSONG
        115 => RemoteAction::VolumeUp,
        114 => RemoteAction::VolumeDown,
        _ => return None,
    };
    let volume = matches!(action, RemoteAction::VolumeUp | RemoteAction::VolumeDown);
    match value {
        1 => Some(action),
        2 if volume => Some(action),
        _ => None,
    }
}

/// The input devices on the Bluetooth bus: `/sys/class/input/eventN` whose
/// bus type is Bluetooth, as `<dev>/eventN`.
pub fn bluetooth_inputs(sys: &Path, dev: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(sys)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with("event") {
                return None;
            }
            let bus = std::fs::read_to_string(entry.path().join("device/id/bustype")).ok()?;
            (bus.trim() == BUS_BLUETOOTH).then(|| dev.join(name))
        })
        .collect();
    found.sort();
    found
}

/// Reads one input device until it goes away, reporting each action.
pub async fn read_device<F, Fut>(path: &Path, report: &F) -> std::io::Result<()>
where
    F: Fn(RemoteAction) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut file = tokio::fs::File::open(path).await?;
    let mut raw = [0u8; EVENT_SIZE];
    loop {
        file.read_exact(&mut raw).await?;
        if let Some(action) = parse_event(&raw).and_then(|(k, c, v)| action_for(k, c, v)) {
            report(action).await;
        }
    }
}

/// Watches for Bluetooth input devices every `every` and reads each new one.
pub async fn watch_remotes<F, Fut>(sys: PathBuf, dev: PathBuf, every: Duration, report: F)
where
    F: Fn(RemoteAction) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let report = Arc::new(report);
    let open: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    loop {
        for path in bluetooth_inputs(&sys, &dev) {
            if !open
                .lock()
                .map(|mut o| o.insert(path.clone()))
                .unwrap_or(false)
            {
                continue;
            }
            println!("bluetooth-service: reading remote {}", path.display());
            let (report, open) = (Arc::clone(&report), Arc::clone(&open));
            tokio::spawn(async move {
                let _ = read_device(&path, report.as_ref()).await;
                println!("bluetooth-service: remote {} went away", path.display());
                if let Ok(mut o) = open.lock() {
                    o.remove(&path);
                }
            });
        }
        tokio::time::sleep(every).await;
    }
}
