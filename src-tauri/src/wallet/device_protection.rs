use super::vault::WalletVaultError;
use secrecy::{ExposeSecret, SecretBox};
use std::fmt;
use zeroize::Zeroizing;

pub(in crate::wallet) const DEVICE_KEY_BYTES: usize = 32;
pub(in crate::wallet) const WINDOWS_DPAPI_ALGORITHM: &str = "windows_dpapi_current_user";

pub(in crate::wallet) struct DeviceKey(SecretBox<[u8; DEVICE_KEY_BYTES]>);

impl DeviceKey {
    fn generate() -> Result<Self, getrandom::Error> {
        let mut random_result = Ok(());
        let key = SecretBox::<[u8; DEVICE_KEY_BYTES]>::init_with_mut(|bytes| {
            random_result = getrandom::fill(bytes);
        });
        random_result?;
        Ok(Self(key))
    }

    fn from_zeroizing_vec(bytes: Zeroizing<Vec<u8>>) -> Option<Self> {
        if bytes.len() != DEVICE_KEY_BYTES {
            return None;
        }
        Some(Self(SecretBox::<[u8; DEVICE_KEY_BYTES]>::init_with_mut(
            |key| {
                key.copy_from_slice(&bytes);
            },
        )))
    }

    pub(in crate::wallet) fn with_exposed<R>(
        &self,
        operation: impl FnOnce(&[u8; DEVICE_KEY_BYTES]) -> R,
    ) -> R {
        operation(self.0.expose_secret())
    }
}

impl fmt::Debug for DeviceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeviceKey([REDACTED])")
    }
}

pub(in crate::wallet) struct ProtectedDeviceKey {
    pub algorithm: &'static str,
    pub protected_bytes: Vec<u8>,
    pub device_key: DeviceKey,
}

pub(in crate::wallet) fn generate_and_protect(
    entropy: &[u8],
) -> Result<ProtectedDeviceKey, WalletVaultError> {
    generate_and_protect_with::<PlatformProtector>(entropy)
}

pub(in crate::wallet) fn unprotect(
    algorithm: &str,
    protected: &[u8],
    entropy: &[u8],
) -> Result<DeviceKey, WalletVaultError> {
    if algorithm != WINDOWS_DPAPI_ALGORITHM {
        return Err(WalletVaultError::InvalidOrUnsupportedFormat);
    }
    PlatformProtector::unprotect(protected, entropy)
}

fn generate_and_protect_with<P: DeviceProtector>(
    entropy: &[u8],
) -> Result<ProtectedDeviceKey, WalletVaultError> {
    let device_key =
        DeviceKey::generate().map_err(|_| WalletVaultError::RandomSourceUnavailable)?;
    let protected_bytes = device_key.with_exposed(|key| P::protect(key, entropy))?;
    if protected_bytes.is_empty() || protected_bytes.len() > P::MAX_PROTECTED_BYTES {
        return Err(WalletVaultError::DeviceProtectionUnavailable);
    }
    Ok(ProtectedDeviceKey {
        algorithm: P::ALGORITHM,
        protected_bytes,
        device_key,
    })
}

trait DeviceProtector {
    const ALGORITHM: &'static str;
    const MAX_PROTECTED_BYTES: usize = 4096;

    fn protect(
        plaintext: &[u8; DEVICE_KEY_BYTES],
        entropy: &[u8],
    ) -> Result<Vec<u8>, WalletVaultError>;

    fn unprotect(protected: &[u8], entropy: &[u8]) -> Result<DeviceKey, WalletVaultError>;
}

struct PlatformProtector;

#[cfg(windows)]
impl DeviceProtector for PlatformProtector {
    const ALGORITHM: &'static str = WINDOWS_DPAPI_ALGORITHM;

    fn protect(
        plaintext: &[u8; DEVICE_KEY_BYTES],
        entropy: &[u8],
    ) -> Result<Vec<u8>, WalletVaultError> {
        windows_dpapi::protect(plaintext, entropy)
    }

    fn unprotect(protected: &[u8], entropy: &[u8]) -> Result<DeviceKey, WalletVaultError> {
        windows_dpapi::unprotect(protected, entropy)
    }
}

