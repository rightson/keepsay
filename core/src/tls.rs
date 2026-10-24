//! TLS setup shared by the two HTTP clients.
//!
//! reqwest is built with `rustls-no-provider` so that aws-lc never enters the
//! build (docs/changes/0012). The ring provider must therefore be installed
//! before any `reqwest::Client` is built, or reqwest panics.

/// Installs rustls' ring provider as the process default. Idempotent: a
/// second call (or a provider installed by the host app) is not an error.
pub(crate) fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    #[test]
    fn install_is_idempotent() {
        super::ensure_crypto_provider();
        super::ensure_crypto_provider();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
