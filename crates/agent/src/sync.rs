//! Keeping the local snapshot in sync with the server.
//!
//! A background task holds one WebSocket to the server, reconnecting with
//! backoff. It forwards policies and acknowledgements to the main loop and
//! uploads the latest usage report. Decisions never wait for it: the main
//! loop works from the local snapshot whether or not the server is there.

mod connection;
pub(crate) mod tls;

use std::path::PathBuf;
use std::time::Duration;

use jiff::Timestamp;
use parentcontrol_core::ids::GroupId;
use parentcontrol_core::policy::Policy;
use parentcontrol_core::protocol::{
    Acknowledgement, AgentMessage, PROTOCOL_VERSION, ServerMessage,
};
use parentcontrol_core::usage::UsageRecord;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tracing::{error, info, warn};

pub(crate) use self::connection::{SyncError, open, receive, send};
use self::tls::Trust;
use crate::link::Link;
use crate::platform;
use crate::store::read_json;

/// First reconnection delay; doubles up to [`MAX_BACKOFF`].
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_mins(1);

/// A connection that lasted this long resets the backoff.
const STABLE: Duration = Duration::from_mins(1);

/// Retry delay after the server refused the agent, in case it was enrolled
/// again with a new token meanwhile.
const AFTER_REJECTION: Duration = Duration::from_mins(5);

/// Delay before looking for a link again on a device not enrolled yet.
const NOT_ENROLLED: Duration = Duration::from_secs(30);

/// How often the report is resent unchanged, as a heartbeat.
const HEARTBEAT: Duration = Duration::from_secs(30);

/// Silence after which the server counts as gone; it acknowledges every
/// report, so it is never silent this long on a live connection.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// What the agent tells the server.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Report {
    /// Usage not yet confirmed in full.
    pub(crate) records: Vec<UsageRecord>,
    /// Groups running now.
    pub(crate) active_groups: Vec<GroupId>,
}

/// What the server told the agent, or what happened to the connection.
#[derive(Debug)]
pub(crate) enum Event {
    /// The connection to the server came up (`true`) or went down.
    Online(bool),
    /// A new policy to apply.
    Policy {
        policy: Policy,
        server_time: Timestamp,
    },
    /// Usage the server has stored.
    Acknowledged(Vec<Acknowledgement>),
}

/// Starts syncing using the link stored at `link_path`, reread before
/// every connection so a new enrollment takes effect without a restart.
pub(crate) fn spawn(
    link_path: PathBuf,
    events: mpsc::Sender<Event>,
    reports: watch::Receiver<Report>,
) {
    tokio::spawn(run(link_path, events, reports));
}

async fn run(
    link_path: PathBuf,
    events: mpsc::Sender<Event>,
    mut reports: watch::Receiver<Report>,
) {
    let mut backoff = MIN_BACKOFF;
    loop {
        let link = match read_json::<Link>(&link_path) {
            Ok(Some(link)) => link,
            Ok(None) => {
                tokio::time::sleep(NOT_ENROLLED).await;
                continue;
            }
            Err(error) => {
                error!(%error, "cannot read the server link");
                tokio::time::sleep(NOT_ENROLLED).await;
                continue;
            }
        };
        let started = Instant::now();
        let ended = session(&link, &events, &mut reports).await;
        if events.send(Event::Online(false)).await.is_err() {
            return;
        }
        let delay = match ended {
            // The main loop is gone; the agent is stopping.
            Ok(Closed::Agent) => return,
            Ok(Closed::Server) => {
                info!("the server closed the connection");
                backoff
            }
            Err(SyncError::Rejected(reason)) => {
                error!(%reason, "the server refused the agent; limits stay in force from the last policy");
                AFTER_REJECTION
            }
            Err(error) => {
                warn!(%error, server = %link.server, "no connection to the server; working offline");
                backoff
            }
        };
        backoff = if started.elapsed() >= STABLE {
            MIN_BACKOFF
        } else {
            (backoff * 2).min(MAX_BACKOFF)
        };
        tokio::time::sleep(delay).await;
    }
}

/// Who ended a session normally.
enum Closed {
    Server,
    Agent,
}

async fn session(
    link: &Link,
    events: &mpsc::Sender<Event>,
    reports: &mut watch::Receiver<Report>,
) -> Result<Closed, SyncError> {
    let mut socket = open(&link.server, Trust::Pinned(link.fingerprint.clone())).await?;
    let hello = AgentMessage::Hello {
        token: link.token.clone(),
        protocol: PROTOCOL_VERSION,
        platform: platform::PLATFORM,
        local_users: platform::local_users(),
    };
    send(&mut socket, &hello).await?;
    info!(server = %link.server, "connected to the server");
    if events.send(Event::Online(true)).await.is_err() {
        return Ok(Closed::Agent);
    }

    // The first tick fires at once and sends the current report.
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    loop {
        tokio::select! {
            message = tokio::time::timeout(IDLE_TIMEOUT, receive(&mut socket)) => {
                let event = match message.map_err(|_| SyncError::Timeout)?? {
                    None => return Ok(Closed::Server),
                    Some(ServerMessage::Policy { policy, server_time }) => Event::Policy { policy: *policy, server_time },
                    Some(ServerMessage::Acknowledged { records }) => Event::Acknowledged(records),
                    Some(ServerMessage::Rejected { reason }) => return Err(SyncError::Rejected(reason)),
                    Some(ServerMessage::Enrolled { .. }) => return Err(SyncError::Unexpected),
                };
                if events.send(event).await.is_err() {
                    return Ok(Closed::Agent);
                }
            }
            changed = reports.changed() => {
                if changed.is_err() {
                    return Ok(Closed::Agent);
                }
                upload(&mut socket, reports).await?;
            }
            _ = heartbeat.tick() => upload(&mut socket, reports).await?,
        }
    }
}

async fn upload(
    socket: &mut connection::Socket,
    reports: &mut watch::Receiver<Report>,
) -> Result<(), SyncError> {
    let Report {
        records,
        active_groups,
    } = reports.borrow_and_update().clone();
    send(
        socket,
        &AgentMessage::Report {
            records,
            active_groups,
        },
    )
    .await
}
