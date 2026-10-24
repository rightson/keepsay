//! US-M1-17 ③: run the whisper engine end to end on generated audio.
//!
//! Needs a ggml model on disk, which tests never download. Point
//! `KEEPSAY_TEST_WHISPER_MODEL` at one (for example ggml-tiny.bin) and run
//! `cargo test -p keepsay-core --features whisper --test stt_whisper`.
//! Without it the test prints why and passes. Only lengths are checked: the
//! content of a transcript is never asserted (CLAUDE.md).
#![cfg(feature = "whisper")]

use std::path::{Path, PathBuf};

use keepsay_core::stt::whisper::WhisperEngine;
use keepsay_core::{CoreError, SttEngine, SttOptions};

fn model() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("KEEPSAY_TEST_WHISPER_MODEL")?);
    p.is_file().then_some(p)
}

/// One second of a quiet 440 Hz tone at 48 kHz, so the resampler runs too.
fn tone_48k() -> Vec<f32> {
    (0..48_000)
        .map(|i| 0.1 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin())
        .collect()
}

#[test]
fn transcribes_generated_audio_when_a_model_is_available() {
    let Some(path) = model() else {
        eprintln!("skipped: set KEEPSAY_TEST_WHISPER_MODEL to a ggml model file to run");
        return;
    };
    let engine = WhisperEngine::load(&path).expect("model loads");
    let opts = SttOptions {
        language: Some("zh-TW".into()),
        glossary: vec!["keepsay".into()],
    };
    let text = engine.transcribe(&tone_48k(), 48_000, &opts).expect("runs");
    // Content is irrelevant for a tone; only bound the length.
    assert!(text.chars().count() < 1_000);
    assert_eq!(engine.transcribe(&[], 16_000, &opts), Ok(String::new()));
    assert_eq!(
        engine.transcribe(&[0.0; 10], 1_000, &opts),
        Err(CoreError::AudioFormat)
    );
}

#[test]
fn missing_model_file_is_an_error() {
    let r = WhisperEngine::load(Path::new("/nonexistent/keepsay-model.bin"));
    assert!(matches!(
        r,
        Err(CoreError::Io(std::io::ErrorKind::NotFound))
    ));
}
