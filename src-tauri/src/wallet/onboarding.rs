#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wallet onboarding remains internal until independent security review"
    )
)]

use super::{
    account::derive_account_identity,
    contract::WalletPublicMetadata,
    recovery::{
        load_recovery_artifact, store_new_recovery_artifact, PortableRecoveryArtifact,
        RecoveryArtifactError,
    },
    secrets::{WalletPassword, WalletSeed},
    vault::{store_new_vault, EncryptedWalletVault, WalletVaultError},
};
use std::{fmt, path::Path};

const SEED_BYTES: usize = 32;
const MAX_LABEL_BYTES: usize = 64;

pub(in crate::wallet) struct PreparedWalletOnboarding {
    metadata: WalletPublicMetadata,
    vault: Option<EncryptedWalletVault>,
    recovery_artifact: PortableRecoveryArtifact,
}

impl fmt::Debug for PreparedWalletOnboarding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedWalletOnboarding([ENCRYPTED, UNVERIFIED])")
    }
}

pub(in crate::wallet) struct VerifiedWalletOnboarding {
    metadata: WalletPublicMetadata,
    vault: Option<EncryptedWalletVault>,
}

impl fmt::Debug for VerifiedWalletOnboarding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VerifiedWalletOnboarding([ENCRYPTED, LOCKED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletOnboardingError {
    InvalidLabel,
    PasswordsMustDiffer,
    SecureRandomUnavailable,
    VaultProtectionUnavailable,
    RecoveryProtectionUnavailable,
    RecoveryDestinationExists,
    RecoveryStorageUnavailable,
    RecoveryBackupMismatch,
    RecoveryPasswordOrDamage,
    OnboardingAlreadyCompleted,
    VaultStorageUnavailable,
}

impl fmt::Display for WalletOnboardingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLabel => "wallet label is invalid",
            Self::PasswordsMustDiffer => "wallet and recovery passwords must be different",
            Self::SecureRandomUnavailable => "secure operating-system randomness is unavailable",
            Self::VaultProtectionUnavailable => "local wallet protection is unavailable",
            Self::RecoveryProtectionUnavailable => "portable recovery protection is unavailable",
            Self::RecoveryDestinationExists => "the recovery destination already exists",
            Self::RecoveryStorageUnavailable => "the recovery destination is unavailable",
            Self::RecoveryBackupMismatch => "the saved recovery backup does not match this wallet",
            Self::RecoveryPasswordOrDamage => {
                "the recovery password is incorrect or the backup is damaged"
            }
            Self::OnboardingAlreadyCompleted => "wallet onboarding is already completed",
            Self::VaultStorageUnavailable => "secure local wallet storage is unavailable",
        })
    }
}

impl std::error::Error for WalletOnboardingError {}

/// Prepares encrypted wallet material entirely inside Rust. The returned value
/// is deliberately unable to store the local vault until its portable backup
/// has been read back, decrypted, and matched to the same public identity.
pub(in crate::wallet) fn prepare_new_wallet(
    wallet_id: &str,
    label: &str,
    created_at_unix_ms: u64,
    wallet_password: &WalletPassword,
    recovery_password: &WalletPassword,
) -> Result<PreparedWalletOnboarding, WalletOnboardingError> {
    validate_label(label)?;
    require_distinct_passwords(wallet_password, recovery_password)?;
    let mut seed_bytes = [0_u8; SEED_BYTES];
    getrandom::fill(&mut seed_bytes).map_err(|_| WalletOnboardingError::SecureRandomUnavailable)?;
    let seed = WalletSeed::from_bytes(seed_bytes);
    prepare_with_vault(
        wallet_id,
        label,
        created_at_unix_ms,
        &seed,
        wallet_password,
        recovery_password,
        EncryptedWalletVault::encrypt(wallet_id, created_at_unix_ms, &seed, wallet_password)
            .map_err(map_vault_protection_error)?,
    )
}

impl PreparedWalletOnboarding {
    pub(in crate::wallet) fn public_metadata(&self) -> &WalletPublicMetadata {
        &self.metadata
    }

    /// Writes only the independently encrypted portable artifact. The caller
    /// must supply a path selected explicitly by the user.
    pub(in crate::wallet) fn store_recovery_backup(
        &self,
        selected_path: &Path,
    ) -> Result<(), WalletOnboardingError> {
        store_new_recovery_artifact(selected_path, &self.recovery_artifact)
            .map_err(map_recovery_storage_error)
    }

