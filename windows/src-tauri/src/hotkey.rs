//! Hotkey strings (FR-2, V8 option (c), docs/changes/0004 and 0016).
//!
//! Hotkeys are registered with RegisterHotKey through
//! tauri-plugin-global-shortcut (`MOD_NOREPEAT` is set by `global-hotkey`).
//! There is no keyboard hook and no Raw Input: the app only ever sees the two
//! combinations it registered, plus Esc while recording.
//!
//! This module only parses and validates the user's choice. Rules
//! (docs/changes/0016):
//! - Exactly one of Alt or Ctrl. Ctrl+Alt is refused because AltGr is
//!   Ctrl+Alt on Windows, so a Ctrl+Alt hotkey would swallow AltGr
//!   characters (for example Polish Ż = AltGr+Shift+Z).
//! - Win is refused: Win+letter is mostly reserved by the system.
//! - Shift is optional.
//! - The key is A–Z, 0–9, F1–F24 or Space. Esc is reserved for cancel.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Letter(u8),
    Digit(u8),
    F(u8),
    Space,
}

impl Key {
    /// The W3C `KeyboardEvent.code` name, which is what `global-hotkey`
    /// parses (`KeyZ`, `Digit1`, `F5`, `Space`).
    pub fn code_name(self) -> String {
        match self {
            Key::Letter(c) => format!("Key{}", c as char),
            Key::Digit(d) => format!("Digit{d}"),
            Key::F(n) => format!("F{n}"),
            Key::Space => "Space".to_owned(),
        }
    }

    fn label(self) -> String {
        match self {
            Key::Letter(c) => (c as char).to_string(),
            Key::Digit(d) => d.to_string(),
            Key::F(n) => format!("F{n}"),
            Key::Space => "Space".to_owned(),
        }
    }

    fn parse(s: &str) -> Option<Key> {
        let up = s.to_ascii_uppercase();
        let b = up.as_bytes();
        if b.len() == 1 && b[0].is_ascii_uppercase() {
            return Some(Key::Letter(b[0]));
        }
        if b.len() == 1 && b[0].is_ascii_digit() {
            return Some(Key::Digit(b[0] - b'0'));
        }
        if let Some(rest) = up.strip_prefix("KEY")
            && rest.len() == 1
            && rest.as_bytes()[0].is_ascii_uppercase()
        {
            return Some(Key::Letter(rest.as_bytes()[0]));
        }
        if let Some(rest) = up.strip_prefix("DIGIT")
            && rest.len() == 1
            && rest.as_bytes()[0].is_ascii_digit()
        {
            return Some(Key::Digit(rest.as_bytes()[0] - b'0'));
        }
        if let Some(rest) = up.strip_prefix('F')
            && !rest.is_empty()
            && rest.len() <= 2
            && rest.bytes().all(|c| c.is_ascii_digit())
            && !rest.starts_with('0')
        {
            let n: u8 = rest.parse().ok()?;
            if (1..=24).contains(&n) {
                return Some(Key::F(n));
            }
        }
        if up == "SPACE" {
            return Some(Key::Space);
        }
        None
    }
}

/// A validated hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyError {
    /// Not of the form `Mod+…+Key`, or an unknown token.
    Syntax,
    /// Neither Alt nor Ctrl.
    NeedsAltOrCtrl,
    /// Ctrl+Alt (= AltGr).
    CtrlAlt,
    /// Win / Super / Meta.
    Win,
    /// Esc is the cancel key.
    Reserved,
    /// Both actions on the same combination.
    Duplicate,
}

impl HotkeyError {
    /// Stable code for the UI (`ipc::IpcError`).
    pub fn code(self) -> &'static str {
        match self {
            HotkeyError::Syntax => "hotkey_syntax",
            HotkeyError::NeedsAltOrCtrl => "hotkey_needs_alt_or_ctrl",
            HotkeyError::CtrlAlt => "hotkey_ctrl_alt",
            HotkeyError::Win => "hotkey_win",
            HotkeyError::Reserved => "hotkey_reserved",
            HotkeyError::Duplicate => "hotkey_duplicate",
        }
    }
}

