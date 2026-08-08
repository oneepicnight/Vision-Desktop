#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "private signing and submission remain unregistered pending exact review"
    )
)]

use super::{
    core_client::{
        WalletCoreClientError, WalletCoreReadSource, WalletCoreSubmissionSource,
        SUPPORTED_STATUS_VERSION, SUPPORTED_WALLET_CORE_CONTRACT,
    },
    lifecycle::WalletCustodyPathAuthority,
    preview::{BoundTransferPreview, PendingTransferConfirmation, WalletPreviewError},
    runtime::{WalletRuntimeError, WalletSigningPermit},
    transaction::{
        TransactionSigningObserver, TransactionSigningStage, VisionTransaction,
        WalletTransactionError,
    },
    transaction_confirmation::NativeConfirmationApproval,
};

mod submission_coordinator;

/// Fixed, non-emitting failure categories for the private confirmation-to-signing bridge.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletPrivateSigningError {
    PreviewUnavailable,
    RuntimeRevoked,
    ActivationUnavailable,
    CoreUnavailable,
    IntentRejected,
    SignatureUnavailable,
    SubmissionUnavailable,
}

/// Single-owner result reserved for a separately reviewed submission tranche.
///
/// This type intentionally implements neither Clone, Debug, Display, nor serialization. In this
/// tranche it is always destroyed inside this module and no signed bytes escape.
struct SignedTransferArtifact {
    transaction: VisionTransaction,
    transaction_id: String,
    wallet_id: String,
    core_identity_fingerprint: [u8; 32],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum SigningCoordinatorStage {
    Promoted,
    BeforeSeedAccess,
    SeedAccountDerivation,
    SignatureConstruction,
    SignatureVerification,
    AfterSignatureVerification,
    BeforeCompletion,
}

pub(in crate::wallet) trait SigningCoordinatorObserver {
    fn checkpoint(&self, stage: SigningCoordinatorStage);
}

struct NoopSigningCoordinatorObserver;

impl SigningCoordinatorObserver for NoopSigningCoordinatorObserver {
    fn checkpoint(&self, _stage: SigningCoordinatorStage) {}
}

struct TransactionObserverAdapter<'a>(&'a dyn SigningCoordinatorObserver);

impl TransactionSigningObserver for TransactionObserverAdapter<'_> {
    fn checkpoint(&self, stage: TransactionSigningStage) {
        let stage = match stage {
            TransactionSigningStage::SeedAccountDerivation => {
                SigningCoordinatorStage::SeedAccountDerivation
            }
            TransactionSigningStage::SignatureConstruction => {
                SigningCoordinatorStage::SignatureConstruction
            }
            TransactionSigningStage::SignatureVerification => {
                SigningCoordinatorStage::SignatureVerification
            }
        };
        self.0.checkpoint(stage);
    }
}

pub(in crate::wallet) fn sign_after_native_approval<S: WalletCoreReadSource>(
    pending: PendingTransferConfirmation<'_, S>,
    approval: NativeConfirmationApproval,
) -> Result<(), WalletPrivateSigningError> {
    sign_after_native_approval_with_observer(pending, approval, &NoopSigningCoordinatorObserver)
}

pub(in crate::wallet) fn sign_and_submit_after_native_approval<S: WalletCoreSubmissionSource>(
    pending: PendingTransferConfirmation<'_, S>,
    approval: NativeConfirmationApproval,
    custody: &WalletCustodyPathAuthority,
    created_at_unix_ms: u64,
    rejection_policy: &crate::wallet::submission::SubmissionRejectionPolicy,
) -> Result<submission_coordinator::PrivateSubmissionResult, WalletPrivateSigningError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sign_and_submit_after_native_approval_inner(
            pending,
            approval,
            custody,
            created_at_unix_ms,
            rejection_policy,
        )
    })) {
        Ok(result) => result,
        Err(_) => Err(WalletPrivateSigningError::RuntimeRevoked),
    }
}

