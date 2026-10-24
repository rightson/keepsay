//! `keepsay-cli` against a wiremock API (US-M1-23): `polish --text-stdin`,
//! `key verify`, `key status` through the real `AnthropicClient`.
//!
//! Needs the CLI's `test-endpoint` feature (it lets `KEEPSAY_DEV_API_ENDPOINT`
//! name a loopback base URL) and a debug build (`--dev-mock-vault`). Run with
//! `cargo test -p keepsay-cli --features test-endpoint` or `--all-features`.
//! Only lengths, exit codes, the fingerprint and the absence of the canary
//! are asserted.

#![cfg(all(feature = "test-endpoint", debug_assertions))]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CANARY: &str = "sk-ant-test-CANARY-7f3a0000";
const INPUT: &str = "嗯那個我們明天下午三點要開會，然後要討論一下 Q4 的 roadmap，記得帶筆電";
const POLISHED: &str = "我們明天下午三點要開會，討論 Q4 的 roadmap，記得帶筆電。";

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn server(rt: &tokio::runtime::Runtime, models_status: u16) -> MockServer {
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(models_status)
                    .set_body_json(serde_json::json!({"data": [], "has_more": false})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{"type": "text", "text": POLISHED}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 40, "output_tokens": 20},
            })))
            .mount(&server)
            .await;
        server
    })
}

fn run(endpoint: &str, dir: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_keepsay-cli"))
        .arg("--data-dir")
        .arg(dir)
        .arg("--dev-mock-vault")
        .args(args)
        .env("KEEPSAY_DEV_API_ENDPOINT", endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("CANARY"));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("CANARY"));
    out
}

fn files_without_canary(dir: &Path) -> usize {
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_file() {
            n += 1;
            let bytes = std::fs::read(&p).unwrap();
            assert!(
                !bytes.windows(b"CANARY".len()).any(|w| w == b"CANARY"),
                "canary in {}",
                p.display()
            );
        }
    }
    n
}

#[test]
fn polish_text_stdin_through_the_api() {
    let rt = rt();
    let api = server(&rt, 200);
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        &api.uri(),
        dir.path(),
        &["polish", "--text-stdin"],
        &format!("{CANARY}\n{INPUT}\n"),
    );
    assert!(out.status.success());
    assert_eq!(
        out.stdout.len(),
        POLISHED.len() + 1,
        "polished text + newline"
    );
    // Usage is recorded; the ledger is the only file and holds no key.
    assert_eq!(files_without_canary(dir.path()), 1);
    assert!(dir.path().join("budget.json").is_file());

    let reqs = rt.block_on(api.received_requests()).unwrap();
    assert_eq!(reqs.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
    assert!(body.get("tools").is_none() && body.get("temperature").is_none());
    // Presence only: tests never compare the key itself.
    assert!(reqs[0].headers.contains_key("x-api-key"));

    // `budget` reports the recorded tokens.
    let out = run(&api.uri(), dir.path(), &["budget"], "");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("40 input / 20 output"));
}

#[test]
fn translate_falls_back_to_raw_when_output_is_untranslated() {
    // The mock answers in Chinese, so the translation guard rejects it and
    // the CLI prints the raw input instead (FR-8).
    let rt = rt();
    let api = server(&rt, 200);
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        &api.uri(),
        dir.path(),
        &["polish", "--text-stdin", "--translate", "en"],
        &format!("{CANARY}\n{INPUT}\n"),
    );
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), INPUT.len() + 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("GuardRejected"));
}

#[test]
fn key_verify_and_status_show_only_the_fingerprint() {
    let rt = rt();
    let api = server(&rt, 200);
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        &api.uri(),
        dir.path(),
        &["key", "verify"],
        &format!("{CANARY}\n"),
    );
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("…0000") && err.contains("valid"));
    assert!(out.stdout.is_empty());

    let out = run(&api.uri(), dir.path(), &["key", "status"], "");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no key stored"));
    assert_eq!(files_without_canary(dir.path()), 0);
}

#[test]
fn unauthorized_key_fails_verify_and_polish_falls_back() {
    let rt = rt();
    let api = server(&rt, 401);
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        &api.uri(),
        dir.path(),
        &["key", "verify"],
        &format!("{CANARY}\n"),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid"));

    // A wrong endpoint (not loopback) is refused, and polish still prints the
    // raw transcript.
    let out = run(
        "https://api.anthropic.com.evil.example",
        dir.path(),
        &["polish", "--text-stdin"],
        &format!("{CANARY}\n{INPUT}\n"),
    );
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), INPUT.len() + 1);
    assert_eq!(files_without_canary(dir.path()), 0);
}
