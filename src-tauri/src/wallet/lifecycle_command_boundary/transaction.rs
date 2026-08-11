//! Private transaction-shaped companion to the reviewed lifecycle command boundary.
//!
//! This module deliberately has no Tauri command attributes, invoke registration, managed state,
//! permission, capability, frontend caller, or production activation shortcut.

use super::{
    fixed_invoke_error, BoundaryFailClosedGuard, MainWalletWindowAuthority,
    WalletExposureAuthority, WalletInvokeRequest, WholeEnvelopeTransportPolicy,
};
use crate::{
    supervisor::SupervisorState,
    wallet::{
        core_client::WalletCoreReadClient,
        journal::{WalletActivityJournal, WalletActivityRecord},
        lifecycle::WalletLifecycleAdapters,
        preview::{PreparedTransferPreview, WalletPreviewError, WalletTransactionPreviewEngine},
        public_request::WalletTransferPreviewRequest,
        receipt::{
            receipt_presentation, WalletReceiptConfidence, WalletReceiptObservation,
            WalletReceiptPresentation,
        },
        reconciliation::ReconciliationPhaseTag,
        runtime::{WalletReconciliationResult, WalletRuntimeError, WalletRuntimeState},
        signing::PrivateSubmissionResult,
        transaction_confirmation::{
            NativeTransactionConfirmationCeremony, WalletConfirmationError,
            WalletTransactionConfirmationEngine,
        },
    },
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    ipc::{InvokeBody, InvokeError, Response},
    Runtime, WebviewWindow,
};

const PREPARE_TRANSFER_PREVIEW: &str = "wallet_prepare_transfer_preview";
const CANCEL_TRANSFER_PREVIEW: &str = "wallet_cancel_transfer_preview";
const CONFIRM_AND_SUBMIT_TRANSFER: &str = "wallet_confirm_and_submit_transfer";
const LIST_ACTIVITY: &str = "wallet_list_activity";
const REFRESH_TRANSACTION_OBSERVATION: &str = "wallet_refresh_transaction_observation";

const MAX_PREVIEW_HANDLE_BYTES: usize = 128;
const TRANSACTION_ID_BYTES: usize = 64;
const MAX_ACTIVITY_RECORDS: usize = 100;

/// Private, unregistered transaction boundary sharing the lifecycle authority model.
///
/// It implements neither `Clone`, `Debug`, formatting, nor serialization.
pub(in crate::wallet) struct WalletTransactionCommandBoundary {
    runtime: Arc<WalletRuntimeState>,
    adapters: Arc<WalletLifecycleAdapters>,
    supervisor: Arc<SupervisorState>,
    expected_main_hwnd: isize,
    transport_policy: WholeEnvelopeTransportPolicy,
    operation_gate: Mutex<()>,
}

enum WalletTransactionEnvelope {
    Prepare(WalletTransferPreviewRequest),
    Cancel(PreviewHandleRequest),
    ConfirmAndSubmit(PreviewHandleRequest),
    ListActivity,
    Refresh(TransactionIdRequest),
}

struct PreviewHandleRequest {
    preview_handle: String,
}

struct TransactionIdRequest {
    transaction_id: String,
}

enum TransactionBoundaryError {
    InvalidRequest,
    InvalidWindow,
    ActivationUnavailable,
    Runtime(WalletRuntimeError),
    Preview(WalletPreviewError),
    Confirmation(WalletConfirmationError),
    ReconciliationPending,
    ActivityUnavailable,
    TransactionUnknown,
    SerializationUnavailable,
}

#[derive(Serialize)]
struct PreviewResponse<'a> {
    preview_handle: &'a str,
    sender: &'a str,
    recipient: &'a str,
    amount: &'a str,
    amount_raw_units: String,
    charged_fee: &'a str,
    charged_fee_raw_units: String,
    maximum_fee: &'a str,
    fee_limit_raw_units: String,
    total_debit: &'a str,
    total_debit_raw_units: String,
    balance: &'a str,
    balance_raw_units: String,
    nonce: String,
    transaction_id: &'a str,
    canonical_tip_height: String,
    canonical_tip_hash: &'a str,
    core_contract: &'a str,
    status_version: &'a str,
    data_age_ms: String,
    expires_after_ms: String,
    warning: &'a str,
}

#[derive(Serialize)]
struct CancelResponse {
    state: &'static str,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum SubmissionResponse {
    Accepted {
        transaction_id: String,
        observation: PublicObservation,
    },
    AcceptedRecordingPending {
        transaction_id: String,
    },
    OutcomeUnknown {
        transaction_id: String,
    },
    Rejected {
        transaction_id: String,
    },
}

#[derive(Serialize)]
struct ActivityResponse {
    records: Vec<PublicActivityRecord>,
    history_complete: bool,
    pending_reconciliation: Option<PendingReconciliationResponse>,
}