impl Combo {
    pub fn parse(s: &str) -> Result<Combo, HotkeyError> {
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        if parts.len() < 2 || parts.len() > 4 || parts.iter().any(|p| p.is_empty()) {
            return Err(HotkeyError::Syntax);
        }
        let (key_part, mods) = parts.split_last().ok_or(HotkeyError::Syntax)?;
        let (mut ctrl, mut alt, mut shift) = (false, false, false);
        for m in mods {
            let flag = match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut ctrl,
                "alt" | "option" => &mut alt,
                "shift" => &mut shift,
                "win" | "super" | "meta" | "cmd" | "command" => return Err(HotkeyError::Win),
                _ => return Err(HotkeyError::Syntax),
            };
            if *flag {
                return Err(HotkeyError::Syntax);
            }
            *flag = true;
        }
        if key_part.eq_ignore_ascii_case("esc") || key_part.eq_ignore_ascii_case("escape") {
            return Err(HotkeyError::Reserved);
        }
        let key = Key::parse(key_part).ok_or(HotkeyError::Syntax)?;
        if ctrl && alt {
            return Err(HotkeyError::CtrlAlt);
        }
        if !ctrl && !alt {
            return Err(HotkeyError::NeedsAltOrCtrl);
        }
        Ok(Combo {
            ctrl,
            alt,
            shift,
            key,
        })
    }

    /// Accelerator string for `global-hotkey` (`alt+shift+KeyZ`).
    pub fn accelerator(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("control+");
        }
        if self.alt {
            s.push_str("alt+");
        }
        if self.shift {
            s.push_str("shift+");
        }
        s.push_str(&self.key.code_name());
        s
    }
}

impl fmt::Display for Combo {
    /// Canonical form, as stored in the config: `Alt+Shift+Z`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.alt {
            f.write_str("Alt+")?;
        }
        if self.shift {
            f.write_str("Shift+")?;
        }
        f.write_str(&self.key.label())
    }
}

/// Validate the polish / polish-and-translate pair.
pub fn parse_pair(polish: &str, translate: &str) -> Result<(Combo, Combo), HotkeyError> {
    let a = Combo::parse(polish)?;
    let b = Combo::parse(translate)?;
    if a == b {
        return Err(HotkeyError::Duplicate);
    }
    Ok((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keepsay_core::config::WINDOWS_HOTKEYS;

    #[test]
    fn defaults_parse() {
        let (a, b) = parse_pair(WINDOWS_HOTKEYS.polish, WINDOWS_HOTKEYS.polish_translate).unwrap();
        assert_eq!(a.to_string(), "Alt+Shift+Z");
        assert_eq!(b.to_string(), "Alt+Shift+X");
        assert_eq!(a.accelerator(), "alt+shift+KeyZ");
        assert_eq!(b.accelerator(), "alt+shift+KeyX");
    }

    #[test]
    fn canonicalizes() {
        let c = Combo::parse(" shift + alt + z ").unwrap();
        assert_eq!(c.to_string(), "Alt+Shift+Z");
        assert_eq!(Combo::parse("Ctrl+F5").unwrap().to_string(), "Ctrl+F5");
        assert_eq!(Combo::parse("Alt+Space").unwrap().accelerator(), "alt+Space");
        assert_eq!(Combo::parse("Alt+Digit3").unwrap().to_string(), "Alt+3");
        assert_eq!(Combo::parse("Ctrl+Shift+KeyQ").unwrap().to_string(), "Ctrl+Shift+Q");
    }

    #[test]
    fn rejects() {
        assert_eq!(Combo::parse("Z"), Err(HotkeyError::Syntax));
        assert_eq!(Combo::parse("Shift+Z"), Err(HotkeyError::NeedsAltOrCtrl));
        assert_eq!(Combo::parse("Ctrl+Alt+Z"), Err(HotkeyError::CtrlAlt));
        assert_eq!(Combo::parse("Ctrl+Alt+Shift+Space"), Err(HotkeyError::CtrlAlt));
        assert_eq!(Combo::parse("Win+H"), Err(HotkeyError::Win));
        assert_eq!(Combo::parse("Alt+Esc"), Err(HotkeyError::Reserved));
        assert_eq!(Combo::parse("Alt+Alt+Z"), Err(HotkeyError::Syntax));
        assert_eq!(Combo::parse("Alt+F25"), Err(HotkeyError::Syntax));
        assert_eq!(Combo::parse("Alt+F0"), Err(HotkeyError::Syntax));
        assert_eq!(Combo::parse("Alt+Enter"), Err(HotkeyError::Syntax));
        assert_eq!(Combo::parse("Alt++Z"), Err(HotkeyError::Syntax));
        assert_eq!(Combo::parse("Alt+Shift+Z+X"), Err(HotkeyError::Syntax));
        assert_eq!(
            parse_pair("Alt+Shift+Z", "shift+alt+z"),
            Err(HotkeyError::Duplicate)
        );
    }
}
