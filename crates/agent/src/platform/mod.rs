//! Operating-system layer of the agent.
//!
//! The only place in the agent allowed to contain `#[cfg]` on the target OS
//! and `unsafe` code. Everything above it talks to the OS through the
//! [`Platform`] trait and the free functions re-exported here, which every
//! implementation provides with the same signatures.

use std::{fmt, io};

use parentcontrol_core::enforcement::Termination;
use parentcontrol_core::process::{OsUser, ProcessId, ProcessInfo, ProcessInspector};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(target_os = "linux")]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{
    Native, PLATFORM, create_private_dir, default_data_dir, install, listen_for_trays, local_users,
    run_service, shutdown_signal, uninstall,
};

#[cfg(windows)]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod windows;
#[cfg(windows)]
pub(crate) use windows::{
    Native, PLATFORM, create_private_dir, default_data_dir, install, listen_for_trays, local_users,
    run_service, shutdown_signal, uninstall,
};

/// A failed OS operation.
#[derive(Debug, Error)]
pub(crate) enum PlatformError {
    /// An I/O call failed.
    #[error("{action}: {source}")]
    Io {
        /// What was being done.
        action: String,
        /// The underlying error.
        source: io::Error,
    },
}

impl PlatformError {
    /// Wraps an I/O error with a description of the failed action.
    pub(crate) fn io(action: impl Into<String>) -> impl FnOnce(io::Error) -> Self {
        let action = action.into();
        move |source| Self::Io { action, source }
    }
}

/// The agent's work when run as a system service.
pub(crate) type ServiceBody = Box<dyn FnOnce() -> Result<(), Box<dyn std::error::Error>> + Send>;

/// Process operations the agent needs from the OS.
pub(crate) trait Platform: ProcessInspector {
    /// Every process currently running, of every user.
    ///
    /// Processes that vanish while being read are skipped.
    fn processes(&self) -> Result<Vec<ProcessInfo>, PlatformError>;

    /// Ends `process`. Succeeds if it is already gone, including when its
    /// PID now belongs to another process.
    fn terminate(&self, process: ProcessId, mode: Termination) -> Result<(), PlatformError>;

    /// Whether [`Termination::Graceful`] works from the agent itself. If
    /// not, the tray in the owner's session closes the windows instead.
    fn closes_softly(&self) -> bool;
}

/// A byte stream to one tray.
pub(crate) trait TrayStream: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> TrayStream for T {}

/// A tray that connected to the agent.
pub(crate) struct TrayClient {
    /// The account the tray runs as, if the OS could tell.
    pub(crate) user: Option<OsUser>,
    pub(crate) stream: Box<dyn TrayStream>,
}

impl fmt::Debug for TrayClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrayClient")
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}
