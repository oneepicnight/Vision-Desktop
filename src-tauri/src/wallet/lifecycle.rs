#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wallet lifecycle adapters remain private until the command boundary passes review"
    )
)]

use super::{
    contract::WalletLifecycleStatus,
    onboarding::{prepare_new_wallet, prepare_restored_wallet, WalletOnboardingError},
    runtime::{RecoveryPathPurpose, WalletOperationKind, WalletRuntimeError, WalletRuntimeState},
    secret_input::SecretInput,
    session::WalletSessionError,
    vault::{load_vault, WalletVaultError},
};
use std::{
    env, fmt,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const WALLET_DIRECTORY: &str = "wallet";
const WALLET_VAULT_FILE: &str = "wallet.vault.json";

/// Private Rust-only orchestration for the first local wallet lifecycle.
///
/// No method is a Tauri command, and this type deliberately implements neither
/// Serde traits, `Clone`, nor `Debug`.
pub(crate) struct WalletLifecycleAdapters {
    runtime: Arc<WalletRuntimeState>,
    vault_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalletLifecycleError {
    RuntimeUnavailable,
    InvalidWindow,
    OperationInProgress,
    InvalidRequest,
    PathAuthorizationInvalid,
    PathAuthorizationExpired,
    WalletAlreadyExists,
    WalletUnavailable,
    InvalidLabel,
    PasswordPolicy,
    PasswordsMustDiffer,
    InvalidPasswordOrDamage,
    UnlockTemporarilyBlocked,
    SecureRandomUnavailable,
    RecoveryProtectionUnavailable,
    RecoveryDestinationExists,
    RecoveryStorageUnavailable,
    RecoveryBackupMismatch,
    VaultProtectionUnavailable,
    VaultStorageUnavailable,
    ClockUnavailable,
}

impl WalletLifecycleError {
    pub(in crate::wallet) const fn code(self) -> &'static str {
        match self {
            Self::RuntimeUnavailable => "wallet_runtime_unavailable",
            Self::InvalidWindow => "invalid_window",
            Self::OperationInProgress => "operation_in_progress",
            Self::InvalidRequest => "invalid_request",
            Self::PathAuthorizationInvalid => "path_authorization_invalid",
            Self::PathAuthorizationExpired => "path_authorization_expired",
            Self::WalletAlreadyExists => "wallet_already_exists",
            Self::WalletUnavailable => "wallet_unavailable",
            Self::InvalidLabel => "invalid_label",
            Self::PasswordPolicy => "password_policy",
            Self::PasswordsMustDiffer => "passwords_must_differ",
            Self::InvalidPasswordOrDamage => "invalid_password_or_damage",
            Self::UnlockTemporarilyBlocked => "unlock_temporarily_blocked",
            Self::SecureRandomUnavailable => "secure_random_unavailable",
            Self::RecoveryProtectionUnavailable => "recovery_protection_unavailable",
            Self::RecoveryDestinationExists => "recovery_destination_exists",
            Self::RecoveryStorageUnavailable => "recovery_storage_unavailable",
            Self::RecoveryBackupMismatch => "recovery_backup_mismatch",
            Self::VaultProtectionUnavailable => "vault_protection_unavailable",
            Self::VaultStorageUnavailable => "vault_storage_unavailable",
            Self::ClockUnavailable => "clock_unavailable",
        }
    }
}

impl fmt::Display for WalletLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RuntimeUnavailable => "secure wallet runtime is unavailable",
            Self::InvalidWindow => "wallet access is unavailable from this window",
            Self::OperationInProgress => "another wallet operation is already in progress",
            Self::InvalidRequest => "wallet request is invalid",
            Self::PathAuthorizationInvalid => "recovery selection is invalid",
            Self::PathAuthorizationExpired => "recovery selection has expired",
            Self::WalletAlreadyExists => "a local wallet already exists",
            Self::WalletUnavailable => "the local wallet is unavailable",
            Self::InvalidLabel => "wallet label is invalid",
            Self::PasswordPolicy => "wallet password does not meet the security policy",
            Self::PasswordsMustDiffer => "wallet and recovery passwords must be different",
            Self::InvalidPasswordOrDamage => {
                "the password is incorrect or encrypted wallet data is damaged"
            }
            Self::UnlockTemporarilyBlocked => {
                "wallet unlock is temporarily unavailable after repeated failures"
            }
            Self::SecureRandomUnavailable => "secure operating-system randomness is unavailable",
            Self::RecoveryProtectionUnavailable => "portable recovery protection is unavailable",
            Self::RecoveryDestinationExists => "the recovery destination already exists",
            Self::RecoveryStorageUnavailable => "the recovery file is unavailable",
            Self::RecoveryBackupMismatch => "the recovery backup does not match the wallet",
            Self::VaultProtectionUnavailable => "local wallet protection is unavailable",
            Self::VaultStorageUnavailable => "secure local wallet storage is unavailable",
            Self::ClockUnavailable => "secure wallet time is unavailable",
        })
    }
}

