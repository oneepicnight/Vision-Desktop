#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "transaction previews remain private until their later command boundary is approved"
    )
)]
#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "production preview wrappers stay unregistered while private helpers are tested"
    )
)]

use super::{
    amount::{format_vision_amount, parse_vision_amount},
    core_client::{
        WalletCoreClientError, WalletCoreReadClient, WalletCoreReadSource,
        SUPPORTED_WALLET_CORE_CONTRACT,
    },
    public_request::WalletTransferPreviewRequest,
    runtime::{
        WalletOperationKind, WalletRuntimeError, WalletRuntimeState, TRANSACTION_PREVIEW_TTL_MS,
    },
    transaction::{
        build_unsigned_cash_transfer, canonical_transaction_id, CashTransferDraft,
        VisionTransaction, WalletTransactionError,
    },
};
use crate::supervisor::SupervisorState;

const NORMAL_RECOVERY_STATE: &str = "normal";
const REORGANIZATION_WARNING: &str =
    "A mined transaction may be reorganized and is never presented as irreversible.";

/// Complete unsigned intent retained only by the Rust wallet runtime.
///
/// It intentionally implements neither Clone, Debug, nor serialization.
pub(in crate::wallet) struct BoundTransferPreview {
    sender_address: String,
    sender_public_key: String,
    recipient_address: String,
    amount_raw_units: u128,
    charged_fee_raw_units: u64,
    fee_limit_raw_units: u64,
    total_debit_raw_units: u128,
    balance_raw_units: u128,
    nonce: u64,
    transaction_id: String,
    canonical_tip_height: u64,
    canonical_tip_hash: String,
    core_contract: String,
    status_version: String,
    core_identity_fingerprint: [u8; 32],
    unsigned_transaction: VisionTransaction,
}

impl BoundTransferPreview {
    pub(in crate::wallet) fn sender_address(&self) -> &str {
        self.sender_address.as_str()
    }

    pub(in crate::wallet) fn sender_public_key(&self) -> &str {
        self.sender_public_key.as_str()
    }
}

/// Public-only preview data for a future reviewed command boundary.
///
/// It deliberately has no unrestricted Debug implementation while it remains timing-correlated
/// wallet activity. The opaque handle is not signing authority.
pub(in crate::wallet) struct PreparedTransferPreview {
    pub handle: String,
    pub sender_address: String,
    pub recipient_address: String,
    pub amount: String,
    pub amount_raw_units: u128,
    pub charged_fee: String,
    pub charged_fee_raw_units: u64,
    pub maximum_fee: String,
    pub fee_limit_raw_units: u64,
    pub total_debit: String,
    pub total_debit_raw_units: u128,
    pub balance: String,
    pub balance_raw_units: u128,
    pub nonce: u64,
    pub transaction_id: String,
    pub canonical_tip_height: u64,
    pub canonical_tip_hash: String,
    pub core_contract: String,
    pub status_version: String,
    pub data_age_ms: u64,
    pub expires_after_ms: u64,
    pub warning: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletPreviewError {
    InvalidRequest,
    WalletUnavailable,
    OperationInProgress,
    CompatibilityUnavailable,
    CoreUnavailable,
    CoreRejected,
    CoreRecovering,
    AccountUnavailable,
    InsufficientBalance,
    ArithmeticRejected,
    RuntimeUnavailable,
}

impl WalletPreviewError {
    pub(in crate::wallet) const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::WalletUnavailable => "wallet_unavailable",
            Self::OperationInProgress => "wallet_operation_in_progress",
            Self::CompatibilityUnavailable => "wallet_core_compatibility_unavailable",
            Self::CoreUnavailable => "wallet_core_unavailable",
            Self::CoreRejected => "wallet_core_response_rejected",
            Self::CoreRecovering => "wallet_core_recovering",
            Self::AccountUnavailable => "wallet_account_unavailable",
            Self::InsufficientBalance => "insufficient_balance",
            Self::ArithmeticRejected => "wallet_amount_arithmetic_rejected",
            Self::RuntimeUnavailable => "wallet_runtime_unavailable",
        }
    }
}

pub(in crate::wallet) struct WalletTransactionPreviewEngine<'a> {
    runtime: &'a WalletRuntimeState,
}

impl<'a> WalletTransactionPreviewEngine<'a> {
    pub(in crate::wallet) fn new(runtime: &'a WalletRuntimeState) -> Self {
        Self { runtime }
    }

