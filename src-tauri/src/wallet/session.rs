use super::{
    runtime::WalletActivationProof,
    secrets::{WalletPassword, WalletSeed},
    vault::{EncryptedWalletVault, WalletVaultError},
};
use std::{fmt, time::Instant};

const AUTO_LOCK_IDLE_MS: u64 = 5 * 60 * 1000;
const SHORT_BACKOFF_MS: u64 = 5 * 1000;
const MEDIUM_BACKOFF_MS: u64 = 30 * 1000;
const MAX_BACKOFF_MS: u64 = 5 * 60 * 1000;

/// A Rust-only unlocked-wallet session.
///
/// The type intentionally implements neither `Clone`, Serde traits, nor
/// `Debug`. Dropping or replacing an unlocked state drops its zeroizing seed.
pub struct WalletSession {
    started_at: Instant,
    state: SessionState,
    throttle: UnlockThrottle,
}

enum SessionState {
    Locked,
    Unlocked {
        wallet_id: String,
        seed: WalletSeed,
        last_activity_ms: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletSessionError {
    Locked,
    UnlockTemporarilyBlocked { retry_after_ms: u64 },
    InvalidPasswordOrCorruptVault,
    PasswordPolicy,
    VaultUnavailable,
}

impl fmt::Display for WalletSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Locked => "wallet is locked",
            Self::UnlockTemporarilyBlocked { .. } => {
                "wallet unlock is temporarily unavailable after repeated failures"
            }
            Self::InvalidPasswordOrCorruptVault => {
                "wallet password is incorrect or the vault is damaged"
            }
            Self::PasswordPolicy => "wallet password does not meet the local security policy",
            Self::VaultUnavailable => "wallet vault is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for WalletSessionError {}

impl WalletSession {
    pub(in crate::wallet) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            state: SessionState::Locked,
            throttle: UnlockThrottle::default(),
        }
    }

    pub(in crate::wallet) fn unlock(
        &mut self,
        activation: &WalletActivationProof,
        vault: &EncryptedWalletVault,
        password: &WalletPassword,
    ) -> Result<(), WalletSessionError> {
        self.unlock_at_authorized(activation, vault, password, self.now_ms())
    }

    fn unlock_at_authorized(
        &mut self,
        activation: &WalletActivationProof,
        vault: &EncryptedWalletVault,
        password: &WalletPassword,
        now_ms: u64,
    ) -> Result<(), WalletSessionError> {
        // Re-authentication and wallet switching always revoke the currently
        // unlocked seed before any new password attempt is evaluated.
        self.lock();
        self.throttle.check(now_ms)?;
        match vault.unlock(activation, password) {
            Ok(seed) => {
                self.throttle.reset();
                self.state = SessionState::Unlocked {
                    wallet_id: vault.wallet_id().to_string(),
                    seed,
                    last_activity_ms: now_ms,
                };
                Ok(())
            }
            Err(error) => {
                let mapped = map_vault_error(error);
                if mapped == WalletSessionError::InvalidPasswordOrCorruptVault {
                    self.throttle.record_failure(now_ms);
                }
                Err(mapped)
            }
        }
    }

    #[cfg(test)]
    fn unlock_at(
        &mut self,
        vault: &EncryptedWalletVault,
        password: &WalletPassword,
        now_ms: u64,
    ) -> Result<(), WalletSessionError> {
        super::runtime::WalletRuntimeState::with_activation_proof_for_test(
            super::runtime::WalletOperationKind::Unlock,
            |activation| self.unlock_at_authorized(activation, vault, password, now_ms),
        )
    }

    #[cfg(test)]
    pub(in crate::wallet) fn unlock_for_test(
        &mut self,
        vault: &EncryptedWalletVault,
        password: &WalletPassword,
    ) -> Result<(), WalletSessionError> {
        self.unlock_at(vault, password, self.now_ms())
    }

    pub(in crate::wallet) fn lock(&mut self) {
        self.state = SessionState::Locked;
    }

    pub(in crate::wallet) fn is_locked(&mut self) -> bool {
        self.is_locked_at(self.now_ms())
    }

    fn is_locked_at(&mut self, now_ms: u64) -> bool {
        self.enforce_idle_lock(now_ms);
        matches!(self.state, SessionState::Locked)
    }

