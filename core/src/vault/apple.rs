//! Keychain-backed vault.
//!
//! Owner: workstream C (docs/changes/0005).
//!
//! kSecClassGenericPassword, service "dev.rightson.keepsay.apikey",
//! kSecAttrAccessibleWhenUnlockedThisDeviceOnly, synchronizable = false, no
//! shared access group (INV-1, INV-10). 【待驗證 V6】

use secrecy::SecretString;

use super::KeyVault;
use crate::CoreError;

pub const SERVICE: &str = "dev.rightson.keepsay.apikey";
pub const ACCOUNT: &str = "anthropic";

#[derive(Debug, Default)]
pub struct KeychainVault {
    _private: (),
}

impl KeychainVault {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyVault for KeychainVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError> {
        let _ = key;
        todo!("M1: vault — Keychain store")
    }

    fn load(&self) -> Result<SecretString, CoreError> {
        todo!("M1: vault — Keychain load")
    }

    fn delete(&self) -> Result<(), CoreError> {
        todo!("M1: vault — Keychain delete")
    }

    fn exists(&self) -> bool {
        todo!("M1: vault — Keychain exists")
    }
}
