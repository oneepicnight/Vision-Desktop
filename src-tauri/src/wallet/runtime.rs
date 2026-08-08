#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wallet runtime remains private until custody commands pass review"
    )
)]

use super::{
    account::derive_account_identity,
    activation::{WalletActivationPolicy, WalletActivationScope},
    contract::{WalletAccountSummary, WalletLifecycleStatus, WalletPublicMetadata},
    core_client::{WalletCoreSubmissionSource, SUPPORTED_WALLET_CORE_CONTRACT},
    journal::{append_accepted_evidence, WalletJournalAuthenticator},
    preview::BoundTransferPreview,
    receipt::prove_exact_reconciliation_lookup,
    reconciliation::{
        AcceptedRecordingAuthority, AcceptedSubmissionEvidence, LiveReconciliationAuthority,
        MayHaveBeenSubmittedAuthority, PreparedReconciliationAuthority,
        ReconciliationAuthenticator, ReconciliationDiscoveryPermit, ReconciliationError,
        ReconciliationRecord, ReconciliationStore, RestartReconciliationPermit,
        SubmissionActivationGrant,
    },
    secrets::WalletPassword,
    session::{WalletSession, WalletSessionError},
    transaction::{
        sign_confirmed_cash_transfer, TransactionSigningObserver, VisionTransaction,
        WalletTransactionError,
    },
    transaction_confirmation::NativeConfirmationApproval,
    vault::EncryptedWalletVault,
};
use std::{
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::Instant,
};
use zeroize::Zeroizing;

#[cfg(test)]
use super::activation::WalletActivationRequirement;

const MAIN_WINDOW_LABEL: &str = "main";
const PATH_TOKEN_BYTES: usize = 32;
const PATH_TOKEN_HEX_BYTES: usize = PATH_TOKEN_BYTES * 2;
const PATH_TOKEN_TTL_MS: u64 = 2 * 60 * 1000;
const WALLET_PROCESS_MUTEX_BASE: &str = "com.vision.desktop.wallet-runtime.v2";
const TRANSACTION_PREVIEW_TOKEN_BYTES: usize = 32;
const TRANSACTION_PREVIEW_TOKEN_HEX_BYTES: usize = TRANSACTION_PREVIEW_TOKEN_BYTES * 2;
pub(in crate::wallet) const TRANSACTION_PREVIEW_TTL_MS: u64 = 60 * 1000;

/// Rust-only wallet authority owned by the application process.
///
/// This type intentionally implements neither `Clone`, Serde traits, nor `Debug`. The Tauri
/// application manages exactly one instance, but no wallet command can access it yet.
pub(crate) struct WalletRuntimeState {
    inner: Mutex<WalletRuntimeInner>,
    revocation_epoch: AtomicU64,
    pending_revocations: AtomicU64,
    activation: WalletActivationPolicy,
    _process_lock: WalletProcessLock,
}

struct WalletRuntimeInner {
    started_at: Instant,
    session: WalletSession,
    active_operation: Option<ActiveOperation>,
    pending_path_selection: Option<PendingPathSelection>,
    path_authorization: Option<PathAuthorization>,
    transaction_preview: Option<PendingTransactionPreview>,
    public_account: Option<WalletAccountSummary>,
    next_generation: u64,
}

struct ActiveOperation {
    generation: u64,
    owner_window: String,
    kind: WalletOperationKind,
}

struct PendingPathSelection {
    generation: u64,
    owner_window: String,
    purpose: RecoveryPathPurpose,
}

struct PathAuthorization {
    token: Zeroizing<String>,
    owner_window: String,
    purpose: RecoveryPathPurpose,
    selected_path: PathBuf,
    issued_at_ms: u64,
}
struct PendingTransactionPreview {
    token: Zeroizing<String>,
    owner_window: String,
    wallet_id: String,
    issued_at_ms: u64,
    revocation_epoch: u64,
    intent: BoundTransferPreview,
}

pub(in crate::wallet) struct TransactionPreviewInstallReceipt {
    handle: String,
    issued_at_ms: u64,
}

impl TransactionPreviewInstallReceipt {
    pub(in crate::wallet) fn into_parts(self) -> (String, u64) {
        (self.handle, self.issued_at_ms)
    }
}

struct WalletProcessLock {
    _platform_lock: platform::ProcessLock,
}

pub(in crate::wallet) struct WalletOperationPermit<'a> {
    state: &'a WalletRuntimeState,
    generation: u64,
    revocation_epoch: u64,
    owner_window: String,
    kind: WalletOperationKind,
    activation_proof: WalletActivationProof,
    armed: bool,
}

/// Linear signing authority produced only by atomic promotion of the live confirmation permit.
///
/// This type intentionally implements neither Clone, Debug, nor serialization. Dropping it before
/// successful completion invalidates the wallet session and all outstanding authority.
pub(in crate::wallet) struct WalletSigningPermit<'a> {
    permit: WalletOperationPermit<'a>,
    wallet_id: String,
    armed: bool,
}

/// Linear submission authority produced only by atomic promotion of a live signing permit.
///
/// Dropping an armed value revokes the complete wallet session. This type intentionally has no
/// Clone, Debug, Display, or serialization implementation.
pub(in crate::wallet) struct WalletSubmissionPermit<'a> {
    permit: WalletOperationPermit<'a>,
    wallet_id: String,
    core_identity_fingerprint: [u8; 32],
    activation_grant: Option<SubmissionActivationGrant>,
    armed: bool,
}

pub(in crate::wallet) struct WalletReconciliationPermit<'a> {
    permit: WalletOperationPermit<'a>,
    wallet_id: String,
    armed: bool,
}

pub(in crate::wallet) struct WalletActivationProof {
    scope: WalletActivationScope,
}

pub(in crate::wallet) struct RecoveryPathToken(Zeroizing<String>);

pub(in crate::wallet) struct RecoverySelectionPermit {
    state: Arc<WalletRuntimeState>,
    generation: u64,
    owner_window: String,
    purpose: RecoveryPathPurpose,
    armed: bool,
}

impl RecoverySelectionPermit {
    fn disarm(&mut self) {
        self.armed = false;
    }

    fn invalidate_or_terminate(&mut self) {
        if !self.armed {
            return;
        }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.state.invalidate_all()))
        {
            Ok(Ok(())) => self.armed = false,
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }
}

impl Drop for RecoverySelectionPermit {
    fn drop(&mut self) {
        self.invalidate_or_terminate();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletOperationKind {
    Create,
    Restore,
    Unlock,
    PreparePreview,
    ConsumePreview,
    Sign,
    Submit,
    Reconcile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum RecoveryPathPurpose {
    Destination,
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalletRuntimeError {
    ProcessLockUnavailable,
    UnsupportedWindowsHost,
    RuntimeUnavailable,
    ActivationUnavailable,
    InvalidWindow,
    OperationInProgress,
    InvalidRequest,
    SecureRandomUnavailable,
    PathAuthorizationInvalid,
    PathAuthorizationExpired,
    RecoverySelectionCancelled,
    RecoveryDestinationInvalid,
    RecoveryDestinationExists,
    RecoverySourceInvalid,
    ReconciliationUnavailable,
}

impl WalletRuntimeError {
    pub(in crate::wallet) const fn code(self) -> &'static str {
        match self {
            Self::ProcessLockUnavailable => "wallet_process_lock_unavailable",
            Self::UnsupportedWindowsHost => "unsupported_windows_host",
            Self::RuntimeUnavailable => "wallet_runtime_unavailable",
            Self::ActivationUnavailable => "wallet_activation_unavailable",
            Self::InvalidWindow => "invalid_window",
            Self::OperationInProgress => "operation_in_progress",
            Self::InvalidRequest => "invalid_request",
            Self::SecureRandomUnavailable => "secure_random_unavailable",
            Self::PathAuthorizationInvalid => "path_authorization_invalid",
            Self::PathAuthorizationExpired => "path_authorization_expired",
            Self::RecoverySelectionCancelled => "recovery_selection_cancelled",
            Self::RecoveryDestinationInvalid => "recovery_destination_invalid",
            Self::RecoveryDestinationExists => "recovery_destination_exists",
            Self::RecoverySourceInvalid => "recovery_source_invalid",
            Self::ReconciliationUnavailable => "wallet_reconciliation_unavailable",
        }
    }
}

impl fmt::Display for WalletRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProcessLockUnavailable => "secure wallet process ownership is unavailable",
            Self::UnsupportedWindowsHost => "wallet custody is unavailable on this Windows edition",
            Self::RuntimeUnavailable => "secure wallet runtime is unavailable",
            Self::ActivationUnavailable => "secure wallet activation is unavailable",
            Self::InvalidWindow => "wallet access is unavailable from this window",
            Self::OperationInProgress => "another wallet operation is already in progress",
            Self::InvalidRequest => "wallet request is invalid",
            Self::SecureRandomUnavailable => "secure operating-system randomness is unavailable",
            Self::PathAuthorizationInvalid => "recovery selection is invalid",
            Self::PathAuthorizationExpired => "recovery selection has expired",
            Self::RecoverySelectionCancelled => "recovery selection was cancelled",
            Self::RecoveryDestinationInvalid => "recovery destination is invalid",
            Self::RecoveryDestinationExists => "recovery destination already exists",
            Self::RecoverySourceInvalid => "recovery source is invalid",
            Self::ReconciliationUnavailable => "wallet reconciliation is unavailable",
        })
    }
}

impl std::error::Error for WalletRuntimeError {}

impl WalletRuntimeState {
    pub(crate) fn initialize() -> Result<Self, WalletRuntimeError> {
        Self::with_process_lock(
            WalletProcessLock::acquire(WALLET_PROCESS_MUTEX_BASE)?,
            WalletActivationPolicy::production(),
        )
    }

    fn with_process_lock(
        process_lock: WalletProcessLock,
        activation: WalletActivationPolicy,
    ) -> Result<Self, WalletRuntimeError> {
        Ok(Self {
            inner: Mutex::new(WalletRuntimeInner {
                started_at: Instant::now(),
                session: WalletSession::new(),
                active_operation: None,
                pending_path_selection: None,
                path_authorization: None,
                public_account: None,
                transaction_preview: None,
                next_generation: 0,
            }),
            revocation_epoch: AtomicU64::new(1),
            pending_revocations: AtomicU64::new(0),
            activation,
            _process_lock: process_lock,
        })
    }

