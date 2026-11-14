//! The named pipe trays connect to.

use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::time::Duration;
use std::{io, mem};

use parentcontrol_core::process::OsUser;
use parentcontrol_core::tray::WINDOWS_PIPE;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc;
use tracing::warn;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

use super::super::TrayClient;
use super::accounts::process_owner;
use super::security::SecurityDescriptor;

/// `SYSTEM` and administrators may do anything; signed-in users may
/// connect, read and write. Without this, the default pipe permissions let
/// users only read, and the tray could not confirm deliveries.
const PIPE_ACCESS: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)";

/// Listens for trays and hands each connection, with the account of the
/// process on the other end, to `clients`.
pub(super) fn listen(clients: mpsc::Sender<TrayClient>) -> io::Result<()> {
    let descriptor = SecurityDescriptor::from_sddl(PIPE_ACCESS)?;
    // Claiming the first instance fails if anyone else already owns the
    // name, so no other process can pose as the agent.
    let first = create(&descriptor, true)?;
    tokio::spawn(async move {
        let mut server = first;
        loop {
            if let Err(error) = server.connect().await {
                warn!(%error, "cannot accept a tray connection");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            let next = match create(&descriptor, false) {
                Ok(next) => next,
                Err(error) => {
                    warn!(%error, "cannot create the tray pipe; trays will not connect");
                    return;
                }
            };
            let connected = mem::replace(&mut server, next);
            let user = client_user(&connected);
            if clients
                .send(TrayClient {
                    user,
                    stream: Box::new(connected),
                })
                .await
                .is_err()
            {
                return;
            }
        }
    });
    Ok(())
}

fn create(descriptor: &SecurityDescriptor, first: bool) -> io::Result<NamedPipeServer> {
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
        lpSecurityDescriptor: descriptor.as_ptr(),
        bInheritHandle: 0,
    };
    // SAFETY: `attributes` is a valid `SECURITY_ATTRIBUTES` whose
    // descriptor outlives the call.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .create_with_security_attributes_raw(WINDOWS_PIPE, (&raw mut attributes).cast())
    }
}

/// The account running the process on the other end of the pipe.
fn client_user(pipe: &NamedPipeServer) -> Option<OsUser> {
    let mut pid = 0;
    // SAFETY: the pipe handle is valid and connected.
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid) } == 0 {
        return None;
    }
    // SAFETY: plain call; a null result means failure.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return None;
    }
    // SAFETY: a non-null result is a process handle we now own.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    process_owner(process.as_raw_handle()).map(OsUser::new)
}
