//! Local speech-to-text (PROPOSAL §6.7). Audio stays in memory (INV-5).
//!
//! Owner: workstream E (docs/changes/0005). The whisper engine is behind the
//! `whisper` cargo feature so the default build does not compile whisper.cpp.
//!
//! The helpers here (resampling, language mapping, initial prompt) have no
//! whisper dependency so they are tested in the default build. Nothing in
//! this module touches the filesystem; `core/tests/no_disk_writes.rs`
//! text-scans it to keep it that way (US-M1-25).

#[cfg(feature = "whisper")]
pub mod whisper;

use std::fmt;

use crate::CoreError;

/// whisper.cpp expects 16 kHz mono f32 PCM.
pub const SAMPLE_RATE: u32 = 16_000;

/// Lowest and highest input rates [`resample_linear`] accepts.
pub const MIN_INPUT_RATE: u32 = 8_000;
pub const MAX_INPUT_RATE: u32 = 192_000;

/// Upper bound on audio length after resampling: 30 minutes at 16 kHz.
/// Dictation is seconds long; this only stops absurd inputs (and keeps the
/// sample count far below `c_int::MAX` for whisper.cpp).
pub const MAX_SAMPLES: usize = 30 * 60 * SAMPLE_RATE as usize;

/// whisper language used when `SttOptions::language` is `None` (FR-3:
/// Taiwan Mandarin with English mixed in; "zh" handles code-switching).
pub const DEFAULT_LANGUAGE: &str = "zh";

/// Character budget for glossary terms in the initial prompt. whisper only
/// conditions on roughly the last 224 prompt tokens, so a longer glossary
/// would push out the Traditional Chinese example sentence.
pub const GLOSSARY_PROMPT_BUDGET: usize = 200;

/// Example sentence that steers whisper towards Traditional Chinese with
/// full-width punctuation and English terms left as-is (§6.7).
pub const ZH_TW_PROMPT: &str = "以下是台灣繁體中文的逐字稿，英文術語保持原樣，例如：我們明天下午開會，討論 API 的設計和 release 時程。";

#[derive(Clone, Default, uniffi::Record)]
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

/// Convert mono PCM at `from_rate` to [`SAMPLE_RATE`] by linear
/// interpolation.
///
/// Deliberately simple: no anti-aliasing low-pass before downsampling, so
/// energy above 8 kHz in 44.1 / 48 kHz input folds back into the band. For
/// speech recognition this is a minor loss; the platform recorders already
/// capture at 16 kHz, so this path only serves WAV files fed to the CLI.
///
/// Output length is `round(len * 16000 / from_rate)`. Non-finite samples
/// become 0.0. Rates outside [`MIN_INPUT_RATE`]..=[`MAX_INPUT_RATE`] and
/// results longer than [`MAX_SAMPLES`] are `CoreError::AudioFormat`.
pub fn resample_linear(pcm: &[f32], from_rate: u32) -> Result<Vec<f32>, CoreError> {
    if !(MIN_INPUT_RATE..=MAX_INPUT_RATE).contains(&from_rate) {
        return Err(CoreError::AudioFormat);
    }
    let clean = |x: f32| if x.is_finite() { x } else { 0.0 };
    if from_rate == SAMPLE_RATE {
        if pcm.len() > MAX_SAMPLES {
            return Err(CoreError::AudioFormat);
        }
        return Ok(pcm.iter().copied().map(clean).collect());
    }
    let ratio = f64::from(from_rate) / f64::from(SAMPLE_RATE);
    let out_len = (pcm.len() as f64 / ratio).round() as usize;
    if out_len > MAX_SAMPLES {
        return Err(CoreError::AudioFormat);
    }
    let Some(last) = pcm.len().checked_sub(1) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = (pos.floor() as usize).min(last);
        let next = (idx + 1).min(last);
        let frac = (pos - idx as f64).clamp(0.0, 1.0) as f32;
        let a = clean(pcm[idx]);
        let b = clean(pcm[next]);
        out.push(a + (b - a) * frac);
    }
    Ok(out)
}

/// Map a BCP-47 hint to a whisper language code: the primary subtag,
/// lowercased (`"zh-TW"` → `"zh"`). `None` → [`DEFAULT_LANGUAGE`];
/// `"auto"` → `None` (whisper auto-detects). Anything that is not 2–3 ASCII
/// letters is `CoreError::Config`.
pub fn whisper_language(hint: Option<&str>) -> Result<Option<String>, CoreError> {
    let Some(hint) = hint else {
        return Ok(Some(DEFAULT_LANGUAGE.to_owned()));
    };
    if hint.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }
    let primary = hint.split(['-', '_']).next().unwrap_or_default();
    if (2..=3).contains(&primary.len()) && primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        Ok(Some(primary.to_ascii_lowercase()))
    } else {
        Err(CoreError::Config)
    }
}

