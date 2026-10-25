//! Platform-independent helpers for the Win32 code, tested on every OS.
//! No `unsafe` and no Win32 calls here.

use zeroize::Zeroize;

/// One key stroke for `SendInput` (a down and an up event).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    /// A UTF-16 code unit sent as VK_PACKET (`KEYEVENTF_UNICODE`).
    Unit(u16),
    /// A line break, sent as VK_RETURN: many apps ignore a VK_PACKET `\r`.
    Enter,
}

impl Zeroize for Stroke {
    fn zeroize(&mut self) {
        // Volatile write through zeroize's own u16 impl, so the text does
        // not linger in the plan buffer.
        if let Stroke::Unit(u) = self {
            u.zeroize();
        }
        *self = Stroke::Enter;
    }
}

/// Turn text into strokes. `\r\n`, `\r` and `\n` each become one Enter;
/// surrogate pairs stay as two units (Windows reassembles them).
pub fn plan_strokes(text: &str) -> Vec<Stroke> {
    let mut out = Vec::with_capacity(text.len());
    let mut prev_cr = false;
    for u in text.encode_utf16() {
        match u {
            0x0D => {
                out.push(Stroke::Enter);
                prev_cr = true;
                continue;
            }
            0x0A if prev_cr => {}
            0x0A => out.push(Stroke::Enter),
            _ => out.push(Stroke::Unit(u)),
        }
        prev_cr = false;
    }
    out
}

/// The sub-slice of `buf` without leading or trailing ASCII spaces, tabs or
/// line breaks (a pasted key often carries them). Works on the UTF-16 buffer
/// in place, so no extra copy of the key is made.
pub fn trim_ascii_ws(buf: &[u16]) -> &[u16] {
    let ws = |u: &u16| matches!(*u, 0x20 | 0x09 | 0x0A | 0x0D);
    let start = buf.iter().position(|u| !ws(u)).unwrap_or(buf.len());
    let end = buf.iter().rposition(|u| !ws(u)).map_or(start, |i| i + 1);
    &buf[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_cover_units_and_line_breaks() {
        let p = plan_strokes("a\r\nb\nc\rd");
        assert_eq!(
            p,
            vec![
                Stroke::Unit(u16::from(b'a')),
                Stroke::Enter,
                Stroke::Unit(u16::from(b'b')),
                Stroke::Enter,
                Stroke::Unit(u16::from(b'c')),
                Stroke::Enter,
                Stroke::Unit(u16::from(b'd')),
            ]
        );
        // CJK and a surrogate pair (U+1F600).
        let p = plan_strokes("你😀");
        assert_eq!(
            p,
            vec![
                Stroke::Unit(0x4F60),
                Stroke::Unit(0xD83D),
                Stroke::Unit(0xDE00)
            ]
        );
        assert!(plan_strokes("").is_empty());
        assert_eq!(plan_strokes("\n\n"), vec![Stroke::Enter, Stroke::Enter]);
    }

    #[test]
    fn stroke_zeroize() {
        let mut p = plan_strokes("xy");
        p.zeroize();
        assert!(p.is_empty());
        let mut s = Stroke::Unit(7);
        s.zeroize();
        assert_eq!(s, Stroke::Enter);
    }

    #[test]
    fn trims_ascii_whitespace_only() {
        let w: Vec<u16> = " \tab c\r\n".encode_utf16().collect();
        assert_eq!(trim_ascii_ws(&w), &w[2..6]);
        let w: Vec<u16> = "   ".encode_utf16().collect();
        assert!(trim_ascii_ws(&w).is_empty());
        assert!(trim_ascii_ws(&[]).is_empty());
        // U+3000 (ideographic space) is not trimmed: core's format check
        // rejects it instead.
        let w: Vec<u16> = "\u{3000}k".encode_utf16().collect();
        assert_eq!(trim_ascii_ws(&w), &w[..]);
    }
}
