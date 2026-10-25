//! Hotkey → record → STT → polish → insert.
//!
//! Logs carry phases, lengths and timings only; never the transcript, the
//! model output or the glossary (§6.9). Audio and text buffers are zeroed
//! after use (INV-5).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keepsay_core::polish::polish;
use keepsay_core::stt::{SttEngine, SttOptions};
use keepsay_core::{CoreError, FallbackReason, Mode, PolishOptions, PolishResult};
use tauri::AppHandle;
use zeroize::Zeroize;

use crate::app::{self, Shared, now_month};
use crate::insert::{self, InsertError, Win32Backend};
use crate::ipc::{OverlayPhase, fallback_code};
use crate::recorder::Recorder;
use crate::session::{Action, Step};
use crate::{audio, lock, tray};

const SHOW_DONE: Duration = Duration::from_millis(1600);
const SHOW_NOTICE: Duration = Duration::from_millis(3500);

/// The loaded whisper model, kept between recordings.
#[derive(Default)]
pub struct SttCache {
    #[cfg(feature = "whisper")]
    engine: Mutex<Option<(String, Arc<keepsay_core::stt::whisper::WhisperEngine>)>>,
    #[cfg(not(feature = "whisper"))]
    engine: Mutex<()>,
}

impl SttCache {
    /// Drop the cached model (another model was selected).
    pub fn clear(&self) {
        #[cfg(feature = "whisper")]
        {
            lock(&self.engine).take();
        }
        #[cfg(not(feature = "whisper"))]
        {
            drop(lock(&self.engine));
        }
    }
}

#[cfg(feature = "whisper")]
fn engine(shared: &Shared) -> Result<Arc<dyn SttEngine + Send + Sync>, &'static str> {
    use keepsay_core::downloader::model_path;
    use keepsay_core::stt::whisper::WhisperEngine;

    let id = lock(&shared.config).stt_model.clone().ok_or("no_model")?;
    let mut cache = lock(&shared.stt.engine);
    if let Some((cached, e)) = cache.as_ref()
        && *cached == id
    {
        return Ok(e.clone());
    }
    let entry = shared.manifest.find(&id).map_err(|_| "no_model")?;
    let path = model_path(entry, shared.models_dir());
    if !path.is_file() {
        return Err("no_model");
    }
    let e = Arc::new(WhisperEngine::load(&path).map_err(|_| "stt")?);
    *cache = Some((id, e.clone()));
    Ok(e)
}

#[cfg(not(feature = "whisper"))]
fn engine(shared: &Shared) -> Result<Arc<dyn SttEngine + Send + Sync>, &'static str> {
    drop(lock(&shared.stt.engine));
    Err("stt_unavailable")
}

pub fn on_hotkey(app: &AppHandle, shared: &Arc<Shared>, action: Action) {
    let step = lock(&shared.session).on_hotkey(action);
    match step {
        Step::StartRecording(a) => start(app, shared, a),
        Step::StopAndProcess(a) => {
            app::set_escape(app, shared, false);
            let rec = lock(&shared.recorder).take();
            let (app, shared) = (app.clone(), shared.clone());
            tauri::async_runtime::spawn(async move {
                process(&app, &shared, a, rec).await;
            });
        }
        Step::Cancel | Step::Ignore => {}
    }
}

pub fn on_escape(app: &AppHandle, shared: &Arc<Shared>) {
    if lock(&shared.session).on_escape() == Step::Cancel {
        app::set_escape(app, shared, false);
        if let Some(rec) = lock(&shared.recorder).take() {
            rec.cancel();
        }
        shared.set_overlay(OverlayPhase::Hidden, false, None);
        app::hide_overlay(app);
        tray::refresh(shared);
        tracing::info!("recording cancelled");
    }
}

fn start(app: &AppHandle, shared: &Arc<Shared>, action: Action) {
    let translate = action == Action::PolishTranslate;
    match Recorder::start() {
        Ok(rec) => {
            *lock(&shared.recorder) = Some(rec);
            shared.set_overlay(OverlayPhase::Recording, translate, None);
            app::set_escape(app, shared, true);
            app::show_overlay(app);
        }
        Err(e) => {
            lock(&shared.session).on_start_failed();
            let seq = shared.set_overlay(OverlayPhase::Error, translate, Some(e.code()));
            app::show_overlay(app);
            hide_later(app, shared, seq, SHOW_NOTICE);
        }
    }
    tray::refresh(shared);
}

async fn process(app: &AppHandle, shared: &Arc<Shared>, action: Action, rec: Option<Recorder>) {
    let translate = action == Action::PolishTranslate;
    let started = Instant::now();
    let outcome = run(shared, translate, rec).await;
    let (phase, notice) = match outcome {
        Ok(None) => (OverlayPhase::Inserted, None),
        Ok(Some(reason)) => (OverlayPhase::Fallback, Some(reason)),
        Err(code) => (OverlayPhase::Error, Some(code)),
    };
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        ok = phase == OverlayPhase::Inserted,
        "dictation finished"
    );
    lock(&shared.session).on_done();
    let seq = shared.set_overlay(phase, translate, notice);
    tray::refresh(shared);
    let wait = if phase == OverlayPhase::Inserted {
        SHOW_DONE
    } else {
        SHOW_NOTICE
    };
    hide_later(app, shared, seq, wait);
}

