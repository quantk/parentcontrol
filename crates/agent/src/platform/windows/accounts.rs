//! Local accounts and process owners.

use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::ptr;

use parentcontrol_core::process::OsUser;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::NetworkManagement::NetManagement::{
    FILTER_NORMAL_ACCOUNT, MAX_PREFERRED_LENGTH, NERR_Success, NetApiBufferFree, NetUserEnum,
    UF_ACCOUNTDISABLE, USER_INFO_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::OpenProcessToken;

use super::wide::{from_wide, from_wide_ptr};

/// Enabled local accounts of people, sorted by name.
pub(super) fn people() -> Vec<OsUser> {
    let mut buffer = ptr::null_mut();
    let (mut read, mut total) = (0, 0);
    // SAFETY: valid out-pointers; the buffer is freed below.
    let status = unsafe {
        NetUserEnum(
            ptr::null(),
            1,
            FILTER_NORMAL_ACCOUNT,
            &raw mut buffer,
            MAX_PREFERRED_LENGTH,
            &raw mut read,
            &raw mut total,
            ptr::null_mut(),
        )
    };
    let mut people = Vec::new();
    if status == NERR_Success && !buffer.is_null() {
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "NetUserEnum allocates the buffer aligned for its records"
        )]
        let records = buffer.cast::<USER_INFO_1>();
        // SAFETY: level 1 returns `read` consecutive `USER_INFO_1` records.
        let users = unsafe { std::slice::from_raw_parts(records, read as usize) };
        for user in users
            .iter()
            .filter(|user| user.usri1_flags & UF_ACCOUNTDISABLE == 0)
        {
            // SAFETY: names in the records are NUL-terminated strings.
            people.push(OsUser::new(unsafe { from_wide_ptr(user.usri1_name) }));
        }
    }
    if !buffer.is_null() {
        // SAFETY: the buffer was allocated by `NetUserEnum`.
        unsafe { NetApiBufferFree(buffer.cast()) };
    }
    people.sort();
    people
}

/// The name of the account running `process`, without the domain.
pub(super) fn process_owner(process: HANDLE) -> Option<String> {
    let mut raw = ptr::null_mut();
    // SAFETY: `process` is open with query rights; `raw` receives a handle.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut raw) } == 0 {
        return None;
    }
    // SAFETY: the call succeeded, so `raw` is a token handle we now own.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let token = token.as_raw_handle();

    // `u64` elements keep the buffer aligned for `TOKEN_USER`.
    let mut buffer = [0_u64; 64];
    let mut needed = 0;
    let capacity = u32::try_from(size_of_val(&buffer)).ok()?;
    // SAFETY: the buffer holds `capacity` writable bytes.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            capacity,
            &raw mut needed,
        )
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: on success the buffer starts with a `TOKEN_USER`.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };

    let mut name = [0_u16; 256];
    let mut domain = [0_u16; 256];
    let (mut name_length, mut domain_length) = (256_u32, 256_u32);
    let mut kind: SID_NAME_USE = 0;
    // SAFETY: buffers and their capacities in UTF-16 units are valid.
    let ok = unsafe {
        LookupAccountSidW(
            ptr::null(),
            sid,
            name.as_mut_ptr(),
            &raw mut name_length,
            domain.as_mut_ptr(),
            &raw mut domain_length,
            &raw mut kind,
        )
    };
    (ok != 0).then(|| from_wide(&name))
}
