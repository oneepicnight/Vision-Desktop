#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wallet lifecycle adapters remain private until the command boundary passes review"
    )
)]
#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "unregistered production lifecycle entry points are exercised after command review"
    )
)]

use super::{
    contract::{WalletLifecycleStatus, WalletLockResult},
    onboarding::{prepare_new_wallet, prepare_restored_wallet, WalletOnboardingError},
    public_request::{WalletCreateRequest, WalletRestoreRequest},
    recovery::RecoveryArtifactError,
    recovery_ceremony::{
        NativeCreateSecrets, NativeRecoveryCredentialCeremony, NativeRestoreSecrets,
        NativeSecretCeremonyError, NativeWalletSecretCeremony, RecoveryCeremonyError,
        RecoveryCredentialCeremony, WalletSecretCeremony,
    },
    runtime::{
        RecoveryPathPurpose, WalletOperationKind, WalletOperationPermit, WalletRuntimeError,
        WalletRuntimeState,
    },
    secret_input::SecretInput,
    session::WalletSessionError,
    vault::{load_vault, EncryptedWalletVault, WalletVaultError},
};
use std::{
    fmt,
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Component, Path, PathBuf, Prefix},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Storage::FileSystem::{GetDriveTypeW, FILE_ATTRIBUTE_REPARSE_POINT},
    System::WindowsProgramming::DRIVE_FIXED,
};
#[cfg(test)]
use zeroize::Zeroizing;

const WALLET_DIRECTORY: &str = "wallet";
const WALLET_VAULT_FILE: &str = "wallet.vault.json";
const WALLET_ACTIVITY_FILE: &str = "wallet.activity.json";

/// Lifecycle-issued authority for this installation's canonical custody files.
///
/// The fields and constructor stay private to the lifecycle boundary. Other wallet modules may
/// borrow this authority, but cannot substitute a vault, journal, or reconciliation directory.
/// It deliberately implements neither `Clone`, `Debug`, nor serialization.
pub(in crate::wallet) struct WalletCustodyPathAuthority {
    vault_path: PathBuf,
    journal_path: PathBuf,
}

impl WalletCustodyPathAuthority {
    fn issue(vault_path: PathBuf) -> Result<Self, WalletLifecycleError> {
        if !vault_path.is_absolute()
            || vault_path.file_name().and_then(|value| value.to_str()) != Some(WALLET_VAULT_FILE)
        {
            return Err(WalletLifecycleError::VaultStorageUnavailable);
        }
        let directory = vault_path
            .parent()
            .ok_or(WalletLifecycleError::VaultStorageUnavailable)?
            .to_path_buf();
        Ok(Self {
            vault_path,
            journal_path: directory.join(WALLET_ACTIVITY_FILE),
        })
    }

    pub(super) fn vault_path(&self) -> &Path {
        &self.vault_path
    }

    pub(super) fn journal_path(&self) -> &Path {
        &self.journal_path
    }

    #[cfg(test)]
    pub(in crate::wallet) fn issue_for_test(vault_path: &Path) -> Self {
        Self::issue(vault_path.to_path_buf()).expect("test custody path must be canonical")
    }
}

/// Private Rust-only orchestration for the first local wallet lifecycle.
///
/// No method is a Tauri command, and this type deliberately implements neither
/// Serde traits, `Clone`, nor `Debug`.
pub(crate) struct WalletLifecycleAdapters {
    runtime: Arc<WalletRuntimeState>,
    custody: WalletCustodyPathAuthority,
    recovery_ceremony: Arc<dyn RecoveryCredentialCeremony>,
    secret_ceremony: Arc<dyn WalletSecretCeremony>,
    #[cfg(test)]
    interruption_checkpoint: Option<WalletLifecycleCheckpoint>,
    #[cfg(test)]
    panic_checkpoint: Option<WalletLifecyclePanicCheckpoint>,
    #[cfg(test)]
    test_recovery_ceremony: Arc<TestRecoveryCredentialCeremony>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestRecoveryCeremonyOutcome {
    Verified,
    Cancelled,
    Unavailable,
    AuthorityRevoked,
}

#[cfg(test)]
struct TestRecoveryCredentialCeremony {
    outcome: TestRecoveryCeremonyOutcome,
    captured_credential: std::sync::Mutex<Option<Zeroizing<String>>>,
    runtime_to_revoke: Option<Arc<WalletRuntimeState>>,
}

#[cfg(test)]
impl TestRecoveryCredentialCeremony {
    fn verified() -> Self {
        Self {
            outcome: TestRecoveryCeremonyOutcome::Verified,
            captured_credential: std::sync::Mutex::new(None),
            runtime_to_revoke: None,
        }
    }

    fn with_outcome(
        outcome: TestRecoveryCeremonyOutcome,
        runtime_to_revoke: Arc<WalletRuntimeState>,
    ) -> Self {
        Self {
            outcome,
            captured_credential: std::sync::Mutex::new(None),
            runtime_to_revoke: Some(runtime_to_revoke),
        }
    }

