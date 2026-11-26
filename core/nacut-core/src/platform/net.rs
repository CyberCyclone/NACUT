//! The HTTPS client every request goes through. Only `api.anthropic.com` is ever contacted.
//!
//! Corporate networks often inspect TLS with their own root certificate and require a proxy, so:
//! - certificates are checked against the OS trust store (Keychain, Windows certificate store, `/etc/ssl`)
//!   as well as the bundled Mozilla roots;
//! - `HTTPS_PROXY` / `ALL_PROXY` (and lowercase forms) are honoured;
//! - plain HTTP is refused, including HTTPS → HTTP redirects.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
            // Errors here only mean fewer extra roots; the bundled ones still apply.
            let native = rustls_native_certs::load_native_certs();
            roots.add_parsable_certificates(native.certs);
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("ring supports the default TLS versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// An HTTPS-only agent with the given overall timeout.
pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .tls_config(tls_config())
        .try_proxy_from_env(true)
        .https_only(true)
        .timeout(timeout)
        .build()
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_tls_config() {
        assert!(!super::tls_config().alpn_protocols.iter().any(|p| p.is_empty()));
        let _ = super::agent(std::time::Duration::from_secs(1));
    }

    /// `cargo test -- --ignored live_tls`: an unauthenticated request, to check TLS setup end to end.
    #[test]
    #[ignore]
    fn live_tls() {
        match super::agent(std::time::Duration::from_secs(20)).get("https://api.anthropic.com/").call() {
            Ok(_) | Err(ureq::Error::Status(..)) => {}
            Err(e) => panic!("{e}"),
        }
    }
}
