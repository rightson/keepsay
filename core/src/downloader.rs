//! User-triggered model download and manual import (INV-7, PROPOSAL §8).
//!
//! Owner: workstream D (docs/changes/0005, 0009).
//!
//! IMPORTANT: this module must never import the key store, the secret
//! wrapper crate, or the Anthropic client. `core/tests/downloader.rs` scans
//! this file to enforce that, so those names must not appear in code here.
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
//!                 "size": 0, "license": "MIT",
//!                 "redirect_hosts": [".hf.co"] } ] }
//! ```
//!
//! - `url` must be `https://`, pinned to an immutable revision (the 40-hex
//!   `revision` appears in the URL path), never a moving branch.
//! - `sha256` is computed by a maintainer who downloaded the file, never
//!   copied from a third-party listing.
//! - `size` is in bytes; downloads larger than this are aborted, and a file
//!   of any other size is rejected.
//! - `redirect_hosts` (optional, default empty) lists the hosts a redirect
//!   may lead to besides the `url` host itself. An entry starting with `.`
//!   matches that domain's subdomains (`.hf.co` matches `us.aws.cdn.hf.co`,
//!   not `hf.co` or `evilhf.co`). Redirect targets must be `https://`.
//!
//! Redirect policy (docs/changes/0009): the reqwest client follows no
//! redirect itself (`Policy::none()`). `download` follows at most
//! [`MAX_REDIRECTS`] hops by hand and checks every `Location` against the
//! entry before connecting. Integrity does not depend on the host (the hash
//! does that); the host check keeps egress to the places the manifest names.
//!
//! On-disk layout: the model lands at `<dest_dir>/<id>.bin` (see
//! [`model_path`]). While downloading or importing, bytes go to a
//! `.<id>.bin.part-*` file in the same directory, which is renamed into place
//! only after the size and hash match, and removed on any failure or when the
//! future is dropped (cancellation).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::CoreError;

/// The manifest shipped in the binary.
const BUNDLED_MANIFEST: &str = include_str!("../../models/manifest.json");

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Redirect hops `download` follows before giving up.
pub const MAX_REDIRECTS: usize = 5;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Per-read idle timeout. There is no total timeout: large models on slow
/// links legitimately take a long time.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// Extra hosts a redirect may lead to; see the module docs.
    #[serde(default)]
    pub redirect_hosts: Vec<String>,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, CoreError> {
        let m: Manifest = serde_json::from_str(json).map_err(|_| CoreError::Config)?;
        if m.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(CoreError::Config);
        }
        for (i, e) in m.models.iter().enumerate() {
            e.validate()?;
            if m.models[..i].iter().any(|o| o.id == e.id) {
                return Err(CoreError::Config);
            }
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

impl ModelEntry {
    /// Static checks on an entry. `Manifest::parse` runs this for every
    /// entry; `download` and `import` run it again on whatever they are given.
    pub fn validate(&self) -> Result<(), CoreError> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 64
            && !self.id.starts_with('.')
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b));
        let sha_ok = is_lower_hex(&self.sha256, 64);
        let rev_ok = is_lower_hex(&self.revision, 40);
        let hosts_ok = self.redirect_hosts.iter().all(|h| {
            !h.is_empty()
                && h != "."
                && h.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.".contains(&b))
        });
        if !(id_ok && sha_ok && rev_ok && hosts_ok && self.size > 0) {
            return Err(CoreError::Config);
        }
        let url = Url::parse(&self.url).map_err(|_| CoreError::Config)?;
        if !url.path().contains(&format!("/{}/", self.revision)) {
            return Err(CoreError::Config);
        }
        if url.scheme() != "https" {
            // Plain HTTP is only reachable for loopback test servers, and only
            // with the `test-endpoint` feature (never in release builds).
            #[cfg(feature = "test-endpoint")]
            if url.scheme() == "http" && is_loopback(&url) {
                return Ok(());
            }
            return Err(CoreError::DisallowedHost);
        }
        if !url.username().is_empty() || url.password().is_some() || url.host_str().is_none() {
            return Err(CoreError::DisallowedHost);
        }
        Ok(())
    }
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(feature = "test-endpoint")]
fn is_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
}

/// Where the model for `entry` lives inside `dir`: `<dir>/<id>.bin`.
pub fn model_path(entry: &ModelEntry, dir: &Path) -> PathBuf {
    dir.join(format!("{}.bin", entry.id))
}

/// Progress callback: (bytes received, total bytes from the manifest).
pub type Progress<'a> = &'a mut (dyn FnMut(u64, u64) + Send);

/// A separate HTTP client from the one that carries the key, with no access
/// to it. Fetches only manifest URLs and follows only checked redirects.
#[derive(Debug)]
pub struct ModelDownloader {
    http: reqwest::Client,
    /// Accept `http://` loopback URLs. Exists only with `test-endpoint`.
    #[cfg(feature = "test-endpoint")]
    allow_loopback_http: bool,
}

