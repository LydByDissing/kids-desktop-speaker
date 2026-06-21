use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvrcpEvent {
    Play,
    Pause,
    Next,
    Prev,
    VolumeUp,
    VolumeDown,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("unknown event: {0}")]
    Unknown(String),
}

/// Parses a line from bluetoothctl output.
///
/// Returns:
/// - None if the line is not an AVRCP event line
/// - Some(Ok(event)) for known KEY_ events
/// - Some(Err(ParseError::Unknown)) for unknown KEY_ strings
pub fn parse_bluetoothctl_line(line: &str) -> Option<Result<AvrcpEvent, ParseError>> {
    // Look for lines starting with "Key: KEY_"
    if let Some(rest) = line.strip_prefix("Key: ") {
        let key = rest.trim();

        let event = match key {
            "KEY_PLAYPAUSE" => AvrcpEvent::Play,
            "KEY_PLAY" => AvrcpEvent::Play,
            "KEY_PAUSE" => AvrcpEvent::Pause,
            "KEY_NEXTSONG" => AvrcpEvent::Next,
            "KEY_PREVIOUSSONG" => AvrcpEvent::Prev,
            "KEY_VOLUMEUP" => AvrcpEvent::VolumeUp,
            "KEY_VOLUMEDOWN" => AvrcpEvent::VolumeDown,
            unknown => return Some(Err(ParseError::Unknown(unknown.to_string()))),
        };

        return Some(Ok(event));
    }

    None
}
