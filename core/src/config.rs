//! Non-secret settings, stored as JSON with a schema version (PROPOSAL §10).
//! Never holds the key (INV-1). The glossary is private: never log it, so
//! `Debug` prints its length only, and errors never carry file contents.
//!
//! Owner: workstream D (docs/changes/0005, 0009). The settings file path is
//! passed in by the platform layer (Swift / Tauri / CLI); this module does not
//! locate App Support itself.
//!
//! Versioning: `load` reads `schema_version` first. A newer version than
//! [`SCHEMA_VERSION`] is refused with `CoreError::Config` (and the caller must
//! not overwrite the file, so a downgrade cannot destroy newer settings). An
//! older version goes through [`migrate`]. Missing fields take their defaults;
//! unknown fields are refused.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::api::types::{DEFAULT_MODEL, MODEL_ALLOWLIST};
use crate::polish::{Mode, TargetLang};

pub const SCHEMA_VERSION: u32 = 1;

/// STT model chosen until the user picks one (manifest id, §6.7).
pub const DEFAULT_STT_MODEL: &str = "whisper-small";

/// User-editable price table (USD per million tokens). The UI must label
/// the defaults "請核對官方定價" (verify against official pricing, §6.8).
///
/// Defaults: Claude Haiku 5.5 first-party list price, $0.10 input /
/// $0.50 output per MTok for prompts up to 100K tokens (claude-api reference,
/// cached 2026-10-06; docs/changes/0009). 【待驗證 V4】
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PriceTable {
    pub input_usd_per_mtok: f64,
    pub output_usd_per_mtok: f64,
}

impl Default for PriceTable {
    fn default() -> Self {
        Self {
            input_usd_per_mtok: 0.10,
            output_usd_per_mtok: 0.50,
        }
    }
}

/// Monthly limits; `None` = no limit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BudgetLimits {
    pub soft_usd: Option<f64>,
    pub hard_usd: Option<f64>,
}

impl Default for BudgetLimits {
    /// Soft $5 / hard $10 a month. At default prices that is tens of millions
    /// of tokens, far above dictation use; it bounds a runaway bug. The
    /// account-side spend limit (§12) remains the real cap.
    fn default() -> Self {
        Self {
            soft_usd: Some(5.0),
            hard_usd: Some(10.0),
        }
    }
}

/// Hotkey overrides as platform-specific strings; `None` = platform default
/// (FR-2). The core only stores them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hotkeys {
    pub polish: Option<String>,
    pub polish_translate: Option<String>,
}

/// A platform's default hotkey pair (docs/changes/0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyDefaults {
    pub polish: &'static str,
    pub polish_translate: &'static str,
}

/// Windows: RegisterHotKey with MOD_NOREPEAT.
pub const WINDOWS_HOTKEYS: HotkeyDefaults = HotkeyDefaults {
    polish: "Alt+Shift+Z",
    polish_translate: "Alt+Shift+X",
};
/// macOS: active CGEventTap (needs Accessibility).
pub const MACOS_HOTKEYS: HotkeyDefaults = HotkeyDefaults {
    polish: "Fn+Z",
    polish_translate: "Fn+Shift+Z",
};
/// macOS fallback via Carbon, no extra permission.
pub const MACOS_FALLBACK_HOTKEYS: HotkeyDefaults = HotkeyDefaults {
    polish: "Option+Space",
    polish_translate: "Option+Shift+Space",
};

impl Hotkeys {
    /// The user's overrides, falling back to `defaults`.
    pub fn resolve(&self, defaults: HotkeyDefaults) -> (String, String) {
        (
            self.polish
                .clone()
                .unwrap_or_else(|| defaults.polish.to_owned()),
            self.polish_translate
                .clone()
                .unwrap_or_else(|| defaults.polish_translate.to_owned()),
        )
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    /// Mode for the plain "polish" hotkey.
    pub mode: Mode,
    /// Target for the "polish and translate" hotkey (FR-8).
    pub translate_to: TargetLang,
    /// Must be in `api::types::MODEL_ALLOWLIST`.
    pub model: String,
    /// Manifest id of the STT model.
    pub stt_model: Option<String>,
    pub glossary: Vec<String>,
    pub hotkeys: Hotkeys,
    pub prices: PriceTable,
    pub budget: BudgetLimits,
    /// Explicit proxy for `AnthropicClient` only; `None` = no proxy (§6.3).
    pub proxy: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            mode: Mode::default(),
            translate_to: TargetLang::default(),
            model: DEFAULT_MODEL.to_owned(),
            stt_model: Some(DEFAULT_STT_MODEL.to_owned()),
            glossary: Vec::new(),
            hotkeys: Hotkeys::default(),
            prices: PriceTable::default(),
            budget: BudgetLimits::default(),
            proxy: None,
        }
    }
}