#[derive(Serialize)]
struct PublicActivityRecord {
    transaction_id: String,
    sender: String,
    recipient: String,
    amount_raw_units: String,
    nonce: String,
    tip_raw_units: String,
    fee_limit_raw_units: String,
    submitted_at_unix_ms: String,
    last_observed_at_unix_ms: Option<String>,
    observation: PublicObservation,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum PendingReconciliationResponse {
    AcceptedRecordingPending { transaction_id: String },
    OutcomeUnknown { transaction_id: String },
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum PublicObservation {
    NotObserved,
    Pending,
    MinedObserved { confirmations: String },
    MinedHighConfidence { confirmations: String },
}

struct ActivityDiscovery {
    journal: Option<WalletActivityJournal>,
    pending: Option<PendingReconciliationResponse>,
}

struct TransactionExecution {
    response: Response,
    durable_submission_outcome: bool,
}

impl WalletTransactionCommandBoundary {
    pub(in crate::wallet) fn new_private(
        runtime: Arc<WalletRuntimeState>,
        adapters: Arc<WalletLifecycleAdapters>,
        supervisor: Arc<SupervisorState>,
        expected_main_hwnd: isize,
    ) -> Self {
        Self {
            runtime,
            adapters,
            supervisor,
            expected_main_hwnd,
            transport_policy: WholeEnvelopeTransportPolicy::production(),
            operation_gate: Mutex::new(()),
        }
    }

    #[cfg(test)]
    fn for_test(
        runtime: Arc<WalletRuntimeState>,
        adapters: Arc<WalletLifecycleAdapters>,
        supervisor: Arc<SupervisorState>,
        expected_main_hwnd: isize,
    ) -> Self {
        Self {
            runtime,
            adapters,
            supervisor,
            expected_main_hwnd,
            transport_policy: WholeEnvelopeTransportPolicy::approved_for_test(),
            operation_gate: Mutex::new(()),
        }
    }

    pub(in crate::wallet) fn execute<R: Runtime>(
        &self,
        request: WalletInvokeRequest<'_>,
        window: &WebviewWindow<R>,
    ) -> Result<Response, InvokeError> {
        let mut guard = BoundaryFailClosedGuard::arm(&self.runtime);
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            let result = (|| {
                let envelope = parse_transaction_envelope(request)?;
                let exposure =
                    WalletExposureAuthority::issue(&self.runtime, &self.transport_policy)
                        .map_err(map_shared_error)?;
                let window = MainWalletWindowAuthority::issue(
                    window,
                    self.expected_main_hwnd,
                    &exposure,
                    &self.runtime,
                )
                .map_err(map_shared_error)?;
                let _linear = self.acquire_operation_gate()?;
                exposure.validate(&self.runtime).map_err(map_shared_error)?;
                window
                    .validate(self.expected_main_hwnd, &self.runtime)
                    .map_err(map_shared_error)?;
                let execution = self.execute_envelope(envelope, &window)?;
                if !execution.durable_submission_outcome {
                    window
                        .validate(self.expected_main_hwnd, &self.runtime)
                        .map_err(map_shared_error)?;
                    exposure.validate(&self.runtime).map_err(map_shared_error)?;
                }
                Ok(execution.response)
            })();
            result.map_err(|error: TransactionBoundaryError| fixed_invoke_error(error.code()))
        }));