    fn take_credential(&self) -> Option<Zeroizing<String>> {
        self.captured_credential.lock().ok()?.take()
    }
}

#[cfg(test)]
impl RecoveryCredentialCeremony for TestRecoveryCredentialCeremony {
    fn present_and_verify(
        &self,
        encoded_credential: &Zeroizing<String>,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<(), RecoveryCeremonyError> {
        if !authority_is_current() {
            return Err(RecoveryCeremonyError::AuthorityRevoked);
        }
        match self.outcome {
            TestRecoveryCeremonyOutcome::Verified => {
                let mut captured = Zeroizing::new(String::with_capacity(encoded_credential.len()));
                captured.push_str(encoded_credential.as_str());
                *self
                    .captured_credential
                    .lock()
                    .map_err(|_| RecoveryCeremonyError::NativeUiUnavailable)? = Some(captured);
                if authority_is_current() {
                    Ok(())
                } else {
                    Err(RecoveryCeremonyError::AuthorityRevoked)
                }
            }
            TestRecoveryCeremonyOutcome::Cancelled => Err(RecoveryCeremonyError::Cancelled),
            TestRecoveryCeremonyOutcome::Unavailable => {
                Err(RecoveryCeremonyError::NativeUiUnavailable)
            }
            TestRecoveryCeremonyOutcome::AuthorityRevoked => {
                if let Some(runtime) = &self.runtime_to_revoke {
                    runtime
                        .invalidate_all()
                        .map_err(|_| RecoveryCeremonyError::AuthorityRevoked)?;
                }
                Err(RecoveryCeremonyError::AuthorityRevoked)
            }
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalletLifecycleCheckpoint {
    CreateDestinationConsumed,
    CreatePrepared,
    CreateRecoveryAcknowledged,
    CreateRecoveryStored,
    CreateRecoveryVerified,
    CreateVaultStored,
    RestoreSourceConsumed,
    RestorePrepared,
    RestoreVaultStored,
}

#[cfg(test)]
struct TestWalletSecretCeremony;

#[cfg(test)]
const TEST_NATIVE_WALLET_PASSWORD: &str = "native-test-password-with-high-entropy";

#[cfg(test)]
impl WalletSecretCeremony for TestWalletSecretCeremony {
    fn capture_create(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<NativeCreateSecrets, NativeSecretCeremonyError> {
        if !authority_is_current() {
            return Err(NativeSecretCeremonyError::AuthorityRevoked);
        }
        Ok(NativeCreateSecrets {
            wallet_password: SecretInput::for_test(TEST_NATIVE_WALLET_PASSWORD),
        })
    }

    fn capture_restore(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<NativeRestoreSecrets, NativeSecretCeremonyError> {
        if !authority_is_current() {
            return Err(NativeSecretCeremonyError::AuthorityRevoked);
        }
        Ok(NativeRestoreSecrets {
            wallet_password: SecretInput::for_test(TEST_NATIVE_WALLET_PASSWORD),
            recovery_credential: SecretInput::for_test("vrc1-test-recovery-credential"),
        })
    }

    fn capture_unlock(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<SecretInput, NativeSecretCeremonyError> {
        if !authority_is_current() {
            return Err(NativeSecretCeremonyError::AuthorityRevoked);
        }
        Ok(SecretInput::for_test(TEST_NATIVE_WALLET_PASSWORD))
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalletLifecyclePanicCheckpoint {
    BeforeRequest,
    AfterPublicValidation,
    BeforeCapabilityConsumption,
    AfterCapabilityConsumption,
    BeforeNativeSecretCeremony,
    AfterNativeSecretCeremony,
    BeforeCryptographicPreparation,
    AfterCryptographicPreparation,
    BeforeRecoveryAcknowledgement,
    AfterRecoveryAcknowledgement,
    BeforeRecoveryPublication,
    AfterRecoveryPublication,
    AfterRecoveryVerification,
    BeforeVaultPublication,
    AfterVaultPublication,
    AfterUnlockSessionInstalled,
    BeforeSuccessCommit,
}

struct LifecycleFailClosedGuard<'a> {
    runtime: &'a WalletRuntimeState,
    armed: bool,
}

impl<'a> LifecycleFailClosedGuard<'a> {
    fn arm(runtime: &'a WalletRuntimeState) -> Self {
        Self {
            runtime,
            armed: true,
        }
    }

    fn commit(&mut self) {
        self.armed = false;
    }

    fn invalidate_or_terminate(&mut self) {
        let invalidated = catch_unwind(AssertUnwindSafe(|| self.runtime.invalidate_all()));
        match invalidated {
            Ok(Ok(())) => self.armed = false,
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }
}

impl Drop for LifecycleFailClosedGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.invalidate_or_terminate();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalletLifecycleError {
    RuntimeUnavailable,
    ActivationUnavailable,
    InvalidWindow,
    OperationInProgress,
    InvalidRequest,
    PathAuthorizationInvalid,
    PathAuthorizationExpired,
    WalletAlreadyExists,
    WalletUnavailable,
    InvalidLabel,
    PasswordPolicy,
    InvalidPasswordOrDamage,
    UnlockTemporarilyBlocked,
    SecureRandomUnavailable,
    RecoveryProtectionUnavailable,
    RecoveryAcknowledgementCancelled,
    RecoveryAcknowledgementUnavailable,
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
            Self::ActivationUnavailable => "wallet_activation_unavailable",
            Self::InvalidWindow => "invalid_window",
            Self::OperationInProgress => "operation_in_progress",
            Self::InvalidRequest => "invalid_request",
            Self::PathAuthorizationInvalid => "path_authorization_invalid",
            Self::PathAuthorizationExpired => "path_authorization_expired",
            Self::WalletAlreadyExists => "wallet_already_exists",
            Self::WalletUnavailable => "wallet_unavailable",
            Self::InvalidLabel => "invalid_label",
            Self::PasswordPolicy => "password_policy",
            Self::InvalidPasswordOrDamage => "invalid_password_or_damage",
            Self::UnlockTemporarilyBlocked => "unlock_temporarily_blocked",
            Self::SecureRandomUnavailable => "secure_random_unavailable",
            Self::RecoveryProtectionUnavailable => "recovery_protection_unavailable",
            Self::RecoveryAcknowledgementCancelled => "recovery_acknowledgement_cancelled",
            Self::RecoveryAcknowledgementUnavailable => "recovery_acknowledgement_unavailable",
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
            Self::ActivationUnavailable => "secure wallet activation is unavailable",
            Self::InvalidWindow => "wallet access is unavailable from this window",
            Self::OperationInProgress => "another wallet operation is already in progress",
            Self::InvalidRequest => "wallet request is invalid",
            Self::PathAuthorizationInvalid => "recovery selection is invalid",
            Self::PathAuthorizationExpired => "recovery selection has expired",
            Self::WalletAlreadyExists => "a local wallet already exists",
            Self::WalletUnavailable => "the local wallet is unavailable",
            Self::InvalidLabel => "wallet label is invalid",
            Self::PasswordPolicy => "wallet password does not meet the security policy",
            Self::InvalidPasswordOrDamage => {
                "the password is incorrect or encrypted wallet data is damaged"
            }
            Self::UnlockTemporarilyBlocked => {
                "wallet unlock is temporarily unavailable after repeated failures"
            }
            Self::SecureRandomUnavailable => "secure operating-system randomness is unavailable",
            Self::RecoveryProtectionUnavailable => "portable recovery protection is unavailable",
            Self::RecoveryAcknowledgementCancelled => {
                "wallet creation was cancelled before recovery acknowledgement"
            }
            Self::RecoveryAcknowledgementUnavailable => {
                "secure recovery acknowledgement is unavailable"
            }
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
        local_app_data: &Path,
        recovery_ceremony: Arc<NativeRecoveryCredentialCeremony>,
        secret_ceremony: Arc<NativeWalletSecretCeremony>,
    ) -> Result<Self, WalletLifecycleError> {
        validate_local_custody_root(local_app_data)?;
        let vault_path = local_app_data
            .join("Vision")
            .join("Desktop")
            .join(WALLET_DIRECTORY)
            .join(WALLET_VAULT_FILE);
        validate_local_custody_root(
            vault_path
                .parent()
                .ok_or(WalletLifecycleError::VaultStorageUnavailable)?,
        )?;
        let custody = WalletCustodyPathAuthority::issue(vault_path)?;
        Ok(Self {
            runtime,
            custody,
            recovery_ceremony,
            secret_ceremony,
            #[cfg(test)]
            interruption_checkpoint: None,
            #[cfg(test)]
            panic_checkpoint: None,
            #[cfg(test)]
            test_recovery_ceremony: Arc::new(TestRecoveryCredentialCeremony::verified()),
        })
    }

    pub(in crate::wallet) fn status(&self) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.run_fail_closed(|| {
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRequest);
            let status = self.status_inner()?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeSuccessCommit);
            Ok(status)
        })
    }

    pub(in crate::wallet) fn custody_path_authority(&self) -> &WalletCustodyPathAuthority {
        &self.custody
    }

    fn status_inner(&self) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        match self.custody.vault_path().try_exists() {
            Ok(false) => self
                .runtime
                .lifecycle_status(false)
                .map_err(map_runtime_error),
            Ok(true) => {
                let vault = load_vault(self.custody.vault_path()).map_err(map_vault_load_error)?;
                self.runtime
                    .lifecycle_status_for_vault(&vault)
                    .map_err(map_runtime_error)
            }
            Err(_) => Err(WalletLifecycleError::WalletUnavailable),
        }
    }

    /// Unregistered production path. The one-time destination capability is consumed before the
    /// native password ceremony opens, so cancellation or panic cannot replay it.
    pub(in crate::wallet) fn create_native(
        &self,
        owner_window: &str,
        request: WalletCreateRequest,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        // Deserialization and bounded validation have completed before this method can be called.
        // Move the validated public values out before any runtime or filesystem interaction.
        let (wallet_id, label, recovery_destination_token) = request.into_parts();
        self.run_fail_closed(|| {
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRequest);
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::AfterPublicValidation);
            self.require_vault_absent()?;
            let operation = self
                .runtime
                .begin_operation(owner_window, WalletOperationKind::Create)
                .map_err(map_runtime_error)?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeCapabilityConsumption);
            let recovery_path = self
                .runtime
                .consume_recovery_path(
                    owner_window,
                    RecoveryPathPurpose::Destination,
                    recovery_destination_token.as_str(),
                )
                .map_err(map_runtime_error)?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::AfterCapabilityConsumption);
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeNativeSecretCeremony);
            let NativeCreateSecrets { wallet_password } = self
                .secret_ceremony
                .capture_create(&|| operation.ensure_current().is_ok())
                .map_err(map_native_secret_ceremony_error)?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::AfterNativeSecretCeremony);
            self.create_authorized(
                operation,
                recovery_path,
                wallet_id.as_str(),
                label.as_str(),
                wallet_password,
                now_unix_ms()?,
            )
        })
    }

    #[cfg(test)]
    pub(in crate::wallet) fn create(
        &self,
        owner_window: &str,
        wallet_id: &str,
        label: &str,
        recovery_destination_token: &str,
        wallet_secret: SecretInput,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.run_fail_closed(|| {
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRequest);
            let status = self.create_at(
                owner_window,
                wallet_id,
                label,
                recovery_destination_token,
                wallet_secret,
                now_unix_ms()?,
            )?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeSuccessCommit);
            Ok(status)
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    fn create_at(
        &self,
        owner_window: &str,
        wallet_id: &str,
        label: &str,
        recovery_destination_token: &str,
        wallet_secret: SecretInput,
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
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::CreateDestinationConsumed)?;
        self.create_authorized(
            operation,
            recovery_path,
            wallet_id,
            label,
            wallet_secret,
            created_at_unix_ms,
        )
    }

    fn create_authorized(
        &self,
        operation: WalletOperationPermit<'_>,
        recovery_path: PathBuf,
        wallet_id: &str,
        label: &str,
        wallet_secret: SecretInput,
        created_at_unix_ms: u64,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        let wallet_password = wallet_secret.into_wallet_password();
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeCryptographicPreparation);
        let mut prepared = operation
            .run_authorized(|activation| {
                prepare_new_wallet(
                    activation,
                    wallet_id,
                    label,
                    created_at_unix_ms,
                    &wallet_password,
                )
                .map_err(map_onboarding_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterCryptographicPreparation);
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::CreatePrepared)?;
        let recovery_credential = operation
            .run_authorized(|_| {
                prepared
                    .recovery_credential_for_native_presentation()
                    .map_err(map_onboarding_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRecoveryAcknowledgement);
        operation
            .run_authorized(|_| {
                self.recovery_ceremony
                    .present_and_verify(&recovery_credential, &|| {
                        operation.ensure_current().is_ok()
                    })
                    .map_err(map_recovery_ceremony_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterRecoveryAcknowledgement);
        drop(recovery_credential);
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::CreateRecoveryAcknowledged)?;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRecoveryPublication);
        operation
            .run_authorized(|_| {
                prepared
                    .store_recovery_backup(&recovery_path)
                    .map_err(map_onboarding_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterRecoveryPublication);
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::CreateRecoveryStored)?;
        let mut verified = operation
            .run_authorized(|activation| {
                prepared
                    .verify_stored_recovery(activation, &recovery_path)
                    .map_err(map_onboarding_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterRecoveryVerification);
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::CreateRecoveryVerified)?;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeVaultPublication);
        let metadata = operation
            .run_authorized(|_| {
                verified
                    .store_local_vault(self.custody.vault_path())
                    .map_err(map_onboarding_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterVaultPublication);
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::CreateVaultStored)?;
        let status = operation
            .run_authorized(|_| {
                self.runtime
                    .remember_public_metadata(metadata)
                    .map_err(map_runtime_error)
            })
            .map_err(map_runtime_error)??;
        let completed = operation.complete(status).map_err(map_runtime_error)?;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeSuccessCommit);
        Ok(completed)
    }

    /// Unregistered production path. The source capability is consumed before any recovery or
    /// password input is accepted by the native ceremony.
    pub(in crate::wallet) fn restore_native(
        &self,
        owner_window: &str,
        request: WalletRestoreRequest,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        // The request type is the production validation boundary. No raw public string path is
        // available outside tests.
        let (wallet_id, label, recovery_source_token) = request.into_parts();
        self.run_fail_closed(|| {
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRequest);
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::AfterPublicValidation);
            self.require_vault_absent()?;
            let operation = self
                .runtime
                .begin_operation(owner_window, WalletOperationKind::Restore)
                .map_err(map_runtime_error)?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeCapabilityConsumption);
            let recovery_path = self
                .runtime
                .consume_recovery_path(
                    owner_window,
                    RecoveryPathPurpose::Source,
                    recovery_source_token.as_str(),
                )
                .map_err(map_runtime_error)?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::AfterCapabilityConsumption);
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeNativeSecretCeremony);
            let NativeRestoreSecrets {
                wallet_password,
                recovery_credential,
            } = self
                .secret_ceremony
                .capture_restore(&|| operation.ensure_current().is_ok())
                .map_err(map_native_secret_ceremony_error)?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::AfterNativeSecretCeremony);
            self.restore_authorized(
                operation,
                recovery_path,
                wallet_id.as_str(),
                label.as_str(),
                wallet_password,
                recovery_credential,
                now_unix_ms()?,
            )
        })
    }

    #[cfg(test)]
    pub(in crate::wallet) fn restore(
        &self,
        owner_window: &str,
        wallet_id: &str,
        label: &str,
        recovery_source_token: &str,
        new_wallet_secret: SecretInput,
        recovery_secret: SecretInput,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.run_fail_closed(|| {
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRequest);
            let status = self.restore_at(
                owner_window,
                wallet_id,
                label,
                recovery_source_token,
                new_wallet_secret,
                recovery_secret,
                now_unix_ms()?,
            )?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeSuccessCommit);
            Ok(status)
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
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
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::RestoreSourceConsumed)?;
        self.restore_authorized(
            operation,
            recovery_path,
            wallet_id,
            label,
            new_wallet_secret,
            recovery_secret,
            created_at_unix_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn restore_authorized(
        &self,
        operation: WalletOperationPermit<'_>,
        recovery_path: PathBuf,
        wallet_id: &str,
        label: &str,
        new_wallet_secret: SecretInput,
        recovery_secret: SecretInput,
        created_at_unix_ms: u64,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        let wallet_password = new_wallet_secret.into_wallet_password();
        let recovery_credential = recovery_secret
            .into_recovery_credential()
            .map_err(map_recovery_credential_error)?;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeCryptographicPreparation);
        let mut restored = operation
            .run_authorized(|activation| {
                prepare_restored_wallet(
                    activation,
                    &recovery_path,
                    wallet_id,
                    label,
                    created_at_unix_ms,
                    &wallet_password,
                    &recovery_credential,
                )
                .map_err(map_onboarding_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterCryptographicPreparation);
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::RestorePrepared)?;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeVaultPublication);
        let metadata = operation
            .run_authorized(|_| {
                restored
                    .store_local_vault(self.custody.vault_path())
                    .map_err(map_onboarding_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterVaultPublication);
        #[cfg(test)]
        self.interrupt_at(WalletLifecycleCheckpoint::RestoreVaultStored)?;
        let status = operation
            .run_authorized(|_| {
                self.runtime
                    .remember_public_metadata(metadata)
                    .map_err(map_runtime_error)
            })
            .map_err(map_runtime_error)??;
        let completed = operation.complete(status).map_err(map_runtime_error)?;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeSuccessCommit);
        Ok(completed)
    }

    pub(in crate::wallet) fn unlock_native(
        &self,
        owner_window: &str,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.run_fail_closed(|| {
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRequest);
            let operation = self
                .runtime
                .begin_operation(owner_window, WalletOperationKind::Unlock)
                .map_err(map_runtime_error)?;
            let vault = operation
                .run_authorized(|_| {
                    load_vault(self.custody.vault_path()).map_err(map_vault_load_error)
                })
                .map_err(map_runtime_error)??;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeNativeSecretCeremony);
            let wallet_secret = self
                .secret_ceremony
                .capture_unlock(&|| operation.ensure_current().is_ok())
                .map_err(map_native_secret_ceremony_error)?;
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::AfterNativeSecretCeremony);
            self.unlock_authorized(operation, vault, wallet_secret)
        })
    }

    #[cfg(test)]
    pub(in crate::wallet) fn unlock(
        &self,
        owner_window: &str,
        wallet_secret: SecretInput,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.run_fail_closed(|| {
            #[cfg(test)]
            self.panic_at(WalletLifecyclePanicCheckpoint::BeforeRequest);
            let operation = self
                .runtime
                .begin_operation(owner_window, WalletOperationKind::Unlock)
                .map_err(map_runtime_error)?;
            let vault = operation
                .run_authorized(|_| {
                    load_vault(self.custody.vault_path()).map_err(map_vault_load_error)
                })
                .map_err(map_runtime_error)??;
            let wallet_password = wallet_secret.into_wallet_password();
            self.unlock_authorized_with_password(operation, vault, wallet_password)
        })
    }

    fn unlock_authorized(
        &self,
        operation: WalletOperationPermit<'_>,
        vault: EncryptedWalletVault,
        wallet_secret: SecretInput,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        self.unlock_authorized_with_password(operation, vault, wallet_secret.into_wallet_password())
    }

    fn unlock_authorized_with_password(
        &self,
        operation: WalletOperationPermit<'_>,
        vault: EncryptedWalletVault,
        wallet_password: super::secrets::WalletPassword,
    ) -> Result<WalletLifecycleStatus, WalletLifecycleError> {
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeCryptographicPreparation);
        let status = operation
            .run_authorized(|activation| {
                self.runtime
                    .unlock_vault(activation, &vault, &wallet_password)
                    .map_err(map_session_error)
            })
            .map_err(map_runtime_error)??;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterCryptographicPreparation);
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::AfterUnlockSessionInstalled);
        let completed = operation.complete(status).map_err(map_runtime_error)?;
        #[cfg(test)]
        self.panic_at(WalletLifecyclePanicCheckpoint::BeforeSuccessCommit);
        Ok(completed)
    }

    pub(in crate::wallet) fn lock(&self) -> Result<WalletLockResult, WalletLifecycleError> {
        match catch_unwind(AssertUnwindSafe(|| self.runtime.invalidate_all())) {
            Ok(Ok(())) => Ok(WalletLockResult { locked: true }),
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }

    fn run_fail_closed<T>(
        &self,
        operation: impl FnOnce() -> Result<T, WalletLifecycleError>,
    ) -> Result<T, WalletLifecycleError> {
        let mut guard = LifecycleFailClosedGuard::arm(&self.runtime);
        match catch_unwind(AssertUnwindSafe(operation)) {
            Ok(Ok(value)) => {
                guard.commit();
                Ok(value)
            }
            Ok(Err(error)) => {
                guard.invalidate_or_terminate();
                Err(error)
            }
            Err(_) => {
                guard.invalidate_or_terminate();
                Err(WalletLifecycleError::RuntimeUnavailable)
            }
        }
    }

    fn require_vault_absent(&self) -> Result<(), WalletLifecycleError> {
        match self.custody.vault_path().try_exists() {
            Ok(false) => Ok(()),
            Ok(true) => Err(WalletLifecycleError::WalletAlreadyExists),
            Err(_) => Err(WalletLifecycleError::VaultStorageUnavailable),
        }
    }

    #[cfg(test)]
    fn for_test(runtime: Arc<WalletRuntimeState>, vault_path: &std::path::Path) -> Self {
        let test_recovery_ceremony = Arc::new(TestRecoveryCredentialCeremony::verified());
        Self {
            runtime,
            custody: WalletCustodyPathAuthority::issue_for_test(vault_path),
            recovery_ceremony: Arc::clone(&test_recovery_ceremony)
                as Arc<dyn RecoveryCredentialCeremony>,
            secret_ceremony: Arc::new(TestWalletSecretCeremony),
            interruption_checkpoint: None,
            panic_checkpoint: None,
            test_recovery_ceremony,
        }
    }

    #[cfg(test)]
    fn for_test_with_interruption(
        runtime: Arc<WalletRuntimeState>,
        vault_path: &std::path::Path,
        checkpoint: WalletLifecycleCheckpoint,
    ) -> Self {
        let test_recovery_ceremony = Arc::new(TestRecoveryCredentialCeremony::verified());
        Self {
            runtime,
            custody: WalletCustodyPathAuthority::issue_for_test(vault_path),
            recovery_ceremony: Arc::clone(&test_recovery_ceremony)
                as Arc<dyn RecoveryCredentialCeremony>,
            secret_ceremony: Arc::new(TestWalletSecretCeremony),
            interruption_checkpoint: Some(checkpoint),
            panic_checkpoint: None,
            test_recovery_ceremony,
        }
    }

    #[cfg(test)]
    fn for_test_with_ceremony(
        runtime: Arc<WalletRuntimeState>,
        vault_path: &std::path::Path,
        outcome: TestRecoveryCeremonyOutcome,
    ) -> Self {
        let test_recovery_ceremony = Arc::new(TestRecoveryCredentialCeremony::with_outcome(
            outcome,
            Arc::clone(&runtime),
        ));
        Self {
            runtime,
            custody: WalletCustodyPathAuthority::issue_for_test(vault_path),
            recovery_ceremony: Arc::clone(&test_recovery_ceremony)
                as Arc<dyn RecoveryCredentialCeremony>,
            secret_ceremony: Arc::new(TestWalletSecretCeremony),
            interruption_checkpoint: None,
            panic_checkpoint: None,
            test_recovery_ceremony,
        }
    }

    #[cfg(test)]
    fn take_test_recovery_credential(&self) -> Option<Zeroizing<String>> {
        self.test_recovery_ceremony.take_credential()
    }

    #[cfg(test)]
    fn for_test_with_panic(
        runtime: Arc<WalletRuntimeState>,
        vault_path: &std::path::Path,
        checkpoint: WalletLifecyclePanicCheckpoint,
    ) -> Self {
        let mut adapters = Self::for_test(runtime, vault_path);
        adapters.panic_checkpoint = Some(checkpoint);
        adapters
    }

    #[cfg(test)]
    fn panic_at(&self, checkpoint: WalletLifecyclePanicCheckpoint) {
        if self.panic_checkpoint == Some(checkpoint) {
            panic!("injected wallet lifecycle panic");
        }
    }

    #[cfg(test)]
    fn interrupt_at(
        &self,
        checkpoint: WalletLifecycleCheckpoint,
    ) -> Result<(), WalletLifecycleError> {
        if self.interruption_checkpoint == Some(checkpoint) {
            self.runtime.invalidate_all().map_err(map_runtime_error)?;
        }
        Ok(())
    }
}

