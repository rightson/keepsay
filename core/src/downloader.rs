//! User-triggered model download and manual import (INV-7, PROPOSAL §8).
//!
//! Owner: workstream D (docs/changes/0005).
//!
//! IMPORTANT: this module must never import `crate::vault`, `secrecy`, or the
//! API client. `core/tests/downloader.rs` scans this file to enforce that.
//! Every file is checked against the SHA-256 pinned in `models/manifest.json`,
//! which is compiled into the binary; there is no remote manifest.
//!
//! Manifest schema (`deny_unknown_fields`):
//!
//! ```json
//! { "schema_version": 1,
//!   "models": [ { "id": "whisper-small", "name": "Whisper small",
//!                 "url": "https://…/resolve/<revision>/<file>.bin",
//!                 "revision": "<commit sha>", "sha256": "<64 lowercase hex>",
//!                 "size": 0, "license": "MIT" } ] }
//! ```
//!
//! - `url` must be pinned to an immutable revision, never a moving branch.
//! - `sha256` is computed by a maintainer who downloaded the file, never
//!   copied from a third-party listing.
//! - `size` is in bytes; downloads larger than this are aborted.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::CoreError;

/// The manifest shipped in the binary.
const BUNDLED_MANIFEST: &str = include_str!("../../models/manifest.json");

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    pub revision: String,
    pub sha256: String,
    pub size: u64,
    pub license: String,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, CoreError> {
        let m: Manifest = serde_json::from_str(json).map_err(|_| CoreError::Config)?;
        if m.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(CoreError::Config);
        }
        Ok(m)
    }

    /// The manifest compiled into this binary.
    pub fn bundled() -> Result<Self, CoreError> {
        Self::parse(BUNDLED_MANIFEST)
    }

    pub fn find(&self, id: &str) -> Result<&ModelEntry, CoreError> {
        self.models
            .iter()
            .find(|m| m.id == id)
            .ok_or(CoreError::ModelUnknown)
    }
}

/// Progress callback: (bytes received, total bytes from the manifest).
pub type Progress<'a> = &'a mut (dyn FnMut(u64, u64) + Send);

/// A separate HTTP client from the one in `api`, with no access to the key.
/// Fetches only manifest URLs, follows no redirect outside them.
#[derive(Debug)]
pub struct ModelDownloader {
    http: reqwest::Client,
}

impl ModelDownloader {
    pub fn new() -> Result<Self, CoreError> {
        todo!("M1: downloader — build reqwest client")
    }

    /// Download `entry` into `dest_dir`, verify size and SHA-256, and return
    /// the final path. Nothing is left behind on failure.
    pub async fn download(
        &self,
        entry: &ModelEntry,
        dest_dir: &Path,
        progress: Progress<'_>,
    ) -> Result<PathBuf, CoreError> {
        let _ = (&self.http, entry, dest_dir, progress);
        todo!("M1: downloader — download")
    }
}

/// Copy a user-supplied model file into `dest_dir` after the same SHA-256
/// check as a download (§8, manual import).
pub fn import(entry: &ModelEntry, src: &Path, dest_dir: &Path) -> Result<PathBuf, CoreError> {
    let _ = (entry, src, dest_dir);
    todo!("M1: downloader — manual import")
}

/// Verify a file on disk against `entry.sha256` and `entry.size`.
pub fn verify_file(entry: &ModelEntry, path: &Path) -> Result<(), CoreError> {
    let _ = (entry, path);
    todo!("M1: downloader — sha256 verify")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manifest_parses() {
        assert!(Manifest::bundled().is_ok());
    }

    #[test]
    fn unknown_fields_and_versions_are_rejected() {
        assert_eq!(
            Manifest::parse(r#"{"schema_version":1,"models":[],"mirror":"x"}"#),
            Err(CoreError::Config)
        );
        assert_eq!(
            Manifest::parse(r#"{"schema_version":2,"models":[]}"#),
            Err(CoreError::Config)
        );
    }
}
