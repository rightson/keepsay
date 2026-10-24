//! INV-1 / INV-2 / INV-6 canary (PROPOSAL §11): run every path with a fake
//! key, then scan logs, every `Debug` / `Display` rendering, and every file
//! written under the config dir; the canary must appear zero times.
//!
//! Joining from other workstreams: build a [`Canary`], run your paths inside
//! `canary.capture(|| ...)` (or keep the guard from `canary.start()` alive in
//! an async test), point any file output at `canary.dir()`, feed public
//! values through `canary.render(&value)`, then call `canary.assert_clean()`.

#![cfg(feature = "test-support")]

use std::fmt::{Debug, Display};
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};

use keepsay_core::CoreError;
use keepsay_core::vault::mock::MockVault;
use keepsay_core::vault::{
    self, KeyVault, VaultOptions, check_format, delete_key, fingerprint, has_key, key_fingerprint,
    store_key,
};
use secrecy::SecretString;
use tracing_subscriber::fmt::MakeWriter;

const CANARY: &str = "sk-ant-test-CANARY-7f3a9c1d";
/// Distinctive substrings: any partial leak of the canary hits one of these.
const NEEDLES: &[&str] = &[CANARY, "CANARY-7f3a", "test-CANARY"];

fn canary_key() -> SecretString {
    SecretString::from(CANARY)
}

// ---------------------------------------------------------------- harness

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl io::Write for Buf {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("log buffer").extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Buf {
    type Writer = Buf;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Captures every tracing event (TRACE and up), a temp config dir, and
/// rendered values; [`Canary::assert_clean`] scans all of it.
struct Canary {
    logs: Buf,
    dir: tempfile::TempDir,
    rendered: Mutex<Vec<String>>,
}

impl Canary {
    fn new() -> Self {
        Self {
            logs: Buf::default(),
            dir: tempfile::tempdir().expect("tempdir"),
            rendered: Mutex::new(Vec::new()),
        }
    }

    /// Directory to use as the config / budget / data dir for the paths
    /// under test. Everything written below it is scanned.
    fn dir(&self) -> &Path {
        self.dir.path()
    }

    fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + 'static {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(self.logs.clone())
            .finish()
    }

    /// Install the capturing subscriber on this thread until the guard
    /// drops. For `#[tokio::test]` (current-thread runtime).
    fn start(&self) -> tracing::subscriber::DefaultGuard {
        tracing::subscriber::set_default(self.subscriber())
    }

    fn capture<R>(&self, f: impl FnOnce() -> R) -> R {
        tracing::subscriber::with_default(self.subscriber(), f)
    }

    /// Record both renderings of a public value.
    fn render<T: Debug + Display + ?Sized>(&self, v: &T) {
        let mut r = self.rendered.lock().expect("rendered");
        r.push(format!("{v:?}"));
        r.push(format!("{v}"));
        r.push(format!("{v:#?}"));
    }

    /// Record the `Debug` rendering of a value that has no `Display`.
    fn render_debug<T: Debug + ?Sized>(&self, v: &T) {
        let mut r = self.rendered.lock().expect("rendered");
        r.push(format!("{v:?}"));
        r.push(format!("{v:#?}"));
    }

    fn assert_clean(&self) {
        let logs = self.logs.0.lock().expect("log buffer").clone();
        assert!(!logs.is_empty(), "log capture is not working");
        assert_eq!(count(&logs), 0, "canary found in captured logs");

        for (i, s) in self.rendered.lock().expect("rendered").iter().enumerate() {
            assert_eq!(
                count(s.as_bytes()),
                0,
                "canary found in rendered value #{i}"
            );
        }

        let mut files = 0;
        scan_dir(self.dir(), &mut files);
        // Not asserting files > 0: vault paths write nothing here. Other
        // workstreams' paths (config, budget) will.
    }
}

/// Occurrences of any needle, as UTF-8 and as UTF-16LE (Windows APIs and
/// Credential Manager exports use UTF-16).
fn count(hay: &[u8]) -> usize {
    NEEDLES
        .iter()
        .map(|n| {
            let utf16: Vec<u8> = n.encode_utf16().flat_map(u16::to_le_bytes).collect();
            occurrences(hay, n.as_bytes()) + occurrences(hay, &utf16)
        })
        .sum()
}

fn occurrences(hay: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || hay.len() < needle.len() {
        return 0;
    }
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

fn scan_dir(dir: &Path, files: &mut usize) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            scan_dir(&path, files);
        } else {
            *files += 1;
            let bytes = std::fs::read(&path).expect("read file");
            // The path is a tempdir path, never content.
            assert_eq!(count(&bytes), 0, "canary found in {}", path.display());
        }
    }
}

