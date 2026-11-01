use bluetooth_service::events::{parse_bluetoothctl_line, AvrcpEvent};

#[test]
fn test_parse_key_playpause() {
    let line = "Key: KEY_PLAYPAUSE";
    let result = parse_bluetoothctl_line(line);
    assert_eq!(result, Some(Ok(AvrcpEvent::Play)));
}

#[test]
fn test_parse_key_nextsong() {
    let line = "Key: KEY_NEXTSONG";
    let result = parse_bluetoothctl_line(line);
    assert_eq!(result, Some(Ok(AvrcpEvent::Next)));
}

#[test]
fn test_parse_key_previoussong() {
    let line = "Key: KEY_PREVIOUSSONG";
    let result = parse_bluetoothctl_line(line);
    assert_eq!(result, Some(Ok(AvrcpEvent::Prev)));
}

#[test]
fn test_parse_key_volumeup() {
    let line = "Key: KEY_VOLUMEUP";
    let result = parse_bluetoothctl_line(line);
    assert_eq!(result, Some(Ok(AvrcpEvent::VolumeUp)));
}

#[test]
fn test_parse_unrecognized_line_returns_none() {
    let line = "[CHG] Device XX:XX:XX:XX:XX:XX ServicesResolved: yes";
    let result = parse_bluetoothctl_line(line);
    assert_eq!(result, None);
}