fn validate_local_custody_root(path: &Path) -> Result<(), WalletLifecycleError> {
    const MAX_WINDOWS_PATH_UNITS: usize = 32_767;

    if path.as_os_str().is_empty()
        || path.as_os_str().encode_wide().count() > MAX_WINDOWS_PATH_UNITS
        || path.as_os_str().encode_wide().any(|unit| unit == 0)
    {
        return Err(WalletLifecycleError::VaultStorageUnavailable);
    }
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) => drive,
            _ => return Err(WalletLifecycleError::VaultStorageUnavailable),
        },
        _ => return Err(WalletLifecycleError::VaultStorageUnavailable),
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(WalletLifecycleError::VaultStorageUnavailable);
    }
    if components.any(|component| !matches!(component, Component::Normal(_))) {
        return Err(WalletLifecycleError::VaultStorageUnavailable);
    }

    let drive_root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: `drive_root` is a valid null-terminated UTF-16 drive-root path.
    if unsafe { GetDriveTypeW(drive_root.as_ptr()) } != DRIVE_FIXED {
        return Err(WalletLifecycleError::VaultStorageUnavailable);
    }

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if !metadata.is_dir()
                    || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                {
                    return Err(WalletLifecycleError::VaultStorageUnavailable);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err(WalletLifecycleError::VaultStorageUnavailable),
        }
    }
    Ok(())
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
        WalletRuntimeError::ActivationUnavailable => WalletLifecycleError::ActivationUnavailable,
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
        WalletRuntimeError::UnsupportedWindowsHost
        | WalletRuntimeError::ProcessLockUnavailable
        | WalletRuntimeError::RuntimeUnavailable
        | WalletRuntimeError::RecoverySelectionCancelled
        | WalletRuntimeError::RecoveryDestinationInvalid
        | WalletRuntimeError::RecoveryDestinationExists
        | WalletRuntimeError::RecoverySourceInvalid
        | WalletRuntimeError::ReconciliationUnavailable => WalletLifecycleError::RuntimeUnavailable,
    }
}