    /// Executes one narrowly scoped secret operation and refreshes idle time.
    pub(in crate::wallet) fn with_seed<R>(
        &mut self,
        operation: impl FnOnce(&str, &WalletSeed) -> R,
    ) -> Result<R, WalletSessionError> {
        self.with_seed_at(self.now_ms(), operation)
    }

    fn with_seed_at<R>(
        &mut self,
        now_ms: u64,
        operation: impl FnOnce(&str, &WalletSeed) -> R,
    ) -> Result<R, WalletSessionError> {
        self.enforce_idle_lock(now_ms);
        match &mut self.state {
            SessionState::Locked => Err(WalletSessionError::Locked),
            SessionState::Unlocked {
                wallet_id,
                seed,
                last_activity_ms,
            } => {
                *last_activity_ms = now_ms;
                Ok(operation(wallet_id, seed))
            }
        }
    }

    fn enforce_idle_lock(&mut self, now_ms: u64) {
        let should_lock = match &self.state {
            SessionState::Locked => false,
            SessionState::Unlocked {
                last_activity_ms, ..
            } => {
                now_ms < *last_activity_ms
                    || now_ms.saturating_sub(*last_activity_ms) >= AUTO_LOCK_IDLE_MS
            }
        };
        if should_lock {
            self.lock();
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

impl Default for WalletSession {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct UnlockThrottle {
    consecutive_failures: u8,
    blocked_until_ms: u64,
}

impl UnlockThrottle {
    fn check(&self, now_ms: u64) -> Result<(), WalletSessionError> {
        if now_ms < self.blocked_until_ms {
            return Err(WalletSessionError::UnlockTemporarilyBlocked {
                retry_after_ms: self.blocked_until_ms - now_ms,
            });
        }
        Ok(())
    }

    fn record_failure(&mut self, now_ms: u64) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let delay = match self.consecutive_failures {
            0..=2 => 0,
            3 => SHORT_BACKOFF_MS,
            4 => MEDIUM_BACKOFF_MS,
            _ => MAX_BACKOFF_MS,
        };
        self.blocked_until_ms = now_ms.saturating_add(delay);
    }

    fn reset(&mut self) {
        self.consecutive_failures = 0;
        self.blocked_until_ms = 0;
    }
}

fn map_vault_error(error: WalletVaultError) -> WalletSessionError {
    match error {
        WalletVaultError::InvalidPasswordOrCorruptVault => {
            WalletSessionError::InvalidPasswordOrCorruptVault
        }
        WalletVaultError::PasswordPolicy => WalletSessionError::PasswordPolicy,
        WalletVaultError::InvalidWalletId
        | WalletVaultError::InvalidOrUnsupportedFormat
        | WalletVaultError::DeviceProtectionUnavailable
        | WalletVaultError::RandomSourceUnavailable
        | WalletVaultError::StorageUnavailable
        | WalletVaultError::VaultAlreadyExists => WalletSessionError::VaultUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORRECT_PASSWORD: &str = "correct horse battery staple";

    fn password(value: &str) -> WalletPassword {
        WalletPassword::new(value.to_string())
    }

    fn test_vault(wallet_id: &str, seed_byte: u8) -> EncryptedWalletVault {
        EncryptedWalletVault::encrypt_for_test(
            wallet_id,
            1_700_000_000_000,
            &WalletSeed::for_test(seed_byte),
            &password(CORRECT_PASSWORD),
        )
        .unwrap()
    }

    #[test]
    fn session_starts_locked_and_explicit_lock_revokes_access() {
        let vault = test_vault("primary", 7);
        let mut session = WalletSession::new();
        assert!(session.is_locked());

        session
            .unlock_for_test(&vault, &password(CORRECT_PASSWORD))
            .unwrap();
        assert_eq!(
            session
                .with_seed(|wallet_id, seed| {
                    (wallet_id.to_string(), seed.with_exposed(|bytes| bytes[0]))
                })
                .unwrap(),
            ("primary".to_string(), 7)
        );
        assert!(!session.is_locked());
        session.lock();

        assert!(session.is_locked_at(2));
        assert_eq!(
            session.with_seed_at(2, |_, _| ()).unwrap_err(),
            WalletSessionError::Locked
        );
    }

    #[test]
    fn idle_timeout_drops_the_unlocked_seed_before_use() {
        let vault = test_vault("primary", 7);
        let mut session = WalletSession::new();
        session
            .unlock_at(&vault, &password(CORRECT_PASSWORD), 100)
            .unwrap();

        assert_eq!(
            session
                .with_seed_at(100 + AUTO_LOCK_IDLE_MS, |_, _| ())
                .unwrap_err(),
            WalletSessionError::Locked
        );
        assert!(session.is_locked_at(100 + AUTO_LOCK_IDLE_MS));
    }

    #[test]
    fn successful_secret_activity_refreshes_idle_timeout() {
        let vault = test_vault("primary", 7);
        let mut session = WalletSession::new();
        session
            .unlock_at(&vault, &password(CORRECT_PASSWORD), 10)
            .unwrap();

        let first = session
            .with_seed_at(AUTO_LOCK_IDLE_MS - 1, |wallet_id, seed| {
                (wallet_id.to_string(), seed.with_exposed(|bytes| bytes[0]))
            })
            .unwrap();
        assert_eq!(first, ("primary".to_string(), 7));
        assert!(!session.is_locked_at(AUTO_LOCK_IDLE_MS * 2 - 2));
    }

    #[test]
    fn regressing_clock_locks_fail_closed() {
        let vault = test_vault("primary", 7);
        let mut session = WalletSession::new();
        session
            .unlock_at(&vault, &password(CORRECT_PASSWORD), 100)
            .unwrap();

        assert!(session.is_locked_at(99));
    }

    #[test]
    fn repeated_failures_trigger_escalating_backoff() {
        let vault = test_vault("primary", 7);
        let mut session = WalletSession::new();
        let wrong = password("this password is definitely incorrect");

        for now in [1, 2, 3] {
            assert_eq!(
                session.unlock_at(&vault, &wrong, now).unwrap_err(),
                WalletSessionError::InvalidPasswordOrCorruptVault
            );
        }
        assert_eq!(
            session
                .unlock_at(&vault, &password(CORRECT_PASSWORD), 4)
                .unwrap_err(),
            WalletSessionError::UnlockTemporarilyBlocked {
                retry_after_ms: SHORT_BACKOFF_MS - 1
            }
        );

        session
            .unlock_at(&vault, &password(CORRECT_PASSWORD), 3 + SHORT_BACKOFF_MS)
            .unwrap();
        assert!(!session.is_locked_at(3 + SHORT_BACKOFF_MS));
    }

    #[test]
    fn successful_unlock_resets_failure_history() {
        let vault = test_vault("primary", 7);
        let mut session = WalletSession::new();
        let wrong = password("this password is definitely incorrect");
        for now in [1, 2] {
            assert_eq!(
                session.unlock_at(&vault, &wrong, now).unwrap_err(),
                WalletSessionError::InvalidPasswordOrCorruptVault
            );
        }
        session
            .unlock_at(&vault, &password(CORRECT_PASSWORD), 3)
            .unwrap();
        session.lock();

        for now in [4, 5] {
            assert_eq!(
                session.unlock_at(&vault, &wrong, now).unwrap_err(),
                WalletSessionError::InvalidPasswordOrCorruptVault
            );
        }
        session
            .unlock_at(&vault, &password(CORRECT_PASSWORD), 6)
            .unwrap();
    }

    #[test]
    fn unlocking_another_wallet_replaces_the_prior_seed() {
        let first = test_vault("first", 1);
        let second = test_vault("second", 2);
        let mut session = WalletSession::new();
        session
            .unlock_at(&first, &password(CORRECT_PASSWORD), 1)
            .unwrap();
        session
            .unlock_at(&second, &password(CORRECT_PASSWORD), 2)
            .unwrap();

        let observed = session
            .with_seed_at(3, |wallet_id, seed| {
                (wallet_id.to_string(), seed.with_exposed(|bytes| bytes[0]))
            })
            .unwrap();
        assert_eq!(observed, ("second".to_string(), 2));
    }

    #[test]
    fn failed_reauthentication_revokes_the_prior_unlocked_seed() {
        let vault = test_vault("primary", 7);
        let mut session = WalletSession::new();
        session
            .unlock_at(&vault, &password(CORRECT_PASSWORD), 1)
            .unwrap();

        assert_eq!(
            session
                .unlock_at(
                    &vault,
                    &password("this password is definitely incorrect"),
                    2
                )
                .unwrap_err(),
            WalletSessionError::InvalidPasswordOrCorruptVault
        );
        assert!(session.is_locked_at(3));
    }
}