    pub(in crate::wallet) fn prepare(
        &self,
        supervisor: &'a SupervisorState,
        owner_window: &str,
        request: WalletTransferPreviewRequest,
    ) -> Result<PreparedTransferPreview, WalletPreviewError> {
        let permit = self
            .runtime
            .begin_operation(owner_window, WalletOperationKind::PreparePreview)
            .map_err(map_runtime_error)?;
        let client = WalletCoreReadClient::from_supervisor(supervisor).map_err(map_core_error)?;
        prepare_with_source(&permit, request, &client)
    }

    pub(in crate::wallet) fn consume(
        &self,
        owner_window: &str,
        handle: &str,
    ) -> Result<BoundTransferPreview, WalletPreviewError> {
        let permit = self
            .runtime
            .begin_operation(owner_window, WalletOperationKind::ConsumePreview)
            .map_err(map_runtime_error)?;
        permit
            .consume_transaction_preview(handle)
            .map_err(map_runtime_error)
    }

    pub(in crate::wallet) fn cancel(
        &self,
        owner_window: &str,
        handle: &str,
    ) -> Result<(), WalletPreviewError> {
        self.consume(owner_window, handle).map(drop)
    }
}

fn prepare_with_source(
    permit: &super::runtime::WalletOperationPermit<'_>,
    request: WalletTransferPreviewRequest,
    source: &impl WalletCoreReadSource,
) -> Result<PreparedTransferPreview, WalletPreviewError> {
    let account = permit.current_public_account().map_err(map_runtime_error)?;
    let (recipient, amount) = request.into_parts();
    let amount_raw_units =
        parse_vision_amount(amount.as_str()).map_err(|_| WalletPreviewError::InvalidRequest)?;
    if amount_raw_units == 0 {
        return Err(WalletPreviewError::InvalidRequest);
    }
    let recipient_address = recipient.into_string();
    if recipient_address == account.address {
        return Err(WalletPreviewError::InvalidRequest);
    }

    permit.ensure_current().map_err(map_runtime_error)?;
    let core_account = source
        .account_snapshot(&account.address)
        .map_err(map_core_error)?;
    permit.ensure_current().map_err(map_runtime_error)?;
    let status = source.status().map_err(map_core_error)?;
    permit.ensure_current().map_err(map_runtime_error)?;

    if core_account.address != account.address {
        return Err(WalletPreviewError::CoreRejected);
    }
    if !core_account.exists {
        return Err(WalletPreviewError::AccountUnavailable);
    }
    if status.recovery_state != NORMAL_RECOVERY_STATE {
        return Err(WalletPreviewError::CoreRecovering);
    }

    let draft = CashTransferDraft::for_current_nonce(
        core_account.nonce,
        recipient_address.clone(),
        amount_raw_units,
    );
    let charged_fee_raw_units = draft
        .charged_fee_raw_units()
        .map_err(map_transaction_error)?;
    let total_debit_raw_units = amount_raw_units
        .checked_add(u128::from(charged_fee_raw_units))
        .ok_or(WalletPreviewError::ArithmeticRejected)?;
    if core_account.balance < total_debit_raw_units {
        return Err(WalletPreviewError::InsufficientBalance);
    }

    let unsigned_transaction =
        build_unsigned_cash_transfer(account.public_key.clone(), &account.address, &draft)
            .map_err(map_transaction_error)?;
    let transaction_id =
        canonical_transaction_id(&unsigned_transaction).map_err(map_transaction_error)?;
    let amount_display = format_vision_amount(amount_raw_units);
    let charged_fee_display = format_vision_amount(u128::from(charged_fee_raw_units));
    let fee_limit_display = format_vision_amount(u128::from(draft.fee_limit_raw_units()));
    let total_debit_display = format_vision_amount(total_debit_raw_units);
    let balance_display = format_vision_amount(core_account.balance);
    let fee_limit_raw_units = draft.fee_limit_raw_units();
    let status_version = status.version;
    let canonical_tip_height = status.canonical_tip_height;
    let canonical_tip_hash = status.canonical_tip_hash;
    let core_contract = SUPPORTED_WALLET_CORE_CONTRACT.to_string();
    let core_identity_fingerprint = source
        .validated_identity_fingerprint()
        .map_err(map_core_error)?;
    permit.ensure_current().map_err(map_runtime_error)?;

    let intent = BoundTransferPreview {
        sender_address: account.address.clone(),
        sender_public_key: account.public_key,
        recipient_address: recipient_address.clone(),
        amount_raw_units,
        charged_fee_raw_units,
        fee_limit_raw_units,
        total_debit_raw_units,
        balance_raw_units: core_account.balance,
        nonce: core_account.nonce,
        transaction_id: transaction_id.clone(),
        canonical_tip_height,
        canonical_tip_hash: canonical_tip_hash.clone(),
        core_contract: core_contract.clone(),
        status_version: status_version.clone(),
        core_identity_fingerprint,
        unsigned_transaction,
    };
    let receipt = permit
        .complete_transaction_preview(intent)
        .map_err(map_runtime_error)?;
    let (handle, _issued_at_monotonic_ms) = receipt.into_parts();
    Ok(PreparedTransferPreview {
        handle,
        sender_address: account.address,
        recipient_address,
        amount: amount_display,
        amount_raw_units,
        charged_fee: charged_fee_display,
        charged_fee_raw_units,
        maximum_fee: fee_limit_display,
        fee_limit_raw_units,
        total_debit: total_debit_display,
        total_debit_raw_units,
        balance: balance_display,
        balance_raw_units: core_account.balance,
        nonce: core_account.nonce,
        transaction_id,
        canonical_tip_height,
        canonical_tip_hash,
        core_contract,
        status_version,
        data_age_ms: 0,
        expires_after_ms: TRANSACTION_PREVIEW_TTL_MS,
        warning: REORGANIZATION_WARNING.to_string(),
    })
}

