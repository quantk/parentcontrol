//! Windows implementation of the agent platform layer: Toolhelp and NT
//! queries for processes, `TerminateProcess`, the service control manager,
//! an administrators-only data directory.

mod accounts;
mod install;
mod processes;
mod security;
mod service;
mod wide;

use std::io;
use std::path::{Path, PathBuf};

use parentcontrol_core::enforcement::Termination;
use parentcontrol_core::process::{OsUser, ProcessId, ProcessInfo, ProcessInspector};
use parentcontrol_core::protocol::DevicePlatform;

use super::{Platform, PlatformError, ServiceBody};

/// The Windows platform.
#[derive(Debug, Default)]
pub(crate) struct Native;

impl Platform for Native {
    fn processes(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        processes::list().map_err(PlatformError::io("listing processes"))
    }

    fn terminate(&self, process: ProcessId, mode: Termination) -> Result<(), PlatformError> {
        match mode {
            // A service cannot reach windows in the user's session; the
            // tray closes them. Without it the forced termination follows
            // after the grace period.
            Termination::Graceful => Ok(()),
            Termination::Forced => processes::terminate(process)
                .map_err(PlatformError::io(format!("terminating process {process}"))),
        }
    }
}

impl ProcessInspector for Native {
    /// Not available yet: the JVM reads jar files through ordinary file
    /// handles, and listing another process's handles needs undocumented
    /// system calls. Detection relies on the command line here.
    fn loaded_files(&self, _: ProcessId) -> Vec<PathBuf> {
        Vec::new()
    }
}

/// This OS, as reported to the server.
pub(crate) const PLATFORM: DevicePlatform = DevicePlatform::Windows;

/// Accounts of people the parent may bind to the child's profile.
pub(crate) fn local_users() -> Vec<OsUser> {
    accounts::people()
}

/// Where the agent keeps its state.
pub(crate) fn default_data_dir() -> PathBuf {
    let program_data = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    PathBuf::from(program_data).join("ParentControl")
}

/// Creates `dir` accessible to administrators and SYSTEM only.
pub(crate) fn create_private_dir(dir: &Path) -> Result<(), PlatformError> {
    security::create_private_dir(dir)
        .map_err(PlatformError::io(format!("protecting {}", dir.display())))
}

/// Resolves when the service manager or an operator asks the agent to stop.
pub(crate) async fn shutdown_signal() -> io::Result<()> {
    // A service has no console, so Ctrl+C may be unavailable; that must
    // not count as a request to stop.
    let interrupted = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    tokio::select! {
        () = interrupted => Ok(()),
        () = service::stop_requested() => Ok(()),
    }
}

/// Runs `body` as the Windows service.
pub(crate) fn run_service(body: ServiceBody) -> Result<(), PlatformError> {
    service::run(body)
}

/// Installs the service and the tray autostart, or upgrades them.
pub(crate) fn install() -> Result<(), PlatformError> {
    install::install()
}

/// Removes the service and the tray autostart.
pub(crate) fn uninstall() -> Result<(), PlatformError> {
    install::uninstall()
}
