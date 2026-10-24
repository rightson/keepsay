//! `AnthropicClient` — the single type that may attach `x-api-key` (INV-3).
//!
//! M1 adds the reqwest client: rustls + platform verifier, `redirect::Policy::none()`,
//! `no_proxy()`, connect 10 s / total 30 s (PROPOSAL §6.3).

use super::error::CoreError;

/// The one and only destination for requests that carry the key.
pub const BASE_URL: &str = "https://api.anthropic.com";

/// Placeholder until M1; holds the validated base URL.
#[derive(Debug)]
pub struct AnthropicClient {
    base_url: String,
}

impl AnthropicClient {
    /// Build a client for the production endpoint.
    pub fn new() -> Self {
        Self {
            base_url: BASE_URL.to_owned(),
        }
    }

    /// Build a client for an arbitrary base URL. Exists only so wiremock tests
    /// can point at localhost; the crate refuses to compile this feature in
    /// release builds.
    #[cfg(feature = "test-endpoint")]
    pub fn with_test_endpoint(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_owned(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Default for AnthropicClient {
    fn default() -> Self {
        Self::new()
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
