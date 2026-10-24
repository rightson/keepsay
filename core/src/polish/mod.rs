//! Prompt construction, output guard, and the `polish` pipeline
//! (PROPOSAL §6.5, §6.6, FR-4, FR-8).
//!
//! Owner: workstream B (docs/changes/0005).

pub mod guard;
pub mod modes;
pub mod prompt;

pub use modes::{FallbackReason, Mode, PolishOptions, PolishResult, TargetLang};

use crate::api::AnthropicClient;
use crate::budget::BudgetStatus;
use crate::vault::KeyVault;

/// Prompt → client → guard. Never fails: any error yields the raw
/// transcript with `fell_back_to_raw` and a reason (FR-4). The caller
/// records `PolishResult::usage` into the budget ledger.
///
/// `budget` is the current status; at the hard limit no request is made.
pub async fn polish(
    client: &AnthropicClient,
    vault: &dyn KeyVault,
    transcript: &str,
    opts: &PolishOptions,
    budget: &BudgetStatus,
) -> PolishResult {
    let _ = (client, vault, transcript, opts, budget);
    todo!("M1: polish — pipeline")
}
