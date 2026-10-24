//! INV-3: the key goes to https://api.anthropic.com and nowhere else.

use keepsay_core::api::client::validate_base_url;

#[test]
fn only_the_hardcoded_endpoint_is_accepted() {
    assert!(validate_base_url("https://api.anthropic.com").is_ok());
    assert!(validate_base_url("https://api.anthropic.com/").is_ok());
}

#[test]
fn everything_else_is_rejected() {
    for url in [
        "http://api.anthropic.com",
        "https://api.anthropic.com.evil.example",
        "https://api.anthropic.com:8443",
        "https://user@api.anthropic.com",
        "https://evil.example/https://api.anthropic.com",
        "https://API.ANTHROPIC.COM",
        "https://api.anthropic.com/v1",
        "",
    ] {
        assert!(validate_base_url(url).is_err(), "{url} must be rejected");
    }
}

#[test]
fn production_client_is_pinned_to_base_url() {
    let client = keepsay_core::api::AnthropicClient::new(None).unwrap();
    assert_eq!(client.base_url(), keepsay_core::api::BASE_URL);
}

#[cfg(all(feature = "test-endpoint", feature = "test-support"))]
mod with_mock_server {
    use keepsay_core::CoreError;
    use keepsay_core::KeyVault;
    use keepsay_core::api::{AnthropicClient, MessagesRequest};
    use keepsay_core::vault::mock::MockVault;
    use secrecy::SecretString;
    use wiremock::matchers::{any, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FAKE_KEY: &str = "sk-ant-test-CANARY-7f3a9c1d";

    fn vault() -> MockVault {
        let v = MockVault::default();
        v.store(SecretString::from(FAKE_KEY)).unwrap();
        v
    }

    #[tokio::test]
    async fn redirect_to_other_host_does_not_leak_key() {
        // Second host: a catch-all whose request log must stay empty.
        let other = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&other)
            .await;

        let first = MockServer::start().await;
        for (verb, p, code) in [("POST", "/v1/messages", 307), ("GET", "/v1/models", 302)] {
            let location = format!("{}{p}", other.uri());
            Mock::given(method(verb))
                .and(path(p))
                .and(header_exists("x-api-key"))
                .respond_with(
                    ResponseTemplate::new(code).insert_header("location", location.as_str()),
                )
                .mount(&first)
                .await;
        }

        let client = AnthropicClient::with_test_endpoint(&first.uri()).unwrap();
        let req = MessagesRequest::single_turn("s".into(), "u".into(), 1, false);
        assert_eq!(
            client.messages(&vault(), &req).await.unwrap_err(),
            CoreError::ApiStatus(307)
        );
        assert_eq!(
            client
                .verify_key(&SecretString::from(FAKE_KEY))
                .await
                .unwrap_err(),
            CoreError::ApiStatus(302)
        );

        assert_eq!(first.received_requests().await.unwrap().len(), 2);
        // Nothing at all reached the second host, so no x-api-key either.
        assert!(other.received_requests().await.unwrap().is_empty());
    }

    #[test]
    fn test_endpoint_accepts_only_plain_loopback() {
        for url in [
            "https://evil.example",
            "http://evil.example:80",
            "http://127.0.0.1.evil.example:80",
            "http://127.0.0.1:80/path",
            "http://127.0.0.1:",
            "http://user@127.0.0.1:80",
            "https://api.anthropic.com",
        ] {
            assert_eq!(
                AnthropicClient::with_test_endpoint(url).unwrap_err(),
                CoreError::DisallowedHost,
                "{url} must be rejected"
            );
        }
        assert!(AnthropicClient::with_test_endpoint("http://127.0.0.1:8080").is_ok());
        assert!(AnthropicClient::with_test_endpoint("http://localhost:8080/").is_ok());
    }
}
