use super::{
    device_protection::{self, DeviceKey, ProtectedDeviceKey},
    secrets::{WalletPassword, WalletSeed},
    storage_security,
};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
#[cfg(any(not(windows), test))]
use std::fs;
use std::{
    fmt,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::{Zeroize, Zeroizing};

const VAULT_SCHEMA: &str = "vision-desktop-wallet-vault";
const VAULT_VERSION: u16 = 2;
const KDF_ALGORITHM: &str = "argon2id";
const KDF_VERSION: u32 = 0x13;
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_ITERATIONS: u32 = 3;
const KDF_LANES: u32 = 1;
const CIPHER_ALGORITHM: &str = "xchacha20poly1305";
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 24;
const SEED_BYTES: usize = 32;
const AUTH_TAG_BYTES: usize = 16;
const MAX_VAULT_JSON_BYTES: usize = 16 * 1024;
const MIN_PASSWORD_BYTES: usize = 16;
const MAX_PASSWORD_BYTES: usize = 1024;
const MAX_PROTECTED_DEVICE_KEY_BYTES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EncryptedWalletVault {
    schema: String,
    version: u16,
    wallet_id: String,
    created_at_unix_ms: u64,
    kdf: VaultKdf,
    device_protection: VaultDeviceProtection,
    cipher: VaultCipher,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VaultKdf {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
    salt_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VaultCipher {
    algorithm: String,
    nonce_hex: String,
    ciphertext_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VaultDeviceProtection {
    algorithm: String,
    protected_key_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletVaultError {
    InvalidWalletId,
    PasswordPolicy,
    InvalidOrUnsupportedFormat,
    InvalidPasswordOrCorruptVault,
    RandomSourceUnavailable,
    DeviceProtectionUnavailable,
    StorageUnavailable,
    VaultAlreadyExists,
}

impl fmt::Display for WalletVaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidWalletId => "wallet identifier is invalid",
            Self::PasswordPolicy => "wallet password does not meet the local security policy",
            Self::InvalidOrUnsupportedFormat => "wallet vault format is invalid or unsupported",
            Self::InvalidPasswordOrCorruptVault => {
                "wallet password is incorrect or the vault is damaged"
            }
            Self::RandomSourceUnavailable => "secure operating-system randomness is unavailable",
            Self::DeviceProtectionUnavailable => {
                "operating-system wallet protection is unavailable"
            }
            Self::StorageUnavailable => "secure wallet storage is unavailable",
            Self::VaultAlreadyExists => "wallet vault already exists",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for WalletVaultError {}

impl EncryptedWalletVault {
    pub(in crate::wallet) fn wallet_id(&self) -> &str {
        &self.wallet_id
    }

    pub(in crate::wallet) fn created_at_unix_ms(&self) -> u64 {
        self.created_at_unix_ms
    }

    /// Encrypts a seed without exposing a creation command to the frontend.
    pub(in crate::wallet) fn encrypt(
        wallet_id: &str,
        created_at_unix_ms: u64,
        seed: &WalletSeed,
        password: &WalletPassword,
    ) -> Result<Self, WalletVaultError> {
        validate_wallet_id(wallet_id)?;
        validate_password(password)?;
        let protected_device_key =
            device_protection::generate_and_protect(&device_entropy(wallet_id))?;
        Self::encrypt_with_device_key(
            wallet_id,
            created_at_unix_ms,
            seed,
            password,
            protected_device_key,
        )
    }

    fn encrypt_with_device_key(
        wallet_id: &str,
        created_at_unix_ms: u64,
        seed: &WalletSeed,
        password: &WalletPassword,
        protected_device_key: ProtectedDeviceKey,
    ) -> Result<Self, WalletVaultError> {
        let mut salt = [0_u8; SALT_BYTES];
        let mut nonce = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut salt).map_err(|_| WalletVaultError::RandomSourceUnavailable)?;
        getrandom::fill(&mut nonce).map_err(|_| WalletVaultError::RandomSourceUnavailable)?;

        let kdf = VaultKdf {
            algorithm: KDF_ALGORITHM.to_string(),
            version: KDF_VERSION,
            memory_kib: KDF_MEMORY_KIB,
            iterations: KDF_ITERATIONS,
            lanes: KDF_LANES,
            salt_hex: hex::encode(salt),
        };
        let mut vault = Self {
            schema: VAULT_SCHEMA.to_string(),
            version: VAULT_VERSION,
            wallet_id: wallet_id.to_string(),
            created_at_unix_ms,
            kdf,
            device_protection: VaultDeviceProtection {
                algorithm: protected_device_key.algorithm.to_string(),
                protected_key_hex: hex::encode(&protected_device_key.protected_bytes),
            },
            cipher: VaultCipher {
                algorithm: CIPHER_ALGORITHM.to_string(),
                nonce_hex: hex::encode(nonce),
                ciphertext_hex: String::new(),
            },
        };

        let password_key = derive_password_key(password, &salt)?;
        let key = combine_keys(&password_key, &protected_device_key.device_key);
        let mut cipher_key = Key::try_from(key.as_ref())
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        let cipher_nonce = XNonce::try_from(nonce.as_slice())
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        let cipher = XChaCha20Poly1305::new(&cipher_key);
        cipher_key.as_mut_slice().zeroize();
        let aad = vault.aad();
        let ciphertext = seed
            .with_exposed(|bytes| {
                cipher.encrypt(
                    &cipher_nonce,
                    Payload {
                        msg: bytes,
                        aad: &aad,
                    },
                )
            })
            .map_err(|_| WalletVaultError::InvalidPasswordOrCorruptVault)?;
        vault.cipher.ciphertext_hex = hex::encode(ciphertext);
        Ok(vault)
    }

    #[cfg(test)]
    pub(in crate::wallet) fn encrypt_for_test(
        wallet_id: &str,
        created_at_unix_ms: u64,
        seed: &WalletSeed,
        password: &WalletPassword,
    ) -> Result<Self, WalletVaultError> {
        validate_wallet_id(wallet_id)?;
        validate_password(password)?;
        let protected_device_key =
            device_protection::generate_and_protect_for_test(&device_entropy(wallet_id))?;
        Self::encrypt_with_device_key(
            wallet_id,
            created_at_unix_ms,
            seed,
            password,
            protected_device_key,
        )
    }

    /// Unlocks a vault inside Rust. Callers receive a redacted, zeroizing seed wrapper.
    pub(in crate::wallet) fn unlock(
        &self,
        password: &WalletPassword,
    ) -> Result<WalletSeed, WalletVaultError> {
        self.validate()?;
        validate_password(password)?;

        let salt = decode_fixed::<SALT_BYTES>(&self.kdf.salt_hex)?;
        let nonce = decode_fixed::<NONCE_BYTES>(&self.cipher.nonce_hex)?;
        let ciphertext = hex::decode(&self.cipher.ciphertext_hex)
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        let protected_device_key = hex::decode(&self.device_protection.protected_key_hex)
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        let device_key = unprotect_device_key(
            &self.device_protection.algorithm,
            &protected_device_key,
            &device_entropy(&self.wallet_id),
        )?;
        let password_key = derive_password_key(password, &salt)?;
        let key = combine_keys(&password_key, &device_key);
        let mut cipher_key = Key::try_from(key.as_ref())
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        let cipher_nonce = XNonce::try_from(nonce.as_slice())
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        let cipher = XChaCha20Poly1305::new(&cipher_key);
        cipher_key.as_mut_slice().zeroize();
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &cipher_nonce,
                    Payload {
                        msg: &ciphertext,
                        aad: &self.aad(),
                    },
                )
                .map_err(|_| WalletVaultError::InvalidPasswordOrCorruptVault)?,
        );
        let seed_bytes: [u8; SEED_BYTES] = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| WalletVaultError::InvalidPasswordOrCorruptVault)?;
        Ok(WalletSeed::from_bytes(seed_bytes))
    }

    pub(in crate::wallet) fn from_json(input: &[u8]) -> Result<Self, WalletVaultError> {
        if input.is_empty() || input.len() > MAX_VAULT_JSON_BYTES {
            return Err(WalletVaultError::InvalidOrUnsupportedFormat);
        }
        let vault: Self = serde_json::from_slice(input)
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        vault.validate()?;
        Ok(vault)
    }

    pub(in crate::wallet) fn to_json(&self) -> Result<Vec<u8>, WalletVaultError> {
        self.validate()?;
        serde_json::to_vec_pretty(self).map_err(|_| WalletVaultError::StorageUnavailable)
    }

    fn validate(&self) -> Result<(), WalletVaultError> {
        validate_wallet_id(&self.wallet_id)?;
        if self.schema != VAULT_SCHEMA
            || self.version != VAULT_VERSION
            || self.kdf.algorithm != KDF_ALGORITHM
            || self.kdf.version != KDF_VERSION
            || self.kdf.memory_kib != KDF_MEMORY_KIB
            || self.kdf.iterations != KDF_ITERATIONS
            || self.kdf.lanes != KDF_LANES
            || !supported_device_algorithm(&self.device_protection.algorithm)
            || self.device_protection.protected_key_hex.is_empty()
            || self.device_protection.protected_key_hex.len() > MAX_PROTECTED_DEVICE_KEY_BYTES * 2
            || !self
                .device_protection
                .protected_key_hex
                .len()
                .is_multiple_of(2)
            || self.cipher.algorithm != CIPHER_ALGORITHM
            || self.kdf.salt_hex.len() != SALT_BYTES * 2
            || self.cipher.nonce_hex.len() != NONCE_BYTES * 2
            || self.cipher.ciphertext_hex.len() != (SEED_BYTES + AUTH_TAG_BYTES) * 2
        {
            return Err(WalletVaultError::InvalidOrUnsupportedFormat);
        }
        decode_fixed::<SALT_BYTES>(&self.kdf.salt_hex)?;
        decode_fixed::<NONCE_BYTES>(&self.cipher.nonce_hex)?;
        decode_fixed::<{ SEED_BYTES + AUTH_TAG_BYTES }>(&self.cipher.ciphertext_hex)?;
        hex::decode(&self.device_protection.protected_key_hex)
            .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
        Ok(())
    }

    fn aad(&self) -> Vec<u8> {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.schema,
            self.version,
            self.wallet_id,
            self.created_at_unix_ms,
            self.kdf.algorithm,
            self.kdf.version,
            self.kdf.memory_kib,
            self.kdf.iterations,
            self.kdf.lanes,
            self.kdf.salt_hex,
            self.device_protection.algorithm,
            self.device_protection.protected_key_hex,
            self.cipher.algorithm,
            self.cipher.nonce_hex
        )
        .into_bytes()
    }
}

