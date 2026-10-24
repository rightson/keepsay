//! INV-1 / INV-2 / INV-6 canary: run every path with a fake key, then scan
//! logs, config dir, temp dir and every file written; the canary must appear
//! zero times.

#[allow(dead_code)]
const CANARY: &str = "sk-ant-test-CANARY-7f3a9c1d";

#[test]
#[ignore = "M1: needs vault, client, polish, budget and a tracing capture"]
fn canary_never_leaks() {}
