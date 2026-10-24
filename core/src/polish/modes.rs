//! Polish modes, translation target, and the polish input/output types
//! (FR-4, FR-8, PROPOSAL §6.1).
//!
//! These types are shared across workstreams (cli, ffi, config): keep their
//! public shape stable during M1 (docs/changes/0005).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::budget::Usage;

/// `Raw` never calls the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Raw,
    Light,
    #[default]
    Polish,
    Formal,
}

/// v1 translation targets (FR-8). Default suggestion is English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Enum)]
pub enum TargetLang {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-TW")]
    ZhTw,
    #[serde(rename = "ja")]
    Ja,
}

impl TargetLang {
    /// BCP-47 code as used in settings and §6.5 (`en`, `zh-TW`, `ja`).
    pub fn code(self) -> &'static str {
        match self {
            TargetLang::En => "en",
            TargetLang::ZhTw => "zh-TW",
            TargetLang::Ja => "ja",
        }
    }
}

#[derive(Clone, Default, uniffi::Record)]
pub struct PolishOptions {
    pub mode: Mode,
    /// `None` = no translation.
    pub translate_to: Option<TargetLang>,
    pub glossary: Vec<String>,
}

impl PolishOptions {
    /// Whether this request needs the API at all.
    pub fn needs_api(&self) -> bool {
        self.mode != Mode::Raw || self.translate_to.is_some()
    }
}

impl fmt::Debug for PolishOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PolishOptions")
            .field("mode", &self.mode)
            .field("translate_to", &self.translate_to)
            .field("glossary_len", &self.glossary.len())
            .finish()
    }
}

/// Why `polish` returned the raw transcript instead of model output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FallbackReason {
    /// Translation requested but the API is not usable in the current state
    /// (raw mode, no key, offline, hard limit): FR-8 "翻譯需要 API".
    TranslationNeedsApi,
    NoKey,
    KeyInvalid,
    /// Local monthly hard limit (FR-6).
    HardLimit,
    /// API reported exhausted credit.
    BudgetExhausted,
    Network,
    Timeout,
    RateLimited,
    /// Any other API or response error.
    ApiError,
    /// Output guard rejected the model output (§6.6).
    GuardRejected,
}

impl FallbackReason {
    /// Map a core error to the user-facing fallback reason.
    pub fn from_error(e: CoreError) -> Self {
        match e {
            CoreError::KeyNotFound => FallbackReason::NoKey,
            CoreError::KeyInvalid | CoreError::KeyFormat => FallbackReason::KeyInvalid,
            CoreError::HardLimitReached => FallbackReason::HardLimit,
            CoreError::BudgetExhausted => FallbackReason::BudgetExhausted,
            CoreError::Network | CoreError::DisallowedHost => FallbackReason::Network,
            CoreError::Timeout => FallbackReason::Timeout,
            CoreError::RateLimited => FallbackReason::RateLimited,
            CoreError::GuardRejected => FallbackReason::GuardRejected,
            _ => FallbackReason::ApiError,
        }
    }
}

/// Output of `polish`. `text` is user content: `Debug` prints its length.
#[derive(Clone, PartialEq, uniffi::Record)]
pub struct PolishResult {
    pub text: String,
    pub fell_back_to_raw: bool,
    pub fallback_reason: Option<FallbackReason>,
    /// `None` when no API call was made.
    pub usage: Option<Usage>,
}

impl PolishResult {
    /// The raw transcript, unchanged, with an optional reason.
    pub fn raw(transcript: &str, reason: Option<FallbackReason>) -> Self {
        Self {
            text: transcript.to_owned(),
            fell_back_to_raw: reason.is_some(),
            fallback_reason: reason,
            usage: None,
        }
    }
}

impl fmt::Debug for PolishResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PolishResult")
            .field("text_len", &self.text.len())
            .field("fell_back_to_raw", &self.fell_back_to_raw)
            .field("fallback_reason", &self.fallback_reason)
            .field("usage", &self.usage)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_lang_serde_uses_codes() {
        for l in [TargetLang::En, TargetLang::ZhTw, TargetLang::Ja] {
            let json = serde_json::to_string(&l).unwrap();
            assert_eq!(json, format!("\"{}\"", l.code()));
        }
    }

    #[test]
    fn debug_hides_content() {
        let o = PolishOptions {
            glossary: vec!["GLOSSARY-MARKER".into()],
            ..Default::default()
        };
        assert!(!format!("{o:?}").contains("MARKER"));
        let r = PolishResult::raw("TEXT-MARKER", None);
        assert!(!format!("{r:?}").contains("MARKER"));
    }
}