    pub(in crate::wallet) fn begin_operation(
        &self,
        owner_window: &str,
        kind: WalletOperationKind,
    ) -> Result<WalletOperationPermit<'_>, WalletRuntimeError> {
        require_main_window(owner_window)?;
        let activation_scope = kind.activation_scope();
        self.require_activation(activation_scope)?;
        if matches!(
            kind,
            WalletOperationKind::Sign
                | WalletOperationKind::Submit
                | WalletOperationKind::Reconcile
        ) {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        let mut inner = self.lock_inner()?;
        if self.revocation_is_pending() {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        if inner.active_operation.is_some() || inner.pending_path_selection.is_some() {
            return Err(WalletRuntimeError::OperationInProgress);
        }
        inner.next_generation = inner.next_generation.wrapping_add(1).max(1);
        let generation = inner.next_generation;
        let revocation_epoch = self.revocation_epoch.load(Ordering::Acquire);
        if matches!(
            kind,
            WalletOperationKind::Create
                | WalletOperationKind::Restore
                | WalletOperationKind::Unlock
                | WalletOperationKind::PreparePreview
        ) {
            inner.transaction_preview = None;
        }
        inner.active_operation = Some(ActiveOperation {
            generation,
            owner_window: owner_window.to_string(),
            kind,
        });
        Ok(WalletOperationPermit {
            state: self,
            generation,
            revocation_epoch,
            owner_window: owner_window.to_string(),
            kind,
            activation_proof: WalletActivationProof {
                scope: activation_scope,
            },
            armed: true,
        })
    }

    pub(in crate::wallet) fn begin_recovery_path_selection(
        self: &Arc<Self>,
        owner_window: &str,
        purpose: RecoveryPathPurpose,
    ) -> Result<RecoverySelectionPermit, WalletRuntimeError> {
        require_main_window(owner_window)?;
        self.require_activation(WalletActivationScope::Lifecycle)?;
        let mut inner = self.lock_inner()?;
        if self.revocation_is_pending() {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        if inner.active_operation.is_some() || inner.pending_path_selection.is_some() {
            return Err(WalletRuntimeError::OperationInProgress);
        }
        inner.next_generation = inner.next_generation.wrapping_add(1).max(1);
        let generation = inner.next_generation;
        inner.path_authorization = None;
        inner.pending_path_selection = Some(PendingPathSelection {
            generation,
            owner_window: owner_window.to_string(),
            purpose,
        });
        Ok(RecoverySelectionPermit {
            state: Arc::clone(self),
            generation,
            owner_window: owner_window.to_string(),
            purpose,
            armed: true,
        })
    }

    pub(in crate::wallet) fn begin_reconciliation_discovery(
        &self,
        owner_window: &str,
    ) -> Result<WalletReconciliationPermit<'_>, WalletRuntimeError> {
        require_main_window(owner_window)?;
        self.require_activation(WalletActivationScope::Reconciliation)?;
        let mut inner = self.lock_inner()?;
        if self.revocation_is_pending()
            || inner.active_operation.is_some()
            || inner.pending_path_selection.is_some()
        {
            return Err(WalletRuntimeError::OperationInProgress);
        }
        let wallet_id = inner
            .session
            .active_wallet_id()
            .map_err(|_| WalletRuntimeError::RuntimeUnavailable)?;
        inner.next_generation = inner.next_generation.wrapping_add(1).max(1);
        let generation = inner.next_generation;
        let revocation_epoch = self.revocation_epoch.load(Ordering::Acquire);
        inner.active_operation = Some(ActiveOperation {
            generation,
            owner_window: owner_window.to_string(),
            kind: WalletOperationKind::Reconcile,
        });
        Ok(WalletReconciliationPermit {
            permit: WalletOperationPermit {
                state: self,
                generation,
                revocation_epoch,
                owner_window: owner_window.to_string(),
                kind: WalletOperationKind::Reconcile,
                activation_proof: WalletActivationProof {
                    scope: WalletActivationScope::Reconciliation,
                },
                armed: true,
            },
            wallet_id,
            armed: true,
        })
    }

    pub(in crate::wallet) fn complete_recovery_path_selection(
        &self,
        permit: RecoverySelectionPermit,
        selected_path: PathBuf,
    ) -> Result<RecoveryPathToken, WalletRuntimeError> {
        let mut token_bytes = Zeroizing::new([0_u8; PATH_TOKEN_BYTES]);
        if getrandom::fill(&mut *token_bytes).is_err() {
            return Err(WalletRuntimeError::SecureRandomUnavailable);
        }
        let now_ms = self.now_ms()?;
        self.complete_recovery_path_selection_at(permit, selected_path, &token_bytes, now_ms)
    }

    fn complete_recovery_path_selection_at(
        &self,
        mut permit: RecoverySelectionPermit,
        selected_path: PathBuf,
        token_bytes: &[u8; PATH_TOKEN_BYTES],
        now_ms: u64,
    ) -> Result<RecoveryPathToken, WalletRuntimeError> {
        if selected_path.as_os_str().is_empty() {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        if !std::ptr::eq(permit.state.as_ref(), self) {
            return Err(WalletRuntimeError::PathAuthorizationInvalid);
        }
        let mut inner = self.lock_inner()?;
        if self.revocation_is_pending() {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        if !inner
            .pending_path_selection
            .as_ref()
            .is_some_and(|pending| pending.matches(&permit))
        {
            return Err(WalletRuntimeError::PathAuthorizationInvalid);
        }
        inner.pending_path_selection = None;
        let token = hex::encode(token_bytes);
        let owner_window = std::mem::take(&mut permit.owner_window);
        inner.path_authorization = Some(PathAuthorization {
            token: Zeroizing::new(token.clone()),
            owner_window,
            purpose: permit.purpose,
            selected_path,
            issued_at_ms: now_ms,
        });
        permit.disarm();
        Ok(RecoveryPathToken(Zeroizing::new(token)))
    }

    pub(in crate::wallet) fn cancel_recovery_path_selection(
        &self,
        permit: &mut RecoverySelectionPermit,
    ) -> Result<(), WalletRuntimeError> {
        if !std::ptr::eq(permit.state.as_ref(), self) {
            return Err(WalletRuntimeError::PathAuthorizationInvalid);
        }
        let mut inner = self.lock_inner()?;
        if self.revocation_is_pending() {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        if !inner
            .pending_path_selection
            .as_ref()
            .is_some_and(|pending| pending.matches(permit))
        {
            return Err(WalletRuntimeError::PathAuthorizationInvalid);
        }
        inner.pending_path_selection = None;
        inner.path_authorization = None;
        permit.disarm();
        Ok(())
    }

    pub(in crate::wallet) fn consume_recovery_path(
        &self,
        owner_window: &str,
        purpose: RecoveryPathPurpose,
        token: &str,
    ) -> Result<PathBuf, WalletRuntimeError> {
        let now_ms = self.now_ms()?;
        self.consume_recovery_path_at(owner_window, purpose, token, now_ms)
    }

    fn consume_recovery_path_at(
        &self,
        owner_window: &str,
        purpose: RecoveryPathPurpose,
        token: &str,
        now_ms: u64,
    ) -> Result<PathBuf, WalletRuntimeError> {
        require_main_window(owner_window)?;
        if token.len() != PATH_TOKEN_HEX_BYTES
            || !token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        let mut inner = self.lock_inner()?;
        if self.revocation_is_pending() {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let authorization = inner
            .path_authorization
            .take()
            .ok_or(WalletRuntimeError::PathAuthorizationInvalid)?;
        if authorization.owner_window != owner_window
            || authorization.purpose != purpose
            || authorization.token.as_str() != token
        {
            return Err(WalletRuntimeError::PathAuthorizationInvalid);
        }
        if now_ms < authorization.issued_at_ms
            || now_ms.saturating_sub(authorization.issued_at_ms) > PATH_TOKEN_TTL_MS
        {
            return Err(WalletRuntimeError::PathAuthorizationExpired);
        }
        Ok(authorization.selected_path)
    }

    pub(crate) fn invalidate_all(&self) -> Result<(), WalletRuntimeError> {
        self.pending_revocations.fetch_add(1, Ordering::AcqRel);
        self.revoke_current_authority();
        let result = match self.inner.lock() {
            Ok(mut inner) => {
                inner.invalidate_all();
                Ok(())
            }
            Err(poisoned) => {
                poisoned.into_inner().invalidate_all();
                Err(WalletRuntimeError::RuntimeUnavailable)
            }
        };
        self.pending_revocations.fetch_sub(1, Ordering::AcqRel);
        result
    }

    pub(in crate::wallet) fn remember_public_metadata(
        &self,
        metadata: WalletPublicMetadata,
    ) -> Result<WalletLifecycleStatus, WalletRuntimeError> {
        let mut inner = self.lock_inner()?;
        inner.session.lock();
        inner.public_account = Some(metadata.into());
        inner.transaction_preview = None;
        Ok(inner.lifecycle_status(true))
    }

    pub(in crate::wallet) fn unlock_vault(
        &self,
        activation: &WalletActivationProof,
        vault: &EncryptedWalletVault,
        password: &WalletPassword,
    ) -> Result<WalletLifecycleStatus, WalletSessionError> {
        let mut inner = self
            .lock_inner()
            .map_err(|_| WalletSessionError::VaultUnavailable)?;
        inner.session.unlock(activation, vault, password)?;
        let identity = match inner
            .session
            .with_seed(|wallet_id, seed| (wallet_id.to_string(), derive_account_identity(seed)))
        {
            Ok(identity) => identity,
            Err(error) => {
                inner.session.lock();
                return Err(error);
            }
        };
        if identity.0 != vault.wallet_id()
            || inner.public_account.as_ref().is_some_and(|account| {
                account.wallet_id != identity.0
                    || account.address != identity.1.address
                    || account.public_key != identity.1.public_key
            })
        {
            inner.session.lock();
            return Err(WalletSessionError::VaultUnavailable);
        }
        let prior = inner.public_account.take();
        inner.public_account = Some(WalletAccountSummary {
            wallet_id: identity.0,
            label: prior.as_ref().and_then(|account| account.label.clone()),
            public_key: identity.1.public_key,
            address: identity.1.address,
            created_at_unix_ms: vault.created_at_unix_ms(),
            backup_verified: prior.and_then(|account| account.backup_verified),
        });
        Ok(inner.lifecycle_status(true))
    }

    pub(in crate::wallet) fn lifecycle_status(
        &self,
        vault_exists: bool,
    ) -> Result<WalletLifecycleStatus, WalletRuntimeError> {
        let mut inner = self.lock_inner()?;
        if !vault_exists {
            inner.session.lock();
            inner.public_account = None;
            inner.transaction_preview = None;
        }
        Ok(inner.lifecycle_status(vault_exists))
    }

    pub(in crate::wallet) fn lifecycle_status_for_vault(
        &self,
        vault: &EncryptedWalletVault,
    ) -> Result<WalletLifecycleStatus, WalletRuntimeError> {
        let mut inner = self.lock_inner()?;
        if inner.public_account.as_ref().is_some_and(|account| {
            account.wallet_id != vault.wallet_id()
                || account.created_at_unix_ms != vault.created_at_unix_ms()
        }) {
            inner.session.lock();
            inner.public_account = None;
            inner.transaction_preview = None;
        }
        Ok(inner.lifecycle_status(true))
    }

    fn now_ms(&self) -> Result<u64, WalletRuntimeError> {
        let inner = self.lock_inner()?;
        Ok(u64::try_from(inner.started_at.elapsed().as_millis()).unwrap_or(u64::MAX))
    }

    fn require_activation(&self, scope: WalletActivationScope) -> Result<(), WalletRuntimeError> {
        if self.activation.is_satisfied(scope) {
            Ok(())
        } else {
            Err(WalletRuntimeError::ActivationUnavailable)
        }
    }

    fn lock_inner(&self) -> Result<MutexGuard<'_, WalletRuntimeInner>, WalletRuntimeError> {
        match self.inner.lock() {
            Ok(inner) => Ok(inner),
            Err(poisoned) => {
                self.pending_revocations.fetch_add(1, Ordering::AcqRel);
                self.revoke_current_authority();
                let mut inner = poisoned.into_inner();
                inner.invalidate_all();
                self.pending_revocations.fetch_sub(1, Ordering::AcqRel);
                Err(WalletRuntimeError::RuntimeUnavailable)
            }
        }
    }

    fn revoke_current_authority(&self) {
        self.revocation_epoch.fetch_add(1, Ordering::AcqRel);
    }

    fn revocation_is_pending(&self) -> bool {
        self.pending_revocations.load(Ordering::Acquire) != 0
    }

    #[cfg(test)]
    pub(in crate::wallet) fn revocation_is_pending_for_test(&self) -> bool {
        self.revocation_is_pending()
    }

    #[cfg(test)]
    pub(in crate::wallet) fn with_activation_proof_for_test<R>(
        kind: WalletOperationKind,
        operation: impl FnOnce(&WalletActivationProof) -> R,
    ) -> R {
        operation(&WalletActivationProof {
            scope: kind.activation_scope(),
        })
    }

    #[cfg(test)]
    pub(in crate::wallet) fn for_test() -> Self {
        Self::for_test_with_activation(WalletActivationPolicy::satisfied_for_test())
    }

    #[cfg(test)]
    pub(in crate::wallet) fn for_test_missing_activation(
        requirement: WalletActivationRequirement,
    ) -> Self {
        Self::for_test_with_activation(WalletActivationPolicy::missing_for_test(requirement))
    }

    #[cfg(test)]
    pub(in crate::wallet) fn for_test_with_production_activation() -> Self {
        Self::for_test_with_activation(WalletActivationPolicy::production())
    }

    #[cfg(test)]
    fn for_test_with_activation(activation: WalletActivationPolicy) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_TEST_LOCK: AtomicU64 = AtomicU64::new(1);
        let suffix = NEXT_TEST_LOCK.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "com.vision.desktop.wallet-runtime.test.{}.{}",
            std::process::id(),
            suffix
        );
        Self::with_process_lock(WalletProcessLock::acquire(&name).unwrap(), activation).unwrap()
    }
}

impl WalletOperationKind {
    const fn activation_scope(self) -> WalletActivationScope {
        match self {
            Self::Create
            | Self::Restore
            | Self::Unlock
            | Self::PreparePreview
            | Self::ConsumePreview => WalletActivationScope::Lifecycle,
            Self::Sign => WalletActivationScope::Signing,
            Self::Submit => WalletActivationScope::Submission,
            Self::Reconcile => WalletActivationScope::Reconciliation,
        }
    }
}

impl WalletActivationProof {
    pub(in crate::wallet) fn require_signing(&self) -> Result<(), WalletRuntimeError> {
        if self.scope == WalletActivationScope::Signing {
            Ok(())
        } else {
            Err(WalletRuntimeError::ActivationUnavailable)
        }
    }

    pub(in crate::wallet) fn require_submission(&self) -> Result<(), WalletRuntimeError> {
        if self.scope == WalletActivationScope::Submission {
            Ok(())
        } else {
            Err(WalletRuntimeError::ActivationUnavailable)
        }
    }

    pub(in crate::wallet) fn require_reconciliation(&self) -> Result<(), WalletRuntimeError> {
        if self.scope == WalletActivationScope::Reconciliation {
            Ok(())
        } else {
            Err(WalletRuntimeError::ActivationUnavailable)
        }
    }
}

impl WalletRuntimeInner {
    fn invalidate_all(&mut self) {
        self.session.lock();
        self.active_operation = None;
        self.pending_path_selection = None;
        self.path_authorization = None;
        self.transaction_preview = None;
    }

    fn lifecycle_status(&mut self, vault_exists: bool) -> WalletLifecycleStatus {
        let locked = self.session.is_locked();
        if locked {
            self.transaction_preview = None;
        }
        WalletLifecycleStatus {
            vault_exists,
            locked,
            account: self.public_account.clone(),
        }
    }
}

impl PendingPathSelection {
    fn matches(&self, permit: &RecoverySelectionPermit) -> bool {
        self.generation == permit.generation
            && self.owner_window == permit.owner_window
            && self.purpose == permit.purpose
    }
}

impl Drop for WalletRuntimeState {
    fn drop(&mut self) {
        self.pending_revocations.store(1, Ordering::Release);
        self.revocation_epoch.fetch_add(1, Ordering::AcqRel);
        match self.inner.get_mut() {
            Ok(inner) => inner.invalidate_all(),
            Err(poisoned) => poisoned.into_inner().invalidate_all(),
        }
    }
}

impl Drop for WalletOperationPermit<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Ok(mut inner) = self.state.lock_inner() else {
            return;
        };
        if inner.active_operation.as_ref().is_some_and(|operation| {
            operation.generation == self.generation && operation.owner_window == self.owner_window
        }) {
            inner.active_operation = None;
        }
    }
}