fn map_runtime_error(error: WalletRuntimeError) -> WalletPreviewError {
    match error {
        WalletRuntimeError::InvalidWindow | WalletRuntimeError::InvalidRequest => {
            WalletPreviewError::WalletUnavailable
        }
        WalletRuntimeError::OperationInProgress => WalletPreviewError::OperationInProgress,
        WalletRuntimeError::ActivationUnavailable => WalletPreviewError::CompatibilityUnavailable,
        WalletRuntimeError::ProcessLockUnavailable
        | WalletRuntimeError::UnsupportedWindowsHost
        | WalletRuntimeError::RuntimeUnavailable
        | WalletRuntimeError::SecureRandomUnavailable
        | WalletRuntimeError::PathAuthorizationInvalid
        | WalletRuntimeError::PathAuthorizationExpired
        | WalletRuntimeError::RecoverySelectionCancelled
        | WalletRuntimeError::RecoveryDestinationInvalid
        | WalletRuntimeError::RecoveryDestinationExists
        | WalletRuntimeError::RecoverySourceInvalid => WalletPreviewError::RuntimeUnavailable,
    }
}

fn map_core_error(error: WalletCoreClientError) -> WalletPreviewError {
    match error {
        WalletCoreClientError::CompatibilityUnavailable => {
            WalletPreviewError::CompatibilityUnavailable
        }
        WalletCoreClientError::CoreUnavailable
        | WalletCoreClientError::CoreIdentityChanged
        | WalletCoreClientError::PeerIdentityRejected
        | WalletCoreClientError::TransportFailed => WalletPreviewError::CoreUnavailable,
        WalletCoreClientError::InvalidAddress
        | WalletCoreClientError::ResponseRejected
        | WalletCoreClientError::ResponseTooLarge
        | WalletCoreClientError::AccountIdentityMismatch
        | WalletCoreClientError::AccountStateRejected => WalletPreviewError::CoreRejected,
    }
}

