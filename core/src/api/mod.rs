//! The only network client allowed to carry the API key (INV-3).

pub mod client;
pub mod error;
pub mod types;

pub use client::{AnthropicClient, BASE_URL};
pub use types::{ContentBlock, Message, MessagesRequest, MessagesResponse, Role};