impl<'a> WalletOperationPermit<'a> {
    /// Proves that no lifecycle event, explicit lock, or newer operation revoked this work.
    pub(in crate::wallet) fn ensure_current(&self) -> Result<(), WalletRuntimeError> {
        if self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let inner = self.state.lock_inner()?;
        if self.is_current(&inner)
            && !self.state.revocation_is_pending()
            && self.state.revocation_epoch.load(Ordering::Acquire) == self.revocation_epoch
        {
            Ok(())
        } else {
            Err(WalletRuntimeError::RuntimeUnavailable)
        }
    }

    /// Executes one sensitive or irreversible stage under generation-bound authority. Revocation
    /// is observed before the stage and again before its result can escape. The inner result keeps
    /// domain errors distinct from runtime revocation without exposing authority to the caller.
    pub(in crate::wallet) fn run_authorized<T, E>(
        &self,
        stage: impl FnOnce(&WalletActivationProof) -> Result<T, E>,
    ) -> Result<Result<T, E>, WalletRuntimeError> {
        self.ensure_current()?;
        let result = stage(&self.activation_proof);
        self.ensure_current()?;
        Ok(result)
    }

    /// Linearizes successful completion against lifecycle revocation and consumes this operation's
    /// active slot. A revocation epoch already requested cannot produce a success value.
    pub(in crate::wallet) fn complete<T>(&self, value: T) -> Result<T, WalletRuntimeError> {
        if self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let mut inner = self.state.lock_inner()?;
        if !self.is_current(&inner)
            || self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        inner.active_operation = None;
        Ok(value)
    }

    /// Atomically promotes the still-live confirmation operation into signing scope.
    ///
    /// Consuming the native approval is mandatory. The old permit is explicitly disarmed before
    /// it is dropped so it cannot clear the continuously occupied active-operation slot.
    pub(in crate::wallet) fn promote_to_signing(
        mut self,
        _approval: NativeConfirmationApproval,
        sender_address: &str,
        sender_public_key: &str,
    ) -> Result<WalletSigningPermit<'a>, WalletRuntimeError> {
        if self.kind != WalletOperationKind::ConsumePreview
            || self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let mut inner = self.state.lock_inner()?;
        if !self.is_current(&inner)
            || !inner
                .active_operation
                .as_ref()
                .is_some_and(|operation| operation.kind == WalletOperationKind::ConsumePreview)
            || self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        if !self
            .state
            .activation
            .is_satisfied(WalletActivationScope::Signing)
        {
            return Err(WalletRuntimeError::ActivationUnavailable);
        }
        let wallet_id = inner
            .session
            .active_wallet_id()
            .map_err(|_| WalletRuntimeError::RuntimeUnavailable)?;
        let account_matches = inner.public_account.as_ref().is_some_and(|account| {
            account.wallet_id == wallet_id
                && account.address == sender_address
                && account.public_key == sender_public_key
        });
        if !account_matches
            || self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let active = inner
            .active_operation
            .as_mut()
            .ok_or(WalletRuntimeError::RuntimeUnavailable)?;
        active.kind = WalletOperationKind::Sign;
        drop(inner);

