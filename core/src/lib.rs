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
//! M0 status: types and seams only; behaviour lands in M1.

#[cfg(all(feature = "test-endpoint", not(debug_assertions)))]
compile_error!("feature `test-endpoint` must never be enabled in release builds (INV-3)");

pub mod api;
pub mod budget;
pub mod config;
pub mod downloader;
pub mod ffi;
pub mod polish;
pub mod stt;
pub mod vault;

pub use api::error::CoreError;