const ALL_ERRORS: &[CoreError] = &[
    CoreError::KeyInvalid,
    CoreError::KeyFormat,
    CoreError::KeyNotFound,
    CoreError::VaultUnavailable,
    CoreError::VaultNotEntitled,
    CoreError::DisallowedHost,
    CoreError::Network,
    CoreError::Timeout,
    CoreError::RateLimited,
    CoreError::BudgetExhausted,
    CoreError::ApiStatus(500),
    CoreError::InvalidResponse,
    CoreError::ModelNotAllowed,
    CoreError::GuardRejected,
    CoreError::HardLimitReached,
    CoreError::AudioFormat,
    CoreError::Stt,
    CoreError::ModelUnknown,
    CoreError::ModelHash,
    CoreError::ModelTooLarge,
    CoreError::Config,
    CoreError::Io(io::ErrorKind::NotFound),
    CoreError::Unsupported,
    CoreError::NotInitialized,
];

/// Exhaustive on purpose (no `_` arm): adding a `CoreError` variant fails to
/// compile here until it is also added to [`ALL_ERRORS`].
fn variant_index(e: CoreError) -> usize {
    use CoreError::*;
    match e {
        KeyInvalid => 0,
        KeyFormat => 1,
        KeyNotFound => 2,
        VaultUnavailable => 3,
        VaultNotEntitled => 4,
        DisallowedHost => 5,
        Network => 6,
        Timeout => 7,
        RateLimited => 8,
        BudgetExhausted => 9,
        ApiStatus(_) => 10,
        InvalidResponse => 11,
        ModelNotAllowed => 12,
        GuardRejected => 13,
        HardLimitReached => 14,
        AudioFormat => 15,
        Stt => 16,
        ModelUnknown => 17,
        ModelHash => 18,
        ModelTooLarge => 19,
        Config => 20,
        Io(_) => 21,
        Unsupported => 22,
        NotInitialized => 23,
    }
}
const VARIANT_COUNT: usize = 24;

#[test]
fn all_errors_lists_every_variant() {
    let mut seen = [false; VARIANT_COUNT];
    for e in ALL_ERRORS {
        seen[variant_index(*e)] = true;
    }
    assert!(
        seen.iter().all(|s| *s),
        "ALL_ERRORS misses a CoreError variant"
    );
}

// ------------------------------------------------------------------ tests

