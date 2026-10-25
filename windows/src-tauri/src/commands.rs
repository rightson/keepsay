//! IPC commands (PROPOSAL §7.3, INV-2).
//!
//! Rules, enforced by tests/ipc_surface.rs:
//! - Every command returns `Result<T, IpcError>` with `T` from the allowlist
//!   in that test (the types in `ipc.rs`, `()` or `Vec` of them).
//! - No command returns, accepts or logs the key. The key is entered in a
//!   native Win32 dialog opened from Rust (`enter_key`), so it never passes
//!   through the WebView; the WebView only ever sees the fingerprint.
//! - Each command is listed in `command_list.rs` and granted per window in
//!   `capabilities/`.

use std::sync::Arc;

use keepsay_core::downloader::{ModelDownloader, model_path, verify_file};
use keepsay_core::vault;
use tauri::{AppHandle, State};

use crate::app::{self, Shared};
use crate::ipc::{
    BudgetView, DownloadPhase, DownloadView, HotkeyStatusView, IpcError, KeyView, ModelView,
    OverlayPhase, OverlayView, SettingsInput, SettingsView, core_error_code,
};
use crate::{lock, tray, win32};

type S<'a> = State<'a, Arc<Shared>>;

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        get_settings,
        save_settings,
        key_status,
        enter_key,
        delete_key,
        budget_status,
        list_models,
        download_model,
        verify_model,
        model_progress,
        hotkey_status,
        open_mic_settings,
        overlay_state,
    ]
}

#[tauri::command]
fn get_settings(state: S<'_>) -> Result<SettingsView, IpcError> {
    Ok(SettingsView::from_config(&lock(&state.config)))
}

/// Sync on purpose: it runs on the main thread, where hotkey registration
/// must happen.
#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: S<'_>,
    input: SettingsInput,
) -> Result<HotkeyStatusView, IpcError> {
    let (old_hotkeys, old_model) = {
        let c = lock(&state.config);
        (c.hotkeys.clone(), c.stt_model.clone())
    };
    {
        let mut cfg = lock(&state.config);
        let next = input.apply(&cfg)?;
        next.save(&state.paths.config)?;
        *cfg = next;
    }
    let (new_hotkeys, new_model) = {
        let c = lock(&state.config);
        (c.hotkeys.clone(), c.stt_model.clone())
    };
    if new_model != old_model {
        state.stt.clear();
    }
    let status = if new_hotkeys != old_hotkeys {
        app::register_hotkeys(&app, &state)
    } else {
        current_hotkeys(&state)
    };
    tray::refresh(&state);
    Ok(status)
}

#[tauri::command]
fn key_status(state: S<'_>) -> Result<KeyView, IpcError> {
    Ok(state.key_view())
}

/// Open the native key dialog, verify the key online (GET /v1/models) and
/// store it (FR-1). Returns only the fingerprint.
#[tauri::command]
async fn enter_key(state: S<'_>) -> Result<KeyView, IpcError> {
    let key = tauri::async_runtime::spawn_blocking(win32::prompt_api_key)
        .await
        .map_err(|_| IpcError::new("dialog_failed"))?
        .map_err(|e| match e {
            win32::Win32Error::NotYetAvailable => IpcError::new("not_yet_available"),
            win32::Win32Error::Busy => IpcError::new("busy"),
            win32::Win32Error::Failed => IpcError::new("dialog_failed"),
        })?;
    let Some(key) = key else {
        return Err(IpcError::new("cancelled"));
    };
    let vault = state
        .vault
        .as_deref()
        .ok_or(IpcError::new("vault_unavailable"))?;
    let client = state.client.as_ref().ok_or(IpcError::new("network"))?;
    vault::store_key(vault, key, async |k| client.verify_key(k).await).await?;
    tray::refresh(&state);
    Ok(state.key_view())
}

#[tauri::command]
fn delete_key(state: S<'_>) -> Result<KeyView, IpcError> {
    let vault = state
        .vault
        .as_deref()
        .ok_or(IpcError::new("vault_unavailable"))?;
    vault::delete_key(vault)?;
    tray::refresh(&state);
    Ok(state.key_view())
}

#[tauri::command]
fn budget_status(state: S<'_>) -> Result<BudgetView, IpcError> {
    Ok(state.budget_view())
}

#[tauri::command]
fn list_models(state: S<'_>) -> Result<Vec<ModelView>, IpcError> {
    let selected = lock(&state.config).stt_model.clone();
    let verified = lock(&state.verified).clone();
    Ok(state
        .manifest
        .models
        .iter()
        .map(|m| ModelView {
            id: m.id.clone(),
            name: m.name.clone(),
            size_bytes: m.size,
            present: model_path(m, state.models_dir()).is_file(),
            verified: verified.contains(&m.id),
            selected: selected.as_deref() == Some(m.id.as_str()),
        })
        .collect())
}

