use super::vault::WalletVaultError;
#[cfg(windows)]
use std::fs::File;
use std::path::Path;

pub(in crate::wallet) fn protect_directory(path: &Path) -> Result<(), WalletVaultError> {
    platform::protect(path, true)
}

#[cfg(any(not(windows), test))]
pub(in crate::wallet) fn protect_file(path: &Path) -> Result<(), WalletVaultError> {
    platform::protect(path, false)
}

pub(in crate::wallet) fn verify_directory(path: &Path) -> Result<(), WalletVaultError> {
    platform::verify(path, true)
}

#[cfg(not(windows))]
pub(in crate::wallet) fn verify_file(path: &Path) -> Result<(), WalletVaultError> {
    platform::verify(path, false)
}

#[cfg(windows)]
pub(in crate::wallet) fn protect_open_file(file: &File) -> Result<(), WalletVaultError> {
    platform::protect_handle(file, false)
}

#[cfg(windows)]
pub(in crate::wallet) fn verify_open_file(file: &File) -> Result<(), WalletVaultError> {
    platform::verify_handle(file, false)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
        ptr,
    };
    use windows_sys::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE, HLOCAL},
            Security::{
                Authorization::{
                    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
                    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
                    GetSecurityInfo, SetNamedSecurityInfoW, SetSecurityInfo, SDDL_REVISION_1,
                    SE_FILE_OBJECT,
                },
                GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, ACL,
                DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
                PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY,
                TOKEN_USER,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
    };

    const MAX_WINDOWS_SECURITY_STRING_UNITS: usize = 4096;

    pub(super) fn protect(path: &Path, directory: bool) -> Result<(), WalletVaultError> {
        let current_user_sid = current_user_sid_string()?;
        let owner_sid = owner_sid_string(path)?;
        if owner_sid != current_user_sid {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let expected = expected_dacl(&current_user_sid, directory);
        apply_dacl(path, &expected)?;
        verify_expected(path, directory, &current_user_sid)
    }

    pub(super) fn verify(path: &Path, directory: bool) -> Result<(), WalletVaultError> {
        let current_user_sid = current_user_sid_string()?;
        let owner_sid = owner_sid_string(path)?;
        if owner_sid != current_user_sid {
            return Err(WalletVaultError::StorageUnavailable);
        }
        verify_expected(path, directory, &current_user_sid)
    }

    pub(super) fn protect_handle(file: &File, directory: bool) -> Result<(), WalletVaultError> {
        let current_user_sid = current_user_sid_string()?;
        let owner_sid = owner_sid_string_handle(file)?;
        if owner_sid != current_user_sid {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let expected = expected_dacl(&current_user_sid, directory);
        apply_dacl_handle(file, &expected)?;
        verify_expected_handle(file, directory, &current_user_sid)
    }

    pub(super) fn verify_handle(file: &File, directory: bool) -> Result<(), WalletVaultError> {
        let current_user_sid = current_user_sid_string()?;
        let owner_sid = owner_sid_string_handle(file)?;
        if owner_sid != current_user_sid {
            return Err(WalletVaultError::StorageUnavailable);
        }
        verify_expected_handle(file, directory, &current_user_sid)
    }

    fn verify_expected(
        path: &Path,
        directory: bool,
        owner_sid: &str,
    ) -> Result<(), WalletVaultError> {
        let actual = dacl_sddl(path)?;
        verify_expected_sddl(&actual, directory, owner_sid)
    }

    fn verify_expected_handle(
        file: &File,
        directory: bool,
        owner_sid: &str,
    ) -> Result<(), WalletVaultError> {
        let actual = dacl_sddl_handle(file)?;
        verify_expected_sddl(&actual, directory, owner_sid)
    }

    fn verify_expected_sddl(
        actual: &str,
        directory: bool,
        owner_sid: &str,
    ) -> Result<(), WalletVaultError> {
        let inheritance = if directory { "OICI" } else { "" };
        let required = [
            format!("(A;{inheritance};FA;;;{owner_sid})"),
            format!("(A;{inheritance};FA;;;SY)"),
            format!("(A;{inheritance};FA;;;BA)"),
        ];

        if !actual.starts_with("D:P")
            || actual.matches('(').count() != required.len()
            || required.iter().any(|entry| !actual.contains(entry))
        {
            return Err(WalletVaultError::StorageUnavailable);
        }
        Ok(())
    }

    fn expected_dacl(owner_sid: &str, directory: bool) -> String {
        let inheritance = if directory { "OICI" } else { "" };
        format!(
            "D:P(A;{inheritance};FA;;;{owner_sid})(A;{inheritance};FA;;;SY)(A;{inheritance};FA;;;BA)"
        )
    }

    fn apply_dacl(path: &Path, sddl: &str) -> Result<(), WalletVaultError> {
        let path_wide = path_wide(path)?;
        let sddl_wide = null_terminated_wide(sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `sddl_wide` is a valid null-terminated UTF-16 buffer and
        // `descriptor` is an initialized out pointer released by LocalFree.
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if converted == 0 || descriptor.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let descriptor_guard = LocalAllocation(descriptor);
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl: *mut ACL = ptr::null_mut();
        // SAFETY: the descriptor is valid for the lifetime of
        // `descriptor_guard`; all output pointers reference local variables.
        let found = unsafe {
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
        };
        if found == 0 || present == 0 || dacl.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }

        // SAFETY: `path_wide` remains alive and null terminated, while `dacl`
        // remains owned by `descriptor_guard` for the duration of the call.
        let result = unsafe {
            SetNamedSecurityInfoW(
                path_wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl,
                ptr::null_mut(),
            )
        };
        drop(descriptor_guard);
        if result != ERROR_SUCCESS {
            return Err(WalletVaultError::StorageUnavailable);
        }
        Ok(())
    }

    fn apply_dacl_handle(file: &File, sddl: &str) -> Result<(), WalletVaultError> {
        let sddl_wide = null_terminated_wide(sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `sddl_wide` is a valid null-terminated UTF-16 buffer and
        // `descriptor` is an initialized out pointer released by LocalFree.
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if converted == 0 || descriptor.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let descriptor_guard = LocalAllocation(descriptor);
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl: *mut ACL = ptr::null_mut();
        // SAFETY: the descriptor is valid for the lifetime of `descriptor_guard`; output pointers
        // reference initialized local variables.
        let found = unsafe {
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
        };
        if found == 0 || present == 0 || dacl.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        // SAFETY: `file` owns a valid handle opened with WRITE_DAC and `dacl` remains owned by the
        // descriptor allocation for the duration of the call.
        let result = unsafe {
            SetSecurityInfo(
                file.as_raw_handle().cast(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl,
                ptr::null_mut(),
            )
        };
        drop(descriptor_guard);
        if result != ERROR_SUCCESS {
            return Err(WalletVaultError::StorageUnavailable);
        }
        Ok(())
    }

    fn owner_sid_string(path: &Path) -> Result<String, WalletVaultError> {
        let path_wide = path_wide(path)?;
        let mut owner: PSID = ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `path_wide` is a valid null-terminated path and each supplied
        // output pointer references initialized local storage.
        let result = unsafe {
            GetNamedSecurityInfoW(
                path_wide.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if result != ERROR_SUCCESS || owner.is_null() || descriptor.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let descriptor_guard = LocalAllocation(descriptor);
        let mut sid_wide: PWSTR = ptr::null_mut();
        // SAFETY: `owner` points inside `descriptor_guard`; `sid_wide` is an
        // initialized out pointer whose result is released by LocalFree.
        let converted = unsafe { ConvertSidToStringSidW(owner, &mut sid_wide) };
        if converted == 0 || sid_wide.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let sid_guard = LocalAllocation(sid_wide.cast());
        let sid = wide_pointer_to_string(sid_wide)?;
        drop(sid_guard);
        drop(descriptor_guard);
        Ok(sid)
    }

    fn owner_sid_string_handle(file: &File) -> Result<String, WalletVaultError> {
        let mut owner: PSID = ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `file` owns a valid handle opened with READ_CONTROL and each supplied output
        // pointer references initialized local storage.
        let result = unsafe {
            GetSecurityInfo(
                file.as_raw_handle().cast(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if result != ERROR_SUCCESS || owner.is_null() || descriptor.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let descriptor_guard = LocalAllocation(descriptor);
        let mut sid_wide: PWSTR = ptr::null_mut();
        // SAFETY: `owner` points inside `descriptor_guard`; `sid_wide` is an initialized output
        // pointer whose result is released by LocalFree.
        let converted = unsafe { ConvertSidToStringSidW(owner, &mut sid_wide) };
        if converted == 0 || sid_wide.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let sid_guard = LocalAllocation(sid_wide.cast());
        let sid = wide_pointer_to_string(sid_wide)?;
        drop(sid_guard);
        drop(descriptor_guard);
        Ok(sid)
    }

    fn current_user_sid_string() -> Result<String, WalletVaultError> {
        let mut token: HANDLE = ptr::null_mut();
        // SAFETY: `GetCurrentProcess` returns a pseudo-handle valid for this
        // process; `token` is an initialized output pointer.
        let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
        if opened == 0 || token.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let token_guard = OwnedHandle(token);
        let mut required_bytes = 0_u32;
        // SAFETY: the documented sizing call uses a null output buffer and
        // zero length while returning the required size through the pointer.
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut required_bytes);
        }
        if required_bytes < u32::try_from(std::mem::size_of::<TOKEN_USER>()).unwrap_or(u32::MAX) {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let required =
            usize::try_from(required_bytes).map_err(|_| WalletVaultError::StorageUnavailable)?;
        let words = required.div_ceil(std::mem::size_of::<usize>());
        let mut aligned_buffer = vec![0_usize; words];
        // SAFETY: the allocation is at least `required_bytes` long and aligned
        // for `TOKEN_USER`; all pointers remain valid for the call.
        let retrieved = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                aligned_buffer.as_mut_ptr().cast(),
                required_bytes,
                &mut required_bytes,
            )
        };
        if retrieved == 0 {
            return Err(WalletVaultError::StorageUnavailable);
        }
        // SAFETY: `GetTokenInformation(TokenUser)` initialized a `TOKEN_USER`
        // at the start of the aligned buffer and its SID remains buffer-owned.
        let user_sid = unsafe { (*(aligned_buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid };
        if user_sid.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let mut sid_wide: PWSTR = ptr::null_mut();
        // SAFETY: `user_sid` remains valid through `aligned_buffer`; the API
        // initializes a LocalFree-compatible output allocation.
        let converted = unsafe { ConvertSidToStringSidW(user_sid, &mut sid_wide) };
        if converted == 0 || sid_wide.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let sid_guard = LocalAllocation(sid_wide.cast());
        let sid = wide_pointer_to_string(sid_wide)?;
        drop(sid_guard);
        drop(token_guard);
        Ok(sid)
    }

    fn dacl_sddl(path: &Path) -> Result<String, WalletVaultError> {
        let path_wide = path_wide(path)?;
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `path_wide` is valid and the descriptor out pointer is
        // initialized. The returned allocation is guarded immediately.
        let result = unsafe {
            GetNamedSecurityInfoW(
                path_wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if result != ERROR_SUCCESS || descriptor.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let descriptor_guard = LocalAllocation(descriptor);
        let mut sddl_wide: PWSTR = ptr::null_mut();
        let mut length = 0_u32;
        // SAFETY: `descriptor` remains valid through `descriptor_guard`; the
        // output pointer and length reference initialized local variables.
        let converted = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut sddl_wide,
                &mut length,
            )
        };
        if converted == 0 || sddl_wide.is_null() || length == 0 {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let sddl_guard = LocalAllocation(sddl_wide.cast());
        let length = usize::try_from(length).map_err(|_| WalletVaultError::StorageUnavailable)?;
        if length > MAX_WINDOWS_SECURITY_STRING_UNITS {
            return Err(WalletVaultError::StorageUnavailable);
        }
        // SAFETY: the API reports `length` UTF-16 units and the allocation
        // remains alive through `sddl_guard`.
        let units = unsafe { std::slice::from_raw_parts(sddl_wide, length) };
        let sddl = String::from_utf16(units).map_err(|_| WalletVaultError::StorageUnavailable)?;
        drop(sddl_guard);
        drop(descriptor_guard);
        Ok(sddl)
    }

    fn dacl_sddl_handle(file: &File) -> Result<String, WalletVaultError> {
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `file` owns a valid handle opened with READ_CONTROL and the descriptor output
        // pointer references initialized local storage.
        let result = unsafe {
            GetSecurityInfo(
                file.as_raw_handle().cast(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if result != ERROR_SUCCESS || descriptor.is_null() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let descriptor_guard = LocalAllocation(descriptor);
        let mut sddl_wide: PWSTR = ptr::null_mut();
        let mut length = 0_u32;
        // SAFETY: `descriptor` remains valid through `descriptor_guard`; output pointers reference
        // initialized local storage.
        let converted = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut sddl_wide,
                &mut length,
            )
        };
        if converted == 0 || sddl_wide.is_null() || length == 0 {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let sddl_guard = LocalAllocation(sddl_wide.cast());
        let length = usize::try_from(length).map_err(|_| WalletVaultError::StorageUnavailable)?;
        if length > MAX_WINDOWS_SECURITY_STRING_UNITS {
            return Err(WalletVaultError::StorageUnavailable);
        }
        // SAFETY: the API reports `length` UTF-16 units and the allocation remains alive through
        // `sddl_guard`.
        let units = unsafe { std::slice::from_raw_parts(sddl_wide, length) };
        let sddl = String::from_utf16(units).map_err(|_| WalletVaultError::StorageUnavailable)?;
        drop(sddl_guard);
        drop(descriptor_guard);
        Ok(sddl)
    }

    fn path_wide(path: &Path) -> Result<Vec<u16>, WalletVaultError> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.is_empty() || wide.contains(&0) {
            return Err(WalletVaultError::StorageUnavailable);
        }
        wide.push(0);
        Ok(wide)
    }

    fn null_terminated_wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn wide_pointer_to_string(pointer: PWSTR) -> Result<String, WalletVaultError> {
        let mut length = 0_usize;
        // SAFETY: callers pass a Windows-allocated null-terminated string. The
        // maximum bound prevents an unbounded scan if the API contract fails.
        unsafe {
            while length < MAX_WINDOWS_SECURITY_STRING_UNITS && *pointer.add(length) != 0 {
                length += 1;
            }
            if length == MAX_WINDOWS_SECURITY_STRING_UNITS {
                return Err(WalletVaultError::StorageUnavailable);
            }
            String::from_utf16(std::slice::from_raw_parts(pointer, length))
                .map_err(|_| WalletVaultError::StorageUnavailable)
        }
    }

    struct LocalAllocation(*mut c_void);

    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: all instances wrap allocations explicitly documented
                // by the Windows API as requiring `LocalFree`.
                unsafe {
                    let _ = LocalFree(self.0 as HLOCAL);
                }
            }
        }
    }

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: this wrapper owns the real process-token handle
                // returned by `OpenProcessToken`.
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        #[test]
        fn applies_and_verifies_restrictive_directory_and_file_dacls() {
            let root = tempfile::tempdir().unwrap();
            let directory = root.path().join("wallets");
            fs::create_dir(&directory).unwrap();
            let file = directory.join("vault.json");
            fs::write(&file, b"encrypted-only-test-data").unwrap();

            protect(&directory, true).unwrap();
            protect(&file, false).unwrap();

            verify(&directory, true).unwrap();
            verify(&file, false).unwrap();
        }

        #[test]
        fn verification_rejects_an_inherited_default_dacl() {
            let root = tempfile::tempdir().unwrap();
            let file = root.path().join("vault.json");
            fs::write(&file, b"encrypted-only-test-data").unwrap();

            assert_eq!(
                verify(&file, false).unwrap_err(),
                WalletVaultError::StorageUnavailable
            );
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    pub(super) fn protect(path: &Path, directory: bool) -> Result<(), WalletVaultError> {
        let mode = if directory { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|_| WalletVaultError::StorageUnavailable)?;
        verify(path, directory)
    }

    pub(super) fn verify(path: &Path, directory: bool) -> Result<(), WalletVaultError> {
        let metadata =
            fs::symlink_metadata(path).map_err(|_| WalletVaultError::StorageUnavailable)?;
        if metadata.file_type().is_symlink()
            || (directory && !metadata.is_dir())
            || (!directory && !metadata.is_file())
        {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let expected = if directory { 0o700 } else { 0o600 };
        if metadata.permissions().mode() & 0o777 != expected {
            return Err(WalletVaultError::StorageUnavailable);
        }
        Ok(())
    }
}

#[cfg(not(any(windows, unix)))]
mod platform {
    use super::*;

    pub(super) fn protect(_path: &Path, _directory: bool) -> Result<(), WalletVaultError> {
        Err(WalletVaultError::StorageUnavailable)
    }

    pub(super) fn verify(_path: &Path, _directory: bool) -> Result<(), WalletVaultError> {
        Err(WalletVaultError::StorageUnavailable)
    }
}
