//! INV-7: the model downloader structurally cannot reach the key.

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

#[test]
#[ignore = "M1: needs sha2 + manifest loader"]
fn checksum_mismatch_is_rejected() {}
