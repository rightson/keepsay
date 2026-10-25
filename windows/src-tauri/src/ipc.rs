//! Types that cross the IPC boundary to the WebView (PROPOSAL §7.3, INV-2).
//!
//! Every IPC command returns one of the types below (or a `Vec` of them)
//! inside `Result<_, IpcError>`. None of them can hold the key: key state is
//! [`KeyView`], whose only key-derived field is the fingerprint
//! (`vault::Fingerprint`, `…a1B2`). `tests/ipc_surface.rs` enforces that
//! command return types stay on this list.
//!
//! Errors are fixed codes; the UI maps them to text. They never carry user
//! content, paths or upstream messages.

use keepsay_core::config::{BudgetLimits, Config, Hotkeys, PriceTable, WINDOWS_HOTKEYS};
use keepsay_core::{BudgetStatus, CoreError, FallbackReason, Mode, TargetLang};
use serde::{Deserialize, Serialize};

use crate::hotkey::{HotkeyError, parse_pair};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IpcError {
    pub code: &'static str,
}

impl IpcError {
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl From<CoreError> for IpcError {
    fn from(e: CoreError) -> Self {
        IpcError::new(core_error_code(e))
    }
}

impl From<HotkeyError> for IpcError {
    fn from(e: HotkeyError) -> Self {
        IpcError::new(e.code())
    }
}

pub fn core_error_code(e: CoreError) -> &'static str {
    match e {
        CoreError::KeyInvalid => "key_invalid",
        CoreError::KeyFormat => "key_format",
        CoreError::KeyNotFound => "key_not_found",
        CoreError::VaultUnavailable => "vault_unavailable",
        CoreError::VaultNotEntitled => "vault_not_entitled",
        CoreError::NotInitialized => "not_initialized",
        CoreError::DisallowedHost => "disallowed_host",
        CoreError::Network => "network",
        CoreError::Timeout => "timeout",
        CoreError::RateLimited => "rate_limited",
        CoreError::BudgetExhausted => "budget_exhausted",
        CoreError::ApiStatus(_) => "api_status",
        CoreError::InvalidResponse => "invalid_response",
        CoreError::ModelNotAllowed => "model_not_allowed",
        CoreError::GuardRejected => "guard_rejected",
        CoreError::HardLimitReached => "hard_limit",
        CoreError::AudioFormat => "audio_format",
        CoreError::Stt => "stt",
        CoreError::ModelUnknown => "model_unknown",
        CoreError::ModelHash => "model_hash",
        CoreError::ModelTooLarge => "model_too_large",
        CoreError::Config => "config",
        CoreError::Io(_) => "io",
        CoreError::Unsupported => "unsupported",
        #[allow(unreachable_patterns)]
        _ => "core",
    }
}

pub fn fallback_code(r: FallbackReason) -> &'static str {
    match r {
        FallbackReason::TranslationNeedsApi => "translation_needs_api",
        FallbackReason::NoKey => "no_key",
        FallbackReason::KeyInvalid => "key_invalid",
        FallbackReason::HardLimit => "hard_limit",
        FallbackReason::BudgetExhausted => "budget_exhausted",
        FallbackReason::Network => "network",
        FallbackReason::Timeout => "timeout",
        FallbackReason::RateLimited => "rate_limited",
        FallbackReason::ApiError => "api_error",
        FallbackReason::GuardRejected => "guard_rejected",
    }
}

/// Key state. The fingerprint is the only key-derived value (INV-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyView {
    pub has_key: bool,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsView {
    pub mode: Mode,
    pub translate_to: TargetLang,
    pub glossary: Vec<String>,
    pub hotkey_polish: String,
    pub hotkey_translate: String,
    pub stt_model: Option<String>,
    pub prices: PriceTable,
    pub budget: BudgetLimits,
}

