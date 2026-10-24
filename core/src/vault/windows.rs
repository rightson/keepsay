//! Credential Manager-backed vault (INV-1; docs/changes/0008).
//!
//! Owner: workstream C (docs/changes/0005).
//!
//! CRED_TYPE_GENERIC, CRED_PERSIST_LOCAL_MACHINE (never ENTERPRISE, which
//! roams with the profile), target "dev.rightson.keepsay/anthropic". The key
//! is stored as its UTF-8 bytes. Every buffer CredReadW returns is wiped
//! before CredFree, and the copy handed to CredWriteW is wiped after.
//!
//! This is the only file in the repository allowed to use `unsafe`
//! (docs/changes/0005). Keep every `unsafe` block minimal and give each one a
//! `// SAFETY:` comment. CI fails if the attribute below appears anywhere else.
#![allow(unsafe_code)]

use std::fmt;
use std::ptr;

use secrecy::{ExposeSecret, SecretString};
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};
use windows::core::{HRESULT, PCWSTR, PWSTR};
use zeroize::{Zeroize, Zeroizing};

use super::{KeyVault, check_format};
use crate::CoreError;

pub const TARGET: &str = "dev.rightson.keepsay/anthropic";

pub struct CredManVault {
    /// NUL-terminated UTF-16 target name.
    target: Vec<u16>,
}

impl Default for CredManVault {
    fn default() -> Self {
        Self::with_target(TARGET)
    }
}

impl fmt::Debug for CredManVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredManVault").finish_non_exhaustive()
    }
}

impl CredManVault {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_target(target: &str) -> Self {
        Self {
            target: target.encode_utf16().chain(Some(0)).collect(),
        }
    }

    /// A vault on a throwaway target name, so integration tests never touch
    /// the user's real key.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests(target: &str) -> Self {
        Self::with_target(target)
    }

    /// `(Type, Persist)` of the stored credential, for the Windows
    /// integration test (docs/changes/0008). The blob is wiped unread.
    #[cfg(any(test, feature = "test-support"))]
    pub fn inspect(&self) -> Result<(u32, u32), CoreError> {
        let cred = ReadCredential::read(&self.target)?;
        Ok(cred.type_and_persist())
    }
}

fn map_err(e: windows::core::Error) -> CoreError {
    if e.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
        CoreError::KeyNotFound
    } else {
        // The HRESULT is a fixed number, never content.
        tracing::warn!(
            hresult = e.code().0,
            "vault: credential manager call failed"
        );
        CoreError::VaultUnavailable
    }
}

/// Owns the `CREDENTIALW` allocated by `CredReadW`. Dropping it wipes the
/// blob, then frees the allocation with `CredFree`.
struct ReadCredential(*mut CREDENTIALW);

impl ReadCredential {
    fn read(target: &[u16]) -> Result<Self, CoreError> {
        let mut p: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: `target` is a NUL-terminated UTF-16 string that outlives
        // the call, and `p` is a valid out-pointer. On success the OS stores
        // a pointer to an allocation we own until `CredFree`.
        unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut p) }
            .map_err(map_err)?;
        if p.is_null() {
            return Err(CoreError::VaultUnavailable);
        }
        Ok(Self(p))
    }

    fn blob(&mut self) -> &mut [u8] {
        // SAFETY: `self.0` is the non-null pointer CredReadW returned and is
        // valid until `CredFree` in `Drop`. Reading two plain fields through
        // it creates no reference into the struct.
        let (data, len) = unsafe { ((*self.0).CredentialBlob, (*self.0).CredentialBlobSize) };
        if data.is_null() || len == 0 {
            return &mut [];
        }
        // SAFETY: `CredentialBlob` points to `CredentialBlobSize` bytes inside
        // the same CredReadW allocation, which nothing else aliases. The
        // returned slice borrows `self`, so it cannot outlive `CredFree`.
        unsafe { std::slice::from_raw_parts_mut(data, len as usize) }
    }

    #[cfg(any(test, feature = "test-support"))]
    fn type_and_persist(&self) -> (u32, u32) {
        // SAFETY: as in `blob`: valid non-null pointer, plain field reads.
        unsafe { ((*self.0).Type.0, (*self.0).Persist.0) }
    }
}

impl Drop for ReadCredential {
    fn drop(&mut self) {
        self.blob().zeroize();
        // SAFETY: `self.0` came from a successful CredReadW and is freed
        // exactly once, here.
        unsafe { CredFree(self.0 as *const core::ffi::c_void) };
    }
}

impl KeyVault for CredManVault {
    fn store(&self, key: SecretString) -> Result<(), CoreError> {
        check_format(&key)?;
        // CREDENTIALW wants `*mut` pointers, so both go through owned
        // buffers; the key copy is wiped on drop.
        let mut blob = Zeroizing::new(key.expose_secret().as_bytes().to_vec());
        let mut target = self.target.clone();
        let size = u32::try_from(blob.len()).map_err(|_| CoreError::KeyFormat)?;
        let cred = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            CredentialBlobSize: size,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: `cred` and the buffers it points to (`target`, `blob`) are
        // alive and not moved for the whole call; CredWriteW only reads them
        // and copies what it keeps. It replaces any existing credential with
        // the same target and type, including its persist value.
        unsafe { CredWriteW(&cred, 0) }.map_err(map_err)?;
        tracing::debug!(len = blob.len(), "vault: credential written");
        Ok(())
    }

    fn load(&self) -> Result<SecretString, CoreError> {
        let mut cred = ReadCredential::read(&self.target)?;
        let s = std::str::from_utf8(cred.blob()).map_err(|_| CoreError::VaultUnavailable)?;
        // Copies into the SecretString; `cred` wipes the OS buffer on drop.
        Ok(SecretString::from(s))
    }

    fn delete(&self) -> Result<(), CoreError> {
        // SAFETY: `self.target` is a NUL-terminated UTF-16 string that
        // outlives the call.
        let r = unsafe { CredDeleteW(PCWSTR(self.target.as_ptr()), CRED_TYPE_GENERIC, None) };
        match r.map_err(map_err) {
            Ok(()) | Err(CoreError::KeyNotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn exists(&self) -> bool {
        // CredReadW has no attribute-only mode; the blob is wiped unread.
        ReadCredential::read(&self.target).is_ok()
    }
}
