//! One WebSocket connection to the server.

use std::io;

use futures_util::{SinkExt as _, StreamExt as _};
use parentcontrol_core::protocol::{AgentMessage, Rejection, ServerMessage};
use rustls::pki_types::ServerName;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::{self, Message};

use super::tls::{self, Trust};
use crate::link::Endpoint;

/// The agent's socket.
pub(crate) type Socket = WebSocketStream<TlsStream<TcpStream>>;

/// A failed exchange with the server.
#[derive(Debug, Error)]
pub(crate) enum SyncError {
    /// The server is unreachable.
    #[error("connecting to the server: {0}")]
    Connect(io::Error),
    /// The TLS setup failed, including a certificate that is not pinned.
    #[error("TLS: {0}")]
    Tls(#[from] rustls::Error),
    /// The host is not a valid TLS server name.
    #[error("invalid server name: {0}")]
    Name(#[from] rustls::pki_types::InvalidDnsNameError),
    /// The WebSocket failed.
    #[error("WebSocket: {0}")]
    Socket(#[from] tungstenite::Error),
    /// The server sent something that is not a protocol message.
    #[error("unreadable message from the server: {0}")]
    Garbage(#[from] serde_json::Error),
    /// The server refused the agent.
    #[error("the server refused the agent: {0}")]
    Rejected(Rejection),
    /// The server sent a message that makes no sense now.
    #[error("unexpected message from the server")]
    Unexpected,
    /// The server stayed silent for too long.
    #[error("the server stopped answering")]
    Timeout,
}

/// Connects to `endpoint` trusting the certificate as `trust` says.
pub(crate) async fn open(endpoint: &Endpoint, trust: Trust) -> Result<Socket, SyncError> {
    let connector = tls::connector(trust)?;
    let tcp = TcpStream::connect(endpoint.address())
        .await
        .map_err(SyncError::Connect)?;
    let name = ServerName::try_from(endpoint.host().to_owned())?;
    let stream = connector
        .connect(name, tcp)
        .await
        .map_err(SyncError::Connect)?;
    let (socket, _) = tokio_tungstenite::client_async(endpoint.websocket_url(), stream).await?;
    Ok(socket)
}

/// Sends one message.
pub(crate) async fn send(socket: &mut Socket, message: &AgentMessage) -> Result<(), SyncError> {
    let json = serde_json::to_string(message)?;
    socket.send(Message::text(json)).await?;
    Ok(())
}

/// The next server message, or `None` once the server closed.
pub(crate) async fn receive(socket: &mut Socket) -> Result<Option<ServerMessage>, SyncError> {
    while let Some(frame) = socket.next().await {
        match frame? {
            Message::Text(text) => return Ok(Some(serde_json::from_str(&text)?)),
            Message::Close(_) => return Ok(None),
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
        }
    }
    Ok(None)
}