impl std::error::Error for WalletLifecycleError {}

impl WalletLifecycleAdapters {
    pub(crate) fn initialize(
        runtime: Arc<WalletRuntimeState>,
    ) -> Result<Self, WalletLifecycleError> {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .ok_or(WalletLifecycleError::VaultStorageUnavailable)?;
        Ok(Self {
            runtime,
            vault_path: PathBuf::from(local_app_data)
                .join("Vision")
                .join("Desktop")
                .join(WALLET_DIRECTORY)
                .join(WALLET_VAULT_FILE),
        })
    }

    pub(in crate::wallet) fn status(&self) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        match self.vault_path.try_exists() {
            Ok(false) => self
                .runtime
                .lifecycle_status(false)
                .map_err(map_runtime_error),
            Ok(true) => {
                let vault = load_vault(&self.vault_path).map_err(map_vault_load_error)?;
                self.runtime
                    .lifecycle_status_for_vault(&vault)
                    .map_err(map_runtime_error)
            }
            Err(_) => Err(WalletLifecycleError::WalletUnavailable),
        }
    }

    pub(in crate::wallet) fn create(
        &self,
        owner_window: &str,
        wallet_id: &str,
        label: &str,
        recovery_destination_token: &str,
        wallet_secret: SecretInput,
        recovery_secret: SecretInput,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.create_at(
            owner_window,
            wallet_id,
            label,
            recovery_destination_token,
            wallet_secret,
            recovery_secret,
            now_unix_ms()?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_at(
        &self,
        owner_window: &str,
        wallet_id: &str,
        label: &str,
        recovery_destination_token: &str,
        wallet_secret: SecretInput,
        recovery_secret: SecretInput,
        created_at_unix_ms: u64,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.require_vault_absent()?;
        let operation = self
            .runtime
            .begin_operation(owner_window, WalletOperationKind::Create)
            .map_err(map_runtime_error)?;
        let recovery_path = self
            .runtime
            .consume_recovery_path(
                owner_window,
                RecoveryPathPurpose::Destination,
                recovery_destination_token,
            )
            .map_err(map_runtime_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        let wallet_password = wallet_secret.into_wallet_password();
        let recovery_password = recovery_secret.into_wallet_password();
        let mut prepared = prepare_new_wallet(
            wallet_id,
            label,
            created_at_unix_ms,
            &wallet_password,
            &recovery_password,
        )
        .map_err(map_onboarding_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        prepared
            .store_recovery_backup(&recovery_path)
            .map_err(map_onboarding_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        let mut verified = prepared
            .verify_stored_recovery(&recovery_path, &recovery_password)
            .map_err(map_onboarding_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        let metadata = verified
            .store_local_vault(&self.vault_path)
            .map_err(map_onboarding_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        self.runtime
            .remember_public_metadata(metadata)
            .map_err(map_runtime_error)
    }

    pub(in crate::wallet) fn restore(
        &self,
        owner_window: &str,
        wallet_id: &str,
        label: &str,
        recovery_source_token: &str,
        new_wallet_secret: SecretInput,
        recovery_secret: SecretInput,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.restore_at(
            owner_window,
            wallet_id,
            label,
            recovery_source_token,
            new_wallet_secret,
            recovery_secret,
            now_unix_ms()?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn restore_at(
        &self,
        owner_window: &str,
        wallet_id: &str,
        label: &str,
        recovery_source_token: &str,
        new_wallet_secret: SecretInput,
        recovery_secret: SecretInput,
        created_at_unix_ms: u64,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.require_vault_absent()?;
        let operation = self
            .runtime
            .begin_operation(owner_window, WalletOperationKind::Restore)
            .map_err(map_runtime_error)?;
        let recovery_path = self
            .runtime
            .consume_recovery_path(
                owner_window,
                RecoveryPathPurpose::Source,
                recovery_source_token,
            )
            .map_err(map_runtime_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        let wallet_password = new_wallet_secret.into_wallet_password();
        let recovery_password = recovery_secret.into_wallet_password();
        let mut restored = prepare_restored_wallet(
            &recovery_path,
            wallet_id,
            label,
            created_at_unix_ms,
            &wallet_password,
            &recovery_password,
        )
        .map_err(map_onboarding_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        let metadata = restored
            .store_local_vault(&self.vault_path)
            .map_err(map_onboarding_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        self.runtime
            .remember_public_metadata(metadata)
            .map_err(map_runtime_error)
    }

    pub(in crate::wallet) fn unlock(
        &self,
        owner_window: &str,
        wallet_secret: SecretInput,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        let operation = self
            .runtime
            .begin_operation(owner_window, WalletOperationKind::Unlock)
            .map_err(map_runtime_error)?;
        let vault = load_vault(&self.vault_path).map_err(map_vault_load_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        let wallet_password = wallet_secret.into_wallet_password();
        let status = self
            .runtime
            .unlock_vault(&vault, &wallet_password)
            .map_err(map_session_error)?;
        operation.ensure_current().map_err(map_runtime_error)?;
        Ok(status)
    }

    pub(in crate::wallet) fn lock(&self) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.runtime.invalidate_all().map_err(map_runtime_error)?;
        self.status()
    }

    fn require_vault_absent(&self) -> Result<(), WalletLifecycleError> {
        match self.vault_path.try_exists() {
            Ok(false) => Ok(()),
            Ok(true) => Err(WalletLifecycleError::WalletAlreadyExists),
            Err(_) => Err(WalletLifecycleError::VaultStorageUnavailable),
        }
    }

    #[cfg(test)]
    fn for_test(runtime: Arc<WalletRuntimeState>, vault_path: &std::path::Path) -> Self {
        Self {
            runtime,
            vault_path: vault_path.to_path_buf(),
        }
    }
}

fn now_unix_ms() -> Result<u64, WalletLifecycleError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WalletLifecycleError::ClockUnavailable)?;
    u64::try_from(duration.as_millis()).map_err(|_| WalletLifecycleError::ClockUnavailable)
}

fn map_runtime_error(error: WalletRuntimeError) -> WalletLifecycleError {
    match error {
        WalletRuntimeError::InvalidWindow => WalletLifecycleError::InvalidWindow,
        WalletRuntimeError::OperationInProgress => WalletLifecycleError::OperationInProgress,
        WalletRuntimeError::InvalidRequest => WalletLifecycleError::InvalidRequest,
        WalletRuntimeError::PathAuthorizationInvalid => {
            WalletLifecycleError::PathAuthorizationInvalid
        }
        WalletRuntimeError::PathAuthorizationExpired => {
            WalletLifecycleError::PathAuthorizationExpired
        }
        WalletRuntimeError::SecureRandomUnavailable => {
            WalletLifecycleError::SecureRandomUnavailable
        }
        WalletRuntimeError::ProcessLockUnavailable
        | WalletRuntimeError::RuntimeUnavailable
        | WalletRuntimeError::RecoverySelectionCancelled
        | WalletRuntimeError::RecoveryDestinationInvalid
        | WalletRuntimeError::RecoveryDestinationExists
        | WalletRuntimeError::RecoverySourceInvalid => WalletLifecycleError::RuntimeUnavailable,
    }
}

fn map_onboarding_error(error: WalletOnboardingError) -> WalletLifecycleError {
    match error {
        WalletOnboardingError::InvalidLabel => WalletLifecycleError::InvalidLabel,
        WalletOnboardingError::PasswordsMustDiffer => WalletLifecycleError::PasswordsMustDiffer,
        WalletOnboardingError::SecureRandomUnavailable => {
            WalletLifecycleError::SecureRandomUnavailable
        }
        WalletOnboardingError::VaultProtectionUnavailable => {
            WalletLifecycleError::VaultProtectionUnavailable
        }
        WalletOnboardingError::RecoveryProtectionUnavailable => {
            WalletLifecycleError::RecoveryProtectionUnavailable
        }
        WalletOnboardingError::RecoveryDestinationExists => {
            WalletLifecycleError::RecoveryDestinationExists
        }
        WalletOnboardingError::RecoveryStorageUnavailable => {
            WalletLifecycleError::RecoveryStorageUnavailable
        }
        WalletOnboardingError::RecoveryBackupMismatch => {
            WalletLifecycleError::RecoveryBackupMismatch
        }
        WalletOnboardingError::RecoveryPasswordOrDamage => {
            WalletLifecycleError::InvalidPasswordOrDamage
        }
        WalletOnboardingError::OnboardingAlreadyCompleted => {
            WalletLifecycleError::RuntimeUnavailable
        }
        WalletOnboardingError::VaultStorageUnavailable => {
            WalletLifecycleError::VaultStorageUnavailable
        }
    }
}

fn map_vault_load_error(error: WalletVaultError) -> WalletLifecycleError {
    match error {
        WalletVaultError::PasswordPolicy => WalletLifecycleError::PasswordPolicy,
        WalletVaultError::InvalidPasswordOrCorruptVault => {
            WalletLifecycleError::InvalidPasswordOrDamage
        }
        WalletVaultError::VaultAlreadyExists => WalletLifecycleError::WalletAlreadyExists,
        WalletVaultError::InvalidWalletId
        | WalletVaultError::InvalidOrUnsupportedFormat
        | WalletVaultError::RandomSourceUnavailable
        | WalletVaultError::DeviceProtectionUnavailable
        | WalletVaultError::StorageUnavailable => WalletLifecycleError::WalletUnavailable,
    }
}

fn map_session_error(error: WalletSessionError) -> WalletLifecycleError {
    match error {
        WalletSessionError::Locked | WalletSessionError::VaultUnavailable => {
            WalletLifecycleError::WalletUnavailable
        }
        WalletSessionError::UnlockTemporarilyBlocked { .. } => {
            WalletLifecycleError::UnlockTemporarilyBlocked
        }
        WalletSessionError::InvalidPasswordOrCorruptVault => {
            WalletLifecycleError::InvalidPasswordOrDamage
        }
        WalletSessionError::PasswordPolicy => WalletLifecycleError::PasswordPolicy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::runtime::{RecoveryPathToken, RecoverySelectionPermit};
    use std::fs;

    const MAIN: &str = "main";
    const WALLET_PASSWORD: &str = "local wallet password";
    const RESTORED_PASSWORD: &str = "replacement wallet password";
    const RECOVERY_PASSWORD: &str = "different recovery password";

    fn secret(value: &str) -> SecretInput {
        serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
    }

    fn selection_token(
        runtime: &WalletRuntimeState,
        purpose: RecoveryPathPurpose,
        path: &std::path::Path,
    ) -> RecoveryPathToken {
        let permit: RecoverySelectionPermit = runtime
            .begin_recovery_path_selection(MAIN, purpose)
            .unwrap();
        runtime
            .complete_recovery_path_selection(permit, path.to_path_buf())
            .unwrap()
    }

    #[test]
    fn create_backup_unlock_lock_and_restore_complete_without_exposing_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let backup_path = directory.path().join("primary.vision-recovery.json");
        let first_runtime = Arc::new(WalletRuntimeState::for_test());
        let first_vault = directory.path().join("first").join(WALLET_VAULT_FILE);
        let first = WalletLifecycleAdapters::for_test(Arc::clone(&first_runtime), &first_vault);

        let destination = selection_token(
            &first_runtime,
            RecoveryPathPurpose::Destination,
            &backup_path,
        );
        let created = first
            .create(
                MAIN,
                "primary",
                "Primary Wallet",
                destination.as_str(),
                secret(WALLET_PASSWORD),
                secret(RECOVERY_PASSWORD),
            )
            .unwrap();
        assert!(created.vault_exists);
        assert!(created.locked);
        let created_account = created.account.unwrap();
        assert_eq!(created_account.label.as_deref(), Some("Primary Wallet"));
        assert_eq!(created_account.backup_verified, Some(true));
        assert!(backup_path.exists());
        assert!(first_vault.exists());

        assert_eq!(
            first
                .unlock(MAIN, secret("incorrect wallet password"))
                .unwrap_err(),
            WalletLifecycleError::InvalidPasswordOrDamage
        );
        let unlocked = first.unlock(MAIN, secret(WALLET_PASSWORD)).unwrap();
        assert!(!unlocked.locked);
        assert_eq!(
            unlocked.account.as_ref().unwrap().address,
            created_account.address
        );
        assert!(first.lock().unwrap().locked);

        let second_runtime = Arc::new(WalletRuntimeState::for_test());
        let second_vault = directory.path().join("second").join(WALLET_VAULT_FILE);
        let second = WalletLifecycleAdapters::for_test(Arc::clone(&second_runtime), &second_vault);
        let source = selection_token(&second_runtime, RecoveryPathPurpose::Source, &backup_path);
        let restored = second
            .restore(
                MAIN,
                "restored",
                "Restored Wallet",
                source.as_str(),
                secret(RESTORED_PASSWORD),
                secret(RECOVERY_PASSWORD),
            )
            .unwrap();
        assert!(restored.locked);
        assert_eq!(
            restored.account.as_ref().unwrap().address,
            created_account.address
        );
        assert!(
            !second
                .unlock(MAIN, secret(RESTORED_PASSWORD))
                .unwrap()
                .locked
        );

        let backup_text = fs::read_to_string(backup_path).unwrap();
        for forbidden in [
            WALLET_PASSWORD,
            RESTORED_PASSWORD,
            RECOVERY_PASSWORD,
            "mnemonic",
        ] {
            assert!(!backup_text.contains(forbidden));
        }
    }

    #[test]
    fn restart_status_and_unlock_do_not_invent_label_or_backup_verification() {
        let directory = tempfile::tempdir().unwrap();
        let backup_path = directory.path().join("restart.vision-recovery.json");
        let vault_path = directory.path().join("wallet").join(WALLET_VAULT_FILE);
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapter = WalletLifecycleAdapters::for_test(Arc::clone(&runtime), &vault_path);
        let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &backup_path);
        adapter
            .create_at(
                MAIN,
                "restart",
                "Restart Wallet",
                token.as_str(),
                secret(WALLET_PASSWORD),
                secret(RECOVERY_PASSWORD),
                1,
            )
            .unwrap();
        drop(adapter);
        drop(runtime);

        let restarted_runtime = Arc::new(WalletRuntimeState::for_test());
        let restarted =
            WalletLifecycleAdapters::for_test(Arc::clone(&restarted_runtime), &vault_path);
        let status = restarted.status().unwrap();
        assert!(status.vault_exists);
        assert!(status.locked);
        assert!(status.account.is_none());
        let unlocked = restarted.unlock(MAIN, secret(WALLET_PASSWORD)).unwrap();
        let account = unlocked.account.unwrap();
        assert!(account.label.is_none());
        assert_eq!(account.backup_verified, None);
    }

    #[test]
    fn tokens_are_single_use_and_existing_vaults_are_never_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let backup_path = directory.path().join("single.vision-recovery.json");
        let vault_path = directory.path().join("wallet").join(WALLET_VAULT_FILE);
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapter = WalletLifecycleAdapters::for_test(Arc::clone(&runtime), &vault_path);
        let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &backup_path);
        adapter
            .create_at(
                MAIN,
                "single",
                "Single Wallet",
                token.as_str(),
                secret(WALLET_PASSWORD),
                secret(RECOVERY_PASSWORD),
                1,
            )
            .unwrap();
        let original = fs::read(&vault_path).unwrap();
        assert_eq!(
            adapter
                .create_at(
                    MAIN,
                    "second",
                    "Second Wallet",
                    token.as_str(),
                    secret(WALLET_PASSWORD),
                    secret(RECOVERY_PASSWORD),
                    2,
                )
                .unwrap_err(),
            WalletLifecycleError::WalletAlreadyExists
        );
        assert_eq!(fs::read(vault_path).unwrap(), original);
    }

    #[test]
    fn invalidation_revokes_in_progress_work_and_explicit_lock_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapter = WalletLifecycleAdapters::for_test(
            Arc::clone(&runtime),
            &directory.path().join("wallet.json"),
        );
        let operation = runtime
            .begin_operation(MAIN, WalletOperationKind::Create)
            .unwrap();
        runtime.invalidate_all().unwrap();
        assert_eq!(
            operation.ensure_current(),
            Err(WalletRuntimeError::RuntimeUnavailable)
        );
        drop(operation);
        assert!(adapter.lock().unwrap().locked);
        assert!(adapter.lock().unwrap().locked);
    }

    #[test]
    fn lifecycle_errors_are_fixed_and_never_disclose_paths_or_retry_timing() {
        let errors = [
            WalletLifecycleError::RuntimeUnavailable,
            WalletLifecycleError::InvalidWindow,
            WalletLifecycleError::OperationInProgress,
            WalletLifecycleError::InvalidRequest,
            WalletLifecycleError::PathAuthorizationInvalid,
            WalletLifecycleError::PathAuthorizationExpired,
            WalletLifecycleError::WalletAlreadyExists,
            WalletLifecycleError::WalletUnavailable,
            WalletLifecycleError::InvalidLabel,
            WalletLifecycleError::PasswordPolicy,
            WalletLifecycleError::PasswordsMustDiffer,
            WalletLifecycleError::InvalidPasswordOrDamage,
            WalletLifecycleError::UnlockTemporarilyBlocked,
            WalletLifecycleError::SecureRandomUnavailable,
            WalletLifecycleError::RecoveryProtectionUnavailable,
            WalletLifecycleError::RecoveryDestinationExists,
            WalletLifecycleError::RecoveryStorageUnavailable,
            WalletLifecycleError::RecoveryBackupMismatch,
            WalletLifecycleError::VaultProtectionUnavailable,
            WalletLifecycleError::VaultStorageUnavailable,
            WalletLifecycleError::ClockUnavailable,
        ];
        for error in errors {
            assert!(error
                .code()
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_'));
            assert!(!error.to_string().contains('\\'));
            assert!(!error.to_string().contains(':'));
            assert!(!error.to_string().contains("retry"));
        }
    }
}
