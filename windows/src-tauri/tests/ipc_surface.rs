//! INV-2 and PROPOSAL §7.3 at the IPC boundary (US-M3-02, US-M3-11).
//!
//! Text scan of the app's sources and config, so it runs on every OS (the
//! app itself only builds on Windows):
//! - the command list in `command_list.rs`, the `#[tauri::command]`
//!   handlers, `generate_handler!` and the capabilities all agree;
//! - every command returns `Result<T, IpcError>` with `T` on a pinned
//!   allowlist of types that cannot hold the key; adding a command fails
//!   here until its return type is reviewed and listed;
//! - `KeyView` carries only `has_key` and the fingerprint;
//! - commands never touch `SecretString`, `expose_secret` or the vault's
//!   `load`;
//! - capabilities grant only keepsay's own commands (no `core:*`, no
//!   plugin permissions), and the overlay only its state;
//! - the CSP, devtools and remote-URL settings are as in §7.3.

use std::collections::BTreeSet;

include!("../src/command_list.rs");

const COMMANDS_RS: &str = include_str!("../src/commands.rs");
const IPC_RS: &str = include_str!("../src/ipc.rs");
const APP_RS: &str = include_str!("../src/app.rs");
const CONF: &str = include_str!("../tauri.conf.json");
const CAP_SETTINGS: &str = include_str!("../capabilities/settings.json");
const CAP_OVERLAY: &str = include_str!("../capabilities/overlay.json");
const MANIFEST: &str = include_str!("../Cargo.toml");

/// (command, exact return type). Reviewed: none of these types can hold
/// the key; key state is `KeyView` (fingerprint only).
const EXPECTED: &[(&str, &str)] = &[
    ("get_settings", "Result<SettingsView, IpcError>"),
    ("save_settings", "Result<HotkeyStatusView, IpcError>"),
    ("key_status", "Result<KeyView, IpcError>"),
    ("enter_key", "Result<KeyView, IpcError>"),
    ("delete_key", "Result<KeyView, IpcError>"),
    ("budget_status", "Result<BudgetView, IpcError>"),
    ("list_models", "Result<Vec<ModelView>, IpcError>"),
    ("download_model", "Result<(), IpcError>"),
    ("verify_model", "Result<ModelView, IpcError>"),
    ("model_progress", "Result<DownloadView, IpcError>"),
    ("hotkey_status", "Result<HotkeyStatusView, IpcError>"),
    ("open_mic_settings", "Result<(), IpcError>"),
    ("overlay_state", "Result<OverlayView, IpcError>"),
];

fn code(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// (name, return type, parameter list) of every `#[tauri::command]`.
fn handlers() -> Vec<(String, String, String)> {
    let src = code(COMMANDS_RS);
    let mut out = Vec::new();
    for chunk in src.split("#[tauri::command]").skip(1) {
        let sig_end = chunk.find('{').expect("fn body");
        let sig = chunk[..sig_end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let after_fn = sig.split("fn ").nth(1).expect("fn keyword");
        let name = after_fn.split('(').next().unwrap().trim().to_owned();
        let params =
            after_fn[after_fn.find('(').unwrap() + 1..after_fn.rfind(')').unwrap()].to_owned();
        let ret = sig
            .split("->")
            .nth(1)
            .expect("return type")
            .trim()
            .to_owned();
        out.push((name, ret, params));
    }
    out
}

fn generate_handler_list() -> BTreeSet<String> {
    let src = code(COMMANDS_RS);
    let start =
        src.find("generate_handler![").expect("generate_handler!") + "generate_handler![".len();
    let end = start + src[start..].find(']').unwrap();
    src[start..end]
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).expect("valid JSON")
}

fn permissions(cap: &str) -> Vec<String> {
    json(cap)["permissions"]
        .as_array()
        .expect("permissions array")
        .iter()
        .map(|p| p.as_str().expect("plain permission string").to_owned())
        .collect()
}

#[test]
fn command_lists_agree() {
    let listed: BTreeSet<String> = COMMANDS.iter().map(|s| s.to_string()).collect();
    assert_eq!(listed.len(), COMMANDS.len(), "duplicate in command_list.rs");
    let handlers: BTreeSet<String> = handlers().into_iter().map(|(n, _, _)| n).collect();
    let expected: BTreeSet<String> = EXPECTED.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(handlers, listed, "#[tauri::command] fns vs command_list.rs");
    assert_eq!(
        generate_handler_list(),
        listed,
        "generate_handler! vs command_list.rs"
    );
    assert_eq!(
        expected, listed,
        "update EXPECTED after reviewing the new command"
    );
}

#[test]
fn return_types_are_pinned() {
    for (name, ret, _) in handlers() {
        let want = EXPECTED
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("command {name} not reviewed"));
        assert_eq!(ret, want, "return type of {name}");
    }
}