fn hide_later(app: &AppHandle, shared: &Arc<Shared>, seq: u64, after: Duration) {
    let (app, shared) = (app.clone(), shared.clone());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(after).await;
        let still = lock(&shared.overlay).seq == seq;
        if still {
            shared.set_overlay(OverlayPhase::Hidden, false, None);
            app::hide_overlay(&app);
        }
    });
}

/// `Ok(None)`: polished (or raw by choice) and inserted. `Ok(Some(code))`:
/// inserted the raw transcript because polishing fell back. `Err(code)`:
/// nothing was inserted.
async fn run(
    shared: &Arc<Shared>,
    translate: bool,
    rec: Option<Recorder>,
) -> Result<Option<&'static str>, &'static str> {
    let rec = rec.ok_or("mic_unavailable")?;
    shared.set_overlay(OverlayPhase::Transcribing, translate, None);

    let (mode, target, glossary) = {
        let c = lock(&shared.config);
        (c.mode, c.translate_to, c.glossary.clone())
    };

    // STT on a blocking thread.
    let stt_shared = shared.clone();
    let stt_glossary = glossary.clone();
    let transcript = tauri::async_runtime::spawn_blocking(move || {
        let mut captured = rec.stop().map_err(|e| e.code())?;
        let rate = captured.rate;
        let mut pcm = audio::finish(&mut captured.pcm, rate).map_err(|_| "audio_format")?;
        let engine = engine(&stt_shared);
        let result = engine.and_then(|e| {
            let t0 = Instant::now();
            let opts = SttOptions {
                language: Some("zh-TW".into()),
                glossary: stt_glossary,
            };
            let r = e
                .transcribe(&pcm, keepsay_core::stt::SAMPLE_RATE, &opts)
                .map_err(|_| "stt");
            tracing::info!(
                samples = pcm.len(),
                elapsed_ms = t0.elapsed().as_millis() as u64,
                "transcribed"
            );
            r
        });
        pcm.zeroize();
        result
    })
    .await
    .map_err(|_| "stt")??;

    let mut transcript = transcript;
    if transcript.trim().is_empty() {
        transcript.zeroize();
        return Err("empty");
    }

    let opts = PolishOptions {
        // FR-2: the translate hotkey always means polish + translate.
        mode: if translate { Mode::Polish } else { mode },
        translate_to: translate.then_some(target),
        glossary,
    };
    let mut result = polish_step(shared, &transcript, &opts, translate).await;
    transcript.zeroize();

    if let Some(usage) = result.usage {
        record_usage(shared, usage);
    }
    let fallback = result.fallback_reason.map(fallback_code);

    let mut text = std::mem::take(&mut result.text);
    let inserted = tauri::async_runtime::spawn_blocking(move || {
        let r = insert::insert(&text, &Win32Backend::default());
        text.zeroize();
        r
    })
    .await
    .map_err(|_| "insert_failed")?;
    match inserted {
        Ok(method) => {
            tracing::info!(clipboard = method == insert::Method::Clipboard, "inserted");
            Ok(fallback)
        }
        Err(InsertError::NotYetAvailable) => Err("insert_unavailable"),
        Err(InsertError::Failed) => Err("insert_failed"),
    }
}

async fn polish_step(
    shared: &Arc<Shared>,
    transcript: &str,
    opts: &PolishOptions,
    translate: bool,
) -> PolishResult {
    if !opts.needs_api() {
        return PolishResult::raw(transcript, None);
    }
    let (status, _) = shared.budget_status();
    let (Some(client), Some(vault)) = (shared.client.as_ref(), shared.vault.as_deref()) else {
        let reason = if translate {
            FallbackReason::TranslationNeedsApi
        } else {
            FallbackReason::from_error(CoreError::Network)
        };
        return PolishResult::raw(transcript, Some(reason));
    };
    shared.set_overlay(OverlayPhase::Polishing, translate, None);
    let t0 = Instant::now();
    let r = polish(client, vault, transcript, opts, &status).await;
    tracing::info!(
        in_len = transcript.len(),
        out_len = r.text.len(),
        fell_back = r.fell_back_to_raw,
        elapsed_ms = t0.elapsed().as_millis() as u64,
        "polished"
    );
    r
}

fn record_usage(shared: &Shared, usage: keepsay_core::Usage) {
    let mut b = lock(&shared.budget);
    if b.unreadable {
        return;
    }
    b.ledger.record(usage, now_month());
    if b.ledger.save(&shared.paths.budget).is_err() {
        tracing::warn!("budget ledger save failed");
    }
}