        match attempt {
            Ok(Ok(response)) => {
                guard.commit();
                Ok(response)
            }
            Ok(Err(error)) => {
                guard.invalidate_or_terminate();
                Err(error)
            }
            Err(_) => {
                guard.invalidate_or_terminate();
                Err(fixed_invoke_error("wallet_runtime_unavailable"))
            }
        }
    }

    fn acquire_operation_gate(&self) -> Result<MutexGuard<'_, ()>, TransactionBoundaryError> {
        match self.operation_gate.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::WouldBlock) => Err(TransactionBoundaryError::Runtime(
                WalletRuntimeError::OperationInProgress,
            )),
            Err(TryLockError::Poisoned(_)) => Err(TransactionBoundaryError::Runtime(
                WalletRuntimeError::RuntimeUnavailable,
            )),
        }
    }

    fn execute_envelope(
        &self,
        envelope: WalletTransactionEnvelope,
        window: &MainWalletWindowAuthority,
    ) -> Result<TransactionExecution, TransactionBoundaryError> {
        match envelope {
            WalletTransactionEnvelope::Prepare(request) => self
                .prepare_transfer_preview(window, request)
                .map(ordinary_execution),
            WalletTransactionEnvelope::Cancel(request) => self
                .cancel_transfer_preview(window, request)
                .map(ordinary_execution),
            WalletTransactionEnvelope::ConfirmAndSubmit(request) => self
                .confirm_and_submit_transfer(window, request)
                .map(|response| TransactionExecution {
                    response,
                    durable_submission_outcome: true,
                }),
            WalletTransactionEnvelope::ListActivity => {
                self.list_activity(window).map(ordinary_execution)
            }
            WalletTransactionEnvelope::Refresh(request) => self
                .refresh_transaction_observation(window, request)
                .map(ordinary_execution),
        }
    }

    fn prepare_transfer_preview(
        &self,
        window: &MainWalletWindowAuthority,
        request: WalletTransferPreviewRequest,
    ) -> Result<Response, TransactionBoundaryError> {
        self.ensure_spending_unblocked(window.owner_label())?;
        let preview = WalletTransactionPreviewEngine::new(&self.runtime)
            .prepare(&self.supervisor, window.owner_label(), request)
            .map_err(TransactionBoundaryError::Preview)?;
        serialize_response(&project_preview(&preview))
    }

    fn cancel_transfer_preview(
        &self,
        window: &MainWalletWindowAuthority,
        request: PreviewHandleRequest,
    ) -> Result<Response, TransactionBoundaryError> {
        WalletTransactionPreviewEngine::new(&self.runtime)
            .cancel(window.owner_label(), request.preview_handle.as_str())
            .map_err(TransactionBoundaryError::Preview)?;
        serialize_response(&CancelResponse { state: "cancelled" })
    }

    fn confirm_and_submit_transfer(
        &self,
        window: &MainWalletWindowAuthority,
        request: PreviewHandleRequest,
    ) -> Result<Response, TransactionBoundaryError> {
        self.ensure_spending_unblocked(window.owner_label())?;
        let ceremony = NativeTransactionConfirmationCeremony::new(window.hwnd).map_err(|_| {
            TransactionBoundaryError::Confirmation(WalletConfirmationError::NativeUiUnavailable)
        })?;
        let result = WalletTransactionConfirmationEngine::new(&self.runtime, &ceremony)
            .confirm_and_submit(
                &self.supervisor,
                window.owner_label(),
                request.preview_handle.as_str(),
                self.adapters.custody_path_authority(),
                now_unix_ms()?,
            )
            .map_err(TransactionBoundaryError::Confirmation)?;
        serialize_response(&project_submission(result))
    }

    fn list_activity(
        &self,
        window: &MainWalletWindowAuthority,
    ) -> Result<Response, TransactionBoundaryError> {
        let discovered = self.discover_activity(window.owner_label())?;
        serialize_response(&project_activity(discovered))
    }

    fn refresh_transaction_observation(
        &self,
        window: &MainWalletWindowAuthority,
        request: TransactionIdRequest,
    ) -> Result<Response, TransactionBoundaryError> {
        let discovered = self.discover_activity(window.owner_label())?;
        if !discovered
            .journal
            .as_ref()
            .map(WalletActivityJournal::records)
            .unwrap_or_default()
            .iter()
            .any(|record| record.tx_id == request.transaction_id)
        {
            return Err(TransactionBoundaryError::TransactionUnknown);
        }
        // Journal v2 authenticates public activity but deliberately does not retain the exact
        // signed envelope required for a safe Core refresh. Keep this reviewed command shape
        // fail-closed until a separately reviewed storage extension exists.
        Err(TransactionBoundaryError::TransactionUnknown)
    }

    fn ensure_spending_unblocked(
        &self,
        owner_window: &str,
    ) -> Result<(), TransactionBoundaryError> {
        let discovery = self.discover_activity(owner_window)?;
        if discovery.pending.is_some() {
            Err(TransactionBoundaryError::ReconciliationPending)
        } else {
            Ok(())
        }
    }

    fn discover_activity(
        &self,
        owner_window: &str,
    ) -> Result<ActivityDiscovery, TransactionBoundaryError> {
        let permit = self
            .runtime
            .begin_reconciliation_discovery(owner_window)
            .map_err(TransactionBoundaryError::Runtime)?;
        let custody = self.adapters.custody_path_authority();
        let mut pending = None;
        if let Some(restart) = permit
            .discover(custody)
            .map_err(|_| TransactionBoundaryError::ActivityUnavailable)?
        {
            let transaction_id = restart.transaction_id().to_string();
            match restart.phase_tag() {
                ReconciliationPhaseTag::Prepared => permit
                    .resolve_prepared(custody, restart)
                    .map_err(|_| TransactionBoundaryError::ActivityUnavailable)?,
                ReconciliationPhaseTag::MayHaveBeenSubmitted => {
                    let result = WalletCoreReadClient::from_supervisor(&self.supervisor)
                        .map_err(|_| TransactionBoundaryError::ActivityUnavailable)
                        .and_then(|source| {
                            permit
                                .reconcile_ambiguous_acceptance(custody, restart, &source)
                                .map_err(|_| TransactionBoundaryError::ActivityUnavailable)
                        });
                    match result {
                        Ok(WalletReconciliationResult::ResolvedRecorded) => {}
                        Ok(WalletReconciliationResult::AcceptedRecordingPending) => {
                            pending =
                                Some(PendingReconciliationResponse::AcceptedRecordingPending {
                                    transaction_id,
                                });
                        }
                        Ok(WalletReconciliationResult::Unresolved) | Err(_) => {
                            pending = Some(PendingReconciliationResponse::OutcomeUnknown {
                                transaction_id,
                            });
                        }
                    }
                }
                ReconciliationPhaseTag::AcceptedRecordingPending => {
                    if permit
                        .complete_accepted_recording(custody, restart)
                        .is_err()
                    {
                        pending = Some(PendingReconciliationResponse::AcceptedRecordingPending {
                            transaction_id,
                        });
                    }
                }
                ReconciliationPhaseTag::ResolvedNotAttempted
                | ReconciliationPhaseTag::ResolvedRejected
                | ReconciliationPhaseTag::ResolvedRecorded => {}
            }
        }
        let journal = match permit.load_activity(custody) {
            Ok(journal) => Some(journal),
            Err(_) if pending.is_some() => None,
            Err(_) => return Err(TransactionBoundaryError::ActivityUnavailable),
        };
        permit
            .complete(ActivityDiscovery { journal, pending })
            .map_err(TransactionBoundaryError::Runtime)
    }
}

