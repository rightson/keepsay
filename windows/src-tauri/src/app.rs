//! App state, startup, windows and hotkey registration.
//!
//! Hardening (PROPOSAL §7.3, docs/changes/0016):
//! - Only the embedded `ui/` is ever loaded (`custom-protocol`; no dev URL,
//!   no remote URL). CSP is set in tauri.conf.json.
//! - Devtools are off in release builds.
//! - No shell / fs / http / dialog plugins. The global-shortcut plugin is
//!   used from Rust only; no capability grants its JS commands.
//! - Each window gets a capability with only the commands it needs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use keepsay_core::api::AnthropicClient;
use keepsay_core::budget::{Budget, YearMonth};
use keepsay_core::config::{Config, WINDOWS_HOTKEYS};
use keepsay_core::downloader::Manifest;
use keepsay_core::{BudgetStatus, CoreError, KeyVault, vault};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::hotkey::{Combo, parse_pair};
use crate::ipc::{BudgetView, DownloadView, HotkeyStatusView, KeyView, OverlayPhase};
use crate::recorder::Recorder;
use crate::session::{Action, Session};
use crate::{commands, lock, pipeline, tray};

pub const SETTINGS_LABEL: &str = "settings";
pub const OVERLAY_LABEL: &str = "overlay";

/// Overlay size in logical pixels (Win11 flyout style, docs/mockups).
const OVERLAY_W: f64 = 340.0;
const OVERLAY_H: f64 = 60.0;

/// `%LOCALAPPDATA%\keepsay`. Local, not roaming: the glossary and budget
/// stay on this machine (§10).
pub struct Paths {
    pub config: PathBuf,
    pub budget: PathBuf,
    pub models: PathBuf,
}

impl Paths {
    fn new() -> Result<Self, CoreError> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(CoreError::Io(std::io::ErrorKind::NotFound))?
            .join("keepsay");
        std::fs::create_dir_all(&base)?;
        Ok(Self {
            config: base.join("config.json"),
            budget: base.join("budget.json"),
            models: base.join("models"),
        })
    }
}

pub struct BudgetState {
    pub ledger: Budget,
    /// budget.json exists but could not be read: treated as at the hard
    /// limit and never overwritten (core `Budget::load` docs).
    pub unreadable: bool,
}

#[derive(Default)]
pub struct HotkeyState {
    pub polish: Option<Shortcut>,
    pub translate: Option<Shortcut>,
    pub view: Option<HotkeyStatusView>,
    pub escape: Option<Shortcut>,
}

pub struct Overlay {
    pub phase: OverlayPhase,
    pub translate: bool,
    pub since: Instant,
    pub notice: Option<&'static str>,
    /// Bumped on every change, so a delayed hide only hides its own state.
    pub seq: u64,
}

pub struct Shared {
    pub paths: Paths,
    pub config: Mutex<Config>,
    pub vault: Option<Box<dyn KeyVault>>,
    pub client: Option<AnthropicClient>,
    pub manifest: Manifest,
    pub budget: Mutex<BudgetState>,
    pub session: Mutex<Session>,
    pub recorder: Mutex<Option<Recorder>>,
    pub overlay: Mutex<Overlay>,
    pub hotkeys: Mutex<HotkeyState>,
    pub downloads: Mutex<DownloadView>,
    pub verified: Mutex<HashSet<String>>,
    pub stt: pipeline::SttCache,
    pub tray: Mutex<Option<tray::TrayHandles>>,
}

pub fn now_month() -> YearMonth {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    YearMonth::from_unix_secs(secs)
}

