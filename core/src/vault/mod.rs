//! OS secure storage for the API key (INV-1, INV-2).
//!
//! Owner: workstream C (docs/changes/0005, 0008).
//!
//! Nothing in this module returns the key across FFI/IPC; callers outside the
//! crate only ever see [`Fingerprint`]. Only `AnthropicClient` calls
//! [`KeyVault::load`], with one exception kept inside this module: the
//! default [`KeyVault::key_fingerprint`] loads the key to compute its last
//! four characters and drops it immediately. Callers that need the
//! fingerprint use [`stored_fingerprint`] (or [`key_fingerprint`]) instead of
//! `load`; `core/tests/no_key_load.rs` text-scans the repository for any
//! other `.load()` call (INV-2, docs/changes/0014).
//!
//! Key lifecycle (FR-1, PROPOSAL §6.1): [`store_key`] → [`has_key`] /
//! [`key_fingerprint`] → [`delete_key`]. The key is never cached: every
//! request reads it from the OS store and drops it after use (§6.2).

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
/// Shortest accepted key, prefix included. Real keys are about 108 bytes;
/// this only rejects obvious typos and truncated pastes.
pub const KEY_MIN_LEN: usize = 20;
/// Longest accepted key. Well under the Credential Manager blob limit
/// (`CRED_MAX_CREDENTIAL_BLOB_SIZE`, 2560 bytes).
pub const KEY_MAX_LEN: usize = 512;

/// Implemented by the Keychain (Apple), Credential Manager (Windows) and a
/// test mock. See PROPOSAL §6.2 for the exact storage attributes.
pub trait KeyVault: Send + Sync {
    /// Replace the stored key. Implementations reject keys that fail
    /// [`check_format`].
    fn store(&self, key: SecretString) -> Result<(), CoreError>;
    /// Read the key. `KeyNotFound` when nothing is stored.
    fn load(&self) -> Result<SecretString, CoreError>;
    /// Remove the key. Deleting when nothing is stored is not an error.
    fn delete(&self) -> Result<(), CoreError>;
    fn exists(&self) -> bool;

