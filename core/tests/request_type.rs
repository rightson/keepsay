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
    assert_eq!(
        keys,
        ["max_tokens", "messages", "model", "system", "temperature"]
    );

    let text = json.to_string();
    for forbidden in ["tools", "tool_choice", "mcp_servers", "tool_use"] {
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
