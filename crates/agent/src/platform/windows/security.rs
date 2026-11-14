//! A directory only administrators and `SYSTEM` can access.

use std::path::Path;
use std::{fs, io, ptr};

use windows_sys::Win32::Foundation::{FALSE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};

use super::wide::to_wide;

/// Full control for `SYSTEM` and the Administrators group, inherited by
/// everything inside, with inheritance from the parent cut off (`P`).
const ADMINS_ONLY: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

/// Creates `dir` if needed and replaces its permissions with
/// [`ADMINS_ONLY`], including on everything already inside.
pub(super) fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;

    let sddl = to_wide(ADMINS_ONLY);
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: `sddl` is NUL-terminated; the descriptor is freed below.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = apply(dir, descriptor);
    // SAFETY: the descriptor was allocated for `LocalFree` above.
    unsafe { LocalFree(descriptor) };
    result
}

fn apply(dir: &Path, descriptor: PSECURITY_DESCRIPTOR) -> io::Result<()> {
    let (mut present, mut defaulted) = (FALSE, FALSE);
    let mut dacl: *mut ACL = ptr::null_mut();
    // SAFETY: `descriptor` is valid; the DACL points into it.
    if unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &raw mut present,
            &raw mut dacl,
            &raw mut defaulted,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let path = to_wide(dir);
    // SAFETY: `path` is NUL-terminated and `dacl` valid while `descriptor`
    // lives. `SetNamedSecurityInfoW` also propagates inheritable entries.
    let status = unsafe {
        SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null(),
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(i32::MAX),
        ));
    }
    Ok(())
}
