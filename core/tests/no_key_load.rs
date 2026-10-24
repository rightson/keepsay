//! INV-2: outside the vault and the one client that sends the key, nothing
//! reads the key out of the vault (docs/changes/0014).
//!
//! Text scan over every non-test Rust source file of the workspace (like
//! `downloader.rs` and `no_disk_writes.rs`). `KeyVault::load` takes no
//! arguments, so a call is `.load()` or `KeyVault::load(`. Other `load`
//! functions (`Config::load(path)`, `Budget::load(path, now)`,
//! `WhisperEngine::load(path)`, atomics' `.load(ordering)`) take arguments
//! and do not match.
//!
//! Allowed:
//! - `core/src/api/client.rs`: `AnthropicClient` attaches the key (INV-3);
//! - `core/src/vault/**`: the backends and the default `key_fingerprint`;
//! - test code: `tests/` directories, `tests.rs` files, and the trailing
//!   `#[cfg(test)] mod tests` block of a source file.
//!
//! Callers that need the fingerprint use `vault::stored_fingerprint`; callers
//! that need to verify a stored key use `AnthropicClient::verify_stored_key`.

use std::fs;
use std::path::{Path, PathBuf};

/// Source roots scanned, relative to the workspace root. Platform crates
/// (`windows/src-tauri`, …) are scanned too once they exist.
const ROOTS: &[&str] = &["core/src", "cli/src", "windows"];

const ALLOWED: &[&str] = &["core/src/api/client.rs"];
const ALLOWED_DIRS: &[&str] = &["core/src/vault/"];

const FORBIDDEN: &[&str] = &[".load()", "KeyVault::load("];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if matches!(name, "target" | "tests" | "node_modules") || name.starts_with('.') {
                continue;
            }
            rust_files(&path, out);
        } else if name.ends_with(".rs") && name != "tests.rs" {
            out.push(path);
        }
    }
}

/// Non-comment lines up to the trailing `#[cfg(test)] mod …` block.
fn production_code(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() == "#[cfg(test)]"
            && lines[i + 1..]
                .iter()
                .find(|n| !n.trim().is_empty())
                .is_some_and(|n| n.trim_start().starts_with("mod "))
        {
            end = i;
            break;
        }
    }
    lines[..end]
        .iter()
        .filter(|l| !l.trim_start().starts_with("//"))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn only_the_client_and_the_vault_load_the_key() {
    let root = workspace_root();
    let mut files = Vec::new();
    for r in ROOTS {
        rust_files(&root.join(r), &mut files);
    }
    assert!(
        files.len() > 10,
        "scan found too few files: {}",
        files.len()
    );

    let mut offenders = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .expect("under root")
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED.contains(&rel.as_str()) || ALLOWED_DIRS.iter().any(|d| rel.starts_with(d)) {
            continue;
        }
        let code = production_code(&fs::read_to_string(path).expect("read source"));
        for f in FORBIDDEN {
            if code.contains(f) {
                offenders.push(format!("{rel}: {f}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "KeyVault::load called outside api/client.rs and vault/ (INV-2): {offenders:?}"
    );
}

#[test]
fn scanner_sees_calls_and_skips_tests() {
    let src = "fn f(v: &dyn KeyVault) {\n    let k = v.load();\n}\n";
    assert!(production_code(src).contains(".load()"));
    let src = "fn f() {}\n// v.load()\n#[cfg(test)]\nmod tests {\n    fn t() { v.load(); }\n}\n";
    assert!(!production_code(src).contains(".load()"));
    // A cfg(test) item that is not the test module does not end the scan.
    let src = "#[cfg(test)]\nfn helper() {}\nfn g() { v.load(); }\n";
    assert!(production_code(src).contains(".load()"));
}
