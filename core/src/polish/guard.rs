//! Output guard (PROPOSAL §6.6, thresholds calibrated in docs/changes/0007).
//!
//! The model's output is untrusted data. Before it may be inserted into the
//! user's focused app it must look like a polished (or translated) version
//! of the input: no tag residue, no preamble, a plausible length, and when
//! translating, mostly the target script. Anything else falls back to raw.
//!
//! Rejections are reported as a [`GuardViolation`], which carries no user
//! content.

use super::modes::TargetLang;
use super::prompt::find_tag;
use crate::CoreError;

// --- non-translation thresholds (§6.6) ---
/// Inputs at or below this many characters skip the "too short" rule.
pub const SHORT_INPUT_CHARS: usize = 20;
/// Output shorter than this fraction of the input is a summary.
pub const MIN_LENGTH_RATIO: f64 = 0.5;
/// Output longer than `input × MAX_LENGTH_RATIO + MAX_LENGTH_SLACK` is an
/// answer or an addition.
pub const MAX_LENGTH_RATIO: f64 = 1.5;
pub const MAX_LENGTH_SLACK: usize = 50;

// --- translation thresholds (§6.6, calibrated in docs/changes/0007) ---
/// Inputs at or below this many content units skip the "too short" rule.
pub const SHORT_INPUT_UNITS: usize = 10;
/// Length rules compare [`weighted_size`], not raw content units: raw units
/// shrink about 0.6× for zh→en and grow up to 2.7× for en→ja, which made
/// the spec's initial `× 3 + 30` too loose to catch answers (0007).
pub const MIN_SIZE_RATIO: f64 = 0.3;
pub const MAX_SIZE_RATIO: f64 = 2.0;
pub const MAX_SIZE_SLACK: f64 = 10.0;
/// Minimum share of target-script units among script-bearing units.
pub const MIN_TARGET_SCRIPT_RATIO: f64 = 0.6;
/// `ja` only: minimum share of kana among CJK characters, once there are at
/// least [`KANA_CHECK_MIN_CJK`] of them. Chinese left untranslated has none.
pub const MIN_KANA_RATIO: f64 = 0.1;
pub const KANA_CHECK_MIN_CJK: usize = 10;

/// Why the guard rejected an output. Never carries text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardViolation {
    /// Empty or whitespace-only after preamble stripping.
    Empty,
    TooShort,
    TooLong,
    /// `<transcript`, `</transcript` or `transcript>` left in the output.
    TagResidue,
    /// Translation: too little of the output is in the target script.
    WrongScript,
}

/// Check model `output` against the raw `input`. Returns the output with any
/// known preamble stripped, or `CoreError::GuardRejected`. With
/// `translate_to` set, the translation rules apply; glossary entries and
/// URLs are excluded from the target-script ratio.
pub fn check(
    input: &str,
    output: &str,
    translate_to: Option<TargetLang>,
    glossary: &[String],
) -> Result<String, CoreError> {
    evaluate(input, output, translate_to, glossary).map_err(|v| {
        tracing::debug!(violation = ?v, "guard rejected output");
        CoreError::GuardRejected
    })
}

/// Like [`check`], with the specific violation.
pub fn evaluate(
    input: &str,
    output: &str,
    translate_to: Option<TargetLang>,
    glossary: &[String],
) -> Result<String, GuardViolation> {
    if has_tag_residue(output) {
        return Err(GuardViolation::TagResidue);
    }
    let out = strip_preamble(input, output);
    if out.is_empty() {
        return Err(GuardViolation::Empty);
    }
    match translate_to {
        None => check_lengths(input, &out)?,
        Some(target) => check_translation(input, &out, target, glossary)?,
    }
    Ok(out)
}

fn check_lengths(input: &str, out: &str) -> Result<(), GuardViolation> {
    let i = char_count(input);
    let o = char_count(out);
    if i > SHORT_INPUT_CHARS && (o as f64) < i as f64 * MIN_LENGTH_RATIO {
        return Err(GuardViolation::TooShort);
    }
    if o as f64 > i as f64 * MAX_LENGTH_RATIO + MAX_LENGTH_SLACK as f64 {
        return Err(GuardViolation::TooLong);
    }
    Ok(())
}