        let promoted = WalletOperationPermit {
            state: self.state,
            generation: self.generation,
            revocation_epoch: self.revocation_epoch,
            owner_window: std::mem::take(&mut self.owner_window),
            kind: WalletOperationKind::Sign,
            activation_proof: WalletActivationProof {
                scope: WalletActivationScope::Signing,
            },
            armed: true,
        };
        self.armed = false;
        Ok(WalletSigningPermit {
            permit: promoted,
            wallet_id,
            armed: true,
        })
    }

    pub(in crate::wallet) fn current_public_account(
        &self,
    ) -> Result<WalletAccountSummary, WalletRuntimeError> {
        if self.kind != WalletOperationKind::PreparePreview {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        self.ensure_current()?;
        let mut inner = self.state.lock_inner()?;
        let wallet_id = inner
            .session
            .active_wallet_id()
            .map_err(|_| WalletRuntimeError::InvalidRequest)?;
        let account = inner
            .public_account
            .as_ref()
            .filter(|account| account.wallet_id == wallet_id)
            .cloned()
            .ok_or(WalletRuntimeError::InvalidRequest)?;
        if !self.is_current(&inner)
            || self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        Ok(account)
    }

    pub(in crate::wallet) fn complete_transaction_preview(
        &self,
        intent: BoundTransferPreview,
    ) -> Result<TransactionPreviewInstallReceipt, WalletRuntimeError> {
        if self.kind != WalletOperationKind::PreparePreview {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        let mut token_bytes = Zeroizing::new([0_u8; TRANSACTION_PREVIEW_TOKEN_BYTES]);
        if getrandom::fill(&mut *token_bytes).is_err() {
            return Err(WalletRuntimeError::SecureRandomUnavailable);
        }
        let handle = hex::encode(token_bytes.as_slice());
        if self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let mut inner = self.state.lock_inner()?;
        if !self.is_current(&inner)
            || self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let wallet_id = inner
            .session
            .active_wallet_id()
            .map_err(|_| WalletRuntimeError::InvalidRequest)?;
        let account_matches = inner.public_account.as_ref().is_some_and(|account| {
            account.wallet_id == wallet_id
                && account.address == intent.sender_address()
                && account.public_key == intent.sender_public_key()
        });
        if !account_matches {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        let issued_at_ms =
            u64::try_from(inner.started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        inner.transaction_preview = Some(PendingTransactionPreview {
            token: Zeroizing::new(handle.clone()),
            owner_window: self.owner_window.clone(),
            wallet_id,
            issued_at_ms,
            revocation_epoch: self.revocation_epoch,
            intent,
        });
        inner.active_operation = None;
        Ok(TransactionPreviewInstallReceipt {
            handle,
            issued_at_ms,
        })
    }

    pub(in crate::wallet) fn consume_transaction_preview(
        &self,
        handle: &str,
    ) -> Result<BoundTransferPreview, WalletRuntimeError> {
        self.consume_transaction_preview_at(handle, None)
    }

    #[cfg(test)]
    pub(in crate::wallet) fn consume_transaction_preview_at_for_test(
        &self,
        handle: &str,
        now_ms: u64,
    ) -> Result<BoundTransferPreview, WalletRuntimeError> {
        self.consume_transaction_preview_at(handle, Some(now_ms))
    }

    fn consume_transaction_preview_at(
        &self,
        handle: &str,
        now_override_ms: Option<u64>,
    ) -> Result<BoundTransferPreview, WalletRuntimeError> {
        if self.kind != WalletOperationKind::ConsumePreview
            || handle.len() != TRANSACTION_PREVIEW_TOKEN_HEX_BYTES
            || !handle
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        if self.state.revocation_is_pending()
            || self.state.revocation_epoch.load(Ordering::Acquire) != self.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let mut inner = self.state.lock_inner()?;
        if !self.is_current(&inner) {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let preview = inner
            .transaction_preview
            .take()
            .ok_or(WalletRuntimeError::InvalidRequest)?;
        let now_ms = now_override_ms.unwrap_or_else(|| {
            u64::try_from(inner.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
        });
        let wallet_id = inner
            .session
            .active_wallet_id()
            .map_err(|_| WalletRuntimeError::InvalidRequest)?;
        let account_matches = inner.public_account.as_ref().is_some_and(|account| {
            account.wallet_id == wallet_id
                && account.address == preview.intent.sender_address()
                && account.public_key == preview.intent.sender_public_key()
        });
        let valid = preview.owner_window == self.owner_window
            && preview.wallet_id == wallet_id
            && preview.revocation_epoch == self.revocation_epoch
            && preview.token.as_str() == handle
            && now_ms >= preview.issued_at_ms
            && now_ms.saturating_sub(preview.issued_at_ms) <= TRANSACTION_PREVIEW_TTL_MS
            && account_matches
            && !self.state.revocation_is_pending()
            && self.state.revocation_epoch.load(Ordering::Acquire) == self.revocation_epoch;
        if !valid {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        Ok(preview.intent)
    }

    fn is_current(&self, inner: &WalletRuntimeInner) -> bool {
        inner.active_operation.as_ref().is_some_and(|operation| {
            operation.generation == self.generation && operation.owner_window == self.owner_window
        })
    }
}

impl<'a> WalletSigningPermit<'a> {
    pub(in crate::wallet) fn wallet_id(&self) -> &str {
        self.wallet_id.as_str()
    }

    pub(in crate::wallet) fn ensure_current(&self) -> Result<(), WalletRuntimeError> {
        if self.permit.kind != WalletOperationKind::Sign {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        self.permit.ensure_current()?;
        let inner = self.permit.state.lock_inner()?;
        if inner
            .active_operation
            .as_ref()
            .is_some_and(|operation| operation.kind == WalletOperationKind::Sign)
        {
            Ok(())
        } else {
            Err(WalletRuntimeError::RuntimeUnavailable)
        }
    }

    pub(in crate::wallet) fn sign_confirmed_intent_with_observer(
        &self,
        intent: &BoundTransferPreview,
        expected_core_contract: &str,
        expected_status_version: &str,
        observer: &dyn TransactionSigningObserver,
    ) -> Result<Result<VisionTransaction, WalletTransactionError>, WalletRuntimeError> {
        self.ensure_current()?;
        self.permit.activation_proof.require_signing()?;
        if !self
            .permit
            .state
            .activation
            .is_satisfied(WalletActivationScope::Signing)
        {
            return Err(WalletRuntimeError::ActivationUnavailable);
        }
        let mut inner = self.permit.state.lock_inner()?;
        if !self.permit.is_current(&inner)
            || self.permit.state.revocation_is_pending()
            || self.permit.state.revocation_epoch.load(Ordering::Acquire)
                != self.permit.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let active_wallet_id = inner
            .session
            .active_wallet_id()
            .map_err(|_| WalletRuntimeError::RuntimeUnavailable)?;
        let account_matches = active_wallet_id == self.wallet_id
            && inner.public_account.as_ref().is_some_and(|account| {
                account.wallet_id == self.wallet_id
                    && account.address == intent.sender_address()
                    && account.public_key == intent.sender_public_key()
            });
        if !account_matches {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }

        let signing_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inner.session.with_seed(|wallet_id, seed| {
                if wallet_id != self.wallet_id {
                    return Err(WalletTransactionError::InvalidSender);
                }
                sign_confirmed_cash_transfer(
                    &self.permit.activation_proof,
                    seed,
                    intent.confirmed_cash_transfer(),
                    expected_core_contract,
                    expected_status_version,
                    observer,
                )
            })
        }));
        let result = match signing_result {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => return Err(WalletRuntimeError::RuntimeUnavailable),
            Err(payload) => {
                self.permit.state.revoke_current_authority();
                inner.invalidate_all();
                drop(inner);
                std::panic::resume_unwind(payload);
            }
        };
        if !self.permit.is_current(&inner)
            || self.permit.state.revocation_is_pending()
            || self.permit.state.revocation_epoch.load(Ordering::Acquire)
                != self.permit.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        Ok(result)
    }

    /// Atomically promotes the occupied signing slot to submission authority.
    pub(in crate::wallet) fn promote_to_submission(
        mut self,
        core_identity_fingerprint: [u8; 32],
    ) -> Result<WalletSubmissionPermit<'a>, WalletRuntimeError> {
        self.ensure_current()?;
        if !self
            .permit
            .state
            .activation
            .is_satisfied(WalletActivationScope::Submission)
        {
            return Err(WalletRuntimeError::ActivationUnavailable);
        }
        let mut inner = self.permit.state.lock_inner()?;
        if !self.permit.is_current(&inner)
            || self.permit.state.revocation_is_pending()
            || self.permit.state.revocation_epoch.load(Ordering::Acquire)
                != self.permit.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let active_wallet_id = inner
            .session
            .active_wallet_id()
            .map_err(|_| WalletRuntimeError::RuntimeUnavailable)?;
        if active_wallet_id != self.wallet_id {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let active = inner
            .active_operation
            .as_mut()
            .filter(|operation| operation.kind == WalletOperationKind::Sign)
            .ok_or(WalletRuntimeError::RuntimeUnavailable)?;
        active.kind = WalletOperationKind::Submit;
        drop(inner);

        let promoted = WalletOperationPermit {
            state: self.permit.state,
            generation: self.permit.generation,
            revocation_epoch: self.permit.revocation_epoch,
            owner_window: std::mem::take(&mut self.permit.owner_window),
            kind: WalletOperationKind::Submit,
            activation_proof: WalletActivationProof {
                scope: WalletActivationScope::Submission,
            },
            armed: true,
        };
        let activation_grant = SubmissionActivationGrant::new(&promoted.activation_proof)?;
        self.permit.armed = false;
        self.armed = false;
        Ok(WalletSubmissionPermit {
            permit: promoted,
            wallet_id: std::mem::take(&mut self.wallet_id),
            core_identity_fingerprint,
            activation_grant: Some(activation_grant),
            armed: true,
        })
    }

    pub(in crate::wallet) fn complete<T>(mut self, value: T) -> Result<T, WalletRuntimeError> {
        let result = self.permit.complete(value);
        if result.is_ok() {
            self.armed = false;
        }
        result
    }
}

impl WalletSubmissionPermit<'_> {
    pub(in crate::wallet) fn ensure_current(&self) -> Result<(), WalletRuntimeError> {
        self.permit.activation_proof.require_submission()?;
        if self.permit.kind != WalletOperationKind::Submit
            || !self
                .permit
                .state
                .activation
                .is_satisfied(WalletActivationScope::Submission)
        {
            return Err(WalletRuntimeError::ActivationUnavailable);
        }
        self.permit.ensure_current()?;
        let mut inner = self.permit.state.lock_inner()?;
        let active_kind_matches = inner
            .active_operation
            .as_ref()
            .is_some_and(|operation| operation.kind == WalletOperationKind::Submit);
        let wallet_matches =
            inner.session.active_wallet_id().ok().as_deref() == Some(self.wallet_id.as_str());
        let current = active_kind_matches && wallet_matches;
        if current {
            Ok(())
        } else {
            Err(WalletRuntimeError::RuntimeUnavailable)
        }
    }

    pub(in crate::wallet) fn wallet_id(&self) -> &str {
        &self.wallet_id
    }

    pub(in crate::wallet) const fn core_identity_fingerprint(&self) -> &[u8; 32] {
        &self.core_identity_fingerprint
    }

    pub(in crate::wallet) fn take_activation_grant(
        &mut self,
    ) -> Result<SubmissionActivationGrant, WalletRuntimeError> {
        self.ensure_current()?;
        self.activation_grant
            .take()
            .ok_or(WalletRuntimeError::RuntimeUnavailable)
    }

    pub(in crate::wallet) fn publish_prepared(
        &self,
        authority: LiveReconciliationAuthority,
        store: &ReconciliationStore,
        record: ReconciliationRecord,
    ) -> Result<PreparedReconciliationAuthority, WalletRuntimeError> {
        self.with_reconciliation_auth(|authenticator| {
            authority.publish_prepared(store, authenticator, record)
        })
    }

    pub(in crate::wallet) fn publish_may_have_been_submitted(
        &self,
        authority: PreparedReconciliationAuthority,
        store: &ReconciliationStore,
    ) -> Result<MayHaveBeenSubmittedAuthority, WalletRuntimeError> {
        self.with_reconciliation_auth(|authenticator| {
            authority.publish_may_have_been_submitted(store, authenticator)
        })
    }

    pub(in crate::wallet) fn resolve_not_attempted(
        &self,
        authority: PreparedReconciliationAuthority,
        store: &ReconciliationStore,
    ) -> Result<(), WalletRuntimeError> {
        self.with_reconciliation_auth(|authenticator| {
            authority.resolve_not_attempted(store, authenticator)
        })
    }

    pub(in crate::wallet) fn publish_accepted(
        &self,
        authority: MayHaveBeenSubmittedAuthority,
        store: &ReconciliationStore,
        accepted_tx_id: String,
        accepted_nonce: u64,
        compatibility_contract_digest_hex: String,
    ) -> Result<AcceptedRecordingAuthority, WalletRuntimeError> {
        self.with_reconciliation_auth(|authenticator| {
            authority.publish_accepted(
                store,
                authenticator,
                accepted_tx_id,
                accepted_nonce,
                compatibility_contract_digest_hex,
            )
        })
    }

    pub(in crate::wallet) fn resolve_rejected(
        &self,
        authority: MayHaveBeenSubmittedAuthority,
        store: &ReconciliationStore,
        http_status: u16,
        rejection_code: String,
        allowlist_digest_hex: String,
    ) -> Result<(), WalletRuntimeError> {
        self.with_reconciliation_auth(|authenticator| {
            authority.resolve_rejected(
                store,
                authenticator,
                http_status,
                rejection_code,
                allowlist_digest_hex,
            )
        })
    }

    pub(in crate::wallet) fn resolve_recorded(
        &self,
        authority: AcceptedRecordingAuthority,
        store: &ReconciliationStore,
    ) -> Result<(), WalletRuntimeError> {
        self.with_reconciliation_auth(|authenticator| {
            authority.resolve_recorded(store, authenticator)
        })
    }

    pub(in crate::wallet) fn record_accepted_evidence(
        &self,
        journal_path: &std::path::Path,
        evidence: &AcceptedSubmissionEvidence,
    ) -> Result<(), WalletRuntimeError> {
        self.ensure_current()?;
        let mut inner = self.permit.state.lock_inner()?;
        if !self.permit.is_current(&inner)
            || self.permit.state.revocation_is_pending()
            || self.permit.state.revocation_epoch.load(Ordering::Acquire)
                != self.permit.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let result = inner.session.with_seed(|wallet_id, seed| {
            if wallet_id != self.wallet_id {
                return Err(());
            }
            let authenticator = WalletJournalAuthenticator::new(wallet_id, seed).map_err(|_| ())?;
            append_accepted_evidence(journal_path, &authenticator, evidence)
                .map(|_| ())
                .map_err(|_| ())
        });
        if !matches!(result, Ok(Ok(()))) {
            return Err(WalletRuntimeError::ReconciliationUnavailable);
        }
        if !self.permit.is_current(&inner)
            || self.permit.state.revocation_is_pending()
            || self.permit.state.revocation_epoch.load(Ordering::Acquire)
                != self.permit.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        Ok(())
    }

    fn with_reconciliation_auth<T>(
        &self,
        action: impl FnOnce(&ReconciliationAuthenticator) -> Result<T, ReconciliationError>,
    ) -> Result<T, WalletRuntimeError> {
        self.ensure_current()?;
        let mut inner = self.permit.state.lock_inner()?;
        if !self.permit.is_current(&inner)
            || self.permit.state.revocation_is_pending()
            || self.permit.state.revocation_epoch.load(Ordering::Acquire)
                != self.permit.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inner.session.with_seed(|wallet_id, seed| {
                if wallet_id != self.wallet_id {
                    return Err(ReconciliationError::AuthenticationFailed);
                }
                let authenticator = ReconciliationAuthenticator::new(wallet_id, seed)?;
                action(&authenticator)
            })
        }));
        let result = match result {
            Ok(Ok(Ok(result))) => result,
            Ok(Ok(Err(_))) => {
                return Err(WalletRuntimeError::ReconciliationUnavailable);
            }
            Ok(Err(_)) => {
                return Err(WalletRuntimeError::ReconciliationUnavailable);
            }
            Err(payload) => {
                self.permit.state.revoke_current_authority();
                inner.invalidate_all();
                drop(inner);
                std::panic::resume_unwind(payload);
            }
        };
        if !self.permit.is_current(&inner)
            || self.permit.state.revocation_is_pending()
            || self.permit.state.revocation_epoch.load(Ordering::Acquire)
                != self.permit.revocation_epoch
        {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }
        Ok(result)
    }

    pub(in crate::wallet) fn complete<T>(mut self, value: T) -> Result<T, WalletRuntimeError> {
        let result = self.permit.complete(value);
        if result.is_ok() {
            self.armed = false;
        }
        result
    }
}

