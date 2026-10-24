//! INV-7: the model downloader structurally cannot reach the key, and only
//! files whose size and SHA-256 match the manifest are ever kept.

use std::fs;
use std::path::Path;

use keepsay_core::CoreError;
use keepsay_core::downloader::{self, ModelEntry};

#[test]
fn downloader_does_not_reference_vault_or_secrets() {
    let src = include_str!("../src/downloader.rs");
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "vault",
        "secrecy",
        "SecretString",
        "AnthropicClient",
        "api::",
    ] {
        assert!(
            !code.contains(forbidden),
            "downloader.rs must not reference {forbidden}"
        );
    }
}

const REV: &str = "0123456789abcdef0123456789abcdef01234567";
const BODY: &[u8] = b"fake ggml model bytes for tests";

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn entry_for(url: String, body: &[u8]) -> ModelEntry {
    ModelEntry {
        id: "test-model".into(),
        name: "Test model".into(),
        url,
        revision: REV.into(),
        sha256: sha256_hex(body),
        size: body.len() as u64,
        license: "MIT".into(),
        redirect_hosts: Vec::new(),
    }
}

fn dir_entries(dir: &Path) -> Vec<String> {
    match fs::read_dir(dir) {
        Ok(rd) => rd
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn https_entry(body: &[u8]) -> ModelEntry {
    entry_for(
        format!("https://huggingface.co/x/resolve/{REV}/test.bin"),
        body,
    )
}

#[test]
fn checksum_mismatch_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src.bin");
    let dest = tmp.path().join("models");
    // Same length, different bytes.
    let mut tampered = BODY.to_vec();
    tampered[0] ^= 1;
    fs::write(&src, &tampered).unwrap();

    let entry = https_entry(BODY);
    assert_eq!(
        downloader::verify_file(&entry, &src),
        Err(CoreError::ModelHash)
    );
    assert_eq!(
        downloader::import(&entry, &src, &dest),
        Err(CoreError::ModelHash)
    );
    assert!(dir_entries(&dest).is_empty(), "nothing may be left behind");
}

#[test]
fn wrong_size_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src.bin");
    let dest = tmp.path().join("models");
    fs::write(&src, b"short").unwrap();
    let entry = https_entry(BODY);
    assert_eq!(
        downloader::verify_file(&entry, &src),
        Err(CoreError::ModelHash)
    );
    assert_eq!(
        downloader::import(&entry, &src, &dest),
        Err(CoreError::ModelHash)
    );
    assert!(dir_entries(&dest).is_empty());
}

#[test]
fn import_with_matching_hash_lands_at_model_path() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src.bin");
    let dest = tmp.path().join("models");
    fs::write(&src, BODY).unwrap();
    let entry = https_entry(BODY);

    let path = downloader::import(&entry, &src, &dest).unwrap();
    assert_eq!(path, downloader::model_path(&entry, &dest));
    assert_eq!(fs::read(&path).unwrap(), BODY);
    assert_eq!(dir_entries(&dest), vec!["test-model.bin".to_owned()]);
    assert_eq!(downloader::verify_file(&entry, &path), Ok(()));
}

#[test]
fn plain_http_is_refused_without_test_endpoint() {
    let entry = entry_for(format!("http://example.com/x/{REV}/test.bin"), BODY);
    assert_eq!(entry.validate(), Err(CoreError::DisallowedHost));
}

/// wiremock-served downloads. Plain-HTTP loopback URLs are only reachable
/// through `ModelDownloader::with_test_endpoint`, which exists only with the
/// `test-endpoint` feature.
#[cfg(feature = "test-endpoint")]
mod served {
    use std::time::Duration;

    use keepsay_core::downloader::ModelDownloader;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const FILE_PATH: &str = "/x/resolve/0123456789abcdef0123456789abcdef01234567/test.bin";

