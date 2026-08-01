use super::secrets::{WalletPassword, WalletSeed};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{Read, Write},
    path::Path,
};
use zeroize::{Zeroize, Zeroizing};

const RECOVERY_SCHEMA: &str = "vision-desktop-portable-recovery";
const RECOVERY_VERSION: u16 = 1;
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
const MIN_RECOVERY_PASSWORD_BYTES: usize = 16;
const MAX_RECOVERY_PASSWORD_BYTES: usize = 1024;
const MAX_RECOVERY_JSON_BYTES: usize = 16 * 1024;

/// Password-encrypted backup of a wallet seed that is intentionally independent
/// of the local DPAPI-bound vault.
///
/// This type remains private to the Rust wallet module. It does not establish a
/// mnemonic, frontend export UI, clipboard path, or automatic destination.
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::wallet) struct PortableRecoveryArtifact {
    schema: String,
    version: u16,
    wallet_id: String,
    created_at_unix_ms: u64,
    kdf: RecoveryKdf,
    cipher: RecoveryCipher,
}

impl fmt::Debug for PortableRecoveryArtifact {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PortableRecoveryArtifact([ENCRYPTED])")
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RecoveryKdf {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
    salt_hex: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RecoveryCipher {
    algorithm: String,
    nonce_hex: String,
    ciphertext_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum RecoveryArtifactError {
    InvalidWalletId,
    PasswordPolicy,
    InvalidOrUnsupportedFormat,
    InvalidPasswordOrDamagedArtifact,
    RandomSourceUnavailable,
    SerializationUnavailable,
    StorageUnavailable,
    ArtifactAlreadyExists,
}

impl fmt::Display for RecoveryArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidWalletId => "wallet identifier is invalid",
            Self::PasswordPolicy => "recovery password does not meet the local security policy",
            Self::InvalidOrUnsupportedFormat => {
                "portable recovery artifact format is invalid or unsupported"
            }
            Self::InvalidPasswordOrDamagedArtifact => {
                "recovery password is incorrect or the artifact is damaged"
            }
            Self::RandomSourceUnavailable => "secure operating-system randomness is unavailable",
            Self::SerializationUnavailable => "portable recovery serialization is unavailable",
            Self::StorageUnavailable => "portable recovery storage is unavailable",
            Self::ArtifactAlreadyExists => "portable recovery destination already exists",
        };
        formatter.write_str(message)
    }
}

/// Stores an encrypted portable artifact at an explicitly selected path.
/// Parent directories are never created and existing files are never replaced.
pub(in crate::wallet) fn store_new_recovery_artifact(
    path: &Path,
    artifact: &PortableRecoveryArtifact,
) -> Result<(), RecoveryArtifactError> {
    let parent = path
        .parent()
        .ok_or(RecoveryArtifactError::StorageUnavailable)?;
    let parent_metadata =
        fs::symlink_metadata(parent).map_err(|_| RecoveryArtifactError::StorageUnavailable)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(RecoveryArtifactError::StorageUnavailable);
    }
    let bytes = Zeroizing::new(artifact.to_json()?);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path).map_err(map_storage_error)?;
    let result = file
        .write_all(bytes.as_slice())
        .and_then(|_| file.sync_all())
        .map_err(|_| RecoveryArtifactError::StorageUnavailable);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

/// Loads only a bounded, regular encrypted artifact from the selected path.
pub(in crate::wallet) fn load_recovery_artifact(
    path: &Path,
) -> Result<PortableRecoveryArtifact, RecoveryArtifactError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| RecoveryArtifactError::StorageUnavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_RECOVERY_JSON_BYTES as u64
    {
        return Err(RecoveryArtifactError::InvalidOrUnsupportedFormat);
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(metadata.len() as usize));
    fs::File::open(path)
        .map_err(|_| RecoveryArtifactError::StorageUnavailable)?
        .take(MAX_RECOVERY_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RecoveryArtifactError::StorageUnavailable)?;
    if bytes.len() > MAX_RECOVERY_JSON_BYTES {
        return Err(RecoveryArtifactError::InvalidOrUnsupportedFormat);
    }
    PortableRecoveryArtifact::from_json(bytes.as_slice())
}

fn map_storage_error(error: std::io::Error) -> RecoveryArtifactError {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        RecoveryArtifactError::ArtifactAlreadyExists
    } else {
        RecoveryArtifactError::StorageUnavailable
    }
}