impl Config {
    /// Read `path`; a missing file yields `Config::default()`. Rejects an
    /// unknown `schema_version`, unknown fields, or invalid values with
    /// `CoreError::Config`.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        Self::from_json(&bytes)
    }

    /// Parse settings JSON, migrating older schema versions.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CoreError> {
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| CoreError::Config)?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or(CoreError::Config)?;
        if version > SCHEMA_VERSION {
            return Err(CoreError::Config);
        }
        if version < SCHEMA_VERSION {
            value = migrate(value, version)?;
        }
        let cfg: Config = serde_json::from_value(value).map_err(|_| CoreError::Config)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Validate, then write via a temp file in the same directory and rename.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        self.validate()?;
        let json = serde_json::to_vec_pretty(self).map_err(|_| CoreError::Config)?;
        write_atomic(path, &json)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(CoreError::Config);
        }
        if !MODEL_ALLOWLIST.contains(&self.model.as_str()) {
            return Err(CoreError::ModelNotAllowed);
        }
        let money_ok = |v: f64| v.is_finite() && v >= 0.0;
        let limit_ok = |v: Option<f64>| v.is_none_or(money_ok);
        if !(money_ok(self.prices.input_usd_per_mtok)
            && money_ok(self.prices.output_usd_per_mtok)
            && limit_ok(self.budget.soft_usd)
            && limit_ok(self.budget.hard_usd))
        {
            return Err(CoreError::Config);
        }
        if self.stt_model.as_deref().is_some_and(str::is_empty) {
            return Err(CoreError::Config);
        }
        Ok(())
    }
}

/// Migration hook: upgrade settings JSON from schema `from` to
/// [`SCHEMA_VERSION`], one version at a time. Version 1 is the first schema,
/// so there is nothing to migrate yet and any older number is refused. When
/// bumping the schema, add a `from => …` arm that rewrites `value` and sets
/// its `schema_version`, and a test with a fixture of the old format.
pub fn migrate(value: serde_json::Value, from: u32) -> Result<serde_json::Value, CoreError> {
    // When SCHEMA_VERSION becomes 2:
    //   let mut value = value;
    //   if from == 1 { /* rewrite fields */ value["schema_version"] = 2.into(); }
    //   return Ok(value);
    let _ = (value, from);
    Err(CoreError::Config)
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("schema_version", &self.schema_version)
            .field("mode", &self.mode)
            .field("translate_to", &self.translate_to)
            .field("model", &self.model)
            .field("stt_model", &self.stt_model)
            .field("glossary_len", &self.glossary.len())
            .field("hotkeys", &self.hotkeys)
            .field("prices", &self.prices)
            .field("budget", &self.budget)
            .field("proxy_set", &self.proxy.is_some())
            .finish()
    }
}

/// Write `bytes` to `path` atomically: a temp file in the same directory,
/// fsync, then rename over `path`. The temp file is removed on failure.
/// Shared by `config.json` and `budget.json`.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path.file_name().ok_or(CoreError::Config)?;
    fs::create_dir_all(&dir)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".tmp-{}-{nonce}", std::process::id()));
    let tmp = dir.join(tmp_name);

    let result = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_spec() {
        let c = Config::default();
        assert_eq!(c.mode, Mode::Polish);
        assert_eq!(c.translate_to, TargetLang::En);
        assert_eq!(c.model, DEFAULT_MODEL);
        assert!(c.proxy.is_none());
        assert_eq!(c.validate(), Ok(()));
        let (p, t) = c.hotkeys.resolve(WINDOWS_HOTKEYS);
        assert_eq!((p.as_str(), t.as_str()), ("Alt+Shift+Z", "Alt+Shift+X"));
    }

    #[test]
    fn default_stt_model_is_in_manifest() {
        let m = crate::downloader::Manifest::bundled().unwrap();
        assert!(m.find(DEFAULT_STT_MODEL).is_ok());
    }

    #[test]
    fn migrate_refuses_unknown_old_versions() {
        assert_eq!(migrate(serde_json::json!({}), 0), Err(CoreError::Config));
    }
}
