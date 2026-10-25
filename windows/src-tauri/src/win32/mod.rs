//! Win32 FFI for the Windows app: the native key dialog (V5), `SendInput`
//! and the clipboard fallback (docs/changes/0016, approved in 0018).
//!
//! This directory is the only place in `windows/` allowed to use `unsafe`
//! (0018). The opt-in below covers this module and its children; CI's
//! `unsafe-scope` job fails on `unsafe_code` anywhere else except
//! `core/src/vault/windows.rs`. Every `unsafe` block carries a `// SAFETY:`
//! comment.
//!
//! - [`prompt_api_key`]: a modal window built with `CreateWindowExW`, one
//!   `ES_PASSWORD` edit, excluded from screen capture
//!   (`WDA_EXCLUDEFROMCAPTURE`). The text is read into a zeroized UTF-16
//!   buffer and goes straight into a `SecretString`; the WebView never sees
//!   it (INV-2).
//! - [`send_unicode`]: `SendInput` with `KEYEVENTF_UNICODE` (VK_PACKET),
//!   which bypasses the IME, so CJK IMEs need no special handling here.
//! - [`paste_concealed`]: back up the clipboard, write the text with
//!   `ExcludeClipboardContentFromMonitorProcessing`,
//!   `CanIncludeInClipboardHistory` = 0 and `CanUploadToCloudClipboard` = 0,
//!   send Ctrl+V, restore the backup (T6).
//! - [`ascii_ime_guard`]: switching a native-mode IME (注音) to
//!   alphanumeric before Ctrl+V needs `ImmGetDefaultIMEWnd`
//!   (`Win32_UI_Input_Ime`), which is not in the approved feature list
//!   (0018). Until it is, this is a no-op (US-M3-09, docs/changes/0016).
//!
//! Nothing here logs text, lengths of the key, or clipboard contents.

#![allow(unsafe_code)]

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod dialog;
#[cfg(windows)]
mod input;
mod plan;

use secrecy::SecretString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Win32Error {
    /// Not available on this platform (the non-Windows stub build).
    #[cfg_attr(windows, allow(dead_code))]
    NotYetAvailable,
    /// Another key dialog is already open.
    Busy,
    /// A Win32 call failed. No detail is kept.
    Failed,
}

/// Show the native password dialog and block until it closes.
/// `Ok(None)` = the user cancelled.
pub fn prompt_api_key() -> Result<Option<SecretString>, Win32Error> {
    #[cfg(windows)]
    {
        dialog::prompt()
    }
    #[cfg(not(windows))]
    {
        Err(Win32Error::NotYetAvailable)
    }
}

/// Type `text` into the focused window with `SendInput` + `KEYEVENTF_UNICODE`.
pub fn send_unicode(text: &str) -> Result<(), Win32Error> {
    #[cfg(windows)]
    {
        input::send_text(text)
    }
    #[cfg(not(windows))]
    {
        let _ = text.len();
        Err(Win32Error::NotYetAvailable)
    }
}

/// Paste `text` through the clipboard with the exclusion formats set, then
/// restore the previous clipboard contents.
pub fn paste_concealed(text: &str) -> Result<(), Win32Error> {
    #[cfg(windows)]
    {
        clipboard::paste(text)
    }
    #[cfg(not(windows))]
    {
        let _ = text.len();
        Err(Win32Error::NotYetAvailable)
    }
}

/// Restores the IME conversion mode when dropped.
#[derive(Debug)]
pub struct ImeGuard {
    _private: (),
}

/// Switch the foreground IME to alphanumeric mode if it is in native mode.
/// `Ok(None)` = nothing to do. Currently always `Ok(None)`: see the module
/// docs (needs `Win32_UI_Input_Ime`, not yet approved).
pub fn ascii_ime_guard() -> Result<Option<ImeGuard>, Win32Error> {
    Ok(None)
}