/// Writes only encrypted JSON and never overwrites an existing vault.
pub(in crate::wallet) fn store_new_vault(
    path: &Path,
    vault: &EncryptedWalletVault,
) -> Result<(), WalletVaultError> {
    #[cfg(windows)]
    return store_new_vault_windows(path, vault);

    #[cfg(not(windows))]
    {
        let parent = path.parent().ok_or(WalletVaultError::StorageUnavailable)?;
        fs::create_dir_all(parent).map_err(|_| WalletVaultError::StorageUnavailable)?;
        storage_security::protect_directory(parent)?;
        if path.exists() {
            return Err(WalletVaultError::VaultAlreadyExists);
        }

        let encrypted_json = Zeroizing::new(vault.to_json()?);
        let mut suffix = [0_u8; 16];
        getrandom::fill(&mut suffix).map_err(|_| WalletVaultError::RandomSourceUnavailable)?;
        let temporary_path = temporary_path(parent, &suffix);
        let write_result = write_new_file(&temporary_path, encrypted_json.as_slice())
            .and_then(|_| storage_security::protect_file(&temporary_path))
            .and_then(|_| fs::hard_link(&temporary_path, path).map_err(map_storage_error))
            .and_then(|_| storage_security::verify_file(path));
        let _ = fs::remove_file(&temporary_path);
        write_result
    }
}

