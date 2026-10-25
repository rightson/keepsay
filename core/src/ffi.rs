//! UniFFI exports for the Swift apps (PROPOSAL §6.1, US-M1-22,
//! docs/changes/0014).
//!
//! INV-2, enforced here and by `core/tests/ffi_surface.rs`:
//! - no exported function returns the key or anything derived from it
//!   except [`Fingerprint`] (`…a1B2`), which crosses as a plain string;
//! - [`store_key`] turns the key into a `SecretString` on entry and wipes
//!   the lifted `String`; the key never comes back out;
//! - nothing in this file reads the key from the vault: [`verify_key`] and
//!   [`polish`] hand the vault to `AnthropicClient`, which reads it at the
//!   point of use (`core/tests/no_key_load.rs`).
//!
//! Process-wide state: the host calls [`init`] once with its app data
//! directory (Application Support). It holds the platform vault and the
//! paths of `config.json`, `budget.json` and `models/`. Every other export
//! returns `CoreError::NotInitialized` before that.
//!
//! Async: the exports are blocking. Each network call builds a
//! current-thread tokio runtime and a fresh `AnthropicClient` /
//! `ModelDownloader` for that call. uniffi's async exports would need the
//! `uniffi/tokio` feature (reqwest needs a tokio reactor), which is not on
//! the approved feature list (docs/changes/0012). The Swift side calls these
//! from a background task (`Task.detached`), never the main thread. Because
//! they create a runtime, they must not be called from inside a tokio
//! runtime; Rust hosts (CLI, Tauri) use the async module APIs instead.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use secrecy::SecretString;
use zeroize::Zeroize;

use crate::CoreError;
use crate::api::AnthropicClient;
use crate::budget::{Budget, BudgetStatus, YearMonth};
use crate::config::{BudgetLimits, Config, Hotkeys, PriceTable};
use crate::downloader::{Manifest, ModelDownloader};
use crate::polish::{FallbackReason, Mode, PolishOptions, PolishResult, TargetLang};
#[cfg(feature = "whisper")]
use crate::stt::SttOptions;
use crate::vault::{self, Fingerprint, KeyVault};

// `Fingerprint` crosses as a string. Lifting one from the foreign side only
// accepts the `…` + 4 characters shape, so it can never carry a key.
uniffi::custom_type!(Fingerprint, String, {
    lower: |fp| String::from(fp),
    try_lift: |s| Fingerprint::from_stored(&s)
        .ok_or_else(|| uniffi::deps::anyhow::anyhow!("not a fingerprint")),
});

/// Receives model download progress (bytes received, total bytes from the
/// manifest). Implemented in Swift.
#[uniffi::export(foreign)]
pub trait DownloadProgress: Send + Sync {
    fn on_progress(&self, received: u64, total: u64);
}

/// The user-editable part of `config.json` (PROPOSAL §10, FR-4/6/7/8),
/// read and written through [`get_settings`] / [`save_settings`] so the apps
/// never write a settings file of their own. `model` (allowlisted) and
/// `proxy` are not exposed and are kept as they are on save. The glossary
/// is private: `Debug` prints its length only (§6.9).
#[derive(Clone, PartialEq, uniffi::Record)]
pub struct Settings {
    pub mode: Mode,
    pub translate_to: TargetLang,
    /// Manifest id of the STT model.
    pub stt_model: Option<String>,
    pub glossary: Vec<String>,
    pub input_usd_per_mtok: f64,
    pub output_usd_per_mtok: f64,
    pub soft_limit_usd: Option<f64>,
    pub hard_limit_usd: Option<f64>,
    /// Platform-specific hotkey strings; `None` = platform default (FR-2).
    pub hotkey_polish: Option<String>,
    pub hotkey_polish_translate: Option<String>,
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("mode", &self.mode)
            .field("translate_to", &self.translate_to)
            .field("stt_model", &self.stt_model)
            .field("glossary_len", &self.glossary.len())
            .finish_non_exhaustive()
    }
}

impl From<&Config> for Settings {
    fn from(c: &Config) -> Self {
        Self {
            mode: c.mode,
            translate_to: c.translate_to,
            stt_model: c.stt_model.clone(),
            glossary: c.glossary.clone(),
            input_usd_per_mtok: c.prices.input_usd_per_mtok,
            output_usd_per_mtok: c.prices.output_usd_per_mtok,
            soft_limit_usd: c.budget.soft_usd,
            hard_limit_usd: c.budget.hard_usd,
            hotkey_polish: c.hotkeys.polish.clone(),
            hotkey_polish_translate: c.hotkeys.polish_translate.clone(),
        }
    }
}

