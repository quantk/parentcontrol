//! Operating-system layer of the agent.
//!
//! The only place in the agent allowed to contain `#[cfg]` on the target OS
//! and `unsafe` code. Everything above it talks to the OS through the
//! [`Platform`] trait and the free functions re-exported here, which every
//! implementation provides with the same signatures.

use std::io;

use parentcontrol_core::enforcement::Termination;
use parentcontrol_core::process::{ProcessId, ProcessInfo, ProcessInspector};
use thiserror::Error;

#[cfg(target_os = "linux")]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{
    Native, PLATFORM, create_private_dir, default_data_dir, local_users, shutdown_signal,
};

#[cfg(windows)]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod windows;
#[cfg(windows)]
pub(crate) use windows::{
    Native, PLATFORM, create_private_dir, default_data_dir, local_users, shutdown_signal,
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
    /// An operation this OS does not support yet.
    #[cfg_attr(
        not(windows),
        expect(dead_code, reason = "only Windows lacks operations")
    )]
    pub(crate) fn unsupported(action: &str) -> Self {
        Self::Io {
            action: action.to_owned(),
            source: io::ErrorKind::Unsupported.into(),
        }
    }

    /// Wraps an I/O error with a description of the failed action.
    #[cfg_attr(windows, expect(dead_code, reason = "Windows has no I/O calls yet"))]
    pub(crate) fn io(action: impl Into<String>) -> impl FnOnce(io::Error) -> Self {
        let action = action.into();
        move |source| Self::Io { action, source }
    }
}

/// Process operations the agent needs from the OS.
pub(crate) trait Platform: ProcessInspector {
    /// Every process currently running, of every user.
    ///
    /// Processes that vanish while being read are skipped.
    fn processes(&self) -> Result<Vec<ProcessInfo>, PlatformError>;

    /// Ends `process`. Succeeds if it is already gone, including when its
    /// PID now belongs to another process.
    fn terminate(&self, process: ProcessId, mode: Termination) -> Result<(), PlatformError>;
}
