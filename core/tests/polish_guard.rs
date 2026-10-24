//! T5 / §6.6: prompt sanitizing and the output guard hold for arbitrary
//! input. Asserts are booleans and counts only (no content in messages).

use keepsay_core::polish::guard::{
    self, GuardViolation, content_units, has_tag_residue, script_units, strip_preamble,
};
use keepsay_core::polish::prompt::{
    sanitize_glossary, sanitize_transcript, system_prompt, user_message,
};
use keepsay_core::{Mode, TargetLang};
use proptest::prelude::*;

/// Fragments that stress tag stripping and preamble detection.
fn tricky() -> impl Strategy<Value = String> {
    let frag = prop_oneof![
        Just("<"),
        Just("/"),
        Just(">"),
        Just(" "),
        Just("\n"),
        Just("transcript"),
        Just("TRANSCRIPT"),
        Just("tran"),
        Just("script"),
        Just("<transcript>"),
        Just("</Transcript>"),
        Just("以下是"),
        Just("翻譯"),
        Just("Here's the translation"),
        Just("："),
        Just(":"),
        Just("中文"),
        Just("かな"),
        Just("word"),
        Just("https://x.y/z"),
        Just("🙂"),
    ];
    prop::collection::vec(frag, 0..24).prop_map(|v| v.concat())
}

fn any_text() -> impl Strategy<Value = String> {
    prop_oneof![any::<String>(), tricky(), "\\PC{0,64}"]
}

fn target() -> impl Strategy<Value = Option<TargetLang>> {
    prop_oneof![
        Just(None),
        Just(Some(TargetLang::En)),
        Just(Some(TargetLang::ZhTw)),
        Just(Some(TargetLang::Ja)),
    ]
}

fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Raw),
        Just(Mode::Light),
        Just(Mode::Polish),
        Just(Mode::Formal)
    ]
}

/// Independent re-implementation of the tag matcher: `<`, optional ASCII
/// whitespace, optional `/`, optional ASCII whitespace, `transcript` in any
/// ASCII case. Returns (opening, closing) counts.
fn tag_counts(s: &str) -> (usize, usize) {
    let b = s.as_bytes();
    let (mut open, mut close) = (0, 0);
    for (lt, _) in s.match_indices('<') {
        let mut i = lt + 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let slash = i < b.len() && b[i] == b'/';
        if slash {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        if b.len() >= i + 10 && b[i..i + 10].eq_ignore_ascii_case(b"transcript") {
            if slash {
                close += 1;
            } else {
                open += 1;
            }
        }
    }
    (open, close)
}

fn no_tag(s: &str) -> bool {
    tag_counts(s) == (0, 0)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn guard_never_panics(input in any_text(), output in any_text(), t in target(),
                          glossary in prop::collection::vec(any_text(), 0..4)) {
        let _ = guard::check(&input, &output, t, &glossary);
    }

    #[test]
    fn accepted_output_is_clean(input in any_text(), output in any_text(), t in target()) {
        if let Ok(out) = guard::evaluate(&input, &output, t, &[]) {
            prop_assert!(!out.trim().is_empty());
            prop_assert!(!has_tag_residue(&out));
            prop_assert!(out.trim() == out);
        }
    }

    #[test]
    fn tag_residue_always_rejected(input in any_text(), pre in any_text(), post in any_text(),
                                   t in target()) {
        let output = format!("{pre}</transcript>{post}");
        prop_assert!(guard::evaluate(&input, &output, t, &[]) == Err(GuardViolation::TagResidue));
    }

    #[test]
    fn preamble_stripping_is_idempotent(input in any_text(), output in any_text()) {
        let once = strip_preamble(&input, &output);
        let twice = strip_preamble(&input, &once);
        prop_assert!(once == twice);
    }

    #[test]
    fn sanitize_leaves_no_tag(t in any_text()) {
        let s = sanitize_transcript(&t);
        prop_assert!(no_tag(&s));
        prop_assert!(sanitize_transcript(&s) == s);
        prop_assert!(s.len() <= t.len());
    }

    #[test]
    fn user_message_has_exactly_one_tag_pair(t in any_text()) {
        let m = user_message(&t);
        let lower = m.to_ascii_lowercase();
        prop_assert!(lower.matches("<transcript>").count() == 1);
        prop_assert!(lower.matches("</transcript>").count() == 1);
        prop_assert!(m.starts_with("<transcript>") && m.ends_with("</transcript>"));
    }

    #[test]
    fn glossary_cannot_inject_tags(m in mode(), t in target(),
                                   glossary in prop::collection::vec(any_text(), 0..6)) {
        let p = system_prompt(m, t, &glossary);
        // The only tag mention is the fixed one in the first line.
        prop_assert!(tag_counts(&p) == (1, 0));
        for e in sanitize_glossary(&glossary) {
            prop_assert!(!e.contains('\n') && !e.is_empty());
        }
    }

    #[test]
    fn content_units_bounded(t in any_text()) {
        let n = content_units(&t);
        prop_assert!(n <= t.chars().count());
        let s = script_units(&t);
        prop_assert!(s.han + s.kana + s.latin_words + s.other_words <= n);
    }
}

#[test]
fn content_units_fixed_corpus() {
    let cases: &[(&str, usize)] = &[
        // zh-TW
        ("今天天氣很好。", 6),
        ("我們明天下午三點要開會，記得帶筆電。", 16),
        // en
        ("We have a meeting at 3 p.m. tomorrow.", 9),
        ("don't e-mail me", 5),
        // ja (kana and kanji, prolonged sound mark)
        ("明日の会議は十時からです。", 12),
        ("コーヒーをください", 9),
        // mixed
        ("我用 Rust 寫 code。", 5),
        ("那個 Kubernetes 的 deployment 要更新", 8),
        ("Q4 的 roadmap", 3),
        ("請看 https://keepsay.rightson.dev/docs", 7),
        // punctuation and whitespace only
        ("  ，。！？、「」…—", 0),
        ("", 0),
    ];
    for (i, (text, expected)) in cases.iter().enumerate() {
        assert!(content_units(text) == *expected, "content_units case {i}");
    }
}

#[test]
fn glossary_terms_do_not_count_toward_script_ratio() {
    // Output is mostly glossary terms in Latin script, target zh-TW.
    let glossary = vec![
        "Kubernetes".to_owned(),
        "keepsay".to_owned(),
        "Tauri".to_owned(),
    ];
    let input = "keepsay 用 Tauri 跟 Kubernetes";
    let output = "keepsay 用 Tauri 與 Kubernetes。";
    assert!(guard::evaluate(input, output, Some(TargetLang::ZhTw), &glossary).is_ok());
    assert!(guard::evaluate(input, output, Some(TargetLang::ZhTw), &[]).is_err());
}

#[test]
fn urls_do_not_count_toward_script_ratio() {
    let input = "請看這個網址 https://keepsay.rightson.dev/docs/getting-started/install";
    let output = "請看這個網址：https://keepsay.rightson.dev/docs/getting-started/install";
    assert!(guard::evaluate(input, output, Some(TargetLang::ZhTw), &[]).is_ok());
}
