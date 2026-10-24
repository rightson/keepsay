//! INV-1 on the real OS stores (US-M1-04, US-M1-05). Runs on the macOS and
//! Windows CI jobs (`--features test-endpoint,test-support`). Uses a throwaway
//! service / target name so a developer's real key is never touched.
//!
//! Only lengths, fingerprints and storage attributes are asserted; the key
//! itself is never compared or printed.

#![cfg(all(
    feature = "test-support",
    any(target_vendor = "apple", target_os = "windows")
))]

use keepsay_core::CoreError;
use keepsay_core::vault::{KeyVault, fingerprint};
use secrecy::{ExposeSecret, SecretString};

/// Per-process name so parallel CI jobs on one machine do not collide.
fn test_name() -> String {
    format!("dev.rightson.keepsay.test.{}", std::process::id())
}

fn fake_key(tail: &str) -> SecretString {
    SecretString::from(format!("sk-ant-test-vault-0123456789-{tail}"))
}

#[cfg(target_vendor = "apple")]
mod apple {
    use super::*;
    use keepsay_core::vault::VaultOptions;
    use keepsay_core::vault::apple::KeychainVault;

    /// The data protection keychain needs the `keychain-access-groups`
    /// entitlement. An unsigned `cargo test` binary does not have it, so the
    /// only acceptable outcomes are full INV-1 attributes or a clean
    /// `VaultNotEntitled` with nothing written anywhere (docs/changes/0008).
    #[test]
    fn keychain_item_is_this_device_only_and_not_synced() {
        let v = KeychainVault::for_tests(&test_name(), VaultOptions::default());
        let key = fake_key("a1B2");
        match v.store(SecretString::from(key.expose_secret())) {
            Err(CoreError::VaultNotEntitled) => {
                // No silent fallback to the legacy file keychain, and the
                // pending item of the replace protocol is not left behind.
                assert!(!v.exists());
                assert!(!v.has_pending());
                assert!(!v.exists_in_synced_store());
                eprintln!("skipped: test binary lacks keychain-access-groups entitlement");
                return;
            }
            Err(e) => panic!("unexpected store error: {e}"),
            Ok(()) => {}
        }

        let result = std::panic::catch_unwind(|| {
            assert!(v.exists());
            assert!(!v.exists_in_synced_store());
            let (protection, label) = v.inspect().expect("attributes");
            assert_eq!(
                protection.as_deref(),
                Some("aku"),
                "WhenUnlockedThisDeviceOnly"
            );
            assert_eq!(label.as_deref(), Some(fingerprint(&key).as_str()));
            assert_eq!(v.key_fingerprint().expect("fp"), Some(fingerprint(&key)));
            assert_eq!(
                v.load().expect("load").expose_secret().len(),
                key.expose_secret().len()
            );

            assert!(!v.has_pending());

            // Replacing writes a fresh item (add pending, delete old, rename),
            // so the attributes are those of a new add, and nothing stays
            // pending (docs/changes/0014).
            let key2 = fake_key("c3D4");
            v.store(SecretString::from(key2.expose_secret()))
                .expect("replace");
            assert_eq!(v.key_fingerprint().expect("fp"), Some(fingerprint(&key2)));
            assert_eq!(v.inspect().expect("attributes").0.as_deref(), Some("aku"));
            assert!(!v.has_pending());
            assert!(!v.exists_in_synced_store());
        });
        v.delete().expect("cleanup");
        assert!(!v.exists());
        v.delete().expect("delete is idempotent");
        if let Err(p) = result {
            std::panic::resume_unwind(p);
        }
    }

    #[test]
    fn missing_item_reads_as_not_found() {
        let v =
            KeychainVault::for_tests(&format!("{}.missing", test_name()), VaultOptions::default());
        assert!(!v.exists());
        assert!(matches!(
            v.load(),
            Err(CoreError::KeyNotFound) | Err(CoreError::VaultNotEntitled)
        ));
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use keepsay_core::vault::windows::CredManVault;

    const CRED_TYPE_GENERIC: u32 = 1;
    const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;

    #[test]
    fn credential_is_generic_and_local_machine() {
        let v = CredManVault::for_tests(&test_name());
        let key = fake_key("a1B2");
        v.store(SecretString::from(key.expose_secret()))
            .expect("store");

        let result = std::panic::catch_unwind(|| {
            assert!(v.exists());
            assert_eq!(
                v.inspect().expect("inspect"),
                (CRED_TYPE_GENERIC, CRED_PERSIST_LOCAL_MACHINE)
            );
            assert_eq!(v.key_fingerprint().expect("fp"), Some(fingerprint(&key)));
            assert_eq!(
                v.load().expect("load").expose_secret().len(),
                key.expose_secret().len()
            );

            let key2 = fake_key("c3D4");
            v.store(SecretString::from(key2.expose_secret()))
                .expect("replace");
            assert_eq!(v.key_fingerprint().expect("fp"), Some(fingerprint(&key2)));
        });
        v.delete().expect("cleanup");
        assert!(!v.exists());
        v.delete().expect("delete is idempotent");
        if let Err(p) = result {
            std::panic::resume_unwind(p);
        }
    }

    #[test]
    fn missing_credential_reads_as_not_found() {
        let v = CredManVault::for_tests(&format!("{}.missing", test_name()));
        assert!(!v.exists());
        assert!(matches!(v.load(), Err(CoreError::KeyNotFound)));
        assert_eq!(v.key_fingerprint(), Ok(None));
    }

    #[test]
    fn rejects_bad_format_without_writing() {
        let v = CredManVault::for_tests(&format!("{}.bad", test_name()));
        assert_eq!(
            v.store(SecretString::from("sk-ant-")),
            Err(CoreError::KeyFormat)
        );
        assert!(!v.exists());
    }
}