fn map_transaction_error(error: WalletTransactionError) -> WalletPreviewError {
    match error {
        WalletTransactionError::FeeArithmeticOverflow => WalletPreviewError::ArithmeticRejected,
        WalletTransactionError::ActivationUnavailable
        | WalletTransactionError::InvalidSender
        | WalletTransactionError::InvalidRecipient
        | WalletTransactionError::ZeroAmount
        | WalletTransactionError::TransferToSelf
        | WalletTransactionError::FeeLimitTooLow
        | WalletTransactionError::FeeExceedsLimit
        | WalletTransactionError::SerializationUnavailable => WalletPreviewError::InvalidRequest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        account::derive_account_identity,
        core_client::{WalletCoreAccountSnapshot, WalletCoreStatus},
        secrets::{WalletPassword, WalletSeed},
        vault::EncryptedWalletVault,
    };

    const MAIN: &str = "main";
    const PASSWORD: &str = "correct horse battery staple";

    struct FakeCore {
        address: String,
        exists: bool,
        balance: u128,
        nonce: u64,
        recovery_state: String,
    }

    impl WalletCoreReadSource for FakeCore {
        fn account_snapshot(
            &self,
            _address: &str,
        ) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError> {
            Ok(WalletCoreAccountSnapshot {
                address: self.address.clone(),
                exists: self.exists,
                balance: self.balance,
                nonce: self.nonce,
            })
        }

        fn status(&self) -> Result<WalletCoreStatus, WalletCoreClientError> {
            Ok(WalletCoreStatus {
                version: "3".to_string(),
                canonical_tip_height: 42,
                canonical_tip_hash: "a".repeat(64),
                peer_count: 2,
                recovery_state: self.recovery_state.clone(),
            })
        }
        fn validated_identity_fingerprint(&self) -> Result<[u8; 32], WalletCoreClientError> {
            Ok([0x42; 32])
        }
    }

    fn unlocked_runtime(seed_byte: u8) -> (WalletRuntimeState, String) {
        let runtime = WalletRuntimeState::for_test();
        let seed = WalletSeed::for_test(seed_byte);
        let identity = derive_account_identity(&seed);
        let password = WalletPassword::for_test(PASSWORD);
        let vault =
            EncryptedWalletVault::encrypt_for_test("primary", 1_700_000_000_000, &seed, &password)
                .unwrap();
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::Unlock)
            .unwrap();
        let status = permit
            .run_authorized(|activation| runtime.unlock_vault(activation, &vault, &password))
            .unwrap()
            .unwrap();
        permit.complete(status).unwrap();
        drop(permit);
        (runtime, identity.address)
    }

    fn request(recipient: &str, amount: &str) -> WalletTransferPreviewRequest {
        serde_json::from_value(serde_json::json!({
            "recipient": recipient,
            "amount": amount
        }))
        .unwrap()
    }

    fn source(address: &str) -> FakeCore {
        FakeCore {
            address: address.to_string(),
            exists: true,
            balance: 10_000_000_000,
            nonce: 7,
            recovery_state: NORMAL_RECOVERY_STATE.to_string(),
        }
    }

    fn prepare(
        runtime: &WalletRuntimeState,
        core: &FakeCore,
        recipient: &str,
        amount: &str,
    ) -> PreparedTransferPreview {
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        prepare_with_source(&permit, request(recipient, amount), core).unwrap()
    }

    #[test]
    fn prepares_complete_exact_public_preview_and_private_unsigned_intent() {
        let (runtime, sender) = unlocked_runtime(7);
        let recipient = "2".repeat(64);
        let preview = prepare(&runtime, &source(&sender), &recipient, "2.5");
        assert_eq!(preview.sender_address, sender);
        assert_eq!(preview.recipient_address, recipient);
        assert_eq!(preview.amount, "2.5");
        assert_eq!(preview.amount_raw_units, 2_500_000_000);
        assert_eq!(preview.charged_fee, "0.000000001");
        assert_eq!(preview.charged_fee_raw_units, 1);
        assert_eq!(preview.maximum_fee, "0.000000201");
        assert_eq!(preview.fee_limit_raw_units, 201);
        assert_eq!(preview.total_debit, "2.500000001");
        assert_eq!(preview.total_debit_raw_units, 2_500_000_001);
        assert_eq!(preview.balance, "10");
        assert_eq!(preview.balance_raw_units, 10_000_000_000);
        assert_eq!(preview.nonce, 7);
        assert_eq!(preview.transaction_id.len(), 64);
        assert_eq!(preview.canonical_tip_height, 42);
        assert_eq!(preview.canonical_tip_hash, "a".repeat(64));
        assert_eq!(preview.core_contract, SUPPORTED_WALLET_CORE_CONTRACT);
        assert_eq!(preview.status_version, "3");
        assert_eq!(preview.expires_after_ms, TRANSACTION_PREVIEW_TTL_MS);
        assert!(preview.warning.contains("reorganized"));

        let intent = WalletTransactionPreviewEngine::new(&runtime)
            .consume(MAIN, &preview.handle)
            .unwrap();
        assert!(intent.unsigned_transaction.sig.is_empty());
        assert_eq!(intent.transaction_id, preview.transaction_id);
        assert_eq!(intent.recipient_address, preview.recipient_address);
        assert_eq!(intent.amount_raw_units, preview.amount_raw_units);
        assert_eq!(intent.charged_fee_raw_units, 1);
        assert_eq!(intent.fee_limit_raw_units, 201);
        assert_eq!(intent.total_debit_raw_units, preview.total_debit_raw_units);
        assert_eq!(intent.balance_raw_units, preview.balance_raw_units);
        assert_eq!(intent.nonce, 7);
        assert_eq!(intent.canonical_tip_height, 42);
        assert_eq!(intent.core_identity_fingerprint, [0x42; 32]);
        assert_eq!(intent.canonical_tip_hash, "a".repeat(64));
        assert_eq!(intent.core_contract, SUPPORTED_WALLET_CORE_CONTRACT);
        assert_eq!(intent.status_version, "3");
    }

    #[test]
    fn preview_handle_is_single_use_and_cancel_consumes_it() {
        let (runtime, sender) = unlocked_runtime(8);
        let preview = prepare(&runtime, &source(&sender), &"3".repeat(64), "1");
        let engine = WalletTransactionPreviewEngine::new(&runtime);
        engine.cancel(MAIN, &preview.handle).unwrap();
        assert_eq!(
            engine.consume(MAIN, &preview.handle).err().unwrap(),
            WalletPreviewError::WalletUnavailable
        );
    }

    #[test]
    fn preview_expires_after_the_short_monotonic_ttl() {
        let (runtime, sender) = unlocked_runtime(18);
        let preview = prepare(&runtime, &source(&sender), &"8".repeat(64), "1");
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::ConsumePreview)
            .unwrap();
        assert_eq!(
            permit
                .consume_transaction_preview_at_for_test(&preview.handle, u64::MAX)
                .err()
                .unwrap(),
            WalletRuntimeError::InvalidRequest
        );
    }

    #[test]
    fn newer_preview_and_runtime_revocation_invalidate_prior_authority() {
        let (runtime, sender) = unlocked_runtime(9);
        let first = prepare(&runtime, &source(&sender), &"4".repeat(64), "1");
        let second = prepare(&runtime, &source(&sender), &"5".repeat(64), "2");
        let engine = WalletTransactionPreviewEngine::new(&runtime);
        assert!(engine.consume(MAIN, &first.handle).is_err());
        assert!(engine.consume(MAIN, &second.handle).is_err());

        let third = prepare(&runtime, &source(&sender), &"6".repeat(64), "3");
        runtime.invalidate_all().unwrap();
        assert!(engine.consume(MAIN, &third.handle).is_err());
    }

    #[test]
    fn rejects_zero_self_transfer_missing_account_recovery_and_insufficient_balance() {
        let (runtime, sender) = unlocked_runtime(10);
        let recipient = "7".repeat(64);
        let normal = source(&sender);
        assert_eq!(
            prepare_with_source(
                &runtime
                    .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                    .unwrap(),
                request(&recipient, "0"),
                &normal,
            )
            .err()
            .unwrap(),
            WalletPreviewError::InvalidRequest
        );
        assert!(prepare_with_source(
            &runtime
                .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                .unwrap(),
            request(&sender, "1"),
            &normal,
        )
        .is_err());

        let mut missing = source(&sender);
        missing.exists = false;
        missing.balance = 0;
        assert_eq!(
            prepare_with_source(
                &runtime
                    .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                    .unwrap(),
                request(&recipient, "1"),
                &missing,
            )
            .err()
            .unwrap(),
            WalletPreviewError::AccountUnavailable
        );

        let mut recovering = source(&sender);
        recovering.recovery_state = "recovering".to_string();
        assert_eq!(
            prepare_with_source(
                &runtime
                    .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                    .unwrap(),
                request(&recipient, "1"),
                &recovering,
            )
            .err()
            .unwrap(),
            WalletPreviewError::CoreRecovering
        );

        let mut poor = source(&sender);
        poor.balance = 1;
        assert_eq!(
            prepare_with_source(
                &runtime
                    .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                    .unwrap(),
                request(&recipient, "1"),
                &poor,
            )
            .err()
            .unwrap(),
            WalletPreviewError::InsufficientBalance
        );
    }

    #[test]
    fn preview_source_contains_no_tauri_signing_submission_or_seed_access() {
        let source = include_str!("preview.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains(&["#[tauri", "::command]"].concat()));
        assert!(!production.contains(&["Wallet", "Seed"].concat()));
        assert!(!production.contains("sign_cash_transfer"));
        assert!(!production.contains(&["PO", "ST "].concat()));
        assert!(!production.contains("submit"));
    }
}
