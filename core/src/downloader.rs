//! User-triggered model download (INV-7).
//!
//! IMPORTANT: this module must never import `crate::vault`, `secrecy`, or the
//! API client. `core/tests/downloader.rs` scans this file to enforce that.
//! Every file is checked against the SHA-256 pinned in `models/manifest.toml`.

use crate::CoreError;

/// A separate HTTP client from `AnthropicClient`, with no access to the key.
#[derive(Debug, Default)]
pub struct ModelDownloader;

impl ModelDownloader {
    pub fn download(&self, _model_id: &str) -> Result<(), CoreError> {
        Err(CoreError::NotImplemented)
    }
}
