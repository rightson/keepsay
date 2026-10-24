//! Non-secret settings, stored as JSON with a schema version (PROPOSAL §10).
//! Never holds the key (INV-1). The glossary is private: never log it, so
//! `Debug` prints its length only.
//!
//! Owner: workstream D (docs/changes/0005). The settings file path is passed
//! in by the platform layer (Swift / Tauri / CLI); this module does not
//! locate App Support itself.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::api::types::DEFAULT_MODEL;
use crate::polish::{Mode, TargetLang};

pub const SCHEMA_VERSION: u32 = 1;

/// User-editable price table (USD per million tokens). Defaults must be
/// labelled "verify against official pricing" in the UI (§6.8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceTable {
    pub input_usd_per_mtok: f64,
    pub output_usd_per_mtok: f64,
}

/// Monthly limits; `None` = no limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BudgetLimits {
    pub soft_usd: Option<f64>,
    pub hard_usd: Option<f64>,
}

/// Hotkey overrides as platform-specific strings; `None` = platform default
/// (FR-2). The core only stores them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkeys {
    pub polish: Option<String>,
    pub polish_translate: Option<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
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
            stt_model: None,
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
    /// unknown `schema_version` with `CoreError::Config`.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let _ = path;
        todo!("M1: config — load")
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let _ = path;
        todo!("M1: config — save")
    }
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
