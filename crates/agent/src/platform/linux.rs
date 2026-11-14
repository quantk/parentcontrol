//! Linux implementation of the agent platform layer: `/proc` for processes,
//! signals for termination, `/etc/passwd` for account names.

mod passwd;
mod procfs;

use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use parentcontrol_core::enforcement::Termination;
use parentcontrol_core::process::{OsUser, ProcessId, ProcessInfo, ProcessInspector};
use parentcontrol_core::protocol::DevicePlatform;
use parentcontrol_core::tray;
use rustix::io::Errno;
use rustix::process::{Pid, Signal};
use tokio::net::UnixListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tracing::warn;

use super::{Platform, PlatformError, ServiceBody, TrayClient};

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

    fn closes_softly(&self) -> bool {
        // `SIGTERM` lets a program save and quit.
        true
    }
}

impl ProcessInspector for Native {
    fn loaded_files(&self, id: ProcessId) -> Vec<PathBuf> {
        procfs::loaded_files(id)
    }
}

/// This OS, as reported to the server.
pub(crate) const PLATFORM: DevicePlatform = DevicePlatform::Linux;

/// Accounts of people, as opposed to system services, that the parent may
/// bind to the child's profile.
pub(crate) fn local_users() -> Vec<OsUser> {
    match passwd::Users::load() {
        Ok(users) => users.people(),
        Err(error) => {
            warn!(%error, "cannot list local accounts");
            Vec::new()
        }
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

/// Runs `body`; systemd needs no special protocol for a simple service.
pub(crate) fn run_service(body: ServiceBody) -> Result<(), PlatformError> {
    body().map_err(|error| PlatformError::Io {
        action: "running the agent".to_owned(),
        source: io::Error::other(error.to_string()),
    })
}

/// Installation on Linux uses the systemd unit or the NixOS module.
pub(crate) fn install() -> Result<(), PlatformError> {
    Err(unsupported(
        "installing (use the systemd unit or the NixOS module)",
    ))
}

/// Removal on Linux uses the package manager or systemd.
pub(crate) fn uninstall() -> Result<(), PlatformError> {
    Err(unsupported("uninstalling (disable the systemd unit)"))
}

fn unsupported(action: &str) -> PlatformError {
    PlatformError::Io {
        action: action.to_owned(),
        source: io::ErrorKind::Unsupported.into(),
    }
}

/// Listens for trays on the Unix socket and hands each connection, with
/// the account it comes from, to `clients`.
pub(crate) fn listen_for_trays(clients: mpsc::Sender<TrayClient>) -> Result<(), PlatformError> {
    let path = std::env::var_os(tray::SOCKET_ENV)
        .map_or_else(|| PathBuf::from(tray::UNIX_SOCKET), PathBuf::from);
    let action = || format!("listening on {}", path.display());
    if let Some(dir) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(dir)
            .map_err(PlatformError::io(action()))?;
    }
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(PlatformError::io(action())(error)),
    }
    let listener = UnixListener::bind(&path).map_err(PlatformError::io(action()))?;
    // Every user's tray may connect; what it learns is only shown to them.
    fs::set_permissions(&path, Permissions::from_mode(0o666))
        .map_err(PlatformError::io(action()))?;
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let uid = stream.peer_cred().ok().map(|credentials| credentials.uid());
                    let user = uid.and_then(|uid| passwd::Users::load().ok()?.name(uid).cloned());
                    if clients
                        .send(TrayClient {
                            user,
                            stream: Box::new(stream),
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    warn!(%error, "cannot accept a tray connection");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    });
    Ok(())
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
