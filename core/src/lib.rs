//! keepsay-core: the security-critical half of keepsay.
//!
//! Hard rules live in `docs/specs/PROPOSAL.md` §3 (INV-1 … INV-10). The ones
//! this crate enforces structurally:
//! - INV-2: the key only ever exists here, as `secrecy::SecretString`.
//! - INV-3: only [`api::AnthropicClient`] may attach the key, and only to
//!   [`api::BASE_URL`].
//! - INV-4: [`api::MessagesRequest`] has no tool fields at all.
//! - INV-7: [`downloader`] never touches [`vault`].
//!
//! The UniFFI surface for the Swift apps is [`ffi`] (PROPOSAL §6.1,
//! docs/changes/0014). Rust hosts (CLI, Tauri) call the modules directly.

#[cfg(all(feature = "test-endpoint", not(debug_assertions)))]
compile_error!("feature `test-endpoint` must never be enabled in release builds (INV-3)");
#[cfg(all(feature = "test-support", not(debug_assertions)))]
compile_error!("feature `test-support` must never be enabled in release builds (INV-1, INV-2)");

pub mod api;
pub mod budget;
pub mod config;
pub mod downloader;
pub mod ffi;
pub mod polish;
pub mod stt;
mod tls;
pub mod vault;

pub use api::error::CoreError;
pub use budget::{BudgetStatus, Usage};
pub use polish::{FallbackReason, Mode, PolishOptions, PolishResult, TargetLang};
pub use stt::{SttEngine, SttOptions};
pub use vault::{Fingerprint, KeyVault};

uniffi::setup_scaffolding!();
