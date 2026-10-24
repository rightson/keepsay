//! Error type shared across the FFI boundary.
//!
//! Variants never carry the key, transcripts, or model output (INV-2, §6.9).

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("key is invalid or revoked")]
    KeyInvalid,
    #[error("no key stored")]
    NoKey,
    #[error("key has the wrong format")]
    KeyFormat,
    #[error("account credit is exhausted")]
    BudgetExhausted,
    #[error("destination host is not allowed")]
    DisallowedHost,
    #[error("network error")]
    Network,
    #[error("request timed out")]
    Timeout,
    #[error("secure storage error")]
    Vault,
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("not implemented yet")]
    NotImplemented,
}