impl ModelDownloader {
    pub fn new() -> Result<Self, CoreError> {
        Ok(Self {
            http: build_client()?,
            #[cfg(feature = "test-endpoint")]
            allow_loopback_http: false,
        })
    }

    /// A downloader that also accepts `http://127.0.0.1` / `localhost` URLs,
    /// for wiremock tests. Exists only with `test-endpoint`, which a release
    /// build refuses at compile time.
    #[cfg(feature = "test-endpoint")]
    pub fn with_test_endpoint() -> Result<Self, CoreError> {
        Ok(Self {
            http: build_client()?,
            allow_loopback_http: true,
        })
    }

    /// Download `entry` into `dest_dir`, verify size and SHA-256, and return
    /// the final path ([`model_path`]). Nothing is left behind on failure or
    /// if the returned future is dropped. An existing file at the final path
    /// is replaced only after the new one verifies.
    pub async fn download(
        &self,
        entry: &ModelEntry,
        dest_dir: &Path,
        progress: Progress<'_>,
    ) -> Result<PathBuf, CoreError> {
        entry.validate()?;
        let start = Url::parse(&entry.url).map_err(|_| CoreError::Config)?;
        self.check_scheme(&start)?;

        fs::create_dir_all(dest_dir)?;
        let mut part = PartFile::create(entry, dest_dir)?;

        let mut url = start.clone();
        let mut hops = 0;
        let mut resp = loop {
            let resp = self.http.get(url.clone()).send().await?;
            if !resp.status().is_redirection() {
                break resp;
            }
            hops += 1;
            if hops > MAX_REDIRECTS {
                return Err(CoreError::Network);
            }
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(CoreError::InvalidResponse)?;
            let next = url.join(location).map_err(|_| CoreError::InvalidResponse)?;
            self.check_redirect(entry, &start, &next)?;
            url = next;
        };

        let status = resp.status();
        if !status.is_success() {
            return Err(CoreError::ApiStatus(status.as_u16()));
        }
        if resp.content_length().is_some_and(|n| n > entry.size) {
            return Err(CoreError::ModelTooLarge);
        }

        let mut hasher = Sha256::new();
        let mut received: u64 = 0;
        progress(0, entry.size);
        while let Some(chunk) = resp.chunk().await? {
            received = received.saturating_add(chunk.len() as u64);
            if received > entry.size {
                return Err(CoreError::ModelTooLarge);
            }
            hasher.update(&chunk);
            part.file.write_all(&chunk)?;
            progress(received, entry.size);
        }

        if received != entry.size || hex(&hasher.finalize()) != entry.sha256 {
            return Err(CoreError::ModelHash);
        }
        part.commit(&model_path(entry, dest_dir))
    }

    fn check_scheme(&self, url: &Url) -> Result<(), CoreError> {
        if url.scheme() == "https" {
            return Ok(());
        }
        #[cfg(feature = "test-endpoint")]
        if self.allow_loopback_http && url.scheme() == "http" && is_loopback(url) {
            return Ok(());
        }
        Err(CoreError::DisallowedHost)
    }

    /// A redirect may go to the start URL's origin, or over HTTPS to a host
    /// listed in `entry.redirect_hosts`.
    fn check_redirect(&self, entry: &ModelEntry, start: &Url, next: &Url) -> Result<(), CoreError> {
        self.check_scheme(next)?;
        if !next.username().is_empty() || next.password().is_some() {
            return Err(CoreError::DisallowedHost);
        }
        let same_origin = next.scheme() == start.scheme()
            && next.host_str() == start.host_str()
            && next.port_or_known_default() == start.port_or_known_default();
        if same_origin {
            return Ok(());
        }
        let host = next.host_str().ok_or(CoreError::DisallowedHost)?;
        let listed = entry
            .redirect_hosts
            .iter()
            .any(|h| match h.strip_prefix('.') {
                Some(domain) => host
                    .strip_suffix(domain)
                    .is_some_and(|sub| sub.len() > 1 && sub.ends_with('.')),
                None => host == h,
            });
        if listed {
            Ok(())
        } else {
            Err(CoreError::DisallowedHost)
        }
    }
}

fn build_client() -> Result<reqwest::Client, CoreError> {
    crate::tls::ensure_crypto_provider();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .map_err(|_| CoreError::Network)
}

/// Copy a user-supplied model file into `dest_dir` after the same size and
/// SHA-256 check as a download (§8, manual import). No network. The hash is
/// computed over the bytes actually written, so a source file that changes
/// mid-copy cannot slip through.
pub fn import(entry: &ModelEntry, src: &Path, dest_dir: &Path) -> Result<PathBuf, CoreError> {
    // Imports skip the URL scheme check: the URL is never used here.
    match entry.validate() {
        Ok(()) | Err(CoreError::DisallowedHost) => {}
        Err(e) => return Err(e),
    }
    let mut input = File::open(src)?;
    if input.metadata()?.len() != entry.size {
        return Err(CoreError::ModelHash);
    }
    fs::create_dir_all(dest_dir)?;
    let mut part = PartFile::create(entry, dest_dir)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut copied: u64 = 0;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        copied = copied.saturating_add(n as u64);
        if copied > entry.size {
            return Err(CoreError::ModelHash);
        }
        hasher.update(&buf[..n]);
        part.file.write_all(&buf[..n])?;
    }
    if copied != entry.size || hex(&hasher.finalize()) != entry.sha256 {
        return Err(CoreError::ModelHash);
    }
    part.commit(&model_path(entry, dest_dir))
}