fn sign_and_submit_after_native_approval_inner<S: WalletCoreSubmissionSource>(
    pending: PendingTransferConfirmation<'_, S>,
    approval: NativeConfirmationApproval,
    custody: &WalletCustodyPathAuthority,
    created_at_unix_ms: u64,
    rejection_policy: &crate::wallet::submission::SubmissionRejectionPolicy,
) -> Result<submission_coordinator::PrivateSubmissionResult, WalletPrivateSigningError> {
    let (permit, source, intent) = pending
        .promote_with_native_approval(approval)
        .map_err(map_preview_error)?;
    validate_core_and_runtime(&permit, &source, &intent)?;
    let signed = permit
        .sign_confirmed_intent_with_observer(
            &intent,
            SUPPORTED_WALLET_CORE_CONTRACT,
            SUPPORTED_STATUS_VERSION,
            &TransactionObserverAdapter(&NoopSigningCoordinatorObserver),
        )
        .map_err(map_runtime_error)?
        .map_err(map_transaction_error)?;
    validate_core_and_runtime(&permit, &source, &intent)?;
    let artifact = SignedTransferArtifact {
        transaction: signed,
        transaction_id: intent.confirmation_fields().transaction_id.to_owned(),
        wallet_id: permit.wallet_id().to_owned(),
        core_identity_fingerprint: *intent.core_identity_fingerprint(),
    };
    let submission_permit = permit
        .promote_to_submission(artifact.core_identity_fingerprint)
        .map_err(map_runtime_error)?;
    submission_coordinator::submit_signed_artifact(
        submission_permit,
        artifact,
        &source,
        custody,
        created_at_unix_ms,
        rejection_policy,
    )
    .map_err(|error| match error {
        submission_coordinator::PrivateSubmissionError::RuntimeRevoked => {
            WalletPrivateSigningError::RuntimeRevoked
        }
        submission_coordinator::PrivateSubmissionError::ActivationUnavailable => {
            WalletPrivateSigningError::ActivationUnavailable
        }
        submission_coordinator::PrivateSubmissionError::IntentRejected => {
            WalletPrivateSigningError::IntentRejected
        }
        submission_coordinator::PrivateSubmissionError::ReconciliationUnavailable => {
            WalletPrivateSigningError::SubmissionUnavailable
        }
    })
}

fn sign_after_native_approval_with_observer<S: WalletCoreReadSource>(
    pending: PendingTransferConfirmation<'_, S>,
    approval: NativeConfirmationApproval,
    observer: &dyn SigningCoordinatorObserver,
) -> Result<(), WalletPrivateSigningError> {
    let (permit, source, intent) = pending
        .promote_with_native_approval(approval)
        .map_err(map_preview_error)?;
    observer.checkpoint(SigningCoordinatorStage::Promoted);

    validate_core_and_runtime(&permit, &source, &intent)?;
    observer.checkpoint(SigningCoordinatorStage::BeforeSeedAccess);
    let signed = permit
        .sign_confirmed_intent_with_observer(
            &intent,
            SUPPORTED_WALLET_CORE_CONTRACT,
            SUPPORTED_STATUS_VERSION,
            &TransactionObserverAdapter(observer),
        )
        .map_err(map_runtime_error)?
        .map_err(map_transaction_error)?;

    observer.checkpoint(SigningCoordinatorStage::AfterSignatureVerification);
    validate_core_and_runtime(&permit, &source, &intent)?;
    let artifact = SignedTransferArtifact {
        transaction: signed,
        transaction_id: intent.confirmation_fields().transaction_id.to_owned(),
        wallet_id: permit.wallet_id().to_owned(),
        core_identity_fingerprint: *intent.core_identity_fingerprint(),
    };

    observer.checkpoint(SigningCoordinatorStage::BeforeCompletion);
    validate_core_and_runtime(&permit, &source, &intent)?;
    let artifact = permit.complete(artifact).map_err(map_runtime_error)?;
    drop(artifact);
    Ok(())
}

#[cfg(test)]
pub(in crate::wallet) fn sign_after_native_approval_with_observer_for_test<
    S: WalletCoreReadSource,
>(
    pending: PendingTransferConfirmation<'_, S>,
    approval: NativeConfirmationApproval,
    observer: &dyn SigningCoordinatorObserver,
) -> Result<(), WalletPrivateSigningError> {
    sign_after_native_approval_with_observer(pending, approval, observer)
}

