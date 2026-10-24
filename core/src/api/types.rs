//! Wire types for `POST /v1/messages`.
//!
//! INV-4 is enforced by omission: there is no `tools`, `tool_choice`,
//! `mcp_servers`, or any other field that lets the model act. Do not add one.
//! `core/tests/request_type.rs` asserts the serialized key set.
//!
//! `Debug` impls here are hand-written: requests carry the transcript and
//! glossary, responses carry model output, so they print lengths only (§6.9).

use std::fmt;

use serde::{Deserialize, Serialize};

pub use crate::budget::Usage;

/// Models the client is allowed to request (V4, docs/changes/0006).
///
/// Only models that accept `thinking: {"type": "disabled"}` belong here,
/// because every request sends it (see [`Thinking`]).
pub const MODEL_ALLOWLIST: &[&str] = &["claude-haiku-5-5"];

/// Claude Haiku 5.5, verified against the official model list (V4).
pub const DEFAULT_MODEL: &str = "claude-haiku-5-5";

/// Wire shape of `POST /v1/messages`.
///
/// No `temperature`: API version `2026-10-21` rejects `temperature`,
/// `top_p` and `top_k` on every model, and Claude Haiku 5.5 already rejected
/// non-default values (V4, docs/changes/0006).
#[derive(Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<Message>,
    pub thinking: Thinking,
}

/// Thinking configuration. Always `disabled`: dictation polish is a short
/// rewrite, and adaptive thinking (on by default for Haiku 5.5) would spend
/// the small `max_tokens` budget and add latency. Serializes to
/// `{"type":"disabled"}`. This only controls hidden reasoning; it gives the
/// model no way to act (INV-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Thinking {
    Disabled,
}

#[derive(Clone, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Only `user` exists: a request is always exactly one user turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
            system,
            messages: vec![Message {
                role: Role::User,
                content: user,
            }],
            thinking: Thinking::Disabled,
        }
    }
}

impl fmt::Debug for MessagesRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessagesRequest")
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("system_len", &self.system.len())
            .field("messages", &self.messages)
            .field("thinking", &self.thinking)
            .finish()
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("role", &self.role)
            .field("content_len", &self.content.len())
            .finish()
    }
}

/// The subset of the `/v1/messages` response the core reads. Unknown fields
/// are ignored.
#[derive(Clone, Deserialize)]
pub struct MessagesResponse {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

/// Only `text` blocks are used. Any other block type (there should be none,
/// since requests never offer tools) deserializes to `Other` and is ignored.
#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

impl MessagesResponse {
    /// Concatenation of all `text` blocks.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                ContentBlock::Other => None,
            })
            .collect()
    }
}

impl MessagesResponse {
    /// The model ended on its own (`end_turn` / `stop_sequence`). `false` for
    /// `max_tokens` (truncated), `refusal` and anything unknown; callers
    /// should fall back to raw instead of inserting a partial result.
    pub fn is_complete(&self) -> bool {
        matches!(
            self.stop_reason.as_deref(),
            Some("end_turn") | Some("stop_sequence")
        )
    }
}

impl fmt::Debug for MessagesResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessagesResponse")
            .field("content", &self.content)
            .field("stop_reason", &self.stop_reason)
            .field("usage", &self.usage)
            .finish()
    }
}

impl fmt::Debug for ContentBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentBlock::Text { text } => f
                .debug_struct("Text")
                .field("text_len", &text.len())
                .finish(),
            ContentBlock::Other => f.write_str("Other"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_prints_lengths_only() {
        let req = MessagesRequest::single_turn("SYS-MARKER".into(), "USER-MARKER".into(), 1, false);
        let dbg = format!("{req:?}");
        assert!(!dbg.contains("SYS-MARKER"));
        assert!(!dbg.contains("USER-MARKER"));
    }

    #[test]
    fn response_ignores_non_text_blocks() {
        let json = r#"{"content":[{"type":"text","text":"a"},{"type":"thinking","thinking":"x"},{"type":"text","text":"b"}],
                       "stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":4,"cache_read_input_tokens":0}}"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.text().len(), 2);
        assert_eq!(resp.usage.output_tokens, 4);
        assert!(!format!("{resp:?}").contains("\"a\""));
        assert!(resp.is_complete());
    }

    #[test]
    fn truncated_or_refused_is_not_complete() {
        for reason in ["max_tokens", "refusal", "pause_turn"] {
            let json = format!(
                r#"{{"content":[],"stop_reason":"{reason}","usage":{{"input_tokens":1,"output_tokens":1}}}}"#
            );
            let resp: MessagesResponse = serde_json::from_str(&json).unwrap();
            assert!(!resp.is_complete(), "{reason}");
        }
    }

    #[test]
    fn default_model_is_allowed() {
        assert!(MODEL_ALLOWLIST.contains(&DEFAULT_MODEL));
    }
}