    /// Reads the selected file back and proves that it restores the exact
    /// public account identity before releasing the encrypted vault for local
    /// storage. The recovery password never enters metadata or persistence.
    pub(in crate::wallet) fn verify_stored_recovery(
        &mut self,
        selected_path: &Path,
        recovery_password: &WalletPassword,
    ) -> Result<VerifiedWalletOnboarding, WalletOnboardingError> {
        let stored = load_recovery_artifact(selected_path).map_err(map_recovery_read_error)?;
        if stored != self.recovery_artifact {
            return Err(WalletOnboardingError::RecoveryBackupMismatch);
        }
        let restored_seed = stored
            .restore(recovery_password)
            .map_err(map_recovery_restore_error)?;
        if derive_account_identity(&restored_seed).address != self.metadata.address {
            return Err(WalletOnboardingError::RecoveryBackupMismatch);
        }
        let vault = self
            .vault
            .take()
            .ok_or(WalletOnboardingError::OnboardingAlreadyCompleted)?;
        let mut metadata = self.metadata.clone();
        metadata.backup_verified = true;
        Ok(VerifiedWalletOnboarding {
            metadata,
            vault: Some(vault),
        })
    }
}

impl VerifiedWalletOnboarding {
    pub(in crate::wallet) fn public_metadata(&self) -> &WalletPublicMetadata {
        &self.metadata
    }

    /// Stores the device-bound local vault only after backup verification.
    /// The resulting public wallet remains locked by default.
    pub(in crate::wallet) fn store_local_vault(
        &mut self,
        vault_path: &Path,
    ) -> Result<WalletPublicMetadata, WalletOnboardingError> {
        let vault = self
            .vault
            .as_ref()
            .ok_or(WalletOnboardingError::OnboardingAlreadyCompleted)?;
        store_new_vault(vault_path, vault).map_err(map_vault_storage_error)?;
        self.vault = None;
        Ok(self.metadata.clone())
    }
}

fn prepare_with_vault(
    wallet_id: &str,
    label: &str,
    created_at_unix_ms: u64,
    seed: &WalletSeed,
    _wallet_password: &WalletPassword,
    recovery_password: &WalletPassword,
    vault: EncryptedWalletVault,
) -> Result<PreparedWalletOnboarding, WalletOnboardingError> {
    let identity = derive_account_identity(seed);
    let recovery_artifact =
        PortableRecoveryArtifact::encrypt(wallet_id, created_at_unix_ms, seed, recovery_password)
            .map_err(map_recovery_protection_error)?;
    Ok(PreparedWalletOnboarding {
        metadata: WalletPublicMetadata {
            wallet_id: wallet_id.to_string(),
            label: label.to_string(),
            public_key: identity.public_key,
            address: identity.address,
            created_at_unix_ms,
            locked: true,
            backup_verified: false,
        },
        vault: Some(vault),
        recovery_artifact,
    })
}

fn validate_label(label: &str) -> Result<(), WalletOnboardingError> {
    if label.is_empty()
        || label.len() > MAX_LABEL_BYTES
        || label.chars().any(char::is_control)
        || label.trim() != label
    {
        return Err(WalletOnboardingError::InvalidLabel);
    }
    Ok(())
}

fn require_distinct_passwords(
    wallet_password: &WalletPassword,
    recovery_password: &WalletPassword,
) -> Result<(), WalletOnboardingError> {
    let same = wallet_password.with_exposed(|wallet_bytes| {
        recovery_password.with_exposed(|recovery_bytes| wallet_bytes == recovery_bytes)
    });
    if same {
        Err(WalletOnboardingError::PasswordsMustDiffer)
    } else {
        Ok(())
    }
}

fn map_vault_protection_error(_error: WalletVaultError) -> WalletOnboardingError {
    WalletOnboardingError::VaultProtectionUnavailable
}

fn map_recovery_protection_error(_error: RecoveryArtifactError) -> WalletOnboardingError {
    WalletOnboardingError::RecoveryProtectionUnavailable
}

fn map_recovery_storage_error(error: RecoveryArtifactError) -> WalletOnboardingError {
    match error {
        RecoveryArtifactError::ArtifactAlreadyExists => {
            WalletOnboardingError::RecoveryDestinationExists
        }
        _ => WalletOnboardingError::RecoveryStorageUnavailable,
    }
}

fn map_recovery_read_error(error: RecoveryArtifactError) -> WalletOnboardingError {
    match error {
        RecoveryArtifactError::InvalidOrUnsupportedFormat => {
            WalletOnboardingError::RecoveryBackupMismatch
        }
        _ => WalletOnboardingError::RecoveryStorageUnavailable,
    }
}

fn map_recovery_restore_error(error: RecoveryArtifactError) -> WalletOnboardingError {
    match error {
        RecoveryArtifactError::InvalidPasswordOrDamagedArtifact => {
            WalletOnboardingError::RecoveryPasswordOrDamage
        }
        _ => WalletOnboardingError::RecoveryBackupMismatch,
    }
}