    async fn serve(body: &[u8]) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(FILE_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn release_constructor_refuses_loopback_http() {
        let server = serve(BODY).await;
        let entry = entry_for(format!("{}{FILE_PATH}", server.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::new().unwrap();
        let r = d.download(&entry, tmp.path(), &mut |_, _| {}).await;
        assert_eq!(r, Err(CoreError::DisallowedHost));
        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(dir_entries(tmp.path()).is_empty());
    }

    #[tokio::test]
    async fn success_verifies_and_reports_progress() {
        let server = serve(BODY).await;
        let entry = entry_for(format!("{}{FILE_PATH}", server.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();

        let mut last = (0, 0);
        let mut calls = 0;
        let path = d
            .download(&entry, tmp.path(), &mut |got, total| {
                last = (got, total);
                calls += 1;
            })
            .await
            .unwrap();
        assert_eq!(path, downloader::model_path(&entry, tmp.path()));
        assert_eq!(fs::read(&path).unwrap(), BODY);
        assert_eq!(last, (BODY.len() as u64, BODY.len() as u64));
        assert!(calls >= 2);
        assert_eq!(dir_entries(tmp.path()), vec!["test-model.bin".to_owned()]);
    }

    #[tokio::test]
    async fn served_hash_mismatch_is_removed() {
        let mut tampered = BODY.to_vec();
        tampered[3] ^= 0x20;
        let server = serve(&tampered).await;
        let entry = entry_for(format!("{}{FILE_PATH}", server.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        let r = d.download(&entry, tmp.path(), &mut |_, _| {}).await;
        assert_eq!(r, Err(CoreError::ModelHash));
        assert!(dir_entries(tmp.path()).is_empty());
    }

    #[tokio::test]
    async fn oversized_body_is_aborted() {
        let mut big = BODY.to_vec();
        big.extend_from_slice(&[0u8; 4096]);
        let server = serve(&big).await;
        let entry = entry_for(format!("{}{FILE_PATH}", server.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        let r = d.download(&entry, tmp.path(), &mut |_, _| {}).await;
        assert_eq!(r, Err(CoreError::ModelTooLarge));
        assert!(dir_entries(tmp.path()).is_empty());
    }

    #[tokio::test]
    async fn existing_model_is_kept_when_new_download_fails() {
        let server = serve(b"not the right bytes, wrong length too").await;
        let entry = entry_for(format!("{}{FILE_PATH}", server.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let final_path = downloader::model_path(&entry, tmp.path());
        fs::write(&final_path, BODY).unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        assert!(
            d.download(&entry, tmp.path(), &mut |_, _| {})
                .await
                .is_err()
        );
        assert_eq!(fs::read(&final_path).unwrap(), BODY);
        assert_eq!(dir_entries(tmp.path()), vec!["test-model.bin".to_owned()]);
    }

    #[tokio::test]
    async fn redirect_to_unlisted_host_is_refused() {
        // The mirror would serve a valid file, but it is not in the entry.
        let mirror = serve(BODY).await;
        let mirror_url = mirror.uri().replace("127.0.0.1", "localhost");
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(FILE_PATH))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{mirror_url}{FILE_PATH}")),
            )
            .mount(&origin)
            .await;
        let entry = entry_for(format!("{}{FILE_PATH}", origin.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        let r = d.download(&entry, tmp.path(), &mut |_, _| {}).await;
        assert_eq!(r, Err(CoreError::DisallowedHost));
        assert!(mirror.received_requests().await.unwrap().is_empty());
        assert!(dir_entries(tmp.path()).is_empty());
    }

    #[tokio::test]
    async fn redirect_to_listed_host_is_followed() {
        let mirror = serve(BODY).await;
        let mirror_url = mirror.uri().replace("127.0.0.1", "localhost");
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(FILE_PATH))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{mirror_url}{FILE_PATH}")),
            )
            .mount(&origin)
            .await;
        let mut entry = entry_for(format!("{}{FILE_PATH}", origin.uri()), BODY);
        entry.redirect_hosts = vec!["localhost".into()];
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        let path = d
            .download(&entry, tmp.path(), &mut |_, _| {})
            .await
            .unwrap();
        assert_eq!(fs::read(path).unwrap(), BODY);
    }

    #[tokio::test]
    async fn redirect_loop_is_bounded() {
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(FILE_PATH))
            .respond_with(ResponseTemplate::new(302).insert_header("location", FILE_PATH))
            .mount(&origin)
            .await;
        let entry = entry_for(format!("{}{FILE_PATH}", origin.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        let r = d.download(&entry, tmp.path(), &mut |_, _| {}).await;
        assert_eq!(r, Err(CoreError::Network));
        let hits = origin.received_requests().await.unwrap().len();
        assert_eq!(hits, downloader::MAX_REDIRECTS + 1);
        assert!(dir_entries(tmp.path()).is_empty());
    }

    #[tokio::test]
    async fn error_status_leaves_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let entry = entry_for(format!("{}{FILE_PATH}", server.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        let r = d.download(&entry, tmp.path(), &mut |_, _| {}).await;
        assert_eq!(r, Err(CoreError::ApiStatus(404)));
        assert!(dir_entries(tmp.path()).is_empty());
    }

    #[tokio::test]
    async fn cancelled_download_leaves_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(FILE_PATH))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(BODY.to_vec())
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;
        let entry = entry_for(format!("{}{FILE_PATH}", server.uri()), BODY);
        let tmp = tempfile::tempdir().unwrap();
        let d = ModelDownloader::with_test_endpoint().unwrap();
        let r = tokio::time::timeout(
            Duration::from_millis(200),
            d.download(&entry, tmp.path(), &mut |_, _| {}),
        )
        .await;
        assert!(r.is_err(), "download should still be pending");
        assert!(dir_entries(tmp.path()).is_empty());
    }
}