pub(in crate::wallet) fn load_vault(path: &Path) -> Result<EncryptedWalletVault, WalletVaultError> {
    #[cfg(windows)]
    return load_vault_windows(path);

    #[cfg(not(windows))]
    {
        let parent = path.parent().ok_or(WalletVaultError::StorageUnavailable)?;
        storage_security::verify_directory(parent)?;
        storage_security::verify_file(path)?;
        let link_metadata =
            fs::symlink_metadata(path).map_err(|_| WalletVaultError::StorageUnavailable)?;
        if link_metadata.file_type().is_symlink() {
            return Err(WalletVaultError::StorageUnavailable);
        }
        let file = fs::File::open(path).map_err(|_| WalletVaultError::StorageUnavailable)?;
        let metadata = file
            .metadata()
            .map_err(|_| WalletVaultError::StorageUnavailable)?;
        if !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_VAULT_JSON_BYTES as u64
        {
            return Err(WalletVaultError::InvalidOrUnsupportedFormat);
        }
        let mut bytes = Zeroizing::new(Vec::with_capacity(metadata.len() as usize));
        file.take(MAX_VAULT_JSON_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| WalletVaultError::StorageUnavailable)?;
        if bytes.len() > MAX_VAULT_JSON_BYTES {
            return Err(WalletVaultError::InvalidOrUnsupportedFormat);
        }
        EncryptedWalletVault::from_json(bytes.as_slice())
    }
}