impl std::error::Error for RecoveryArtifactError {}

impl PortableRecoveryArtifact {
    /// Creates an in-memory portable artifact without writing a file or exposing
    /// a command to the frontend.
    pub(in crate::wallet) fn encrypt(
        wallet_id: &str,
        created_at_unix_ms: u64,
        seed: &WalletSeed,
        recovery_password: &WalletPassword,
    ) -> Result<Self, RecoveryArtifactError> {
        validate_wallet_id(wallet_id)?;
        validate_recovery_password(recovery_password)?;

        let mut salt = [0_u8; SALT_BYTES];
        let mut nonce = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut salt).map_err(|_| RecoveryArtifactError::RandomSourceUnavailable)?;
        getrandom::fill(&mut nonce).map_err(|_| RecoveryArtifactError::RandomSourceUnavailable)?;

        let mut artifact = Self {
            schema: RECOVERY_SCHEMA.to_string(),
            version: RECOVERY_VERSION,
            wallet_id: wallet_id.to_string(),
            created_at_unix_ms,
            kdf: RecoveryKdf {
                algorithm: KDF_ALGORITHM.to_string(),
                version: KDF_VERSION,
                memory_kib: KDF_MEMORY_KIB,
                iterations: KDF_ITERATIONS,
                lanes: KDF_LANES,
                salt_hex: hex::encode(salt),
            },
            cipher: RecoveryCipher {
                algorithm: CIPHER_ALGORITHM.to_string(),
                nonce_hex: hex::encode(nonce),
                ciphertext_hex: String::new(),
            },
        };

        let key = derive_recovery_key(recovery_password, &salt)?;
        let mut cipher_key = Key::try_from(key.as_ref())
            .map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)?;
        let cipher_nonce = XNonce::try_from(nonce.as_slice())
            .map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)?;
        let cipher = XChaCha20Poly1305::new(&cipher_key);
        cipher_key.as_mut_slice().zeroize();
        let aad = artifact.aad();
        let ciphertext = seed
            .with_exposed(|seed_bytes| {
                cipher.encrypt(
                    &cipher_nonce,
                    Payload {
                        msg: seed_bytes,
                        aad: &aad,
                    },
                )
            })
            .map_err(|_| RecoveryArtifactError::InvalidPasswordOrDamagedArtifact)?;
        artifact.cipher.ciphertext_hex = hex::encode(ciphertext);
        Ok(artifact)
    }

    /// Restores only the original opaque seed. Vision key and address derivation
    /// remains gated by the separately approved Core compatibility contract.
    pub(in crate::wallet) fn restore(
        &self,
        recovery_password: &WalletPassword,
    ) -> Result<WalletSeed, RecoveryArtifactError> {
        self.validate()?;
        validate_recovery_password(recovery_password)?;

        let salt = decode_fixed::<SALT_BYTES>(&self.kdf.salt_hex)?;
        let nonce = decode_fixed::<NONCE_BYTES>(&self.cipher.nonce_hex)?;
        let ciphertext =
            decode_fixed::<{ SEED_BYTES + AUTH_TAG_BYTES }>(&self.cipher.ciphertext_hex)?;
        let key = derive_recovery_key(recovery_password, &salt)?;
        let mut cipher_key = Key::try_from(key.as_ref())
            .map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)?;
        let cipher_nonce = XNonce::try_from(nonce.as_slice())
            .map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)?;
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
                .map_err(|_| RecoveryArtifactError::InvalidPasswordOrDamagedArtifact)?,
        );
        let seed_bytes: [u8; SEED_BYTES] = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| RecoveryArtifactError::InvalidPasswordOrDamagedArtifact)?;
        Ok(WalletSeed::from_bytes(seed_bytes))
    }

    pub(in crate::wallet) fn from_json(input: &[u8]) -> Result<Self, RecoveryArtifactError> {
        if input.is_empty() || input.len() > MAX_RECOVERY_JSON_BYTES {
            return Err(RecoveryArtifactError::InvalidOrUnsupportedFormat);
        }
        let artifact: Self = serde_json::from_slice(input)
            .map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub(in crate::wallet) fn to_json(&self) -> Result<Vec<u8>, RecoveryArtifactError> {
        self.validate()?;
        serde_json::to_vec_pretty(self).map_err(|_| RecoveryArtifactError::SerializationUnavailable)
    }

    fn validate(&self) -> Result<(), RecoveryArtifactError> {
        validate_wallet_id(&self.wallet_id)?;
        if self.schema != RECOVERY_SCHEMA
            || self.version != RECOVERY_VERSION
            || self.kdf.algorithm != KDF_ALGORITHM
            || self.kdf.version != KDF_VERSION
            || self.kdf.memory_kib != KDF_MEMORY_KIB
            || self.kdf.iterations != KDF_ITERATIONS
            || self.kdf.lanes != KDF_LANES
            || self.cipher.algorithm != CIPHER_ALGORITHM
            || self.kdf.salt_hex.len() != SALT_BYTES * 2
            || self.cipher.nonce_hex.len() != NONCE_BYTES * 2
            || self.cipher.ciphertext_hex.len() != (SEED_BYTES + AUTH_TAG_BYTES) * 2
        {
            return Err(RecoveryArtifactError::InvalidOrUnsupportedFormat);
        }
        decode_fixed::<SALT_BYTES>(&self.kdf.salt_hex)?;
        decode_fixed::<NONCE_BYTES>(&self.cipher.nonce_hex)?;
        decode_fixed::<{ SEED_BYTES + AUTH_TAG_BYTES }>(&self.cipher.ciphertext_hex)?;
        Ok(())
    }

    fn aad(&self) -> Vec<u8> {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
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
            self.cipher.algorithm,
            self.cipher.nonce_hex
        )
        .into_bytes()
    }
}

