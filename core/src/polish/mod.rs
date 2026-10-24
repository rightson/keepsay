//! Prompt construction and output guard (PROPOSAL §6.5, §6.6).

pub mod guard;
pub mod modes;
pub mod prompt;

pub use modes::{Mode, PolishOptions, TargetLang};
