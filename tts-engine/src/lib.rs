//! Text-to-speech for the speaker. See the TTS Engine solution design.

pub mod cache;
pub mod synth;

pub use cache::{Lookup, TtsCache, DEFAULT_TTL};
pub use synth::{Engine, Synthesizer, TtsError};