    /// The fingerprint of the stored key, `None` when nothing is stored.
    ///
    /// The default loads the key and drops it right away. Backends that can
    /// read a non-secret attribute instead (Keychain label: no biometric
    /// prompt) override it.
    fn key_fingerprint(&self) -> Result<Option<Fingerprint>, CoreError> {
        match self.load() {
            Ok(key) => Ok(Some(fingerprint(&key))),
            Err(CoreError::KeyNotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Non-secret vault settings chosen by the user (PROPOSAL §6.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VaultOptions {
    /// Apple only: require Touch ID / Face ID / passcode on every read
    /// (`SecAccessControl` `.userPresence`, still `WhenUnlockedThisDeviceOnly`).
    /// Ignored on Windows, where Credential Manager has no equivalent.
    pub require_user_presence: bool,
}

/// The OS vault for the current platform, or `Unsupported` elsewhere.
pub fn platform_vault() -> Result<Box<dyn KeyVault>, CoreError> {
    platform_vault_with(VaultOptions::default())
}

/// [`platform_vault`] with explicit [`VaultOptions`].
pub fn platform_vault_with(options: VaultOptions) -> Result<Box<dyn KeyVault>, CoreError> {
    #[cfg(target_vendor = "apple")]
    {
        Ok(Box::new(apple::KeychainVault::with_options(options)))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = options;
        Ok(Box::new(windows::CredManVault::new()))
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "windows")))]
    {
        let _ = options;
        Err(CoreError::Unsupported)
    }
}

/// Cheap format check before the online verification (FR-1): `sk-ant-`
/// prefix, [`KEY_MIN_LEN`]..=[`KEY_MAX_LEN`] bytes, and only the characters
/// Anthropic keys use (ASCII letters, digits, `-`, `_`). Rejects pasted
/// whitespace, quotes and newlines.
pub fn check_format(key: &SecretString) -> Result<(), CoreError> {
    let k = key.expose_secret();
    let charset_ok = k
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if k.starts_with(KEY_PREFIX) && (KEY_MIN_LEN..=KEY_MAX_LEN).contains(&k.len()) && charset_ok {
        Ok(())
    } else {
        Err(CoreError::KeyFormat)
    }
}

/// `…` plus the last four characters of the key: the only key-derived value
/// allowed out of the core (INV-2). Constructed only by [`fingerprint`] and,
/// inside this module, from a stored Keychain label.
#[derive(Clone, PartialEq, Eq)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Accept a previously computed fingerprint read back from non-secret
    /// storage, only if it has exactly the `…` + 4 ASCII characters shape.
    pub(crate) fn from_stored(s: &str) -> Option<Self> {
        let tail = s.strip_prefix('…')?;
        let ok = tail.len() == 4
            && tail
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        ok.then(|| Fingerprint(s.to_owned()))
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

/// FR-1: format check → online verification → store → fingerprint.
///
/// `verify` is the online check. In production it is always
/// `async |k| client.verify_key(k).await` (`AnthropicClient`, `GET
/// /v1/models`); it is a parameter so this flow can be tested without the
/// network. Nothing is stored unless both checks pass, and a failed attempt
/// never touches an already stored key.
pub async fn store_key<V>(
    vault: &dyn KeyVault,
    key: SecretString,
    verify: V,
) -> Result<Fingerprint, CoreError>
where
    V: AsyncFnOnce(&SecretString) -> Result<(), CoreError>,
{
    check_format(&key)?;
    verify(&key).await?;
    let fp = fingerprint(&key);
    vault.store(key)?;
    tracing::info!("vault: key stored");
    Ok(fp)
}

/// Whether a key is stored. Never reads the key itself on Apple.
pub fn has_key(vault: &dyn KeyVault) -> bool {
    vault.exists()
}

/// `Some("…a1B2")` when a key is stored. Errors (locked Keychain, cancelled
/// biometric prompt) read as `None`; the UI then shows "no key" or retries.
pub fn key_fingerprint(vault: &dyn KeyVault) -> Option<Fingerprint> {
    vault.key_fingerprint().ok().flatten()
}

/// The fingerprint of the stored key, `Ok(None)` when nothing is stored.
/// Unlike [`key_fingerprint`], errors (locked Keychain, missing entitlement,
/// cancelled prompt) are reported. This is what callers outside the vault
/// use instead of [`KeyVault::load`].
pub fn stored_fingerprint(vault: &dyn KeyVault) -> Result<Option<Fingerprint>, CoreError> {
    vault.key_fingerprint()
}

/// Remove the stored key. Idempotent.
pub fn delete_key(vault: &dyn KeyVault) -> Result<(), CoreError> {
    vault.delete()?;
    tracing::info!("vault: key deleted");
    Ok(())
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
        assert!(check_format(&SecretString::from("sk-other-0123456789abcd")).is_err());
        assert!(check_format(&SecretString::from("sk-ant-")).is_err());
        assert!(check_format(&SecretString::from("sk-ant-0123")).is_err());
        assert!(check_format(&SecretString::from("sk-ant-0123 456789abcdef")).is_err());
        assert!(check_format(&SecretString::from("sk-ant-0123456789abcdef\n")).is_err());
        assert!(check_format(&SecretString::from("\"sk-ant-0123456789abcdef\"")).is_err());
        assert!(check_format(&SecretString::from("sk-ant-0123456789abcdé")).is_err());
        let long = format!("{KEY_PREFIX}{}", "a".repeat(KEY_MAX_LEN));
        assert!(check_format(&SecretString::from(long)).is_err());
        let max = format!("{KEY_PREFIX}{}", "a".repeat(KEY_MAX_LEN - KEY_PREFIX.len()));
        assert!(check_format(&SecretString::from(max)).is_ok());
    }

    #[test]
    fn stored_fingerprint_shape_is_enforced() {
        assert!(Fingerprint::from_stored("…a1B2").is_some());
        assert!(Fingerprint::from_stored("a1B2").is_none());
        assert!(Fingerprint::from_stored("…a1B2c").is_none());
        assert!(Fingerprint::from_stored("…a1 2").is_none());
    }

    #[test]
    fn debug_is_redacted() {
        let key = SecretString::from("sk-ant-test-CANARY-7f3a");
        assert!(!format!("{key:?}").contains("CANARY"));
    }
}