#[cfg(not(windows))]
impl DeviceProtector for PlatformProtector {
    const ALGORITHM: &'static str = "unsupported_platform";

    fn protect(
        _plaintext: &[u8; DEVICE_KEY_BYTES],
        _entropy: &[u8],
    ) -> Result<Vec<u8>, WalletVaultError> {
        Err(WalletVaultError::DeviceProtectionUnavailable)
    }

    fn unprotect(_protected: &[u8], _entropy: &[u8]) -> Result<DeviceKey, WalletVaultError> {
        Err(WalletVaultError::DeviceProtectionUnavailable)
    }
}

#[cfg(windows)]
mod windows_dpapi {
    use super::*;
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    use zeroize::{Zeroize, Zeroizing};

    pub(super) fn protect(
        plaintext: &[u8; DEVICE_KEY_BYTES],
        entropy: &[u8],
    ) -> Result<Vec<u8>, WalletVaultError> {
        let input = blob_from_slice(plaintext)?;
        let entropy_blob = blob_from_slice(entropy)?;
        let mut output = empty_blob();
        // Deliberately use default current-user DPAPI scope. Machine-wide scope would allow any
        // local account to unwrap this factor and would weaken per-user isolation.
        // SAFETY: both input blobs reference live Rust slices, all optional UI
        // pointers are null, and `output` is an initialized out structure.
        let protected = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy_blob,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if protected == 0 {
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        copy_public_output_blob(&mut output)
    }

    pub(super) fn unprotect(
        protected: &[u8],
        entropy: &[u8],
    ) -> Result<DeviceKey, WalletVaultError> {
        if protected.is_empty() || protected.len() > PlatformProtector::MAX_PROTECTED_BYTES {
            return Err(WalletVaultError::InvalidOrUnsupportedFormat);
        }
        let input = blob_from_slice(protected)?;
        let entropy_blob = blob_from_slice(entropy)?;
        let mut output = empty_blob();
        // SAFETY: input buffers remain alive, no UI is permitted, and the
        // initialized output blob is released and zeroized below.
        let unprotected = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy_blob,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if unprotected == 0 {
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        DeviceKey::from_zeroizing_vec(copy_secret_output_blob(&mut output)?)
            .ok_or(WalletVaultError::DeviceProtectionUnavailable)
    }

    fn blob_from_slice(bytes: &[u8]) -> Result<CRYPT_INTEGER_BLOB, WalletVaultError> {
        let length = u32::try_from(bytes.len())
            .map_err(|_| WalletVaultError::DeviceProtectionUnavailable)?;
        Ok(CRYPT_INTEGER_BLOB {
            cbData: length,
            pbData: bytes.as_ptr().cast_mut(),
        })
    }

    fn empty_blob() -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        }
    }

