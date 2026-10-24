//! Prompt construction, output guard, and the `polish` pipeline
//! (PROPOSAL §6.5, §6.6, FR-4, FR-8).
//!
//! Owner: workstream B (docs/changes/0005).

pub mod guard;
pub mod modes;
pub mod prompt;

use std::future::Future;
use std::time::Instant;

pub use modes::{FallbackReason, Mode, PolishOptions, PolishResult, TargetLang};

use crate::CoreError;
use crate::api::{AnthropicClient, MessagesRequest, MessagesResponse};
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
    // The vault goes to the client only; the pipeline never sees the key.
    run(transcript, opts, budget, vault.exists(), |req| async move {
        client.messages(vault, &req).await
    })
    .await
}

/// The pipeline with the transport abstracted, so tests can substitute the
/// API without a key or a network. `has_key` is `KeyVault::exists`.
async fn run<F, Fut>(
    transcript: &str,
    opts: &PolishOptions,
    budget: &BudgetStatus,
    has_key: bool,
    send: F,
) -> PolishResult
where
    F: FnOnce(MessagesRequest) -> Fut,
    Fut: Future<Output = Result<MessagesResponse, CoreError>>,
{
    if !opts.needs_api() || transcript.trim().is_empty() {
        return PolishResult::raw(transcript, None);
    }
    let translate = opts.translate_to;

    let blocked = if opts.mode == Mode::Raw {
        // Only reachable with translation: raw mode means "no API" (FR-8).
        Some(FallbackReason::TranslationNeedsApi)
    } else if budget.hard_limited() {
        Some(FallbackReason::HardLimit)
    } else if !has_key {
        Some(FallbackReason::NoKey)
    } else {
        None
    };
    if let Some(reason) = blocked {
        tracing::info!(?reason, "polish skipped API");
        return PolishResult::raw(transcript, Some(api_unusable(reason, translate)));
    }

    let system = prompt::system_prompt(opts.mode, translate, &opts.glossary);
    let user = prompt::user_message(transcript);
    let estimate = prompt::estimate_tokens(&prompt::sanitize_transcript(transcript));
    let request = MessagesRequest::single_turn(system, user, estimate, translate.is_some());

    let started = Instant::now();
    let response = send(request).await;
    let elapsed_ms = started.elapsed().as_millis();

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            let reason = api_unusable(FallbackReason::from_error(e), translate);
            tracing::info!(?reason, elapsed_ms, "polish request failed");
            return PolishResult::raw(transcript, Some(reason));
        }
    };

    let usage = Some(response.usage);
    // `end_turn` / `stop_sequence` only. `max_tokens` (truncated), `refusal`,
    // a missing or unknown stop reason are half-finished outputs: raw
    // instead (FR-8, docs/changes/0006).
    let complete = response.is_complete();
    let checked = if complete {
        guard::check(transcript, &response.text(), translate, &opts.glossary)
    } else {
        Err(CoreError::GuardRejected)
    };
    match checked {
        Ok(text) => {
            tracing::info!(
                elapsed_ms,
                input_len = transcript.len(),
                output_len = text.len(),
                translate = translate.is_some(),
                "polish ok"
            );
            PolishResult {
                text,
                fell_back_to_raw: false,
                fallback_reason: None,
                usage,
            }
        }
        Err(e) => {
            tracing::info!(elapsed_ms, complete, "polish output rejected");
            PolishResult {
                usage,
                ..PolishResult::raw(transcript, Some(FallbackReason::from_error(e)))
            }
        }
    }
}

/// With translation requested, "the API is not usable right now" (no key,
/// offline, hard limit) is reported as FR-8's "翻譯需要 API". Other
/// failures keep their own reason.
fn api_unusable(reason: FallbackReason, translate: Option<TargetLang>) -> FallbackReason {
    match reason {
        FallbackReason::NoKey | FallbackReason::Network | FallbackReason::HardLimit
            if translate.is_some() =>
        {
            FallbackReason::TranslationNeedsApi
        }
        r => r,
    }
}

#[cfg(test)]
mod tests;
