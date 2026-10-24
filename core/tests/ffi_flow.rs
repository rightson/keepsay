#![cfg(all(feature = "test-support", feature = "test-endpoint"))]
//! The FFI exports (PROPOSAL §6.1, US-M1-22) end to end: mock vault,
//! wiremock for the API and the model download, a temp data dir.
//!
//! One `#[test]`: the FFI keeps process-wide state, so the steps run in
//! order. The exports build their own runtime, so wiremock is driven from a
//! separate runtime and never from inside an export. Only lengths, flags,
//! reasons and the fingerprint are asserted.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use keepsay_core::downloader::ModelEntry;
use keepsay_core::ffi::{self, DownloadProgress, testing};
use keepsay_core::vault::mock::MockVault;
use keepsay_core::{CoreError, FallbackReason, Mode, PolishOptions, TargetLang};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-ant-test-ffi-0123456789-a1B2";
const KEY2: &str = "sk-ant-test-ffi-0123456789-c3D4";
const INPUT: &str = "嗯那個我們明天下午三點要開會，然後要討論一下 Q4 的 roadmap，記得帶筆電";
const TRANSLATED: &str = "We have a meeting at 3 p.m. tomorrow to discuss the Q4 roadmap. Remember to bring your laptop.";
const MODEL: &[u8] = b"fake ggml model bytes for the ffi test";
const REV: &str = "0123456789abcdef0123456789abcdef01234567";
const MODEL_PATH: &str = "/0123456789abcdef0123456789abcdef01234567/model.bin";

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

async fn api(models_status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(models_status).set_body_json(serde_json::json!({
                "data": [], "has_more": false
            })),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": TRANSLATED}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 40, "output_tokens": 20},
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(MODEL_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(MODEL))
        .mount(&server)
        .await;
    server
}

fn translate() -> PolishOptions {
    PolishOptions {
        mode: Mode::Polish,
        translate_to: Some(TargetLang::En),
        glossary: vec!["roadmap".into()],
    }
}

#[derive(Default)]
struct Counter(AtomicU64);
impl DownloadProgress for Counter {
    fn on_progress(&self, received: u64, total: u64) {
        assert!(received <= total);
        self.0.store(received, Ordering::Relaxed);
    }
}

