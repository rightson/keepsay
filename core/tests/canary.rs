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
    }
}
const VARIANT_COUNT: usize = 23;

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