impl Drop for WalletSubmissionPermit<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.permit.state.invalidate_all()
        })) {
            Ok(Ok(())) => self.armed = false,
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }
}

impl WalletReconciliationPermit<'_> {
    pub(in crate::wallet) fn discover(
        &self,
        store: &ReconciliationStore,
    ) -> Result<Option<RestartReconciliationPermit>, WalletRuntimeError> {
        self.run_fail_closed(|| self.discover_inner(store))
    }

    fn discover_inner(
        &self,
        store: &ReconciliationStore,
    ) -> Result<Option<RestartReconciliationPermit>, WalletRuntimeError> {
        self.ensure_current()?;
        let discovery = ReconciliationDiscoveryPermit::new(&self.permit.activation_proof)?;
        let mut inner = self.permit.state.lock_inner()?;
        let result = inner.session.with_seed(|wallet_id, seed| {
            if wallet_id != self.wallet_id {
                return Err(ReconciliationError::AuthenticationFailed);
            }
            let authenticator = ReconciliationAuthenticator::new(wallet_id, seed)?;
            discovery.discover(store, &authenticator)
        });
        let result = match result {
            Ok(Ok(value)) => value,
            Ok(Err(_)) | Err(_) => {
                return Err(WalletRuntimeError::ReconciliationUnavailable);
            }
        };
        drop(inner);
        self.ensure_current()?;
        Ok(result)
    }

    pub(in crate::wallet) fn resolve_prepared(
        &self,
        store: &ReconciliationStore,
        restart: RestartReconciliationPermit,
    ) -> Result<(), WalletRuntimeError> {
        self.run_fail_closed(|| self.resolve_prepared_inner(store, restart))
    }

    fn resolve_prepared_inner(
        &self,
        store: &ReconciliationStore,
        restart: RestartReconciliationPermit,
    ) -> Result<(), WalletRuntimeError> {
        self.ensure_current()?;
        let mut inner = self.permit.state.lock_inner()?;
        let result = inner.session.with_seed(|wallet_id, seed| {
            if wallet_id != self.wallet_id {
                return Err(ReconciliationError::AuthenticationFailed);
            }
            let authenticator = ReconciliationAuthenticator::new(wallet_id, seed)?;
            restart.resolve_prepared(store, &authenticator)
        });
        if !matches!(result, Ok(Ok(()))) {
            return Err(WalletRuntimeError::ReconciliationUnavailable);
        }
        drop(inner);
        self.ensure_current()
    }

    pub(in crate::wallet) fn complete_accepted_recording(
        &self,
        store: &ReconciliationStore,
        journal_path: &std::path::Path,
        restart: RestartReconciliationPermit,
    ) -> Result<(), WalletRuntimeError> {
        self.run_fail_closed(|| {
            self.complete_accepted_recording_inner(store, journal_path, restart)
        })
    }

    fn complete_accepted_recording_inner(
        &self,
        store: &ReconciliationStore,
        journal_path: &std::path::Path,
        restart: RestartReconciliationPermit,
    ) -> Result<(), WalletRuntimeError> {
        self.ensure_current()?;
        let (accepted, evidence) = restart
            .accepted_evidence()
            .map_err(|_| WalletRuntimeError::ReconciliationUnavailable)?;
        let mut inner = self.permit.state.lock_inner()?;
        let result = inner.session.with_seed(|wallet_id, seed| {
            if wallet_id != self.wallet_id || evidence.wallet_id() != wallet_id {
                return Err(ReconciliationError::AuthenticationFailed);
            }
            let journal_authenticator = WalletJournalAuthenticator::new(wallet_id, seed)
                .map_err(|_| ReconciliationError::StorageUnavailable)?;
            append_accepted_evidence(journal_path, &journal_authenticator, &evidence)
                .map_err(|_| ReconciliationError::StorageUnavailable)?;
            let reconciliation_authenticator = ReconciliationAuthenticator::new(wallet_id, seed)?;
            accepted.resolve_recorded(store, &reconciliation_authenticator)
        });
        if !matches!(result, Ok(Ok(()))) {
            return Err(WalletRuntimeError::ReconciliationUnavailable);
        }
        drop(inner);
        self.ensure_current()
    }

    pub(in crate::wallet) fn reconcile_ambiguous_acceptance(
        &self,
        store: &ReconciliationStore,
        journal_path: &std::path::Path,
        restart: RestartReconciliationPermit,
        source: &impl WalletCoreSubmissionSource,
    ) -> Result<bool, WalletRuntimeError> {
        self.run_fail_closed(|| {
            self.reconcile_ambiguous_acceptance_inner(store, journal_path, restart, source)
        })
    }

    fn reconcile_ambiguous_acceptance_inner(
        &self,
        store: &ReconciliationStore,
        journal_path: &std::path::Path,
        restart: RestartReconciliationPermit,
        source: &impl WalletCoreSubmissionSource,
    ) -> Result<bool, WalletRuntimeError> {
        self.ensure_current()?;
        let (restart, expectation) = restart
            .lookup_expectation()
            .map_err(|_| WalletRuntimeError::ReconciliationUnavailable)?;
        let fingerprint = source
            .validated_identity_fingerprint()
            .map_err(|_| WalletRuntimeError::ReconciliationUnavailable)?;
        if hex::encode(fingerprint) != expectation.original_core_identity_fingerprint_hex() {
            return Err(WalletRuntimeError::ReconciliationUnavailable);
        }
        let body = source
            .transaction_lookup(expectation.transaction_id())
            .map_err(|_| WalletRuntimeError::ReconciliationUnavailable)?;
        let proof = prove_exact_reconciliation_lookup(&body, &expectation)
            .map_err(|_| WalletRuntimeError::ReconciliationUnavailable)?;
        let second_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(|_| WalletRuntimeError::ReconciliationUnavailable)?;
        self.ensure_current()?;
        if second_fingerprint != fingerprint {
            return Err(WalletRuntimeError::ReconciliationUnavailable);
        }
        let Some(proof) = proof else {
            return Ok(false);
        };
        let mut inner = self.permit.state.lock_inner()?;
        let result = inner.session.with_seed(|wallet_id, seed| {
            if wallet_id != self.wallet_id {
                return Err(ReconciliationError::AuthenticationFailed);
            }
            let reconciliation_authenticator = ReconciliationAuthenticator::new(wallet_id, seed)?;
            let accepted = restart.publish_reconciled_acceptance(
                store,
                &reconciliation_authenticator,
                proof,
                compatibility_contract_digest_hex(),
            )?;
            let evidence = accepted.evidence()?;
            let journal_authenticator = WalletJournalAuthenticator::new(wallet_id, seed)
                .map_err(|_| ReconciliationError::StorageUnavailable)?;
            append_accepted_evidence(journal_path, &journal_authenticator, &evidence)
                .map_err(|_| ReconciliationError::StorageUnavailable)?;
            accepted.resolve_recorded(store, &reconciliation_authenticator)
        });
        if !matches!(result, Ok(Ok(()))) {
            return Err(WalletRuntimeError::ReconciliationUnavailable);
        }
        drop(inner);
        self.ensure_current()?;
        Ok(true)
    }

    fn run_fail_closed<T>(
        &self,
        operation: impl FnOnce() -> Result<T, WalletRuntimeError>,
    ) -> Result<T, WalletRuntimeError> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
            Ok(result) => result,
            Err(_) => match self.permit.state.invalidate_all() {
                Ok(()) => Err(WalletRuntimeError::RuntimeUnavailable),
                Err(_) => std::process::abort(),
            },
        }
    }

    fn ensure_current(&self) -> Result<(), WalletRuntimeError> {
        if self.permit.kind != WalletOperationKind::Reconcile
            || !self
                .permit
                .state
                .activation
                .is_satisfied(WalletActivationScope::Reconciliation)
        {
            return Err(WalletRuntimeError::ActivationUnavailable);
        }
        self.permit.ensure_current()
    }

    pub(in crate::wallet) fn complete<T>(mut self, value: T) -> Result<T, WalletRuntimeError> {
        let result = self.permit.complete(value);
        if result.is_ok() {
            self.armed = false;
        }
        result
    }
}

fn compatibility_contract_digest_hex() -> String {
    let mut hasher =
        blake3::Hasher::new_derive_key("com.vision.desktop.wallet-submission-contract.v1");
    hasher.update(SUPPORTED_WALLET_CORE_CONTRACT.as_bytes());
    hasher.finalize().to_hex().to_string()
}

impl Drop for WalletReconciliationPermit<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.permit.state.invalidate_all()
        })) {
            Ok(Ok(())) => self.armed = false,
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }
}

impl Drop for WalletSigningPermit<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.permit.state.invalidate_all()
        })) {
            Ok(Ok(())) => self.armed = false,
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }
}

impl RecoveryPathToken {
    pub(in crate::wallet) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl WalletProcessLock {
    fn acquire(name: &str) -> Result<Self, WalletRuntimeError> {
        platform::acquire(name).map(|platform_lock| Self {
            _platform_lock: platform_lock,
        })
    }
}

fn require_main_window(window_label: &str) -> Result<(), WalletRuntimeError> {
    if window_label == MAIN_WINDOW_LABEL {
        Ok(())
    } else {
        Err(WalletRuntimeError::InvalidWindow)
    }
}

#[cfg(windows)]
mod platform {
    use super::WalletRuntimeError;
    use crate::wallet::storage_security;
    use std::{mem::size_of, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HLOCAL, STATUS_SUCCESS,
        },
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        System::{
            SystemInformation::{
                GetProductInfo, OSVERSIONINFOEXW, OSVERSIONINFOW, PRODUCT_CORE,
                PRODUCT_CORE_COUNTRYSPECIFIC, PRODUCT_CORE_N, PRODUCT_CORE_SINGLELANGUAGE,
                PRODUCT_EDUCATION, PRODUCT_EDUCATION_N, PRODUCT_ENTERPRISE, PRODUCT_ENTERPRISE_N,
                PRODUCT_ENTERPRISE_S, PRODUCT_ENTERPRISE_S_N, PRODUCT_PROFESSIONAL,
                PRODUCT_PROFESSIONAL_N, PRODUCT_PRO_WORKSTATION, PRODUCT_PRO_WORKSTATION_N,
            },
            SystemServices::{
                PRODUCT_PRO_FOR_EDUCATION, PRODUCT_PRO_FOR_EDUCATION_N, VER_NT_WORKSTATION,
            },
            Threading::CreateMutexW,
        },
    };

