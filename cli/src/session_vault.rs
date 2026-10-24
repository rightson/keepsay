//! `--dev-mock-vault`: an in-memory `KeyVault` for one CLI run.
//!
//! Why it exists: an unsigned CLI on macOS cannot write the data-protection
//! keychain (ThisDeviceOnly, V6), and Linux has no vault backend at all. The
//! core's `MockVault` is only exposed with the core `test-support` feature,
//! which the CLI does not enable, so the CLI implements the public
//! `KeyVault` trait itself.
//!
//! Limits, so this cannot weaken INV-1 / INV-2:
//! - Compiled only into debug builds (`cfg(debug_assertions)`); a release
//!   CLI rejects the flag.
//! - The key lives in a `SecretString` in this process only and is zeroized
//!   on drop. Nothing is persisted; it is gone when the process exits.
//! - The key still only reaches the network through `AnthropicClient`.

use std::sync::Mutex;

use keepsay_core::{CoreError, KeyVault};
use secrecy::{ExposeSecret, SecretString};

pub const WARNING: &str = "\
WARNING: --dev-mock-vault: the key is kept in memory for this run only and
WARNING: never written to the OS secure storage. Development use only.";

#[derive(Default)]
pub struct SessionVault {
    slot: Mutex<Option<SecretString>>,
}

impl SessionVault {
    pub fn with_key(key: SecretString) -> Self {
        Self {
            slot: Mutex::new(Some(key)),
        }
    }
}

impl KeyVault for SessionVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError> {
        *self.slot.lock().map_err(|_| CoreError::VaultUnavailable)? = Some(key);
        Ok(())
    }

    fn load(&self) -> Result<SecretString, CoreError> {
        let slot = self.slot.lock().map_err(|_| CoreError::VaultUnavailable)?;
        slot.as_ref()
            .map(|k| SecretString::from(k.expose_secret()))
            .ok_or(CoreError::KeyNotFound)
    }

    fn delete(&self) -> Result<(), CoreError> {
        *self.slot.lock().map_err(|_| CoreError::VaultUnavailable)? = None;
        Ok(())
    }

    fn exists(&self) -> bool {
        self.slot.lock().is_ok_and(|s| s.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_without_printing() {
        let v = SessionVault::default();
        assert!(!v.exists());
        assert_eq!(v.load().err(), Some(CoreError::KeyNotFound));
        v.store(SecretString::from("sk-ant-test-0123456789"))
            .unwrap();
        assert!(v.exists());
        assert_eq!(v.load().unwrap().expose_secret().len(), 22);
        v.delete().unwrap();
        assert!(!v.exists());
    }
}
