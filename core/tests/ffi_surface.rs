//! INV-2 at the FFI boundary (US-M1-22 ②, docs/changes/0014): text scan of
//! `core/src/ffi.rs`. No exported function may return the key or anything
//! derived from it except `Fingerprint`.
//!
//! The list of exports is pinned: adding one fails this test until its
//! return type is reviewed and listed here.

const SRC: &str = include_str!("../src/ffi.rs");

/// (name, exact return type) of every `#[uniffi::export]` function.
const EXPECTED: &[(&str, &str)] = &[
    ("init", "Result<(), CoreError>"),
    ("set_utc_offset", "Result<(), CoreError>"),
    ("store_key", "Result<Fingerprint, CoreError>"),
    ("has_key", "bool"),
    ("key_fingerprint", "Option<Fingerprint>"),
    ("delete_key", "Result<(), CoreError>"),
    ("verify_key", "Result<(), CoreError>"),
    ("polish", "Result<PolishResult, CoreError>"),
    // Transcript text: user content, not key material.
    ("transcribe", "Result<String, CoreError>"),
    ("budget_status", "Result<BudgetStatus, CoreError>"),
    ("download_model", "Result<(), CoreError>"),
    ("import_model", "Result<(), CoreError>"),
    // Non-secret config.json fields (docs/changes/0019); no key, no proxy.
    ("get_settings", "Result<Settings, CoreError>"),
    ("save_settings", "Result<(), CoreError>"),
    ("list_models", "Result<Vec<ModelStatus>, CoreError>"),
];

/// The fields of the `Settings` record are pinned: it must not grow one
/// that could carry the key, or the proxy URL (which may embed credentials).
#[test]
fn settings_record_fields_are_pinned() {
    let code = code();
    let start = code.find("pub struct Settings {").expect("Settings record");
    let body = &code[start..start + code[start..].find('}').expect("end")];
    let fields: Vec<&str> = body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub "))
        .filter_map(|l| l.split_once(':').map(|(n, _)| n))
        .collect();
    assert_eq!(
        fields,
        [
            "mode",
            "translate_to",
            "stt_model",
            "glossary",
            "input_usd_per_mtok",
            "output_usd_per_mtok",
            "soft_limit_usd",
            "hard_limit_usd",
            "hotkey_polish",
            "hotkey_polish_translate",
        ]
    );
}

/// Return types allowed for anything about the key.
const KEY_RETURNS: &[&str] = &[
    "Result<Fingerprint, CoreError>",
    "Option<Fingerprint>",
    "Result<(), CoreError>",
    "bool",
];

fn code() -> String {
    SRC.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `(name, return type)` for each `#[uniffi::export]` free function.
fn exports(code: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("#[uniffi::export]") {
        rest = &rest[i + "#[uniffi::export]".len()..];
        let item = rest.trim_start();
        let Some(sig) = item.strip_prefix("pub fn ") else {
            // `#[uniffi::export(foreign)] pub trait …` uses another
            // attribute form; anything else here is unexpected.
            panic!("unexpected exported item: {}", &item[..item.len().min(40)]);
        };
        let name: String = sig
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let header = &sig[..sig.find('{').expect("function body")];
        let ret = header
            .rsplit_once("->")
            .map_or("()", |(_, r)| r)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        out.push((name, ret));
    }
    out
}

#[test]
fn exported_functions_are_exactly_the_reviewed_set() {
    let got = exports(&code());
    let want: Vec<(String, String)> = EXPECTED
        .iter()
        .map(|(n, r)| ((*n).to_owned(), (*r).to_owned()))
        .collect();
    let mut got_sorted = got.clone();
    got_sorted.sort();
    let mut want_sorted = want.clone();
    want_sorted.sort();
    assert_eq!(got_sorted, want_sorted, "FFI exports changed: review INV-2");
}

#[test]
fn nothing_key_related_returns_a_string() {
    for (name, ret) in exports(&code()) {
        assert!(!ret.contains("SecretString"), "{name} returns a secret");
        assert!(!ret.contains("Secret"), "{name} returns a secret type");
        if name.contains("key") || name.contains("fingerprint") {
            assert!(
                KEY_RETURNS.contains(&ret.as_str()),
                "{name} returns {ret}; only Fingerprint may leave the core (INV-2)"
            );
        } else if ret.contains("String") {
            assert_eq!(name, "transcribe", "{name} returns a String");
        }
    }
}

#[test]
fn ffi_never_exposes_or_loads_the_key() {
    let code = code();
    for forbidden in [
        "expose_secret",
        ".load()",
        "-> SecretString",
        "Option<SecretString>",
        "Result<SecretString",
    ] {
        assert!(
            !code.contains(forbidden),
            "ffi.rs must not contain {forbidden}"
        );
    }
    // Foreign-implemented traits only receive progress numbers.
    assert_eq!(code.matches("#[uniffi::export(foreign)]").count(), 1);
    assert!(code.contains("fn on_progress(&self, received: u64, total: u64);"));
}

#[test]
fn scanner_parses_signatures() {
    let src = "#[uniffi::export]\npub fn a(x: String) -> Result<String, E> {\n}\n#[uniffi::export]\npub fn b() {\n}\n";
    assert_eq!(
        exports(src),
        vec![
            ("a".to_owned(), "Result<String, E>".to_owned()),
            ("b".to_owned(), "()".to_owned())
        ]
    );
}