    // Exact non-evaluation Windows Client edition allowlist. Enterprise E/G, Pro Single Language,
    // Windows SE, Cloud, Server, multi-session, IoT, evaluation, unknown, and future editions are
    // intentionally absent until reviewed.
    const SUPPORTED_WINDOWS_CLIENT_PRODUCTS: &[u32] = &[
        PRODUCT_CORE,
        PRODUCT_CORE_N,
        PRODUCT_CORE_COUNTRYSPECIFIC,
        PRODUCT_CORE_SINGLELANGUAGE,
        PRODUCT_PROFESSIONAL,
        PRODUCT_PROFESSIONAL_N,
        PRODUCT_PRO_WORKSTATION,
        PRODUCT_PRO_WORKSTATION_N,
        PRODUCT_PRO_FOR_EDUCATION,
        PRODUCT_PRO_FOR_EDUCATION_N,
        PRODUCT_ENTERPRISE,
        PRODUCT_ENTERPRISE_N,
        PRODUCT_ENTERPRISE_S,
        PRODUCT_ENTERPRISE_S_N,
        PRODUCT_EDUCATION,
        PRODUCT_EDUCATION_N,
    ];

    // Windows 11 24H2, 25H2, and 26H1 are the only reviewed release families. Cumulative-update
    // revisions do not change `dwBuildNumber`. New base builds remain denied until reviewed.
    const SUPPORTED_WINDOWS_BUILD_FAMILIES: &[u32] = &[26100, 26200, 28000];

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct WindowsHostIdentity {
        major: u32,
        minor: u32,
        build: u32,
        service_pack_major: u16,
        service_pack_minor: u16,
        product_family: u8,
    }

    pub(super) struct ProcessLock(isize);

    pub(super) fn acquire(base_name: &str) -> Result<ProcessLock, WalletRuntimeError> {
        ensure_supported_wallet_host()?;
        let user_sid = storage_security::current_user_sid_string()
            .map_err(|_| WalletRuntimeError::ProcessLockUnavailable)?;
        let name = lock_name(base_name, &user_sid)?;
        let descriptor = SecurityDescriptor::for_current_user(&user_sid)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                .map_err(|_| WalletRuntimeError::ProcessLockUnavailable)?,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let wide: Vec<u16> = std::ffi::OsStr::new(&name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `attributes` references the valid descriptor allocation for this call and the
        // handle is explicitly non-inheritable. The mutex is deliberately not thread-owned: the
        // retained kernel-object name, rather than mutex ownership state, is the process lease.
        let handle = unsafe { CreateMutexW(&attributes, 0, wide.as_ptr()) };
        if handle.is_null() {
            return Err(WalletRuntimeError::ProcessLockUnavailable);
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: this process owns the real handle returned by `CreateMutexW`. Closing the
            // duplicate reference preserves the other process's lease and leaks no handle.
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(WalletRuntimeError::ProcessLockUnavailable);
        }
        Ok(ProcessLock(handle as isize))
    }

    fn ensure_supported_wallet_host() -> Result<(), WalletRuntimeError> {
        let host = actual_windows_host_identity()?;
        let mut product_type = 0;
        // Supplying the actual version prevents `GetProductInfo` from mapping the product to an
        // older Windows product set. The separate build-family check still denies a future Windows
        // release that retains major/minor version 10.0.
        // SAFETY: `product_type` is a valid writable output pointer for the duration of the call.
        let succeeded = unsafe {
            GetProductInfo(
                host.major,
                host.minor,
                u32::from(host.service_pack_major),
                u32::from(host.service_pack_minor),
                &mut product_type,
            )
        };
        if succeeded == 0 || !is_supported_windows_host(host, product_type) {
            return Err(WalletRuntimeError::UnsupportedWindowsHost);
        }
        Ok(())
    }

    fn actual_windows_host_identity() -> Result<WindowsHostIdentity, WalletRuntimeError> {
        let mut version = OSVERSIONINFOEXW {
            dwOSVersionInfoSize: u32::try_from(size_of::<OSVERSIONINFOEXW>())
                .map_err(|_| WalletRuntimeError::UnsupportedWindowsHost)?,
            ..Default::default()
        };
        // SAFETY: `version` has the required size field and is a valid writable structure. The
        // API accepts either OSVERSIONINFOW or its layout-compatible extended form.
        let status = unsafe {
            RtlGetVersion((&mut version as *mut OSVERSIONINFOEXW).cast::<OSVERSIONINFOW>())
        };
        if status != STATUS_SUCCESS {
            return Err(WalletRuntimeError::UnsupportedWindowsHost);
        }
        Ok(WindowsHostIdentity {
            major: version.dwMajorVersion,
            minor: version.dwMinorVersion,
            build: version.dwBuildNumber,
            service_pack_major: version.wServicePackMajor,
            service_pack_minor: version.wServicePackMinor,
            product_family: version.wProductType,
        })
    }

    fn is_supported_windows_host(host: WindowsHostIdentity, product_type: u32) -> bool {
        host.major == 10
            && host.minor == 0
            && host.product_family == VER_NT_WORKSTATION as u8
            && SUPPORTED_WINDOWS_BUILD_FAMILIES.contains(&host.build)
            && is_supported_windows_client_product(product_type)
    }

    fn is_supported_windows_client_product(product_type: u32) -> bool {
        SUPPORTED_WINDOWS_CLIENT_PRODUCTS.contains(&product_type)
    }

    fn lock_name(base_name: &str, user_sid: &str) -> Result<String, WalletRuntimeError> {
        if base_name.is_empty()
            || base_name.len() > 128
            || !base_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            || user_sid.is_empty()
        {
            return Err(WalletRuntimeError::ProcessLockUnavailable);
        }
        let user_scope = blake3::hash(user_sid.as_bytes());
        Ok(format!(
            "Global\\{base_name}.{}",
            hex::encode(user_scope.as_bytes())
        ))
    }

    fn security_descriptor_sddl(user_sid: &str) -> String {
        format!("D:P(A;;GA;;;{user_sid})(A;;GA;;;SY)(A;;GA;;;BA)")
    }

    struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

    impl SecurityDescriptor {
        fn for_current_user(user_sid: &str) -> Result<Self, WalletRuntimeError> {
            let mut sddl: Vec<u16> = security_descriptor_sddl(user_sid).encode_utf16().collect();
            sddl.push(0);
            let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
            // SAFETY: `sddl` is a valid null-terminated SDDL string and `descriptor` is an
            // initialized output pointer. The returned allocation is immediately owned by the
            // guard and released with LocalFree.
            let converted = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            };
            if converted == 0 || descriptor.is_null() {
                return Err(WalletRuntimeError::ProcessLockUnavailable);
            }
            Ok(Self(descriptor))
        }
    }

