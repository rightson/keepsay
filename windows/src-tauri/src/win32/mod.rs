//! Win32 FFI for the Windows app: the native key dialog (V5), SendInput,
//! the clipboard fallback, and the IME switch (docs/changes/0016).
//!
//! PENDING HUMAN APPROVAL (ROADMAP D-2). These need `unsafe` and extra
//! `windows` crate features, and the workspace denies `unsafe_code`
//! everywhere except `core/src/vault/windows.rs`. Until a human approves
//! relaxing that for exactly this directory, every function here is a stub
//! that returns [`Win32Error::NotYetAvailable`] and contains no `unsafe`.
//!
//! Planned implementation (recorded in docs/changes/0016):
//! - `prompt_api_key`: a modal dialog built in memory (`DialogBoxIndirectParamW`)
//!   with an `ES_PASSWORD` edit control, so the key never reaches the WebView
//!   (INV-2). The edit buffer is read with `GetWindowTextW` into a zeroized
//!   buffer, converted straight into `SecretString`, and the control text is
//!   overwritten before the dialog is destroyed. `EM_SETPASSWORDCHAR` keeps
//!   the text masked; `WM_COPY` / `WM_CUT` are blocked by the password style.
//! - `send_unicode`: `SendInput` with `KEYEVENTF_UNICODE` (VK_PACKET),
//!   UTF-16 code units, key down + key up per unit. Bypasses the IME.
//! - `paste_concealed`: back up every clipboard format, write CF_UNICODETEXT
//!   plus `ExcludeClipboardContentFromMonitorProcessing`,
//!   `CanIncludeInClipboardHistory` = 0 and `CanUploadToCloudClipboard` = 0,
//!   send Ctrl+V, wait, restore the original formats.
//! - `ascii_ime_guard`: if the foreground IME is in native (e.g. 注音)
//!   conversion mode, send `WM_IME_CONTROL` / `IMC_SETCONVERSIONMODE` to its
//!   default IME window to switch to alphanumeric before Ctrl+V, and restore
//!   the previous mode afterwards. Clipboard path only.

use secrecy::SecretString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Win32Error {
    /// The Win32 implementation is waiting for approval (D-2).
    NotYetAvailable,
    /// A Win32 call failed. No detail is kept.
    Failed,
}

/// Show the native password dialog. `Ok(None)` = the user cancelled.
pub fn prompt_api_key() -> Result<Option<SecretString>, Win32Error> {
    Err(Win32Error::NotYetAvailable)
}

/// Type `text` into the focused window with `SendInput` + `KEYEVENTF_UNICODE`.
pub fn send_unicode(text: &str) -> Result<(), Win32Error> {
    let _ = text.len();
    Err(Win32Error::NotYetAvailable)
}

/// Paste `text` through the clipboard with the exclusion formats set, then
/// restore the previous clipboard contents.
pub fn paste_concealed(text: &str) -> Result<(), Win32Error> {
    let _ = text.len();
    Err(Win32Error::NotYetAvailable)
}

/// Restores the IME conversion mode when dropped.
#[derive(Debug)]
pub struct ImeGuard {
    _private: (),
}

/// Switch the foreground IME to alphanumeric mode if it is in native mode.
/// `Ok(None)` = nothing to do.
pub fn ascii_ime_guard() -> Result<Option<ImeGuard>, Win32Error> {
    Err(Win32Error::NotYetAvailable)
}
