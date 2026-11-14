//! Security descriptors: an administrators-only data directory and the
//! tray pipe.

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

/// A self-relative security descriptor built from SDDL.
#[derive(Debug)]
pub(super) struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

// SAFETY: the descriptor is plain memory from `LocalAlloc`, owned
// exclusively by this value and not tied to the creating thread.
unsafe impl Send for SecurityDescriptor {}

impl SecurityDescriptor {
    /// Parses an SDDL string such as `D:P(A;;GA;;;SY)`.
    pub(super) fn from_sddl(sddl: &str) -> io::Result<Self> {
        let sddl = to_wide(sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; ownership of the result passes
        // to `Self`, which frees it.
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
        Ok(Self(descriptor))
    }

    /// The raw descriptor, valid while `self` lives.
    pub(super) const fn as_ptr(&self) -> PSECURITY_DESCRIPTOR {
        self.0
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: the descriptor was allocated for `LocalFree` and is
        // freed only here.
        unsafe { LocalFree(self.0) };
    }
}

/// Creates `dir` if needed and replaces its permissions with
/// [`ADMINS_ONLY`], including on everything already inside.
pub(super) fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let descriptor = SecurityDescriptor::from_sddl(ADMINS_ONLY)?;

    let (mut present, mut defaulted) = (FALSE, FALSE);
    let mut dacl: *mut ACL = ptr::null_mut();
    // SAFETY: the descriptor is valid; the DACL points into it.
    let ok = unsafe {
        GetSecurityDescriptorDacl(
            descriptor.as_ptr(),
            &raw mut present,
            &raw mut dacl,
            &raw mut defaulted,
        )
    };
    if ok == 0 {
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