    impl Drop for SecurityDescriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: SDDL conversion returns an allocation documented for LocalFree.
                unsafe {
                    let _ = LocalFree(self.0 as HLOCAL);
                }
                self.0 = ptr::null_mut();
            }
        }
    }

    #[cfg(test)]
    pub(super) fn lock_name_for_test(
        base_name: &str,
        user_sid: &str,
    ) -> Result<String, WalletRuntimeError> {
        lock_name(base_name, user_sid)
    }

    #[cfg(test)]
    pub(super) fn security_descriptor_sddl_for_test(user_sid: &str) -> String {
        security_descriptor_sddl(user_sid)
    }

    #[cfg(test)]
    pub(super) fn supported_product_for_test(product_type: u32) -> bool {
        is_supported_windows_client_product(product_type)
    }

    #[cfg(test)]
    pub(super) fn supported_products_for_test() -> &'static [u32] {
        SUPPORTED_WINDOWS_CLIENT_PRODUCTS
    }

    #[cfg(test)]
    pub(super) fn supported_builds_for_test() -> &'static [u32] {
        SUPPORTED_WINDOWS_BUILD_FAMILIES
    }

    #[cfg(test)]
    pub(super) fn supported_host_for_test(
        major: u32,
        minor: u32,
        build: u32,
        product_family: u8,
        product_type: u32,
    ) -> bool {
        is_supported_windows_host(
            WindowsHostIdentity {
                major,
                minor,
                build,
                service_pack_major: 0,
                service_pack_minor: 0,
                product_family,
            },
            product_type,
        )
    }

    #[cfg(test)]
    pub(super) fn ensure_supported_host_for_test() -> Result<(), WalletRuntimeError> {
        ensure_supported_wallet_host()
    }

    impl Drop for ProcessLock {
        fn drop(&mut self) {
            let handle = self.0 as *mut std::ffi::c_void;
            if !handle.is_null() {
                // SAFETY: this wrapper owns the kernel-object handle. It is not thread-owned, so
                // closing from any teardown thread releases this process's reference. Windows
                // also closes it automatically after abnormal process termination.
                unsafe {
                    let _ = CloseHandle(handle);
                }
                self.0 = 0;
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::WalletRuntimeError;

    pub(super) struct ProcessLock;

    pub(super) fn acquire(_name: &str) -> Result<ProcessLock, WalletRuntimeError> {
        Err(WalletRuntimeError::ProcessLockUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        activation::{
            lifecycle_activation_requirements_for_test, signing_activation_requirements_for_test,
        },
        secrets::{WalletPassword, WalletSeed},
        vault::EncryptedWalletVault,
    };
    use std::fs;
    #[cfg(windows)]
    use windows_sys::Win32::System::{
        SystemInformation::{
            PRODUCT_CORE, PRODUCT_CORE_COUNTRYSPECIFIC, PRODUCT_CORE_N,
            PRODUCT_CORE_SINGLELANGUAGE, PRODUCT_EDUCATION, PRODUCT_EDUCATION_N,
            PRODUCT_ENTERPRISE, PRODUCT_ENTERPRISE_E, PRODUCT_ENTERPRISE_EVALUATION,
            PRODUCT_ENTERPRISE_N, PRODUCT_ENTERPRISE_N_EVALUATION, PRODUCT_ENTERPRISE_S,
            PRODUCT_ENTERPRISE_S_EVALUATION, PRODUCT_ENTERPRISE_S_N,
            PRODUCT_ENTERPRISE_S_N_EVALUATION, PRODUCT_PROFESSIONAL, PRODUCT_PROFESSIONAL_N,
            PRODUCT_PRO_WORKSTATION, PRODUCT_PRO_WORKSTATION_N, PRODUCT_STANDARD_SERVER,
        },
        SystemServices::{
            PRODUCT_CLOUDEDITION, PRODUCT_CLOUDEDITIONN, PRODUCT_ENTERPRISEG, PRODUCT_ENTERPRISEGN,
            PRODUCT_IOTENTERPRISE, PRODUCT_PRO_FOR_EDUCATION, PRODUCT_PRO_FOR_EDUCATION_N,
            PRODUCT_PRO_SINGLE_LANGUAGE, PRODUCT_SERVERRDSH, VER_NT_SERVER, VER_NT_WORKSTATION,
        },
    };

    #[cfg(windows)]
    #[test]
    fn wallet_host_allowlist_is_exact_and_rejects_evaluation_and_multisession() {
        let expected = [
            PRODUCT_CORE,
            PRODUCT_CORE_N,
            PRODUCT_CORE_COUNTRYSPECIFIC,
            PRODUCT_CORE_SINGLELANGUAGE,
            PRODUCT_PROFESSIONAL,
            PRODUCT_PROFESSIONAL_N,
            PRODUCT_PRO_WORKSTATION,
            PRODUCT_PRO_WORKSTATION_N,
            PRODUCT_PRO_FOR_EDUCATION,
            PRODUCT_PRO_FOR_EDUCATION_N,
            PRODUCT_ENTERPRISE,
            PRODUCT_ENTERPRISE_N,
            PRODUCT_ENTERPRISE_S,
            PRODUCT_ENTERPRISE_S_N,
            PRODUCT_EDUCATION,
            PRODUCT_EDUCATION_N,
        ];
        assert_eq!(platform::supported_products_for_test(), expected);
        for product in expected {
            assert!(platform::supported_product_for_test(product));
        }
        for product in [
            0x00, // Unknown
            PRODUCT_ENTERPRISE_EVALUATION,
            PRODUCT_ENTERPRISE_N_EVALUATION,
            PRODUCT_ENTERPRISE_S_EVALUATION,
            PRODUCT_ENTERPRISE_S_N_EVALUATION,
            PRODUCT_ENTERPRISE_E,
            PRODUCT_ENTERPRISEG,
            PRODUCT_ENTERPRISEGN,
            PRODUCT_PRO_SINGLE_LANGUAGE,
            PRODUCT_CLOUDEDITION,
            PRODUCT_CLOUDEDITIONN,
            PRODUCT_STANDARD_SERVER,
            PRODUCT_SERVERRDSH,
            PRODUCT_IOTENTERPRISE,
            u32::MAX,
        ] {
            assert!(!platform::supported_product_for_test(product));
        }
    }

    #[cfg(windows)]
    #[test]
    fn wallet_host_version_and_family_boundary_rejects_future_back_mapping() {
        assert_eq!(platform::supported_builds_for_test(), [26100, 26200, 28000]);
        for build in platform::supported_builds_for_test() {
            assert!(platform::supported_host_for_test(
                10,
                0,
                *build,
                VER_NT_WORKSTATION as u8,
                PRODUCT_CORE,
            ));
        }

        // Even if a future release were back-mapped to an allowlisted Home code, its unreviewed
        // version/build family and any non-workstation family remain fail closed.
        assert!(!platform::supported_host_for_test(
            11,
            0,
            28000,
            VER_NT_WORKSTATION as u8,
            PRODUCT_CORE,
        ));
        assert!(!platform::supported_host_for_test(
            10,
            0,
            29000,
            VER_NT_WORKSTATION as u8,
            PRODUCT_CORE,
        ));
        assert!(!platform::supported_host_for_test(
            10,
            0,
            26200,
            VER_NT_SERVER as u8,
            PRODUCT_CORE,
        ));
    }

    #[cfg(windows)]
    #[test]
    fn current_windows_host_is_inside_the_wallet_support_boundary() {
        platform::ensure_supported_host_for_test().unwrap();
    }

    #[test]
    fn independent_process_lock_is_exclusive_and_recoverable() {
        let name = format!(
            "com.vision.desktop.wallet-runtime.lock-test.{}",
            std::process::id()
        );
        let first = WalletProcessLock::acquire(&name).unwrap();
        assert_eq!(
            WalletProcessLock::acquire(&name).err(),
            Some(WalletRuntimeError::ProcessLockUnavailable)
        );
        drop(first);
        WalletProcessLock::acquire(&name).unwrap();
    }

    #[test]
    fn global_process_lock_name_is_per_user_and_does_not_disclose_the_sid() {
        let base = "com.vision.desktop.wallet-runtime.name-test";
        let first_sid = "S-1-5-21-100-200-300-400";
        let second_sid = "S-1-5-21-100-200-300-401";
        let first = platform::lock_name_for_test(base, first_sid).unwrap();
        let second = platform::lock_name_for_test(base, second_sid).unwrap();

        assert!(first.starts_with(&format!("Global\\{base}.")));
        assert_ne!(first, second);
        assert!(!first.contains(first_sid));
        assert_eq!(first.len(), "Global\\".len() + base.len() + 1 + 64);
        assert!(platform::lock_name_for_test("Local\\unsafe", first_sid).is_err());
    }

    #[test]
    fn global_process_lock_security_is_restricted_to_user_system_and_admins() {
        let user_sid = "S-1-5-21-100-200-300-400";
        let sddl = platform::security_descriptor_sddl_for_test(user_sid);
        assert!(sddl.starts_with("D:P"));
        assert_eq!(sddl.matches('(').count(), 3);
        assert!(sddl.contains(&format!("(A;;GA;;;{user_sid})")));
        assert!(sddl.contains("(A;;GA;;;SY)"));
        assert!(sddl.contains("(A;;GA;;;BA)"));
        assert!(!sddl.contains(";;;WD"));
        assert!(!sddl.contains(";;;AU"));
    }

    const CHILD_LOCK_BASE_ENV: &str = "VISION_WALLET_LOCK_TEST_BASE";
    const CHILD_LOCK_READY_ENV: &str = "VISION_WALLET_LOCK_TEST_READY";

    #[test]
    fn cross_process_lock_child_helper() {
        let Ok(base) = std::env::var(CHILD_LOCK_BASE_ENV) else {
            return;
        };
        let ready = std::env::var_os(CHILD_LOCK_READY_ENV).unwrap();
        let _lock = WalletProcessLock::acquire(&base).unwrap();
        fs::write(ready, b"ready").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(30));
    }

    #[test]
    fn abnormal_process_termination_releases_cross_process_ownership() {
        use std::{process::Stdio, time::Duration};

        let directory = tempfile::tempdir().unwrap();
        let ready = directory.path().join("ready");
        let base = format!(
            "com.vision.desktop.wallet-runtime.child-test.{}",
            std::process::id()
        );
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "wallet::runtime::tests::cross_process_lock_child_helper",
                "--nocapture",
            ])
            .env(CHILD_LOCK_BASE_ENV, &base)
            .env(CHILD_LOCK_READY_ENV, &ready)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !ready.exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "lock helper exited before acquiring ownership"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "lock helper did not acquire ownership"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            WalletProcessLock::acquire(&base).err(),
            Some(WalletRuntimeError::ProcessLockUnavailable)
        );

        child.kill().unwrap();
        child.wait().unwrap();
        WalletProcessLock::acquire(&base).unwrap();
    }

    /// Manual real-Windows console/RDP and fast-user-switching qualification probe.
    ///
    /// Start the `owner` role in the first Windows session. While it remains alive, start the
    /// `contender` role under the same Windows account in the second session; it must pass by being
    /// denied. After terminating the owner, run `recovery`; it must acquire the same global lease.
    #[test]
    #[ignore = "requires coordinated processes in separate Windows sessions"]
    fn real_windows_cross_session_wallet_ownership() {
        let role = std::env::var("VISION_WALLET_QUALIFICATION_ROLE")
            .expect("set VISION_WALLET_QUALIFICATION_ROLE to owner, contender, or recovery");
        let lock_name = "com.vision.desktop.wallet-runtime.cross-session-qualification.v1";
        match role.as_str() {
            "owner" => {
                let _owner = WalletProcessLock::acquire(lock_name)
                    .expect("qualification owner could not acquire the wallet lease");
                let hold_seconds = std::env::var("VISION_WALLET_QUALIFICATION_HOLD_SECONDS")
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|value| (30..=1_800).contains(value))
                    .unwrap_or(600);
                println!(
                    "VISION_WALLET_QUALIFICATION_OWNER_READY pid={} hold_seconds={hold_seconds}",
                    std::process::id()
                );
                std::thread::sleep(std::time::Duration::from_secs(hold_seconds));
                println!("VISION_WALLET_QUALIFICATION_OWNER_RELEASED");
            }
            "contender" => {
                assert_eq!(
                    WalletProcessLock::acquire(lock_name).err(),
                    Some(WalletRuntimeError::ProcessLockUnavailable)
                );
                println!("VISION_WALLET_QUALIFICATION_CONTENDER_DENIED");
            }
            "recovery" => {
                let _recovered = WalletProcessLock::acquire(lock_name)
                    .expect("wallet lease was not recovered after owner termination");
                println!("VISION_WALLET_QUALIFICATION_OWNERSHIP_RECOVERED");
            }
            _ => panic!("unsupported qualification role"),
        }
    }

    #[test]
    fn operations_are_main_window_owned_and_mutually_exclusive() {
        let runtime = WalletRuntimeState::for_test();
        assert_eq!(
            runtime
                .begin_operation("secondary", WalletOperationKind::Unlock)
                .err(),
            Some(WalletRuntimeError::InvalidWindow)
        );

        let first = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Create)
            .unwrap();
        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Restore)
                .err(),
            Some(WalletRuntimeError::OperationInProgress)
        );
        drop(first);
        runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
    }

    #[test]
    fn lifecycle_requirements_block_lifecycle_signing_and_recovery_authority() {
        for requirement in lifecycle_activation_requirements_for_test() {
            let runtime = Arc::new(WalletRuntimeState::for_test_missing_activation(requirement));
            for kind in [
                WalletOperationKind::Create,
                WalletOperationKind::Restore,
                WalletOperationKind::Unlock,
                WalletOperationKind::PreparePreview,
                WalletOperationKind::ConsumePreview,
                WalletOperationKind::Sign,
            ] {
                assert_eq!(
                    runtime.begin_operation(MAIN_WINDOW_LABEL, kind).err(),
                    Some(WalletRuntimeError::ActivationUnavailable),
                    "missing requirement: {requirement:?}; operation: {kind:?}",
                );
            }
            assert_eq!(
                runtime
                    .begin_recovery_path_selection(
                        MAIN_WINDOW_LABEL,
                        RecoveryPathPurpose::Destination,
                    )
                    .err(),
                Some(WalletRuntimeError::ActivationUnavailable),
                "missing requirement: {requirement:?}",
            );
        }
    }

    #[test]
    fn signing_requirements_block_only_signing_authority() {
        for requirement in signing_activation_requirements_for_test() {
            for kind in [
                WalletOperationKind::Create,
                WalletOperationKind::Restore,
                WalletOperationKind::Unlock,
                WalletOperationKind::PreparePreview,
                WalletOperationKind::ConsumePreview,
            ] {
                let runtime = WalletRuntimeState::for_test_missing_activation(requirement);
                runtime.begin_operation(MAIN_WINDOW_LABEL, kind).unwrap();
            }

            let runtime = WalletRuntimeState::for_test_missing_activation(requirement);
            assert_eq!(
                runtime
                    .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Sign)
                    .err(),
                Some(WalletRuntimeError::ActivationUnavailable),
                "missing requirement: {requirement:?}",
            );

            let runtime = Arc::new(WalletRuntimeState::for_test_missing_activation(requirement));
            runtime
                .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination)
                .unwrap();
        }
    }

    #[test]
    fn signing_submission_and_reconciliation_cannot_start_as_ordinary_operations() {
        let runtime = WalletRuntimeState::for_test();

        for kind in [
            WalletOperationKind::Sign,
            WalletOperationKind::Submit,
            WalletOperationKind::Reconcile,
        ] {
            assert_eq!(
                runtime.begin_operation(MAIN_WINDOW_LABEL, kind).err(),
                Some(WalletRuntimeError::InvalidRequest)
            );
        }
    }

    #[test]
    fn production_activation_policy_issues_no_sensitive_authority() {
        let runtime = Arc::new(WalletRuntimeState::for_test_with_production_activation());

        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Create)
                .err(),
            Some(WalletRuntimeError::ActivationUnavailable),
        );
        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::PreparePreview)
                .err(),
            Some(WalletRuntimeError::ActivationUnavailable),
        );
        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Sign)
                .err(),
            Some(WalletRuntimeError::ActivationUnavailable),
        );
        assert_eq!(
            runtime
                .begin_reconciliation_discovery(MAIN_WINDOW_LABEL)
                .err(),
            Some(WalletRuntimeError::ActivationUnavailable),
        );
        assert_eq!(
            runtime
                .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination,)
                .err(),
            Some(WalletRuntimeError::ActivationUnavailable),
        );
    }

    #[test]
    fn invalidation_revokes_operations_and_stale_permits_cannot_clear_new_work() {
        let runtime = WalletRuntimeState::for_test();
        let stale = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Create)
            .unwrap();
        runtime.invalidate_all().unwrap();
        let current = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
        assert_eq!(
            stale.ensure_current(),
            Err(WalletRuntimeError::RuntimeUnavailable)
        );
        current.ensure_current().unwrap();
        drop(stale);
        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Restore)
                .err(),
            Some(WalletRuntimeError::OperationInProgress)
        );
        drop(current);
        runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Restore)
            .unwrap();
    }

    #[test]
    fn concurrent_revocation_during_authorized_stage_suppresses_its_result() {
        let runtime = WalletRuntimeState::for_test();
        let permit = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Create)
            .unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let stage_completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let result = std::thread::scope(|scope| {
            let stage_completed_for_worker = std::sync::Arc::clone(&stage_completed);
            let worker = scope.spawn(move || {
                permit.run_authorized(|_| -> Result<&'static str, ()> {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    stage_completed_for_worker.store(true, Ordering::Release);
                    Ok("must-not-escape")
                })
            });
            entered_rx.recv().unwrap();
            runtime.invalidate_all().unwrap();
            release_tx.send(()).unwrap();
            worker.join().unwrap()
        });

        assert!(stage_completed.load(Ordering::Acquire));
        assert_eq!(result, Err(WalletRuntimeError::RuntimeUnavailable));
    }

    #[test]
    fn queued_invalidation_advances_epoch_before_waiting_for_runtime_mutex() {
        use std::time::Duration;

        let runtime = WalletRuntimeState::for_test();
        let permit = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
        let operation_epoch = permit.revocation_epoch;
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);

        let (operation_result, invalidation_result) = std::thread::scope(|scope| {
            let runtime_for_worker = &runtime;
            let worker = scope.spawn(move || {
                permit.run_authorized(|_| -> Result<&'static str, ()> {
                    let _busy_runtime = runtime_for_worker.inner.lock().unwrap();
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok("must-not-escape")
                })
            });
            entered_rx.recv().unwrap();
            let runtime_for_invalidator = &runtime;
            let invalidator = scope.spawn(move || runtime_for_invalidator.invalidate_all());

            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while runtime.revocation_epoch.load(Ordering::Acquire) == operation_epoch {
                assert!(
                    std::time::Instant::now() < deadline,
                    "invalidation did not advance the revocation epoch"
                );
                std::thread::yield_now();
            }
            assert!(runtime.revocation_is_pending());
            release_tx.send(()).unwrap();
            (worker.join().unwrap(), invalidator.join().unwrap())
        });

        assert_eq!(
            operation_result,
            Err(WalletRuntimeError::RuntimeUnavailable)
        );
        assert_eq!(invalidation_result, Ok(()));
    }

    #[test]
    fn pending_revocation_rejects_new_authority() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        runtime.pending_revocations.store(1, Ordering::Release);

        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
                .err(),
            Some(WalletRuntimeError::RuntimeUnavailable)
        );
        assert_eq!(
            runtime
                .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination,)
                .err(),
            Some(WalletRuntimeError::RuntimeUnavailable)
        );

        runtime.pending_revocations.store(0, Ordering::Release);
        runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
    }

    #[test]
    fn overlapping_invalidations_keep_authority_closed_until_all_complete() {
        use std::time::Duration;

        let runtime = WalletRuntimeState::for_test();
        let permit = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
        let initial_epoch = permit.revocation_epoch;
        let held_runtime = runtime.inner.lock().unwrap();

        let (first_result, second_result) = std::thread::scope(|scope| {
            let first = scope.spawn(|| runtime.invalidate_all());
            let second = scope.spawn(|| runtime.invalidate_all());
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while runtime.pending_revocations.load(Ordering::Acquire) != 2
                || runtime.revocation_epoch.load(Ordering::Acquire) != initial_epoch + 2
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "overlapping invalidations did not close authority"
                );
                std::thread::yield_now();
            }
            assert_eq!(
                permit.ensure_current(),
                Err(WalletRuntimeError::RuntimeUnavailable)
            );
            drop(held_runtime);
            (first.join().unwrap(), second.join().unwrap())
        });

        assert_eq!(first_result, Ok(()));
        assert_eq!(second_result, Ok(()));
        assert_eq!(runtime.pending_revocations.load(Ordering::Acquire), 0);
        runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
    }

    #[test]
    fn completion_cannot_escape_after_revocation() {
        let runtime = WalletRuntimeState::for_test();
        let permit = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
        runtime.invalidate_all().unwrap();
        assert_eq!(
            permit.complete("must-not-escape"),
            Err(WalletRuntimeError::RuntimeUnavailable)
        );
    }

    #[test]
    fn recovery_path_tokens_are_window_bound_single_use_and_expiring() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let selected = PathBuf::from(r"C:\safe\wallet.vision-recovery.json");
        let permit = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination)
            .unwrap();
        let token = runtime
            .complete_recovery_path_selection_at(
                permit,
                selected.clone(),
                &[0xab; PATH_TOKEN_BYTES],
                100,
            )
            .unwrap();
        assert_eq!(token.as_str().len(), PATH_TOKEN_HEX_BYTES);
        let uppercase = token.as_str().to_ascii_uppercase();
        assert_eq!(
            runtime
                .consume_recovery_path_at(
                    MAIN_WINDOW_LABEL,
                    RecoveryPathPurpose::Destination,
                    uppercase.as_str(),
                    101,
                )
                .unwrap_err(),
            WalletRuntimeError::InvalidRequest
        );
        assert_eq!(
            runtime
                .consume_recovery_path_at(
                    "secondary",
                    RecoveryPathPurpose::Destination,
                    token.as_str(),
                    101,
                )
                .unwrap_err(),
            WalletRuntimeError::InvalidWindow
        );
        assert_eq!(
            runtime
                .consume_recovery_path_at(
                    MAIN_WINDOW_LABEL,
                    RecoveryPathPurpose::Destination,
                    token.as_str(),
                    101,
                )
                .unwrap(),
            selected
        );
        assert_eq!(
            runtime
                .consume_recovery_path_at(
                    MAIN_WINDOW_LABEL,
                    RecoveryPathPurpose::Destination,
                    token.as_str(),
                    102,
                )
                .unwrap_err(),
            WalletRuntimeError::PathAuthorizationInvalid
        );

        let permit = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Source)
            .unwrap();
        let expired = runtime
            .complete_recovery_path_selection_at(
                permit,
                PathBuf::from(r"C:\safe\backup.vision-recovery.json"),
                &[8; PATH_TOKEN_BYTES],
                200,
            )
            .unwrap();
        assert_eq!(
            runtime
                .consume_recovery_path_at(
                    MAIN_WINDOW_LABEL,
                    RecoveryPathPurpose::Source,
                    expired.as_str(),
                    200 + PATH_TOKEN_TTL_MS + 1,
                )
                .unwrap_err(),
            WalletRuntimeError::PathAuthorizationExpired
        );
    }

    #[test]
    fn window_invalidation_revokes_every_path_authorization() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let permit = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Source)
            .unwrap();
        let token = runtime
            .complete_recovery_path_selection_at(
                permit,
                PathBuf::from(r"C:\safe\backup.vision-recovery.json"),
                &[9; PATH_TOKEN_BYTES],
                1,
            )
            .unwrap();
        runtime.invalidate_all().unwrap();
        assert_eq!(
            runtime
                .consume_recovery_path_at(
                    MAIN_WINDOW_LABEL,
                    RecoveryPathPurpose::Source,
                    token.as_str(),
                    2,
                )
                .unwrap_err(),
            WalletRuntimeError::PathAuthorizationInvalid
        );
    }

    #[test]
    fn random_path_token_round_trip_uses_the_monotonic_runtime_clock() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let selected = PathBuf::from(r"C:\safe\generated.vision-recovery.json");
        let permit = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination)
            .unwrap();
        let token = runtime
            .complete_recovery_path_selection(permit, selected.clone())
            .unwrap();

        assert_eq!(
            runtime
                .consume_recovery_path(
                    MAIN_WINDOW_LABEL,
                    RecoveryPathPurpose::Destination,
                    token.as_str(),
                )
                .unwrap(),
            selected
        );
    }

    #[test]
    fn pending_selection_excludes_operations_and_stale_completion_cannot_win() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let stale = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination)
            .unwrap();
        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Create)
                .err(),
            Some(WalletRuntimeError::OperationInProgress)
        );
        assert_eq!(
            runtime
                .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Source)
                .err(),
            Some(WalletRuntimeError::OperationInProgress)
        );

        runtime.invalidate_all().unwrap();
        let mut current = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Source)
            .unwrap();
        assert_eq!(
            runtime
                .complete_recovery_path_selection(
                    stale,
                    PathBuf::from(r"C:\safe\stale.vision-recovery.json"),
                )
                .err(),
            Some(WalletRuntimeError::PathAuthorizationInvalid)
        );
        assert_eq!(
            runtime.cancel_recovery_path_selection(&mut current),
            Err(WalletRuntimeError::PathAuthorizationInvalid),
        );
        drop(current);
        runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Restore)
            .unwrap();
    }

    #[test]
    fn poisoned_runtime_fails_closed_and_clears_authority() {
        let runtime = WalletRuntimeState::for_test();
        let password = WalletPassword::for_test("correct horse battery staple");
        let vault = EncryptedWalletVault::encrypt_for_test(
            "poison_test",
            1,
            &WalletSeed::for_test(42),
            &password,
        )
        .unwrap();
        let permit = runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
        {
            let mut inner = runtime.inner.lock().unwrap();
            inner
                .session
                .unlock(&permit.activation_proof, &vault, &password)
                .unwrap();
            assert!(!inner.session.is_locked());
        }
        std::thread::scope(|scope| {
            assert!(scope
                .spawn(|| {
                    let _guard = runtime.inner.lock().unwrap();
                    panic!("intentional runtime poison test");
                })
                .join()
                .is_err());
        });
        assert_eq!(
            runtime.invalidate_all().unwrap_err(),
            WalletRuntimeError::RuntimeUnavailable
        );
        drop(permit);
        let mut inner = match runtime.inner.lock() {
            Ok(_) => panic!("runtime mutex unexpectedly recovered from poison"),
            Err(poisoned) => poisoned.into_inner(),
        };
        assert!(inner.session.is_locked());
        assert!(inner.active_operation.is_none());
        assert!(inner.pending_path_selection.is_none());
        assert!(inner.path_authorization.is_none());
    }

    #[test]
    fn error_contract_contains_only_fixed_codes_and_messages() {
        let errors = [
            WalletRuntimeError::ProcessLockUnavailable,
            WalletRuntimeError::UnsupportedWindowsHost,
            WalletRuntimeError::RuntimeUnavailable,
            WalletRuntimeError::ActivationUnavailable,
            WalletRuntimeError::InvalidWindow,
            WalletRuntimeError::OperationInProgress,
            WalletRuntimeError::InvalidRequest,
            WalletRuntimeError::SecureRandomUnavailable,
            WalletRuntimeError::PathAuthorizationInvalid,
            WalletRuntimeError::PathAuthorizationExpired,
            WalletRuntimeError::RecoverySelectionCancelled,
            WalletRuntimeError::RecoveryDestinationInvalid,
            WalletRuntimeError::RecoveryDestinationExists,
            WalletRuntimeError::RecoverySourceInvalid,
        ];
        for error in errors {
            assert!(error
                .code()
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_'));
            assert!(!error.to_string().contains('\\'));
            assert!(!error.to_string().contains(':'));
        }
    }
}
