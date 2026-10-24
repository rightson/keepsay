//! In-memory vault for unit tests only.

use std::sync::Mutex;

use secrecy::{ExposeSecret, SecretString};

use super::KeyVault;
use crate::CoreError;

#[derive(Default)]
pub struct MockVault {
    slot: Mutex<Option<SecretString>>,
}

impl KeyVault for MockVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError> {
        *self.slot.lock().map_err(|_| CoreError::Vault)? = Some(key);
        Ok(())
    }

    fn load(&self) -> Result<SecretString, CoreError> {
        let slot = self.slot.lock().map_err(|_| CoreError::Vault)?;
        slot.as_ref()
            .map(|k| SecretString::from(k.expose_secret()))
            .ok_or(CoreError::NoKey)
    }

    fn delete(&self) -> Result<(), CoreError> {
        *self.slot.lock().map_err(|_| CoreError::Vault)? = None;
        Ok(())
    }

    fn exists(&self) -> bool {
        self.slot.lock().map(|s| s.is_some()).unwrap_or(false)
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
        v.delete().unwrap();
        assert!(matches!(v.load(), Err(CoreError::NoKey)));
    }
}
