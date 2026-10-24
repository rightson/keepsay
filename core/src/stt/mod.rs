//! Local speech-to-text (PROPOSAL §6.7). Audio stays in memory (INV-5).

pub mod whisper;

use crate::CoreError;

#[derive(Debug, Clone, Default)]
pub struct SttOptions {
    /// BCP-47 hint, e.g. "zh-TW".
    pub language: Option<String>,
    pub glossary: Vec<String>,
}

pub trait SttEngine {
    fn transcribe(
        &self,
        pcm: &[f32],
        sample_rate: u32,
        opts: &SttOptions,
    ) -> Result<String, CoreError>;
}
