//! Operating-system layer of the tray.
//!
//! The only place in the tray allowed to contain `#[cfg]` on the target OS
//! and `unsafe` code: the tray icon, notifications, the channel to the agent
//! and, on Windows, closing windows with `WM_CLOSE`.

#[cfg(target_os = "linux")]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod linux;

#[cfg(windows)]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod windows;