/// User-triggered model download (INV-7). Progress is read with
/// `model_progress`.
#[tauri::command]
async fn download_model(state: S<'_>, id: String) -> Result<(), IpcError> {
    let entry = state.manifest.find(&id)?.clone();
    {
        let mut d = lock(&state.downloads);
        if matches!(
            d.phase,
            DownloadPhase::Downloading | DownloadPhase::Verifying
        ) {
            return Err(IpcError::new("busy"));
        }
        *d = DownloadView {
            id: Some(entry.id.clone()),
            phase: DownloadPhase::Downloading,
            received: 0,
            total: entry.size,
            error: None,
        };
    }
    let shared = state.inner().clone();
    let progress_shared = shared.clone();
    let mut progress = move |received: u64, total: u64| {
        let mut d = lock(&progress_shared.downloads);
        d.received = received;
        d.total = total;
        if received >= total {
            d.phase = DownloadPhase::Verifying;
        }
    };
    let result = match ModelDownloader::new() {
        Ok(dl) => dl
            .download(&entry, shared.models_dir(), &mut progress)
            .await
            .map(|_| ()),
        Err(e) => Err(e),
    };
    let mut d = lock(&shared.downloads);
    match result {
        Ok(()) => {
            d.phase = DownloadPhase::Done;
            lock(&shared.verified).insert(entry.id.clone());
            tracing::info!("model downloaded");
            Ok(())
        }
        Err(e) => {
            d.phase = DownloadPhase::Failed;
            d.error = Some(core_error_code(e));
            tracing::warn!(error = %e, "model download failed");
            Err(e.into())
        }
    }
}

/// Check the SHA-256 of a model file the user placed in the models folder
/// by hand (manual import, §8).
#[tauri::command]
async fn verify_model(state: S<'_>, id: String) -> Result<ModelView, IpcError> {
    let entry = state.manifest.find(&id)?.clone();
    let path = model_path(&entry, state.models_dir());
    let check = entry.clone();
    tauri::async_runtime::spawn_blocking(move || verify_file(&check, &path))
        .await
        .map_err(|_| IpcError::new("io"))??;
    lock(&state.verified).insert(entry.id.clone());
    let selected = lock(&state.config).stt_model.as_deref() == Some(entry.id.as_str());
    Ok(ModelView {
        id: entry.id,
        name: entry.name,
        size_bytes: entry.size,
        present: true,
        verified: true,
        selected,
    })
}

#[tauri::command]
fn model_progress(state: S<'_>) -> Result<DownloadView, IpcError> {
    Ok(lock(&state.downloads).clone())
}

fn current_hotkeys(state: &Shared) -> HotkeyStatusView {
    lock(&state.hotkeys)
        .view
        .clone()
        .unwrap_or(HotkeyStatusView {
            polish: String::new(),
            translate: String::new(),
            polish_registered: false,
            translate_registered: false,
        })
}

#[tauri::command]
fn hotkey_status(state: S<'_>) -> Result<HotkeyStatusView, IpcError> {
    Ok(current_hotkeys(&state))
}

/// Settings → Privacy & security → Microphone. A local URI handled by the
/// shell; no network.
#[tauri::command]
fn open_mic_settings() -> Result<(), IpcError> {
    std::process::Command::new("explorer.exe")
        .arg("ms-settings:privacy-microphone")
        .spawn()
        .map(|_| ())
        .map_err(|_| IpcError::new("io"))
}

/// Polled by the overlay window. Codes and numbers only.
#[tauri::command]
fn overlay_state(state: S<'_>) -> Result<OverlayView, IpcError> {
    let (phase, translate, since, notice) = {
        let o = lock(&state.overlay);
        (o.phase, o.translate, o.since, o.notice)
    };
    let (level, at_cap) = match lock(&state.recorder).as_ref() {
        Some(r) if phase == OverlayPhase::Recording => (r.level(), r.at_cap()),
        _ => (0.0, false),
    };
    let mode = lock(&state.config).mode;
    let stop_hint = lock(&state.hotkeys)
        .view
        .as_ref()
        .map(|v| {
            if translate {
                v.translate.clone()
            } else {
                v.polish.clone()
            }
        })
        .unwrap_or_default();
    Ok(OverlayView {
        phase,
        translate,
        mode,
        elapsed_ms: since.elapsed().as_millis() as u64,
        level,
        stop_hint,
        notice,
        at_cap,
    })
}
