#![cfg(all(feature = "test-support", feature = "test-endpoint"))]
//! US-M1-07 / US-M1-08: AnthropicClient headers, response parsing, error
//! mapping and the single bounded retry (PROPOSAL §6.3, V4 in
//! docs/changes/0006).
//!
//! The fake key lives only in the mock vault and is checked with a wiremock
//! header matcher; no assertion message or output ever includes it. Mocks
//! use no `.expect(n)` (its failure report would dump request headers);
//! request counts are checked through `received_requests` instead.

use std::time::{Duration, Instant};

use keepsay_core::CoreError;
use keepsay_core::KeyVault;
use keepsay_core::api::client::ANTHROPIC_VERSION;
use keepsay_core::api::{AnthropicClient, MessagesRequest};
use keepsay_core::vault::mock::MockVault;
use secrecy::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FAKE_KEY: &str = "sk-ant-test-CANARY-7f3a9c1d";

fn vault() -> MockVault {
    let v = MockVault::default();
    v.store(SecretString::from(FAKE_KEY)).unwrap();
    v
}

fn request() -> MessagesRequest {
    MessagesRequest::single_turn(
        "system".into(),
        "<transcript>x</transcript>".into(),
        5,
        false,
    )
}

fn ok_body() -> Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-haiku-5-5",
        "content": [{"type": "text", "text": "hello"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 12, "output_tokens": 3, "cache_read_input_tokens": 0}
    })
}

fn error_body(kind: &str, message: &str) -> Value {
    json!({"type": "error", "error": {"type": kind, "message": message}, "request_id": "req_1"})
}

async fn server_with(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

async fn count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

async fn call(server: &MockServer) -> Result<keepsay_core::api::MessagesResponse, CoreError> {
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    client.messages(&vault(), &request()).await
}

#[tokio::test]
async fn messages_sends_required_headers_and_parses_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", FAKE_KEY))
        .and(header("anthropic-version", ANTHROPIC_VERSION))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
        .mount(&server)
        .await;

    let resp = call(&server).await.unwrap();
    assert_eq!(resp.text().len(), 5);
    assert!(resp.is_complete());
    assert_eq!(resp.usage.input_tokens, 12);
    assert_eq!(resp.usage.output_tokens, 3);

    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1);
    let body: Value = serde_json::from_slice(&reqs[0].body).unwrap();
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["max_tokens", "messages", "model", "system", "thinking"]
    );
}

#[tokio::test]
async fn verify_key_calls_models_with_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(query_param("limit", "1"))
        .and(header("x-api-key", FAKE_KEY))
        .and(header("anthropic-version", ANTHROPIC_VERSION))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": [], "first_id": null, "last_id": null, "has_more": false}),
        ))
        .mount(&server)
        .await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    client
        .verify_key(&SecretString::from(FAKE_KEY))
        .await
        .unwrap();
    assert_eq!(count(&server).await, 1);
}

#[tokio::test]
async fn verify_key_maps_401_to_key_invalid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(error_body("authentication_error", "invalid x-api-key")),
        )
        .mount(&server)
        .await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    assert_eq!(
        client
            .verify_key(&SecretString::from(FAKE_KEY))
            .await
            .unwrap_err(),
        CoreError::KeyInvalid
    );
}

#[tokio::test]
async fn auth_errors_map_to_key_invalid_and_keep_the_key() {
    for code in [401u16, 403] {
        let server = server_with(
            ResponseTemplate::new(code).set_body_json(error_body("authentication_error", "no")),
        )
        .await;
        let v = vault();
        let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
        assert_eq!(
            client.messages(&v, &request()).await.unwrap_err(),
            CoreError::KeyInvalid
        );
        assert!(v.exists(), "a {code} must never delete the stored key");
        assert_eq!(count(&server).await, 1, "{code} is not retried");
    }
}

#[tokio::test]
async fn credit_and_spend_limit_errors_map_to_budget_exhausted() {
    let cases = [
        (
            400u16,
            error_body(
                "invalid_request_error",
                "Your credit balance is too low to access the Anthropic API.",
            ),
        ),
        (
            400,
            error_body(
                "invalid_request_error",
                "You have reached your specified API usage limits. You will regain access on 2026-11-01 at 00:00 UTC.",
            ),
        ),
        (
            400,
            error_body(
                "invalid_request_error",
                "You have reached your specified workspace API usage limits.",
            ),
        ),
        (402, error_body("billing_error", "billing problem")),
        (
            429,
            json!({"type": "error", "error": {
                "type": "rate_limit_error",
                "message": "You have reached your API usage limits.",
                "details": {"error_code": "enforced_spend_limit_reached"}
            }}),
        ),
    ];
    for (code, body) in cases {
        let server = server_with(ResponseTemplate::new(code).set_body_json(body)).await;
        assert_eq!(
            call(&server).await.unwrap_err(),
            CoreError::BudgetExhausted,
            "status {code}"
        );
        assert_eq!(count(&server).await, 1, "status {code} is not retried");
    }
}