impl Settings {
    /// `base` with these settings applied; `model`, `proxy` and the schema
    /// version come from `base`.
    fn apply_to(self, base: Config) -> Config {
        Config {
            mode: self.mode,
            translate_to: self.translate_to,
            stt_model: self.stt_model,
            glossary: self.glossary,
            prices: PriceTable {
                input_usd_per_mtok: self.input_usd_per_mtok,
                output_usd_per_mtok: self.output_usd_per_mtok,
            },
            budget: BudgetLimits {
                soft_usd: self.soft_limit_usd,
                hard_usd: self.hard_limit_usd,
            },
            hotkeys: Hotkeys {
                polish: self.hotkey_polish,
                polish_translate: self.hotkey_polish_translate,
            },
            ..base
        }
    }
}

/// One `models/manifest.json` entry and whether its file is present in
/// `models/`. `installed` only checks that the file exists; the SHA-256 is
/// verified when the model is loaded (INV-7).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub installed: bool,
}

struct Core {
    vault: Box<dyn KeyVault>,
    data_dir: PathBuf,
    /// Seconds east of UTC for the monthly budget reset (§6.8, 0009).
    utc_offset_secs: Mutex<i32>,
    /// Serializes budget read-modify-write between concurrent polishes.
    budget_lock: Mutex<()>,
    #[cfg(feature = "whisper")]
    stt: Mutex<Option<(String, crate::stt::whisper::WhisperEngine)>>,
    /// Loopback API base for wiremock tests. Exists only with
    /// `test-endpoint`, which release builds refuse.
    #[cfg(feature = "test-endpoint")]
    api_base: Mutex<Option<String>>,
}

static CORE: RwLock<Option<Arc<Core>>> = RwLock::new(None);

fn core() -> Result<Arc<Core>, CoreError> {
    CORE.read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or(CoreError::NotInitialized)
}

fn install(core: Core) {
    *CORE.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(core));
}

impl Core {
    fn new(vault: Box<dyn KeyVault>, data_dir: PathBuf) -> Self {
        Self {
            vault,
            data_dir,
            utc_offset_secs: Mutex::new(0),
            budget_lock: Mutex::new(()),
            #[cfg(feature = "whisper")]
            stt: Mutex::new(None),
            #[cfg(feature = "test-endpoint")]
            api_base: Mutex::new(None),
        }
    }

    #[cfg(feature = "test-endpoint")]
    fn test_api_base(&self) -> Option<String> {
        self.api_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn config(&self) -> Result<Config, CoreError> {
        Config::load(&self.data_dir.join("config.json"))
    }

    fn budget_path(&self) -> PathBuf {
        self.data_dir.join("budget.json")
    }

    fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    fn now(&self) -> YearMonth {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        let offset = *self
            .utc_offset_secs
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        YearMonth::from_unix_secs_with_offset(secs, offset)
    }

    fn client(&self, config: &Config) -> Result<AnthropicClient, CoreError> {
        #[cfg(feature = "test-endpoint")]
        if let Some(base) = self.test_api_base() {
            return AnthropicClient::with_test_endpoint(&base);
        }
        AnthropicClient::new(config.proxy.as_deref())
    }

    fn downloader(&self) -> Result<ModelDownloader, CoreError> {
        #[cfg(feature = "test-endpoint")]
        if self.test_api_base().is_some() {
            return ModelDownloader::with_test_endpoint();
        }
        ModelDownloader::new()
    }

    /// Budget status for `config`. A ledger that cannot be read counts as
    /// "at the hard limit", never as unspent (docs/changes/0009).
    fn budget_status(&self, config: &Config) -> BudgetStatus {
        let now = self.now();
        match Budget::load(&self.budget_path(), now) {
            Ok(b) => b.status(&config.prices, &config.budget, now),
            Err(e) => {
                tracing::warn!(error = %e, "budget ledger unreadable; treating as hard limit");
                BudgetStatus {
                    hard_limit_usd: Some(0.0),
                    ..BudgetStatus::default()
                }
            }
        }
    }
}

/// A current-thread runtime for one blocking FFI call.
fn runtime() -> Result<tokio::runtime::Runtime, CoreError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(CoreError::from)
}

// ------------------------------------------------------------- exports

/// Set up the process-wide core: the platform vault (Keychain / Credential
/// Manager) and the host's app data directory, which holds `config.json`,
/// `budget.json` and `models/`. Calling it again replaces the previous state.
/// Also installs the location-only panic hook (§6.9).
#[uniffi::export]
pub fn init(data_dir: String) -> Result<(), CoreError> {
    crate::panic_hook::install();
    let vault = vault::platform_vault()?;
    install(Core::new(vault, PathBuf::from(data_dir)));
    tracing::info!("ffi: core initialized");
    Ok(())
}

