//! Output guard (PROPOSAL §6.6). Full rules and proptest land in M1; the
//! content-unit counter for translation is defined here so the spec has a
//! concrete reference implementation.

/// Content units = CJK characters (incl. kana) + non-CJK words. Punctuation
/// and whitespace do not count.
pub fn content_units(text: &str) -> usize {
    let mut units = 0;
    let mut in_word = false;
    for c in text.chars() {
        if is_cjk(c) {
            units += 1;
            in_word = false;
        } else if c.is_alphanumeric() {
            if !in_word {
                units += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    units
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // Hiragana, Katakana
        | 0x3400..=0x4DBF // CJK Ext A
        | 0x4E00..=0x9FFF // CJK Unified
        | 0xF900..=0xFAFF // CJK Compatibility
        | 0x20000..=0x2FA1F)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_mixed() {
        assert_eq!(content_units("我用 Rust 寫 code。"), 5);
        assert_eq!(content_units("Hello, world!"), 2);
        assert_eq!(content_units("ひらがな"), 4);
        assert_eq!(content_units("  ，。!? "), 0);
    }
}