#[tokio::test]
async fn other_400_is_api_status() {
    let server = server_with(
        ResponseTemplate::new(400)
            .set_body_json(error_body("invalid_request_error", "messages: bad")),
    )
    .await;
    assert_eq!(call(&server).await.unwrap_err(), CoreError::ApiStatus(400));
}

#[tokio::test]
async fn server_error_is_api_status_without_retry() {
    let server = server_with(ResponseTemplate::new(500)).await;
    assert_eq!(call(&server).await.unwrap_err(), CoreError::ApiStatus(500));
    assert_eq!(count(&server).await, 1);
}

#[tokio::test]
async fn rate_limit_retries_once_then_succeeds() {
    for code in [429u16, 529] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(code).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
            .with_priority(2)
            .mount(&server)
            .await;
        assert!(call(&server).await.is_ok(), "status {code}");
        assert_eq!(count(&server).await, 2, "status {code}");
    }
}

#[tokio::test]
async fn rate_limit_retries_only_once() {
    for code in [429u16, 529] {
        let server =
            server_with(ResponseTemplate::new(code).insert_header("retry-after", "0")).await;
        assert_eq!(
            call(&server).await.unwrap_err(),
            CoreError::RateLimited,
            "status {code}"
        );
        assert_eq!(count(&server).await, 2, "status {code}: exactly one retry");
    }
}

#[tokio::test]
async fn retry_after_is_capped_at_two_seconds() {
    let server = server_with(ResponseTemplate::new(429).insert_header("retry-after", "30")).await;
    let started = Instant::now();
    assert_eq!(call(&server).await.unwrap_err(), CoreError::RateLimited);
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(1900), "waited {elapsed:?}");
    assert!(elapsed < Duration::from_secs(5), "waited {elapsed:?}");
    assert_eq!(count(&server).await, 2);
}

#[tokio::test]
async fn timeout_maps_to_timeout() {
    let server = server_with(
        ResponseTemplate::new(200)
            .set_body_json(ok_body())
            .set_delay(Duration::from_secs(3)),
    )
    .await;
    let client =
        AnthropicClient::with_test_endpoint_timeout(&server.uri(), Duration::from_millis(200))
            .unwrap();
    assert_eq!(
        client.messages(&vault(), &request()).await.unwrap_err(),
        CoreError::Timeout
    );
}

#[tokio::test]
async fn malformed_success_body_is_invalid_response() {
    let server = server_with(ResponseTemplate::new(200).set_body_string("not json")).await;
    assert_eq!(call(&server).await.unwrap_err(), CoreError::InvalidResponse);
}

#[tokio::test]
async fn disallowed_model_sends_nothing() {
    let server = server_with(ResponseTemplate::new(200).set_body_json(ok_body())).await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    let mut req = request();
    req.model = "claude-opus-5-5".into();
    assert_eq!(
        client.messages(&vault(), &req).await.unwrap_err(),
        CoreError::ModelNotAllowed
    );
    assert_eq!(count(&server).await, 0);
}

#[tokio::test]
async fn missing_key_sends_nothing() {
    let server = server_with(ResponseTemplate::new(200).set_body_json(ok_body())).await;
    let client = AnthropicClient::with_test_endpoint(&server.uri()).unwrap();
    assert_eq!(
        client
            .messages(&MockVault::default(), &request())
            .await
            .unwrap_err(),
        CoreError::KeyNotFound
    );
    assert_eq!(count(&server).await, 0);
}

#[tokio::test]
async fn error_messages_carry_no_upstream_or_user_content() {
    let marker = "UPSTREAM-MARKER";
    let server = server_with(
        ResponseTemplate::new(418).set_body_json(error_body("invalid_request_error", marker)),
    )
    .await;
    let err = call(&server).await.unwrap_err();
    assert_eq!(err, CoreError::ApiStatus(418));
    let shown = format!("{err} {err:?}");
    assert!(!shown.contains(marker));
    assert!(!shown.contains("transcript"));
}
