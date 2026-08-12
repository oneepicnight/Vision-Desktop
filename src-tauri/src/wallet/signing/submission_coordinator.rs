use super::SignedTransferArtifact;
use crate::wallet::{
    core_client::WalletCoreSubmissionSource,
    envelope_store::{EnvelopeEntryInput, EnvelopeStore},
    lifecycle::WalletCustodyPathAuthority,
    reconciliation::{ReconciliationRecord, ReconciliationStore},
    runtime::{WalletRuntimeError, WalletSubmissionPermit},
    submission::{
        classify_submission_response, PrivateSubmissionResponseDisposition,
        SubmissionRejectionPolicy,
    },
    transaction::canonical_transaction_id,
};
use serde::Deserialize;
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use zeroize::Zeroizing;

const MAX_SIGNED_BODY_BYTES: usize = 64 * 1024;
const BODY_DIGEST_CONTEXT: &str = "com.vision.desktop.wallet-signed-envelope-digest.v1";

pub(in crate::wallet) enum PrivateSubmissionResult {
    Accepted { transaction_id: String },
    AcceptedRecordingPending { transaction_id: String },
    Rejected { transaction_id: String },
    OutcomeUnknown { transaction_id: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PrivateSubmissionError {
    RuntimeRevoked,
    ActivationUnavailable,
    IntentRejected,
    ReconciliationUnavailable,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CashTransferArgs {
    to: String,
    amount: u128,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DurableSubmissionPhase {
    PreWrite,
    MayHaveBeenSubmitted,
    AcceptedRecordingPending,
    Accepted,
    Rejected,
}

pub(super) fn submit_signed_artifact<S: WalletCoreSubmissionSource>(
    permit: WalletSubmissionPermit<'_>,
    artifact: SignedTransferArtifact,
    source: &S,
    custody: &WalletCustodyPathAuthority,
    created_at_unix_ms: u64,
    rejection_policy: &SubmissionRejectionPolicy,
) -> Result<PrivateSubmissionResult, PrivateSubmissionError> {
    let phase = Cell::new(DurableSubmissionPhase::PreWrite);
    let transaction_id = artifact.transaction_id.clone();
    let attempt = catch_unwind(AssertUnwindSafe(|| {
        submit_signed_artifact_inner(
            permit,
            artifact,
            source,
            custody,
            created_at_unix_ms,
            rejection_policy,
            &phase,
        )
    }));
    match attempt {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => durable_result(phase.get(), transaction_id).ok_or(error),
        Err(_) => durable_result(phase.get(), transaction_id)
            .ok_or(PrivateSubmissionError::RuntimeRevoked),
    }
}

fn durable_result(
    phase: DurableSubmissionPhase,
    transaction_id: String,
) -> Option<PrivateSubmissionResult> {
    match phase {
        DurableSubmissionPhase::PreWrite => None,
        DurableSubmissionPhase::MayHaveBeenSubmitted => {
            Some(PrivateSubmissionResult::OutcomeUnknown { transaction_id })
        }
        DurableSubmissionPhase::AcceptedRecordingPending => {
            Some(PrivateSubmissionResult::AcceptedRecordingPending { transaction_id })
        }
        DurableSubmissionPhase::Accepted => {
            Some(PrivateSubmissionResult::Accepted { transaction_id })
        }
        DurableSubmissionPhase::Rejected => {
            Some(PrivateSubmissionResult::Rejected { transaction_id })
        }
    }
}

fn submit_signed_artifact_inner<S: WalletCoreSubmissionSource>(
    mut permit: WalletSubmissionPermit<'_>,
    artifact: SignedTransferArtifact,
    source: &S,
    custody: &WalletCustodyPathAuthority,
    created_at_unix_ms: u64,
    rejection_policy: &SubmissionRejectionPolicy,
    phase: &Cell<DurableSubmissionPhase>,
) -> Result<PrivateSubmissionResult, PrivateSubmissionError> {
    validate_authority(&permit, &artifact, source)?;
    let exact_body = exact_signed_body(&permit, &artifact)?;
    let signed_body_digest_hex = digest_hex(BODY_DIGEST_CONTEXT, exact_body.as_slice());
    let args: CashTransferArgs = serde_json::from_slice(&artifact.transaction.args)
        .map_err(|_| PrivateSubmissionError::IntentRejected)?;
    if !is_lower_hex_32(&args.to)
        || args.to == artifact.transaction.sender_pubkey
        || args.amount == 0
    {
        return Err(PrivateSubmissionError::IntentRejected);
    }
    let mut attempt_bytes = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut *attempt_bytes)
        .map_err(|_| PrivateSubmissionError::ReconciliationUnavailable)?;
    let store = ReconciliationStore::for_custody(custody)
        .map_err(|_| PrivateSubmissionError::ReconciliationUnavailable)?;
    let envelope_store = EnvelopeStore::for_custody(custody)
        .map_err(|_| PrivateSubmissionError::ReconciliationUnavailable)?;
    let compatibility_contract_digest_hex =
        crate::wallet::submission::compatibility_contract_digest(rejection_policy);
    let reservation = permit
        .reserve_reconciliation(&store)
        .map_err(map_runtime_error)?;
    let prepared_envelope = permit
        .publish_prepared_envelope(
            custody,
            &envelope_store,
            EnvelopeEntryInput {
                wallet_id: &artifact.wallet_id,
                attempt_id: &hex::encode(attempt_bytes.as_slice()),
                transaction_id: &artifact.transaction_id,
                transaction: &artifact.transaction,
                exact_body: exact_body.as_slice(),
                signed_body_digest_hex: &signed_body_digest_hex,
                compatibility_contract_digest_hex: &compatibility_contract_digest_hex,
                created_at_unix_ms,
            },
            &reservation,
        )
        .map_err(map_runtime_error)?;
    let record = ReconciliationRecord::prepared_bound(
        artifact.wallet_id.clone(),
        hex::encode(attempt_bytes.as_slice()),
        artifact.transaction_id.clone(),
        artifact.transaction.sender_pubkey.clone(),
        args.to,
        args.amount.to_string(),
        artifact.transaction.nonce,
        artifact.transaction.tip,
        artifact.transaction.fee_limit,
        signed_body_digest_hex,
        prepared_envelope.commitment_hex().to_string(),
        &reservation,
        hex::encode(artifact.core_identity_fingerprint),
        created_at_unix_ms,
    );

    let grant = permit.take_activation_grant().map_err(map_runtime_error)?;
    let (live, write) = grant.split();
    let prepared = match permit.publish_prepared(live, &store, record, reservation) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = permit.remove_prewrite_envelope(&envelope_store, prepared_envelope);
            return Err(map_runtime_error(error));
        }
    };
    if prepared
        .verify_envelope_binding(&prepared_envelope)
        .is_err()
        || permit
            .verify_prepared_envelope(&envelope_store, &prepared_envelope)
            .is_err()
    {
        return Err(PrivateSubmissionError::ReconciliationUnavailable);
    }
    if validate_authority(&permit, &artifact, source).is_err() {
        permit
            .resolve_not_attempted(prepared, &store)
            .map_err(map_runtime_error)?;
        permit
            .remove_prewrite_envelope(&envelope_store, prepared_envelope)
            .map_err(map_runtime_error)?;
        permit.complete(()).map_err(map_runtime_error)?;
        return Err(PrivateSubmissionError::RuntimeRevoked);
    }
    let may_have = permit
        .publish_may_have_been_submitted(prepared, &store)
        .map_err(map_runtime_error)?;
    phase.set(DurableSubmissionPhase::MayHaveBeenSubmitted);
    let ambiguous_envelope = permit
        .mark_envelope_ambiguous(&envelope_store, prepared_envelope)
        .map_err(map_runtime_error)?;
    let write_ready = may_have.combine(write);
    let (may_have, write_once) = write_ready.into_parts();

    if validate_authority(&permit, &artifact, source).is_err() {
        drop(may_have);
        return complete_with_core_validation(
            permit,
            &artifact,
            source,
            PrivateSubmissionResult::OutcomeUnknown {
                transaction_id: artifact.transaction_id.clone(),
            },
        );
    }

    let response = match source.submit_once(write_once, exact_body.as_slice()) {
        Ok(response) => response,
        Err(_) => {
            drop(may_have);
            return complete_with_core_validation(
                permit,
                &artifact,
                source,
                PrivateSubmissionResult::OutcomeUnknown {
                    transaction_id: artifact.transaction_id.clone(),
                },
            );
        }
    };
    let disposition = classify_submission_response(
        response.status,
        response.body.as_slice(),
        &artifact.transaction_id,
        artifact.transaction.nonce,
        rejection_policy,
    );
    if validate_authority(&permit, &artifact, source).is_err() {
        drop(may_have);
        return complete_with_core_validation(
            permit,
            &artifact,
            source,
            PrivateSubmissionResult::OutcomeUnknown {
                transaction_id: artifact.transaction_id.clone(),
            },
        );
    }

    match disposition {
        PrivateSubmissionResponseDisposition::Accepted {
            transaction_id,
            nonce,
        } => {
            phase.set(DurableSubmissionPhase::AcceptedRecordingPending);
            let accepted = permit
                .publish_accepted(
                    may_have,
                    &store,
                    transaction_id,
                    nonce,
                    compatibility_contract_digest_hex,
                )
                .map_err(map_runtime_error)?;
            let accepted_envelope = permit
                .mark_envelope_accepted(&envelope_store, ambiguous_envelope)
                .map_err(map_runtime_error)?;
            let evidence = match accepted.evidence() {
                Ok(evidence) => evidence,
                Err(_) => {
                    return permit
                        .complete(PrivateSubmissionResult::AcceptedRecordingPending {
                            transaction_id: artifact.transaction_id.clone(),
                        })
                        .map_err(map_runtime_error)
                }
            };
            if evidence.envelope_commitment_hex() != accepted_envelope.commitment_hex()
                || evidence.transaction_id() != accepted_envelope.transaction_id()
                || permit
                    .verify_accepted_envelope(&envelope_store, &accepted_envelope)
                    .is_err()
            {
                return permit
                    .complete(PrivateSubmissionResult::AcceptedRecordingPending {
                        transaction_id: artifact.transaction_id.clone(),
                    })
                    .map_err(map_runtime_error);
            }
            if permit.record_accepted_evidence(custody, &evidence).is_err() {
                return permit
                    .complete(PrivateSubmissionResult::AcceptedRecordingPending {
                        transaction_id: artifact.transaction_id.clone(),
                    })
                    .map_err(map_runtime_error);
            }
            if permit.resolve_recorded(accepted, &store).is_err() {
                return permit
                    .complete(PrivateSubmissionResult::AcceptedRecordingPending {
                        transaction_id: artifact.transaction_id.clone(),
                    })
                    .map_err(map_runtime_error);
            }
            phase.set(DurableSubmissionPhase::Accepted);
            validate_authority(&permit, &artifact, source)?;
            permit
                .complete(PrivateSubmissionResult::Accepted {
                    transaction_id: artifact.transaction_id.clone(),
                })
                .map_err(map_runtime_error)
        }
        PrivateSubmissionResponseDisposition::DefinitiveRejected {
            http_status,
            code,
            allowlist_digest_hex,
        } => {
            phase.set(DurableSubmissionPhase::Rejected);
            permit
                .resolve_rejected(
                    may_have,
                    &store,
                    http_status,
                    code.as_str().to_string(),
                    allowlist_digest_hex,
                )
                .map_err(map_runtime_error)?;
            validate_authority(&permit, &artifact, source)?;
            permit
                .complete(PrivateSubmissionResult::Rejected {
                    transaction_id: artifact.transaction_id.clone(),
                })
                .map_err(map_runtime_error)
        }
        PrivateSubmissionResponseDisposition::OutcomeUnknown => {
            drop(may_have);
            drop(ambiguous_envelope);
            complete_with_core_validation(
                permit,
                &artifact,
                source,
                PrivateSubmissionResult::OutcomeUnknown {
                    transaction_id: artifact.transaction_id.clone(),
                },
            )
        }
    }
}

fn complete_with_core_validation(
    permit: WalletSubmissionPermit<'_>,
    artifact: &SignedTransferArtifact,
    source: &impl WalletCoreSubmissionSource,
    desired: PrivateSubmissionResult,
) -> Result<PrivateSubmissionResult, PrivateSubmissionError> {
    let result = if validate_authority(&permit, artifact, source).is_ok() {
        desired
    } else {
        PrivateSubmissionResult::OutcomeUnknown {
            transaction_id: artifact.transaction_id.clone(),
        }
    };
    permit.complete(result).map_err(map_runtime_error)
}

fn validate_authority(
    permit: &WalletSubmissionPermit<'_>,
    artifact: &SignedTransferArtifact,
    source: &impl WalletCoreSubmissionSource,
) -> Result<(), PrivateSubmissionError> {
    permit.ensure_current().map_err(map_runtime_error)?;
    if permit.wallet_id() != artifact.wallet_id
        || permit.core_identity_fingerprint() != &artifact.core_identity_fingerprint
    {
        return Err(PrivateSubmissionError::RuntimeRevoked);
    }
    let fingerprint = source
        .validated_identity_fingerprint()
        .map_err(|_| PrivateSubmissionError::RuntimeRevoked)?;
    permit.ensure_current().map_err(map_runtime_error)?;
    if fingerprint != artifact.core_identity_fingerprint {
        return Err(PrivateSubmissionError::RuntimeRevoked);
    }
    Ok(())
}

fn exact_signed_body(
    permit: &WalletSubmissionPermit<'_>,
    artifact: &SignedTransferArtifact,
) -> Result<Zeroizing<Vec<u8>>, PrivateSubmissionError> {
    permit.ensure_current().map_err(map_runtime_error)?;
    if canonical_transaction_id(&artifact.transaction)
        .ok()
        .as_deref()
        != Some(artifact.transaction_id.as_str())
        || artifact.transaction.sig.len() != 128
        || !artifact
            .transaction
            .sig
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PrivateSubmissionError::IntentRejected);
    }
    let body = serde_json::to_vec(&artifact.transaction)
        .map_err(|_| PrivateSubmissionError::IntentRejected)?;
    if body.is_empty() || body.len() > MAX_SIGNED_BODY_BYTES {
        return Err(PrivateSubmissionError::IntentRejected);
    }
    Ok(Zeroizing::new(body))
}