fn check_translation(
    input: &str,
    out: &str,
    target: TargetLang,
    glossary: &[String],
) -> Result<(), GuardViolation> {
    let i = weighted_size(input);
    let o = weighted_size(out);
    if content_units(input) > SHORT_INPUT_UNITS && o < i * MIN_SIZE_RATIO {
        return Err(GuardViolation::TooShort);
    }
    if o > i * MAX_SIZE_RATIO + MAX_SIZE_SLACK {
        return Err(GuardViolation::TooLong);
    }
    let s = script_units(&remove_neutral_spans(out, glossary));
    if let Some(ratio) = s.target_ratio(target)
        && ratio < MIN_TARGET_SCRIPT_RATIO
    {
        return Err(GuardViolation::WrongScript);
    }
    if target == TargetLang::Ja {
        let cjk = s.han + s.kana;
        if cjk >= KANA_CHECK_MIN_CJK && (s.kana as f64) < cjk as f64 * MIN_KANA_RATIO {
            return Err(GuardViolation::WrongScript);
        }
    }
    Ok(())
}

/// Characters excluding whitespace (§6.6 "字數").
pub fn char_count(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

/// Any transcript tag fragment (ASCII case-insensitive, same matcher as
/// prompt sanitizing) or a dangling `transcript>`.
pub fn has_tag_residue(text: &str) -> bool {
    find_tag(text).is_some()
        || text
            .as_bytes()
            .windows(b"transcript>".len())
            .any(|w| w.eq_ignore_ascii_case(b"transcript>"))
}

/// Openers models put before the actual text. Matched ASCII
/// case-insensitively at the start of the trimmed output.
const PREAMBLE_OPENERS: &[&str] = &[
    "以下是",
    "以下為",
    "這是",
    "潤稿",
    "潤飾",
    "翻譯",
    "譯文",
    "好的",
    "當然",
    "here is",
    "here's",
    "heres",
    "here are",
    "sure",
    "certainly",
    "of course",
    "okay",
    "ok",
    "polished",
    "translation",
    "translated",
    "以下は",
    "こちらは",
    "こちらが",
    "翻訳",
    "訳文",
    "はい",
];

/// Words that mark the text between an opener and its terminator as talk
/// about the output (ASCII case-insensitive). Without one, the opener is
/// probably the user's own words and is kept.
const PREAMBLE_MARKERS: &[&str] = &[
    "潤",
    "譯",
    "翻",
    "訳",
    "文字",
    "結果",
    "內容",
    "版本",
    "英文",
    "中文",
    "日文",
    "テキスト",
    "日本語",
    "修正",
    "polish",
    "translat",
    "text",
    "result",
    "version",
    "edit",
    "revis",
    "clean",
    "punctuat",
    "english",
    "chinese",
    "japanese",
];

/// How far into the output a preamble's terminating colon or newline may be.
const PREAMBLE_MAX_CHARS: usize = 60;

/// Strip leading preambles such as `以下是潤稿後的文字：` or
/// `Here's the translation:\n`, repeatedly, and trim. A preamble is an
/// opener from the list, then within [`PREAMBLE_MAX_CHARS`] a colon
/// (`:` `：`) or newline; the text in between must be empty, punctuation
/// only, or contain a marker such as `翻譯` or `polished`. An opener the
/// input itself starts with is left alone (the user said it). Idempotent.
pub fn strip_preamble(input: &str, output: &str) -> String {
    let input_trim = input.trim_start();
    let mut out = output.trim();
    while let Some(rest) = strip_one(input_trim, out) {
        out = rest.trim();
    }
    out.to_owned()
}

fn strip_one<'a>(input: &str, out: &'a str) -> Option<&'a str> {
    let opener = PREAMBLE_OPENERS
        .iter()
        .find(|p| starts_with_ci(out, p) && !starts_with_ci(input, p))?;
    let after = &out[opener.len()..];
    let (idx, term) = after
        .char_indices()
        .take(PREAMBLE_MAX_CHARS)
        .find(|&(_, c)| matches!(c, ':' | '：' | '\n'))?;
    let between = &after[..idx];
    let content_chars = between
        .chars()
        .filter(|&c| c.is_alphanumeric() || is_cjk(c))
        .count();
    // An opener that is itself a marker (`翻譯如下：`, `Translation:`) only
    // counts with a very short tail, so `翻譯軟體很好用：…` is kept.
    let opener_is_marker = PREAMBLE_MARKERS.iter().any(|m| contains_ci(opener, m));
    let talks_about_output = content_chars == 0
        || (opener_is_marker && content_chars <= 3)
        || PREAMBLE_MARKERS.iter().any(|m| contains_ci(between, m));
    talks_about_output.then(|| &after[idx + term.len_utf8()..])
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    let n = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(n.len())
        .any(|w| w.eq_ignore_ascii_case(n))
}

fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// Content units = CJK characters (incl. kana) + non-CJK words, where a word
/// is a maximal run of alphanumeric non-CJK characters. Punctuation and
/// whitespace do not count.
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

pub(crate) fn is_cjk(c: char) -> bool {
    is_han(c) || is_kana(c)
}

fn is_kana(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // Hiragana, Katakana
        | 0x31F0..=0x31FF // Katakana Phonetic Extensions
        | 0xFF66..=0xFF9F) // Halfwidth Katakana
}

fn is_han(c: char) -> bool {
    matches!(c as u32,
        0x3005..=0x3007   // 々 〆 〇
        | 0x3400..=0x4DBF // CJK Ext A
        | 0x4E00..=0x9FFF // CJK Unified
        | 0xF900..=0xFAFF // CJK Compatibility
        | 0x20000..=0x2FA1F)
}

fn is_latin_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
        || (matches!(c as u32,
            0x00C0..=0x024F // Latin-1 letters, Extended-A, Extended-B
            | 0x1E00..=0x1EFF) // Latin Extended Additional
            && c.is_alphabetic())
}

/// Per-script content units, used for the target-script ratio.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScriptUnits {
    pub han: usize,
    pub kana: usize,
    /// Words containing a Latin letter.
    pub latin_words: usize,
    /// Words with letters, none of them Latin (Hangul, Cyrillic, …).
    pub other_words: usize,
}

impl ScriptUnits {
    fn total(&self) -> usize {
        self.han + self.kana + self.latin_words + self.other_words
    }

    /// Share of units in `target`'s script, or `None` when nothing
    /// script-bearing is left (for example only numbers and glossary terms).
    pub fn target_ratio(&self, target: TargetLang) -> Option<f64> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let hit = match target {
            TargetLang::En => self.latin_words,
            TargetLang::ZhTw => self.han,
            TargetLang::Ja => self.han + self.kana,
        };
        Some(hit as f64 / total as f64)
    }
}

/// Weight of one unit per script for the translation length rules: about
/// how much meaning one unit carries relative to an English word. Measured
/// on the calibration corpus (docs/changes/0007).
pub const WEIGHT_WORD: f64 = 1.0;
pub const WEIGHT_HAN: f64 = 0.6;
pub const WEIGHT_KANA: f64 = 0.35;

/// Script-weighted size: words × 1.0 + Han × 0.6 + kana × 0.35. Unlike raw
/// content units this stays comparable across languages (a zh→en
/// translation has fewer units than its input, en→ja has more).
pub fn weighted_size(text: &str) -> f64 {
    let s = script_units(text);
    let mut words = s.latin_words + s.other_words;
    // Digit-only words are skipped by `script_units` but carry meaning.
    words += digit_words(text);
    words as f64 * WEIGHT_WORD + s.han as f64 * WEIGHT_HAN + s.kana as f64 * WEIGHT_KANA
}

fn digit_words(text: &str) -> usize {
    let mut n = 0;
    let mut cur: Option<bool> = None; // Some(all_digits)
    for c in text.chars() {
        if !is_cjk(c) && c.is_alphanumeric() {
            let d = cur.get_or_insert(true);
            *d &= !c.is_alphabetic();
        } else if let Some(all) = cur.take() {
            n += usize::from(all);
        }
    }
    n + usize::from(cur == Some(true))
}