/// Verify a file on disk against `entry.sha256` and `entry.size`.
pub fn verify_file(entry: &ModelEntry, path: &Path) -> Result<(), CoreError> {
    let mut f = File::open(path)?;
    if f.metadata()?.len() != entry.size {
        return Err(CoreError::ModelHash);
    }
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n as u64);
        hasher.update(&buf[..n]);
    }
    if total != entry.size || hex(&hasher.finalize()) != entry.sha256 {
        return Err(CoreError::ModelHash);
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[usize::from(b >> 4)] as char);
        s.push(DIGITS[usize::from(b & 0xf)] as char);
    }
    s
}

/// A `.part` file next to the final path. Removed on drop unless committed,
/// which makes `download` cancellation-safe: dropping the future drops this.
struct PartFile {
    path: PathBuf,
    file: File,
    committed: bool,
}

impl PartFile {
    fn create(entry: &ModelEntry, dir: &Path) -> Result<Self, CoreError> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = dir.join(format!(
            ".{}.bin.part-{}-{nonce}",
            entry.id,
            std::process::id()
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            file,
            committed: false,
        })
    }

    fn commit(mut self, dest: &Path) -> Result<PathBuf, CoreError> {
        self.file.sync_all()?;
        fs::rename(&self.path, dest)?;
        self.committed = true;
        Ok(dest.to_path_buf())
    }
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manifest_parses() {
        assert!(Manifest::bundled().is_ok());
    }

    #[test]
    fn bundled_entries_are_pinned_https() {
        let m = Manifest::bundled().unwrap();
        for id in ["whisper-small", "whisper-large-v3-turbo"] {
            assert!(m.find(id).is_ok(), "missing {id}");
        }
        for e in &m.models {
            let url = Url::parse(&e.url).unwrap();
            assert_eq!(url.scheme(), "https");
            assert_eq!(url.host_str(), Some("huggingface.co"));
            assert!(url.path().contains(&format!("/resolve/{}/", e.revision)));
        }
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

    fn entry(url: &str) -> ModelEntry {
        ModelEntry {
            id: "m".into(),
            name: "m".into(),
            url: url.into(),
            revision: "a".repeat(40),
            sha256: "0".repeat(64),
            size: 1,
            license: "MIT".into(),
            redirect_hosts: vec![".hf.co".into(), "cdn.example.org".into()],
        }
    }

    #[test]
    fn entry_validation() {
        let rev = "a".repeat(40);
        let good = format!("https://huggingface.co/x/resolve/{rev}/f.bin");
        assert_eq!(entry(&good).validate(), Ok(()));
        let unpinned = "https://huggingface.co/x/resolve/main/f.bin";
        assert_eq!(entry(unpinned).validate(), Err(CoreError::Config));
        let http = format!("http://huggingface.co/x/resolve/{rev}/f.bin");
        assert_eq!(entry(&http).validate(), Err(CoreError::DisallowedHost));
        let creds = format!("https://u:p@huggingface.co/x/resolve/{rev}/f.bin");
        assert_eq!(entry(&creds).validate(), Err(CoreError::DisallowedHost));
        let mut bad_id = entry(&good);
        bad_id.id = "../x".into();
        assert_eq!(bad_id.validate(), Err(CoreError::Config));
        let mut bad_sha = entry(&good);
        bad_sha.sha256 = "A".repeat(64);
        assert_eq!(bad_sha.validate(), Err(CoreError::Config));
    }

    #[test]
    fn redirect_host_matching() {
        let d = ModelDownloader::new().unwrap();
        let rev = "a".repeat(40);
        let start = Url::parse(&format!("https://huggingface.co/x/resolve/{rev}/f.bin")).unwrap();
        let e = entry(start.as_str());
        let ok = |u: &str| d.check_redirect(&e, &start, &Url::parse(u).unwrap());
        assert_eq!(ok("https://huggingface.co/other"), Ok(()));
        assert_eq!(ok("https://us.aws.cdn.hf.co/blob"), Ok(()));
        assert_eq!(ok("https://cdn.example.org/blob"), Ok(()));
        for bad in [
            "https://hf.co/blob",
            "https://evilhf.co/blob",
            "https://hf.co.evil.com/blob",
            "https://sub.cdn.example.org/blob",
            "http://us.aws.cdn.hf.co/blob",
            "http://huggingface.co/other",
            "https://huggingface.co:8443/other",
            "https://u@us.aws.cdn.hf.co/blob",
        ] {
            assert_eq!(ok(bad), Err(CoreError::DisallowedHost), "{bad}");
        }
    }

    #[test]
    fn hex_is_lowercase() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
        assert_eq!(
            hex(&Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
