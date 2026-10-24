//! Pipeline tests with a stand-in transport (no key, no network). The
//! wiremock end-to-end test is `core/tests/polish_e2e.rs`.

use std::cell::Cell;

use super::*;
use crate::budget::Usage;

const INPUT: &str = "嗯那個我們明天下午三點要開會，然後要討論一下 Q4 的 roadmap，記得帶筆電";
const POLISHED: &str = "我們明天下午三點要開會，討論 Q4 的 roadmap，記得帶筆電。";
const TRANSLATED: &str = "We have a meeting at 3 p.m. tomorrow to discuss the Q4 roadmap. Remember to bring your laptop.";

fn response(text: &str, stop: Option<&str>) -> MessagesResponse {
    let json = serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "stop_reason": stop,
        "usage": {"input_tokens": 11, "output_tokens": 7},
    });
    serde_json::from_value(json).unwrap()
}

fn opts(mode: Mode, translate_to: Option<TargetLang>) -> PolishOptions {
    PolishOptions {
        mode,
        translate_to,
        glossary: vec!["roadmap".into()],
    }
}

fn hard_limited() -> BudgetStatus {
    BudgetStatus {
        estimated_usd: 10.0,
        hard_limit_usd: Some(5.0),
        ..Default::default()
    }
}

/// Run the pipeline with a fixed transport result; returns the result and
/// whether the transport was called.
fn go(
    o: &PolishOptions,
    budget: &BudgetStatus,
    has_key: bool,
    reply: Result<MessagesResponse, CoreError>,
) -> (PolishResult, bool) {
    let called = Cell::new(false);
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let r = rt.block_on(run(INPUT, o, budget, has_key, |_req| {
        called.set(true);
        async move { reply }
    }));
    (r, called.get())
}

fn is_raw(r: &PolishResult, reason: Option<FallbackReason>) -> bool {
    r.text == INPUT && r.fell_back_to_raw == reason.is_some() && r.fallback_reason == reason
}

#[test]
fn raw_mode_never_calls_api() {
    let (r, called) = go(
        &opts(Mode::Raw, None),
        &BudgetStatus::default(),
        true,
        Err(CoreError::Network),
    );
    assert!(!called);
    assert!(is_raw(&r, None));
    assert!(r.usage.is_none());
}

#[test]
fn polish_ok() {
    for mode in [Mode::Light, Mode::Polish, Mode::Formal] {
        let (r, called) = go(
            &opts(mode, None),
            &BudgetStatus::default(),
            true,
            Ok(response(POLISHED, Some("end_turn"))),
        );
        assert!(called);
        assert!(r.text == POLISHED);
        assert!(!r.fell_back_to_raw && r.fallback_reason.is_none());
        assert!(
            r.usage
                == Some(Usage {
                    input_tokens: 11,
                    output_tokens: 7
                })
        );
    }
}

#[test]
fn translate_ok_and_preamble_stripped() {
    let text = format!("Here's the translation:\n{TRANSLATED}");
    let (r, _) = go(
        &opts(Mode::Polish, Some(TargetLang::En)),
        &BudgetStatus::default(),
        true,
        Ok(response(&text, Some("end_turn"))),
    );
    assert!(r.text == TRANSLATED);
    assert!(!r.fell_back_to_raw);
}

#[test]
fn translation_without_api_outputs_original() {
    let t = Some(TargetLang::En);
    let cases = [
        (opts(Mode::Raw, t), BudgetStatus::default(), true),
        (opts(Mode::Polish, t), hard_limited(), true),
        (opts(Mode::Polish, t), BudgetStatus::default(), false),
    ];
    for (o, b, key) in cases {
        let (r, called) = go(&o, &b, key, Ok(response(TRANSLATED, None)));
        assert!(!called);
        assert!(is_raw(&r, Some(FallbackReason::TranslationNeedsApi)));
    }
    // Offline is also "needs API".
    let (r, called) = go(
        &opts(Mode::Polish, t),
        &BudgetStatus::default(),
        true,
        Err(CoreError::Network),
    );
    assert!(called);
    assert!(is_raw(&r, Some(FallbackReason::TranslationNeedsApi)));
}