impl Shared {
    fn load() -> Result<Self, CoreError> {
        let paths = Paths::new()?;
        let config = Config::load(&paths.config).unwrap_or_else(|_| {
            tracing::warn!("config unreadable, using defaults");
            Config::default()
        });
        let now = now_month();
        let budget = match Budget::load(&paths.budget, now) {
            Ok(ledger) => BudgetState {
                ledger,
                unreadable: false,
            },
            Err(_) => {
                tracing::warn!("budget ledger unreadable, treating as hard limit");
                BudgetState {
                    ledger: Budget::new(now),
                    unreadable: true,
                }
            }
        };
        let vault = vault::platform_vault().ok();
        let client = AnthropicClient::new(config.proxy.as_deref()).ok();
        let manifest = Manifest::bundled()?;
        Ok(Self {
            paths,
            config: Mutex::new(config),
            vault,
            client,
            manifest,
            budget: Mutex::new(budget),
            session: Mutex::new(Session::default()),
            recorder: Mutex::new(None),
            overlay: Mutex::new(Overlay {
                phase: OverlayPhase::Hidden,
                translate: false,
                since: Instant::now(),
                notice: None,
                seq: 0,
            }),
            hotkeys: Mutex::new(HotkeyState::default()),
            downloads: Mutex::new(DownloadView::default()),
            verified: Mutex::new(HashSet::new()),
            stt: pipeline::SttCache::default(),
            tray: Mutex::new(None),
        })
    }

    /// Key state for the UI: existence and fingerprint only (INV-2). Uses
    /// `vault::stored_fingerprint`, never `KeyVault::load` (0014).
    pub fn key_view(&self) -> KeyView {
        let Some(v) = self.vault.as_deref() else {
            return KeyView {
                has_key: false,
                fingerprint: None,
            };
        };
        match vault::stored_fingerprint(v) {
            Ok(fp) => KeyView {
                has_key: fp.is_some(),
                fingerprint: fp.map(String::from),
            },
            Err(_) => KeyView {
                has_key: vault::has_key(v),
                fingerprint: None,
            },
        }
    }

    /// The status `polish` sees. An unreadable ledger counts as the hard
    /// limit.
    pub fn budget_status(&self) -> (BudgetStatus, bool) {
        let cfg = lock(&self.config);
        let b = lock(&self.budget);
        let mut s = b.ledger.status(&cfg.prices, &cfg.budget, now_month());
        if b.unreadable {
            s.hard_limit_usd = Some(0.0);
        }
        (s, b.unreadable)
    }

    pub fn budget_view(&self) -> BudgetView {
        let (s, unreadable) = self.budget_status();
        let mut v = BudgetView::new(&s, unreadable);
        if unreadable {
            v.hard_limit_usd = lock(&self.config).budget.hard_usd;
        }
        v
    }

    pub fn models_dir(&self) -> &Path {
        &self.paths.models
    }

