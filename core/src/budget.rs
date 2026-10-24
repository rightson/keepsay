//! Token usage and monthly budget (FR-6, PROPOSAL §6.8). Non-secret.
//!
//! Owner: workstream D (docs/changes/0005, 0009). The ledger lives in a
//! `budget.json` whose path the platform layer passes in; this module never
//! decides where App Support is.
//!
//! The month is injected as a [`YearMonth`]. The core has no time-zone
//! database, so "resets on the 1st, local time" works like this: the platform
//! layer computes the local month itself, or passes its current UTC offset to
//! [`YearMonth::from_unix_secs_with_offset`].
//!
//! Rollover only moves forward: if the clock goes back to an earlier month,
//! usage keeps accumulating in the stored month instead of resetting, so a
//! clock change can never wipe the counter that drives the hard limit.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::config::{BudgetLimits, PriceTable, write_atomic};

/// `budget.json` schema version. Independent of the settings schema.
pub const BUDGET_SCHEMA_VERSION: u32 = 1;

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
        Self::from_unix_secs_with_offset(unix_secs, 0)
    }

    /// The month containing `unix_secs` in a zone `utc_offset_secs` east of
    /// UTC (e.g. +28800 for Asia/Taipei). The platform layer supplies the
    /// offset; this is how the reset happens on the local 1st.
    pub fn from_unix_secs_with_offset(unix_secs: i64, utc_offset_secs: i32) -> Self {
        let local = unix_secs.saturating_add(i64::from(utc_offset_secs));
        let (year, month, _) = civil_from_days(local.div_euclid(86_400));
        Self { year, month }
    }

    pub fn is_valid(self) -> bool {
        (1..=12).contains(&self.month)
    }
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    let year = i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX });
    // m in 1..=12 and d in 1..=31 by construction.
    (year, m as u8, d as u8)
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
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub schema_version: u32,
    pub month: YearMonth,
    pub usage: Usage,
}

impl Budget {
    pub fn new(now: YearMonth) -> Self {
        Self {
            schema_version: BUDGET_SCHEMA_VERSION,
            month: now,
            usage: Usage::default(),
        }
    }

    /// Read `path`; a missing file yields a fresh ledger for `now`. A ledger
    /// from an earlier month is rolled over to `now`.
    ///
    /// A corrupt file or an unknown schema version is `CoreError::Config`,
    /// not a silent reset: the caller decides (and should keep treating the
    /// month as at the hard limit rather than as unspent).
    pub fn load(path: &Path, now: YearMonth) -> Result<Self, CoreError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::new(now)),
            Err(e) => return Err(e.into()),
        };
        let mut b: Budget = serde_json::from_slice(&bytes).map_err(|_| CoreError::Config)?;
        if b.schema_version != BUDGET_SCHEMA_VERSION || !b.month.is_valid() {
            return Err(CoreError::Config);
        }
        b.roll_over(now);
        Ok(b)
    }

    /// Write via a temp file in the same directory and rename.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let json = serde_json::to_vec_pretty(self).map_err(|_| CoreError::Config)?;
        write_atomic(path, &json)
    }

    /// Add one response's usage, resetting first if `now` is a new month.
    pub fn record(&mut self, usage: Usage, now: YearMonth) {
        self.roll_over(now);
        self.usage.input_tokens = self.usage.input_tokens.saturating_add(usage.input_tokens);
        self.usage.output_tokens = self.usage.output_tokens.saturating_add(usage.output_tokens);
    }

    pub fn status(
        &self,
        prices: &PriceTable,
        limits: &BudgetLimits,
        now: YearMonth,
    ) -> BudgetStatus {
        let month_usage = if now > self.month {
            Usage::default()
        } else {
            self.usage
        };
        BudgetStatus {
            month_usage,
            estimated_usd: estimate_usd(month_usage, prices),
            soft_limit_usd: limits.soft_usd,
            hard_limit_usd: limits.hard_usd,
        }
    }

    /// Reset when `now` is a later month. Never moves backwards.
    fn roll_over(&mut self, now: YearMonth) {
        if now > self.month {
            self.month = now;
            self.usage = Usage::default();
        }
    }
}

/// Estimated cost in USD of `usage` at `prices` (USD per million tokens).
pub fn estimate_usd(usage: Usage, prices: &PriceTable) -> f64 {
    (usage.input_tokens as f64 * prices.input_usd_per_mtok
        + usage.output_tokens as f64 * prices.output_usd_per_mtok)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ym(year: i32, month: u8) -> YearMonth {
        YearMonth { year, month }
    }

    #[test]
    fn civil_dates() {
        assert_eq!(YearMonth::from_unix_secs(0), ym(1970, 1));
        // 2026-10-24T00:00:00Z
        assert_eq!(YearMonth::from_unix_secs(1_792_800_000), ym(2026, 10));
        // 2024-02-29T23:59:59Z, leap day.
        assert_eq!(civil_from_days(1_709_251_199 / 86_400), (2024, 2, 29));
        // 2024-03-01T00:00:00Z
        assert_eq!(YearMonth::from_unix_secs(1_709_251_200), ym(2024, 3));
        // 1969-12-31T23:59:59Z
        assert_eq!(YearMonth::from_unix_secs(-1), ym(1969, 12));
    }

    #[test]
    fn local_offset_moves_the_month_boundary() {
        // 2026-10-31T16:00:00Z is 2026-11-01T00:00 in Taipei (UTC+8).
        let t = 1_793_462_400;
        assert_eq!(YearMonth::from_unix_secs(t), ym(2026, 10));
        assert_eq!(
            YearMonth::from_unix_secs_with_offset(t, 8 * 3600),
            ym(2026, 11)
        );
        assert_eq!(
            YearMonth::from_unix_secs_with_offset(t - 1, 8 * 3600),
            ym(2026, 10)
        );
        // West of UTC: 2026-11-01T03:00Z is still October in New York (UTC-4).
        let t2 = 1_793_502_000;
        assert_eq!(YearMonth::from_unix_secs(t2), ym(2026, 11));
        assert_eq!(
            YearMonth::from_unix_secs_with_offset(t2, -4 * 3600),
            ym(2026, 10)
        );
    }

    #[test]
    fn estimate() {
        let p = PriceTable {
            input_usd_per_mtok: 1.0,
            output_usd_per_mtok: 5.0,
        };
        let u = Usage {
            input_tokens: 2_000_000,
            output_tokens: 1_000_000,
        };
        assert!((estimate_usd(u, &p) - 7.0).abs() < 1e-9);
    }
}
