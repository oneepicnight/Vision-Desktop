use super::SignedTransferArtifact;
use crate::wallet::{
    core_client::{
        WalletCoreSubmissionSource, SUPPORTED_STATUS_VERSION, SUPPORTED_WALLET_CORE_CONTRACT,
    },
    reconciliation::{ReconciliationRecord, ReconciliationStore},
    runtime::{WalletRuntimeError, WalletSubmissionPermit},
    submission::{
        classify_submission_response, PrivateSubmissionResponseDisposition,
        SubmissionRejectionPolicy,
    },
    transaction::canonical_transaction_id,
};
use serde::Deserialize;
use std::path::Path;
use zeroize::Zeroizing;

const MAX_SIGNED_BODY_BYTES: usize = 64 * 1024;
const BODY_DIGEST_CONTEXT: &str = "com.vision.desktop.wallet-signed-envelope-digest.v1";
const CONTRACT_DIGEST_CONTEXT: &str = "com.vision.desktop.wallet-submission-contract.v1";
const PRIVATE_SUBMISSION_BOUNDARY_CONTRACT: &str = "vision-wallet-private-submission-v1";

pub(in crate::wallet) enum PrivateSubmissionResult {
    Accepted,
    Rejected,
    OutcomeUnknown,
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

pub(super) fn submit_signed_artifact<S: WalletCoreSubmissionSource>(
    mut permit: WalletSubmissionPermit<'_>,
    artifact: SignedTransferArtifact,
    source: &S,
    vault_path: &Path,
    journal_path: &Path,
    created_at_unix_ms: u64,
    rejection_policy: &SubmissionRejectionPolicy,
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
    let store = ReconciliationStore::for_vault_path(vault_path)
        .map_err(|_| PrivateSubmissionError::ReconciliationUnavailable)?;
    let record = ReconciliationRecord::prepared(
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
        hex::encode(artifact.core_identity_fingerprint),
        created_at_unix_ms,
    );

    let grant = permit.take_activation_grant().map_err(map_runtime_error)?;
    let (live, write) = grant.split();
    let prepared = permit
        .publish_prepared(live, &store, record)
        .map_err(map_runtime_error)?;
    if validate_authority(&permit, &artifact, source).is_err() {
        permit
            .resolve_not_attempted(prepared, &store)
            .map_err(map_runtime_error)?;
        return permit
            .complete(PrivateSubmissionResult::OutcomeUnknown)
            .map_err(map_runtime_error);
    }
    let may_have = permit
        .publish_may_have_been_submitted(prepared, &store)
        .map_err(map_runtime_error)?;
    let write_ready = may_have.combine(write);
    let (may_have, write_once) = write_ready.into_parts();

    if validate_authority(&permit, &artifact, source).is_err() {
        drop(may_have);
        return complete_with_core_validation(
            permit,
            &artifact,
            source,
            PrivateSubmissionResult::OutcomeUnknown,
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
                PrivateSubmissionResult::OutcomeUnknown,
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
            PrivateSubmissionResult::OutcomeUnknown,
        );
    }

    match disposition {
        PrivateSubmissionResponseDisposition::Accepted {
            transaction_id,
            nonce,
        } => {
            let accepted = permit
                .publish_accepted(
                    may_have,
                    &store,
                    transaction_id,
                    nonce,
                    compatibility_contract_digest(rejection_policy),
                )
                .map_err(map_runtime_error)?;
            let evidence = accepted
                .evidence()
                .map_err(|_| PrivateSubmissionError::ReconciliationUnavailable)?;
            permit
                .record_accepted_evidence(journal_path, &evidence)
                .map_err(map_runtime_error)?;
            permit
                .resolve_recorded(accepted, &store)
                .map_err(map_runtime_error)?;
            complete_with_core_validation(
                permit,
                &artifact,
                source,
                PrivateSubmissionResult::Accepted,
            )
        }
        PrivateSubmissionResponseDisposition::DefinitiveRejected {
            http_status,
            code,
            allowlist_digest_hex,
        } => {
            permit
                .resolve_rejected(
                    may_have,
                    &store,
                    http_status,
                    code.as_str().to_string(),
                    allowlist_digest_hex,
                )
                .map_err(map_runtime_error)?;
            complete_with_core_validation(
                permit,
                &artifact,
                source,
                PrivateSubmissionResult::Rejected,
            )
        }
        PrivateSubmissionResponseDisposition::OutcomeUnknown => {
            drop(may_have);
            complete_with_core_validation(
                permit,
                &artifact,
                source,
                PrivateSubmissionResult::OutcomeUnknown,
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
        PrivateSubmissionResult::OutcomeUnknown
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

fn compatibility_contract_digest(rejection_policy: &SubmissionRejectionPolicy) -> String {
    let mut hasher = blake3::Hasher::new_derive_key(CONTRACT_DIGEST_CONTEXT);
    let rejection_digest = rejection_policy.digest_hex();
    for value in [
        PRIVATE_SUBMISSION_BOUNDARY_CONTRACT,
        SUPPORTED_WALLET_CORE_CONTRACT,
        SUPPORTED_STATUS_VERSION,
        rejection_digest.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
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
            WalletCoreReadSource, WalletCoreStatus,
        },
        preview::{
            bind_consumed_preview_for_test, prepare_with_source_for_test,
            PendingTransferConfirmation,
        },
        public_request::WalletTransferPreviewRequest,
        reconciliation::{
            publish_accepted_for_test, publish_prepared_for_test, CoreWriteOnce,
            ReconciliationAuthenticator, ReconciliationRecord, ReconciliationStore,
        },
        runtime::{WalletOperationKind, WalletOperationPermit, WalletRuntimeState},
        secrets::{WalletPassword, WalletSeed},
        submission::SubmissionRejectionPolicy,
        transaction::VisionTransaction,
        transaction_confirmation::NativeConfirmationApproval,
        vault::EncryptedWalletVault,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
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
    }

    struct FakeSubmissionCore {
        address: String,
        writes: Arc<AtomicUsize>,
        mode: ResponseMode,
        fingerprint: [u8; 32],
        identity_calls: Arc<AtomicUsize>,
        revoke_on_identity_call: Option<(usize, Arc<WalletRuntimeState>)>,
        lookup_body: Arc<Mutex<Option<Vec<u8>>>>,
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
                nonce: 7,
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
            {
                return Ok([0x43; 32]);
            }
            Ok(self.fingerprint)
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
            if matches!(self.mode, ResponseMode::PanicDuringWrite) {
                panic!("injected private submission write panic");
            }
            let transaction: VisionTransaction = serde_json::from_slice(exact_body).unwrap();
            let tx_id = canonical_transaction_id(&transaction).unwrap();
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
                return Err(WalletCoreClientError::TransportFailed);
            }
            if matches!(self.mode, ResponseMode::Malformed) {
                return Ok(WalletCoreHttpResponse {
                    status: 200,
                    body: Zeroizing::new(br#"{"unknown":true}"#.to_vec()),
                });
            }
            let (status, code) = match self.mode {
                ResponseMode::Accepted
                | ResponseMode::CoreReplacementBeforeWrite
                | ResponseMode::CoreReplacementAfterWrite
                | ResponseMode::PanicDuringLookup => ("accepted", None),
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
            Ok(WalletCoreHttpResponse {
                status: if code.is_some() { 422 } else { 200 },
                body: Zeroizing::new(serde_json::to_vec(&body).unwrap()),
            })
        }

        fn transaction_lookup(
            &self,
            _transaction_id: &str,
        ) -> Result<Zeroizing<Vec<u8>>, WalletCoreClientError> {
            if matches!(self.mode, ResponseMode::PanicDuringLookup) {
                panic!("injected restart reconciliation lookup panic");
            }
            self.lookup_body
                .lock()
                .unwrap()
                .clone()
                .map(Zeroizing::new)
                .ok_or(WalletCoreClientError::TransportFailed)
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
        pending_with_revocation(runtime, sender, recipient, writes, lookup_body, mode, None)
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
        let identity_calls = Arc::new(AtomicUsize::new(0));
        let prepare_source = FakeSubmissionCore {
            address: sender.to_string(),
            writes: writes.clone(),
            mode,
            fingerprint: [0x42; 32],
            identity_calls: identity_calls.clone(),
            revoke_on_identity_call: revoke_on_identity_call.clone(),
            lookup_body: lookup_body.clone(),
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
            },
        )
        .unwrap()
    }

    fn paths(directory: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
        crate::wallet::storage_security::protect_directory(directory.path()).unwrap();
        (
            directory.path().join("wallet.vault.json"),
            directory.path().join("wallet.activity.json"),
        )
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
        let (vault_path, journal_path) = paths(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &vault_path,
            &journal_path,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(result, PrivateSubmissionResult::Accepted));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert!(journal_path.exists());
        let record = fs_read_record(&directory);
        assert_eq!(record["phase"]["kind"], "resolved_recorded");
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
            let (vault_path, journal_path) = paths(&directory);
            let result = super::super::sign_and_submit_after_native_approval(
                pending,
                NativeConfirmationApproval::issue_for_test(),
                &vault_path,
                &journal_path,
                1_700_000_000_123,
                &SubmissionRejectionPolicy::production(),
            )
            .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
            assert!(matches!(result, PrivateSubmissionResult::OutcomeUnknown));
            assert_eq!(writes.load(Ordering::SeqCst), 1);
            assert!(!journal_path.exists());
            let record = fs_read_record(&directory);
            assert_eq!(record["phase"]["kind"], "may_have_been_submitted");
            if !matches!(mode, ResponseMode::TransportFailure) {
                let store = ReconciliationStore::for_vault_path(&vault_path).unwrap();
                let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
                let restart = reconciliation.discover(&store).unwrap().unwrap();
                let source = FakeSubmissionCore {
                    address: sender.clone(),
                    writes: writes.clone(),
                    mode,
                    fingerprint: [0x42; 32],
                    identity_calls: Arc::new(AtomicUsize::new(0)),
                    revoke_on_identity_call: None,
                    lookup_body,
                };
                assert!(reconciliation
                    .reconcile_ambiguous_acceptance(&store, &journal_path, restart, &source,)
                    .unwrap());
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
        let (vault_path, journal_path) = paths(&directory);
        let policy = SubmissionRejectionPolicy::for_test(&[(
            422,
            crate::wallet::submission::WalletSubmissionRejection::StaleNonce,
        )]);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &vault_path,
            &journal_path,
            1_700_000_000_123,
            &policy,
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(result, PrivateSubmissionResult::Rejected));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "resolved_rejected"
        );
        assert!(!journal_path.exists());
    }

    #[test]
    fn restart_not_found_remains_ambiguous_and_never_writes_again() {
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
        let (vault_path, journal_path) = paths(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &vault_path,
            &journal_path,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(result, PrivateSubmissionResult::OutcomeUnknown));
        let record = fs_read_record(&directory);
        let transaction_id = record["transaction_id"].as_str().unwrap();
        *lookup_body.lock().unwrap() = Some(
            serde_json::to_vec(&serde_json::json!({
                "tx_id": transaction_id,
                "found": false,
                "block_hash": null,
                "block_height": null,
                "tx_index": null,
                "tx": null
            }))
            .unwrap(),
        );
        let store = ReconciliationStore::for_vault_path(&vault_path).unwrap();
        let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let restart = reconciliation.discover(&store).unwrap().unwrap();
        let source = FakeSubmissionCore {
            address: sender,
            writes: writes.clone(),
            mode: ResponseMode::TransportFailure,
            fingerprint: [0x42; 32],
            identity_calls: Arc::new(AtomicUsize::new(0)),
            revoke_on_identity_call: None,
            lookup_body,
        };
        assert!(!reconciliation
            .reconcile_ambiguous_acceptance(&store, &journal_path, restart, &source)
            .unwrap());
        reconciliation.complete(()).unwrap();
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "may_have_been_submitted"
        );
        assert!(!journal_path.exists());
    }

    #[test]
    fn restart_permit_resolves_prepared_and_completes_accepted_recording_without_write_authority() {
        let (runtime, sender) = unlocked_runtime();
        let directory = tempfile::tempdir().unwrap();
        let (vault_path, journal_path) = paths(&directory);
        let store = ReconciliationStore::for_vault_path(&vault_path).unwrap();
        let seed = WalletSeed::for_test(0x41);
        let authenticator = ReconciliationAuthenticator::new("primary", &seed).unwrap();

        publish_prepared_for_test(&store, &authenticator, reconciliation_record(&sender, "11"))
            .unwrap();
        let cleanup = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let prepared = cleanup.discover(&store).unwrap().unwrap();
        cleanup.resolve_prepared(&store, prepared).unwrap();
        cleanup.complete(()).unwrap();
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "resolved_not_attempted"
        );

        publish_accepted_for_test(&store, &authenticator, reconciliation_record(&sender, "77"))
            .unwrap();
        let recording = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let accepted = recording.discover(&store).unwrap().unwrap();
        recording
            .complete_accepted_recording(&store, &journal_path, accepted)
            .unwrap();
        recording.complete(()).unwrap();
        assert!(journal_path.exists());
        assert_eq!(
            fs_read_record(&directory)["phase"]["kind"],
            "resolved_recorded"
        );
    }

    #[test]
    fn panic_after_durable_ambiguity_is_contained_and_revokes_the_wallet_session() {
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
        let (vault_path, journal_path) = paths(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &vault_path,
            &journal_path,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        );
        assert!(matches!(
            result,
            Err(crate::wallet::signing::WalletPrivateSigningError::RuntimeRevoked)
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
            let (vault_path, journal_path) = paths(&directory);
            let result = super::super::sign_and_submit_after_native_approval(
                pending,
                NativeConfirmationApproval::issue_for_test(),
                &vault_path,
                &journal_path,
                1_700_000_000_123,
                &SubmissionRejectionPolicy::production(),
            )
            .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
            assert!(matches!(result, PrivateSubmissionResult::OutcomeUnknown));
            assert_eq!(writes.load(Ordering::SeqCst), expected_writes);
            assert_eq!(
                fs_read_record(&directory)["phase"]["kind"],
                "may_have_been_submitted"
            );
            assert!(!journal_path.exists());
        }
    }

    #[test]
    fn lifecycle_revocation_at_each_submission_transition_suppresses_result_and_preserves_phase() {
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
            let (vault_path, journal_path) = paths(&directory);
            let result = super::super::sign_and_submit_after_native_approval(
                pending,
                NativeConfirmationApproval::issue_for_test(),
                &vault_path,
                &journal_path,
                1_700_000_000_123,
                &SubmissionRejectionPolicy::production(),
            );
            assert!(matches!(
                result,
                Err(crate::wallet::signing::WalletPrivateSigningError::RuntimeRevoked)
            ));
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
        let (vault_path, journal_path) = paths(&directory);
        let result = super::super::sign_and_submit_after_native_approval(
            pending,
            NativeConfirmationApproval::issue_for_test(),
            &vault_path,
            &journal_path,
            1_700_000_000_123,
            &SubmissionRejectionPolicy::production(),
        )
        .unwrap_or_else(|error| panic!("submission failed: {}", signing_error_name(error)));
        assert!(matches!(result, PrivateSubmissionResult::OutcomeUnknown));

        let store = ReconciliationStore::for_vault_path(&vault_path).unwrap();
        let reconciliation = runtime.begin_reconciliation_discovery(MAIN).unwrap();
        let restart = reconciliation.discover(&store).unwrap().unwrap();
        let source = FakeSubmissionCore {
            address: sender,
            writes,
            mode: ResponseMode::PanicDuringLookup,
            fingerprint: [0x42; 32],
            identity_calls: Arc::new(AtomicUsize::new(0)),
            revoke_on_identity_call: None,
            lookup_body,
        };
        assert_eq!(
            reconciliation
                .reconcile_ambiguous_acceptance(&store, &journal_path, restart, &source)
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

    fn reconciliation_record(sender: &str, attempt_byte: &str) -> ReconciliationRecord {
        ReconciliationRecord::prepared(
            "primary".to_string(),
            attempt_byte.repeat(32),
            "22".repeat(32),
            sender.to_string(),
            "b".repeat(64),
            "250000000".to_string(),
            7,
            0,
            201,
            "44".repeat(32),
            "42".repeat(32),
            1_700_000_000_123,
        )
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
