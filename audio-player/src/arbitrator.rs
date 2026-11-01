use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Airplay,
    Spotify,
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
        states.insert(Source::Airplay, SourceState::Stopped);
        states.insert(Source::Spotify, SourceState::Stopped);
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