#[test]
fn no_key_and_hard_limit_without_translation() {
    let (r, called) = go(
        &opts(Mode::Polish, None),
        &BudgetStatus::default(),
        false,
        Ok(response(POLISHED, None)),
    );
    assert!(!called);
    assert!(is_raw(&r, Some(FallbackReason::NoKey)));

    let (r, called) = go(
        &opts(Mode::Polish, None),
        &hard_limited(),
        true,
        Ok(response(POLISHED, None)),
    );
    assert!(!called);
    assert!(is_raw(&r, Some(FallbackReason::HardLimit)));
}

#[test]
fn api_errors_fall_back_untranslated() {
    let cases = [
        (CoreError::KeyInvalid, FallbackReason::KeyInvalid),
        (CoreError::Timeout, FallbackReason::Timeout),
        (CoreError::RateLimited, FallbackReason::RateLimited),
        (CoreError::BudgetExhausted, FallbackReason::BudgetExhausted),
        (CoreError::ApiStatus(500), FallbackReason::ApiError),
        (CoreError::InvalidResponse, FallbackReason::ApiError),
    ];
    for t in [None, Some(TargetLang::Ja)] {
        for (e, reason) in cases {
            let (r, _) = go(
                &opts(Mode::Polish, t),
                &BudgetStatus::default(),
                true,
                Err(e),
            );
            assert!(is_raw(&r, Some(reason)));
            assert!(r.usage.is_none());
        }
    }
}

#[test]
fn guard_rejection_keeps_usage() {
    let bad = [
        response("</transcript>", Some("end_turn")),
        response("", Some("end_turn")),
        response("好。", Some("end_turn")),
        response(POLISHED, Some("max_tokens")),
        response(POLISHED, Some("refusal")),
    ];
    for resp in bad {
        let (r, _) = go(
            &opts(Mode::Polish, None),
            &BudgetStatus::default(),
            true,
            Ok(resp),
        );
        assert!(is_raw(&r, Some(FallbackReason::GuardRejected)));
        assert!(r.usage.is_some());
    }
}

#[test]
fn translation_left_in_source_language_is_rejected() {
    let (r, _) = go(
        &opts(Mode::Polish, Some(TargetLang::En)),
        &BudgetStatus::default(),
        true,
        Ok(response(POLISHED, Some("end_turn"))),
    );
    assert!(is_raw(&r, Some(FallbackReason::GuardRejected)));
}

#[test]
fn request_shape() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for t in [None, Some(TargetLang::ZhTw)] {
        let o = opts(Mode::Polish, t);
        let seen = Cell::new(None);
        rt.block_on(run(INPUT, &o, &BudgetStatus::default(), true, |req| {
            seen.set(Some((
                req.messages.len(),
                req.messages[0].content.starts_with("<transcript>"),
                req.system.starts_with(if t.is_some() {
                    "你是聽寫潤稿與翻譯器。"
                } else {
                    "你是聽寫潤稿器。"
                }),
                req.max_tokens,
            )));
            async { Err(CoreError::Network) }
        }));
        let (n, wrapped, sys_ok, max_tokens) = seen.get().unwrap();
        assert!(n == 1 && wrapped && sys_ok);
        let est = prompt::estimate_tokens(INPUT);
        let factor = if t.is_some() { 3 } else { 2 };
        assert!(max_tokens == (est * factor + 256).min(2048));
    }
}

#[test]
fn empty_transcript_skips_api() {
    let called = Cell::new(false);
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let r = rt.block_on(run(
        "  \n",
        &opts(Mode::Polish, Some(TargetLang::En)),
        &BudgetStatus::default(),
        true,
        |_req| {
            called.set(true);
            async { Err(CoreError::Network) }
        },
    ));
    assert!(!called.get());
    assert!(!r.fell_back_to_raw && r.text == "  \n");
}
