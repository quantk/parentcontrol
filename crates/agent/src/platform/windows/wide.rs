//! UTF-16 strings for the Windows API.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;

/// `text` as a NUL-terminated UTF-16 string.
pub(super) fn to_wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// UTF-16 up to the first NUL, or the whole slice, as a Rust string.
pub(super) fn from_wide(wide: &[u16]) -> String {
    let end = wide
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// The NUL-terminated UTF-16 string at `pointer`.
///
/// # Safety
///
/// `pointer` must be null or point to a NUL-terminated UTF-16 string that
/// stays valid for the duration of the call.
pub(super) unsafe fn from_wide_ptr(pointer: *const u16) -> String {
    if pointer.is_null() {
        return String::new();
    }
    let mut length = 0;
    // SAFETY: the caller guarantees a NUL terminator, so every unit up to
    // and including it is readable.
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the `length` units before the terminator were just read.
    from_wide(unsafe { std::slice::from_raw_parts(pointer, length) })
}
