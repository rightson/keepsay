//! Local speech-to-text (PROPOSAL §6.7). Audio stays in memory (INV-5).
//!
//! Owner: workstream E (docs/changes/0005). The whisper engine is behind the
//! `whisper` cargo feature so the default build does not compile whisper.cpp.

#[cfg(feature = "whisper")]
pub mod whisper;

use std::fmt;

use crate::CoreError;

/// whisper.cpp expects 16 kHz mono f32 PCM.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Clone, Default)]
pub struct SttOptions {
    /// BCP-47 hint, e.g. "zh-TW".
    pub language: Option<String>,
    /// Injected into the initial prompt (FR-7). Private: never logged.
    pub glossary: Vec<String>,
}

impl fmt::Debug for SttOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SttOptions")
            .field("language", &self.language)
            .field("glossary_len", &self.glossary.len())
            .finish()
    }
}

pub trait SttEngine {
    fn transcribe(
        &self,
        pcm: &[f32],
        sample_rate: u32,
        opts: &SttOptions,
    ) -> Result<String, CoreError>;
}
