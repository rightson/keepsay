//! Text insertion into the focused app (FR-5, T6, PROPOSAL §7.3).
//!
//! 1. `SendInput` with `KEYEVENTF_UNICODE`, unit by unit. It does not touch
//!    the clipboard and bypasses the IME.
//! 2. Long text, or if (1) fails: the clipboard, marked so clipboard history,
//!    cloud clipboard and monitors skip it, then restored. Only on this path
//!    the IME is switched to alphanumeric first so Ctrl+V is not eaten by a
//!    CJK IME in native mode, and switched back afterwards (US-M3-09).
//!
//! No MSIX AppContainer: UIPI would block `SendInput` into other apps.

/// Above this many UTF-16 units, typing takes long enough (and is visible
/// enough) that the clipboard path is used directly.
pub const SEND_INPUT_MAX_UNITS: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    SendInput,
    Clipboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertError {
    /// The Win32 side is not implemented yet (pending approval, D-2).
    NotYetAvailable,
    Failed,
}

/// The Win32 operations, behind a trait so the fallback order is testable.
pub trait Backend {
    fn send_unicode(&self, text: &str) -> Result<(), InsertError>;
    /// Switch a native-mode IME to alphanumeric. Best effort.
    fn ime_ascii(&self);
    /// Undo [`Backend::ime_ascii`], if it changed anything.
    fn ime_restore(&self);
    fn paste_concealed(&self, text: &str) -> Result<(), InsertError>;
}

pub fn choose(text: &str) -> Method {
    if text.encode_utf16().count() > SEND_INPUT_MAX_UNITS {
        Method::Clipboard
    } else {
        Method::SendInput
    }
}

/// Insert `text`, returning the method that worked.
pub fn insert(text: &str, backend: &dyn Backend) -> Result<Method, InsertError> {
    if text.is_empty() {
        return Ok(Method::SendInput);
    }
    let send_err = if choose(text) == Method::SendInput {
        match backend.send_unicode(text) {
            Ok(()) => return Ok(Method::SendInput),
            Err(e) => Some(e),
        }
    } else {
        None
    };
    backend.ime_ascii();
    let pasted = backend.paste_concealed(text);
    backend.ime_restore();
    match pasted {
        Ok(()) => Ok(Method::Clipboard),
        // Report "not yet available" when the first path was a stub too.
        Err(e) => Err(match send_err {
            Some(InsertError::NotYetAvailable) => InsertError::NotYetAvailable,
            _ => e,
        }),
    }
}

/// The real backend.
#[cfg(windows)]
#[derive(Default)]
pub struct Win32Backend {
    ime: std::cell::RefCell<Option<crate::win32::ImeGuard>>,
}

#[cfg(windows)]
impl Backend for Win32Backend {
    fn send_unicode(&self, text: &str) -> Result<(), InsertError> {
        crate::win32::send_unicode(text).map_err(map_err)
    }

    fn ime_ascii(&self) {
        if let Ok(guard) = crate::win32::ascii_ime_guard() {
            *self.ime.borrow_mut() = guard;
        }
    }

    fn ime_restore(&self) {
        // Dropping the guard restores the previous conversion mode.
        self.ime.borrow_mut().take();
    }

    fn paste_concealed(&self, text: &str) -> Result<(), InsertError> {
        crate::win32::paste_concealed(text).map_err(map_err)
    }
}

#[cfg(windows)]
fn map_err(e: crate::win32::Win32Error) -> InsertError {
    match e {
        crate::win32::Win32Error::NotYetAvailable => InsertError::NotYetAvailable,
        crate::win32::Win32Error::Failed => InsertError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Mock {
        send: Option<InsertError>,
        paste: Option<InsertError>,
        log: RefCell<Vec<&'static str>>,
    }

    impl Backend for Mock {
        fn send_unicode(&self, _: &str) -> Result<(), InsertError> {
            self.log.borrow_mut().push("send");
            self.send.map_or(Ok(()), Err)
        }
        fn ime_ascii(&self) {
            self.log.borrow_mut().push("ime_ascii");
        }
        fn ime_restore(&self) {
            self.log.borrow_mut().push("ime_restore");
        }
        fn paste_concealed(&self, _: &str) -> Result<(), InsertError> {
            self.log.borrow_mut().push("paste");
            self.paste.map_or(Ok(()), Err)
        }
    }

    #[test]
    fn short_text_uses_send_input_only() {
        let m = Mock::default();
        assert_eq!(insert("你好 hello", &m), Ok(Method::SendInput));
        assert_eq!(*m.log.borrow(), vec!["send"]);
    }

    #[test]
    fn send_failure_falls_back_to_clipboard_with_ime_switch() {
        let m = Mock {
            send: Some(InsertError::Failed),
            ..Default::default()
        };
        assert_eq!(insert("abc", &m), Ok(Method::Clipboard));
        assert_eq!(
            *m.log.borrow(),
            vec!["send", "ime_ascii", "paste", "ime_restore"]
        );
    }

    #[test]
    fn long_text_goes_straight_to_clipboard() {
        let m = Mock::default();
        let long = "字".repeat(SEND_INPUT_MAX_UNITS + 1);
        assert_eq!(choose(&long), Method::Clipboard);
        assert_eq!(insert(&long, &m), Ok(Method::Clipboard));
        assert_eq!(*m.log.borrow(), vec!["ime_ascii", "paste", "ime_restore"]);
    }

    #[test]
    fn failures() {
        let m = Mock {
            send: Some(InsertError::Failed),
            paste: Some(InsertError::Failed),
            ..Default::default()
        };
        assert_eq!(insert("abc", &m), Err(InsertError::Failed));
        // IME is restored even when the paste fails.
        assert_eq!(m.log.borrow().last(), Some(&"ime_restore"));
        assert_eq!(insert("", &m), Ok(Method::SendInput));

        let stub = Mock {
            send: Some(InsertError::NotYetAvailable),
            paste: Some(InsertError::NotYetAvailable),
            ..Default::default()
        };
        assert_eq!(insert("abc", &stub), Err(InsertError::NotYetAvailable));
    }
}
