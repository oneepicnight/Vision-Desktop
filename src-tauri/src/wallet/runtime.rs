#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wallet runtime remains private until custody commands pass review"
    )
)]

use super::{
    account::derive_account_identity,
    activation::WalletActivationPolicy,
    contract::{WalletAccountSummary, WalletLifecycleStatus, WalletPublicMetadata},
    secrets::WalletPassword,
    session::{WalletSession, WalletSessionError},
    vault::EncryptedWalletVault,
};
use std::{
    fmt,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
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

/// Rust-only wallet authority owned by the application process.
///
/// This type intentionally implements neither `Clone`, Serde traits, nor `Debug`. The Tauri
/// application manages exactly one instance, but no wallet command can access it yet.
pub(crate) struct WalletRuntimeState {
    inner: Mutex<WalletRuntimeInner>,
    activation: WalletActivationPolicy,
    _process_lock: WalletProcessLock,
}

struct WalletRuntimeInner {
    started_at: Instant,
    session: WalletSession,
    active_operation: Option<ActiveOperation>,
    pending_path_selection: Option<PendingPathSelection>,
    path_authorization: Option<PathAuthorization>,
    public_account: Option<WalletAccountSummary>,
    next_generation: u64,
}

struct ActiveOperation {
    generation: u64,
    owner_window: String,
    _kind: WalletOperationKind,
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

struct WalletProcessLock {
    _platform_lock: platform::ProcessLock,
}

pub(in crate::wallet) struct WalletOperationPermit<'a> {
    state: &'a WalletRuntimeState,
    generation: u64,
    owner_window: String,
    activation_proof: WalletActivationProof,
}

pub(in crate::wallet) struct WalletActivationProof {
    _private: (),
}

pub(in crate::wallet) struct RecoveryPathToken(Zeroizing<String>);

pub(in crate::wallet) struct RecoverySelectionPermit {
    generation: u64,
    owner_window: String,
    purpose: RecoveryPathPurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletOperationKind {
    Create,
    Restore,
    Unlock,
    Sign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum RecoveryPathPurpose {
    Destination,
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalletRuntimeError {
    ProcessLockUnavailable,
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
}

impl WalletRuntimeError {
    pub(in crate::wallet) const fn code(self) -> &'static str {
        match self {
            Self::ProcessLockUnavailable => "wallet_process_lock_unavailable",
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
        }
    }
}

impl fmt::Display for WalletRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProcessLockUnavailable => "secure wallet process ownership is unavailable",
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
                next_generation: 0,
            }),
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
        self.require_activation()?;
        let mut inner = self.lock_inner()?;
        if inner.active_operation.is_some() || inner.pending_path_selection.is_some() {
            return Err(WalletRuntimeError::OperationInProgress);
        }
        inner.next_generation = inner.next_generation.wrapping_add(1).max(1);
        let generation = inner.next_generation;
        inner.active_operation = Some(ActiveOperation {
            generation,
            owner_window: owner_window.to_string(),
            _kind: kind,
        });
        Ok(WalletOperationPermit {
            state: self,
            generation,
            owner_window: owner_window.to_string(),
            activation_proof: WalletActivationProof { _private: () },
        })
    }

    pub(in crate::wallet) fn begin_recovery_path_selection(
        &self,
        owner_window: &str,
        purpose: RecoveryPathPurpose,
    ) -> Result<RecoverySelectionPermit, WalletRuntimeError> {
        require_main_window(owner_window)?;
        self.require_activation()?;
        let mut inner = self.lock_inner()?;
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
            generation,
            owner_window: owner_window.to_string(),
            purpose,
        })
    }

    pub(in crate::wallet) fn complete_recovery_path_selection(
        &self,
        permit: RecoverySelectionPermit,
        selected_path: PathBuf,
    ) -> Result<RecoveryPathToken, WalletRuntimeError> {
        let mut token_bytes = Zeroizing::new([0_u8; PATH_TOKEN_BYTES]);
        if getrandom::fill(&mut *token_bytes).is_err() {
            let _ = self.cancel_recovery_path_selection(&permit);
            return Err(WalletRuntimeError::SecureRandomUnavailable);
        }
        let now_ms = match self.now_ms() {
            Ok(now_ms) => now_ms,
            Err(error) => {
                let _ = self.cancel_recovery_path_selection(&permit);
                return Err(error);
            }
        };
        self.complete_recovery_path_selection_at(permit, selected_path, &token_bytes, now_ms)
    }

    fn complete_recovery_path_selection_at(
        &self,
        permit: RecoverySelectionPermit,
        selected_path: PathBuf,
        token_bytes: &[u8; PATH_TOKEN_BYTES],
        now_ms: u64,
    ) -> Result<RecoveryPathToken, WalletRuntimeError> {
        if selected_path.as_os_str().is_empty() {
            let _ = self.cancel_recovery_path_selection(&permit);
            return Err(WalletRuntimeError::InvalidRequest);
        }
        let mut inner = self.lock_inner()?;
        if !inner
            .pending_path_selection
            .as_ref()
            .is_some_and(|pending| pending.matches(&permit))
        {
            return Err(WalletRuntimeError::PathAuthorizationInvalid);
        }
        inner.pending_path_selection = None;
        let token = hex::encode(token_bytes);
        inner.path_authorization = Some(PathAuthorization {
            token: Zeroizing::new(token.clone()),
            owner_window: permit.owner_window,
            purpose: permit.purpose,
            selected_path,
            issued_at_ms: now_ms,
        });
        Ok(RecoveryPathToken(Zeroizing::new(token)))
    }

    pub(in crate::wallet) fn cancel_recovery_path_selection(
        &self,
        permit: &RecoverySelectionPermit,
    ) -> Result<(), WalletRuntimeError> {
        let mut inner = self.lock_inner()?;
        if !inner
            .pending_path_selection
            .as_ref()
            .is_some_and(|pending| pending.matches(permit))
        {
            return Err(WalletRuntimeError::PathAuthorizationInvalid);
        }
        inner.pending_path_selection = None;
        inner.path_authorization = None;
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
            || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(WalletRuntimeError::InvalidRequest);
        }
        let mut inner = self.lock_inner()?;
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
        let mut inner = self.lock_inner()?;
        inner.invalidate_all();
        Ok(())
    }

    pub(in crate::wallet) fn remember_public_metadata(
        &self,
        metadata: WalletPublicMetadata,
    ) -> Result<WalletLifecycleStatus, WalletRuntimeError> {
        let mut inner = self.lock_inner()?;
        inner.session.lock();
        inner.public_account = Some(metadata.into());
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
        }
        Ok(inner.lifecycle_status(true))
    }

    fn now_ms(&self) -> Result<u64, WalletRuntimeError> {
        let inner = self.lock_inner()?;
        Ok(u64::try_from(inner.started_at.elapsed().as_millis()).unwrap_or(u64::MAX))
    }

    fn require_activation(&self) -> Result<(), WalletRuntimeError> {
        if self.activation.is_satisfied() {
            Ok(())
        } else {
            Err(WalletRuntimeError::ActivationUnavailable)
        }
    }

    fn lock_inner(&self) -> Result<MutexGuard<'_, WalletRuntimeInner>, WalletRuntimeError> {
        match self.inner.lock() {
            Ok(inner) => Ok(inner),
            Err(poisoned) => {
                let mut inner = poisoned.into_inner();
                inner.invalidate_all();
                Err(WalletRuntimeError::RuntimeUnavailable)
            }
        }
    }

    #[cfg(test)]
    pub(in crate::wallet) fn with_activation_proof_for_test<R>(
        kind: WalletOperationKind,
        operation: impl FnOnce(&WalletActivationProof) -> R,
    ) -> R {
        let runtime = Self::for_test();
        let permit = runtime.begin_operation(MAIN_WINDOW_LABEL, kind).unwrap();
        operation(permit.activation_proof())
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

impl WalletRuntimeInner {
    fn invalidate_all(&mut self) {
        self.session.lock();
        self.active_operation = None;
        self.pending_path_selection = None;
        self.path_authorization = None;
    }

    fn lifecycle_status(&mut self, vault_exists: bool) -> WalletLifecycleStatus {
        WalletLifecycleStatus {
            vault_exists,
            locked: self.session.is_locked(),
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
        match self.inner.get_mut() {
            Ok(inner) => inner.invalidate_all(),
            Err(poisoned) => poisoned.into_inner().invalidate_all(),
        }
    }
}

impl Drop for WalletOperationPermit<'_> {
    fn drop(&mut self) {
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

impl WalletOperationPermit<'_> {
    pub(in crate::wallet) fn activation_proof(&self) -> &WalletActivationProof {
        &self.activation_proof
    }

    /// Proves that no lifecycle event, explicit lock, or newer operation revoked this work.
    pub(in crate::wallet) fn ensure_current(&self) -> Result<(), WalletRuntimeError> {
        let inner = self.state.lock_inner()?;
        if inner.active_operation.as_ref().is_some_and(|operation| {
            operation.generation == self.generation && operation.owner_window == self.owner_window
        }) {
            Ok(())
        } else {
            Err(WalletRuntimeError::RuntimeUnavailable)
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
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HLOCAL},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        System::Threading::CreateMutexW,
    };

    pub(super) struct ProcessLock(isize);

    pub(super) fn acquire(base_name: &str) -> Result<ProcessLock, WalletRuntimeError> {
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
        activation::all_activation_requirements_for_test,
        secrets::{WalletPassword, WalletSeed},
        vault::EncryptedWalletVault,
    };
    use std::fs;

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
    fn every_missing_activation_requirement_blocks_all_sensitive_authority() {
        for requirement in all_activation_requirements_for_test() {
            let runtime = WalletRuntimeState::for_test_missing_activation(requirement);
            for kind in [
                WalletOperationKind::Create,
                WalletOperationKind::Restore,
                WalletOperationKind::Unlock,
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
    fn production_activation_policy_issues_no_sensitive_authority() {
        let runtime = WalletRuntimeState::for_test_with_production_activation();

        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Sign)
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
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Sign)
            .unwrap();
        assert_eq!(
            stale.ensure_current(),
            Err(WalletRuntimeError::RuntimeUnavailable)
        );
        current.ensure_current().unwrap();
        drop(stale);
        assert_eq!(
            runtime
                .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
                .err(),
            Some(WalletRuntimeError::OperationInProgress)
        );
        drop(current);
        runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Unlock)
            .unwrap();
    }

    #[test]
    fn recovery_path_tokens_are_window_bound_single_use_and_expiring() {
        let runtime = WalletRuntimeState::for_test();
        let selected = PathBuf::from(r"C:\safe\wallet.vision-recovery.json");
        let permit = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination)
            .unwrap();
        let token = runtime
            .complete_recovery_path_selection_at(
                permit,
                selected.clone(),
                &[7; PATH_TOKEN_BYTES],
                100,
            )
            .unwrap();
        assert_eq!(token.as_str().len(), PATH_TOKEN_HEX_BYTES);
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
        let runtime = WalletRuntimeState::for_test();
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
        let runtime = WalletRuntimeState::for_test();
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
        let runtime = WalletRuntimeState::for_test();
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
        let current = runtime
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
        runtime.cancel_recovery_path_selection(&current).unwrap();
        runtime
            .begin_operation(MAIN_WINDOW_LABEL, WalletOperationKind::Restore)
            .unwrap();
    }

    #[test]
    fn poisoned_runtime_fails_closed_and_clears_authority() {
        let runtime = WalletRuntimeState::for_test();
        let password = WalletPassword::new("correct horse battery staple".to_string());
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
                .unlock(permit.activation_proof(), &vault, &password)
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
