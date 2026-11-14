//! Joining a server with a code from the parent's panel.

use std::sync::{Arc, Mutex, PoisonError};

use parentcontrol_core::ids::DeviceId;
use parentcontrol_core::protocol::{AgentMessage, ServerMessage};
use thiserror::Error;

use crate::link::{Endpoint, Link};
use crate::platform;
use crate::store::{Store, StoreError};
use crate::sync::tls::Trust;
use crate::sync::{SyncError, open, receive, send};

/// A failed enrollment.
#[derive(Debug, Error)]
pub(crate) enum EnrollError {
    /// The exchange with the server failed or was refused.
    #[error(transparent)]
    Sync(#[from] SyncError),
    /// The link could not be saved.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// A completed enrollment.
#[derive(Debug)]
pub(crate) struct Enrolled {
    pub(crate) device: DeviceId,
    /// Fingerprint of the server certificate, now pinned.
    pub(crate) fingerprint: String,
}

/// Trades `code` for a token from `server` and stores the link.
///
/// The certificate is trusted on first use; the caller shows its
/// fingerprint so the parent can compare it with the panel.
pub(crate) async fn enroll(
    store: &Store,
    server: Endpoint,
    code: &str,
) -> Result<Enrolled, EnrollError> {
    let seen = Arc::new(Mutex::new(None));
    let mut socket = open(&server, Trust::FirstUse(Arc::clone(&seen))).await?;
    send(
        &mut socket,
        &AgentMessage::Enroll {
            code: code.to_owned(),
            platform: platform::PLATFORM,
        },
    )
    .await?;
    let (device, token) = match receive(&mut socket).await? {
        Some(ServerMessage::Enrolled { device, token }) => (device, token),
        Some(ServerMessage::Rejected { reason }) => return Err(SyncError::Rejected(reason).into()),
        _ => return Err(SyncError::Unexpected.into()),
    };
    // The handshake has completed, so the verifier has run.
    let fingerprint = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or(SyncError::Unexpected)?;
    store.save_link(&Link {
        server,
        token,
        fingerprint: fingerprint.clone(),
    })?;
    Ok(Enrolled {
        device,
        fingerprint,
    })
}
