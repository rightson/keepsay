#![cfg(all(feature = "test-support", feature = "test-endpoint"))]
//! US-M1-13 ③: `polish()` end to end against wiremock, through the real
//! `AnthropicClient`. Asserts only lengths, flags and reasons.

use keepsay_core::api::AnthropicClient;
use keepsay_core::vault::mock::MockVault;
use keepsay_core::{BudgetStatus, FallbackReason, KeyVault, Mode, PolishOptions, TargetLang};
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const INPUT: &str = "嗯那個我們明天下午三點要開會，然後要討論一下 Q4 的 roadmap，記得帶筆電";
const TRANSLATED: &str = "We have a meeting at 3 p.m. tomorrow to discuss the Q4 roadmap. Remember to bring your laptop.";

fn vault() -> MockVault {
    let v = MockVault::default();
    v.store(SecretString::from("sk-ant-test-e2e-0000000000"))
        .unwrap();
    v
}

async fn server_replying(status: u16, text: &str) -> MockServer {
    let server = MockServer::start().await;
    let body = serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 40, "output_tokens": 20},
    });
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn opts(translate_to: Option<TargetLang>) -> PolishOptions {
    PolishOptions {
        mode: Mode::Polish,
        translate_to,
        glossary: vec!["roadmap".into()],
    }
}

#[tokio::test]
#[ignore = "M1: needs workstream A's AnthropicClient (with_test_endpoint, messages)"]
async fn translate_round_trip() {
    let server = server_replying(200, &format!("Here's the translation:\n{TRANSLATED}")).await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    let v = vault();
    let r = keepsay_core::polish::polish(
        &client,
        &v,
        INPUT,
        &opts(Some(TargetLang::En)),
        &BudgetStatus::default(),
    )
    .await;
    assert!(!r.fell_back_to_raw);
    assert!(r.text.len() == TRANSLATED.len());
    assert!(
        r.usage
            .is_some_and(|u| u.input_tokens == 40 && u.output_tokens == 20)
    );

    let reqs = server.received_requests().await.unwrap();
    assert!(reqs.len() == 1);
    let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
    let user = body["messages"][0]["content"].as_str().unwrap();
    assert!(user.starts_with("<transcript>") && user.ends_with("</transcript>"));
    assert!(body.get("tools").is_none());
}

#[tokio::test]
#[ignore = "M1: needs workstream A's AnthropicClient (with_test_endpoint, messages)"]
async fn untranslated_output_falls_back_with_usage() {
    let server = server_replying(200, INPUT).await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    let r = keepsay_core::polish::polish(
        &client,
        &vault(),
        INPUT,
        &opts(Some(TargetLang::En)),
        &BudgetStatus::default(),
    )
    .await;
    assert!(r.fell_back_to_raw && r.text == INPUT);
    assert!(r.fallback_reason == Some(FallbackReason::GuardRejected));
    assert!(r.usage.is_some());
}

#[tokio::test]
#[ignore = "M1: needs workstream A's AnthropicClient (with_test_endpoint, messages)"]
async fn unauthorized_falls_back_to_raw() {
    let server = server_replying(401, "").await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    let r = keepsay_core::polish::polish(
        &client,
        &vault(),
        INPUT,
        &opts(None),
        &BudgetStatus::default(),
    )
    .await;
    assert!(r.fell_back_to_raw && r.text == INPUT);
    assert!(r.fallback_reason == Some(FallbackReason::KeyInvalid));
}

#[tokio::test]
#[ignore = "M1: needs workstream A's AnthropicClient (with_test_endpoint, messages)"]
async fn hard_limit_and_no_key_never_reach_the_server() {
    let server = server_replying(200, TRANSLATED).await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    let limited = BudgetStatus {
        estimated_usd: 9.0,
        hard_limit_usd: Some(5.0),
        ..Default::default()
    };
    let r = keepsay_core::polish::polish(&client, &vault(), INPUT, &opts(None), &limited).await;
    assert!(r.fallback_reason == Some(FallbackReason::HardLimit));
    let empty = MockVault::default();
    let r = keepsay_core::polish::polish(
        &client,
        &empty,
        INPUT,
        &opts(Some(TargetLang::Ja)),
        &BudgetStatus::default(),
    )
    .await;
    assert!(r.fallback_reason == Some(FallbackReason::TranslationNeedsApi));
    assert!(server.received_requests().await.unwrap().is_empty());
}