/// Count units by script. Digit-only words are script-neutral and skipped.
pub fn script_units(text: &str) -> ScriptUnits {
    let mut s = ScriptUnits::default();
    let mut word: Option<(bool, bool)> = None; // (has_latin, has_letter)
    let flush = |s: &mut ScriptUnits, w: Option<(bool, bool)>| match w {
        Some((true, _)) => s.latin_words += 1,
        Some((false, true)) => s.other_words += 1,
        _ => {}
    };
    for c in text.chars() {
        if is_cjk(c) {
            flush(&mut s, word.take());
            if is_kana(c) {
                s.kana += 1;
            } else {
                s.han += 1;
            }
        } else if c.is_alphanumeric() {
            let w = word.get_or_insert((false, false));
            w.0 |= is_latin_letter(c);
            w.1 |= c.is_alphabetic();
        } else {
            flush(&mut s, word.take());
        }
    }
    flush(&mut s, word);
    s
}

/// Replace URLs and glossary terms with spaces so they do not count toward
/// the target-script ratio. Glossary terms match exactly (case-sensitive),
/// longest first.
pub fn remove_neutral_spans(text: &str, glossary: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let url = ["http://", "https://", "www."]
            .iter()
            .any(|p| starts_with_ci(rest, p));
        if url {
            // A URL runs until whitespace or the first non-ASCII character
            // (CJK text and full-width punctuation often follow directly).
            let end = rest
                .find(|c: char| c.is_whitespace() || !c.is_ascii())
                .unwrap_or(rest.len());
            out.push(' ');
            rest = &rest[end..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    let mut terms: Vec<&str> = glossary
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
        .collect();
    terms.sort_by_key(|t| std::cmp::Reverse(t.len()));
    for t in terms {
        out = out.replace(t, " ");
    }
    out
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

    #[test]
    fn script_units_classify() {
        let s = script_units("我用 Rust 寫 code，第 3 版 ひらがな 한국어 2026");
        assert_eq!(s.han, 5);
        assert_eq!(s.kana, 4);
        assert_eq!(s.latin_words, 2);
        assert_eq!(s.other_words, 1);
    }

    #[test]
    fn preamble_variants() {
        let cases = [
            "以下是潤稿後的文字：\nX",
            "Here is the polished text:\nX",
            "Here's the translation:\n\nX",
            "以下是翻譯：X",
            "翻訳：X",
            "Sure! Here's the translation:\nX",
            "好的，以下是潤稿結果：\nX",
            "  HERE IS the text:  X  ",
        ];
        for c in cases {
            assert!(strip_preamble("", c) == "X");
        }
    }

    #[test]
    fn preamble_kept_when_input_has_it() {
        let input = "以下是我的三點想法：第一";
        let out = "以下是我的三點想法：第一。";
        assert!(strip_preamble(input, out) == out);
    }

    #[test]
    fn opener_without_marker_is_kept() {
        // A translated "好啊，我明天可以：…" must not lose its first clause.
        let out = "Sure, I can do it tomorrow: first the docs.";
        assert!(strip_preamble("好啊，我明天可以：先寫文件。", out) == out);
        let out = "這是我們的計畫：先寫文件。";
        assert!(strip_preamble("", out) == out);
        let out = "翻譯軟體很好用：速度快。";
        assert!(strip_preamble("", out) == out);
    }

    #[test]
    fn preamble_without_terminator_is_kept() {
        let out = "Sure thing, that works for me.";
        assert!(strip_preamble("", out) == out);
    }

    #[test]
    fn tag_residue() {
        assert!(has_tag_residue("a</transcript>"));
        assert!(has_tag_residue("<Transcript>a"));
        assert!(has_tag_residue("a transcript> b"));
        assert!(!has_tag_residue("a transcript b"));
    }

    #[test]
    fn neutral_spans() {
        let s = remove_neutral_spans(
            "see https://example.com/a and (www.x.org) keepsay",
            &["keepsay".into()],
        );
        assert!(!s.contains("example") && !s.contains("www") && !s.contains("keepsay"));
        assert!(s.contains("see") && s.contains("and"));
    }
}