/// Vault lifecycle, happy path and every vault error path, with the mock.
#[tokio::test]
async fn vault_paths_never_leak_the_canary() {
    let canary = Canary::new();
    let _guard = canary.start();
    tracing::info!("canary: capture active");

    // Happy path: format → verify → store → has / fingerprint → load → delete.
    let v = MockVault::default();
    let fp = store_key(&v, canary_key(), async |_k: &SecretString| Ok(()))
        .await
        .expect("store");
    canary.render(&fp);
    assert!(has_key(&v));
    let fp2 = key_fingerprint(&v).expect("fingerprint");
    assert_eq!(fp, fp2);
    canary.render(&fp2);
    canary.render(&fingerprint(&canary_key()));
    let loaded = v.load().expect("load");
    canary.render_debug(&loaded);
    canary.render_debug(&v);
    canary.render_debug(&Some(canary_key()));
    delete_key(&v).expect("delete");
    assert!(!has_key(&v));
    assert_eq!(key_fingerprint(&v), None);

    // Verification fails (401 → KeyInvalid): nothing stored.
    let e = store_key(&v, canary_key(), async |_k: &SecretString| {
        Err(CoreError::KeyInvalid)
    })
    .await
    .expect_err("verify fails");
    canary.render(&e);
    assert!(!has_key(&v));

    // Bad format: never reaches the verifier.
    let bad = SecretString::from(format!("{CANARY} \n"));
    let e = store_key(
        &v,
        bad,
        async |_k: &SecretString| -> Result<(), CoreError> {
            panic!("verifier must not run for a malformed key")
        },
    )
    .await
    .expect_err("format");
    assert_eq!(e, CoreError::KeyFormat);
    canary.render(&e);
    canary.render(&check_format(&SecretString::from("sk-ant-")).expect_err("short"));

    // Storage refuses (locked keychain, missing entitlement).
    for err in [
        CoreError::VaultUnavailable,
        CoreError::VaultNotEntitled,
        CoreError::Unsupported,
    ] {
        let failing = MockVault::failing(err);
        let e = store_key(&failing, canary_key(), async |_k: &SecretString| Ok(()))
            .await
            .expect_err("vault fails");
        canary.render(&e);
        canary.render_debug(&failing.load());
        canary.render_debug(&failing.delete());
        assert_eq!(key_fingerprint(&failing), None);
        canary.render_debug(&failing);
    }

    // Load with nothing stored.
    canary.render_debug(&v.load());

    // The platform vault: on Linux `Unsupported`; on macOS / Windows its
    // Debug output. Never stores anything here (see vault_platform.rs).
    match vault::platform_vault_with(VaultOptions::default()) {
        Ok(p) => canary.render_debug(&p.exists()),
        Err(e) => canary.render(&e),
    }
    canary.render_debug(&VaultOptions::default());

    for e in ALL_ERRORS {
        canary.render(e);
    }

    write_config_artifacts(canary.dir());
    canary.assert_clean();
}

/// Hook for paths that write files (config, budget). Workstream D / the
/// integration pass add their `save` calls here, using `dir` as the config
/// dir. Kept as a smoke file so the directory scan is exercised.
fn write_config_artifacts(dir: &Path) {
    std::fs::write(dir.join("smoke.txt"), b"canary harness").expect("write");
}

/// §6.9 / US-M1-20: the core panic hook logs the location, never the
/// payload. The hook is process-wide; this binary's other tests do not
/// panic on purpose, and the previous hook is restored afterwards.
#[test]
fn panic_hook_logs_location_not_payload() {
    let canary = Canary::new();
    let previous = std::panic::take_hook();
    keepsay_core::panic_hook::install();
    let r = canary.capture(|| std::panic::catch_unwind(|| panic!("payload {CANARY}")));
    std::panic::set_hook(previous);
    assert!(r.is_err());
    let logs = String::from_utf8(canary.logs.0.lock().expect("logs").clone()).expect("utf8");
    assert!(logs.contains("canary.rs"), "location is logged");
    canary.assert_clean();
}

#[test]
fn scanner_detects_leaks() {
    // Guards against the scanner silently passing.
    assert_eq!(count(CANARY.as_bytes()), NEEDLES.len());
    let utf16: Vec<u8> = CANARY.encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert!(count(&utf16) > 0);
    assert!(count(b"prefix CANARY-7f3a suffix") > 0);
    assert_eq!(count(fingerprint(&canary_key()).as_str().as_bytes()), 0);

    let canary = Canary::new();
    canary.capture(|| tracing::info!(len = CANARY.len(), "canary: capture works"));
    std::fs::write(canary.dir().join("leak.json"), CANARY).expect("write");
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scan_dir(canary.dir(), &mut 0)
    }));
    assert!(r.is_err(), "a leaked file must fail the scan");
}

