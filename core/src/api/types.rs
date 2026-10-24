//! Wire types for `POST /v1/messages`.
//!
//! INV-4 is enforced by omission: there is no `tools`, `tool_choice`,
//! `mcp_servers`, or any other field that lets the model act. Do not add one.
//! `deny_unknown_fields` is not relevant here (we only serialize); the test in
//! `core/tests/request_type.rs` asserts the serialized key set.

use serde::Serialize;

/// Models the client is allowed to request. 【待驗證 V4】
pub const MODEL_ALLOWLIST: &[&str] = &["claude-haiku-5-5"];

pub const DEFAULT_MODEL: &str = "claude-haiku-5-5";

#[derive(Debug, Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub system: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Only `user` exists: a request is always exactly one user turn.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
}

impl MessagesRequest {
    /// Build a single-turn request. `max_tokens` follows PROPOSAL §6.4.
    pub fn single_turn(
        system: String,
        user: String,
        estimated_input_tokens: u32,
        translate: bool,
    ) -> Self {
        let factor = if translate { 3 } else { 2 };
        Self {
            model: DEFAULT_MODEL.to_owned(),
            max_tokens: estimated_input_tokens
                .saturating_mul(factor)
                .saturating_add(256)
                .min(2048),
            temperature: 0.2,
            system,
            messages: vec![Message {
                role: Role::User,
                content: user,
            }],
        }
    }
}