fn ordinary_execution(response: Response) -> TransactionExecution {
    TransactionExecution {
        response,
        durable_submission_outcome: false,
    }
}

fn parse_transaction_envelope(
    request: WalletInvokeRequest<'_>,
) -> Result<WalletTransactionEnvelope, TransactionBoundaryError> {
    let InvokeBody::Json(Value::Object(object)) = request.body else {
        return Err(TransactionBoundaryError::InvalidRequest);
    };
    if request.declared_command != request.invoked_command {
        return Err(TransactionBoundaryError::InvalidRequest);
    }
    match request.declared_command {
        PREPARE_TRANSFER_PREVIEW => {
            parse_preview_request(object).map(WalletTransactionEnvelope::Prepare)
        }
        CANCEL_TRANSFER_PREVIEW => parse_handle(object).map(WalletTransactionEnvelope::Cancel),
        CONFIRM_AND_SUBMIT_TRANSFER => {
            parse_handle(object).map(WalletTransactionEnvelope::ConfirmAndSubmit)
        }
        LIST_ACTIVITY if object.is_empty() => Ok(WalletTransactionEnvelope::ListActivity),
        REFRESH_TRANSACTION_OBSERVATION => {
            let request = exact_request_object(object, &["transaction_id"])?;
            let transaction_id = request
                .get("transaction_id")
                .and_then(Value::as_str)
                .ok_or(TransactionBoundaryError::InvalidRequest)?;
            if !is_lower_hex_32(transaction_id) {
                return Err(TransactionBoundaryError::InvalidRequest);
            }
            Ok(WalletTransactionEnvelope::Refresh(TransactionIdRequest {
                transaction_id: transaction_id.to_owned(),
            }))
        }
        _ => Err(TransactionBoundaryError::InvalidRequest),
    }
}

fn parse_handle(
    object: &Map<String, Value>,
) -> Result<PreviewHandleRequest, TransactionBoundaryError> {
    let request = exact_request_object(object, &["preview_handle"])?;
    let preview_handle = request
        .get("preview_handle")
        .and_then(Value::as_str)
        .ok_or(TransactionBoundaryError::InvalidRequest)?;
    if preview_handle.is_empty()
        || preview_handle.len() > MAX_PREVIEW_HANDLE_BYTES
        || !preview_handle.is_ascii()
    {
        return Err(TransactionBoundaryError::InvalidRequest);
    }
    Ok(PreviewHandleRequest {
        preview_handle: preview_handle.to_owned(),
    })
}

fn parse_preview_request(
    object: &Map<String, Value>,
) -> Result<WalletTransferPreviewRequest, TransactionBoundaryError> {
    let request = exact_request_object(object, &["recipient", "amount"])?;
    let recipient = request
        .get("recipient")
        .and_then(Value::as_str)
        .ok_or(TransactionBoundaryError::InvalidRequest)?;
    let amount = request
        .get("amount")
        .and_then(Value::as_str)
        .ok_or(TransactionBoundaryError::InvalidRequest)?;
    WalletTransferPreviewRequest::from_borrowed(recipient, amount)
        .map_err(|_| TransactionBoundaryError::InvalidRequest)
}

fn exact_request_object<'a>(
    object: &'a Map<String, Value>,
    expected_fields: &[&str],
) -> Result<&'a Map<String, Value>, TransactionBoundaryError> {
    if object.len() != 1 {
        return Err(TransactionBoundaryError::InvalidRequest);
    }
    let request = object
        .get("request")
        .and_then(Value::as_object)
        .ok_or(TransactionBoundaryError::InvalidRequest)?;
    if request.len() != expected_fields.len()
        || expected_fields
            .iter()
            .any(|field| !request.contains_key(*field))
    {
        return Err(TransactionBoundaryError::InvalidRequest);
    }
    Ok(request)
}

fn project_preview(preview: &PreparedTransferPreview) -> PreviewResponse<'_> {
    PreviewResponse {
        preview_handle: preview.handle.as_str(),
        sender: preview.sender_address.as_str(),
        recipient: preview.recipient_address.as_str(),
        amount: preview.amount.as_str(),
        amount_raw_units: preview.amount_raw_units.to_string(),
        charged_fee: preview.charged_fee.as_str(),
        charged_fee_raw_units: preview.charged_fee_raw_units.to_string(),
        maximum_fee: preview.maximum_fee.as_str(),
        fee_limit_raw_units: preview.fee_limit_raw_units.to_string(),
        total_debit: preview.total_debit.as_str(),
        total_debit_raw_units: preview.total_debit_raw_units.to_string(),
        balance: preview.balance.as_str(),
        balance_raw_units: preview.balance_raw_units.to_string(),
        nonce: preview.nonce.to_string(),
        transaction_id: preview.transaction_id.as_str(),
        canonical_tip_height: preview.canonical_tip_height.to_string(),
        canonical_tip_hash: preview.canonical_tip_hash.as_str(),
        core_contract: preview.core_contract.as_str(),
        status_version: preview.status_version.as_str(),
        data_age_ms: preview.data_age_ms.to_string(),
        expires_after_ms: preview.expires_after_ms.to_string(),
        warning: preview.warning.as_str(),
    }
}

