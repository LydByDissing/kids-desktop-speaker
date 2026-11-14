use audio_player::sources::{device_name, spotify_args, SPOTIFY_STATE_DIR};

#[test]
fn device_name_capitalises_the_hostname() {
    assert_eq!(device_name("kidsspeaker-2df5\n"), "Kidsspeaker-2df5");
    assert_eq!(device_name("Already"), "Already");
    assert_eq!(device_name(""), "Kids Speaker");
}

fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

#[test]
fn spotify_plays_through_the_shared_output() {
    let args = spotify_args("Kidsspeaker-2df5");
    assert_eq!(value_of(&args, "--name"), Some("Kidsspeaker-2df5"));
    assert_eq!(value_of(&args, "--backend"), Some("alsa"));
    assert_eq!(value_of(&args, "--device"), Some("speaker"));
    assert_eq!(value_of(&args, "--format"), Some("S16"));
}

#[test]
fn spotify_keeps_credentials_but_caches_no_audio() {
    let args = spotify_args("x");
    assert_eq!(value_of(&args, "--system-cache"), Some(SPOTIFY_STATE_DIR));
    assert!(SPOTIFY_STATE_DIR.starts_with("/data/"));
    assert!(args.iter().any(|a| a == "--disable-audio-cache"));
    assert!(
        !args.iter().any(|a| a == "--cache"),
        "no audio cache on the SD card"
    );
}

#[test]
fn spotify_plays_at_full_level_and_reports_its_volume() {
    // The speaker's volume is the only one (DEC-049): librespot doesn't turn
    // the audio down; its volume events reach the Audio Player instead.
    let args = spotify_args("x");
    assert_eq!(value_of(&args, "--volume-ctrl"), Some("fixed"));
    assert_eq!(value_of(&args, "--volume-steps"), Some("100"));
    assert!(value_of(&args, "--onevent").is_some());
}

#[test]
fn a_name_with_spaces_stays_one_argument() {
    let args = spotify_args("Kids Room");
    assert_eq!(value_of(&args, "--name"), Some("Kids Room"));
}

#[test]
fn bluetooth_plays_through_the_shared_output() {
    use audio_player::sources::{bluetooth, bluetooth_args};
    assert_eq!(bluetooth_args(), vec!["--pcm=speaker".to_string()]);
    assert_eq!(bluetooth().name(), "bluealsa-aplay");
}
