//! `AnthropicClient` — the single type that may attach `x-api-key` (INV-3).
//!
//! Owner: workstream A (docs/changes/0005). Implements PROPOSAL §6.3:
//! reqwest + rustls (platform verifier, reqwest's default for the `rustls`
//! feature), `redirect::Policy::none()`, `no_proxy()` unless the user set an
//! explicit proxy, connect 10 s / total 30 s, one retry on 429/529.
//! API facts (version header, error shapes) are from V4, docs/changes/0006.
//!
//! Nothing here logs or returns the key, request bodies, response bodies or
//! upstream error messages. Only status codes and timings are traced.

use std::fmt;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderValue, RETRY_AFTER};
use reqwest::{RequestBuilder, Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use super::error::CoreError;
use super::types::{MODEL_ALLOWLIST, MessagesRequest, MessagesResponse};
use crate::vault::KeyVault;

/// The one and only destination for requests that carry the key.
pub const BASE_URL: &str = "https://api.anthropic.com";

/// `anthropic-version` header value (V4: latest version as of 2026-10-24).
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// Connect timeout (§6.3).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Total request timeout (§6.3).
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
/// Upper bound for the single 429/529 back-off, whatever `retry-after` says.
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(2);
/// Back-off when a 429/529 carries no usable `retry-after`.
const DEFAULT_RETRY_DELAY: Duration = Duration::from_secs(1);

/// HTTP client for `BASE_URL`. Holds no key: the key is read from the vault
/// per request and dropped right after (§6.2).
pub struct AnthropicClient {
    http: reqwest::Client,
    base_url: String,
}

/// Hand-written: reqwest's `Debug` would print the proxy configuration,
/// which may carry proxy credentials.
impl fmt::Debug for AnthropicClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl AnthropicClient {
    /// Build a client for the production endpoint.
    ///
    /// `proxy` is the user's explicit proxy setting (`Config::proxy`); `None`
    /// means no proxy at all, and environment variables are never read.
    pub fn new(proxy: Option<&str>) -> Result<Self, CoreError> {
        Self::build(BASE_URL, proxy, TOTAL_TIMEOUT, true)
    }

    /// Build a client for an arbitrary base URL. Exists only so wiremock tests
    /// can point at localhost; the crate refuses to compile this feature in
    /// release builds. Only plain-HTTP loopback URLs are accepted.
    #[cfg(feature = "test-endpoint")]
    pub fn with_test_endpoint(base_url: &str) -> Result<Self, CoreError> {
        Self::with_test_endpoint_timeout(base_url, TOTAL_TIMEOUT)
    }

    /// Like [`Self::with_test_endpoint`] with a shorter total timeout, so the
    /// timeout mapping can be tested without waiting 30 s.
    #[cfg(feature = "test-endpoint")]
    pub fn with_test_endpoint_timeout(
        base_url: &str,
        total_timeout: Duration,
    ) -> Result<Self, CoreError> {
        let rest = base_url
            .strip_prefix("http://127.0.0.1:")
            .or_else(|| base_url.strip_prefix("http://localhost:"))
            .ok_or(CoreError::DisallowedHost)?;
        let port = rest.strip_suffix('/').unwrap_or(rest);
        if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
            return Err(CoreError::DisallowedHost);
        }
        let base = base_url.strip_suffix('/').unwrap_or(base_url);
        Self::build_unchecked(base, None, total_timeout, false)
    }

    fn build(
        base_url: &str,
        proxy: Option<&str>,
        total_timeout: Duration,
        https_only: bool,
    ) -> Result<Self, CoreError> {
        validate_base_url(base_url)?;
        let base = base_url.strip_suffix('/').unwrap_or(base_url);
        Self::build_unchecked(base, proxy, total_timeout, https_only)
    }

    fn build_unchecked(
        base_url: &str,
        proxy: Option<&str>,
        total_timeout: Duration,
        https_only: bool,
    ) -> Result<Self, CoreError> {
        crate::tls::ensure_crypto_provider();
        let mut builder = reqwest::Client::builder()
            // INV-3: a redirect is returned as a 3xx status, never followed,
            // so the key can never be re-sent to a `Location` host.
            .redirect(reqwest::redirect::Policy::none())
            // Retries are explicit and bounded (one, on 429/529 only).
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(total_timeout)
            .referer(false)
            .https_only(https_only);
        if let Some(p) = proxy {
            let p = reqwest::Proxy::all(p).map_err(|_| CoreError::Config)?;
            builder = builder.proxy(p);
        }
        let http = builder.build().map_err(|_| CoreError::Network)?;
        Ok(Self {
            http,
            base_url: base_url.to_owned(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `GET /v1/models` with a key that is not stored yet (FR-1: verify, then
    /// store). 401/403 → `KeyInvalid`.
    pub async fn verify_key(&self, key: &SecretString) -> Result<(), CoreError> {
        let url = format!("{}/v1/models?limit=1", self.base_url);
        let resp = self
            .send_with_retry("models", || {
                Ok(self.http.get(&url).header("x-api-key", key_header(key)?))
            })
            .await?;
        // Drain the body so the connection can be reused; content unused.
        let _ = resp.bytes().await.map_err(CoreError::from)?;
        Ok(())
    }

    /// `POST /v1/messages`. Loads the key from `vault` at the point of use
    /// and drops it before returning. Errors map per §6.3.
    pub async fn messages(
        &self,
        vault: &dyn KeyVault,
        request: &MessagesRequest,
    ) -> Result<MessagesResponse, CoreError> {
        if !MODEL_ALLOWLIST.contains(&request.model.as_str()) {
            return Err(CoreError::ModelNotAllowed);
        }
        let url = format!("{}/v1/messages", self.base_url);
        let resp = self
            .send_with_retry("messages", || {
                let key = vault.load()?;
                let value = key_header(&key)?;
                drop(key);
                Ok(self
                    .http
                    .post(&url)
                    .header("x-api-key", value)
                    .json(request))
            })
            .await?;
        let body = resp.bytes().await.map_err(CoreError::from)?;
        serde_json::from_slice(&body).map_err(|_| CoreError::InvalidResponse)
    }

    /// Send, and on 429/529 wait `min(retry-after, 2 s)` and send exactly
    /// once more. `build` is called per attempt so the key is read fresh and
    /// not held across the back-off.
    async fn send_with_retry<F>(
        &self,
        endpoint: &'static str,
        build: F,
    ) -> Result<Response, CoreError>
    where
        F: Fn() -> Result<RequestBuilder, CoreError>,
    {
        let mut attempt = 0u8;
        loop {
            attempt += 1;
            let request = build()?.header("anthropic-version", ANTHROPIC_VERSION);
            let started = Instant::now();
            let resp = request.send().await.map_err(CoreError::from)?;
            let status = resp.status();
            tracing::debug!(
                endpoint,
                attempt,
                status = status.as_u16(),
                elapsed_ms = started.elapsed().as_millis() as u64,
                "anthropic api response"
            );
            if status.is_success() {
                return Ok(resp);
            }
            match classify(resp).await {
                Failure::Retryable(delay) if attempt == 1 => tokio::time::sleep(delay).await,
                Failure::Retryable(_) => return Err(CoreError::RateLimited),
                Failure::Fatal(e) => return Err(e),
            }
        }
    }
}

/// Build the `x-api-key` header value, marked sensitive so hyper never
/// prints it and HPACK never indexes it.
fn key_header(key: &SecretString) -> Result<HeaderValue, CoreError> {
    let mut value = HeaderValue::from_str(key.expose_secret()).map_err(|_| CoreError::KeyFormat)?;
    value.set_sensitive(true);
    Ok(value)
}

enum Failure {
    Retryable(Duration),
    Fatal(CoreError),
}

/// Error body: `{"type":"error","error":{"type":…,"message":…,"details":…}}`.
/// Read only to classify; never stored, logged or returned.
#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    details: Option<ErrorDetails>,
}

#[derive(Deserialize)]
struct ErrorDetails {
    #[serde(default)]
    error_code: Option<String>,
}

async fn classify(resp: Response) -> Failure {
    let status = resp.status();
    let retry_after = retry_delay(resp.headers().get(RETRY_AFTER));
    let body: Option<ErrorBody> = match status.as_u16() {
        400 | 429 => resp
            .bytes()
            .await
            .ok()
            .and_then(|b| serde_json::from_slice::<ErrorEnvelope>(&b).ok())
            .map(|e| e.error),
        _ => None,
    };
    match status.as_u16() {
        // Never auto-delete the stored key (§6.3).
        401 | 403 => Failure::Fatal(CoreError::KeyInvalid),
        402 => Failure::Fatal(CoreError::BudgetExhausted),
        400 if body.as_ref().is_some_and(is_credit_exhausted) => {
            Failure::Fatal(CoreError::BudgetExhausted)
        }
        429 if body.as_ref().is_some_and(is_spend_cap) => {
            Failure::Fatal(CoreError::BudgetExhausted)
        }
        429 | 529 => Failure::Retryable(retry_after),
        _ if status == StatusCode::REQUEST_TIMEOUT || status.as_u16() == 504 => {
            Failure::Fatal(CoreError::Timeout)
        }
        code => Failure::Fatal(CoreError::ApiStatus(code)),
    }
}

/// 400 `invalid_request_error` for a spend limit the user set ("You have
/// reached your specified [workspace] API usage limits …") or exhausted
/// prepaid credit ("… credit balance is too low …"). V4, docs/changes/0006.
fn is_credit_exhausted(e: &ErrorBody) -> bool {
    if e.kind != "invalid_request_error" {
        return false;
    }
    let m = e.message.to_ascii_lowercase();
    m.contains("credit balance") || m.contains("api usage limits")
}

/// 429 for the tier's monthly spend cap. It has no `retry-after` and keeps
/// failing until next month, so retrying is pointless.
fn is_spend_cap(e: &ErrorBody) -> bool {
    e.details
        .as_ref()
        .and_then(|d| d.error_code.as_deref())
        .is_some_and(|c| c == "enforced_spend_limit_reached")
}

/// `retry-after` in seconds (integer or decimal), capped at 2 s.
fn retry_delay(header: Option<&HeaderValue>) -> Duration {
    header
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(|s| Duration::from_secs_f64(s.min(MAX_RETRY_DELAY.as_secs_f64())))
        .unwrap_or(DEFAULT_RETRY_DELAY)
        .min(MAX_RETRY_DELAY)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn hv(s: &str) -> HeaderValue {
        HeaderValue::from_str(s).unwrap()
    }

    #[test]
    fn retry_delay_is_capped() {
        assert_eq!(retry_delay(Some(&hv("10"))), MAX_RETRY_DELAY);
        assert_eq!(retry_delay(Some(&hv("0"))), Duration::ZERO);
        assert_eq!(retry_delay(Some(&hv("1.5"))), Duration::from_millis(1500));
        assert_eq!(retry_delay(None), DEFAULT_RETRY_DELAY);
        assert_eq!(retry_delay(Some(&hv("-3"))), DEFAULT_RETRY_DELAY);
        assert_eq!(
            retry_delay(Some(&hv("Wed, 21 Oct 2026 07:28:00 GMT"))),
            DEFAULT_RETRY_DELAY
        );
    }

    #[test]
    fn production_client_targets_base_url() {
        let c = AnthropicClient::new(None).unwrap();
        assert_eq!(c.base_url(), BASE_URL);
        assert!(!format!("{c:?}").contains("proxy"));
    }

    #[test]
    fn proxy_is_opt_in_and_not_printed() {
        let c = AnthropicClient::new(Some("http://user:pw@127.0.0.1:3128")).unwrap();
        assert!(!format!("{c:?}").contains("pw"));
        assert_eq!(
            AnthropicClient::new(Some("not a url")).unwrap_err(),
            CoreError::Config
        );
    }

    #[test]
    fn non_whitelisted_base_url_fails_construction() {
        for url in [
            "http://api.anthropic.com",
            "https://api.anthropic.com.evil.example",
            "https://evil.example",
        ] {
            assert_eq!(
                AnthropicClient::build(url, None, TOTAL_TIMEOUT, true).unwrap_err(),
                CoreError::DisallowedHost
            );
        }
    }

    #[test]
    fn key_header_is_sensitive() {
        let key = SecretString::from("sk-ant-test-0123456789");
        assert!(key_header(&key).unwrap().is_sensitive());
        let bad = SecretString::from("sk-ant-\n0123456789");
        assert_eq!(key_header(&bad).unwrap_err(), CoreError::KeyFormat);
    }
}
