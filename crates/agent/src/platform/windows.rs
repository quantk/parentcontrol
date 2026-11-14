//! Windows implementation of the agent platform layer.
//!
//! Process listing, termination and protected storage arrive with the
//! Windows service. Until then every operation reports `Unsupported`, and
//! the agent refuses to start because it cannot create its data directory.

use std::io;
use std::path::{Path, PathBuf};

use parentcontrol_core::enforcement::Termination;
use parentcontrol_core::process::{ProcessId, ProcessInfo, ProcessInspector};

use super::{Platform, PlatformError};

/// The Windows platform.
#[derive(Debug, Default)]
pub(crate) struct Native;

impl Platform for Native {
    fn processes(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        Err(PlatformError::unsupported("listing processes"))
    }

    fn terminate(&self, _: ProcessId, _: Termination) -> Result<(), PlatformError> {
        Err(PlatformError::unsupported("terminating processes"))
    }
}

impl ProcessInspector for Native {
    fn loaded_files(&self, _: ProcessId) -> Vec<PathBuf> {
        Vec::new()
    }
}

/// Where the agent keeps its state.
pub(crate) fn default_data_dir() -> PathBuf {
    let program_data = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    PathBuf::from(program_data).join("ParentControl")
}

/// Creates `dir` accessible to administrators and SYSTEM only.
pub(crate) fn create_private_dir(_: &Path) -> Result<(), PlatformError> {
    Err(PlatformError::unsupported("protected data directory"))
}

/// Resolves when an operator asks the agent to stop.
pub(crate) async fn shutdown_signal() -> io::Result<()> {
    tokio::signal::ctrl_c().await
}
