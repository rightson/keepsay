//! Keychain-backed vault (INV-1, V6; docs/changes/0008).
//!
//! Owner: workstream C (docs/changes/0005).
//!
//! Every item is written with:
//! - `kSecClassGenericPassword`, service `dev.rightson.keepsay.apikey`,
//!   account `anthropic`;
//! - `kSecAttrAccessControl` built by `SecAccessControl::create_with_protection`
//!   with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` (plus
//!   `.userPresence` when the user opts in);
//! - `kSecAttrSynchronizable = false` on every query (set, get, delete,
//!   search), so the iCloud-synced store is never touched;
//! - `kSecUseDataProtectionKeychain = true` (macOS needs the `OSX_10_15`
//!   feature; iOS always uses it). The legacy file-based macOS keychain
//!   ignores protection classes and synchronizable, so it is never used;
//! - no `kSecAttrAccessGroup` (INV-10: the iOS keyboard extension cannot see
//!   the item);
//! - `kSecAttrLabel` = the fingerprint (`…a1B2`), so the UI can show it
//!   without reading the secret (and without a biometric prompt).
//!
//! NEVER call `PasswordOptions::set_access_control_options`: it builds the
//! access control with protection `None`, which security-framework turns
//! into plain `kSecAttrAccessibleWhenUnlocked` (not ThisDeviceOnly, so the
//! item can migrate to another device through a backup), breaking INV-1.
//!
//! The data protection keychain needs the `keychain-access-groups`
//! entitlement on macOS. Unsigned binaries (the dev CLI, `cargo test`) get
//! `errSecMissingEntitlement`; that maps to `CoreError::Unsupported` and is
//! never retried against a weaker store.

use secrecy::{ExposeSecret, SecretString};
use security_framework::access_control::{ProtectionMode, SecAccessControl};
use security_framework::base::Error as SecError;
use security_framework::item::{ItemClass, ItemSearchOptions};
use security_framework::passwords::{
    AccessControlOptions, PasswordOptions, delete_generic_password_options, generic_password,
    set_generic_password_options,
};
use zeroize::Zeroizing;

use super::{Fingerprint, KeyVault, VaultOptions, check_format, fingerprint};
use crate::CoreError;

pub const SERVICE: &str = "dev.rightson.keepsay.apikey";
pub const ACCOUNT: &str = "anthropic";

// OSStatus values (Security/SecBase.h). Local constants so core does not
// depend on security-framework-sys directly.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_MISSING_ENTITLEMENT: i32 = -34018;
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;

#[derive(Debug)]
pub struct KeychainVault {
    service: String,
    user_presence: bool,
}

impl Default for KeychainVault {
    fn default() -> Self {
        Self::with_options(VaultOptions::default())
    }
}

impl KeychainVault {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: VaultOptions) -> Self {
        Self {
            service: SERVICE.to_owned(),
            user_presence: options.require_user_presence,
        }
    }

    /// A vault on a throwaway service name, so integration tests never touch
    /// the user's real key.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests(service: &str, options: VaultOptions) -> Self {
        Self {
            service: service.to_owned(),
            user_presence: options.require_user_presence,
        }
    }

    /// Base query shared by every operation: class, service, account,
    /// non-synchronizable store, data protection keychain.
    fn query(&self) -> PasswordOptions {
        let mut o = PasswordOptions::new_generic_password(&self.service, ACCOUNT);
        o.set_access_synchronized(Some(false));
        o.use_protected_keychain();
        o
    }

    /// Attribute-only search (never returns the secret).
    fn search(&self) -> ItemSearchOptions {
        let mut s = ItemSearchOptions::new();
        s.class(ItemClass::generic_password())
            .service(&self.service)
            .account(ACCOUNT)
            .cloud_sync(Some(false));
        #[cfg(target_os = "macos")]
        s.ignore_legacy_keychains();
        s
    }

    fn access_control(&self) -> Result<SecAccessControl, CoreError> {
        let flags = if self.user_presence {
            AccessControlOptions::USER_PRESENCE.bits()
        } else {
            0
        };
        SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            flags,
        )
        .map_err(map_err)
    }

    /// Non-secret attributes of the stored item, for the Apple integration
    /// test (docs/changes/0008). Returns `(protection class, label)`;
    /// `WhenUnlockedThisDeviceOnly` reads as `"aku"`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn inspect(&self) -> Result<(Option<String>, Option<String>), CoreError> {
        let attrs = self.attributes()?;
        Ok((attrs.get("pdmn").cloned(), attrs.get("labl").cloned()))
    }

    /// Whether an item with the same service exists in the iCloud-synced
    /// store (it never should). Test support only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn exists_in_synced_store(&self) -> bool {
        let mut s = self.search();
        s.cloud_sync(Some(true));
        s.search().is_ok()
    }

    fn attributes(&self) -> Result<std::collections::HashMap<String, String>, CoreError> {
        let mut s = self.search();
        s.load_attributes(true);
        let results = s.search().map_err(map_err)?;
        results
            .first()
            .and_then(|r| r.simplify_dict())
            .ok_or(CoreError::VaultUnavailable)
    }
}

