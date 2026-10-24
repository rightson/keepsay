//! OS secure storage for the API key (INV-1, INV-2).
//!
//! Owner: workstream C (docs/changes/0005).
//!
//! Nothing in this module returns the key across FFI/IPC; callers outside the
//! crate only ever see [`Fingerprint`]. Inside the crate, only
//! `AnthropicClient` calls [`KeyVault::load`].

#[cfg(target_vendor = "apple")]
pub mod apple;
#[cfg(any(test, feature = "test-support"))]
pub mod mock;
// The only module allowed to use `unsafe` (Win32 Credential Manager FFI,
// docs/changes/0005). It opts in with an inner attribute; CI rejects that
// attribute anywhere else.
#[cfg(target_os = "windows")]
pub mod windows;

use std::fmt;

use secrecy::{ExposeSecret, SecretString};

use crate::CoreError;

pub const KEY_PREFIX: &str = "sk-ant-";

/// Implemented by the Keychain (Apple), Credential Manager (Windows) and a
/// test mock. See PROPOSAL §6.2 for the exact storage attributes.
pub trait KeyVault: Send + Sync {
    fn store(&self, key: SecretString) -> Result<(), CoreError>;
    fn load(&self) -> Result<SecretString, CoreError>;
    fn delete(&self) -> Result<(), CoreError>;
    fn exists(&self) -> bool;
}

/// The OS vault for the current platform, or `Unsupported` elsewhere.
pub fn platform_vault() -> Result<Box<dyn KeyVault>, CoreError> {
    #[cfg(target_vendor = "apple")]
    {
        Ok(Box::new(apple::KeychainVault::new()))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::CredManVault::new()))
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "windows")))]
    {
        Err(CoreError::Unsupported)
    }
}

/// Cheap format check before the online verification (FR-1).
pub fn check_format(key: &SecretString) -> Result<(), CoreError> {
    let k = key.expose_secret();
    if k.starts_with(KEY_PREFIX)
        && k.len() > KEY_PREFIX.len() + 8
        && !k.chars().any(char::is_whitespace)
    {
        Ok(())
    } else {
        Err(CoreError::KeyFormat)
    }
}

/// `…` plus the last four characters of the key: the only key-derived value
/// allowed out of the core (INV-2). Constructed only by [`fingerprint`].
#[derive(Clone, PartialEq, Eq)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.0)
    }
}

impl From<Fingerprint> for String {
    fn from(fp: Fingerprint) -> Self {
        fp.0
    }
}

pub fn fingerprint(key: &SecretString) -> Fingerprint {
    let k = key.expose_secret();
    let tail: String = k
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Fingerprint(format!("…{tail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_last_four() {
        let key = SecretString::from("sk-ant-test-abcdefa1B2");
        assert_eq!(fingerprint(&key).as_str(), "…a1B2");
    }

    #[test]
    fn format_check() {
        assert!(check_format(&SecretString::from("sk-ant-api03-0123456789")).is_ok());
        assert!(check_format(&SecretString::from("sk-other-0123456789")).is_err());
        assert!(check_format(&SecretString::from("sk-ant-")).is_err());
        assert!(check_format(&SecretString::from("sk-ant-0123 456789")).is_err());
    }

    #[test]
    fn debug_is_redacted() {
        let key = SecretString::from("sk-ant-test-CANARY-7f3a");
        assert!(!format!("{key:?}").contains("CANARY"));
    }
}