fn map_onboarding_error(error: WalletOnboardingError) -> WalletLifecycleError {
    match error {
        WalletOnboardingError::InvalidLabel => WalletLifecycleError::InvalidLabel,
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
        WalletOnboardingError::RecoveryCredentialOrDamage => {
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

fn map_recovery_ceremony_error(error: RecoveryCeremonyError) -> WalletLifecycleError {
    match error {
        RecoveryCeremonyError::Cancelled => WalletLifecycleError::RecoveryAcknowledgementCancelled,
        RecoveryCeremonyError::AuthorityRevoked => WalletLifecycleError::RuntimeUnavailable,
        RecoveryCeremonyError::NativeUiUnavailable => {
            WalletLifecycleError::RecoveryAcknowledgementUnavailable
        }
    }
}

fn map_native_secret_ceremony_error(error: NativeSecretCeremonyError) -> WalletLifecycleError {
    match error {
        NativeSecretCeremonyError::Cancelled => {
            WalletLifecycleError::RecoveryAcknowledgementCancelled
        }
        NativeSecretCeremonyError::AuthorityRevoked => WalletLifecycleError::RuntimeUnavailable,
        NativeSecretCeremonyError::InvalidInput => WalletLifecycleError::PasswordPolicy,
        NativeSecretCeremonyError::NativeUiUnavailable => WalletLifecycleError::RuntimeUnavailable,
    }
}

fn map_recovery_credential_error(_error: RecoveryArtifactError) -> WalletLifecycleError {
    WalletLifecycleError::InvalidPasswordOrDamage
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
    use crate::wallet::{
        activation::lifecycle_activation_requirements_for_test,
        runtime::{RecoveryPathToken, RecoverySelectionPermit},
    };
    use std::fs;

    const MAIN: &str = "main";
    const WALLET_PASSWORD: &str = "local wallet password";
    const RESTORED_PASSWORD: &str = "replacement wallet password";
    const INVALID_LEGACY_RECOVERY_PASSWORD: &str = "different recovery password";

    fn secret(value: &str) -> SecretInput {
        SecretInput::for_test(value)
    }

    fn selection_token(
        runtime: &Arc<WalletRuntimeState>,
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

    fn native_create_request(token: &RecoveryPathToken) -> WalletCreateRequest {
        serde_json::from_str(&format!(
            r#"{{"wallet_id":"native-panic-test","label":"Native Panic Test","recovery_destination_handle":"{}"}}"#,
            token.as_str(),
        ))
        .unwrap()
    }

    fn native_restore_request(token: &RecoveryPathToken) -> WalletRestoreRequest {
        serde_json::from_str(&format!(
            r#"{{"wallet_id":"native-restore-test","label":"Native Restore Test","recovery_source_handle":"{}"}}"#,
            token.as_str(),
        ))
        .unwrap()
    }

    #[test]
    fn lifecycle_adapters_refuse_every_individually_unmet_activation_gate() {
        let directory = tempfile::tempdir().unwrap();
        for requirement in lifecycle_activation_requirements_for_test() {
            let runtime = Arc::new(WalletRuntimeState::for_test_missing_activation(requirement));
            let vault_path = directory
                .path()
                .join(format!("blocked-{requirement:?}"))
                .join(WALLET_VAULT_FILE);
            let adapters = WalletLifecycleAdapters::for_test(runtime, &vault_path);

            assert_eq!(
                adapters
                    .create(
                        MAIN,
                        "blocked",
                        "Blocked",
                        "unused",
                        secret(WALLET_PASSWORD),
                    )
                    .unwrap_err(),
                WalletLifecycleError::ActivationUnavailable,
            );
            assert_eq!(
                adapters
                    .restore(
                        MAIN,
                        "blocked",
                        "Blocked",
                        "unused",
                        secret(RESTORED_PASSWORD),
                        secret(INVALID_LEGACY_RECOVERY_PASSWORD),
                    )
                    .unwrap_err(),
                WalletLifecycleError::ActivationUnavailable,
            );
            assert_eq!(
                adapters.unlock(MAIN, secret(WALLET_PASSWORD)).unwrap_err(),
                WalletLifecycleError::ActivationUnavailable,
            );
            assert_eq!(adapters.lock().unwrap(), WalletLockResult { locked: true },);
        }
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
            )
            .unwrap();
        assert!(created.vault_exists);
        assert!(created.locked);
        let recovery_credential = first.take_test_recovery_credential().unwrap();
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
                secret(recovery_credential.as_str()),
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
            recovery_credential.as_str(),
            "mnemonic",
        ] {
            assert!(!backup_text.contains(forbidden));
        }
    }

    #[test]
    fn recovery_acknowledgement_failure_never_publishes_backup_or_vault() {
        let root = tempfile::tempdir().unwrap();
        let cases = [
            (
                TestRecoveryCeremonyOutcome::Cancelled,
                WalletLifecycleError::RecoveryAcknowledgementCancelled,
            ),
            (
                TestRecoveryCeremonyOutcome::Unavailable,
                WalletLifecycleError::RecoveryAcknowledgementUnavailable,
            ),
            (
                TestRecoveryCeremonyOutcome::AuthorityRevoked,
                WalletLifecycleError::RuntimeUnavailable,
            ),
        ];

        for (index, (outcome, expected_error)) in cases.into_iter().enumerate() {
            let directory = root.path().join(format!("ceremony-{index}"));
            fs::create_dir(&directory).unwrap();
            let backup_path = directory.join("wallet.vision-recovery.json");
            let vault_path = directory.join("wallet").join(WALLET_VAULT_FILE);
            let runtime = Arc::new(WalletRuntimeState::for_test());
            let adapter = WalletLifecycleAdapters::for_test_with_ceremony(
                Arc::clone(&runtime),
                &vault_path,
                outcome,
            );
            let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &backup_path);

            assert_eq!(
                adapter
                    .create_at(
                        MAIN,
                        &format!("ceremony-{index}"),
                        "Ceremony Wallet",
                        token.as_str(),
                        secret(WALLET_PASSWORD),
                        1,
                    )
                    .unwrap_err(),
                expected_error
            );
            assert!(!backup_path.exists());
            assert!(!vault_path.exists());
            assert!(adapter.take_test_recovery_credential().is_none());
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
            &directory.path().join(WALLET_VAULT_FILE),
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
    fn explicit_lock_succeeds_even_when_vault_status_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let backup_path = directory.path().join("lock.vision-recovery.json");
        let vault_path = directory.path().join("wallet").join(WALLET_VAULT_FILE);
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapter = WalletLifecycleAdapters::for_test(Arc::clone(&runtime), &vault_path);
        let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &backup_path);
        adapter
            .create_at(
                MAIN,
                "lock-independent",
                "Lock Independent",
                token.as_str(),
                secret(WALLET_PASSWORD),
                1,
            )
            .unwrap();
        assert!(
            !adapter
                .unlock(MAIN, secret(WALLET_PASSWORD))
                .unwrap()
                .locked
        );

        fs::write(&vault_path, b"damaged encrypted vault").unwrap();
        assert_eq!(
            adapter.status().unwrap_err(),
            WalletLifecycleError::WalletUnavailable
        );
        assert_eq!(adapter.lock().unwrap(), WalletLockResult { locked: true });
        assert!(runtime.lifecycle_status(true).unwrap().locked);
        assert_eq!(adapter.lock().unwrap(), WalletLockResult { locked: true });
    }

    #[test]
    fn panic_after_unlock_installs_session_is_caught_and_fully_invalidated() {
        let directory = tempfile::tempdir().unwrap();
        let recovery_path = directory.path().join("panic.vision-recovery.json");
        let vault_path = directory.path().join("wallet").join(WALLET_VAULT_FILE);
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let creator = WalletLifecycleAdapters::for_test(Arc::clone(&runtime), &vault_path);
        let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &recovery_path);
        creator
            .create_at(
                MAIN,
                "panic-guard",
                "Panic Guard",
                token.as_str(),
                secret(WALLET_PASSWORD),
                1,
            )
            .unwrap();

        let guarded = WalletLifecycleAdapters::for_test_with_panic(
            Arc::clone(&runtime),
            &vault_path,
            WalletLifecyclePanicCheckpoint::AfterUnlockSessionInstalled,
        );
        assert_eq!(
            guarded.unlock(MAIN, secret(WALLET_PASSWORD)).unwrap_err(),
            WalletLifecycleError::RuntimeUnavailable
        );
        let vault = load_vault(&vault_path).unwrap();
        let status = runtime.lifecycle_status_for_vault(&vault).unwrap();
        assert!(status.locked);
        assert!(status.account.is_some());
        assert!(runtime
            .begin_operation(MAIN, WalletOperationKind::Unlock)
            .is_ok());
    }

    #[test]
    fn panic_before_request_returns_only_the_fixed_runtime_error() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let guarded = WalletLifecycleAdapters::for_test_with_panic(
            Arc::clone(&runtime),
            &directory.path().join(WALLET_VAULT_FILE),
            WalletLifecyclePanicCheckpoint::BeforeRequest,
        );

        let error = guarded.status().unwrap_err();
        assert_eq!(error, WalletLifecycleError::RuntimeUnavailable);
        assert_eq!(error.code(), "wallet_runtime_unavailable");
        assert!(runtime.lifecycle_status(false).unwrap().locked);
    }

    #[test]
    fn native_create_panics_at_every_sensitive_stage_and_recovers_locked() {
        let checkpoints = [
            WalletLifecyclePanicCheckpoint::BeforeRequest,
            WalletLifecyclePanicCheckpoint::AfterPublicValidation,
            WalletLifecyclePanicCheckpoint::BeforeCapabilityConsumption,
            WalletLifecyclePanicCheckpoint::AfterCapabilityConsumption,
            WalletLifecyclePanicCheckpoint::BeforeNativeSecretCeremony,
            WalletLifecyclePanicCheckpoint::AfterNativeSecretCeremony,
            WalletLifecyclePanicCheckpoint::BeforeCryptographicPreparation,
            WalletLifecyclePanicCheckpoint::AfterCryptographicPreparation,
            WalletLifecyclePanicCheckpoint::BeforeRecoveryAcknowledgement,
            WalletLifecyclePanicCheckpoint::AfterRecoveryAcknowledgement,
            WalletLifecyclePanicCheckpoint::BeforeRecoveryPublication,
            WalletLifecyclePanicCheckpoint::AfterRecoveryPublication,
            WalletLifecyclePanicCheckpoint::AfterRecoveryVerification,
            WalletLifecyclePanicCheckpoint::BeforeVaultPublication,
            WalletLifecyclePanicCheckpoint::AfterVaultPublication,
            WalletLifecyclePanicCheckpoint::BeforeSuccessCommit,
        ];

        for (index, checkpoint) in checkpoints.into_iter().enumerate() {
            let directory = tempfile::tempdir().unwrap();
            let vault_path = directory.path().join("wallet").join(WALLET_VAULT_FILE);
            let recovery_path = directory
                .path()
                .join(format!("panic-{index}.vision-recovery.json"));
            let runtime = Arc::new(WalletRuntimeState::for_test());
            let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &recovery_path);
            let adapters = WalletLifecycleAdapters::for_test_with_panic(
                Arc::clone(&runtime),
                &vault_path,
                checkpoint,
            );

            assert_eq!(
                adapters
                    .create_native(MAIN, native_create_request(&token))
                    .unwrap_err(),
                WalletLifecycleError::RuntimeUnavailable,
                "checkpoint {checkpoint:?}",
            );
            let status = runtime.lifecycle_status(vault_path.exists()).unwrap();
            assert!(status.locked, "checkpoint {checkpoint:?}");
            assert!(runtime
                .begin_operation(MAIN, WalletOperationKind::Unlock)
                .is_ok());
        }
    }

    #[test]
    fn every_native_secret_ceremony_is_inside_the_panic_boundary() {
        for checkpoint in [
            WalletLifecyclePanicCheckpoint::BeforeNativeSecretCeremony,
            WalletLifecyclePanicCheckpoint::AfterNativeSecretCeremony,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source.vision-recovery.json");
            fs::write(&source, b"bounded-encrypted-placeholder").unwrap();
            let runtime = Arc::new(WalletRuntimeState::for_test());
            let token = selection_token(&runtime, RecoveryPathPurpose::Source, &source);
            let adapters = WalletLifecycleAdapters::for_test_with_panic(
                Arc::clone(&runtime),
                &directory
                    .path()
                    .join("restore-wallet")
                    .join(WALLET_VAULT_FILE),
                checkpoint,
            );
            assert_eq!(
                adapters
                    .restore_native(MAIN, native_restore_request(&token))
                    .unwrap_err(),
                WalletLifecycleError::RuntimeUnavailable,
            );
            assert!(runtime.lifecycle_status(false).unwrap().locked);
        }

        for checkpoint in [
            WalletLifecyclePanicCheckpoint::BeforeNativeSecretCeremony,
            WalletLifecyclePanicCheckpoint::AfterNativeSecretCeremony,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let recovery_path = directory.path().join("unlock.vision-recovery.json");
            let vault_path = directory.path().join("wallet").join(WALLET_VAULT_FILE);
            let runtime = Arc::new(WalletRuntimeState::for_test());
            let creator = WalletLifecycleAdapters::for_test(Arc::clone(&runtime), &vault_path);
            let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &recovery_path);
            creator
                .create_at(
                    MAIN,
                    "native-unlock-test",
                    "Native Unlock Test",
                    token.as_str(),
                    secret(TEST_NATIVE_WALLET_PASSWORD),
                    1,
                )
                .unwrap();

            let guarded = WalletLifecycleAdapters::for_test_with_panic(
                Arc::clone(&runtime),
                &vault_path,
                checkpoint,
            );
            assert_eq!(
                guarded.unlock_native(MAIN).unwrap_err(),
                WalletLifecycleError::RuntimeUnavailable,
            );
            assert!(runtime.lifecycle_status(true).unwrap().locked);
        }
    }

    #[test]
    fn create_interruption_checkpoints_revoke_authority_and_preserve_only_completed_files() {
        let root = tempfile::tempdir().unwrap();
        let cases = [
            (
                WalletLifecycleCheckpoint::CreateDestinationConsumed,
                false,
                false,
            ),
            (WalletLifecycleCheckpoint::CreatePrepared, false, false),
            (
                WalletLifecycleCheckpoint::CreateRecoveryAcknowledged,
                false,
                false,
            ),
            (WalletLifecycleCheckpoint::CreateRecoveryStored, true, false),
            (
                WalletLifecycleCheckpoint::CreateRecoveryVerified,
                true,
                false,
            ),
            (WalletLifecycleCheckpoint::CreateVaultStored, true, true),
        ];

        for (index, (checkpoint, recovery_exists, vault_exists)) in cases.into_iter().enumerate() {
            let directory = root.path().join(format!("create-{index}"));
            fs::create_dir(&directory).unwrap();
            let recovery_path = directory.join("wallet.vision-recovery.json");
            let vault_path = directory.join("local").join(WALLET_VAULT_FILE);
            let runtime = Arc::new(WalletRuntimeState::for_test());
            let adapter = WalletLifecycleAdapters::for_test_with_interruption(
                Arc::clone(&runtime),
                &vault_path,
                checkpoint,
            );
            let token = selection_token(&runtime, RecoveryPathPurpose::Destination, &recovery_path);

            assert_eq!(
                adapter
                    .create_at(
                        MAIN,
                        &format!("interrupted-{index}"),
                        "Interrupted Wallet",
                        token.as_str(),
                        secret(WALLET_PASSWORD),
                        1,
                    )
                    .unwrap_err(),
                WalletLifecycleError::RuntimeUnavailable
            );
            assert_eq!(recovery_path.exists(), recovery_exists);
            assert_eq!(vault_path.exists(), vault_exists);
            let status = runtime.lifecycle_status(vault_exists).unwrap();
            assert!(status.locked);
            assert!(status.account.is_none());
            if recovery_exists {
                let encrypted = fs::read_to_string(&recovery_path).unwrap();
                assert!(!encrypted.contains(WALLET_PASSWORD));
                assert!(!encrypted.contains(INVALID_LEGACY_RECOVERY_PASSWORD));
            }
        }
    }

    #[test]
    fn restore_interruption_checkpoints_never_change_the_source_backup() {
        let root = tempfile::tempdir().unwrap();
        let recovery_path = root.path().join("source.vision-recovery.json");
        let source_runtime = Arc::new(WalletRuntimeState::for_test());
        let source_vault = root.path().join("source-local").join(WALLET_VAULT_FILE);
        let source = WalletLifecycleAdapters::for_test(Arc::clone(&source_runtime), &source_vault);
        let destination = selection_token(
            &source_runtime,
            RecoveryPathPurpose::Destination,
            &recovery_path,
        );
        source
            .create_at(
                MAIN,
                "source",
                "Source Wallet",
                destination.as_str(),
                secret(WALLET_PASSWORD),
                1,
            )
            .unwrap();
        let recovery_credential = source.take_test_recovery_credential().unwrap();
        let original_recovery = fs::read(&recovery_path).unwrap();
        let cases = [
            (WalletLifecycleCheckpoint::RestoreSourceConsumed, false),
            (WalletLifecycleCheckpoint::RestorePrepared, false),
            (WalletLifecycleCheckpoint::RestoreVaultStored, true),
        ];

        for (index, (checkpoint, vault_exists)) in cases.into_iter().enumerate() {
            let vault_path = root
                .path()
                .join(format!("restored-{index}"))
                .join(WALLET_VAULT_FILE);
            let runtime = Arc::new(WalletRuntimeState::for_test());
            let adapter = WalletLifecycleAdapters::for_test_with_interruption(
                Arc::clone(&runtime),
                &vault_path,
                checkpoint,
            );
            let source_token =
                selection_token(&runtime, RecoveryPathPurpose::Source, &recovery_path);

            assert_eq!(
                adapter
                    .restore_at(
                        MAIN,
                        &format!("restored-{index}"),
                        "Restored Wallet",
                        source_token.as_str(),
                        secret(RESTORED_PASSWORD),
                        secret(recovery_credential.as_str()),
                        2,
                    )
                    .unwrap_err(),
                WalletLifecycleError::RuntimeUnavailable
            );
            assert_eq!(fs::read(&recovery_path).unwrap(), original_recovery);
            assert_eq!(vault_path.exists(), vault_exists);
            let status = runtime.lifecycle_status(vault_exists).unwrap();
            assert!(status.locked);
            assert!(status.account.is_none());
        }
    }

    #[test]
    fn lifecycle_errors_are_fixed_and_never_disclose_paths_or_retry_timing() {
        let errors = [
            WalletLifecycleError::RuntimeUnavailable,
            WalletLifecycleError::ActivationUnavailable,
            WalletLifecycleError::InvalidWindow,
            WalletLifecycleError::OperationInProgress,
            WalletLifecycleError::InvalidRequest,
            WalletLifecycleError::PathAuthorizationInvalid,
            WalletLifecycleError::PathAuthorizationExpired,
            WalletLifecycleError::WalletAlreadyExists,
            WalletLifecycleError::WalletUnavailable,
            WalletLifecycleError::InvalidLabel,
            WalletLifecycleError::PasswordPolicy,
            WalletLifecycleError::InvalidPasswordOrDamage,
            WalletLifecycleError::UnlockTemporarilyBlocked,
            WalletLifecycleError::SecureRandomUnavailable,
            WalletLifecycleError::RecoveryProtectionUnavailable,
            WalletLifecycleError::RecoveryAcknowledgementCancelled,
            WalletLifecycleError::RecoveryAcknowledgementUnavailable,
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

    #[test]
    fn custody_root_requires_a_fixed_local_absolute_non_reparse_path() {
        let local_app_data = std::env::var_os("LOCALAPPDATA").unwrap();
        validate_local_custody_root(Path::new(&local_app_data)).unwrap();

        for rejected in [
            PathBuf::from("relative"),
            PathBuf::from(r"\\server\share\wallet"),
            PathBuf::from(r"\\?\C:\wallet"),
            PathBuf::from(r"C:\safe\..\wallet"),
        ] {
            assert_eq!(
                validate_local_custody_root(&rejected),
                Err(WalletLifecycleError::VaultStorageUnavailable)
            );
        }
    }
}
