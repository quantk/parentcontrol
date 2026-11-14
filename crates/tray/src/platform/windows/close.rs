//! Asking the windows of a process to close, as the close button does.

use std::collections::HashSet;

use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};
use windows_sys::core::BOOL;

/// Posts `WM_CLOSE` to every visible top-level window of `processes`. The
/// programs may save and quit; the agent forces them later if they do not.
pub(crate) fn close_windows(processes: &[u32]) {
    let processes: HashSet<u32> = processes.iter().copied().collect();
    // SAFETY: the callback only runs during this call, while `processes`
    // lives, and receives its address unchanged.
    unsafe { EnumWindows(Some(close_if_listed), (&raw const processes) as LPARAM) };
}

unsafe extern "system" fn close_if_listed(window: HWND, processes: LPARAM) -> BOOL {
    // SAFETY: `close_windows` passes a pointer to a live `HashSet<u32>`.
    let processes = unsafe { &*(processes as *const HashSet<u32>) };
    let mut pid = 0;
    // SAFETY: `window` comes from `EnumWindows`; `pid` is a valid out-pointer.
    unsafe { GetWindowThreadProcessId(window, &raw mut pid) };
    // SAFETY: plain calls on a window handle from `EnumWindows`.
    if processes.contains(&pid) && unsafe { IsWindowVisible(window) } != 0 {
        // SAFETY: as above; a failure only means the window is gone.
        unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
    }
    TRUE
}
