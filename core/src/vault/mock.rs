//! In-memory vault for unit tests and, via the `test-support` feature,
//! integration tests. Never compiled into a release build (lib.rs refuses
//! `test-support` there).
//!
//! Behaves like the OS backends: rejects badly formatted keys, deletes
//! idempotently, and can be told to fail so error paths are testable.

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use secrecy::{ExposeSecret, SecretString};

use super::{KeyVault, check_format};
use crate::CoreError;

#[derive(Default)]
pub struct MockVault {
    slot: Mutex<Option<SecretString>>,
    fail_with: Option<CoreError>,
    loads: AtomicUsize,
}

impl MockVault {
    /// A vault where every operation fails with `err` (for example
    /// `VaultUnavailable` for a locked Keychain) and `exists` is false.
    pub fn failing(err: CoreError) -> Self {
        Self {
            fail_with: Some(err),
            ..Self::default()
        }
    }

    /// How many times [`KeyVault::load`] was called. Lets tests check that
    /// callers read the key per request instead of caching it (§6.2).
    pub fn load_count(&self) -> usize {
        self.loads.load(Ordering::Relaxed)
    }

    fn check(&self) -> Result<(), CoreError> {
        self.fail_with.map_or(Ok(()), Err)
    }
}

impl fmt::Debug for MockVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockVault")
            .field("has_key", &self.exists())
            .field("fail_with", &self.fail_with)
            .finish()
    }
}

impl KeyVault for MockVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError> {
        self.check()?;
        check_format(&key)?;
        *self.slot.lock().map_err(|_| CoreError::VaultUnavailable)? = Some(key);
        Ok(())
    }

    fn load(&self) -> Result<SecretString, CoreError> {
        self.loads.fetch_add(1, Ordering::Relaxed);
        self.check()?;
        let slot = self.slot.lock().map_err(|_| CoreError::VaultUnavailable)?;
        slot.as_ref()
            .map(|k| SecretString::from(k.expose_secret()))
            .ok_or(CoreError::KeyNotFound)
    }

    fn delete(&self) -> Result<(), CoreError> {
        self.check()?;
        *self.slot.lock().map_err(|_| CoreError::VaultUnavailable)? = None;
        Ok(())
    }

    fn exists(&self) -> bool {
        self.fail_with.is_none() && self.slot.lock().map(|s| s.is_some()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let v = MockVault::default();
        assert!(!v.exists());
        v.store(SecretString::from("sk-ant-test-0123456789"))
            .unwrap();
        assert!(v.exists());
        assert_eq!(v.load().unwrap().expose_secret(), "sk-ant-test-0123456789");
        assert_eq!(v.load_count(), 1);
        v.delete().unwrap();
        v.delete().unwrap();
        assert!(matches!(v.load(), Err(CoreError::KeyNotFound)));
    }

    #[test]
    fn rejects_bad_format() {
        let v = MockVault::default();
        assert_eq!(
            v.store(SecretString::from("not-a-key")),
            Err(CoreError::KeyFormat)
        );
        assert!(!v.exists());
    }

    #[test]
    fn failing_vault_fails_everything() {
        let v = MockVault::failing(CoreError::VaultUnavailable);
        assert_eq!(
            v.store(SecretString::from("sk-ant-test-0123456789")),
            Err(CoreError::VaultUnavailable)
        );
        assert!(v.load().is_err());
        assert!(v.delete().is_err());
        assert!(!v.exists());
    }
}