    fn copy_public_output_blob(
        output: &mut CRYPT_INTEGER_BLOB,
    ) -> Result<Vec<u8>, WalletVaultError> {
        if output.pbData.is_null() || output.cbData == 0 {
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        let length = usize::try_from(output.cbData)
            .map_err(|_| WalletVaultError::DeviceProtectionUnavailable)?;
        if length > PlatformProtector::MAX_PROTECTED_BYTES {
            // SAFETY: DPAPI returned this allocation and length. It is wiped
            // before being returned to the Windows local allocator.
            unsafe {
                std::slice::from_raw_parts_mut(output.pbData, length).zeroize();
                let _ = LocalFree(output.pbData.cast::<c_void>() as HLOCAL);
            }
            output.pbData = ptr::null_mut();
            output.cbData = 0;
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        // SAFETY: DPAPI returned an allocation of `cbData` bytes.
        let copied = unsafe { std::slice::from_raw_parts(output.pbData, length).to_vec() };
        // SAFETY: DPAPI documents that its output must be freed by LocalFree.
        unsafe {
            let _ = LocalFree(output.pbData.cast::<c_void>() as HLOCAL);
        }
        output.pbData = ptr::null_mut();
        output.cbData = 0;
        Ok(copied)
    }

    fn copy_secret_output_blob(
        output: &mut CRYPT_INTEGER_BLOB,
    ) -> Result<Zeroizing<Vec<u8>>, WalletVaultError> {
        if output.pbData.is_null() || output.cbData == 0 {
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        let length = usize::try_from(output.cbData)
            .map_err(|_| WalletVaultError::DeviceProtectionUnavailable)?;
        if length > PlatformProtector::MAX_PROTECTED_BYTES {
            // SAFETY: DPAPI returned this allocation and length. It is wiped
            // before being returned to the Windows local allocator.
            unsafe {
                std::slice::from_raw_parts_mut(output.pbData, length).zeroize();
                let _ = LocalFree(output.pbData.cast::<c_void>() as HLOCAL);
            }
            output.pbData = ptr::null_mut();
            output.cbData = 0;
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        // The only copy out of DPAPI lands directly in zeroizing heap storage.
        let copied =
            Zeroizing::new(unsafe { std::slice::from_raw_parts(output.pbData, length).to_vec() });
        // SAFETY: the mutable slice covers the DPAPI output allocation.
        unsafe {
            std::slice::from_raw_parts_mut(output.pbData, length).zeroize();
            let _ = LocalFree(output.pbData.cast::<c_void>() as HLOCAL);
        }
        output.pbData = ptr::null_mut();
        output.cbData = 0;
        Ok(copied)
    }
}

#[cfg(test)]
pub(in crate::wallet) const TEST_PROTECTOR_ALGORITHM: &str = "test_device_protector";

#[cfg(test)]
struct TestProtector;

#[cfg(test)]
impl DeviceProtector for TestProtector {
    const ALGORITHM: &'static str = TEST_PROTECTOR_ALGORITHM;

    fn protect(
        plaintext: &[u8; DEVICE_KEY_BYTES],
        entropy: &[u8],
    ) -> Result<Vec<u8>, WalletVaultError> {
        if entropy.is_empty() {
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        Ok(plaintext
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ entropy[index % entropy.len()])
            .collect())
    }

    fn unprotect(protected: &[u8], entropy: &[u8]) -> Result<DeviceKey, WalletVaultError> {
        if protected.len() != DEVICE_KEY_BYTES || entropy.is_empty() {
            return Err(WalletVaultError::DeviceProtectionUnavailable);
        }
        let bytes = Zeroizing::new(
            protected
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ entropy[index % entropy.len()])
                .collect(),
        );
        DeviceKey::from_zeroizing_vec(bytes).ok_or(WalletVaultError::DeviceProtectionUnavailable)
    }
}

#[cfg(test)]
pub(in crate::wallet) fn generate_and_protect_for_test(
    entropy: &[u8],
) -> Result<ProtectedDeviceKey, WalletVaultError> {
    generate_and_protect_with::<TestProtector>(entropy)
}

#[cfg(test)]
pub(in crate::wallet) fn unprotect_for_test(
    protected: &[u8],
    entropy: &[u8],
) -> Result<DeviceKey, WalletVaultError> {
    TestProtector::unprotect(protected, entropy)
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[test]
    fn unwrapped_device_key_enters_secret_box_without_an_ordinary_array() {
        let key =
            DeviceKey::from_zeroizing_vec(Zeroizing::new(vec![0x4d; DEVICE_KEY_BYTES])).unwrap();

        assert!(key.with_exposed(|restored| restored == &[0x4d; DEVICE_KEY_BYTES]));
    }

    #[test]
    fn unwrapped_device_key_requires_the_exact_key_length() {
        assert!(DeviceKey::from_zeroizing_vec(Zeroizing::new(vec![7; 31])).is_none());
        assert!(DeviceKey::from_zeroizing_vec(Zeroizing::new(vec![7; 33])).is_none());
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_dpapi_round_trips_only_with_matching_entropy() {
        let entropy = b"vision-wallet-current-user-dpapi-test";
        let protected = generate_and_protect(entropy).unwrap();
        let restored = unprotect(protected.algorithm, &protected.protected_bytes, entropy).unwrap();
        let matches = protected
            .device_key
            .with_exposed(|expected| restored.with_exposed(|actual| actual == expected));
        assert!(matches);
        assert_eq!(
            unprotect(
                protected.algorithm,
                &protected.protected_bytes,
                b"wrong-entropy"
            )
            .unwrap_err(),
            WalletVaultError::DeviceProtectionUnavailable
        );
    }
}