impl SettingsView {
    pub fn from_config(c: &Config) -> Self {
        let (hotkey_polish, hotkey_translate) = c.hotkeys.resolve(WINDOWS_HOTKEYS);
        Self {
            mode: c.mode,
            translate_to: c.translate_to,
            glossary: c.glossary.clone(),
            hotkey_polish,
            hotkey_translate,
            stt_model: c.stt_model.clone(),
            prices: c.prices,
            budget: c.budget,
        }
    }
}

/// What the settings page sends back. Everything else in `Config` (API
/// model, proxy) is not editable from the WebView.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsInput {
    pub mode: Mode,
    pub translate_to: TargetLang,
    pub glossary: Vec<String>,
    pub hotkey_polish: String,
    pub hotkey_translate: String,
    pub stt_model: Option<String>,
    pub prices: PriceTable,
    pub budget: BudgetLimits,
}

pub const GLOSSARY_MAX_ENTRIES: usize = 200;
pub const GLOSSARY_MAX_CHARS: usize = 64;

impl SettingsInput {
    /// Merge into `base`, validating everything. Hotkeys equal to the
    /// platform default are stored as `None` (follow future defaults).
    pub fn apply(self, base: &Config) -> Result<Config, IpcError> {
        let (p, t) = parse_pair(&self.hotkey_polish, &self.hotkey_translate)?;
        let (p, t) = (p.to_string(), t.to_string());
        let mut cfg = base.clone();
        cfg.mode = self.mode;
        cfg.translate_to = self.translate_to;
        cfg.glossary = normalize_glossary(self.glossary)?;
        cfg.hotkeys = Hotkeys {
            polish: (p != WINDOWS_HOTKEYS.polish).then_some(p),
            polish_translate: (t != WINDOWS_HOTKEYS.polish_translate).then_some(t),
        };
        cfg.stt_model = self.stt_model;
        cfg.prices = self.prices;
        cfg.budget = self.budget;
        cfg.validate()?;
        Ok(cfg)
    }
}

