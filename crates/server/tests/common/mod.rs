//! A real server in the test process and minimal clients for it: HTTP
//! for the panel and WebSocket for the agent protocol, both over TLS with
//! the server's certificate pinned by fingerprint.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use parentcontrol_core::protocol::{AgentMessage, ServerMessage};
use parentcontrol_server::{Config, Server, set_password};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

pub(crate) const PASSWORD: &str = "correct horse battery";

/// How long to wait for a pushed message before failing.
pub(crate) const PUSH_TIMEOUT: Duration = Duration::from_secs(5);

/// A server on a random local port with its own data directory.
pub(crate) struct TestServer {
    pub(crate) address: SocketAddr,
    pub(crate) fingerprint: String,
    _data: TempDir,
}

impl TestServer {
    pub(crate) async fn start() -> Self {
        let data = tempfile::tempdir().unwrap();
        set_password(data.path(), PASSWORD).await.unwrap();
        let config = Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            data_dir: data.path().to_owned(),
            time_zone: jiff::tz::TimeZone::get("Europe/Moscow").unwrap(),
            day_start: "04:00".parse().unwrap(),
            tls_cert: None,
            tls_key: None,
        };
        let server = Server::start(config).await.unwrap();
        let (address, fingerprint) = (server.address(), server.fingerprint().to_owned());
        tokio::spawn(server.run_until(std::future::pending()));
        Self {
            address,
            fingerprint,
            _data: data,
        }
    }

    async fn connect(&self) -> TlsStream<TcpStream> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = Pinned {
            fingerprint: self.fingerprint.clone(),
            provider: Arc::clone(&provider),
        };
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        let tcp = TcpStream::connect(self.address).await.unwrap();
        let name = ServerName::try_from("parentcontrol").unwrap();
        TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
            .unwrap()
    }

    /// Sends one HTTP request and returns status, headers and body.
    pub(crate) async fn http(
        &self,
        method: &str,
        path: &str,
        cookie: Option<&str>,
        form: &[(&str, &str)],
    ) -> Response {
        let body = serde_urlencoded::to_string(form).unwrap();
        let mut request =
            format!("{method} {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
        if let Some(cookie) = cookie {
            let _ = write!(request, "Cookie: {cookie}\r\n");
        }
        if method == "POST" {
            request.push_str("Content-Type: application/x-www-form-urlencoded\r\n");
            let _ = write!(request, "Content-Length: {}\r\n", body.len());
        }
        request.push_str("\r\n");
        request.push_str(&body);

        let mut stream = self.connect().await;
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        // The server may close without TLS close_notify; what arrived is enough.
        let _ = stream.read_to_end(&mut raw).await;
        Response::parse(&String::from_utf8_lossy(&raw))
    }

    /// Logs in and returns the session cookie.
    pub(crate) async fn login(&self) -> String {
        let response = self
            .http("POST", "/login", None, &[("password", PASSWORD)])
            .await;
        let cookie = response.header("set-cookie").expect("no session cookie");
        cookie.split(';').next().unwrap().to_owned()
    }

    /// Opens the agent WebSocket.
    pub(crate) async fn agent(&self) -> Agent {
        let url = format!("wss://{}/api/agent", self.address);
        let (socket, _) = tokio_tungstenite::client_async(url, self.connect().await)
            .await
            .unwrap();
        Agent(socket)
    }
}

pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
}

impl Response {
    fn parse(raw: &str) -> Self {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .collect();
        Self {
            status,
            headers,
            body: body.to_owned(),
        }
    }

    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// An agent's side of the protocol.
pub(crate) struct Agent(WebSocketStream<TlsStream<TcpStream>>);

impl Agent {
    pub(crate) async fn send(&mut self, message: &AgentMessage) {
        let json = serde_json::to_string(message).unwrap();
        self.0.send(Message::text(json)).await.unwrap();
    }

    /// The next server message, failing the test after [`PUSH_TIMEOUT`].
    pub(crate) async fn receive(&mut self) -> ServerMessage {
        loop {
            let frame = tokio::time::timeout(PUSH_TIMEOUT, self.0.next())
                .await
                .expect("no message from the server in time")
                .expect("connection closed")
                .unwrap();
            if let Message::Text(text) = frame {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    /// Skips messages until one satisfies `wanted`.
    pub(crate) async fn receive_until(
        &mut self,
        mut wanted: impl FnMut(&ServerMessage) -> bool,
    ) -> ServerMessage {
        loop {
            let message = self.receive().await;
            if wanted(&message) {
                return message;
            }
        }
    }
}

/// Accepts only the certificate with the expected fingerprint, as the
/// agent does.
#[derive(Debug)]
struct Pinned {
    fingerprint: String,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let actual =
            Sha256::digest(end_entity.as_ref())
                .iter()
                .fold(String::new(), |mut text, byte| {
                    let _ = write!(text, "{byte:02x}");
                    text
                });
        if actual == self.fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("fingerprint mismatch".to_owned()))
        }
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