fn map_err(e: SecError) -> CoreError {
    let status = e.code();
    match status {
        ERR_SEC_ITEM_NOT_FOUND => CoreError::KeyNotFound,
        ERR_SEC_MISSING_ENTITLEMENT => {
            // The OSStatus is a fixed number, never content.
            tracing::warn!(
                status,
                "vault: binary lacks keychain-access-groups entitlement; data protection keychain unavailable"
            );
            CoreError::Unsupported
        }
        _ => {
            tracing::warn!(status, "vault: keychain operation failed");
            CoreError::VaultUnavailable
        }
    }
}

impl KeyVault for KeychainVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError> {
        check_format(&key)?;
        let fp = fingerprint(&key);
        let ac = self.access_control()?;
        // Delete first, then add: `set_generic_password_options` would
        // otherwise update an existing item in place and keep whatever access
        // control it had. A fresh add guarantees the attributes above. If the
        // add then fails, the old key is gone too; the caller reports the
        // error and the user enters the key again.
        self.delete()?;
        let mut o = self.query();
        o.set_access_control(ac);
        o.set_label(fp.as_str());
        set_generic_password_options(key.expose_secret().as_bytes(), o).map_err(map_err)?;
        tracing::debug!(
            len = key.expose_secret().len(),
            "vault: keychain item written"
        );
        Ok(())
    }

    fn load(&self) -> Result<SecretString, CoreError> {
        let bytes = Zeroizing::new(generic_password(self.query()).map_err(map_err)?);
        let s = std::str::from_utf8(&bytes).map_err(|_| CoreError::VaultUnavailable)?;
        // Copies into the SecretString; `bytes` is wiped when dropped.
        Ok(SecretString::from(s))
    }

    fn delete(&self) -> Result<(), CoreError> {
        match delete_generic_password_options(self.query()).map_err(map_err) {
            Ok(()) | Err(CoreError::KeyNotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn exists(&self) -> bool {
        // No `kSecReturn*`: the secret is not read, so no biometric prompt.
        match self.search().search() {
            Ok(_) => true,
            // The item is there but needs authentication or an unlock.
            Err(e) if e.code() == ERR_SEC_INTERACTION_NOT_ALLOWED => true,
            Err(_) => false,
        }
    }

    fn key_fingerprint(&self) -> Result<Option<Fingerprint>, CoreError> {
        match self.attributes() {
            Ok(attrs) => match attrs.get("labl").and_then(|l| Fingerprint::from_stored(l)) {
                Some(fp) => Ok(Some(fp)),
                // No usable label (item written by something else): fall back
                // to reading the key, which may prompt with userPresence.
                None => self.load().map(|k| Some(fingerprint(&k))),
            },
            Err(CoreError::KeyNotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
