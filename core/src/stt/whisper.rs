//! whisper-rs engine. Metal on Apple targets (enabled in core/Cargo.toml).
//!
//! Owner: workstream E (docs/changes/0005).
//!
//! - `whisper_rs::install_logging_hooks()` runs before any model load: with
//!   neither `log_backend` nor `tracing_backend` enabled it discards
//!   whisper.cpp / GGML output, which could otherwise reach stderr.
//! - All `print_*` params are off, so whisper.cpp never prints the
//!   transcript (it would otherwise go straight to stdout).
//! - Never the `translate` task: translation happens in polish (§6.7, ADR-9).
//! - Audio stays in memory; nothing here writes to disk (INV-5).
//! - Logs carry sample counts, lengths and timings only.

use std::path::Path;
use std::time::Instant;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{SttEngine, SttOptions, initial_prompt, resample_linear, whisper_language};
use crate::CoreError;

/// A loaded whisper model. The model file must already have passed the
/// manifest SHA-256 check (INV-7).
pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    /// Load a ggml model file. The path is never logged or put in an error.
    pub fn load(model_path: &Path) -> Result<Self, CoreError> {
        whisper_rs::install_logging_hooks();
        if !model_path.is_file() {
            return Err(CoreError::Io(std::io::ErrorKind::NotFound));
        }
        let started = Instant::now();
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|_| CoreError::Stt)?;
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "stt model loaded"
        );
        Ok(Self { ctx })
    }
}

fn thread_count() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4) as i32
}

impl SttEngine for WhisperEngine {
    /// Mono f32 PCM at any rate in `MIN_INPUT_RATE..=MAX_INPUT_RATE`; it is
    /// resampled to 16 kHz first. Empty input yields an empty transcript.
    fn transcribe(
        &self,
        pcm: &[f32],
        sample_rate: u32,
        opts: &SttOptions,
    ) -> Result<String, CoreError> {
        let audio = resample_linear(pcm, sample_rate)?;
        if audio.is_empty() {
            return Ok(String::new());
        }
        let language = whisper_language(opts.language.as_deref())?;
        let prompt = initial_prompt(&opts.glossary);

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(thread_count());
        params.set_translate(false);
        params.set_language(language.as_deref());
        params.set_initial_prompt(&prompt);
        params.set_no_timestamps(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);

        let started = Instant::now();
        let mut state = self.ctx.create_state().map_err(|_| CoreError::Stt)?;
        state.full(params, &audio).map_err(|_| CoreError::Stt)?;

        let mut text = String::new();
        for segment in state.as_iter() {
            let s = segment.to_str_lossy().map_err(|_| CoreError::Stt)?;
            text.push_str(&s);
        }
        let text = text.trim().to_owned();
        tracing::info!(
            input_samples = pcm.len(),
            input_rate = sample_rate,
            samples_16k = audio.len(),
            prompt_chars = prompt.chars().count(),
            output_chars = text.chars().count(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "stt done"
        );
        Ok(text)
    }
}
