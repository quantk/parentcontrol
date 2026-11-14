//! Operating-system layer of the agent.
//!
//! The only place in the agent allowed to contain `#[cfg]` on the target OS
//! and `unsafe` code. Everything above it talks to the OS through one trait:
//! process listing, process termination, local user listing and the channel
//! to the tray.

#[cfg(target_os = "linux")]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod linux;

#[cfg(windows)]
#[allow(unsafe_code, reason = "OS FFI; each block has a SAFETY comment")]
mod windows;