#[cfg(windows)]
fn store_new_vault_windows(
    path: &Path,
    vault: &EncryptedWalletVault,
) -> Result<(), WalletVaultError> {
    use super::secure_filesystem::{
        create_new_publishable_file, publish_open_file, rewind, DirectoryChainGuard,
    };

    let parent = path.parent().ok_or(WalletVaultError::StorageUnavailable)?;
    let _directories =
        DirectoryChainGuard::ensure(parent).map_err(|_| WalletVaultError::StorageUnavailable)?;
    storage_security::protect_directory(parent)?;
    if path.exists() {
        return Err(WalletVaultError::VaultAlreadyExists);
    }
    let encrypted_json = Zeroizing::new(vault.to_json()?);
    let mut suffix = [0_u8; 16];
    getrandom::fill(&mut suffix).map_err(|_| WalletVaultError::RandomSourceUnavailable)?;
    let temporary_path = temporary_path(parent, &suffix);
    let mut temporary = create_new_publishable_file(&temporary_path).map_err(map_storage_error)?;
    storage_security::protect_open_file(&temporary)?;
    temporary
        .write_all(encrypted_json.as_slice())
        .and_then(|_| temporary.sync_all())
        .map_err(|_| WalletVaultError::StorageUnavailable)?;
    storage_security::verify_open_file(&temporary)?;
    publish_open_file(&temporary, path).map_err(map_storage_error)?;
    storage_security::verify_open_file(&temporary)?;
    rewind(&mut temporary).map_err(|_| WalletVaultError::StorageUnavailable)?;
    let mut stored = Zeroizing::new(Vec::with_capacity(encrypted_json.len()));
    temporary
        .take(MAX_VAULT_JSON_BYTES as u64 + 1)
        .read_to_end(&mut stored)
        .map_err(|_| WalletVaultError::StorageUnavailable)?;
    if stored.as_slice() != encrypted_json.as_slice() {
        return Err(WalletVaultError::StorageUnavailable);
    }
    Ok(())
}

#[cfg(windows)]
fn load_vault_windows(path: &Path) -> Result<EncryptedWalletVault, WalletVaultError> {
    use super::secure_filesystem::{open_existing_file, DirectoryChainGuard};

    let parent = path.parent().ok_or(WalletVaultError::StorageUnavailable)?;
    let _directories = DirectoryChainGuard::open_existing(parent)
        .map_err(|_| WalletVaultError::StorageUnavailable)?;
    let file = open_existing_file(path).map_err(|_| WalletVaultError::StorageUnavailable)?;
    storage_security::verify_directory(parent)?;
    storage_security::verify_open_file(&file)?;
    let metadata = file
        .metadata()
        .map_err(|_| WalletVaultError::StorageUnavailable)?;
    if metadata.len() == 0 || metadata.len() > MAX_VAULT_JSON_BYTES as u64 {
        return Err(WalletVaultError::InvalidOrUnsupportedFormat);
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(metadata.len() as usize));
    file.take(MAX_VAULT_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| WalletVaultError::StorageUnavailable)?;
    if bytes.len() > MAX_VAULT_JSON_BYTES {
        return Err(WalletVaultError::InvalidOrUnsupportedFormat);
    }
    EncryptedWalletVault::from_json(bytes.as_slice())
}

fn derive_password_key(
    password: &WalletPassword,
    salt: &[u8; SALT_BYTES],
) -> Result<Zeroizing<[u8; 32]>, WalletVaultError> {
    let params = Params::new(KDF_MEMORY_KIB, KDF_ITERATIONS, KDF_LANES, Some(32))
        .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; 32]);
    password
        .with_exposed(|bytes| argon2.hash_password_into(bytes, salt, key.as_mut()))
        .map_err(|_| WalletVaultError::InvalidPasswordOrCorruptVault)?;
    Ok(key)
}

fn combine_keys(password_key: &[u8; 32], device_key: &DeviceKey) -> Zeroizing<[u8; 32]> {
    let mut combined = Zeroizing::new([0_u8; 32]);
    device_key.with_exposed(|device_bytes| {
        for (index, output) in combined.iter_mut().enumerate() {
            *output = password_key[index] ^ device_bytes[index];
        }
    });
    combined
}