fn derive_recovery_key(
    password: &WalletPassword,
    salt: &[u8; SALT_BYTES],
) -> Result<Zeroizing<[u8; 32]>, RecoveryArtifactError> {
    let params = Params::new(KDF_MEMORY_KIB, KDF_ITERATIONS, KDF_LANES, Some(32))
        .map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; 32]);
    password
        .with_exposed(|bytes| argon2.hash_password_into(bytes, salt, key.as_mut()))
        .map_err(|_| RecoveryArtifactError::InvalidPasswordOrDamagedArtifact)?;
    Ok(key)
}

fn validate_wallet_id(wallet_id: &str) -> Result<(), RecoveryArtifactError> {
    if wallet_id.is_empty()
        || wallet_id.len() > 64
        || !wallet_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(RecoveryArtifactError::InvalidWalletId);
    }
    Ok(())
}

fn validate_recovery_password(password: &WalletPassword) -> Result<(), RecoveryArtifactError> {
    let valid = password.with_exposed(|bytes| {
        (MIN_RECOVERY_PASSWORD_BYTES..=MAX_RECOVERY_PASSWORD_BYTES).contains(&bytes.len())
    });
    if valid {
        Ok(())
    } else {
        Err(RecoveryArtifactError::PasswordPolicy)
    }
}

fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], RecoveryArtifactError> {
    let decoded =
        hex::decode(value).map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)?;
    decoded
        .try_into()
        .map_err(|_| RecoveryArtifactError::InvalidOrUnsupportedFormat)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PASSWORD: &str = "independent offline recovery password";

    fn password(value: &str) -> WalletPassword {
        WalletPassword::new(value.to_string())
    }

    fn test_artifact() -> PortableRecoveryArtifact {
        PortableRecoveryArtifact::encrypt(
            "primary_wallet",
            1_700_000_000_000,
            &WalletSeed::from_bytes([0x6b; SEED_BYTES]),
            &password(TEST_PASSWORD),
        )
        .unwrap()
    }

    #[test]
    fn portable_artifact_round_trips_the_exact_seed() {
        let artifact = test_artifact();
        let serialized = artifact.to_json().unwrap();
        let parsed = PortableRecoveryArtifact::from_json(&serialized).unwrap();
        let restored = parsed.restore(&password(TEST_PASSWORD)).unwrap();

        assert!(restored.with_exposed(|bytes| bytes == &[0x6b; SEED_BYTES]));
    }

    #[test]
    fn debug_output_does_not_dump_encrypted_artifact_contents() {
        assert_eq!(
            format!("{:?}", test_artifact()),
            "PortableRecoveryArtifact([ENCRYPTED])"
        );
    }

    #[test]
    fn artifact_is_password_only_and_contains_no_device_binding() {
        let json = String::from_utf8(test_artifact().to_json().unwrap()).unwrap();

        assert!(!json.contains("device_protection"));
        assert!(!json.contains("dpapi"));
        assert!(!json.contains("protected_key"));
    }

    #[test]
    fn artifact_never_serializes_plaintext_seed_or_password() {
        let json = String::from_utf8(test_artifact().to_json().unwrap()).unwrap();

        assert!(!json.contains(&"6b".repeat(SEED_BYTES)));
        assert!(!json.contains(TEST_PASSWORD));
        assert!(!json.contains("password"));
        assert!(!json.contains("seed"));
    }

    #[test]
    fn wrong_password_and_ciphertext_damage_share_one_error() {
        let artifact = test_artifact();
        let wrong_password_error = artifact
            .restore(&password("a different recovery password"))
            .unwrap_err();
        let mut damaged = artifact;
        let replacement = if damaged.cipher.ciphertext_hex.starts_with("00") {
            "ff"
        } else {
            "00"
        };
        damaged
            .cipher
            .ciphertext_hex
            .replace_range(0..2, replacement);
        let damage_error = damaged.restore(&password(TEST_PASSWORD)).unwrap_err();

        assert_eq!(
            wrong_password_error,
            RecoveryArtifactError::InvalidPasswordOrDamagedArtifact
        );
        assert_eq!(damage_error, wrong_password_error);
    }

    #[test]
    fn authenticated_metadata_cannot_be_modified() {
        let mut artifact = test_artifact();
        artifact.created_at_unix_ms += 1;

        assert_eq!(
            artifact.restore(&password(TEST_PASSWORD)).unwrap_err(),
            RecoveryArtifactError::InvalidPasswordOrDamagedArtifact
        );
    }

    #[test]
    fn separate_artifacts_use_unique_salts_nonces_and_ciphertext() {
        let first = test_artifact();
        let second = test_artifact();

        assert_ne!(first.kdf.salt_hex, second.kdf.salt_hex);
        assert_ne!(first.cipher.nonce_hex, second.cipher.nonce_hex);
        assert_ne!(first.cipher.ciphertext_hex, second.cipher.ciphertext_hex);
    }

    #[test]
    fn recovery_password_policy_rejects_short_and_oversized_values() {
        let seed = WalletSeed::from_bytes([1; SEED_BYTES]);
        assert_eq!(
            PortableRecoveryArtifact::encrypt("wallet", 1, &seed, &password("too-short"))
                .unwrap_err(),
            RecoveryArtifactError::PasswordPolicy
        );
        assert_eq!(
            PortableRecoveryArtifact::encrypt(
                "wallet",
                1,
                &seed,
                &password(&"x".repeat(MAX_RECOVERY_PASSWORD_BYTES + 1)),
            )
            .unwrap_err(),
            RecoveryArtifactError::PasswordPolicy
        );
    }

    #[test]
    fn parser_rejects_oversized_unknown_or_unsupported_inputs_before_kdf() {
        assert_eq!(
            PortableRecoveryArtifact::from_json(&vec![b' '; MAX_RECOVERY_JSON_BYTES + 1])
                .unwrap_err(),
            RecoveryArtifactError::InvalidOrUnsupportedFormat
        );

        let mut unsupported = test_artifact();
        unsupported.kdf.memory_kib += 1;
        assert_eq!(
            PortableRecoveryArtifact::from_json(&serde_json::to_vec(&unsupported).unwrap())
                .unwrap_err(),
            RecoveryArtifactError::InvalidOrUnsupportedFormat
        );

        let mut unexpected = serde_json::to_value(test_artifact()).unwrap();
        unexpected["unexpected_secret_field"] = serde_json::Value::String("rejected".to_string());
        assert_eq!(
            PortableRecoveryArtifact::from_json(&serde_json::to_vec(&unexpected).unwrap())
                .unwrap_err(),
            RecoveryArtifactError::InvalidOrUnsupportedFormat
        );
    }

    #[test]
    fn invalid_wallet_identifiers_are_rejected() {
        let seed = WalletSeed::from_bytes([1; SEED_BYTES]);
        for invalid in ["", "../escape", "space wallet", "wallet|metadata"] {
            assert_eq!(
                PortableRecoveryArtifact::encrypt(invalid, 1, &seed, &password(TEST_PASSWORD),)
                    .unwrap_err(),
                RecoveryArtifactError::InvalidWalletId
            );
        }
    }
}