fn project_submission(result: PrivateSubmissionResult) -> SubmissionResponse {
    match result {
        PrivateSubmissionResult::Accepted { transaction_id } => SubmissionResponse::Accepted {
            transaction_id,
            observation: PublicObservation::NotObserved,
        },
        PrivateSubmissionResult::AcceptedRecordingPending { transaction_id } => {
            SubmissionResponse::AcceptedRecordingPending { transaction_id }
        }
        PrivateSubmissionResult::Rejected { transaction_id } => {
            SubmissionResponse::Rejected { transaction_id }
        }
        PrivateSubmissionResult::OutcomeUnknown { transaction_id } => {
            SubmissionResponse::OutcomeUnknown { transaction_id }
        }
    }
}

fn project_activity(discovery: ActivityDiscovery) -> ActivityResponse {
    let records = discovery
        .journal
        .as_ref()
        .map(WalletActivityJournal::records)
        .unwrap_or_default()
        .iter()
        .rev()
        .take(MAX_ACTIVITY_RECORDS)
        .map(project_activity_record)
        .collect();
    ActivityResponse {
        records,
        history_complete: false,
        pending_reconciliation: discovery.pending,
    }
}

fn project_activity_record(record: &WalletActivityRecord) -> PublicActivityRecord {
    PublicActivityRecord {
        transaction_id: record.tx_id.clone(),
        sender: record.sender_address.clone(),
        recipient: record.recipient_address.clone(),
        amount_raw_units: record.amount_raw_units.clone(),
        nonce: record.nonce.to_string(),
        tip_raw_units: record.tip_raw_units.to_string(),
        fee_limit_raw_units: record.fee_limit_raw_units.to_string(),
        submitted_at_unix_ms: record.submitted_at_unix_ms.to_string(),
        last_observed_at_unix_ms: record
            .last_observed_at_unix_ms
            .map(|value| value.to_string()),
        observation: project_observation(&record.observation),
    }
}

fn project_observation(observation: &WalletReceiptObservation) -> PublicObservation {
    match receipt_presentation(observation) {
        WalletReceiptPresentation::NotObserved => PublicObservation::NotObserved,
        WalletReceiptPresentation::Pending => PublicObservation::Pending,
        WalletReceiptPresentation::Mined {
            confirmations,
            confidence: WalletReceiptConfidence::Observed,
        } => PublicObservation::MinedObserved {
            confirmations: confirmations.to_string(),
        },
        WalletReceiptPresentation::Mined {
            confirmations,
            confidence: WalletReceiptConfidence::High,
        } => PublicObservation::MinedHighConfidence {
            confirmations: confirmations.to_string(),
        },
    }
}

fn serialize_response<T: Serialize>(value: &T) -> Result<Response, TransactionBoundaryError> {
    serde_json::to_string(value)
        .map(Response::new)
        .map_err(|_| TransactionBoundaryError::SerializationUnavailable)
}

fn now_unix_ms() -> Result<u64, TransactionBoundaryError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| TransactionBoundaryError::Runtime(WalletRuntimeError::RuntimeUnavailable))?;
    u64::try_from(elapsed.as_millis())
        .map_err(|_| TransactionBoundaryError::Runtime(WalletRuntimeError::RuntimeUnavailable))
}

