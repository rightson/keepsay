//! whisper-rs engine. Metal on Apple targets (enabled in core/Cargo.toml).
//!
//! Owner: workstream E (docs/changes/0005). Call
//! `whisper_rs::install_logging_hooks()` before loading a model: with neither
//! `log_backend` nor `tracing_backend` enabled it discards whisper.cpp / GGML
//! output, which could otherwise reach stderr.

use std::path::Path;

use super::{SttEngine, SttOptions};
use crate::CoreError;

/// A loaded whisper model. The model file must already have passed the
/// manifest SHA-256 check (INV-7).
pub struct WhisperEngine {
    ctx: whisper_rs::WhisperContext,
}

impl WhisperEngine {
    pub fn load(model_path: &Path) -> Result<Self, CoreError> {
        let _ = model_path;
        todo!("M1: stt — load whisper model")
    }
}

impl SttEngine for WhisperEngine {
    fn transcribe(
        &self,
        pcm: &[f32],
        sample_rate: u32,
        opts: &SttOptions,
    ) -> Result<String, CoreError> {
        let _ = (&self.ctx, pcm, sample_rate, opts);
        todo!("M1: stt — whisper transcribe")
    }
}
