//! Operating-system layer of the tray.
//!
//! The only place in the tray allowed to contain `#[cfg]` on the target OS
//! and `unsafe` code. Each implementation provides the same items: `Ui`
//! with `start`, `show_status` and `notify`; `connect` to the agent; and
//! `close_windows`.

#[cfg(target_os = "linux")]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{Ui, close_windows, connect};

#[cfg(windows)]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod windows;
#[cfg(windows)]
pub(crate) use windows::{Ui, close_windows, connect};
