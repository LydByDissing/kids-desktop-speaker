//! A Bluetooth remote's buttons from Linux input events (RMT_REQ_001, DEC-042).

use bluetooth_service::input::{
    action_for, bluetooth_inputs, parse_event, read_device, RemoteAction, EVENT_SIZE,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// One raw input event as the kernel writes it.
fn event(kind: u16, code: u16, value: i32) -> Vec<u8> {
    let mut raw = vec![0u8; EVENT_SIZE - 8];
    raw.extend_from_slice(&kind.to_le_bytes());
    raw.extend_from_slice(&code.to_le_bytes());
    raw.extend_from_slice(&value.to_le_bytes());
    raw
}

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;

#[test]
fn events_are_parsed() {
    assert_eq!(parse_event(&event(EV_KEY, 164, 1)), Some((EV_KEY, 164, 1)));
    assert_eq!(parse_event(&[0u8; 5]), None);
}

#[test]
fn buttons_map_to_actions() {
    assert_eq!(action_for(EV_KEY, 164, 1), Some(RemoteAction::PlayPause));
    assert_eq!(
        action_for(EV_KEY, 200, 1),
        Some(RemoteAction::PlayPause),
        "PLAYCD"
    );
    assert_eq!(action_for(EV_KEY, 163, 1), Some(RemoteAction::Next));
    assert_eq!(action_for(EV_KEY, 165, 1), Some(RemoteAction::Previous));
    assert_eq!(action_for(EV_KEY, 115, 1), Some(RemoteAction::VolumeUp));
    assert_eq!(action_for(EV_KEY, 114, 1), Some(RemoteAction::VolumeDown));
    assert_eq!(action_for(EV_KEY, 164, 0), None, "a release does nothing");
    assert_eq!(
        action_for(EV_KEY, 115, 2),
        Some(RemoteAction::VolumeUp),
        "held volume repeats"
    );
    assert_eq!(action_for(EV_KEY, 164, 2), None, "held play/pause does not");
    assert_eq!(action_for(EV_SYN, 0, 0), None);
    assert_eq!(action_for(EV_KEY, 30, 1), None, "other keys are ignored");
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kds-input-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn only_bluetooth_input_devices_are_used() {
    let sys = temp_dir("sys");
    for (event, bus) in [("event0", "0003"), ("event3", "0005"), ("mouse0", "0005")] {
        std::fs::create_dir_all(sys.join(event).join("device/id")).unwrap();
        std::fs::write(
            sys.join(event).join("device/id/bustype"),
            format!("{bus}\n"),
        )
        .unwrap();
    }
    let found = bluetooth_inputs(&sys, std::path::Path::new("/dev/input"));
    assert_eq!(
        found,
        vec![PathBuf::from("/dev/input/event3")],
        "USB keyboards and non-event nodes are skipped"
    );
}

#[tokio::test]
async fn a_device_is_read_until_it_goes_away() {
    let dir = temp_dir("device");
    let device = dir.join("event3");
    let mut raw = Vec::new();
    for (k, c, v) in [
        (EV_KEY, 115, 1),
        (EV_SYN, 0, 0),
        (EV_KEY, 115, 0),
        (EV_KEY, 164, 1),
        (EV_KEY, 164, 0),
        (EV_KEY, 163, 1),
    ] {
        raw.extend(event(k, c, v));
    }
    std::fs::write(&device, raw).unwrap();
    let seen: Arc<Mutex<Vec<RemoteAction>>> = Arc::default();
    let log = Arc::clone(&seen);
    let report = move |a: RemoteAction| {
        let log = Arc::clone(&log);
        async move { log.lock().unwrap().push(a) }
    };
    let ended = read_device(&device, &report).await;
    assert!(ended.is_err(), "the end of the device ends reading");
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            RemoteAction::VolumeUp,
            RemoteAction::PlayPause,
            RemoteAction::Next
        ]
    );
}
