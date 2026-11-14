//! One agent's WebSocket session.

use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use jiff::Timestamp;
use parentcontrol_core::ids::DeviceId;
use parentcontrol_core::process::OsUser;
use parentcontrol_core::protocol::{
    AgentMessage, AgentToken, DevicePlatform, PROTOCOL_VERSION, Rejection, ServerMessage,
};
use tracing::{info, warn};

use super::hub::ConnectionId;
use super::policy::device_policy;
use crate::db::{DbError, devices, usage};
use crate::secret;
use crate::state::AppState;

/// How long an agent may take to introduce itself.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Silence after which an agent counts as gone; agents report at least
/// every 30 seconds.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Accepts an agent's WebSocket.
pub(crate) async fn upgrade(socket: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    socket.on_upgrade(move |socket| serve(socket, state))
}

/// Why a session ended other than by the agent leaving.
#[derive(Debug, thiserror::Error)]
enum SessionError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("sending: {0}")]
    Send(axum::Error),
    #[error("generating a token: {0}")]
    Random(#[from] getrandom::Error),
    #[error("agent misbehaved: {0}")]
    Protocol(String),
}

async fn serve(mut socket: WebSocket, state: AppState) {
    let first = match tokio::time::timeout(HANDSHAKE_TIMEOUT, receive(&mut socket)).await {
        Ok(Some(Ok(message))) => message,
        Ok(Some(Err(reason))) => {
            warn!(%reason, "agent sent garbage");
            return;
        }
        Ok(None) | Err(_) => return,
    };
    let result = match first {
        AgentMessage::Enroll { code, platform } => {
            enroll(&mut socket, &state, &code, platform).await
        }
        AgentMessage::Hello {
            token,
            protocol,
            platform,
            local_users,
        } => {
            session(
                &mut socket,
                &state,
                Hello {
                    token,
                    protocol,
                    platform,
                    local_users,
                },
            )
            .await
        }
        AgentMessage::Report { .. } => reject(&mut socket, Rejection::UnexpectedMessage).await,
    };
    if let Err(error) = result {
        warn!(%error, "agent session ended");
    }
}

async fn enroll(
    socket: &mut WebSocket,
    state: &AppState,
    code: &str,
    platform: DevicePlatform,
) -> Result<(), SessionError> {
    let code_hash = secret::hash(&secret::normalize_code(code));
    let token = secret::token()?;
    let token_hash = secret::hash(&token);
    let db = state.db.clone();
    let mut device = None;
    state
        .throttle
        .check(async {
            device = db
                .call(move |c| {
                    devices::enroll(c, &code_hash, &token_hash, platform, Timestamp::now())
                })
                .await?;
            Ok::<_, DbError>(device.is_some())
        })
        .await?;
    let Some(device) = device else {
        return reject(socket, Rejection::InvalidCode).await;
    };
    info!(%device, %platform, "device enrolled");
    state.hub.policy_changed();
    send(
        socket,
        &ServerMessage::Enrolled {
            device,
            token: AgentToken::new(token),
        },
    )
    .await
}

struct Hello {
    token: AgentToken,
    protocol: u32,
    platform: DevicePlatform,
    local_users: Vec<OsUser>,
}

async fn session(
    socket: &mut WebSocket,
    state: &AppState,
    hello: Hello,
) -> Result<(), SessionError> {
    if hello.protocol != PROTOCOL_VERSION {
        return reject(socket, Rejection::UnsupportedProtocol).await;
    }
    let token_hash = secret::hash(hello.token.expose());
    let db = state.db.clone();
    let mut device = None;
    state
        .throttle
        .check(async {
            device = db
                .call(move |c| devices::authenticate(c, &token_hash))
                .await?;
            Ok::<_, DbError>(device.is_some())
        })
        .await?;
    let Some(device) = device else {
        return reject(socket, Rejection::InvalidToken).await;
    };

    let (platform, users) = (hello.platform, hello.local_users);
    state
        .db
        .call(move |c| devices::record_hello(c, device, platform, &users, Timestamp::now()))
        .await?;
    let connection = state.hub.connected(device, Timestamp::now());
    info!(%device, "agent connected");
    let result = exchange(socket, state, device, connection).await;
    state.hub.disconnected(device, connection);
    info!(%device, "agent disconnected");
    result
}

/// Pushes policies and accepts reports until the agent leaves.
async fn exchange(
    socket: &mut WebSocket,
    state: &AppState,
    device: DeviceId,
    connection: ConnectionId,
) -> Result<(), SessionError> {
    let mut changes = state.hub.subscribe();
    if !push_policy(socket, state, device).await? {
        return Ok(());
    }
    loop {
        tokio::select! {
            message = tokio::time::timeout(IDLE_TIMEOUT, receive(socket)) => match message {
                Err(_) => return Err(SessionError::Protocol("silent for too long".to_owned())),
                Ok(None) => return Ok(()),
                Ok(Some(Err(reason))) => return Err(SessionError::Protocol(reason)),
                Ok(Some(Ok(AgentMessage::Report { records, active_groups }))) => {
                    let had_records = !records.is_empty();
                    let records = state
                        .db
                        .call(move |c| {
                            let now = Timestamp::now();
                            devices::touch(c, device, now)?;
                            usage::merge(c, device, &records)
                        })
                        .await?;
                    send(socket, &ServerMessage::Acknowledged { records }).await?;
                    let active_changed = state.hub.set_active(device, connection, active_groups);
                    if had_records || active_changed {
                        // Other devices' remaining time depends on this usage.
                        state.hub.policy_changed();
                    }
                }
                Ok(Some(Ok(_))) => return reject(socket, Rejection::UnexpectedMessage).await,
            },
            notified = changes.changed() => {
                if notified.is_err() || !push_policy(socket, state, device).await? {
                    return Ok(());
                }
            }
        }
    }
}

/// Sends the current policy; `false` if the device no longer exists, in
/// which case the agent has been told so.
async fn push_policy(
    socket: &mut WebSocket,
    state: &AppState,
    device: DeviceId,
) -> Result<bool, SessionError> {
    let policy = state
        .db
        .call(move |c| device_policy(c, device, Timestamp::now()))
        .await?;
    let Some(policy) = policy else {
        reject(socket, Rejection::InvalidToken).await?;
        return Ok(false);
    };
    let message = ServerMessage::Policy {
        policy: Box::new(policy),
        server_time: Timestamp::now(),
    };
    send(socket, &message).await?;
    Ok(true)
}

async fn reject(socket: &mut WebSocket, reason: Rejection) -> Result<(), SessionError> {
    send(socket, &ServerMessage::Rejected { reason }).await?;
    Err(SessionError::Protocol(reason.to_string()))
}

async fn send(socket: &mut WebSocket, message: &ServerMessage) -> Result<(), SessionError> {
    let json = serde_json::to_string(message)
        .map_err(|error| SessionError::Protocol(error.to_string()))?;
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(SessionError::Send)
}

/// The next agent message; `None` when the connection closed, `Some(Err)`
/// for a frame that is not a valid message.
async fn receive(socket: &mut WebSocket) -> Option<Result<AgentMessage, String>> {
    loop {
        match socket.recv().await? {
            Ok(Message::Text(text)) => {
                return Some(serde_json::from_str(&text).map_err(|error| error.to_string()));
            }
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_)) => {}
        }
    }
}
