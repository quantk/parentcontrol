//! The container health check: a real HTTPS request to `/healthz`.
//!
//! It runs inside the container against the local listener, so it trusts
//! whatever certificate is served; checking it would prove nothing here.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::ServerError;

/// Requests `/healthz` from the server listening on `listen`'s port on
/// this machine.
///
/// # Errors
///
/// Fails when the server does not answer `200 OK`.
pub async fn check(listen: SocketAddr) -> Result<(), ServerError> {
    let unhealthy = |reason: String| ServerError::Unhealthy(reason);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(|error| unhealthy(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AnyCertificate(provider)))
        .with_no_client_auth();

    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, listen.port()));
    let tcp = TcpStream::connect(address)
        .await
        .map_err(|error| unhealthy(error.to_string()))?;
    let name = ServerName::try_from("localhost").map_err(|error| unhealthy(error.to_string()))?;
    let mut tls = TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|error| unhealthy(error.to_string()))?;
    tls.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .map_err(|error| unhealthy(error.to_string()))?;
    let mut response = Vec::new();
    tls.read_to_end(&mut response)
        .await
        .map_err(|error| unhealthy(error.to_string()))?;
    if response.starts_with(b"HTTP/1.1 200") {
        Ok(())
    } else {
        Err(unhealthy(
            String::from_utf8_lossy(&response)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
        ))
    }
}

/// Accepts any certificate but still checks handshake signatures.
#[derive(Debug)]
struct AnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

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
            &self.0.signature_verification_algorithms,
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
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