fn device_entropy(wallet_id: &str) -> Vec<u8> {
    format!("{VAULT_SCHEMA}|{VAULT_VERSION}|{wallet_id}|device-key").into_bytes()
}

fn unprotect_device_key(
    algorithm: &str,
    protected: &[u8],
    entropy: &[u8],
) -> Result<DeviceKey, WalletVaultError> {
    #[cfg(test)]
    if algorithm == device_protection::TEST_PROTECTOR_ALGORITHM {
        return device_protection::unprotect_for_test(protected, entropy)
            .map_err(|_| WalletVaultError::InvalidPasswordOrCorruptVault);
    }
    device_protection::unprotect(algorithm, protected, entropy)
        .map_err(|_| WalletVaultError::InvalidPasswordOrCorruptVault)
}

fn supported_device_algorithm(algorithm: &str) -> bool {
    if algorithm == device_protection::WINDOWS_DPAPI_ALGORITHM {
        return true;
    }
    #[cfg(test)]
    if algorithm == device_protection::TEST_PROTECTOR_ALGORITHM {
        return true;
    }
    false
}

fn validate_wallet_id(wallet_id: &str) -> Result<(), WalletVaultError> {
    if wallet_id.is_empty()
        || wallet_id.len() > 64
        || !wallet_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(WalletVaultError::InvalidWalletId);
    }
    Ok(())
}

fn validate_password(password: &WalletPassword) -> Result<(), WalletVaultError> {
    let valid = password
        .with_exposed(|bytes| (MIN_PASSWORD_BYTES..=MAX_PASSWORD_BYTES).contains(&bytes.len()));
    if valid {
        Ok(())
    } else {
        Err(WalletVaultError::PasswordPolicy)
    }
}

fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], WalletVaultError> {
    let decoded = hex::decode(value).map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)?;
    decoded
        .try_into()
        .map_err(|_| WalletVaultError::InvalidOrUnsupportedFormat)
}

fn temporary_path(parent: &Path, suffix: &[u8; 16]) -> PathBuf {
    parent.join(format!(".wallet-vault-{}.tmp", hex::encode(suffix)))
}

#[cfg(not(windows))]
fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), WalletVaultError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(map_storage_error)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| WalletVaultError::StorageUnavailable)
}