// ------------------------------------------------- integration paths (M1)

/// API error paths, polish fallbacks, config / budget saves, downloader
/// errors and the FFI, all with the canary as the stored key (US-M1-21).
/// Also INV-5: the polish paths leave the data dir empty, and the canary
/// reaches the mock API only in the `x-api-key` header.
///
/// A plain `#[test]`: the FFI exports build their own runtime, so async
/// parts run in `rt.block_on` and the FFI calls run outside it. The capture
/// guard covers both (same thread).
#[cfg(feature = "test-endpoint")]
#[test]
fn integration_paths_never_leak_the_canary() {
    use std::time::Duration;

    use keepsay_core::api::{AnthropicClient, MessagesRequest};
    use keepsay_core::budget::{Budget, YearMonth};
    use keepsay_core::config::Config;
    use keepsay_core::downloader::{self, ModelDownloader, ModelEntry};
    use keepsay_core::ffi::{self, testing};
    use keepsay_core::polish::polish;
    use keepsay_core::{BudgetStatus, Mode, PolishOptions, TargetLang};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const INPUT: &str = "嗯那個我們明天下午三點要開會，然後要討論一下 Q4 的 roadmap，記得帶筆電";
    const POLISHED: &str = "我們明天下午三點要開會，討論 Q4 的 roadmap，記得帶筆電。";

    let canary = Canary::new();
    let _guard = canary.start();
    tracing::info!("canary: integration capture active");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let vault = MockVault::default();
    vault.store(canary_key()).expect("store canary");

    let ok_body = |text: &str, stop: &str| {
        serde_json::json!({
            "content": [{"type": "text", "text": text}],
            "stop_reason": stop,
            "usage": {"input_tokens": 40, "output_tokens": 20},
        })
    };
    let error_body = |kind: &str, msg: &str| serde_json::json!({"type": "error", "error": {"type": kind, "message": msg}});
    let mut servers = Vec::new();

    // --- API error paths: 401, 403, 402, 400 credit, 429 twice, 529, 500,
    // 302, malformed body, timeout. Both verify_key and messages.
    let cases: Vec<ResponseTemplate> = vec![
        ResponseTemplate::new(401).set_body_json(error_body("authentication_error", "invalid")),
        ResponseTemplate::new(403).set_body_json(error_body("permission_error", "denied")),
        ResponseTemplate::new(402).set_body_json(error_body("billing_error", "billing")),
        ResponseTemplate::new(400).set_body_json(error_body(
            "invalid_request_error",
            "Your credit balance is too low",
        )),
        ResponseTemplate::new(429)
            .insert_header("retry-after", "0")
            .set_body_json(error_body("rate_limit_error", "slow down")),
        ResponseTemplate::new(529).insert_header("retry-after", "0"),
        ResponseTemplate::new(500).set_body_string("internal"),
        ResponseTemplate::new(302).insert_header("location", "https://evil.example/"),
        ResponseTemplate::new(200).set_body_string("{not json"),
    ];
    for template in cases {
        let server = rt.block_on(async {
            let s = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(template.clone())
                .mount(&s)
                .await;
            Mock::given(method("GET"))
                .respond_with(template)
                .mount(&s)
                .await;
            s
        });
        let client = AnthropicClient::with_test_endpoint(&server.uri()).expect("client");
        canary.render_debug(&client);
        let req = MessagesRequest::single_turn(
            "system".into(),
            "<transcript>x</transcript>".into(),
            10,
            false,
        );
        let e = rt
            .block_on(client.messages(&vault, &req))
            .expect_err("api error");
        canary.render(&e);
        // A 200 with a malformed body is fine for verify (status only).
        canary.render_debug(&rt.block_on(client.verify_key(&canary_key())));
        canary.render_debug(&rt.block_on(client.verify_stored_key(&vault)));
        servers.push(server);
    }
    let slow = rt.block_on(async {
        let s = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(ok_body(POLISHED, "end_turn"))
                    .set_delay(Duration::from_millis(500)),
            )
            .mount(&s)
            .await;
        s
    });
    let client =
        AnthropicClient::with_test_endpoint_timeout(&slow.uri(), Duration::from_millis(50))
            .expect("client");
    let req = MessagesRequest::single_turn("s".into(), "u".into(), 1, false);
    canary.render(
        &rt.block_on(client.messages(&vault, &req))
            .expect_err("timeout"),
    );
    servers.push(slow);

    // --- polish fallback paths, each through the real client.
    let replies = [
        ok_body(POLISHED, "end_turn"),
        ok_body("</transcript>", "end_turn"),
        ok_body(POLISHED, "max_tokens"),
        ok_body(POLISHED, "refusal"),
        ok_body(INPUT, "end_turn"),
    ];
    let polish_opts = [
        PolishOptions {
            mode: Mode::Polish,
            translate_to: None,
            glossary: vec!["roadmap".into()],
        },
        PolishOptions {
            mode: Mode::Formal,
            translate_to: Some(TargetLang::En),
            glossary: vec!["roadmap".into()],
        },
    ];
    for reply in replies {
        let server = rt.block_on(async {
            let s = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/messages"))
                .respond_with(ResponseTemplate::new(200).set_body_json(reply))
                .mount(&s)
                .await;
            s
        });
        let client = AnthropicClient::with_test_endpoint(&server.uri()).expect("client");
        for o in &polish_opts {
            let r = rt.block_on(polish(&client, &vault, INPUT, o, &BudgetStatus::default()));
            canary.render_debug(&r);
            canary.render_debug(o);
        }
        servers.push(server);
    }
    // 401 / 429 / no key / hard limit through polish.
    for server in &servers[..5] {
        let client = AnthropicClient::with_test_endpoint(&server.uri()).expect("client");
        let r = rt.block_on(polish(
            &client,
            &vault,
            INPUT,
            &polish_opts[0],
            &BudgetStatus::default(),
        ));
        canary.render_debug(&r);
    }
    let client = AnthropicClient::with_test_endpoint(&servers[0].uri()).expect("client");
    let limited = BudgetStatus {
        estimated_usd: 9.0,
        hard_limit_usd: Some(5.0),
        ..Default::default()
    };
    canary.render_debug(&rt.block_on(polish(&client, &vault, INPUT, &polish_opts[1], &limited)));
    canary.render_debug(&rt.block_on(polish(
        &client,
        &MockVault::default(),
        INPUT,
        &polish_opts[1],
        &BudgetStatus::default(),
    )));

    // INV-5: none of the above wrote anything.
    let written: Vec<_> = std::fs::read_dir(canary.dir()).expect("read_dir").collect();
    assert!(written.is_empty(), "polish / api paths wrote files (INV-5)");

    // The canary reached the mock servers only as the x-api-key header.
    for server in &servers {
        for req in rt.block_on(server.received_requests()).expect("recording") {
            assert_eq!(count(&req.body), 0, "canary in a request body");
            assert_eq!(count(req.url.as_str().as_bytes()), 0, "canary in a URL");
            for (name, value) in req.headers.iter() {
                if name.as_str() != "x-api-key" {
                    assert_eq!(count(value.as_bytes()), 0, "canary in header {name}");
                }
            }
        }
    }

    // --- config and budget saves (non-secret, with a glossary and proxy).
    let mut config = Config {
        glossary: vec!["roadmap".into(), "keepsay".into()],
        proxy: Some("http://user:pw@127.0.0.1:3128".into()),
        ..Config::default()
    };
    config
        .save(&canary.dir().join("config.json"))
        .expect("config save");
    canary.render_debug(&config);
    config.model = "not-allowed".into();
    canary.render(
        &config
            .save(&canary.dir().join("bad.json"))
            .expect_err("invalid"),
    );
    let now = YearMonth::from_unix_secs(1_792_800_000);
    let mut budget = Budget::new(now);
    budget.record(
        keepsay_core::Usage {
            input_tokens: 40,
            output_tokens: 20,
        },
        now,
    );
    budget
        .save(&canary.dir().join("budget.json"))
        .expect("budget save");
    canary.render_debug(&budget);
    std::fs::write(canary.dir().join("corrupt.json"), b"{nope").expect("write");
    canary.render(&Budget::load(&canary.dir().join("corrupt.json"), now).expect_err("corrupt"));
    canary.render(&Config::load(&canary.dir().join("corrupt.json")).expect_err("corrupt"));

    // --- downloader error paths, into the scanned dir.
    let body: &[u8] = b"fake model bytes";
    let rev = "0123456789abcdef0123456789abcdef01234567";
    let model_server = rt.block_on(async {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/{rev}/ok.bin")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{rev}/big.bin")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 4096]))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{rev}/away.bin")))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", "https://evil.example/x.bin"),
            )
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{rev}/gone.bin")))
            .respond_with(ResponseTemplate::new(404))
            .mount(&s)
            .await;
        s
    });
    let entry = |file: &str, sha256: &str| ModelEntry {
        id: "canary-model".into(),
        name: "Canary model".into(),
        url: format!("{}/{rev}/{file}", model_server.uri()),
        revision: rev.into(),
        sha256: sha256.into(),
        size: body.len() as u64,
        license: "MIT".into(),
        redirect_hosts: Vec::new(),
    };
    let dl = ModelDownloader::with_test_endpoint().expect("downloader");
    canary.render_debug(&dl);
    let models = canary.dir().join("models");
    for file in ["ok.bin", "big.bin", "away.bin", "gone.bin"] {
        let mut progress = |_: u64, _: u64| {};
        let e = rt
            .block_on(dl.download(&entry(file, &"0".repeat(64)), &models, &mut progress))
            .expect_err("download error");
        canary.render(&e);
    }
    let src = canary.dir().join("import-src.bin");
    std::fs::write(&src, body).expect("write");
    canary.render(
        &downloader::import(&entry("ok.bin", &"0".repeat(64)), &src, &models).expect_err("hash"),
    );
    for req in rt
        .block_on(model_server.received_requests())
        .expect("recording")
    {
        assert!(!req.headers.contains_key("x-api-key"), "INV-7");
    }

    // --- the FFI, with the canary in the mock vault.
    let ffi_dir = canary.dir().join("ffi");
    let ok_server = &servers[servers.len() - 5]; // replies POLISHED / end_turn
    testing::init_with(
        Box::new(MockVault::default()),
        &ffi_dir,
        Some(&ok_server.uri()),
    );
    canary.render(&ffi::store_key(CANARY.into()).expect_err("no /v1/models mock: 404"));
    testing::set_api_base(Some(&servers[0].uri())); // 401 everywhere
    canary.render(&ffi::store_key(CANARY.into()).expect_err("401"));
    canary.render_debug(&ffi::key_fingerprint());
    canary.render_debug(&ffi::polish(INPUT.into(), polish_opts[1].clone()));
    canary.render_debug(&ffi::budget_status());
    testing::reset();
    let v = MockVault::default();
    v.store(canary_key()).expect("store");
    testing::init_with(Box::new(v), &ffi_dir, Some(&ok_server.uri()));
    let fp = ffi::key_fingerprint().expect("fingerprint");
    canary.render(&fp);
    canary.render_debug(&ffi::polish(INPUT.into(), polish_opts[0].clone()));
    testing::set_api_base(Some(&servers[0].uri()));
    canary.render(&ffi::verify_key().expect_err("401"));
    canary.render_debug(&ffi::polish(INPUT.into(), polish_opts[0].clone()));
    canary.render_debug(&ffi::budget_status());
    ffi::delete_key().expect("delete");
    testing::reset();

    canary.assert_clean();
}