/// Trim, drop empties and duplicates, enforce size limits.
pub fn normalize_glossary(items: Vec<String>) -> Result<Vec<String>, IpcError> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let t = item.trim();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > GLOSSARY_MAX_CHARS || t.chars().any(char::is_control) {
            return Err(IpcError::new("glossary_entry_invalid"));
        }
        if !out.iter().any(|o| o == t) {
            out.push(t.to_owned());
        }
    }
    if out.len() > GLOSSARY_MAX_ENTRIES {
        return Err(IpcError::new("glossary_too_long"));
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BudgetView {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_usd: f64,
    pub soft_limit_usd: Option<f64>,
    pub hard_limit_usd: Option<f64>,
    pub soft_limited: bool,
    pub hard_limited: bool,
    /// The ledger file could not be read; treated as at the hard limit.
    pub ledger_unreadable: bool,
}

impl BudgetView {
    pub fn new(s: &BudgetStatus, ledger_unreadable: bool) -> Self {
        Self {
            input_tokens: s.month_usage.input_tokens,
            output_tokens: s.month_usage.output_tokens,
            estimated_usd: s.estimated_usd,
            soft_limit_usd: s.soft_limit_usd,
            hard_limit_usd: s.hard_limit_usd,
            soft_limited: s.soft_limited(),
            hard_limited: ledger_unreadable || s.hard_limited(),
            ledger_unreadable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    /// A file exists at the model path.
    pub present: bool,
    /// Its SHA-256 matched the manifest in this session.
    pub verified: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    Idle,
    Downloading,
    Verifying,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadView {
    pub id: Option<String>,
    pub phase: DownloadPhase,
    pub received: u64,
    pub total: u64,
    pub error: Option<&'static str>,
}

impl Default for DownloadView {
    fn default() -> Self {
        Self {
            id: None,
            phase: DownloadPhase::Idle,
            received: 0,
            total: 0,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HotkeyStatusView {
    pub polish: String,
    pub translate: String,
    pub polish_registered: bool,
    pub translate_registered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPhase {
    Hidden,
    Recording,
    Transcribing,
    Polishing,
    Inserted,
    Fallback,
    Error,
}

/// What the recording overlay shows. Codes and numbers only: no transcript
/// or model output ever reaches the overlay.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverlayView {
    pub phase: OverlayPhase,
    pub translate: bool,
    pub mode: Mode,
    pub elapsed_ms: u64,
    /// 0.0 – 1.0 input level while recording.
    pub level: f32,
    /// Hotkey that stops the recording, for the hint.
    pub stop_hint: String,
    /// Fallback reason or error code.
    pub notice: Option<&'static str>,
    pub at_cap: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> SettingsInput {
        SettingsInput {
            mode: Mode::Formal,
            translate_to: TargetLang::Ja,
            glossary: vec![
                " keepsay ".into(),
                "".into(),
                "keepsay".into(),
                "Kevin".into(),
            ],
            hotkey_polish: "shift+alt+z".into(),
            hotkey_translate: "Ctrl+Shift+F9".into(),
            stt_model: Some("whisper-small".into()),
            prices: PriceTable::default(),
            budget: BudgetLimits::default(),
        }
    }

    #[test]
    fn apply_normalizes() {
        let base = Config::default();
        let cfg = input().apply(&base).unwrap();
        assert_eq!(cfg.mode, Mode::Formal);
        assert_eq!(cfg.glossary, vec!["keepsay".to_owned(), "Kevin".to_owned()]);
        // Default polish hotkey is stored as "follow the default".
        assert_eq!(cfg.hotkeys.polish, None);
        assert_eq!(
            cfg.hotkeys.polish_translate.as_deref(),
            Some("Ctrl+Shift+F9")
        );
        // Fields the WebView cannot edit are kept.
        assert_eq!(cfg.model, base.model);
        assert_eq!(cfg.proxy, base.proxy);
        let view = SettingsView::from_config(&cfg);
        assert_eq!(view.hotkey_polish, "Alt+Shift+Z");
    }

    #[test]
    fn apply_rejects() {
        let base = Config::default();
        let mut i = input();
        i.hotkey_translate = "Ctrl+Alt+X".into();
        assert_eq!(i.apply(&base).unwrap_err().code, "hotkey_ctrl_alt");
        let mut i = input();
        i.glossary = vec!["x".repeat(GLOSSARY_MAX_CHARS + 1)];
        assert_eq!(i.apply(&base).unwrap_err().code, "glossary_entry_invalid");
        let mut i = input();
        i.glossary = (0..=GLOSSARY_MAX_ENTRIES).map(|n| n.to_string()).collect();
        assert_eq!(i.apply(&base).unwrap_err().code, "glossary_too_long");
        let mut i = input();
        i.prices.input_usd_per_mtok = -1.0;
        assert_eq!(i.apply(&base).unwrap_err().code, "config");
        let mut i = input();
        i.stt_model = Some(String::new());
        assert_eq!(i.apply(&base).unwrap_err().code, "config");
    }

    #[test]
    fn input_rejects_unknown_fields() {
        let json = r#"{"mode":"raw","translate_to":"en","glossary":[],"hotkey_polish":"Alt+Shift+Z",
            "hotkey_translate":"Alt+Shift+X","stt_model":null,
            "prices":{"input_usd_per_mtok":1,"output_usd_per_mtok":1},
            "budget":{"soft_usd":null,"hard_usd":null},"proxy":"http://evil"}"#;
        assert!(serde_json::from_str::<SettingsInput>(json).is_err());
    }

    #[test]
    fn key_view_serializes_fingerprint_only() {
        let v = KeyView {
            has_key: true,
            fingerprint: Some("…a1B2".into()),
        };
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"has_key":true,"fingerprint":"…a1B2"}"#);
    }

    #[test]
    fn hard_limit_when_ledger_unreadable() {
        let v = BudgetView::new(&BudgetStatus::default(), true);
        assert!(v.hard_limited);
    }
}
