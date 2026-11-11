//! Synthesis backends: command-line engines that write a WAV file. The engine is
//! still to be chosen (KDS-o2d); Piper can't run on the Pi Zero W.

use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{engine} failed: {detail}")]
    Engine { engine: String, detail: String },
    #[error("{0} produced no audio")]
    NoAudio(String),
}

pub trait Synthesizer: Send + Sync {
    /// Identifies the engine and voice, so the cache never mixes their output.
    fn id(&self) -> String;
    /// Writes the spoken `text` as a WAV file at `output`.
    fn synthesize(&self, text: &str, output: &Path) -> Result<(), TtsError>;
}

/// A command-line engine and voice, measured on the Pi Zero W.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engine {
    /// espeak-ng: fastest, 22 kHz, robotic.
    EspeakNg { voice: String },
    /// espeak-ng read slowly: `speed` words a minute (default 175) and
    /// `word_gap` tens of milliseconds between words, so digits come one at
    /// a time (the setup code, DEC-038).
    EspeakNgSlow {
        voice: String,
        speed: u32,
        word_gap: u32,
    },
    /// flite: 16 kHz voices awb, rms and slt sound more natural; kal is 8 kHz.
    Flite { voice: String },
}

impl Engine {
    /// The program and its arguments. The text is one argument and no shell is
    /// involved, so it can't be interpreted as commands.
    pub fn command(&self, text: &str, output: &Path) -> (String, Vec<String>) {
        let output = output.to_string_lossy().into_owned();
        match self {
            Engine::EspeakNg { voice } => (
                "espeak-ng".into(),
                vec![
                    "-v".into(),
                    voice.clone(),
                    "-w".into(),
                    output,
                    "--".into(),
                    text.into(),
                ],
            ),
            Engine::EspeakNgSlow {
                voice,
                speed,
                word_gap,
            } => (
                "espeak-ng".into(),
                vec![
                    "-v".into(),
                    voice.clone(),
                    "-s".into(),
                    speed.to_string(),
                    "-g".into(),
                    word_gap.to_string(),
                    "-w".into(),
                    output,
                    "--".into(),
                    text.into(),
                ],
            ),
            Engine::Flite { voice } => (
                "flite".into(),
                vec![
                    "-voice".into(),
                    voice.clone(),
                    "-t".into(),
                    text.into(),
                    "-o".into(),
                    output,
                ],
            ),
        }
    }
}

impl Synthesizer for Engine {
    fn id(&self) -> String {
        match self {
            Engine::EspeakNg { voice } => format!("espeak-ng:{voice}"),
            Engine::EspeakNgSlow {
                voice,
                speed,
                word_gap,
            } => format!("espeak-ng:{voice}:s{speed}:g{word_gap}"),
            Engine::Flite { voice } => format!("flite:{voice}"),
        }
    }

    fn synthesize(&self, text: &str, output: &Path) -> Result<(), TtsError> {
        let (program, args) = self.command(text, output);
        let result = Command::new(&program).args(&args).output()?;
        if !result.status.success() {
            return Err(TtsError::Engine {
                engine: self.id(),
                detail: String::from_utf8_lossy(&result.stderr).trim().to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_name_the_engine_and_voice() {
        assert_eq!(Engine::EspeakNg { voice: "en".into() }.id(), "espeak-ng:en");
        assert_eq!(
            Engine::Flite {
                voice: "slt".into()
            }
            .id(),
            "flite:slt"
        );
    }

    #[test]
    fn text_is_a_single_argument() {
        let text = "Hello; rm -rf / && echo $HOME";
        let (program, args) = Engine::Flite {
            voice: "slt".into(),
        }
        .command(text, Path::new("/tmp/x.wav"));
        assert_eq!(program, "flite");
        assert_eq!(args, ["-voice", "slt", "-t", text, "-o", "/tmp/x.wav"]);
    }

    #[test]
    fn espeak_text_after_option_terminator() {
        // A message starting with "-" must not be read as an option.
        let (_, args) =
            Engine::EspeakNg { voice: "en".into() }.command("-v hi", Path::new("/o.wav"));
        assert_eq!(args, ["-v", "en", "-w", "/o.wav", "--", "-v hi"]);
    }

    #[test]
    fn slow_espeak_sets_speed_and_word_gap_and_has_its_own_id() {
        let slow = Engine::EspeakNgSlow {
            voice: "da".into(),
            speed: 140,
            word_gap: 60,
        };
        let (_, args) = slow.command("4, 7", Path::new("/o.wav"));
        assert_eq!(
            args,
            ["-v", "da", "-s", "140", "-g", "60", "-w", "/o.wav", "--", "4, 7"]
        );
        // A different id keeps slow and normal speech apart in the cache.
        assert_eq!(slow.id(), "espeak-ng:da:s140:g60");
    }
}
