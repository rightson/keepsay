//! Black-box tests of the `keepsay-cli` binary.
//!
//! Only lengths, exit codes and the absence of the canary are asserted,
//! never transcript or model content (CLAUDE.md). Nothing here touches the
//! network or downloads a model.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const CANARY: &str = "sk-ant-test-CANARY-7f3a0000";

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_keepsay-cli"))
}

fn run_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = bin()
        .args(args)
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
    child.wait_with_output().expect("wait")
}

fn files_in(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
}

fn no_canary(out: &Output) {
    assert!(!String::from_utf8_lossy(&out.stdout).contains("CANARY"));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("CANARY"));
}

#[test]
fn version_and_help_go_to_stderr() {
    for flag in ["--version", "--help"] {
        let out = bin().arg(flag).output().unwrap();
        assert!(out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}

#[test]
fn usage_errors_exit_2() {
    let out = bin().arg("frobnicate").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn key_on_argv_is_refused_and_not_echoed() {
    for args in [
        vec!["key", "set", CANARY],
        vec![CANARY],
        vec!["polish", "--text-stdin", CANARY],
        vec!["--data-dir", "/nonexistent", "key", "verify", CANARY],
    ] {
        let out = bin().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        no_canary(&out);
    }
}

#[test]
fn raw_mode_passes_text_through_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    let input = "0123456789 abcdef";
    let out = run_with_stdin(
        &["--data-dir", d, "polish", "--text-stdin", "--mode", "raw"],
        input,
    );
    assert!(out.status.success());
    // Product output is the text plus a newline; compare lengths only.
    assert_eq!(out.stdout.len(), input.len() + 1);
    assert!(out.stderr.is_empty());
    assert_eq!(files_in(dir.path()), 0, "INV-5: no files written");
}

#[test]
fn missing_wav_is_an_error_without_paths_or_writes() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    let out = bin()
        .args([
            "--data-dir",
            d,
            "transcribe",
            "/nonexistent/secret-name.wav",
            "--model",
            "/nonexistent/m.bin",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("secret-name"));
    assert_eq!(files_in(dir.path()), 0);
}

#[cfg(debug_assertions)]
#[test]
fn dev_mock_vault_rejects_bad_key_without_echo() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    let out = run_with_stdin(
        &["--data-dir", d, "--dev-mock-vault", "key", "verify"],
        "not-a-key-CANARY\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("WARNING"));
    no_canary(&out);
    assert_eq!(files_in(dir.path()), 0);
}

/// 16-bit PCM mono WAV with half a second of a quiet tone at 22.05 kHz,
/// so the resampler runs too.
fn tone_wav() -> Vec<u8> {
    let rate = 22_050u32;
    let data: Vec<u8> = (0..rate / 2)
        .flat_map(|i| {
            let x = 0.1 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin();
            ((x * 32_767.0) as i16).to_le_bytes()
        })
        .collect();
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data.len() as u32).to_le_bytes());
    b.extend_from_slice(&data);
    b
}

/// End to end through whisper. Needs a local ggml model; tests never
/// download one. Set `KEEPSAY_TEST_WHISPER_MODEL=/path/to/ggml-tiny.bin`.
#[test]
fn transcribe_generated_wav_when_a_model_is_available() {
    if !cfg!(feature = "whisper") {
        eprintln!("skipped: built without the `whisper` feature");
        return;
    }
    let Some(model) = std::env::var_os("KEEPSAY_TEST_WHISPER_MODEL") else {
        eprintln!("skipped: set KEEPSAY_TEST_WHISPER_MODEL to a ggml model file to run");
        return;
    };
    let audio_dir = tempfile::tempdir().unwrap();
    let wav = audio_dir.path().join("tone.wav");
    std::fs::write(&wav, tone_wav()).unwrap();
    let data_dir = tempfile::tempdir().unwrap();

    let out = bin()
        .arg("--data-dir")
        .arg(data_dir.path())
        .arg("transcribe")
        .arg(&wav)
        .arg("--model")
        .arg(&model)
        .output()
        .unwrap();
    assert!(out.status.success(), "transcribe failed");
    // Content is irrelevant for a tone; it is a single line of output.
    assert!(out.stdout.ends_with(b"\n"));
    assert!(out.stdout.len() < 4_096);
    assert_eq!(files_in(data_dir.path()), 0, "INV-5: no files written");
    assert_eq!(files_in(audio_dir.path()), 1);
}
