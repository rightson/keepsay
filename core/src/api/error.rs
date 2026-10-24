//! Error type shared by every module and across the FFI boundary.
//!
//! FROZEN during M1 parallel work (docs/changes/0005): add a variant only by
//! coordinating with every workstream.
//!
//! Variants never carry the key, transcripts, model output, glossary, file
//! paths, URLs, or upstream error messages (INV-2, §6.9). Only fixed
//! descriptions, HTTP status codes and `std::io::ErrorKind` are allowed.

/// Every fallible core function returns this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    // --- key / vault ---
    /// 401 / 403 from the API. Never auto-delete the stored key (§6.3).
    #[error("key is invalid or revoked")]
    KeyInvalid,
    /// Fails the local `sk-ant-` format check (FR-1).
    #[error("key has the wrong format")]
    KeyFormat,
    /// No key in the OS secure storage.
    #[error("no key stored")]
    KeyNotFound,
    /// The OS secure storage refused or failed (Keychain / Credential Manager).
    #[error("secure storage is unavailable")]
    VaultUnavailable,

    // --- network / API ---
    /// A base URL or download URL that is not on the hardcoded allowlist.
    #[error("destination host is not allowed")]
    DisallowedHost,
    #[error("network error")]
    Network,
    #[error("request timed out")]
    Timeout,
    /// 429 / 529 after the single retry (§6.3).
    #[error("rate limited")]
    RateLimited,
    /// 400 with an insufficient-credit message (§6.3).
    #[error("account credit is exhausted")]
    BudgetExhausted,
    /// Any other non-success status. Only the code, never the body.
    #[error("unexpected HTTP status {0}")]
    ApiStatus(u16),
    /// The response body did not match the expected shape.
    #[error("unexpected response format")]
    InvalidResponse,
    /// A model ID outside `api::types::MODEL_ALLOWLIST`.
    #[error("model is not allowed")]
    ModelNotAllowed,

    // --- polish ---
    /// The output guard rejected the model output (§6.6).
    #[error("output rejected by guard")]
    GuardRejected,

    // --- budget ---
    /// The local monthly hard limit is reached (FR-6). Distinct from
    /// `BudgetExhausted`, which comes from the API.
    #[error("monthly hard limit reached")]
    HardLimitReached,

    // --- audio / STT ---
    /// Unsupported or malformed audio input (sample rate, WAV header, …).
    #[error("unsupported audio format")]
    AudioFormat,
    /// The STT engine failed (model load or inference).
    #[error("speech recognition failed")]
    Stt,

    // --- models / downloader ---
    #[error("unknown model id")]
    ModelUnknown,
    /// SHA-256 of a downloaded or imported model file does not match the
    /// manifest (INV-7).
    #[error("model checksum mismatch")]
    ModelHash,
    /// The download exceeded the size pinned in the manifest.
    #[error("model file is larger than the manifest allows")]
    ModelTooLarge,

    // --- config / io ---
    /// Settings, budget ledger or manifest could not be parsed or has an
    /// unsupported schema version.
    #[error("invalid configuration")]
    Config,
    /// Filesystem error. Only the kind is kept, never the path.
    #[error("i/o error: {0:?}")]
    Io(std::io::ErrorKind),

    /// The feature does not exist on this platform (e.g. no vault backend).
    #[error("not supported on this platform")]
    Unsupported,
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Io(e.kind())
    }
}

impl From<reqwest::Error> for CoreError {
    /// Drops the message and URL; keeps only the category.
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            CoreError::Timeout
        } else if let Some(status) = e.status() {
            CoreError::ApiStatus(status.as_u16())
        } else if e.is_decode() {
            CoreError::InvalidResponse
        } else {
            CoreError::Network
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_keeps_only_kind() {
        let e = std::io::Error::new(std::io::ErrorKind::NotFound, "/secret/path");
        let core: CoreError = e.into();
        assert_eq!(core, CoreError::Io(std::io::ErrorKind::NotFound));
        assert!(!core.to_string().contains("secret"));
    }
}
