use audio_player::arbitrator::{Arbitrator, Source, SourceState};

#[test]
fn test_airplay_starts_first() {
    let mut arb = Arbitrator::new();

    let transitions = arb.source_started(Source::Airplay);

    // Airplay should be Playing
    assert_eq!(arb.state_of(&Source::Airplay), SourceState::Playing);
    // Spotify should remain Stopped
    assert_eq!(arb.state_of(&Source::Spotify), SourceState::Stopped);
    // Active should be Airplay
    assert_eq!(arb.active(), Some(Source::Airplay));

    // Verify the transitions returned
    assert!(transitions.iter().any(|(s, st)| *s == Source::Airplay && *st == SourceState::Playing));
}

#[test]
fn test_spotify_wins_over_airplay() {
    let mut arb = Arbitrator::new();

    // Start Airplay
    arb.source_started(Source::Airplay);
    assert_eq!(arb.active(), Some(Source::Airplay));

    // Start Spotify - it should win
    let transitions = arb.source_started(Source::Spotify);

    // Spotify should be Playing
    assert_eq!(arb.state_of(&Source::Spotify), SourceState::Playing);
    // Airplay should be Paused
    assert_eq!(arb.state_of(&Source::Airplay), SourceState::Paused);
    // Active should be Spotify
    assert_eq!(arb.active(), Some(Source::Spotify));

    // Verify the transitions
    assert!(transitions.iter().any(|(s, st)| *s == Source::Spotify && *st == SourceState::Playing));
    assert!(transitions.iter().any(|(s, st)| *s == Source::Airplay && *st == SourceState::Paused));
}

#[test]
fn test_airplay_wins_over_spotify() {
    let mut arb = Arbitrator::new();

    // Start Spotify
    arb.source_started(Source::Spotify);
    assert_eq!(arb.active(), Some(Source::Spotify));

    // Start Airplay - it should win
    let transitions = arb.source_started(Source::Airplay);

    // Airplay should be Playing
    assert_eq!(arb.state_of(&Source::Airplay), SourceState::Playing);
    // Spotify should be Paused
    assert_eq!(arb.state_of(&Source::Spotify), SourceState::Paused);
    // Active should be Airplay
    assert_eq!(arb.active(), Some(Source::Airplay));

    // Verify the transitions
    assert!(transitions.iter().any(|(s, st)| *s == Source::Airplay && *st == SourceState::Playing));
    assert!(transitions.iter().any(|(s, st)| *s == Source::Spotify && *st == SourceState::Paused));
}

#[test]
fn test_source_stopped_clears_active() {
    let mut arb = Arbitrator::new();

    // Start Airplay
    arb.source_started(Source::Airplay);
    assert_eq!(arb.active(), Some(Source::Airplay));

    // Stop Airplay
    arb.source_stopped(Source::Airplay);

    // Should be no active source now
    assert_eq!(arb.active(), None);
    assert_eq!(arb.state_of(&Source::Airplay), SourceState::Stopped);
}

#[test]
fn test_active_returns_none_when_idle() {
    let arb = Arbitrator::new();

    // Both sources should be Stopped initially
    assert_eq!(arb.state_of(&Source::Airplay), SourceState::Stopped);
    assert_eq!(arb.state_of(&Source::Spotify), SourceState::Stopped);
    // Active should be None
    assert_eq!(arb.active(), None);
}
