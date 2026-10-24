//! US-M1-14: monthly token ledger, price estimate, limits, rollover on the
//! 1st with an injected clock, and atomic persistence.

use std::fs;

use keepsay_core::CoreError;
use keepsay_core::Usage;
use keepsay_core::budget::{BUDGET_SCHEMA_VERSION, Budget, YearMonth};
use keepsay_core::config::{BudgetLimits, PriceTable};

fn ym(year: i32, month: u8) -> YearMonth {
    YearMonth { year, month }
}

fn usage(input_tokens: u64, output_tokens: u64) -> Usage {
    Usage {
        input_tokens,
        output_tokens,
    }
}

const PRICES: PriceTable = PriceTable {
    input_usd_per_mtok: 1.0,
    output_usd_per_mtok: 5.0,
};

#[test]
fn records_accumulate_within_a_month() {
    let oct = ym(2026, 10);
    let mut b = Budget::new(oct);
    b.record(usage(100, 10), oct);
    b.record(usage(50, 5), oct);
    assert_eq!(b.usage, usage(150, 15));
}

#[test]
fn new_month_resets_on_record_and_in_status() {
    let oct = ym(2026, 10);
    let nov = ym(2026, 11);
    let mut b = Budget::new(oct);
    b.record(usage(1_000_000, 0), oct);

    // Status for November sees an empty month even before anything is recorded.
    let s = b.status(&PRICES, &BudgetLimits::default(), nov);
    assert_eq!(s.month_usage, Usage::default());
    assert_eq!(s.estimated_usd, 0.0);

    b.record(usage(10, 1), nov);
    assert_eq!(b.month, nov);
    assert_eq!(b.usage, usage(10, 1));
}

#[test]
fn year_boundary_rolls_over() {
    let mut b = Budget::new(ym(2026, 12));
    b.record(usage(5, 5), ym(2026, 12));
    b.record(usage(1, 1), ym(2027, 1));
    assert_eq!((b.month, b.usage), (ym(2027, 1), usage(1, 1)));
}

#[test]
fn clock_going_back_does_not_reset() {
    let mut b = Budget::new(ym(2026, 11));
    b.record(usage(7, 7), ym(2026, 11));
    b.record(usage(1, 1), ym(2026, 10));
    assert_eq!((b.month, b.usage), (ym(2026, 11), usage(8, 8)));
    let s = b.status(&PRICES, &BudgetLimits::default(), ym(2026, 10));
    assert_eq!(s.month_usage, usage(8, 8));
}

#[test]
fn local_first_of_month_resets() {
    // 2026-10-31T16:00:00Z = 2026-11-01T00:00 in Taipei.
    let t = 1_793_462_400;
    let tz = 8 * 3600;
    let before = YearMonth::from_unix_secs_with_offset(t - 1, tz);
    let after = YearMonth::from_unix_secs_with_offset(t, tz);
    let mut b = Budget::new(before);
    b.record(usage(9, 9), before);
    b.record(usage(1, 0), after);
    assert_eq!(b.usage, usage(1, 0));
}

#[test]
fn soft_and_hard_limits() {
    let oct = ym(2026, 10);
    let limits = BudgetLimits {
        soft_usd: Some(1.0),
        hard_usd: Some(2.0),
    };
    let mut b = Budget::new(oct);

    b.record(usage(500_000, 0), oct); // $0.50
    let s = b.status(&PRICES, &limits, oct);
    assert!(!s.soft_limited() && !s.hard_limited());

    b.record(usage(0, 100_000), oct); // +$0.50 = $1.00
    let s = b.status(&PRICES, &limits, oct);
    assert!((s.estimated_usd - 1.0).abs() < 1e-9);
    assert!(s.soft_limited() && !s.hard_limited());

    b.record(usage(1_000_000, 0), oct); // $2.00
    let s = b.status(&PRICES, &limits, oct);
    assert!(s.soft_limited() && s.hard_limited());
    assert_eq!(s.soft_limit_usd, Some(1.0));
    assert_eq!(s.hard_limit_usd, Some(2.0));

    let none = BudgetLimits {
        soft_usd: None,
        hard_usd: None,
    };
    let s = b.status(&PRICES, &none, oct);
    assert!(!s.soft_limited() && !s.hard_limited());
}

#[test]
fn saturates_instead_of_overflowing() {
    let oct = ym(2026, 10);
    let mut b = Budget::new(oct);
    b.record(usage(u64::MAX, u64::MAX), oct);
    b.record(usage(1, 1), oct);
    assert_eq!(b.usage, usage(u64::MAX, u64::MAX));
}

#[test]
fn save_and_load_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nested").join("budget.json");
    let oct = ym(2026, 10);

    assert_eq!(Budget::load(&path, oct).unwrap(), Budget::new(oct));

    let mut b = Budget::new(oct);
    b.record(usage(123, 45), oct);
    b.save(&path).unwrap();
    assert_eq!(Budget::load(&path, oct).unwrap(), b);

    // Only budget.json remains: no temp files.
    let names: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("budget.json")]);

    let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(json["schema_version"], BUDGET_SCHEMA_VERSION);

    // Loading in a later month rolls over.
    let loaded = Budget::load(&path, ym(2026, 11)).unwrap();
    assert_eq!(loaded.usage, Usage::default());
    assert_eq!(loaded.month, ym(2026, 11));
}

#[test]
fn bad_files_are_refused_not_reset() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("budget.json");
    let oct = ym(2026, 10);
    for bad in [
        "not json",
        r#"{"schema_version":99,"month":{"year":2026,"month":10},"usage":{"input_tokens":0,"output_tokens":0}}"#,
        r#"{"schema_version":1,"month":{"year":2026,"month":13},"usage":{"input_tokens":0,"output_tokens":0}}"#,
        r#"{"schema_version":1,"month":{"year":2026,"month":10},"usage":{"input_tokens":0,"output_tokens":0},"extra":1}"#,
    ] {
        fs::write(&path, bad).unwrap();
        assert_eq!(Budget::load(&path, oct), Err(CoreError::Config));
    }
}