fn is_lower_hex_32(value: &str) -> bool {
    value.len() == TRANSACTION_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn map_shared_error(error: super::BoundaryError) -> TransactionBoundaryError {
    match error {
        super::BoundaryError::InvalidRequest => TransactionBoundaryError::InvalidRequest,
        super::BoundaryError::InvalidWindow => TransactionBoundaryError::InvalidWindow,
        super::BoundaryError::ActivationUnavailable => {
            TransactionBoundaryError::ActivationUnavailable
        }
        super::BoundaryError::Runtime(error) => TransactionBoundaryError::Runtime(error),
        super::BoundaryError::Lifecycle(_) | super::BoundaryError::SerializationUnavailable => {
            TransactionBoundaryError::Runtime(WalletRuntimeError::RuntimeUnavailable)
        }
    }
}

impl TransactionBoundaryError {
    const fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidWindow => "invalid_window",
            Self::ActivationUnavailable => "wallet_activation_unavailable",
            Self::Runtime(error) => match error {
                WalletRuntimeError::InvalidWindow => "invalid_window",
                WalletRuntimeError::ActivationUnavailable => "wallet_activation_unavailable",
                WalletRuntimeError::OperationInProgress => "wallet_operation_in_progress",
                WalletRuntimeError::InvalidRequest => "invalid_request",
                WalletRuntimeError::ReconciliationUnavailable => "wallet_activity_unavailable",
                _ => "wallet_runtime_unavailable",
            },
            Self::Preview(error) => error.code(),
            Self::Confirmation(error) => error.code(),
            Self::ReconciliationPending => "wallet_reconciliation_pending",
            Self::ActivityUnavailable => "wallet_activity_unavailable",
            Self::TransactionUnknown => "wallet_transaction_unknown",
            Self::SerializationUnavailable => "wallet_runtime_unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        account::derive_account_identity,
        lifecycle::WalletCustodyPathAuthority,
        reconciliation::{
            publish_accepted_for_test, publish_may_have_been_submitted_for_test,
            ReconciliationAuthenticator, ReconciliationRecord, ReconciliationStore,
        },
        runtime::WalletOperationKind,
        secrets::{WalletPassword, WalletSeed},
        vault::EncryptedWalletVault,
    };
    use std::path::Path;
    use std::{sync::mpsc, thread, time::Duration};
    use tauri::ipc::IpcResponse;

    const TEST_HWND: isize = 0x1234;
    const MAIN: &str = "main";
    const PASSWORD: &str = "correct horse battery staple";

    fn request<'a>(command: &'static str, body: &'a InvokeBody) -> WalletInvokeRequest<'a> {
        WalletInvokeRequest {
            declared_command: command,
            invoked_command: command,
            body,
        }
    }

    fn json(value: Value) -> InvokeBody {
        InvokeBody::Json(value)
    }

    fn unlocked_boundary(
        directory: &Path,
    ) -> (
        Arc<WalletRuntimeState>,
        WalletTransactionCommandBoundary,
        MainWalletWindowAuthority,
        WalletSeed,
        String,
    ) {
        crate::wallet::storage_security::protect_directory(directory).unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let seed = WalletSeed::for_test(0x31);
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
        let adapters = Arc::new(WalletLifecycleAdapters::for_test(
            Arc::clone(&runtime),
            &directory.join("wallet.vault.json"),
        ));
        let boundary = WalletTransactionCommandBoundary::for_test(
            Arc::clone(&runtime),
            adapters,
            Arc::new(SupervisorState::default()),
            TEST_HWND,
        );
        let authority = MainWalletWindowAuthority {
            revocation_epoch: runtime.capture_boundary_epoch().unwrap(),
            hwnd: TEST_HWND,
        };
        (runtime, boundary, authority, seed, identity.address)
    }

    fn reconciliation_record(sender: &str) -> ReconciliationRecord {
        ReconciliationRecord::prepared(
            "primary".to_string(),
            "10".repeat(32),
            "22".repeat(32),
            sender.to_string(),
            "30".repeat(32),
            "125000000".to_string(),
            7,
            0,
            201,
            "40".repeat(32),
            "50".repeat(32),
            1_700_000_000_123,
        )
    }

    fn response_value(response: Response) -> Value {
        response.body().unwrap().deserialize().unwrap()
    }

    fn expect_response(result: Result<TransactionExecution, TransactionBoundaryError>) -> Response {
        match result {
            Ok(execution) => execution.response,
            Err(_) => panic!("transaction boundary unexpectedly failed"),
        }
    }

    #[test]
    fn exact_transaction_envelopes_parse() {
        let cases = [
            (
                PREPARE_TRANSFER_PREVIEW,
                json(serde_json::json!({"request":{"recipient":"11".repeat(32),"amount":"1.25"}})),
            ),
            (
                CANCEL_TRANSFER_PREVIEW,
                json(serde_json::json!({"request":{"preview_handle":"ab".repeat(32)}})),
            ),
            (
                CONFIRM_AND_SUBMIT_TRANSFER,
                json(serde_json::json!({"request":{"preview_handle":"cd".repeat(32)}})),
            ),
            (LIST_ACTIVITY, json(serde_json::json!({}))),
            (
                REFRESH_TRANSACTION_OBSERVATION,
                json(serde_json::json!({"request":{"transaction_id":"22".repeat(32)}})),
            ),
        ];
        for (command, body) in &cases {
            assert!(parse_transaction_envelope(request(command, body)).is_ok());
        }
    }

    #[test]
    fn transaction_envelopes_reject_raw_unknown_wrong_case_and_secret_fields() {
        let cases = [
            InvokeBody::Raw(b"{}".to_vec()),
            json(Value::Null),
            json(serde_json::json!({"Request":{}})),
            json(serde_json::json!({"request":{},"password":"canary"})),
            json(serde_json::json!({"seed":"canary"})),
        ];
        for body in &cases {
            assert!(matches!(
                parse_transaction_envelope(request(PREPARE_TRANSFER_PREVIEW, body)),
                Err(TransactionBoundaryError::InvalidRequest)
            ));
        }
    }

    #[test]
    fn command_mismatch_and_unknown_command_fail_closed() {
        let body = json(serde_json::json!({}));
        let mismatch = WalletInvokeRequest {
            declared_command: LIST_ACTIVITY,
            invoked_command: CANCEL_TRANSFER_PREVIEW,
            body: &body,
        };
        assert!(matches!(
            parse_transaction_envelope(mismatch),
            Err(TransactionBoundaryError::InvalidRequest)
        ));
        assert!(matches!(
            parse_transaction_envelope(request("wallet_unknown", &body)),
            Err(TransactionBoundaryError::InvalidRequest)
        ));
    }

    #[test]
    fn handles_and_transaction_identifiers_are_bounded_and_canonical() {
        for handle in [String::new(), "a".repeat(129), "\u{2603}".to_string()] {
            let body = json(serde_json::json!({"request":{"preview_handle":handle}}));
            assert!(parse_transaction_envelope(request(CANCEL_TRANSFER_PREVIEW, &body)).is_err());
        }
        for transaction_id in ["A".repeat(64), "a".repeat(63), "g".repeat(64)] {
            let body = json(serde_json::json!({"request":{"transaction_id":transaction_id}}));
            assert!(
                parse_transaction_envelope(request(REFRESH_TRANSACTION_OBSERVATION, &body))
                    .is_err()
            );
        }
    }

    #[test]
    fn oversized_and_nested_values_are_rejected_before_owned_request_construction() {
        let oversized = "a".repeat(1024 * 1024);
        let cases = [
            (
                PREPARE_TRANSFER_PREVIEW,
                json(serde_json::json!({
                    "request": {"recipient": oversized, "amount": "1"}
                })),
            ),
            (
                PREPARE_TRANSFER_PREVIEW,
                json(serde_json::json!({
                    "request": {"recipient": "11".repeat(32), "amount": oversized}
                })),
            ),
            (
                CANCEL_TRANSFER_PREVIEW,
                json(serde_json::json!({"request":{"preview_handle":oversized}})),
            ),
            (
                REFRESH_TRANSACTION_OBSERVATION,
                json(serde_json::json!({"request":{"transaction_id":oversized}})),
            ),
            (
                CANCEL_TRANSFER_PREVIEW,
                json(serde_json::json!({
                    "request":{"preview_handle":{"nested":oversized}}
                })),
            ),
        ];
        for (command, body) in &cases {
            assert!(matches!(
                parse_transaction_envelope(request(command, body)),
                Err(TransactionBoundaryError::InvalidRequest)
            ));
        }

        let source = include_str!("transaction.rs");
        let parser = source
            .split("fn project_preview")
            .next()
            .expect("parser source");
        assert!(!parser.contains("deserialize_request"));
        assert!(!parser.contains("value.clone()"));
    }

    #[test]
    fn concurrent_and_reordered_operations_fail_without_queuing() {
        let directory = tempfile::tempdir().unwrap();
        let (_, boundary, _, _, _) = unlocked_boundary(directory.path());
        let boundary = Arc::new(boundary);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first = Arc::clone(&boundary);
        let owner = thread::spawn(move || {
            let Ok(_guard) = first.acquire_operation_gate() else {
                panic!("first operation did not acquire the gate");
            };
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let (result_tx, result_rx) = mpsc::channel();
        let contender = Arc::clone(&boundary);
        let rejected = thread::spawn(move || {
            let result = contender.acquire_operation_gate();
            result_tx
                .send(matches!(
                    result,
                    Err(TransactionBoundaryError::Runtime(
                        WalletRuntimeError::OperationInProgress
                    ))
                ))
                .unwrap();
        });
        assert!(result_rx.recv_timeout(Duration::from_secs(1)).unwrap());
        contender_join(rejected);
        release_tx.send(()).unwrap();
        contender_join(owner);
        assert!(boundary.acquire_operation_gate().is_ok());
    }

    fn contender_join(handle: thread::JoinHandle<()>) {
        assert!(handle.join().is_ok());
    }

    #[test]
    fn submission_projection_keeps_known_acceptance_distinct_from_ambiguity() {
        let id = "11".repeat(32);
        let accepted_pending = serde_json::to_value(project_submission(
            PrivateSubmissionResult::AcceptedRecordingPending {
                transaction_id: id.clone(),
            },
        ))
        .unwrap();
        let unknown = serde_json::to_value(project_submission(
            PrivateSubmissionResult::OutcomeUnknown {
                transaction_id: id.clone(),
            },
        ))
        .unwrap();
        assert_eq!(accepted_pending["state"], "accepted_recording_pending");
        assert_eq!(unknown["state"], "outcome_unknown");
        assert_eq!(accepted_pending["transaction_id"], id);
    }

    #[test]
    fn preview_projection_uses_decimal_strings_and_excludes_private_authority() {
        let preview = PreparedTransferPreview {
            handle: "aa".repeat(32),
            sender_address: "11".repeat(32),
            recipient_address: "22".repeat(32),
            amount: "1.25".to_string(),
            amount_raw_units: u128::MAX,
            charged_fee: "0.00000001".to_string(),
            charged_fee_raw_units: u64::MAX,
            maximum_fee: "0.00000201".to_string(),
            fee_limit_raw_units: 201,
            total_debit: "1.25000001".to_string(),
            total_debit_raw_units: u128::MAX - 1,
            balance: "99".to_string(),
            balance_raw_units: u128::MAX - 2,
            nonce: u64::MAX,
            transaction_id: "33".repeat(32),
            canonical_tip_height: u64::MAX,
            canonical_tip_hash: "44".repeat(32),
            core_contract: "contract-v1".to_string(),
            status_version: "3".to_string(),
            data_age_ms: 17,
            expires_after_ms: 30_000,
            warning: "warning".to_string(),
        };
        let value = serde_json::to_value(project_preview(&preview)).unwrap();
        assert_eq!(value["amount_raw_units"], u128::MAX.to_string());
        assert_eq!(value["nonce"], u64::MAX.to_string());
        assert_eq!(value["canonical_tip_height"], u64::MAX.to_string());
        for prohibited in [
            "unsigned_transaction",
            "canonical_argument_bytes",
            "core_identity_fingerprint",
            "process_identity",
            "signature",
            "signed_body",
        ] {
            assert!(value.get(prohibited).is_none());
        }
    }

    #[test]
    fn activity_projection_is_newest_first_bounded_and_explicitly_incomplete() {
        let records = (0_u64..105)
            .map(|nonce| WalletActivityRecord {
                tx_id: format!("{nonce:064x}"),
                sender_address: "11".repeat(32),
                recipient_address: "22".repeat(32),
                amount_raw_units: "1".to_string(),
                nonce,
                tip_raw_units: 0,
                fee_limit_raw_units: 201,
                submitted_at_unix_ms: nonce,
                last_observed_at_unix_ms: None,
                observation: WalletReceiptObservation::NotFound,
            })
            .collect();
        let response = project_activity(ActivityDiscovery {
            journal: Some(WalletActivityJournal::from_records_for_test(records)),
            pending: None,
        });
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["records"].as_array().unwrap().len(), 100);
        assert_eq!(value["records"][0]["nonce"], "104");
        assert_eq!(value["records"][99]["nonce"], "5");
        assert_eq!(value["history_complete"], false);
    }

    #[test]
    fn fixed_errors_never_embed_request_or_secret_material() {
        let serialized = [
            TransactionBoundaryError::InvalidRequest,
            TransactionBoundaryError::ReconciliationPending,
            TransactionBoundaryError::ActivityUnavailable,
            TransactionBoundaryError::TransactionUnknown,
        ]
        .into_iter()
        .map(|error| serde_json::json!({"code": error.code()}).to_string())
        .collect::<String>();
        for canary in [
            "password-canary",
            "seed-canary",
            "vault-canary",
            "signature-canary",
        ] {
            assert!(!serialized.contains(canary));
        }
    }

    #[test]
    fn repeated_activity_discovery_restores_ambiguity_and_blocks_new_spending() {
        let directory = tempfile::tempdir().unwrap();
        let (_runtime, boundary, authority, seed, sender) = unlocked_boundary(directory.path());
        let custody: &WalletCustodyPathAuthority = boundary.adapters.custody_path_authority();
        let store = ReconciliationStore::for_custody(custody).unwrap();
        let authenticator = ReconciliationAuthenticator::new("primary", &seed).unwrap();
        publish_may_have_been_submitted_for_test(
            &store,
            &authenticator,
            reconciliation_record(sender.as_str()),
        )
        .unwrap();

        for _ in 0..2 {
            let value = response_value(expect_response(
                boundary.execute_envelope(WalletTransactionEnvelope::ListActivity, &authority),
            ));
            assert_eq!(value["pending_reconciliation"]["state"], "outcome_unknown");
            assert_eq!(
                value["pending_reconciliation"]["transaction_id"],
                "22".repeat(32)
            );
            assert_eq!(value["history_complete"], false);
        }

        let prepare: WalletTransferPreviewRequest = serde_json::from_value(serde_json::json!({
            "recipient": "60".repeat(32),
            "amount": "1.25"
        }))
        .unwrap();
        assert!(matches!(
            boundary.execute_envelope(WalletTransactionEnvelope::Prepare(prepare), &authority),
            Err(TransactionBoundaryError::ReconciliationPending)
        ));
    }

    #[test]
    fn accepted_recording_pending_completes_once_without_core_access() {
        let directory = tempfile::tempdir().unwrap();
        let (_runtime, boundary, authority, seed, sender) = unlocked_boundary(directory.path());
        let custody = boundary.adapters.custody_path_authority();
        let store = ReconciliationStore::for_custody(custody).unwrap();
        let authenticator = ReconciliationAuthenticator::new("primary", &seed).unwrap();
        publish_accepted_for_test(
            &store,
            &authenticator,
            reconciliation_record(sender.as_str()),
        )
        .unwrap();

        for _ in 0..2 {
            let value = response_value(expect_response(
                boundary.execute_envelope(WalletTransactionEnvelope::ListActivity, &authority),
            ));
            assert!(value["pending_reconciliation"].is_null());
            assert_eq!(value["records"].as_array().unwrap().len(), 1);
            assert_eq!(value["records"][0]["transaction_id"], "22".repeat(32));
        }

        assert!(matches!(
            boundary.execute_envelope(
                WalletTransactionEnvelope::Refresh(TransactionIdRequest {
                    transaction_id: "22".repeat(32),
                }),
                &authority,
            ),
            Err(TransactionBoundaryError::TransactionUnknown)
        ));
    }

    #[test]
    fn journal_failure_preserves_known_acceptance_without_claiming_ambiguity() {
        let directory = tempfile::tempdir().unwrap();
        let (_runtime, boundary, authority, seed, sender) = unlocked_boundary(directory.path());
        let custody = boundary.adapters.custody_path_authority();
        let store = ReconciliationStore::for_custody(custody).unwrap();
        let authenticator = ReconciliationAuthenticator::new("primary", &seed).unwrap();
        publish_accepted_for_test(
            &store,
            &authenticator,
            reconciliation_record(sender.as_str()),
        )
        .unwrap();
        std::fs::create_dir(custody.journal_path()).unwrap();

        let value = response_value(expect_response(
            boundary.execute_envelope(WalletTransactionEnvelope::ListActivity, &authority),
        ));
        assert_eq!(
            value["pending_reconciliation"]["state"],
            "accepted_recording_pending"
        );
        assert_ne!(value["pending_reconciliation"]["state"], "outcome_unknown");
        assert!(value["records"].as_array().unwrap().is_empty());
    }
}