/// The host's current UTC offset in seconds (e.g. +28800 for Asia/Taipei),
/// so the budget resets on the local 1st (§6.8). Default 0 (UTC).
#[uniffi::export]
pub fn set_utc_offset(seconds: i32) -> Result<(), CoreError> {
    *core()?
        .utc_offset_secs
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = seconds;
    Ok(())
}

/// FR-1: format check → `GET /v1/models` → store → fingerprint. Nothing is
/// stored if a check fails, and an already stored key is left untouched.
/// The key is moved into a `SecretString` immediately and the lifted
/// `String` is wiped; it is never returned.
#[uniffi::export]
pub fn store_key(mut key: String) -> Result<Fingerprint, CoreError> {
    let secret = SecretString::from(key.as_str());
    key.zeroize();
    let core = core()?;
    let config = core.config()?;
    let client = core.client(&config)?;
    runtime()?.block_on(vault::store_key(
        core.vault.as_ref(),
        secret,
        async |k: &SecretString| client.verify_key(k).await,
    ))
}

/// Whether a key is stored. Does not read the key on Apple.
#[uniffi::export]
pub fn has_key() -> bool {
    core().is_ok_and(|c| vault::has_key(c.vault.as_ref()))
}

/// `Some("…a1B2")` when a key is stored, else `None` (also on errors).
#[uniffi::export]
pub fn key_fingerprint() -> Option<Fingerprint> {
    core()
        .ok()
        .and_then(|c| vault::key_fingerprint(c.vault.as_ref()))
}

/// Remove the stored key. Idempotent.
#[uniffi::export]
pub fn delete_key() -> Result<(), CoreError> {
    vault::delete_key(core()?.vault.as_ref())
}

/// `GET /v1/models` with the stored key. 401/403 → `KeyInvalid`; the key is
/// never deleted automatically.
#[uniffi::export]
pub fn verify_key() -> Result<(), CoreError> {
    let core = core()?;
    let config = core.config()?;
    let client = core.client(&config)?;
    runtime()?.block_on(client.verify_stored_key(core.vault.as_ref()))
}

/// Polish (and optionally translate) a transcript. Never fails once
/// initialized: any problem yields the raw transcript with a fallback reason
/// (FR-4, FR-8). Usage is recorded in `budget.json` here.
#[uniffi::export]
pub fn polish(transcript: String, opts: PolishOptions) -> Result<PolishResult, CoreError> {
    let core = core()?;
    let raw = |e: CoreError| PolishResult::raw(&transcript, Some(fallback(e, &opts)));
    let config = match core.config() {
        Ok(c) => c,
        Err(e) => return Ok(raw(e)),
    };
    let client = match core.client(&config) {
        Ok(c) => c,
        Err(e) => return Ok(raw(e)),
    };
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(e) => return Ok(raw(e)),
    };
    let status = core.budget_status(&config);
    let result = rt.block_on(crate::polish::polish(
        &client,
        core.vault.as_ref(),
        &transcript,
        &opts,
        &status,
    ));
    if let Some(usage) = result.usage {
        let _guard = core
            .budget_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let now = core.now();
        let saved = Budget::load(&core.budget_path(), now).and_then(|mut b| {
            b.record(usage, now);
            std::fs::create_dir_all(&core.data_dir)?;
            b.save(&core.budget_path())
        });
        if let Err(e) = saved {
            tracing::warn!(error = %e, "budget: usage not recorded");
        }
    }
    Ok(result)
}

/// Reason for a raw fallback that happens before the pipeline runs. Same
/// FR-8 rule as the pipeline: with translation, "API not usable" is
/// reported as `TranslationNeedsApi`.
fn fallback(e: CoreError, opts: &PolishOptions) -> FallbackReason {
    match FallbackReason::from_error(e) {
        FallbackReason::Network if opts.translate_to.is_some() => {
            FallbackReason::TranslationNeedsApi
        }
        r => r,
    }
}

/// Local speech-to-text with the configured model (`Config::stt_model`).
/// The model file is SHA-256-verified against the manifest when it is first
/// loaded (INV-7) and then kept in memory. Audio never touches the disk
/// (INV-5).
#[cfg(feature = "whisper")]
#[uniffi::export]
pub fn transcribe(pcm: Vec<f32>, sample_rate: u32, opts: SttOptions) -> Result<String, CoreError> {
    use crate::stt::SttEngine;
    use crate::stt::whisper::WhisperEngine;

    let core = core()?;
    let config = core.config()?;
    let id = config.stt_model.clone().ok_or(CoreError::ModelUnknown)?;
    let mut slot = core.stt.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.as_ref().is_none_or(|(loaded, _)| *loaded != id) {
        *slot = None;
        let manifest = Manifest::bundled()?;
        let entry = manifest.find(&id)?;
        let path = crate::downloader::model_path(entry, &core.models_dir());
        crate::downloader::verify_file(entry, &path)?;
        *slot = Some((id, WhisperEngine::load(&path)?));
    }
    let (_, engine) = slot.as_ref().ok_or(CoreError::Stt)?;
    engine.transcribe(&pcm, sample_rate, &opts)
}

