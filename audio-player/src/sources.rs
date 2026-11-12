//! The streaming sources the Audio Player starts (SW_REQ_001, SW_REQ_002,
//! BT_REQ_001).

use crate::manager::Manager;

/// Where librespot keeps the Spotify credentials, so a speaker stays linked to
/// its account across updates (DEC-034).
pub const SPOTIFY_STATE_DIR: &str = "/data/librespot";

/// librespot runs this on each player event, with $PLAYER_EVENT set.
pub const SPOTIFY_EVENT_HOOK: &str = "/usr/local/bin/kds-spotify-event";

/// The name the speaker shows in the Spotify and AirPlay pickers: the hostname
/// with a capital first letter, as shairport-sync's "%H" gives
/// ("kidsspeaker-2df5" becomes "Kidsspeaker-2df5").
pub fn device_name(hostname: &str) -> String {
    let hostname = hostname.trim();
    let mut chars = hostname.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Kids Speaker".to_string(),
    }
}

/// shairport-sync reads /etc/shairport-sync.conf (name, output device).
pub fn airplay() -> Manager {
    Manager::new("shairport-sync", "/usr/bin/shairport-sync", vec![])
}

/// librespot arguments: the shared `speaker` output in its native S16 format,
/// credentials on the data partition, and no audio cache to spare the SD card.
/// Its volume doesn't scale the audio: the phone's volume becomes the
/// speaker's one volume through the event hook (DEC-049).
pub fn spotify_args(name: &str) -> Vec<String> {
    [
        "--name",
        name,
        "--device-type",
        "speaker",
        "--backend",
        "alsa",
        "--device",
        "speaker",
        "--format",
        "S16",
        "--bitrate",
        "160",
        "--system-cache",
        SPOTIFY_STATE_DIR,
        "--disable-audio-cache",
        "--volume-ctrl",
        "fixed",
        // Fine steps for volume buttons in the Spotify app.
        "--volume-steps",
        "100",
        // Reports playing/paused to the Audio Player (DEC-039).
        "--onevent",
        SPOTIFY_EVENT_HOOK,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn spotify(name: &str) -> Manager {
    Manager::new("librespot", "/usr/bin/librespot", spotify_args(name))
}

/// bluealsa-aplay plays Bluetooth audio from BlueALSA's A2DP sink into the
/// shared output (DEC-035). With no phone connected it waits.
pub fn bluetooth_args() -> Vec<String> {
    vec!["--pcm=speaker".to_string()]
}

pub fn bluetooth() -> Manager {
    Manager::new(
        "bluealsa-aplay",
        "/usr/bin/bluealsa-aplay",
        bluetooth_args(),
    )
}

/// The system's hostname, or an empty string if it can't be read.
pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default()
}
