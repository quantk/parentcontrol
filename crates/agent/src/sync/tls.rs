//! TLS to the server with its certificate pinned by fingerprint.
//!
//! The server's certificate is usually self-signed, so the agent trusts
//! exactly the certificate it saw when it enrolled, not any CA.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio_rustls::TlsConnector;

/// What the agent expects of the server certificate.
#[derive(Debug, Clone)]
pub(crate) enum Trust {
    /// Exactly the certificate with this fingerprint.
    Pinned(String),
    /// Any certificate, recording its fingerprint: trust on first use,
    /// for enrollment. The parent compares the fingerprint with the panel.
    FirstUse(Arc<Mutex<Option<String>>>),
}

/// A TLS connector enforcing `trust`.
pub(crate) fn connector(trust: Trust) -> Result<TlsConnector, rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Verifier {
        trust,
        provider: Arc::clone(&provider),
    };
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

/// SHA-256 of a DER certificate in lowercase hex.
pub(crate) fn fingerprint(certificate: &[u8]) -> String {
    Sha256::digest(certificate)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            // Writing to a `String` cannot fail.
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// `3f2a91bc…` as `3f2a 91bc …`, as the panel shows it.
pub(crate) fn grouped(fingerprint: &str) -> String {
    fingerprint
        .as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug)]
struct Verifier {
    trust: Trust,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let actual = fingerprint(end_entity.as_ref());
        match &self.trust {
            Trust::Pinned(expected) if *expected == actual => Ok(ServerCertVerified::assertion()),
            Trust::Pinned(_) => Err(rustls::Error::General(
                "the server certificate differs from the one seen at enrollment".to_owned(),
            )),
            Trust::FirstUse(seen) => {
                *seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(actual);
                Ok(ServerCertVerified::assertion())
            }
        }
    }

    // Handshake signatures are checked as usual: pinning replaces only the
    // chain validation, not the proof that the server holds the key.
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_hex_sha256() {
        assert_eq!(fingerprint(b"").len(), 64);
        assert!(fingerprint(b"").starts_with("e3b0c442"));
    }

    #[test]
    fn grouping_matches_the_panel() {
        assert_eq!(grouped("0123456789ab"), "0123 4567 89ab");
    }
}