#[test]
fn parameters_never_carry_the_key() {
    for (name, _, params) in handlers() {
        for bad in ["SecretString", "SecretBox", "key:", "api_key"] {
            assert!(!params.contains(bad), "{name} takes {bad}");
        }
    }
}

#[test]
fn commands_never_handle_key_material() {
    let src = code(COMMANDS_RS);
    for bad in [
        "expose_secret",
        "SecretString",
        ".load()",
        "KeyVault::load(",
        "ExposeSecret",
    ] {
        assert!(!src.contains(bad), "commands.rs mentions {bad}");
    }
    // The dialog result goes straight into core's store flow.
    assert!(src.contains("vault::store_key("));
}

#[test]
fn key_view_is_fingerprint_only() {
    let src = code(IPC_RS);
    let start = src.find("pub struct KeyView {").expect("KeyView");
    let body = &src[start..start + src[start..].find('}').unwrap()];
    let fields: Vec<&str> = body
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| l.starts_with("pub "))
        .collect();
    assert_eq!(
        fields,
        vec!["pub has_key: bool,", "pub fingerprint: Option<String>,"]
    );
    // The view is built from vault::stored_fingerprint, never from load.
    assert!(code(APP_RS).contains("vault::stored_fingerprint("));
}

#[test]
fn capabilities_grant_only_own_commands() {
    let listed: BTreeSet<String> = COMMANDS.iter().map(|s| s.to_string()).collect();
    let mut granted = BTreeSet::new();
    for (cap, window) in [(CAP_SETTINGS, "settings"), (CAP_OVERLAY, "overlay")] {
        let v = json(cap);
        assert_eq!(
            v["local"],
            serde_json::Value::Bool(true),
            "{window}: local only"
        );
        assert!(v.get("remote").is_none(), "{window}: no remote URLs");
        assert_eq!(v["windows"], serde_json::json!([window]));
        for p in permissions(cap) {
            let cmd = p
                .strip_prefix("allow-")
                .unwrap_or_else(|| panic!("{window}: {p} is not an app command permission"))
                .replace('-', "_");
            assert!(
                !p.contains(':'),
                "{window}: {p} is a core/plugin permission"
            );
            assert!(listed.contains(&cmd), "{window}: {p} has no command");
            granted.insert(cmd);
        }
    }
    assert_eq!(granted, listed, "every command is granted somewhere");
    assert_eq!(
        permissions(CAP_OVERLAY),
        vec!["allow-overlay-state".to_owned()]
    );
    assert!(!permissions(CAP_SETTINGS).contains(&"allow-overlay-state".to_owned()));
}

#[test]
fn webview_hardening() {
    let conf = json(CONF);
    let sec = &conf["app"]["security"];
    assert_eq!(
        sec["csp"],
        "default-src 'self'; connect-src ipc: http://ipc.localhost"
    );
    assert!(sec.get("devCsp").is_none());
    assert!(sec.get("dangerousDisableAssetCspModification").is_none());
    assert_eq!(sec["freezePrototype"], true);
    assert_eq!(
        sec["capabilities"],
        serde_json::json!(["settings", "overlay"])
    );
    assert_eq!(conf["build"]["frontendDist"], "../ui");
    assert!(conf["build"].get("devUrl").is_none(), "no dev server URL");
    // Windows are built in Rust from WebviewUrl::App only.
    assert_eq!(conf["app"]["windows"], serde_json::json!([]));
    // The only URL in the config is the IPC origin inside the CSP.
    assert_eq!(CONF.matches("http://").count(), 1);
    assert!(!CONF.contains("https://"));
    let app = code(APP_RS);
    assert!(!app.contains("WebviewUrl::External"));
    assert!(!app.contains(".devtools(true)"));
    assert_eq!(app.matches(".devtools(cfg!(debug_assertions))").count(), 2);

    // tauri features: no devtools in release, no extra HTTP client, no
    // remote-asset or isolation escape hatches.
    let tauri_line = MANIFEST
        .lines()
        .find(|l| l.starts_with("tauri = "))
        .expect("tauri dependency");
    for bad in [
        "devtools",
        "native-tls",
        "rustls-tls",
        "protocol-asset",
        "dynamic-acl",
    ] {
        assert!(!tauri_line.contains(bad), "tauri feature {bad}");
    }
    for bad in [
        "tauri-plugin-shell",
        "tauri-plugin-fs",
        "tauri-plugin-http",
        "tauri-plugin-updater",
        "tauri-plugin-dialog",
    ] {
        assert!(!MANIFEST.contains(bad), "{bad} must not be a dependency");
    }
}
