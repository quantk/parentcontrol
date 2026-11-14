//! Linux implementation of the agent platform layer: `/proc` for processes,
//! signals for termination, `/etc/passwd` for account names.

mod passwd;
mod procfs;

use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use parentcontrol_core::enforcement::Termination;
use parentcontrol_core::process::{ProcessId, ProcessInfo, ProcessInspector};
use rustix::io::Errno;
use rustix::process::{Pid, Signal};
use tokio::signal::unix::{SignalKind, signal};

use super::{Platform, PlatformError};

/// The Linux platform.
#[derive(Debug, Default)]
pub(crate) struct Native;

impl Platform for Native {
    fn processes(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        let users = passwd::Users::load().map_err(PlatformError::io("reading /etc/passwd"))?;
        procfs::processes(&users).map_err(PlatformError::io("listing /proc"))
    }

    fn terminate(&self, process: ProcessId, mode: Termination) -> Result<(), PlatformError> {
        if !procfs::is_running(process) {
            return Ok(());
        }
        let Some(pid) = i32::try_from(process.pid()).ok().and_then(Pid::from_raw) else {
            return Ok(());
        };
        let signal = match mode {
            Termination::Graceful => Signal::TERM,
            Termination::Forced => Signal::KILL,
        };
        match rustix::process::kill_process(pid, signal) {
            Ok(()) | Err(Errno::SRCH) => Ok(()),
            Err(errno) => Err(PlatformError::Io {
                action: format!("signalling process {process}"),
                source: errno.into(),
            }),
        }
    }
}

impl ProcessInspector for Native {
    fn loaded_files(&self, id: ProcessId) -> Vec<PathBuf> {
        procfs::loaded_files(id)
    }
}

/// Where the agent keeps its state.
pub(crate) fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/parentcontrol")
}

/// Creates `dir` if needed and makes it accessible to root only.
pub(crate) fn create_private_dir(dir: &Path) -> Result<(), PlatformError> {
    let action = || format!("preparing {}", dir.display());
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(PlatformError::io(action()))?;
    // An existing directory keeps its mode on creation; tighten it.
    fs::set_permissions(dir, Permissions::from_mode(0o700)).map_err(PlatformError::io(action()))
}

/// Resolves when the service manager or an operator asks the agent to stop.
pub(crate) async fn shutdown_signal() -> io::Result<()> {
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = terminate.recv() => Ok(()),
        result = tokio::signal::ctrl_c() => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_dir_is_created_and_tightened() {
        let root = tempfile::tempdir().unwrap();
        let fresh = root.path().join("fresh/nested");
        create_private_dir(&fresh).unwrap();

        let loose = root.path().join("loose");
        fs::create_dir(&loose).unwrap();
        fs::set_permissions(&loose, Permissions::from_mode(0o755)).unwrap();
        create_private_dir(&loose).unwrap();

        for dir in [fresh, loose] {
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }
}
