//! `AnthropicClient` — the single type that may attach `x-api-key` (INV-3).
//!
//! Owner: workstream A (docs/changes/0005). The public signatures below are
//! the shared contract; bodies are A's to implement per PROPOSAL §6.3:
//! reqwest + rustls (platform verifier, reqwest's default for the `rustls`
//! feature), `redirect::Policy::none()`, `no_proxy()` unless the user set an
//! explicit proxy, connect 10 s / total 30 s, one retry on 429/529.

use secrecy::SecretString;

use super::error::CoreError;
use super::types::{MessagesRequest, MessagesResponse};
use crate::vault::KeyVault;

/// The one and only destination for requests that carry the key.
pub const BASE_URL: &str = "https://api.anthropic.com";

/// HTTP client for `BASE_URL`. Holds no key: the key is read from the vault
/// per request and dropped right after (§6.2).
#[derive(Debug)]
pub struct AnthropicClient {
    http: reqwest::Client,
    base_url: String,
}

impl AnthropicClient {
    /// Build a client for the production endpoint.
    ///
    /// `proxy` is the user's explicit proxy setting (`Config::proxy`); `None`
    /// means no proxy at all, and environment variables are never read.
    pub fn new(proxy: Option<&str>) -> Result<Self, CoreError> {
        let _ = proxy;
        todo!("M1: api — build reqwest client (§6.3)")
    }

    /// Build a client for an arbitrary base URL. Exists only so wiremock tests
    /// can point at localhost; the crate refuses to compile this feature in
    /// release builds.
    #[cfg(feature = "test-endpoint")]
    pub fn with_test_endpoint(base_url: &str) -> Result<Self, CoreError> {
        let _ = base_url;
        todo!("M1: api — test endpoint client")
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `GET /v1/models` with a key that is not stored yet (FR-1: verify, then
    /// store). 401/403 → `KeyInvalid`.
    pub async fn verify_key(&self, key: &SecretString) -> Result<(), CoreError> {
        let _ = (&self.http, key);
        todo!("M1: api — verify_key")
    }

    /// `POST /v1/messages`. Loads the key from `vault` at the point of use
    /// and drops it before returning. Errors map per §6.3.
    pub async fn messages(
        &self,
        vault: &dyn KeyVault,
        request: &MessagesRequest,
    ) -> Result<MessagesResponse, CoreError> {
        let _ = (vault, request);
        todo!("M1: api — messages")
    }
}

/// Accept a base URL only if it is exactly the hardcoded HTTPS endpoint.
///
/// No parsing tricks: anything other than `https://api.anthropic.com`
/// (optionally with one trailing `/`) is rejected, including other ports,
/// userinfo, paths, other schemes, and look-alike hosts.
pub fn validate_base_url(url: &str) -> Result<(), CoreError> {
    let trimmed = url.strip_suffix('/').unwrap_or(url);
    if trimmed == BASE_URL {
        Ok(())
    } else {
        Err(CoreError::DisallowedHost)
    }
}