fn entry(server: &MockServer, sha256: String) -> ModelEntry {
    ModelEntry {
        id: "ffi-test-model".into(),
        name: "FFI test model".into(),
        url: format!("{}{MODEL_PATH}", server.uri()),
        revision: REV.into(),
        sha256,
        size: MODEL.len() as u64,
        license: "MIT".into(),
        redirect_hosts: Vec::new(),
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn ffi_lifecycle() {
    let rt = rt();
    let dir = tempfile::tempdir().unwrap();

    // Before init: no state, and nothing works.
    testing::reset();
    assert!(!ffi::has_key());
    assert_eq!(ffi::key_fingerprint(), None);
    assert_eq!(ffi::store_key(KEY.into()), Err(CoreError::NotInitialized));
    assert_eq!(ffi::verify_key(), Err(CoreError::NotInitialized));
    assert_eq!(
        ffi::polish(INPUT.into(), translate()).err(),
        Some(CoreError::NotInitialized)
    );
    assert_eq!(ffi::budget_status().err(), Some(CoreError::NotInitialized));

    // Linux has no platform vault: init reports it instead of faking one.
    #[cfg(not(any(target_vendor = "apple", target_os = "windows")))]
    assert_eq!(
        ffi::init(dir.path().to_string_lossy().into_owned()),
        Err(CoreError::Unsupported)
    );

    let ok = rt.block_on(api(200));
    testing::init_with(Box::new(MockVault::default()), dir.path(), Some(&ok.uri()));
    ffi::set_utc_offset(8 * 3600).unwrap();

    // Polish without a key: raw, "translation needs API", no request.
    let r = ffi::polish(INPUT.into(), translate()).unwrap();
    assert!(r.fell_back_to_raw && r.text == INPUT);
    assert_eq!(r.fallback_reason, Some(FallbackReason::TranslationNeedsApi));

    // FR-1: store → fingerprint only.
    let fp = ffi::store_key(KEY.into()).unwrap();
    assert_eq!(String::from(fp.clone()), "…a1B2");
    assert!(ffi::has_key());
    assert_eq!(ffi::key_fingerprint(), Some(fp.clone()));
    ffi::verify_key().unwrap();
    assert_eq!(
        ffi::store_key("not-a-key".into()),
        Err(CoreError::KeyFormat)
    );
    assert_eq!(ffi::key_fingerprint(), Some(fp.clone()));

    // Polish and translate; usage lands in budget.json.
    let r = ffi::polish(INPUT.into(), translate()).unwrap();
    assert!(!r.fell_back_to_raw);
    assert_eq!(r.text.len(), TRANSLATED.len());
    let s = ffi::budget_status().unwrap();
    assert_eq!(s.month_usage.input_tokens, 40);
    assert_eq!(s.month_usage.output_tokens, 20);
    assert!(dir.path().join("budget.json").is_file());

    // A replacement that fails verification (401) leaves the old key, and
    // a 401 never deletes the stored key (§6.3).
    let unauthorized = rt.block_on(api(401));
    testing::set_api_base(Some(&unauthorized.uri()));
    assert_eq!(ffi::store_key(KEY2.into()), Err(CoreError::KeyInvalid));
    assert_eq!(ffi::key_fingerprint(), Some(fp.clone()));
    assert_eq!(ffi::verify_key(), Err(CoreError::KeyInvalid));
    assert!(ffi::has_key());
    testing::set_api_base(Some(&ok.uri()));
    let fp2 = ffi::store_key(KEY2.into()).unwrap();
    assert_eq!(String::from(fp2.clone()), "…c3D4");
    assert_eq!(ffi::key_fingerprint(), Some(fp2));

    // delete → no key → raw with NoKey (no translation).
    ffi::delete_key().unwrap();
    ffi::delete_key().unwrap();
    assert!(!ffi::has_key());
    let plain = PolishOptions {
        translate_to: None,
        ..translate()
    };
    let r = ffi::polish(INPUT.into(), plain.clone()).unwrap();
    assert_eq!(r.fallback_reason, Some(FallbackReason::NoKey));

    // A corrupt ledger reads as the hard limit, not as unspent.
    ffi::store_key(KEY.into()).unwrap();
    std::fs::write(dir.path().join("budget.json"), b"{not json").unwrap();
    assert!(ffi::budget_status().unwrap().hard_limited());
    let r = ffi::polish(INPUT.into(), plain).unwrap();
    assert_eq!(r.fallback_reason, Some(FallbackReason::HardLimit));

    // Model download with progress, and a hash mismatch leaves nothing.
    let progress = Arc::new(Counter::default());
    testing::download_entry(&entry(&ok, sha256_hex(MODEL)), progress.clone()).unwrap();
    assert_eq!(progress.0.load(Ordering::Relaxed), MODEL.len() as u64);
    let models = dir.path().join("models");
    assert!(models.join("ffi-test-model.bin").is_file());
    std::fs::remove_file(models.join("ffi-test-model.bin")).unwrap();
    let bad = entry(&ok, "0".repeat(64));
    assert_eq!(
        testing::download_entry(&bad, Arc::new(Counter::default())),
        Err(CoreError::ModelHash)
    );
    assert_eq!(std::fs::read_dir(&models).unwrap().count(), 0);
    assert_eq!(
        ffi::download_model("no-such-model".into(), Arc::new(Counter::default())),
        Err(CoreError::ModelUnknown)
    );

    // No request carried anything but the expected endpoints.
    for server in [&ok, &unauthorized] {
        for req in rt.block_on(server.received_requests()).unwrap() {
            assert!(["/v1/models", "/v1/messages", MODEL_PATH].contains(&req.url.path()));
            if req.url.path() == MODEL_PATH {
                assert!(!req.headers.contains_key("x-api-key"), "INV-7");
            }
        }
    }
    testing::reset();
}
