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
#[ignore = "M1: needs AnthropicClient + wiremock (feature test-endpoint)"]
fn redirect_to_other_host_does_not_leak_key() {
    // mock server answers 302 -> second mock server; assert the second one
    // never receives x-api-key.
}
