//! keepsay for Windows (M3): a Tauri 2 tray app over keepsay-core.
//!
//! The app is Windows only. On other targets this binary is a stub, so the
//! workspace still builds and the platform-independent modules (hotkeys,
//! session, audio, insertion order, IPC types) are tested on Linux CI
//! (docs/changes/0016).

// No console window for the release build.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::sync::{Mutex, MutexGuard, PoisonError};

#[cfg_attr(not(windows), allow(dead_code))]
mod audio;
#[cfg_attr(not(windows), allow(dead_code))]
mod hotkey;
#[cfg_attr(not(windows), allow(dead_code))]
mod insert;
#[cfg_attr(not(windows), allow(dead_code))]
mod ipc;
#[cfg_attr(not(windows), allow(dead_code))]
mod session;
#[cfg_attr(not(windows), allow(dead_code))]
mod win32;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod pipeline;
#[cfg(windows)]
mod recorder;
#[cfg(windows)]
mod tray;

/// Lock, ignoring poisoning: every guarded value stays consistent between
/// statements, and a panic elsewhere must not take the tray app down.
#[cfg_attr(not(windows), allow(dead_code))]
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("keepsay-windows only runs on Windows");
    std::process::exit(2);
}
