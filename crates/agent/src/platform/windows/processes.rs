//! Listing and terminating processes.
//!
//! The service runs as `LocalSystem`, so it can open every process of the
//! child's session with limited query rights. Protected system processes
//! that refuse even that are skipped: they are never the child's programs.

use std::ffi::c_void;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::PathBuf;
use std::{io, ptr};

use parentcontrol_core::process::{OsUser, ProcessId, ProcessInfo};
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree, STATUS_INFO_LENGTH_MISMATCH, UNICODE_STRING,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, QueryFullProcessImageNameW,
    TerminateProcess,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

use super::accounts::process_owner;
use super::wide::{from_wide, from_wide_ptr};

/// The idle and system pseudo-processes, which cannot be opened.
const PSEUDO_PROCESSES: [u32; 2] = [0, 4];

/// Every process that could be opened.
pub(super) fn list() -> io::Result<Vec<ProcessInfo>> {
    // SAFETY: plain call; the returned handle is checked below.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a valid snapshot handle we own from here on.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };

    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(u32::MAX),
        ..PROCESSENTRY32W::default()
    };
    let mut processes = Vec::new();
    // SAFETY: `entry.dwSize` is initialized as the API requires.
    let mut more = unsafe { Process32FirstW(handle(&snapshot), &raw mut entry) } != 0;
    while more {
        if !PSEUDO_PROCESSES.contains(&entry.th32ProcessID)
            && let Some(process) = read(entry.th32ProcessID)
        {
            processes.push(process);
        }
        // SAFETY: as above; the snapshot stays open.
        more = unsafe { Process32NextW(handle(&snapshot), &raw mut entry) } != 0;
    }
    Ok(processes)
}

/// Ends `process` at once, unless it is gone or its PID was reused.
pub(super) fn terminate(process: ProcessId) -> io::Result<()> {
    let Some(opened) = open(
        process.pid(),
        PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
    ) else {
        // Gone, or never ours to end.
        return Ok(());
    };
    if creation_time(&opened) != Some(process.started()) {
        return Ok(());
    }
    // SAFETY: the handle is open with `PROCESS_TERMINATE`.
    if unsafe { TerminateProcess(handle(&opened), 1) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn read(pid: u32) -> Option<ProcessInfo> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let started = creation_time(&process)?;
    Some(ProcessInfo {
        id: ProcessId::new(pid, started),
        user: process_owner(handle(&process)).map(OsUser::new),
        executable: image_path(&process),
        command_line: command_line(&process).unwrap_or_default(),
    })
}

fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Option<OwnedHandle> {
    // SAFETY: plain call; a null result means failure.
    let raw = unsafe { OpenProcess(access, 0, pid) };
    // SAFETY: a non-null result is a process handle we now own.
    (!raw.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(raw) })
}

fn handle(owned: &OwnedHandle) -> HANDLE {
    owned.as_raw_handle()
}

/// Creation time in 100 ns units since 1601, unique per process.
fn creation_time(process: &OwnedHandle) -> Option<u64> {
    let mut times = [FILETIME::default(); 4];
    let [creation, exit, kernel, user] = &mut times;
    // SAFETY: four valid out-pointers to `FILETIME`.
    let ok = unsafe { GetProcessTimes(handle(process), creation, exit, kernel, user) } != 0;
    ok.then(|| (u64::from(times[0].dwHighDateTime) << 32) | u64::from(times[0].dwLowDateTime))
}

fn image_path(process: &OwnedHandle) -> Option<PathBuf> {
    let mut buffer = [0_u16; 1024];
    let mut length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `length` is the buffer capacity in UTF-16 units.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle(process),
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &raw mut length,
        )
    } != 0;
    ok.then(|| PathBuf::from(from_wide(&buffer[..length as usize])))
}

/// The command line, split the way the C runtime splits it for `main`.
fn command_line(process: &OwnedHandle) -> Option<Vec<String>> {
    let mut needed = 0_u32;
    // SAFETY: a zero-length query only reports the size needed.
    let status = unsafe {
        NtQueryInformationProcess(
            handle(process),
            ProcessCommandLineInformation,
            ptr::null_mut(),
            0,
            &raw mut needed,
        )
    };
    if status != STATUS_INFO_LENGTH_MISMATCH || needed == 0 {
        return None;
    }
    // `u64` elements keep the buffer aligned for the `UNICODE_STRING` header.
    let mut buffer = vec![0_u64; (needed as usize).div_ceil(size_of::<u64>())];
    let capacity = u32::try_from(buffer.len() * size_of::<u64>()).ok()?;
    // SAFETY: the buffer holds at least `capacity` writable bytes.
    let status = unsafe {
        NtQueryInformationProcess(
            handle(process),
            ProcessCommandLineInformation,
            buffer.as_mut_ptr().cast::<c_void>(),
            capacity,
            &raw mut needed,
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: on success the buffer starts with a `UNICODE_STRING` whose
    // `Buffer` points into the same allocation.
    let text = unsafe {
        let header = &*buffer.as_ptr().cast::<UNICODE_STRING>();
        std::slice::from_raw_parts(header.Buffer, usize::from(header.Length) / 2)
    };
    Some(split(text))
}

fn split(command_line: &[u16]) -> Vec<String> {
    let mut wide = command_line.to_vec();
    wide.push(0);
    let mut count = 0;
    // SAFETY: `wide` is NUL-terminated; the result is freed below.
    let arguments = unsafe { CommandLineToArgvW(wide.as_ptr(), &raw mut count) };
    if arguments.is_null() {
        return Vec::new();
    }
    let count = usize::try_from(count).unwrap_or(0);
    // SAFETY: `CommandLineToArgvW` returned `count` NUL-terminated strings.
    let split = (0..count)
        .map(|index| unsafe { from_wide_ptr(*arguments.add(index)) })
        .collect();
    // SAFETY: the array was allocated by `CommandLineToArgvW` for `LocalFree`.
    unsafe { LocalFree(arguments.cast()) };
    split
}
