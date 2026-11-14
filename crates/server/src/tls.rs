//! The server certificate: configured, or self-signed on first start.
//!
//! Agents pin the certificate's SHA-256 fingerprint at enrollment, so a
//! self-signed certificate is as safe for them as a CA-issued one. Only the
//! parent's browser warns about it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use axum_server::tls_rustls::RustlsConfig;
use rcgen::CertifiedKey;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::info;

use crate::config::Config;
use crate::secret;

/// Host names in the generated certificate. Agents ignore them, since
/// they pin the certificate itself.
const GENERATED_NAMES: [&str; 2] = ["parentcontrol", "localhost"];

/// A certificate that cannot be loaded or created.
#[derive(Debug, Error)]
pub enum TlsError {
    /// A certificate or key file cannot be read or written.
    #[error("{}: {source}", .path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// Generating a self-signed certificate failed.
    #[error("generating a certificate: {0}")]
    Generate(#[from] rcgen::Error),
    /// The certificate file holds no PEM certificate.
    #[error("{}: no PEM certificate found", .0.display())]
    NoCertificate(PathBuf),
    /// rustls rejected the certificate or key.
    #[error("loading the certificate: {0}")]
    Rustls(io::Error),
}

/// The loaded certificate.
#[derive(Debug)]
pub(crate) struct Tls {
    pub(crate) config: RustlsConfig,
    /// SHA-256 of the leaf certificate in lowercase hex.
    pub(crate) fingerprint: String,
}

/// Loads the configured certificate, or the generated one, creating it
/// in the data directory if it does not exist yet.
pub(crate) async fn load(config: &Config) -> Result<Tls, TlsError> {
    let (cert, key) = if let (Some(cert), Some(key)) = (&config.tls_cert, &config.tls_key) {
        (cert.clone(), key.clone())
    } else {
        let dir = config.data_dir.join("tls");
        let paths = (dir.join("cert.pem"), dir.join("key.pem"));
        if !paths.0.exists() || !paths.1.exists() {
            generate(&dir, &paths.0, &paths.1)?;
        }
        paths
    };
    let cert_pem = read(&cert)?;
    let key_pem = read(&key)?;
    let fingerprint = fingerprint(&cert_pem).ok_or(TlsError::NoCertificate(cert))?;
    let config = RustlsConfig::from_pem(cert_pem, key_pem)
        .await
        .map_err(TlsError::Rustls)?;
    Ok(Tls {
        config,
        fingerprint,
    })
}

/// SHA-256 of the first certificate in `pem`, in lowercase hex.
pub(crate) fn fingerprint(pem: &[u8]) -> Option<String> {
    let certificate = CertificateDer::pem_slice_iter(pem).next()?.ok()?;
    Some(secret::hex(&Sha256::digest(certificate.as_ref())))
}

fn generate(dir: &Path, cert: &Path, key: &Path) -> Result<(), TlsError> {
    let io_error = |path: &Path| {
        let path = path.to_owned();
        move |source| TlsError::Io { path, source }
    };
    fs::create_dir_all(dir).map_err(io_error(dir))?;
    let names = GENERATED_NAMES.map(str::to_owned).to_vec();
    let CertifiedKey {
        cert: certificate,
        signing_key,
    } = rcgen::generate_simple_self_signed(names)?;
    fs::write(key, signing_key.serialize_pem()).map_err(io_error(key))?;
    fs::write(cert, certificate.pem()).map_err(io_error(cert))?;
    info!(path = %cert.display(), "generated a self-signed certificate");
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, TlsError> {
    fs::read(path).map_err(|source| TlsError::Io {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_certificate_has_a_stable_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let (cert, key) = (dir.path().join("cert.pem"), dir.path().join("key.pem"));
        generate(dir.path(), &cert, &key).unwrap();
        let pem = fs::read(&cert).unwrap();
        let first = fingerprint(&pem).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(fingerprint(&pem), Some(first));
    }

    #[test]
    fn garbage_has_no_fingerprint() {
        assert_eq!(fingerprint(b"not a certificate"), None);
    }
}