/// The whisper initial prompt: [`ZH_TW_PROMPT`] plus the glossary (FR-7).
///
/// Terms are trimmed, control characters (including NUL, which whisper-rs
/// would panic on) are removed, empty terms are skipped, and terms are added
/// whole until [`GLOSSARY_PROMPT_BUDGET`] characters are used. The result is
/// private user data: never log it, only its length.
pub fn initial_prompt(glossary: &[String]) -> String {
    let mut terms: Vec<String> = Vec::new();
    let mut used = 0usize;
    for term in glossary {
        let clean: String = term.chars().filter(|c| !c.is_control()).collect();
        let clean = clean.trim();
        if clean.is_empty() {
            continue;
        }
        let n = clean.chars().count();
        if used + n > GLOSSARY_PROMPT_BUDGET {
            continue;
        }
        used += n;
        terms.push(clean.to_owned());
    }
    if terms.is_empty() {
        ZH_TW_PROMPT.to_owned()
    } else {
        format!("{ZH_TW_PROMPT}詞彙：{}。", terms.join("、"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_is_identity() {
        let x = vec![0.0, 0.5, -0.5, 1.0];
        assert_eq!(resample_linear(&x, SAMPLE_RATE).unwrap(), x);
    }

    #[test]
    fn resample_lengths() {
        let x = vec![0.0f32; 48_000];
        assert_eq!(resample_linear(&x, 48_000).unwrap().len(), 16_000);
        let x = vec![0.0f32; 44_100];
        assert_eq!(resample_linear(&x, 44_100).unwrap().len(), 16_000);
        let x = vec![0.0f32; 8_000];
        assert_eq!(resample_linear(&x, 8_000).unwrap().len(), 16_000);
        assert!(resample_linear(&[], 48_000).unwrap().is_empty());
    }

    #[test]
    fn resample_interpolates() {
        // 8 kHz ramp → 16 kHz: midpoints appear between original samples.
        let out = resample_linear(&[0.0, 1.0, 2.0], 8_000).unwrap();
        assert_eq!(out.len(), 6);
        assert_eq!(&out[..5], &[0.0, 0.5, 1.0, 1.5, 2.0]);
        // Past the end it holds the last sample.
        assert_eq!(out[5], 2.0);
    }

    #[test]
    fn resample_rejects_bad_rates_and_cleans_nan() {
        assert_eq!(resample_linear(&[0.0], 0), Err(CoreError::AudioFormat));
        assert_eq!(resample_linear(&[0.0], 7_999), Err(CoreError::AudioFormat));
        assert_eq!(
            resample_linear(&[0.0], 192_001),
            Err(CoreError::AudioFormat)
        );
        let out = resample_linear(&[f32::NAN, f32::INFINITY], SAMPLE_RATE).unwrap();
        assert_eq!(out, vec![0.0, 0.0]);
    }

    #[test]
    fn resample_rejects_too_long() {
        let x = vec![0.0f32; MAX_SAMPLES + 1];
        assert_eq!(
            resample_linear(&x, SAMPLE_RATE),
            Err(CoreError::AudioFormat)
        );
    }

    #[test]
    fn language_mapping() {
        assert_eq!(whisper_language(None).unwrap().as_deref(), Some("zh"));
        assert_eq!(
            whisper_language(Some("zh-TW")).unwrap().as_deref(),
            Some("zh")
        );
        assert_eq!(whisper_language(Some("EN")).unwrap().as_deref(), Some("en"));
        assert_eq!(
            whisper_language(Some("ja_JP")).unwrap().as_deref(),
            Some("ja")
        );
        assert_eq!(whisper_language(Some("auto")).unwrap(), None);
        assert_eq!(whisper_language(Some("")), Err(CoreError::Config));
        assert_eq!(whisper_language(Some("z\0")), Err(CoreError::Config));
        assert_eq!(whisper_language(Some("chinese")), Err(CoreError::Config));
    }

    // Assertions are structural only (lengths, separators, NUL), never on
    // glossary content (CLAUDE.md).
    #[test]
    fn prompt_without_glossary_is_the_example() {
        assert_eq!(initial_prompt(&[]), ZH_TW_PROMPT);
        assert_eq!(initial_prompt(&[" ".into(), "\0".into()]), ZH_TW_PROMPT);
    }

    #[test]
    fn prompt_includes_glossary_structurally() {
        let g = vec!["a1".to_owned(), "b\0b".to_owned(), "c3".to_owned()];
        let p = initial_prompt(&g);
        assert!(p.starts_with(ZH_TW_PROMPT));
        assert!(!p.contains('\0'));
        // Three terms → two separators.
        assert_eq!(p.matches('、').count(), 2);
        assert_eq!(
            p.chars().count(),
            // label + three 2-char terms (NUL removed) + 2 separators + "。"
            ZH_TW_PROMPT.chars().count() + "詞彙：".chars().count() + 3 * 2 + 2 + 1
        );
    }

    #[test]
    fn prompt_respects_budget() {
        let g: Vec<String> = (0..100).map(|_| "x".repeat(30)).collect();
        let p = initial_prompt(&g);
        let extra = p.chars().count() - ZH_TW_PROMPT.chars().count();
        assert!(extra <= GLOSSARY_PROMPT_BUDGET + 64);
    }
}
