//! US-M1-15: non-secret settings JSON with a schema version, atomic save,
//! unknown-version handling, and no key or glossary leakage.

use std::fs;

use keepsay_core::config::{
    BudgetLimits, Config, DEFAULT_STT_MODEL, Hotkeys, MACOS_FALLBACK_HOTKEYS, MACOS_HOTKEYS,
    PriceTable, SCHEMA_VERSION, WINDOWS_HOTKEYS,
};
use keepsay_core::{CoreError, Mode, TargetLang};

/// A glossary term that must never show up in `Debug` output. It is a
/// synthetic canary, not user data.
const GLOSSARY_CANARY: &str = "GLOSSARY-CANARY-5d1e";

fn sample() -> Config {
    Config {
        mode: Mode::Formal,
        translate_to: TargetLang::Ja,
        stt_model: Some("whisper-large-v3-turbo".into()),
        glossary: vec![GLOSSARY_CANARY.into(), "keepsay".into()],
        hotkeys: Hotkeys {
            polish: Some("Ctrl+Alt+Space".into()),
            polish_translate: None,
        },
        prices: PriceTable {
            input_usd_per_mtok: 0.2,
            output_usd_per_mtok: 1.0,
        },
        budget: BudgetLimits {
            soft_usd: Some(1.0),
            hard_usd: None,
        },
        proxy: Some("http://proxy.invalid:3128".into()),
        ..Config::default()
    }
}

#[test]
fn missing_file_is_default() {
    let tmp = tempfile::tempdir().unwrap();
    let c = Config::load(&tmp.path().join("config.json")).unwrap();
    assert_eq!(c, Config::default());
    assert_eq!(c.schema_version, SCHEMA_VERSION);
    assert_eq!(c.mode, Mode::Polish);
    assert_eq!(c.translate_to, TargetLang::En);
    assert_eq!(c.stt_model.as_deref(), Some(DEFAULT_STT_MODEL));
    assert_eq!(c.hotkeys, Hotkeys::default());
    assert!(c.glossary.is_empty());
    assert!(c.proxy.is_none());
}

#[test]
fn round_trip_is_atomic_and_versioned() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("app").join("config.json");
    let c = sample();
    c.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), c);

    // Overwrite works and leaves no temp files.
    Config::default().save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), Config::default());
    let names: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("config.json")]);

    let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(json["schema_version"], SCHEMA_VERSION);
    assert_eq!(json["mode"], "polish");
    assert_eq!(json["translate_to"], "en");
}

#[test]
fn file_never_contains_key_material() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    sample().save(&path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    for needle in ["sk-ant", "api_key", "x-api-key", "CANARY-7f3a"] {
        assert!(
            !text.contains(needle),
            "config.json must not contain {needle}"
        );
    }
}

#[test]
fn debug_hides_glossary_and_proxy() {
    let dbg = format!("{:?}", sample());
    assert!(!dbg.contains(GLOSSARY_CANARY));
    assert!(!dbg.contains("proxy.invalid"));
    assert!(dbg.contains("glossary_len: 2"));
}

#[test]
fn missing_fields_take_defaults() {
    let c = Config::from_json(br#"{"schema_version":1,"mode":"light"}"#).unwrap();
    assert_eq!(c.mode, Mode::Light);
    assert_eq!(c.prices, PriceTable::default());
    assert_eq!(c.budget, BudgetLimits::default());
    let c =
        Config::from_json(br#"{"schema_version":1,"prices":{"input_usd_per_mtok":2.0}}"#).unwrap();
    assert_eq!(c.prices.input_usd_per_mtok, 2.0);
    assert_eq!(
        c.prices.output_usd_per_mtok,
        PriceTable::default().output_usd_per_mtok
    );
}

#[test]
fn unknown_versions_and_fields_are_refused() {
    for bad in [
        &br#"{"mode":"polish"}"#[..],
        br#"{"schema_version":0}"#,
        br#"{"schema_version":2}"#,
        br#"{"schema_version":"1"}"#,
        br#"{"schema_version":1,"api_key":"x"}"#,
        br#"{"schema_version":1,"hotkeys":{"polish":null,"other":"x"}}"#,
        br#"{"schema_version":1,"mode":"shout"}"#,
        br#"{"schema_version":1,"translate_to":"fr"}"#,
        br#"{"schema_version":1,"prices":{"input_usd_per_mtok":-1.0}}"#,
        br#"{"schema_version":1,"budget":{"hard_usd":-5.0}}"#,
        br#"not json"#,
    ] {
        assert_eq!(Config::from_json(bad), Err(CoreError::Config));
    }
}

#[test]
fn newer_version_on_disk_is_left_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    let newer = br#"{"schema_version":2,"future_field":true}"#;
    fs::write(&path, newer).unwrap();
    assert_eq!(Config::load(&path), Err(CoreError::Config));
    assert_eq!(fs::read(&path).unwrap(), newer);
}

#[test]
fn model_outside_allowlist_is_refused() {
    assert_eq!(
        Config::from_json(br#"{"schema_version":1,"model":"some-other-model"}"#),
        Err(CoreError::ModelNotAllowed)
    );
    let c = Config {
        model: "some-other-model".into(),
        ..Config::default()
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    assert_eq!(c.save(&path), Err(CoreError::ModelNotAllowed));
    assert!(!path.exists());
}

#[test]
fn hotkey_defaults_follow_0004() {
    let h = Hotkeys::default();
    assert_eq!(
        h.resolve(WINDOWS_HOTKEYS),
        ("Alt+Shift+Z".to_owned(), "Alt+Shift+X".to_owned())
    );
    assert_eq!(
        h.resolve(MACOS_HOTKEYS),
        ("Fn+Z".to_owned(), "Fn+Shift+Z".to_owned())
    );
    assert_eq!(
        h.resolve(MACOS_FALLBACK_HOTKEYS),
        ("Option+Space".to_owned(), "Option+Shift+Space".to_owned())
    );
    let custom = Hotkeys {
        polish: Some("Ctrl+Alt+Space".into()),
        polish_translate: None,
    };
    assert_eq!(
        custom.resolve(WINDOWS_HOTKEYS),
        ("Ctrl+Alt+Space".to_owned(), "Alt+Shift+X".to_owned())
    );
}