/// Monthly usage, estimated cost and limits (FR-6). An unreadable ledger
/// reads as "at the hard limit".
#[uniffi::export]
pub fn budget_status() -> Result<BudgetStatus, CoreError> {
    let core = core()?;
    let config = core.config()?;
    Ok(core.budget_status(&config))
}

/// Download manifest model `id` into `models/`, verifying size and SHA-256
/// (INV-7). Nothing is left behind on failure.
#[uniffi::export]
pub fn download_model(id: String, progress: Arc<dyn DownloadProgress>) -> Result<(), CoreError> {
    let core = core()?;
    let manifest = Manifest::bundled()?;
    let entry = manifest.find(&id)?;
    let downloader = core.downloader()?;
    let dir = core.models_dir();
    let mut report = |got: u64, total: u64| progress.on_progress(got, total);
    runtime()?
        .block_on(downloader.download(entry, &dir, &mut report))
        .map(|_| ())
}

/// Verify and copy a model file the user picked (§8, no network).
#[uniffi::export]
pub fn import_model(id: String, path: String) -> Result<(), CoreError> {
    let core = core()?;
    let manifest = Manifest::bundled()?;
    let entry = manifest.find(&id)?;
    crate::downloader::import(entry, Path::new(&path), &core.models_dir()).map(|_| ())
}

/// The user-editable settings from `config.json` (defaults when the file
/// does not exist yet).
#[uniffi::export]
pub fn get_settings() -> Result<Settings, CoreError> {
    Ok(Settings::from(&core()?.config()?))
}

/// Validate and write `settings` into `config.json` (temp file + rename).
/// An unreadable or newer-schema file is refused with `Config` and left
/// untouched (§10: a downgrade never destroys newer settings).
#[uniffi::export]
pub fn save_settings(settings: Settings) -> Result<(), CoreError> {
    let core = core()?;
    let config = settings.apply_to(core.config()?);
    std::fs::create_dir_all(&core.data_dir)?;
    config.save(&core.data_dir.join("config.json"))?;
    tracing::info!("ffi: settings saved");
    Ok(())
}

/// The bundled manifest's models and whether each is present in `models/`.
#[uniffi::export]
pub fn list_models() -> Result<Vec<ModelStatus>, CoreError> {
    let core = core()?;
    let manifest = Manifest::bundled()?;
    let dir = core.models_dir();
    Ok(manifest
        .models
        .iter()
        .map(|e| ModelStatus {
            id: e.id.clone(),
            name: e.name.clone(),
            size: e.size,
            installed: crate::downloader::model_path(e, &dir).is_file(),
        })
        .collect())
}

// ------------------------------------------------------- test support

/// Rust-only hooks for `core/tests/ffi_*.rs`. Not exported over FFI, and
/// absent unless both `test-support` and `test-endpoint` are on (release
/// builds refuse both).
#[cfg(all(feature = "test-support", feature = "test-endpoint"))]
pub mod testing {
    use super::*;

    /// Like [`init`], with a given vault (the mock) and a loopback API base
    /// for wiremock; the downloader then also accepts loopback URLs.
    pub fn init_with(vault: Box<dyn KeyVault>, data_dir: &Path, api_base: Option<&str>) {
        let core = Core::new(vault, data_dir.to_path_buf());
        *core.api_base.lock().unwrap_or_else(PoisonError::into_inner) = api_base.map(str::to_owned);
        install(core);
    }

    /// Point the initialized core at another loopback API base, keeping the
    /// vault and its contents.
    pub fn set_api_base(api_base: Option<&str>) {
        if let Ok(core) = core() {
            *core.api_base.lock().unwrap_or_else(PoisonError::into_inner) =
                api_base.map(str::to_owned);
        }
    }

    /// Back to the uninitialized state.
    pub fn reset() {
        *CORE.write().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Download from an explicit manifest entry (the bundled manifest only
    /// has real HTTPS URLs), through the same path as [`download_model`].
    pub fn download_entry(
        entry: &crate::downloader::ModelEntry,
        progress: Arc<dyn DownloadProgress>,
    ) -> Result<(), CoreError> {
        let core = core()?;
        let downloader = core.downloader()?;
        let dir = core.models_dir();
        let mut report = |got: u64, total: u64| progress.on_progress(got, total);
        runtime()?
            .block_on(downloader.download(entry, &dir, &mut report))
            .map(|_| ())
    }
}