fn validate_core_and_runtime(
    permit: &WalletSigningPermit<'_>,
    source: &impl WalletCoreReadSource,
    intent: &BoundTransferPreview,
) -> Result<(), WalletPrivateSigningError> {
    permit.ensure_current().map_err(map_runtime_error)?;
    let fingerprint = source
        .validated_identity_fingerprint()
        .map_err(map_core_error)?;
    permit.ensure_current().map_err(map_runtime_error)?;
    if !intent.matches_core_identity(&fingerprint) {
        return Err(WalletPrivateSigningError::CoreUnavailable);
    }
    Ok(())
}

const fn map_preview_error(error: WalletPreviewError) -> WalletPrivateSigningError {
    match error {
        WalletPreviewError::CompatibilityUnavailable | WalletPreviewError::CoreUnavailable => {
            WalletPrivateSigningError::CoreUnavailable
        }
        WalletPreviewError::RuntimeUnavailable => WalletPrivateSigningError::RuntimeRevoked,
        WalletPreviewError::OperationInProgress => WalletPrivateSigningError::PreviewUnavailable,
        WalletPreviewError::InvalidRequest
        | WalletPreviewError::WalletUnavailable
        | WalletPreviewError::CoreRejected
        | WalletPreviewError::CoreRecovering
        | WalletPreviewError::AccountUnavailable
        | WalletPreviewError::InsufficientBalance
        | WalletPreviewError::ArithmeticRejected => WalletPrivateSigningError::PreviewUnavailable,
    }
}

const fn map_runtime_error(error: WalletRuntimeError) -> WalletPrivateSigningError {
    match error {
        WalletRuntimeError::ActivationUnavailable => {
            WalletPrivateSigningError::ActivationUnavailable
        }
        WalletRuntimeError::RuntimeUnavailable => WalletPrivateSigningError::RuntimeRevoked,
        WalletRuntimeError::ProcessLockUnavailable
        | WalletRuntimeError::UnsupportedWindowsHost
        | WalletRuntimeError::InvalidWindow
        | WalletRuntimeError::OperationInProgress
        | WalletRuntimeError::InvalidRequest
        | WalletRuntimeError::SecureRandomUnavailable
        | WalletRuntimeError::PathAuthorizationInvalid
        | WalletRuntimeError::PathAuthorizationExpired
        | WalletRuntimeError::RecoverySelectionCancelled
        | WalletRuntimeError::RecoveryDestinationInvalid
        | WalletRuntimeError::RecoveryDestinationExists
        | WalletRuntimeError::RecoverySourceInvalid
        | WalletRuntimeError::ReconciliationUnavailable => {
            WalletPrivateSigningError::RuntimeRevoked
        }
    }
}

const fn map_core_error(_error: WalletCoreClientError) -> WalletPrivateSigningError {
    WalletPrivateSigningError::CoreUnavailable
}

const fn map_transaction_error(error: WalletTransactionError) -> WalletPrivateSigningError {
    match error {
        WalletTransactionError::ActivationUnavailable => {
            WalletPrivateSigningError::ActivationUnavailable
        }
        WalletTransactionError::InvalidSender
        | WalletTransactionError::ConfirmedIntentMismatch
        | WalletTransactionError::InvalidRecipient
        | WalletTransactionError::TransferToSelf
        | WalletTransactionError::ZeroAmount
        | WalletTransactionError::FeeLimitTooLow
        | WalletTransactionError::FeeArithmeticOverflow
        | WalletTransactionError::FeeExceedsLimit
        | WalletTransactionError::SerializationUnavailable => {
            WalletPrivateSigningError::IntentRejected
        }
        WalletTransactionError::SignatureUnavailable
        | WalletTransactionError::SignatureVerificationFailed => {
            WalletPrivateSigningError::SignatureUnavailable
        }
    }
}

#[cfg(test)]
std::thread_local! {
    static SIGNED_ARTIFACT_DROPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Drop for SignedTransferArtifact {
    fn drop(&mut self) {
        SIGNED_ARTIFACT_DROPS.with(|count| count.set(count.get() + 1));
    }
}

#[cfg(test)]
pub(in crate::wallet) fn reset_signed_artifact_drop_count_for_test() {
    SIGNED_ARTIFACT_DROPS.with(|count| count.set(0));
}

#[cfg(test)]
pub(in crate::wallet) fn signed_artifact_drop_count_for_test() -> usize {
    SIGNED_ARTIFACT_DROPS.with(std::cell::Cell::get)
}