fn map_vault_storage_error(_error: WalletVaultError) -> WalletOnboardingError {
    WalletOnboardingError::VaultStorageUnavailable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::vault::load_vault;
    use std::fs;

    const WALLET_PASSWORD: &str = "local wallet password";
    const RECOVERY_PASSWORD: &str = "different offline recovery password";

    fn password(value: &str) -> WalletPassword {
        WalletPassword::new(value.to_string())
    }

    #[test]
    fn production_entry_point_remains_part_of_the_internal_contract() {
        let entry: fn(
            &str,
            &str,
            u64,
            &WalletPassword,
            &WalletPassword,
        ) -> Result<PreparedWalletOnboarding, WalletOnboardingError> = prepare_new_wallet;
        std::hint::black_box(entry);
    }

    fn prepared() -> PreparedWalletOnboarding {
        let seed = WalletSeed::from_bytes([7_u8; SEED_BYTES]);
        let wallet_password = password(WALLET_PASSWORD);
        let recovery_password = password(RECOVERY_PASSWORD);
        let vault = EncryptedWalletVault::encrypt_for_test(
            "primary",
            1_700_000_000_000,
            &seed,
            &wallet_password,
        )
        .unwrap();
        prepare_with_vault(
            "primary",
            "Primary Wallet",
            1_700_000_000_000,
            &seed,
            &wallet_password,
            &recovery_password,
            vault,
        )
        .unwrap()
    }

    #[test]
    fn separate_passwords_and_clean_labels_are_mandatory_before_generation() {
        let same = password("same password long enough");
        assert_eq!(
            require_distinct_passwords(&same, &same).unwrap_err(),
            WalletOnboardingError::PasswordsMustDiffer
        );
        for label in ["", " leading", "trailing ", "line\nbreak"] {
            assert_eq!(
                validate_label(label).unwrap_err(),
                WalletOnboardingError::InvalidLabel
            );
        }
    }

    #[test]
    fn recovery_must_be_stored_and_verified_before_vault_storage() {
        let directory = tempfile::tempdir().unwrap();
        let backup_path = directory.path().join("primary-recovery.json");
        let vault_path = directory.path().join("wallets").join("primary.json");
        let mut prepared = prepared();
        assert!(!prepared.public_metadata().backup_verified);
        assert!(prepared.public_metadata().locked);

        prepared.store_recovery_backup(&backup_path).unwrap();
        let backup_json = fs::read_to_string(&backup_path).unwrap();
        for forbidden in [
            WALLET_PASSWORD,
            RECOVERY_PASSWORD,
            &"07".repeat(SEED_BYTES),
            "private_key",
            "mnemonic",
        ] {
            assert!(!backup_json.contains(forbidden));
        }

        let mut verified = prepared
            .verify_stored_recovery(&backup_path, &password(RECOVERY_PASSWORD))
            .unwrap();
        assert!(verified.public_metadata().backup_verified);
        assert!(verified.public_metadata().locked);
        let metadata = verified.store_local_vault(&vault_path).unwrap();
        assert!(metadata.backup_verified);
        assert!(metadata.locked);

        let vault = load_vault(&vault_path).unwrap();
        let restored = vault.unlock(&password(WALLET_PASSWORD)).unwrap();
        assert_eq!(derive_account_identity(&restored).address, metadata.address);
    }

    #[test]
    fn wrong_password_or_changed_backup_never_releases_the_vault() {
        let directory = tempfile::tempdir().unwrap();
        let backup_path = directory.path().join("primary-recovery.json");
        let mut prepared = prepared();
        prepared.store_recovery_backup(&backup_path).unwrap();
        assert_eq!(
            prepared
                .verify_stored_recovery(
                    &backup_path,
                    &password("wrong but sufficiently long recovery password"),
                )
                .unwrap_err(),
            WalletOnboardingError::RecoveryPasswordOrDamage
        );

        let mut bytes = fs::read(&backup_path).unwrap();
        let index = bytes.len() / 2;
        bytes[index] ^= 1;
        fs::write(&backup_path, bytes).unwrap();
        assert_eq!(
            prepared
                .verify_stored_recovery(&backup_path, &password(RECOVERY_PASSWORD))
                .unwrap_err(),
            WalletOnboardingError::RecoveryBackupMismatch
        );
    }

    #[test]
    fn existing_backup_and_vault_files_are_never_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let backup_path = directory.path().join("primary-recovery.json");
        let vault_path = directory.path().join("wallets").join("primary.json");
        let mut prepared = prepared();
        prepared.store_recovery_backup(&backup_path).unwrap();
        assert_eq!(
            prepared.store_recovery_backup(&backup_path).unwrap_err(),
            WalletOnboardingError::RecoveryDestinationExists
        );
        let mut verified = prepared
            .verify_stored_recovery(&backup_path, &password(RECOVERY_PASSWORD))
            .unwrap();
        verified.store_local_vault(&vault_path).unwrap();
        assert_eq!(
            verified.store_local_vault(&vault_path).unwrap_err(),
            WalletOnboardingError::OnboardingAlreadyCompleted
        );
    }

    #[test]
    fn debug_and_public_metadata_exclude_secret_material() {
        let prepared = prepared();
        assert_eq!(
            format!("{prepared:?}"),
            "PreparedWalletOnboarding([ENCRYPTED, UNVERIFIED])"
        );
        let json = serde_json::to_string(prepared.public_metadata()).unwrap();
        for forbidden in [
            WALLET_PASSWORD,
            RECOVERY_PASSWORD,
            "seed",
            "password",
            "recovery_artifact",
            "vault",
        ] {
            assert!(!json.contains(forbidden));
        }
    }
}