    /// Change the overlay state; returns the new sequence number.
    pub fn set_overlay(
        &self,
        phase: OverlayPhase,
        translate: bool,
        notice: Option<&'static str>,
    ) -> u64 {
        let mut o = lock(&self.overlay);
        o.phase = phase;
        o.translate = translate;
        o.notice = notice;
        o.since = Instant::now();
        o.seq += 1;
        o.seq
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
    // §6.9: location only, never the payload (core, 0014).
    keepsay_core::panic_hook::install();

    let shared = match Shared::load() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!(error = %e, "startup failed");
            std::process::exit(1);
        }
    };

    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        on_shortcut(app, shortcut);
                    }
                })
                .build(),
        )
        .manage(shared.clone())
        .invoke_handler(commands::handler())
        .setup(move |app| {
            let handle = app.handle().clone();
            tray::build(&handle, &shared)?;
            build_overlay(&handle)?;
            let status = register_hotkeys(&handle, &shared);
            if !(status.polish_registered && status.translate_registered) {
                tracing::warn!("hotkey registration failed");
                show_settings(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(_) => {
            tracing::error!("tauri failed to start");
            std::process::exit(1);
        }
    };
    app.run(|_app, event| {
        // A tray app keeps running with no window open.
        if let RunEvent::ExitRequested { api, code, .. } = event
            && code.is_none()
        {
            api.prevent_exit();
        }
    });
}

fn on_shortcut(app: &AppHandle, shortcut: &Shortcut) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    let (is_polish, is_translate, is_escape) = {
        let h = lock(&shared.hotkeys);
        (
            h.polish.as_ref() == Some(shortcut),
            h.translate.as_ref() == Some(shortcut),
            h.escape.as_ref() == Some(shortcut),
        )
    };
    if is_escape {
        pipeline::on_escape(app, &shared);
    } else if is_polish {
        pipeline::on_hotkey(app, &shared, Action::Polish);
    } else if is_translate {
        pipeline::on_hotkey(app, &shared, Action::PolishTranslate);
    }
}

fn to_shortcut(c: &Combo) -> Option<Shortcut> {
    c.accelerator().parse::<Shortcut>().ok()
}

/// (Re-)register both hotkeys from the config. A combination another app
/// already holds fails here; the settings page then asks for another one.
pub fn register_hotkeys(app: &AppHandle, shared: &Shared) -> HotkeyStatusView {
    let gs = app.global_shortcut();
    let (p, t) = lock(&shared.config)
        .hotkeys
        .resolve(WINDOWS_HOTKEYS);
    let (pc, tc) = match parse_pair(&p, &t) {
        Ok(pair) => pair,
        Err(_) => {
            tracing::warn!("stored hotkeys invalid, using defaults");
            match parse_pair(WINDOWS_HOTKEYS.polish, WINDOWS_HOTKEYS.polish_translate) {
                Ok(pair) => pair,
                Err(_) => unreachable!("default hotkeys are valid"),
            }
        }
    };
    let mut h = lock(&shared.hotkeys);
    for old in [h.polish.take(), h.translate.take()].into_iter().flatten() {
        let _ = gs.unregister(old);
    }
    let try_register =|c: &Combo| -> (Option<Shortcut>, bool) {
        match to_shortcut(c) {
            Some(s) => {
                let ok = gs.register(s).is_ok();
                (ok.then_some(s), ok)
            }
            None => (None, false),
        }
    };
    let (ps, pok) = try_register(&pc);
    let (ts, tok) = try_register(&tc);
    h.polish = ps;
    h.translate = ts;
    let view = HotkeyStatusView {
        polish: pc.to_string(),
        translate: tc.to_string(),
        polish_registered: pok,
        translate_registered: tok,
    };
    h.view = Some(view.clone());
    tracing::info!(polish = pok, translate = tok, "hotkeys registered");
    view
}

/// Esc is registered only while recording, so it is not taken from other
/// apps the rest of the time.
pub fn set_escape(app: &AppHandle, shared: &Shared, on: bool) {
    let gs = app.global_shortcut();
    let mut h = lock(&shared.hotkeys);
    if on {
        if h.escape.is_none()
            && let Ok(s) = "Escape".parse::<Shortcut>()
            && gs.register(s).is_ok()
        {
            h.escape = Some(s);
        }
    } else if let Some(s) = h.escape.take() {
        let _ = gs.unregister(s);
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("index.html".into()))
        .title("keepsay 設定")
        .inner_size(860.0, 600.0)
        .min_inner_size(640.0, 480.0)
        .devtools(cfg!(debug_assertions))
        .build();
    if built.is_err() {
        tracing::warn!("settings window failed to open");
    }
}

fn build_overlay(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
        .title("keepsay")
        .inner_size(OVERLAY_W, OVERLAY_H)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // Never take focus from the app the text goes into.
        .focusable(false)
        .focused(false)
        .visible(false)
        .devtools(cfg!(debug_assertions))
        .build()?;
    Ok(())
}

/// Bottom centre of the primary monitor's work area (above the taskbar).
pub fn show_overlay(app: &AppHandle) {
    let Some(w) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    if let Ok(Some(m)) = app.primary_monitor() {
        let area = m.work_area();
        let scale = m.scale_factor();
        let w_px = (OVERLAY_W * scale) as i32;
        let h_px = (OVERLAY_H * scale) as i32;
        let x = area.position.x + (area.size.width as i32 - w_px) / 2;
        let y = area.position.y + area.size.height as i32 - h_px - (16.0 * scale) as i32;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = w.show();
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = w.hide();
    }
}