fn map_storage_error(error: std::io::Error) -> WalletVaultError {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        WalletVaultError::VaultAlreadyExists
    } else {
        WalletVaultError::StorageUnavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password(value: &str) -> WalletPassword {
        WalletPassword::new(value.to_string())
    }

    fn test_vault() -> EncryptedWalletVault {
        EncryptedWalletVault::encrypt_for_test(
            "primary_wallet",
            1_700_000_000_000,
            &WalletSeed::from_bytes([0x5a; SEED_BYTES]),
            &password("correct horse battery staple"),
        )
        .unwrap()
    }

    #[test]
    fn encrypted_vault_round_trips_seed() {
        let vault = test_vault();
        let unlocked = vault
            .unlock(&password("correct horse battery staple"))
            .unwrap();

        assert!(unlocked.with_exposed(|bytes| bytes == &[0x5a; SEED_BYTES]));
    }

    #[test]
    fn vault_never_serializes_plaintext_seed_or_password() {
        let vault = test_vault();
        let json = String::from_utf8(vault.to_json().unwrap()).unwrap();

        assert!(!json.contains(&"5a".repeat(SEED_BYTES)));
        assert!(!json.contains("correct horse battery staple"));
        assert!(!json.contains("password"));
        assert!(!json.contains("seed"));
    }

    #[test]
    fn wrong_password_and_ciphertext_damage_share_one_error() {
        let vault = test_vault();
        let wrong_password_error = vault
            .unlock(&password("this is definitely the wrong password"))
            .unwrap_err();
        let mut damaged = vault;
        damaged.cipher.ciphertext_hex.replace_range(0..2, "00");
        let damage_error = damaged
            .unlock(&password("correct horse battery staple"))
            .unwrap_err();

        assert_eq!(
            wrong_password_error,
            WalletVaultError::InvalidPasswordOrCorruptVault
        );
        assert_eq!(damage_error, wrong_password_error);
    }

    #[test]
    fn authenticated_metadata_cannot_be_modified() {
        let mut vault = test_vault();
        vault.created_at_unix_ms += 1;

        assert_eq!(
            vault
                .unlock(&password("correct horse battery staple"))
                .unwrap_err(),
            WalletVaultError::InvalidPasswordOrCorruptVault
        );
    }

    #[test]
    fn protected_device_key_cannot_be_modified() {
        let mut vault = test_vault();
        let replacement = if vault.device_protection.protected_key_hex.starts_with("00") {
            "ff"
        } else {
            "00"
        };
        vault
            .device_protection
            .protected_key_hex
            .replace_range(0..2, replacement);

        assert_eq!(
            vault
                .unlock(&password("correct horse battery staple"))
                .unwrap_err(),
            WalletVaultError::InvalidPasswordOrCorruptVault
        );
    }

    #[test]
    fn unique_randomness_produces_different_vaults() {
        let first = test_vault();
        let second = test_vault();

        assert_ne!(first.kdf.salt_hex, second.kdf.salt_hex);
        assert_ne!(first.cipher.nonce_hex, second.cipher.nonce_hex);
        assert_ne!(first.cipher.ciphertext_hex, second.cipher.ciphertext_hex);
    }

    #[test]
    fn password_policy_rejects_short_and_oversized_values() {
        let seed = WalletSeed::from_bytes([1; SEED_BYTES]);
        assert_eq!(
            validate_password(&password(&"x".repeat(MIN_PASSWORD_BYTES - 1))).unwrap_err(),
            WalletVaultError::PasswordPolicy
        );
        assert_eq!(
            validate_password(&password(&"x".repeat(MIN_PASSWORD_BYTES))),
            Ok(())
        );
        assert_eq!(
            EncryptedWalletVault::encrypt("wallet", 1, &seed, &password(&"x".repeat(1025)))
                .unwrap_err(),
            WalletVaultError::PasswordPolicy
        );
    }

    #[test]
    fn format_limits_reject_unsupported_or_oversized_inputs_before_kdf() {
        assert_eq!(
            EncryptedWalletVault::from_json(&vec![b' '; MAX_VAULT_JSON_BYTES + 1]).unwrap_err(),
            WalletVaultError::InvalidOrUnsupportedFormat
        );
        let mut vault = test_vault();
        vault.kdf.memory_kib += 1;
        let json = serde_json::to_vec(&vault).unwrap();
        assert_eq!(
            EncryptedWalletVault::from_json(&json).unwrap_err(),
            WalletVaultError::InvalidOrUnsupportedFormat
        );

        let mut unexpected = serde_json::to_value(test_vault()).unwrap();
        unexpected["unexpected_secret_field"] = serde_json::Value::String("rejected".to_string());
        assert_eq!(
            EncryptedWalletVault::from_json(&serde_json::to_vec(&unexpected).unwrap()).unwrap_err(),
            WalletVaultError::InvalidOrUnsupportedFormat
        );
    }

    #[test]
    fn encrypted_store_round_trips_without_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("primary.wallet.json");
        let vault = test_vault();

        store_new_vault(&path, &vault).unwrap();
        let stored = fs::read_to_string(&path).unwrap();
        assert!(!stored.contains(&"5a".repeat(SEED_BYTES)));
        assert_eq!(load_vault(&path).unwrap(), vault);
        assert_eq!(
            store_new_vault(&path, &vault).unwrap_err(),
            WalletVaultError::VaultAlreadyExists
        );
    }

    #[test]
    fn invalid_wallet_identifiers_are_rejected() {
        let seed = WalletSeed::from_bytes([1; SEED_BYTES]);
        for invalid in ["", "../escape", "space wallet", "wallet|metadata"] {
            assert_eq!(
                EncryptedWalletVault::encrypt(
                    invalid,
                    1,
                    &seed,
                    &password("correct horse battery staple")
                )
                .unwrap_err(),
                WalletVaultError::InvalidWalletId
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_device_bound_vault_round_trips_through_dpapi() {
        let seed = WalletSeed::from_bytes([0x3c; SEED_BYTES]);
        let password = password("correct horse battery staple");
        let vault = EncryptedWalletVault::encrypt("windows_wallet", 1, &seed, &password).unwrap();

        assert_eq!(
            vault.device_protection.algorithm,
            device_protection::WINDOWS_DPAPI_ALGORITHM
        );
        let unlocked = vault.unlock(&password).unwrap();
        assert!(unlocked.with_exposed(|bytes| bytes == &[0x3c; SEED_BYTES]));
    }
}
