//! Credential Manager-backed vault.
//!
//! Owner: workstream C (docs/changes/0005).
//!
//! CRED_TYPE_GENERIC, CRED_PERSIST_LOCAL_MACHINE (never ENTERPRISE), target
//! "dev.rightson.keepsay/anthropic". Zero the CredReadW buffer before CredFree
//! (INV-1).
//!
//! This is the only file in the repository allowed to use `unsafe`
//! (docs/changes/0005). Keep every `unsafe` block minimal and give each one a
//! `// SAFETY:` comment. CI fails if the attribute below appears anywhere else.
#![allow(unsafe_code)]

use secrecy::SecretString;

use super::KeyVault;
use crate::CoreError;

pub const TARGET: &str = "dev.rightson.keepsay/anthropic";

#[derive(Debug, Default)]
pub struct CredManVault {
    _private: (),
}

impl CredManVault {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyVault for CredManVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError> {
        let _ = key;
        todo!("M1: vault — CredWriteW")
    }

    fn load(&self) -> Result<SecretString, CoreError> {
        todo!("M1: vault — CredReadW")
    }

    fn delete(&self) -> Result<(), CoreError> {
        todo!("M1: vault — CredDeleteW")
    }

    fn exists(&self) -> bool {
        todo!("M1: vault — exists")
    }
}