fn digest_hex(context: &'static str, bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

fn is_lower_hex_32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

const fn map_runtime_error(error: WalletRuntimeError) -> PrivateSubmissionError {
    match error {
        WalletRuntimeError::ActivationUnavailable => PrivateSubmissionError::ActivationUnavailable,
        WalletRuntimeError::ReconciliationUnavailable => {
            PrivateSubmissionError::ReconciliationUnavailable
        }
        WalletRuntimeError::ProcessLockUnavailable
        | WalletRuntimeError::UnsupportedWindowsHost
        | WalletRuntimeError::RuntimeUnavailable
        | WalletRuntimeError::InvalidWindow
        | WalletRuntimeError::OperationInProgress
        | WalletRuntimeError::InvalidRequest
        | WalletRuntimeError::SecureRandomUnavailable
        | WalletRuntimeError::PathAuthorizationInvalid
        | WalletRuntimeError::PathAuthorizationExpired
        | WalletRuntimeError::RecoverySelectionCancelled
        | WalletRuntimeError::RecoveryDestinationInvalid
        | WalletRuntimeError::RecoveryDestinationExists
        | WalletRuntimeError::RecoverySourceInvalid => PrivateSubmissionError::RuntimeRevoked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        account::derive_account_identity,
        core_client::{
            WalletCoreAccountSnapshot, WalletCoreClientError, WalletCoreHttpResponse,
            WalletCoreReadSource, WalletCoreReceiptSource, WalletCoreStatus,
        },
        journal::{append_accepted_submission, WalletJournalAuthenticator},
        lifecycle::WalletCustodyPathAuthority,
        preview::{
            bind_consumed_preview_for_test, prepare_with_source_for_test,
            PendingTransferConfirmation,
        },
        public_request::WalletTransferPreviewRequest,
        reconciliation::{CoreWriteOnce, ReconciliationStore},
        runtime::{
            WalletOperationKind, WalletOperationPermit, WalletReconciliationResult,
            WalletRuntimeState,
        },
        secrets::{WalletPassword, WalletSeed},
        submission::{SubmissionRejectionPolicy, WalletSubmissionOutcome},
        test_request_ledger::TestRequestLedger,
        transaction::{
            canonical_transaction_id, sign_cash_transfer_for_test, CashTransferDraft,
            VisionTransaction,
        },
        transaction_confirmation::NativeConfirmationApproval,
        vault::EncryptedWalletVault,
    };
    use std::sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    };

    const MAIN: &str = "main";
    const PASSWORD: &str = "correct horse battery staple";

    #[derive(Clone, Copy)]
    enum ResponseMode {
        Accepted,
        TransportFailure,
        AcceptedResponseLost,
        Malformed,
        Duplicate,
        Rejected,
        CoreReplacementBeforeWrite,
        CoreReplacementAfterWrite,
        PanicDuringWrite,
        PanicDuringLookup,
        CoreReplacementDuringRestartLookup,
    }

    struct FakeSubmissionCore {
        address: String,
        writes: Arc<AtomicUsize>,
        mode: ResponseMode,
        fingerprint: [u8; 32],
        identity_calls: Arc<AtomicUsize>,
        revoke_on_identity_call: Option<(usize, Arc<WalletRuntimeState>)>,
        lookup_body: Arc<Mutex<Option<Vec<u8>>>>,
        journal_failure_path: Option<std::path::PathBuf>,
        account_nonce: Arc<AtomicU64>,
        request_ledger: TestRequestLedger,
    }

    impl WalletCoreReadSource for FakeSubmissionCore {
        fn account_snapshot(
            &self,
            _address: &str,
        ) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError> {
            Ok(WalletCoreAccountSnapshot {
                address: self.address.clone(),
                exists: true,
                balance: 10_000_000_000,
                nonce: self.account_nonce.load(Ordering::SeqCst),
            })
        }

        fn status(&self) -> Result<WalletCoreStatus, WalletCoreClientError> {
            Ok(WalletCoreStatus {
                version: "3".to_string(),
                canonical_tip_height: 42,
                canonical_tip_hash: "a".repeat(64),
                peer_count: 2,
                recovery_state: "normal".to_string(),
            })
        }

        fn validated_identity_fingerprint(&self) -> Result<[u8; 32], WalletCoreClientError> {
            let call = self.identity_calls.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some((target, runtime)) = &self.revoke_on_identity_call {
                if call == *target {
                    runtime.invalidate_all().unwrap();
                }
            }
            if (matches!(self.mode, ResponseMode::CoreReplacementBeforeWrite) && call >= 10)
                || (matches!(self.mode, ResponseMode::CoreReplacementAfterWrite)
                    && self.writes.load(Ordering::SeqCst) > 0)
                || (matches!(self.mode, ResponseMode::CoreReplacementDuringRestartLookup)
                    && call >= 2)
            {
                return Ok([0x43; 32]);
            }
            Ok(self.fingerprint)
        }
    }

    impl WalletCoreReceiptSource for FakeSubmissionCore {
        fn transaction_lookup(
            &self,
            transaction_id: &str,
        ) -> Result<Zeroizing<Vec<u8>>, WalletCoreClientError> {
            if matches!(self.mode, ResponseMode::PanicDuringLookup) {
                self.request_ledger.record(
                    "GET",
                    format!("/transactions/{transaction_id}"),
                    transaction_id,
                    &[],
                    "panic",
                );
                panic!("injected restart reconciliation lookup panic");
            }
            let body = self.lookup_body.lock().unwrap().clone();
            match body {
                Some(body) => {
                    let outcome = serde_json::from_slice::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|value| value.get("found").and_then(serde_json::Value::as_bool))
                        .map_or(
                            "malformed",
                            |found| if found { "found" } else { "not_found" },
                        );
                    self.request_ledger.record(
                        "GET",
                        format!("/transactions/{transaction_id}"),
                        transaction_id,
                        &[],
                        outcome,
                    );
                    Ok(Zeroizing::new(body))
                }
                None => {
                    self.request_ledger.record(
                        "GET",
                        format!("/transactions/{transaction_id}"),
                        transaction_id,
                        &[],
                        "transport_failed",
                    );
                    Err(WalletCoreClientError::TransportFailed)
                }
            }
        }
    }

    impl WalletCoreSubmissionSource for FakeSubmissionCore {
        fn submit_once(
            &self,
            _authority: CoreWriteOnce,
            exact_body: &[u8],
        ) -> Result<WalletCoreHttpResponse, WalletCoreClientError> {
            let previous = self.writes.fetch_add(1, Ordering::SeqCst);
            assert_eq!(previous, 0, "the write capability was reused");
            let transaction: VisionTransaction = serde_json::from_slice(exact_body).unwrap();
            let tx_id = canonical_transaction_id(&transaction).unwrap();
            if matches!(self.mode, ResponseMode::PanicDuringWrite) {
                self.request_ledger
                    .record("POST", "/transactions", &tx_id, exact_body, "panic");
                panic!("injected private submission write panic");
            }
            if let Some(path) = &self.journal_failure_path {
                std::fs::create_dir(path).unwrap();
            }
            if !matches!(self.mode, ResponseMode::TransportFailure) {
                *self.lookup_body.lock().unwrap() = Some(
                    serde_json::to_vec(&serde_json::json!({
                        "tx_id": tx_id,
                        "found": true,
                        "block_hash": null,
                        "block_height": null,
                        "tx_index": null,
                        "tx": transaction,
                    }))
                    .unwrap(),
                );
            }
            if matches!(
                self.mode,
                ResponseMode::TransportFailure | ResponseMode::AcceptedResponseLost
            ) {
                self.request_ledger.record(
                    "POST",
                    "/transactions",
                    &tx_id,
                    exact_body,
                    "transport_failed",
                );
                return Err(WalletCoreClientError::TransportFailed);
            }
            if matches!(self.mode, ResponseMode::Malformed) {
                self.request_ledger.record(
                    "POST",
                    "/transactions",
                    &tx_id,
                    exact_body,
                    "malformed_response",
                );
                return Ok(WalletCoreHttpResponse {
                    status: 200,
                    body: Zeroizing::new(br#"{"unknown":true}"#.to_vec()),
                });
            }
            let (status, code) = match self.mode {
                ResponseMode::Accepted
                | ResponseMode::CoreReplacementBeforeWrite
                | ResponseMode::CoreReplacementAfterWrite
                | ResponseMode::PanicDuringLookup
                | ResponseMode::CoreReplacementDuringRestartLookup => ("accepted", None),
                ResponseMode::Duplicate => ("rejected", Some("duplicate_canonical_tx_id")),
                ResponseMode::Rejected => ("rejected", Some("stale_nonce")),
                ResponseMode::TransportFailure
                | ResponseMode::AcceptedResponseLost
                | ResponseMode::Malformed => unreachable!(),
                ResponseMode::PanicDuringWrite => unreachable!(),
            };
            let body = if let Some(code) = code {
                serde_json::json!({
                    "status": status,
                    "tx_id": tx_id,
                    "current_nonce": transaction.nonce,
                    "error": {"code": code, "message": "duplicate"}
                })
            } else {
                serde_json::json!({
                    "status": status,
                    "tx_id": tx_id,
                    "current_nonce": transaction.nonce,
                    "decision": {"kind": "accept"}
                })
            };
            self.request_ledger.record(
                "POST",
                "/transactions",
                &tx_id,
                exact_body,
                if code.is_some() {
                    "rejected"
                } else {
                    "accepted"
                },
            );
            Ok(WalletCoreHttpResponse {
                status: if code.is_some() { 422 } else { 200 },
                body: Zeroizing::new(serde_json::to_vec(&body).unwrap()),
            })
        }
    }

    fn unlocked_runtime() -> (Arc<WalletRuntimeState>, String) {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let seed = WalletSeed::for_test(0x41);
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

    fn pending<'a>(
        runtime: &'a WalletRuntimeState,
        sender: &str,
        recipient: &str,
        writes: Arc<AtomicUsize>,
        lookup_body: Arc<Mutex<Option<Vec<u8>>>>,
        mode: ResponseMode,
    ) -> PendingTransferConfirmation<'a, FakeSubmissionCore> {
        pending_with_options(
            runtime,
            sender,
            recipient,
            writes,
            lookup_body,
            mode,
            None,
            None,
            TestRequestLedger::default(),
        )
    }

    fn pending_with_ledger<'a>(
        runtime: &'a WalletRuntimeState,
        sender: &str,
        recipient: &str,
        writes: Arc<AtomicUsize>,
        lookup_body: Arc<Mutex<Option<Vec<u8>>>>,
        mode: ResponseMode,
        request_ledger: TestRequestLedger,
    ) -> PendingTransferConfirmation<'a, FakeSubmissionCore> {
        pending_with_options(
            runtime,
            sender,
            recipient,
            writes,
            lookup_body,
            mode,
            None,
            None,
            request_ledger,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn pending_with_revocation<'a>(
        runtime: &'a WalletRuntimeState,
        sender: &str,
        recipient: &str,
        writes: Arc<AtomicUsize>,
        lookup_body: Arc<Mutex<Option<Vec<u8>>>>,
        mode: ResponseMode,
        revoke_on_identity_call: Option<(usize, Arc<WalletRuntimeState>)>,
    ) -> PendingTransferConfirmation<'a, FakeSubmissionCore> {
        pending_with_options(
            runtime,
            sender,
            recipient,
            writes,
            lookup_body,
            mode,
            revoke_on_identity_call,
            None,
            TestRequestLedger::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn pending_with_options<'a>(
        runtime: &'a WalletRuntimeState,
        sender: &str,
        recipient: &str,
        writes: Arc<AtomicUsize>,
        lookup_body: Arc<Mutex<Option<Vec<u8>>>>,
        mode: ResponseMode,
        revoke_on_identity_call: Option<(usize, Arc<WalletRuntimeState>)>,
        journal_failure_path: Option<std::path::PathBuf>,
        request_ledger: TestRequestLedger,
    ) -> PendingTransferConfirmation<'a, FakeSubmissionCore> {
        let identity_calls = Arc::new(AtomicUsize::new(0));
        let account_nonce = Arc::new(AtomicU64::new(7));
        let prepare_source = FakeSubmissionCore {
            address: sender.to_string(),
            writes: writes.clone(),
            mode,
            fingerprint: [0x42; 32],
            identity_calls: identity_calls.clone(),
            revoke_on_identity_call: revoke_on_identity_call.clone(),
            lookup_body: lookup_body.clone(),
            journal_failure_path: journal_failure_path.clone(),
            account_nonce: Arc::clone(&account_nonce),
            request_ledger: request_ledger.clone(),
        };
        let request: WalletTransferPreviewRequest = serde_json::from_value(serde_json::json!({
            "recipient": recipient,
            "amount": "2.5"
        }))
        .unwrap();
        let prepare_permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        let preview =
            prepare_with_source_for_test(&prepare_permit, request, &prepare_source).unwrap();
        let consume_permit: WalletOperationPermit<'a> = runtime
            .begin_operation(MAIN, WalletOperationKind::ConsumePreview)
            .unwrap();
        let intent = consume_permit
            .consume_transaction_preview(&preview.handle)
            .unwrap();
        bind_consumed_preview_for_test(
            consume_permit,
            intent,
            FakeSubmissionCore {
                address: sender.to_string(),
                writes,
                mode,
                fingerprint: [0x42; 32],
                identity_calls,
                revoke_on_identity_call,
                lookup_body,
                journal_failure_path,
                account_nonce,
                request_ledger,
            },
        )
        .unwrap()
    }

    fn custody(directory: &tempfile::TempDir) -> WalletCustodyPathAuthority {
        crate::wallet::storage_security::protect_directory(directory.path()).unwrap();
        WalletCustodyPathAuthority::issue_for_test(&directory.path().join("wallet.vault.json"))
    }

    #[test]
    fn accepted_attempt_writes_once_records_activity_and_resolves_store() {
        let (runtime, sender) = unlocked_runtime();
        let recipient = "b".repeat(64);
        let writes = Arc::new(AtomicUsize::new(0));
        let lookup_body = Arc::new(Mutex::new(None));
        let pending = pending(
            &runtime,
            &sender,
            &recipient,
            writes.clone(),
            lookup_body,
            ResponseMode::Accepted,
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let journal_path = custody.journal_path().to_path_buf();
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(result, PrivateSubmissionResult::Accepted { .. }));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert!(journal_path.exists());
        let record = fs_read_record(&directory);
        assert_eq!(record["phase"]["kind"], "resolved_recorded");
    }

    #[test]
    fn journal_collision_rechecked_immediately_before_envelope_publication_prevents_write() {
        let (runtime, sender) = unlocked_runtime();
        let recipient = "c".repeat(64);
        let writes = Arc::new(AtomicUsize::new(0));
        let pending = pending(
            &runtime,
            &sender,
            &recipient,
            writes.clone(),
            Arc::new(Mutex::new(None)),
            ResponseMode::Accepted,
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let seed = WalletSeed::for_test(0x41);
        let transaction = sign_cash_transfer_for_test(
            &seed,
            &CashTransferDraft {
                nonce: 7,
                recipient,
                amount_raw_units: 2_500_000_000,
                tip_raw_units: 0,
                fee_limit_raw_units: 201,
            },
        )
        .unwrap();
        let transaction_id = canonical_transaction_id(&transaction).unwrap();
        let journal_authenticator = WalletJournalAuthenticator::new("primary", &seed).unwrap();
        append_accepted_submission(
            custody.journal_path(),
            &journal_authenticator,
            &transaction,
            &WalletSubmissionOutcome::Accepted {
                tx_id: transaction_id,
                current_nonce: 7,
            },
            1,
        )
        .unwrap();

        assert!(super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            2,
            &SubmissionRejectionPolicy::production(),
        )
        .is_err());
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        assert!(!directory
            .path()
            .join("wallet.signed-envelopes.v1.enc")
            .exists());
    }

    #[test]
    fn proven_acceptance_with_journal_failure_remains_recording_pending() {
        let (runtime, sender) = unlocked_runtime();
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let writes = Arc::new(AtomicUsize::new(0));
        let pending = pending_with_options(
            &runtime,
            &sender,
            &"b".repeat(64),
            writes.clone(),
            Arc::new(Mutex::new(None)),
            ResponseMode::Accepted,
            None,
            Some(custody.journal_path().to_path_buf()),
            TestRequestLedger::default(),
        );
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(
            result,
            PrivateSubmissionResult::AcceptedRecordingPending { .. }
        ));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "accepted_recording_pending"
        );
        assert!(!runtime.lifecycle_status(true).unwrap().locked);
    }

    #[test]
    fn transport_failure_and_duplicate_response_remain_ambiguous_without_retry() {
        for mode in [
            ResponseMode::TransportFailure,
            ResponseMode::AcceptedResponseLost,
            ResponseMode::Malformed,
            ResponseMode::Duplicate,
        ] {
            let (runtime, sender) = unlocked_runtime();
            let writes = Arc::new(AtomicUsize::new(0));
            let lookup_body = Arc::new(Mutex::new(None));
            let pending = pending(
                &runtime,
                &sender,
                &"b".repeat(64),
                writes.clone(),
                lookup_body.clone(),
                mode,
            );
            let directory = tempfile::tempdir().unwrap();
            let custody = custody(&directory);
            let journal_path = custody.journal_path().to_path_buf();
            let result = super::super::sign_and_submit_after_native_approval(
                pending,
                NativeConfirmationApproval::issue_for_test(),
                &custody,
                1_700_000_000_123,
                &SubmissionRejectionPolicy::production(),
            )
            .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
            assert!(matches!(
                result,
                PrivateSubmissionResult::OutcomeUnknown { .. }
            ));
            assert_eq!(writes.load(Ordering::SeqCst), 1);
            assert!(!journal_path.exists());
            let record = fs_read_record(&directory);
            assert_eq!(record["phase"]["kind"], "may_have_been_submitted");
            if !matches!(mode, ResponseMode::TransportFailure) {
                let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
                let restart = reconciliation.discover(&custody).unwrap().unwrap();
                let source = FakeSubmissionCore {
                    address: sender.clone(),
                    writes: writes.clone(),
                    mode,
                    // Restart reconciliation deliberately accepts a newer, independently
                    // validated Core generation while retaining the original fingerprint as
                    // authenticated provenance.
                    fingerprint: [0x43; 32],
                    identity_calls: Arc::new(AtomicUsize::new(0)),
                    revoke_on_identity_call: None,
                    lookup_body,
                    journal_failure_path: None,
                    account_nonce: Arc::new(AtomicU64::new(7)),
                    request_ledger: TestRequestLedger::default(),
                };
                assert!(matches!(
                    reconciliation
                        .reconcile_ambiguous_acceptance(&custody, restart, &source)
                        .unwrap(),
                    WalletReconciliationResult::ResolvedRecorded
                ));
                reconciliation.complete(()).unwrap();
                assert!(journal_path.exists());
                assert_eq!(
                    fs_read_record(&directory)["phase"]["kind"],
                    "resolved_recorded"
                );
            }
        }
    }

    #[test]
    fn reconciled_exact_acceptance_with_journal_failure_stays_accepted_recording_pending() {
        let (runtime, sender) = unlocked_runtime();
        let writes = Arc::new(AtomicUsize::new(0));
        let lookup_body = Arc::new(Mutex::new(None));
        let pending = pending(
            &runtime,
            &sender,
            &"b".repeat(64),
            writes.clone(),
            lookup_body.clone(),
            ResponseMode::AcceptedResponseLost,
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(
            result,
            PrivateSubmissionResult::OutcomeUnknown { .. }
        ));
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "may_have_been_submitted"
        );
        std::fs::create_dir(custody.journal_path()).unwrap();

        let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let restart = reconciliation.discover(&custody).unwrap().unwrap();
        let source = FakeSubmissionCore {
            address: sender,
            writes: writes.clone(),
            mode: ResponseMode::AcceptedResponseLost,
            fingerprint: [0x43; 32],
            identity_calls: Arc::new(AtomicUsize::new(0)),
            revoke_on_identity_call: None,
            lookup_body,
            journal_failure_path: None,
            account_nonce: Arc::new(AtomicU64::new(7)),
            request_ledger: TestRequestLedger::default(),
        };
        assert!(matches!(
            reconciliation
                .reconcile_ambiguous_acceptance(&custody, restart, &source)
                .unwrap(),
            WalletReconciliationResult::AcceptedRecordingPending
        ));
        reconciliation.complete(()).unwrap();
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "accepted_recording_pending"
        );
    }

    #[test]
    fn restart_lookup_rejects_a_core_generation_change_during_the_read() {
        let (runtime, sender) = unlocked_runtime();
        let writes = Arc::new(AtomicUsize::new(0));
        let lookup_body = Arc::new(Mutex::new(None));
        let pending = pending(
            &runtime,
            &sender,
            &"b".repeat(64),
            writes.clone(),
            lookup_body.clone(),
            ResponseMode::AcceptedResponseLost,
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(
            result,
            PrivateSubmissionResult::OutcomeUnknown { .. }
        ));

        let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let restart = reconciliation.discover(&custody).unwrap().unwrap();
        let source = FakeSubmissionCore {
            address: sender,
            writes,
            mode: ResponseMode::CoreReplacementDuringRestartLookup,
            fingerprint: [0x55; 32],
            identity_calls: Arc::new(AtomicUsize::new(0)),
            revoke_on_identity_call: None,
            lookup_body,
            journal_failure_path: None,
            account_nonce: Arc::new(AtomicU64::new(7)),
            request_ledger: TestRequestLedger::default(),
        };
        assert_eq!(
            reconciliation
                .reconcile_ambiguous_acceptance(&custody, restart, &source)
                .err(),
            Some(WalletRuntimeError::ReconciliationUnavailable)
        );
        drop(reconciliation);
        assert!(runtime.lifecycle_status(true).unwrap().locked);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "may_have_been_submitted"
        );
    }

    #[test]
    fn custody_authority_derives_all_submission_files_from_one_directory() {
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let store = ReconciliationStore::for_custody(&custody).unwrap();
        assert_eq!(
            custody.vault_path().parent(),
            custody.journal_path().parent()
        );
        assert_eq!(
            custody
                .journal_path()
                .file_name()
                .and_then(|value| value.to_str()),
            Some("wallet.activity.json")
        );
        assert!(store.is_bound_to(&custody));
    }

    #[test]
    fn only_an_explicit_nonmutating_allowlist_can_resolve_a_rejection() {
        let (runtime, sender) = unlocked_runtime();
        let writes = Arc::new(AtomicUsize::new(0));
        let pending = pending(
            &runtime,
            &sender,
            &"b".repeat(64),
            writes.clone(),
            Arc::new(Mutex::new(None)),
            ResponseMode::Rejected,
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let journal_path = custody.journal_path().to_path_buf();
        let policy = SubmissionRejectionPolicy::for_test(&[(
            422,
            crate::wallet::submission::WalletSubmissionRejection::StaleNonce,
        )]);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &policy,
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(result, PrivateSubmissionResult::Rejected { .. }));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "resolved_rejected"
        );
        assert!(!journal_path.exists());
    }

    #[test]
    fn repeated_not_found_after_observed_nonce_movement_never_retries() {
        let (runtime, sender) = unlocked_runtime();
        let writes = Arc::new(AtomicUsize::new(0));
        let lookup_body = Arc::new(Mutex::new(None));
        let request_ledger = TestRequestLedger::default();
        let recipient = "b".repeat(64);
        let pending = pending_with_ledger(
            &runtime,
            &sender,
            &recipient,
            writes.clone(),
            lookup_body.clone(),
            ResponseMode::TransportFailure,
            request_ledger.clone(),
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let journal_path = custody.journal_path().to_path_buf();
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        let transaction_id = match result {
            PrivateSubmissionResult::OutcomeUnknown { transaction_id } => transaction_id,
            _ => panic!("transport ambiguity must remain outcome_unknown"),
        };
        *lookup_body.lock().unwrap() = Some(
            serde_json::to_vec(&serde_json::json!({
                "tx_id": &transaction_id,
                "found": false,
                "block_hash": null,
                "block_height": null,
                "tx_index": null,
                "tx": null
            }))
            .unwrap(),
        );
        let source = FakeSubmissionCore {
            address: sender,
            writes: writes.clone(),
            mode: ResponseMode::TransportFailure,
            fingerprint: [0x42; 32],
            identity_calls: Arc::new(AtomicUsize::new(0)),
            revoke_on_identity_call: None,
            lookup_body,
            journal_failure_path: None,
            // A later canonical nonce must never justify retrying an ambiguous envelope.
            account_nonce: Arc::new(AtomicU64::new(99)),
            request_ledger: request_ledger.clone(),
        };
        let advanced_account = source.account_snapshot(&source.address).unwrap();
        assert_eq!(advanced_account.nonce, 99);

        // Canonical nonce movement cannot resolve an ambiguous signed envelope. Only
        // authenticated, read-only exact-envelope lookup is permitted after the write attempt.
        for _ in 0..3 {
            let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
            let restart = reconciliation.discover(&custody).unwrap().unwrap();
            assert!(matches!(
                reconciliation
                    .reconcile_ambiguous_acceptance(&custody, restart, &source)
                    .unwrap(),
                WalletReconciliationResult::Unresolved
            ));
            reconciliation.complete(()).unwrap();
        }

        let expected_transaction = sign_cash_transfer_for_test(
            &WalletSeed::for_test(0x41),
            &CashTransferDraft {
                nonce: 7,
                recipient,
                amount_raw_units: 2_500_000_000,
                tip_raw_units: 0,
                fee_limit_raw_units: 201,
            },
        )
        .unwrap();
        let expected_body = serde_json::to_vec(&expected_transaction).unwrap();
        request_ledger.assert_single_post_for_intent(&transaction_id, &expected_body);
        let lookups: Vec<_> = request_ledger
            .snapshot()
            .into_iter()
            .filter(|record| record.method == "GET" && record.transaction_id == transaction_id)
            .collect();
        assert_eq!(lookups.len(), 3);
        assert!(lookups
            .iter()
            .enumerate()
            .all(|(index, record)| record.attempt_number == index + 1
                && record.outcome == "not_found"));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "may_have_been_submitted"
        );
        assert!(!journal_path.exists());
    }

    #[test]
    fn restart_permit_completes_accepted_recording_without_write_authority() {
        let (runtime, sender) = unlocked_runtime();
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let journal_path = custody.journal_path().to_path_buf();

        let writes = Arc::new(AtomicUsize::new(0));
        let pending = pending_with_options(
            &runtime,
            &sender,
            &"b".repeat(64),
            writes.clone(),
            Arc::new(Mutex::new(None)),
            ResponseMode::Accepted,
            None,
            Some(journal_path.clone()),
            TestRequestLedger::default(),
        );
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(
            result,
            PrivateSubmissionResult::AcceptedRecordingPending { .. }
        ));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        std::fs::remove_dir(&journal_path).unwrap();
        let recording = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let accepted = recording.discover(&custody).unwrap().unwrap();
        recording
            .complete_accepted_recording(&custody, accepted)
            .unwrap();
        recording.complete(()).unwrap();
        assert!(journal_path.exists());
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "resolved_recorded"
        );
    }

    #[test]
    fn panic_after_durable_ambiguity_returns_outcome_unknown_and_revokes_the_wallet_session() {
        let (runtime, sender) = unlocked_runtime();
        let writes = Arc::new(AtomicUsize::new(0));
        let pending = pending(
            &runtime,
            &sender,
            &"b".repeat(64),
            writes.clone(),
            Arc::new(Mutex::new(None)),
            ResponseMode::PanicDuringWrite,
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        );
        assert!(matches!(
            result,
            Ok(PrivateSubmissionResult::OutcomeUnknown { .. })
        ));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "may_have_been_submitted"
        );
        assert!(runtime.lifecycle_status(true).unwrap().locked);
    }

    #[test]
    fn core_replacement_before_or_after_the_write_suppresses_success_without_retry() {
        for (mode, expected_writes) in [
            (ResponseMode::CoreReplacementBeforeWrite, 0),
            (ResponseMode::CoreReplacementAfterWrite, 1),
        ] {
            let (runtime, sender) = unlocked_runtime();
            let writes = Arc::new(AtomicUsize::new(0));
            let pending = pending(
                &runtime,
                &sender,
                &"b".repeat(64),
                writes.clone(),
                Arc::new(Mutex::new(None)),
                mode,
            );
            let directory = tempfile::tempdir().unwrap();
            let custody = custody(&directory);
            let journal_path = custody.journal_path().to_path_buf();
            let result = super::super::sign_and_submit_after_native_approval(
                pending,
                NativeConfirmationApproval::issue_for_test(),
                &custody,
                1_700_000_000_123,
                &SubmissionRejectionPolicy::production(),
            )
            .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
            assert!(matches!(
                result,
                PrivateSubmissionResult::OutcomeUnknown { .. }
            ));
            assert_eq!(writes.load(Ordering::SeqCst), expected_writes);
            assert_eq!(
                fs_read_record(&directory)["phase"]["kind"],
                "may_have_been_submitted"
            );
            assert!(!journal_path.exists());
        }
    }

    #[test]
    fn lifecycle_revocation_reports_the_authenticated_durable_phase() {
        for (identity_call, expected_writes, expected_phase) in [
            (9, 0, "prepared"),
            (10, 0, "may_have_been_submitted"),
            (11, 1, "may_have_been_submitted"),
            (12, 1, "resolved_recorded"),
        ] {
            let (runtime, sender) = unlocked_runtime();
            let writes = Arc::new(AtomicUsize::new(0));
            let pending = pending_with_revocation(
                &runtime,
                &sender,
                &"b".repeat(64),
                writes.clone(),
                Arc::new(Mutex::new(None)),
                ResponseMode::Accepted,
                Some((identity_call, Arc::clone(&runtime))),
            );
            let directory = tempfile::tempdir().unwrap();
            let custody = custody(&directory);
            let result = super::super::sign_and_submit_after_native_approval(
                pending,
                NativeConfirmationApproval::issue_for_test(),
                &custody,
                1_700_000_000_123,
                &SubmissionRejectionPolicy::production(),
            );
            match expected_phase {
                "prepared" => assert!(matches!(
                    result,
                    Err(crate::wallet::signing::WalletPrivateSigningError::RuntimeRevoked)
                )),
                "may_have_been_submitted" => assert!(matches!(
                    result,
                    Ok(PrivateSubmissionResult::OutcomeUnknown { .. })
                )),
                "resolved_recorded" => assert!(matches!(
                    result,
                    Ok(PrivateSubmissionResult::Accepted { .. })
                )),
                _ => panic!("unexpected durable phase"),
            }
            assert_eq!(writes.load(Ordering::SeqCst), expected_writes);
            assert_eq!(fs_read_record(&directory)["phase"]["kind"], expected_phase);
            assert!(runtime.lifecycle_status(true).unwrap().locked);
        }
    }

    #[test]
    fn restart_lookup_panic_is_contained_revokes_runtime_and_preserves_ambiguity() {
        let (runtime, sender) = unlocked_runtime();
        let writes = Arc::new(AtomicUsize::new(0));
        let lookup_body = Arc::new(Mutex::new(None));
        let pending = pending(
            &runtime,
            &sender,
            &"b".repeat(64),
            writes.clone(),
            lookup_body.clone(),
            ResponseMode::TransportFailure,
        );
        let directory = tempfile::tempdir().unwrap();
        let custody = custody(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &custody,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(
            result,
            PrivateSubmissionResult::OutcomeUnknown { .. }
        ));

        let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let restart = reconciliation.discover(&custody).unwrap().unwrap();
        let source = FakeSubmissionCore {
            address: sender,
            writes,
            mode: ResponseMode::PanicDuringLookup,
            fingerprint: [0x42; 32],
            identity_calls: Arc::new(AtomicUsize::new(0)),
            revoke_on_identity_call: None,
            lookup_body,
            journal_failure_path: None,
            account_nonce: Arc::new(AtomicU64::new(7)),
            request_ledger: TestRequestLedger::default(),
        };
        assert_eq!(
            reconciliation
                .reconcile_ambiguous_acceptance(&custody, restart, &source)
                .err(),
            Some(WalletRuntimeError::RuntimeUnavailable)
        );
        drop(reconciliation);
        assert!(runtime.lifecycle_status(true).unwrap().locked);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "may_have_been_submitted"
        );
    }

    fn fs_read_record(directory: &tempfile::TempDir) -> serde_json::Value {
        serde_json::from_slice(
            &std::fs::read(
                directory
                    .path()
                    .join("wallet.submission-reconciliation.json"),
            )
            .unwrap(),
        )
        .unwrap()
    }

    const fn signing_error_name(
        error: crate::wallet::signing::WalletPrivateSigningError,
    ) -> &'static str {
        use crate::wallet::signing::WalletPrivateSigningError::*;
        match error {
            PreviewUnavailable => "preview",
            RuntimeRevoked => "runtime",
            ActivationUnavailable => "activation",
            CoreUnavailable => "core",
            IntentRejected => "intent",
            SignatureUnavailable => "signature",
            SubmissionUnavailable => "submission",
        }
    }
}
