use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Airplay,
    Spotify,
    Bluetooth,
    Radio,
}

impl Source {
    pub const ALL: [Source; 4] = [
        Source::Airplay,
        Source::Spotify,
        Source::Bluetooth,
        Source::Radio,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Source::Airplay => "airplay",
            Source::Spotify => "spotify",
            Source::Bluetooth => "bluetooth",
            Source::Radio => "radio",
        }
    }

    pub fn parse(name: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|s| s.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    Playing,
    Paused,
    Stopped,
}

pub struct Arbitrator {
    states: HashMap<Source, SourceState>,
}

impl Arbitrator {
    pub fn new() -> Self {
        let mut states = HashMap::new();
        for source in Source::ALL {
            states.insert(source, SourceState::Stopped);
        }
        Arbitrator { states }
    }

    pub fn source_started(&mut self, src: Source) -> Vec<(Source, SourceState)> {
        // The started source becomes Playing
        self.states.insert(src, SourceState::Playing);

        let mut transitions = vec![(src, SourceState::Playing)];

        // Pause all other sources that are currently Playing
        for (source, state) in self.states.iter_mut() {
            if *source != src && *state == SourceState::Playing {
                *state = SourceState::Paused;
                transitions.push((*source, SourceState::Paused));
            }
        }

        transitions
    }

    pub fn source_stopped(&mut self, src: Source) -> Vec<(Source, SourceState)> {
        self.states.insert(src, SourceState::Stopped);
        vec![(src, SourceState::Stopped)]
    }

    pub fn active(&self) -> Option<Source> {
        for (source, state) in self.states.iter() {
            if *state == SourceState::Playing {
                return Some(*source);
            }
        }
        None
    }

    pub fn state_of(&self, src: &Source) -> SourceState {
        self.states
            .get(src)
            .copied()
            .unwrap_or(SourceState::Stopped)
    }
}

impl Default for Arbitrator {
    fn default() -> Self {
        Self::new()
    }
}
