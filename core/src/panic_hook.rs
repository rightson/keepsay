//! Panic hook that records the location only (PROPOSAL §6.9, US-M1-20).
//!
//! A panic payload can contain anything a `format!` saw, including user
//! text, so it is never printed or logged. The hook emits one tracing event
//! with file and line; there is no crash upload or other sink (INV-6).

/// Replace the process panic hook. Idempotent. `ffi::init` calls it; Rust
/// hosts call it at startup (the CLI has its own stderr-only hook).
pub fn install() {
    std::panic::set_hook(Box::new(|info| match info.location() {
        Some(loc) => tracing::error!(file = loc.file(), line = loc.line(), "panic"),
        None => tracing::error!("panic"),
    }));
}
