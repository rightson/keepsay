//! INV-4: requests to Claude can never carry tools or anything that lets the
//! model act.

use keepsay_core::api::MessagesRequest;

#[test]
fn serialized_request_has_only_allowed_keys() {
    let req = MessagesRequest::single_turn(
        "system".into(),
        "<transcript>x</transcript>".into(),
        10,
        false,
    );
    let json = serde_json::to_value(&req).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    // No `temperature`: API version 2026-10-21 rejects sampling parameters
    // (docs/changes/0006). `thinking` is only ever `{"type":"disabled"}`.
    assert_eq!(
        keys,
        ["max_tokens", "messages", "model", "system", "thinking"]
    );
    assert_eq!(json["thinking"], serde_json::json!({"type": "disabled"}));

    let text = json.to_string();
    for forbidden in [
        "tools",
        "tool_choice",
        "mcp_servers",
        "tool_use",
        "container",
        "temperature",
    ] {
        assert!(
            !text.contains(forbidden),
            "request must not mention {forbidden}"
        );
    }
}

#[test]
fn exactly_one_user_message() {
    let req = MessagesRequest::single_turn("s".into(), "u".into(), 10, true);
    let json = serde_json::to_value(&req).unwrap();
    let msgs = json["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
}

#[test]
fn message_has_only_role_and_string_content() {
    let req = MessagesRequest::single_turn("s".into(), "u".into(), 10, false);
    let json = serde_json::to_value(&req).unwrap();
    let msg = json["messages"][0].as_object().unwrap();
    let mut keys: Vec<&str> = msg.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["content", "role"]);
    assert!(msg["content"].is_string(), "content must be a plain string");
}

#[test]
fn default_model_is_on_the_allowlist() {
    let req = MessagesRequest::single_turn(String::new(), String::new(), 1, false);
    assert!(keepsay_core::api::types::MODEL_ALLOWLIST.contains(&req.model.as_str()));
}

#[test]
fn max_tokens_formula() {
    assert_eq!(
        MessagesRequest::single_turn(String::new(), String::new(), 100, false).max_tokens,
        456
    );
    assert_eq!(
        MessagesRequest::single_turn(String::new(), String::new(), 100, true).max_tokens,
        556
    );
    assert_eq!(
        MessagesRequest::single_turn(String::new(), String::new(), 10_000, false).max_tokens,
        2048
    );
}
