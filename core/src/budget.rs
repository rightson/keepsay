//! Token usage and monthly budget (FR-6, PROPOSAL §6.8). Non-secret.
//!
//! Owner: workstream D (docs/changes/0005). The ledger lives in a
//! `budget.json` whose directory the platform layer passes in; this module
//! never decides where App Support is.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::config::{BudgetLimits, PriceTable};

/// Token counts as reported by the API (`usage` in the messages response).
/// Unknown fields such as cache counters are ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// A calendar month. Injected by callers so the monthly reset is testable
/// without a clock (US-M1-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct YearMonth {
    pub year: i32,
    /// 1–12.
    pub month: u8,
}

impl YearMonth {
    /// The UTC month containing `unix_secs`.
    pub fn from_unix_secs(unix_secs: i64) -> Self {
        let _ = unix_secs;
        todo!("M1: budget — civil date from unix time")
    }
}

/// Snapshot for the UI and for the hard-limit check in `polish`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BudgetStatus {
    pub month_usage: Usage,
    pub estimated_usd: f64,
    pub soft_limit_usd: Option<f64>,
    pub hard_limit_usd: Option<f64>,
}

impl BudgetStatus {
    /// At or past the soft limit: warn the user.
    pub fn soft_limited(&self) -> bool {
        self.soft_limit_usd.is_some_and(|s| self.estimated_usd >= s)
    }

    /// At or past the hard limit, the app switches to raw mode.
    pub fn hard_limited(&self) -> bool {
        self.hard_limit_usd.is_some_and(|h| self.estimated_usd >= h)
    }
}

/// The persisted monthly ledger (`budget.json`, with a schema version).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub schema_version: u32,
    pub month: YearMonth,
    pub usage: Usage,
}

impl Budget {
    pub fn new(now: YearMonth) -> Self {
        Self {
            schema_version: crate::config::SCHEMA_VERSION,
            month: now,
            usage: Usage::default(),
        }
    }

    /// Read `path`; a missing file yields a fresh ledger for `now`.
    pub fn load(path: &Path, now: YearMonth) -> Result<Self, CoreError> {
        let _ = (path, now);
        todo!("M1: budget — load")
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let _ = path;
        todo!("M1: budget — save")
    }

    /// Add one response's usage, resetting first if `now` is a new month.
    pub fn record(&mut self, usage: Usage, now: YearMonth) {
        let _ = (usage, now);
        todo!("M1: budget — record")
    }

    pub fn status(
        &self,
        prices: &PriceTable,
        limits: &BudgetLimits,
        now: YearMonth,
    ) -> BudgetStatus {
        let _ = (prices, limits, now);
        todo!("M1: budget — status")
    }
}
