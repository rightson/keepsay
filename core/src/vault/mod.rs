//! OS secure storage for the API key (INV-1, INV-2).
//!
//! Nothing in this module returns the key across FFI/IPC; callers outside the
//! crate only ever see [`fingerprint`].

#[cfg(target_vendor = "apple")]
pub mod apple;
#[cfg(test)]
pub mod mock;
#[cfg(target_os = "windows")]
pub mod windows;

use secrecy::{ExposeSecret, SecretString};

use crate::CoreError;

pub const KEY_PREFIX: &str = "sk-ant-";

/// Implemented by the Keychain (Apple), Credential Manager (Windows) and a
/// test mock. See PROPOSAL §6.2 for the exact storage attributes.
pub trait KeyVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError>;
    fn load(&self) -> Result<SecretString, CoreError>;
    fn delete(&self) -> Result<(), CoreError>;
    fn exists(&self) -> bool;
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

/// The only key-derived value allowed out of the core: `…` plus the last
/// four characters.
pub fn fingerprint(key: &SecretString) -> String {
    let k = key.expose_secret();
    let tail: String = k
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_last_four() {
        let key = SecretString::from("sk-ant-test-abcdefa1B2");
        assert_eq!(fingerprint(&key), "…a1B2");
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
