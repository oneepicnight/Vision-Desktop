use super::{
    core_client::{
        WalletCoreClientError, WalletCoreReadClient, WalletCoreReceiptSource,
        SUPPORTED_STATUS_VERSION,
    },
    journal::WalletActivityRecord,
    lifecycle::WalletCustodyPathAuthority,
    receipt::{
        classify_receipt_change, parse_exact_signed_receipt_observation, WalletReceiptChange,
    },
    runtime::{
        PreparedReceiptRefresh, WalletReceiptRefreshPermit, WalletRuntimeError, WalletRuntimeState,
    },
};
use crate::supervisor::SupervisorState;
use std::panic::{catch_unwind, AssertUnwindSafe};

const NORMAL_RECOVERY_STATE: &str = "normal";

pub(in crate::wallet) struct PrivateReceiptRefreshResult {
    pub record: WalletActivityRecord,
    pub change: WalletReceiptChange,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletReceiptRefreshError {
    Runtime(WalletRuntimeError),
    TransactionUnknown,
    ActivityUnavailable,
    CoreCompatibilityUnavailable,
    CoreUnavailable,
    CoreRecovering,
    CoreResponseRejected,
}

pub(in crate::wallet) struct WalletReceiptRefreshEngine<'runtime> {
    runtime: &'runtime WalletRuntimeState,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReceiptRefreshCheckpoint {
    BeforePermit,
    PermitAcquired,
    LocalEligibilityAuthenticated,
    CoreAuthorityIssued,
    InitialIdentityValidated,
    StatusRead,
    StatusIdentityValidated,
    LookupRead,
    LookupIdentityValidated,
    ReceiptParsed,
    FinalIdentityValidated,
    BeforeJournalWrite,
    JournalRecorded,
    BeforeComplete,
}

trait ReceiptRefreshObserver {
    fn checkpoint(&self, _checkpoint: ReceiptRefreshCheckpoint) {}
}

struct ProductionReceiptRefreshObserver;

impl ReceiptRefreshObserver for ProductionReceiptRefreshObserver {}

impl<'runtime> WalletReceiptRefreshEngine<'runtime> {
    pub(in crate::wallet) const fn new(runtime: &'runtime WalletRuntimeState) -> Self {
        Self { runtime }
    }

    pub(in crate::wallet) fn refresh(
        &self,
        supervisor: &SupervisorState,
        owner_window: &str,
        custody: &WalletCustodyPathAuthority,
        transaction_id: &str,
        observed_at_unix_ms: u64,
    ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError> {
        self.refresh_with_factory_and_observer(
            owner_window,
            custody,
            transaction_id,
            observed_at_unix_ms,
            || WalletCoreReadClient::from_supervisor(supervisor),
            &ProductionReceiptRefreshObserver,
        )
    }

    #[cfg(test)]
    fn refresh_with_source(
        &self,
        owner_window: &str,
        custody: &WalletCustodyPathAuthority,
        transaction_id: &str,
        observed_at_unix_ms: u64,
        source: &impl WalletCoreReceiptSource,
    ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError> {
        self.run_contained(|| {
            let observer = ProductionReceiptRefreshObserver;
            let (permit, prepared) =
                self.prepare_local(owner_window, custody, transaction_id, &observer)?;
            observer.checkpoint(ReceiptRefreshCheckpoint::CoreAuthorityIssued);
            self.refresh_prepared(
                permit,
                prepared,
                custody,
                observed_at_unix_ms,
                source,
                &observer,
            )
        })
    }

    fn run_contained(
        &self,
        operation: impl FnOnce() -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError>,
    ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError> {
        let attempt = catch_unwind(AssertUnwindSafe(operation));
        match attempt {
            Ok(result) => result,
            Err(_) => {
                let _ = self.runtime.invalidate_all();
                Err(WalletReceiptRefreshError::Runtime(
                    WalletRuntimeError::RuntimeUnavailable,
                ))
            }
        }
    }

    fn prepare_local<'a>(
        &'a self,
        owner_window: &str,
        custody: &WalletCustodyPathAuthority,
        transaction_id: &str,
        observer: &impl ReceiptRefreshObserver,
    ) -> Result<(WalletReceiptRefreshPermit<'a>, PreparedReceiptRefresh), WalletReceiptRefreshError>
    {
        observer.checkpoint(ReceiptRefreshCheckpoint::BeforePermit);
        let permit = self
            .runtime
            .begin_receipt_refresh(owner_window)
            .map_err(map_runtime_error)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::PermitAcquired);
        let prepared = permit
            .prepare(custody, transaction_id)
            .map_err(map_runtime_error)?
            .ok_or(WalletReceiptRefreshError::TransactionUnknown)?;
        permit.ensure_current().map_err(map_runtime_error)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::LocalEligibilityAuthenticated);
        Ok((permit, prepared))
    }

    fn refresh_prepared(
        &self,
        permit: WalletReceiptRefreshPermit<'_>,
        prepared: PreparedReceiptRefresh,
        custody: &WalletCustodyPathAuthority,
        observed_at_unix_ms: u64,
        source: &impl WalletCoreReceiptSource,
        observer: &impl ReceiptRefreshObserver,
    ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError> {
        let initial_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(map_core_error)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::InitialIdentityValidated);
        let status = source.status().map_err(map_core_error)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::StatusRead);
        permit.ensure_current().map_err(map_runtime_error)?;
        let status_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(map_core_error)?;
        if status_fingerprint != initial_fingerprint || status.version != SUPPORTED_STATUS_VERSION {
            return Err(WalletReceiptRefreshError::CoreCompatibilityUnavailable);
        }
        if status.recovery_state != NORMAL_RECOVERY_STATE {
            return Err(WalletReceiptRefreshError::CoreRecovering);
        }
        observer.checkpoint(ReceiptRefreshCheckpoint::StatusIdentityValidated);

        let body = source
            .transaction_lookup(prepared.transaction_id())
            .map_err(map_core_error)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::LookupRead);
        permit.ensure_current().map_err(map_runtime_error)?;
        let lookup_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(map_core_error)?;
        if lookup_fingerprint != initial_fingerprint {
            return Err(WalletReceiptRefreshError::CoreUnavailable);
        }
        observer.checkpoint(ReceiptRefreshCheckpoint::LookupIdentityValidated);
        let observation = parse_exact_signed_receipt_observation(
            body.as_slice(),
            prepared.transaction(),
            prepared.signed_body_digest_hex(),
            status.canonical_tip_height,
        )
        .map_err(|_| WalletReceiptRefreshError::CoreResponseRejected)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::ReceiptParsed);
        permit.ensure_current().map_err(map_runtime_error)?;
        let final_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(map_core_error)?;
        if final_fingerprint != initial_fingerprint {
            return Err(WalletReceiptRefreshError::CoreUnavailable);
        }
        observer.checkpoint(ReceiptRefreshCheckpoint::FinalIdentityValidated);
        let change = classify_receipt_change(Some(prepared.previous_observation()), &observation);
        permit.ensure_current().map_err(map_runtime_error)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::BeforeJournalWrite);
        let record = permit
            .record_observation(custody, prepared, &observation, observed_at_unix_ms)
            .map_err(map_runtime_error)?;
        observer.checkpoint(ReceiptRefreshCheckpoint::JournalRecorded);
        observer.checkpoint(ReceiptRefreshCheckpoint::BeforeComplete);
        permit
            .complete(PrivateReceiptRefreshResult { record, change })
            .map_err(map_runtime_error)
    }

    fn refresh_with_factory_and_observer<S>(
        &self,
        owner_window: &str,
        custody: &WalletCustodyPathAuthority,
        transaction_id: &str,
        observed_at_unix_ms: u64,
        source_factory: impl FnOnce() -> Result<S, WalletCoreClientError>,
        observer: &impl ReceiptRefreshObserver,
    ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError>
    where
        S: WalletCoreReceiptSource,
    {
        self.run_contained(|| {
            let (permit, prepared) =
                self.prepare_local(owner_window, custody, transaction_id, observer)?;
            let source = source_factory().map_err(map_core_error)?;
            observer.checkpoint(ReceiptRefreshCheckpoint::CoreAuthorityIssued);
            self.refresh_prepared(
                permit,
                prepared,
                custody,
                observed_at_unix_ms,
                &source,
                observer,
            )
        })
    }
}

fn map_runtime_error(error: WalletRuntimeError) -> WalletReceiptRefreshError {
    match error {
        WalletRuntimeError::ReconciliationUnavailable => {
            WalletReceiptRefreshError::ActivityUnavailable
        }
        _ => WalletReceiptRefreshError::Runtime(error),
    }
}

fn map_core_error(error: WalletCoreClientError) -> WalletReceiptRefreshError {
    match error {
        WalletCoreClientError::CompatibilityUnavailable => {
            WalletReceiptRefreshError::CoreCompatibilityUnavailable
        }
        WalletCoreClientError::ResponseRejected
        | WalletCoreClientError::ResponseTooLarge
        | WalletCoreClientError::AccountIdentityMismatch
        | WalletCoreClientError::AccountStateRejected
        | WalletCoreClientError::InvalidAddress => WalletReceiptRefreshError::CoreResponseRejected,
        WalletCoreClientError::CoreUnavailable
        | WalletCoreClientError::CoreIdentityChanged
        | WalletCoreClientError::PeerIdentityRejected
        | WalletCoreClientError::TransportFailed => WalletReceiptRefreshError::CoreUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        account::derive_account_identity,
        core_client::{WalletCoreAccountSnapshot, WalletCoreReadSource, WalletCoreStatus},
        envelope_store::{EnvelopeEntryInput, EnvelopeStore, EnvelopeStoreAuthenticator},
        journal::{
            append_accepted_evidence, fail_next_receipt_append_for_test, load_activity_journal,
            WalletJournalAuthenticator,
        },
        receipt::WalletReceiptObservation,
        reconciliation::{
            AcceptedSubmissionEvidence, ReconciliationAuthenticator, ReconciliationStore,
        },
        secrets::{WalletPassword, WalletSeed},
        storage_security,
        submission::{compatibility_contract_digest, SubmissionRejectionPolicy},
        test_request_ledger::TestRequestLedger,
        transaction::{
            canonical_transaction_id, sign_cash_transfer_for_test, CashTransferDraft,
            VisionTransaction,
        },
        vault::EncryptedWalletVault,
    };
    use std::{
        cell::{Cell, RefCell},
        fs,
        sync::{Arc, LazyLock, Mutex, MutexGuard},
    };
    use zeroize::Zeroizing;

    const MAIN: &str = "main";
    const PASSWORD: &str = "correct horse battery staple";
    const SUBMITTED_AT: u64 = 1_700_000_000_000;
    static FIXTURE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct Fixture {
        _fixture_guard: MutexGuard<'static, ()>,
        _directory: tempfile::TempDir,
        custody: WalletCustodyPathAuthority,
        runtime: Arc<WalletRuntimeState>,
        seed: WalletSeed,
        transaction: VisionTransaction,
        transaction_id: String,
        request_ledger: TestRequestLedger,
    }

    struct FakeReceiptSource {
        body: RefCell<Vec<u8>>,
        identity_calls: Cell<usize>,
        lookup_calls: Cell<usize>,
        change_identity_at: Cell<Option<usize>>,
        panic_on_status: Cell<bool>,
        revoke_on_identity_call: Cell<Option<usize>>,
        revoke_on_status: Cell<bool>,
        revoke_on_lookup: Cell<bool>,
        revocation_runtime: RefCell<Option<Arc<WalletRuntimeState>>>,
        recovery_state: RefCell<String>,
        tip_height: Cell<u64>,
        identity_error_at: Cell<Option<usize>>,
        status_error: Cell<bool>,
        lookup_error: Cell<bool>,
        request_ledger: TestRequestLedger,
    }

    struct PanicObserver {
        checkpoint: ReceiptRefreshCheckpoint,
    }

    struct ActionObserver {
        checkpoint: ReceiptRefreshCheckpoint,
        action: RefCell<Option<Box<dyn FnOnce()>>>,
    }

    impl ReceiptRefreshObserver for PanicObserver {
        fn checkpoint(&self, checkpoint: ReceiptRefreshCheckpoint) {
            assert_ne!(checkpoint, self.checkpoint, "injected refresh-stage panic");
        }
    }

    impl ReceiptRefreshObserver for ActionObserver {
        fn checkpoint(&self, checkpoint: ReceiptRefreshCheckpoint) {
            if checkpoint == self.checkpoint {
                if let Some(action) = self.action.borrow_mut().take() {
                    action();
                }
            }
        }
    }

    impl FakeReceiptSource {
        fn new(body: Vec<u8>) -> Self {
            Self::with_ledger(body, TestRequestLedger::default())
        }

        fn with_ledger(body: Vec<u8>, request_ledger: TestRequestLedger) -> Self {
            Self {
                body: RefCell::new(body),
                identity_calls: Cell::new(0),
                lookup_calls: Cell::new(0),
                change_identity_at: Cell::new(None),
                panic_on_status: Cell::new(false),
                revoke_on_identity_call: Cell::new(None),
                revoke_on_status: Cell::new(false),
                revoke_on_lookup: Cell::new(false),
                revocation_runtime: RefCell::new(None),
                recovery_state: RefCell::new("normal".to_string()),
                tip_height: Cell::new(100),
                identity_error_at: Cell::new(None),
                status_error: Cell::new(false),
                lookup_error: Cell::new(false),
                request_ledger,
            }
        }

        fn replace_body(&self, body: Vec<u8>) {
            *self.body.borrow_mut() = body;
        }

        fn revoke_with(&self, runtime: &Arc<WalletRuntimeState>) {
            *self.revocation_runtime.borrow_mut() = Some(Arc::clone(runtime));
        }

        fn revoke_now(&self) {
            if let Some(runtime) = self.revocation_runtime.borrow().as_ref() {
                runtime.invalidate_all().unwrap();
            }
        }
    }

    impl WalletCoreReadSource for FakeReceiptSource {
        fn account_snapshot(
            &self,
            _address: &str,
        ) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError> {
            panic!("receipt refresh must not query account state")
        }

        fn status(&self) -> Result<WalletCoreStatus, WalletCoreClientError> {
            assert!(!self.panic_on_status.get(), "injected status panic");
            if self.status_error.get() {
                return Err(WalletCoreClientError::CoreUnavailable);
            }
            if self.revoke_on_status.get() {
                self.revoke_now();
            }
            Ok(WalletCoreStatus {
                version: SUPPORTED_STATUS_VERSION.to_string(),
                canonical_tip_height: self.tip_height.get(),
                canonical_tip_hash: "11".repeat(32),
                peer_count: 1,
                recovery_state: self.recovery_state.borrow().clone(),
            })
        }

        fn validated_identity_fingerprint(&self) -> Result<[u8; 32], WalletCoreClientError> {
            let call = self.identity_calls.get() + 1;
            self.identity_calls.set(call);
            if self.identity_error_at.get() == Some(call) {
                return Err(WalletCoreClientError::CoreIdentityChanged);
            }
            if self.revoke_on_identity_call.get() == Some(call) {
                self.revoke_now();
            }
            Ok(if self.change_identity_at.get() == Some(call) {
                [0x99; 32]
            } else {
                [0x31; 32]
            })
        }
    }

    impl WalletCoreReceiptSource for FakeReceiptSource {
        fn transaction_lookup(
            &self,
            transaction_id: &str,
        ) -> Result<Zeroizing<Vec<u8>>, WalletCoreClientError> {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            if self.lookup_error.get() {
                self.request_ledger.record(
                    "GET",
                    format!("/transactions/{transaction_id}"),
                    transaction_id,
                    &[],
                    "transport_failed",
                );
                return Err(WalletCoreClientError::TransportFailed);
            }
            if self.revoke_on_lookup.get() {
                self.revoke_now();
            }
            let body = self.body.borrow().clone();
            let outcome = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    let found = value.get("found")?.as_bool()?;
                    if !found {
                        return Some("not_found");
                    }
                    Some(
                        if value.get("block_hash").is_some_and(|hash| !hash.is_null()) {
                            "mined"
                        } else {
                            "pending"
                        },
                    )
                })
                .unwrap_or("malformed");
            self.request_ledger.record(
                "GET",
                format!("/transactions/{transaction_id}"),
                transaction_id,
                &[],
                outcome,
            );
            Ok(Zeroizing::new(body))
        }
    }

    impl Fixture {
        fn new() -> Self {
            let fixture_guard = FIXTURE_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let directory = tempfile::tempdir().unwrap();
            storage_security::protect_directory(directory.path()).unwrap();
            let custody = WalletCustodyPathAuthority::issue_for_test(
                &directory.path().join("wallet.vault.json"),
            );
            let seed = WalletSeed::for_test(0x31);
            let identity = derive_account_identity(&seed);
            let transaction = sign_cash_transfer_for_test(
                &seed,
                &CashTransferDraft {
                    nonce: 7,
                    recipient: "22".repeat(32),
                    amount_raw_units: 42,
                    tip_raw_units: 0,
                    fee_limit_raw_units: 201,
                },
            )
            .unwrap();
            assert_eq!(transaction.sender_pubkey, identity.address);
            let transaction_id = canonical_transaction_id(&transaction).unwrap();
            let exact_body = Zeroizing::new(serde_json::to_vec(&transaction).unwrap());
            let request_ledger = TestRequestLedger::default();
            request_ledger.record(
                "POST",
                "/transactions",
                &transaction_id,
                exact_body.as_slice(),
                "accepted",
            );
            let mut body_hasher = blake3::Hasher::new_derive_key(
                "com.vision.desktop.wallet-signed-envelope-digest.v1",
            );
            body_hasher.update(exact_body.as_slice());
            let body_digest = body_hasher.finalize().to_hex().to_string();

            let envelope_store = EnvelopeStore::for_custody(&custody).unwrap();
            let envelope_authenticator = EnvelopeStoreAuthenticator::new("primary", &seed).unwrap();
            let reconciliation_store = ReconciliationStore::for_custody(&custody).unwrap();
            let reconciliation_authenticator =
                ReconciliationAuthenticator::new("primary", &seed).unwrap();
            let reservation = reconciliation_store
                .reserve_prepared(&reconciliation_authenticator)
                .unwrap();
            let compatibility =
                compatibility_contract_digest(&SubmissionRejectionPolicy::production());
            let prepared = envelope_store
                .publish_prepared(
                    &envelope_authenticator,
                    EnvelopeEntryInput {
                        wallet_id: "primary",
                        attempt_id: &"aa".repeat(32),
                        transaction_id: &transaction_id,
                        transaction: &transaction,
                        exact_body: exact_body.as_slice(),
                        signed_body_digest_hex: &body_digest,
                        compatibility_contract_digest_hex: &compatibility,
                        created_at_unix_ms: SUBMITTED_AT,
                    },
                    &reservation,
                )
                .unwrap();
            let ambiguous = envelope_store
                .mark_ambiguous(&envelope_authenticator, prepared)
                .unwrap();
            let accepted = envelope_store
                .mark_accepted(&envelope_authenticator, ambiguous)
                .unwrap();
            let journal_authenticator = WalletJournalAuthenticator::new("primary", &seed).unwrap();
            let evidence = AcceptedSubmissionEvidence::for_receipt_refresh_test(
                "primary".to_string(),
                transaction_id.clone(),
                identity.address,
                "22".repeat(32),
                "42".to_string(),
                7,
                0,
                201,
                accepted.commitment_hex().to_string(),
                SUBMITTED_AT,
            );
            append_accepted_evidence(custody.journal_path(), &journal_authenticator, &evidence)
                .unwrap();

            let runtime = Arc::new(WalletRuntimeState::for_test());
            let password = WalletPassword::for_test(PASSWORD);
            let vault =
                EncryptedWalletVault::encrypt_for_test("primary", SUBMITTED_AT, &seed, &password)
                    .unwrap();
            let unlock = runtime
                .begin_operation(MAIN, crate::wallet::runtime::WalletOperationKind::Unlock)
                .unwrap();
            let status = unlock
                .run_authorized(|activation| runtime.unlock_vault(activation, &vault, &password))
                .unwrap()
                .unwrap();
            unlock.complete(status).unwrap();
            drop(unlock);

            Self {
                _fixture_guard: fixture_guard,
                _directory: directory,
                custody,
                runtime,
                seed,
                transaction,
                transaction_id,
                request_ledger,
            }
        }

        fn record(&self) -> WalletActivityRecord {
            let auth = WalletJournalAuthenticator::new("primary", &self.seed).unwrap();
            load_activity_journal(self.custody.journal_path(), &auth)
                .unwrap()
                .records()[0]
                .clone()
        }

        fn refresh(
            &self,
            source: &FakeReceiptSource,
            observed_at: u64,
        ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError> {
            WalletReceiptRefreshEngine::new(&self.runtime).refresh_with_source(
                MAIN,
                &self.custody,
                &self.transaction_id,
                observed_at,
                source,
            )
        }
    }

    fn lookup_body(
        transaction: &VisionTransaction,
        transaction_id: &str,
        observation: &WalletReceiptObservation,
    ) -> Vec<u8> {
        match observation {
            WalletReceiptObservation::NotFound => serde_json::to_vec(&serde_json::json!({
                "tx_id": transaction_id,
                "found": false,
                "block_hash": null,
                "block_height": null,
                "tx_index": null,
                "tx": null
            }))
            .unwrap(),
            WalletReceiptObservation::Pending => serde_json::to_vec(&serde_json::json!({
                "tx_id": transaction_id,
                "found": true,
                "block_hash": null,
                "block_height": null,
                "tx_index": null,
                "tx": transaction
            }))
            .unwrap(),
            WalletReceiptObservation::Mined {
                block_hash,
                block_height,
                tx_index,
                ..
            } => serde_json::to_vec(&serde_json::json!({
                "tx_id": transaction_id,
                "found": true,
                "block_hash": block_hash,
                "block_height": block_height,
                "tx_index": tx_index,
                "tx": transaction
            }))
            .unwrap(),
        }
    }

    #[test]
    fn receipt_state_machine_records_pending_mined_advancement_reorg_and_loss() {
        let fixture = Fixture::new();
        let pending = WalletReceiptObservation::Pending;
        let source = FakeReceiptSource::with_ledger(
            lookup_body(&fixture.transaction, &fixture.transaction_id, &pending),
            fixture.request_ledger.clone(),
        );
        let first = fixture.refresh(&source, SUBMITTED_AT + 1).unwrap();
        assert_eq!(first.change, WalletReceiptChange::FirstObservation);
        assert_eq!(first.record.observation, pending);

        let mined = WalletReceiptObservation::Mined {
            block_hash: "33".repeat(32),
            block_height: 90,
            tx_index: 2,
            confirmations: 11,
        };
        source.replace_body(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &mined,
        ));
        let mined_result = fixture.refresh(&source, SUBMITTED_AT + 2).unwrap();
        assert_eq!(mined_result.change, WalletReceiptChange::PendingToMined);

        source.tip_height.set(110);
        let advanced = WalletReceiptObservation::Mined {
            block_hash: "33".repeat(32),
            block_height: 90,
            tx_index: 2,
            confirmations: 21,
        };
        source.replace_body(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &advanced,
        ));
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 3).unwrap().change,
            WalletReceiptChange::ConfirmationsAdvanced
        );

        let reorganized = WalletReceiptObservation::Mined {
            block_hash: "44".repeat(32),
            block_height: 100,
            tx_index: 1,
            confirmations: 11,
        };
        source.replace_body(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &reorganized,
        ));
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 4).unwrap().change,
            WalletReceiptChange::Reorganized
        );

        source.replace_body(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &WalletReceiptObservation::Pending,
        ));
        assert_eq!(
            fixture
                .refresh(&source, SUBMITTED_AT + 60_000)
                .unwrap()
                .change,
            WalletReceiptChange::Reorganized
        );

        source.replace_body(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &WalletReceiptObservation::NotFound,
        ));
        assert_eq!(
            fixture
                .refresh(&source, SUBMITTED_AT + 86_400_000)
                .unwrap()
                .change,
            WalletReceiptChange::ObservationLost
        );
        assert_eq!(
            fixture
                .refresh(&source, SUBMITTED_AT + 172_800_000)
                .unwrap()
                .change,
            WalletReceiptChange::Unchanged
        );

        let exact_body = serde_json::to_vec(&fixture.transaction).unwrap();
        fixture
            .request_ledger
            .assert_single_post_for_intent(&fixture.transaction_id, &exact_body);
        let requests = fixture.request_ledger.snapshot();
        let lookups: Vec<_> = requests
            .iter()
            .filter(|record| {
                record.method == "GET" && record.transaction_id == fixture.transaction_id
            })
            .collect();
        assert_eq!(
            lookups
                .iter()
                .map(|record| record.outcome)
                .collect::<Vec<_>>(),
            vec![
                "pending",
                "mined",
                "mined",
                "mined",
                "pending",
                "not_found",
                "not_found",
            ]
        );
        assert!(lookups
            .iter()
            .enumerate()
            .all(|(index, record)| record.attempt_number == index + 1
                && record.route == format!("/transactions/{}", fixture.transaction_id)));
        assert_eq!(source.lookup_calls.get(), 7);
    }

    #[test]
    fn unchanged_observation_does_not_grow_or_retimestamp_journal() {
        let fixture = Fixture::new();
        let source = FakeReceiptSource::new(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &WalletReceiptObservation::NotFound,
        ));
        let before = fs::read(fixture.custody.journal_path()).unwrap();
        let result = fixture.refresh(&source, SUBMITTED_AT + 99).unwrap();
        let after = fs::read(fixture.custody.journal_path()).unwrap();
        assert_eq!(result.change, WalletReceiptChange::Unchanged);
        assert_eq!(result.record.last_observed_at_unix_ms, None);
        assert_eq!(before, after);
    }

    #[test]
    fn wrong_envelope_malformed_receipt_and_core_replacement_fail_closed() {
        let fixture = Fixture::new();
        let source = FakeReceiptSource::new(b"{}".to_vec());
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 1).err(),
            Some(WalletReceiptRefreshError::CoreResponseRejected)
        );
        assert_eq!(
            fixture.record().observation,
            WalletReceiptObservation::NotFound
        );

        let mut foreign = fixture.transaction.clone();
        foreign.sig = "00".repeat(64);
        source.replace_body(lookup_body(
            &foreign,
            &fixture.transaction_id,
            &WalletReceiptObservation::Pending,
        ));
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 2).err(),
            Some(WalletReceiptRefreshError::CoreResponseRejected)
        );

        source.replace_body(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &WalletReceiptObservation::Pending,
        ));
        source
            .change_identity_at
            .set(Some(source.identity_calls.get() + 3));
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 3).err(),
            Some(WalletReceiptRefreshError::CoreUnavailable)
        );
        assert_eq!(
            fixture.record().observation,
            WalletReceiptObservation::NotFound
        );
    }

    #[test]
    fn contention_unknown_recovery_and_panic_never_queue_or_write() {
        let fixture = Fixture::new();
        let source = FakeReceiptSource::new(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &WalletReceiptObservation::Pending,
        ));
        let held = fixture.runtime.begin_receipt_refresh(MAIN).unwrap();
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 1).err(),
            Some(WalletReceiptRefreshError::Runtime(
                WalletRuntimeError::OperationInProgress
            ))
        );
        assert_eq!(source.lookup_calls.get(), 0);
        drop(held);

        let engine = WalletReceiptRefreshEngine::new(&fixture.runtime);
        assert_eq!(
            engine
                .refresh_with_source(
                    MAIN,
                    &fixture.custody,
                    &"ff".repeat(32),
                    SUBMITTED_AT + 2,
                    &source,
                )
                .err(),
            Some(WalletReceiptRefreshError::TransactionUnknown)
        );
        assert_eq!(source.lookup_calls.get(), 0);

        *source.recovery_state.borrow_mut() = "recovering".to_string();
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 3).err(),
            Some(WalletReceiptRefreshError::CoreRecovering)
        );
        assert_eq!(source.lookup_calls.get(), 0);

        *source.recovery_state.borrow_mut() = "normal".to_string();
        source.panic_on_status.set(true);
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 4).err(),
            Some(WalletReceiptRefreshError::Runtime(
                WalletRuntimeError::RuntimeUnavailable
            ))
        );
        assert_eq!(
            fixture.runtime.begin_receipt_refresh(MAIN).err(),
            Some(WalletRuntimeError::RuntimeUnavailable)
        );
    }

    #[test]
    fn lifecycle_revocation_fails_closed_at_every_core_checkpoint() {
        enum Checkpoint {
            InitialIdentity,
            Status,
            Lookup,
            PostLookupIdentity,
            FinalIdentity,
        }
        for checkpoint in [
            Checkpoint::InitialIdentity,
            Checkpoint::Status,
            Checkpoint::Lookup,
            Checkpoint::PostLookupIdentity,
            Checkpoint::FinalIdentity,
        ] {
            let fixture = Fixture::new();
            let source = FakeReceiptSource::new(lookup_body(
                &fixture.transaction,
                &fixture.transaction_id,
                &WalletReceiptObservation::Pending,
            ));
            source.revoke_with(&fixture.runtime);
            match checkpoint {
                Checkpoint::InitialIdentity => source.revoke_on_identity_call.set(Some(1)),
                Checkpoint::Status => source.revoke_on_status.set(true),
                Checkpoint::Lookup => source.revoke_on_lookup.set(true),
                Checkpoint::PostLookupIdentity => source.revoke_on_identity_call.set(Some(3)),
                Checkpoint::FinalIdentity => source.revoke_on_identity_call.set(Some(4)),
            }
            assert_eq!(
                fixture.refresh(&source, SUBMITTED_AT + 1).err(),
                Some(WalletReceiptRefreshError::Runtime(
                    WalletRuntimeError::RuntimeUnavailable
                ))
            );
            assert_eq!(
                fixture.record().observation,
                WalletReceiptObservation::NotFound
            );
        }
    }

    #[test]
    fn journal_write_failure_preserves_last_authenticated_observation() {
        let fixture = Fixture::new();
        let source = FakeReceiptSource::new(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &WalletReceiptObservation::Pending,
        ));
        let before = fs::read(fixture.custody.journal_path()).unwrap();
        fail_next_receipt_append_for_test();
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 1).err(),
            Some(WalletReceiptRefreshError::ActivityUnavailable)
        );
        assert_eq!(fs::read(fixture.custody.journal_path()).unwrap(), before);
        assert_eq!(
            fixture.record().observation,
            WalletReceiptObservation::NotFound
        );
    }

    #[test]
    fn local_eligibility_failures_never_request_core_authority() {
        fn pending_source() -> FakeReceiptSource {
            FakeReceiptSource::new(Vec::new())
        }

        let fixture = Fixture::new();
        let calls = Cell::new(0);
        let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
            .refresh_with_factory_and_observer(
                MAIN,
                &fixture.custody,
                &"ff".repeat(32),
                SUBMITTED_AT + 1,
                || {
                    calls.set(calls.get() + 1);
                    Ok(pending_source())
                },
                &ProductionReceiptRefreshObserver,
            );
        assert_eq!(
            result.err(),
            Some(WalletReceiptRefreshError::TransactionUnknown)
        );
        assert_eq!(calls.get(), 0);
        drop(fixture);

        let fixture = Fixture::new();
        for name in [
            "wallet.signed-envelopes.v1.enc",
            "wallet.signed-envelopes.v1.head.json",
        ] {
            fs::remove_file(fixture._directory.path().join(name)).unwrap();
        }
        let calls = Cell::new(0);
        let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
            .refresh_with_factory_and_observer(
                MAIN,
                &fixture.custody,
                &fixture.transaction_id,
                SUBMITTED_AT + 2,
                || {
                    calls.set(calls.get() + 1);
                    Ok(pending_source())
                },
                &ProductionReceiptRefreshObserver,
            );
        assert_eq!(
            result.err(),
            Some(WalletReceiptRefreshError::ActivityUnavailable)
        );
        assert_eq!(calls.get(), 0);
        drop(fixture);

        let fixture = Fixture::new();
        fs::write(
            fixture
                ._directory
                .path()
                .join("wallet.signed-envelopes.v1.enc"),
            b"{}",
        )
        .unwrap();
        let calls = Cell::new(0);
        let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
            .refresh_with_factory_and_observer(
                MAIN,
                &fixture.custody,
                &fixture.transaction_id,
                SUBMITTED_AT + 3,
                || {
                    calls.set(calls.get() + 1);
                    Ok(pending_source())
                },
                &ProductionReceiptRefreshObserver,
            );
        assert_eq!(
            result.err(),
            Some(WalletReceiptRefreshError::ActivityUnavailable)
        );
        assert_eq!(calls.get(), 0);
        drop(fixture);

        let fixture = Fixture::new();
        fs::write(fixture.custody.journal_path(), b"{}").unwrap();
        let calls = Cell::new(0);
        let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
            .refresh_with_factory_and_observer(
                MAIN,
                &fixture.custody,
                &fixture.transaction_id,
                SUBMITTED_AT + 4,
                || {
                    calls.set(calls.get() + 1);
                    Ok(pending_source())
                },
                &ProductionReceiptRefreshObserver,
            );
        assert_eq!(
            result.err(),
            Some(WalletReceiptRefreshError::ActivityUnavailable)
        );
        assert_eq!(calls.get(), 0);
        drop(fixture);

        let fixture = Fixture::new();
        let held = fixture.runtime.begin_receipt_refresh(MAIN).unwrap();
        let calls = Cell::new(0);
        let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
            .refresh_with_factory_and_observer(
                MAIN,
                &fixture.custody,
                &fixture.transaction_id,
                SUBMITTED_AT + 5,
                || {
                    calls.set(calls.get() + 1);
                    Ok(pending_source())
                },
                &ProductionReceiptRefreshObserver,
            );
        assert_eq!(
            result.err(),
            Some(WalletReceiptRefreshError::Runtime(
                WalletRuntimeError::OperationInProgress
            ))
        );
        assert_eq!(calls.get(), 0);
        drop(held);
    }

    #[test]
    fn every_refresh_stage_panic_is_contained_and_revokes_authority() {
        for checkpoint in [
            ReceiptRefreshCheckpoint::BeforePermit,
            ReceiptRefreshCheckpoint::PermitAcquired,
            ReceiptRefreshCheckpoint::LocalEligibilityAuthenticated,
            ReceiptRefreshCheckpoint::CoreAuthorityIssued,
            ReceiptRefreshCheckpoint::InitialIdentityValidated,
            ReceiptRefreshCheckpoint::StatusRead,
            ReceiptRefreshCheckpoint::StatusIdentityValidated,
            ReceiptRefreshCheckpoint::LookupRead,
            ReceiptRefreshCheckpoint::LookupIdentityValidated,
            ReceiptRefreshCheckpoint::ReceiptParsed,
            ReceiptRefreshCheckpoint::FinalIdentityValidated,
            ReceiptRefreshCheckpoint::BeforeJournalWrite,
            ReceiptRefreshCheckpoint::JournalRecorded,
            ReceiptRefreshCheckpoint::BeforeComplete,
        ] {
            let fixture = Fixture::new();
            let body = lookup_body(
                &fixture.transaction,
                &fixture.transaction_id,
                &WalletReceiptObservation::Pending,
            );
            let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
                .refresh_with_factory_and_observer(
                    MAIN,
                    &fixture.custody,
                    &fixture.transaction_id,
                    SUBMITTED_AT + 1,
                    || Ok(FakeReceiptSource::new(body)),
                    &PanicObserver { checkpoint },
                );
            assert_eq!(
                result.err(),
                Some(WalletReceiptRefreshError::Runtime(
                    WalletRuntimeError::RuntimeUnavailable
                )),
                "checkpoint: {checkpoint:?}"
            );
            assert_eq!(
                fixture.runtime.begin_receipt_refresh(MAIN).err(),
                Some(WalletRuntimeError::RuntimeUnavailable),
                "checkpoint: {checkpoint:?}"
            );
        }
    }

    #[test]
    fn every_core_read_checkpoint_rejects_stop_restart_or_generation_change() {
        enum Failure {
            InitialIdentity,
            Status,
            StatusIdentity,
            Lookup,
            LookupIdentity,
            FinalIdentity,
        }
        for failure in [
            Failure::InitialIdentity,
            Failure::Status,
            Failure::StatusIdentity,
            Failure::Lookup,
            Failure::LookupIdentity,
            Failure::FinalIdentity,
        ] {
            let fixture = Fixture::new();
            let source = FakeReceiptSource::new(lookup_body(
                &fixture.transaction,
                &fixture.transaction_id,
                &WalletReceiptObservation::Pending,
            ));
            match failure {
                Failure::InitialIdentity => source.identity_error_at.set(Some(1)),
                Failure::Status => source.status_error.set(true),
                Failure::StatusIdentity => source.change_identity_at.set(Some(2)),
                Failure::Lookup => source.lookup_error.set(true),
                Failure::LookupIdentity => source.change_identity_at.set(Some(3)),
                Failure::FinalIdentity => source.change_identity_at.set(Some(4)),
            }
            assert!(matches!(
                fixture.refresh(&source, SUBMITTED_AT + 1),
                Err(WalletReceiptRefreshError::CoreUnavailable)
                    | Err(WalletReceiptRefreshError::CoreCompatibilityUnavailable)
            ));
            assert_eq!(
                fixture.record().observation,
                WalletReceiptObservation::NotFound
            );
        }
    }

    #[test]
    fn authenticated_store_mutation_between_prepare_and_record_fails_closed() {
        for target in ["journal", "envelope"] {
            let fixture = Fixture::new();
            let body = lookup_body(
                &fixture.transaction,
                &fixture.transaction_id,
                &WalletReceiptObservation::Pending,
            );
            let path = if target == "journal" {
                fixture.custody.journal_path().to_path_buf()
            } else {
                fixture
                    ._directory
                    .path()
                    .join("wallet.signed-envelopes.v1.enc")
            };
            let observer = ActionObserver {
                checkpoint: ReceiptRefreshCheckpoint::BeforeJournalWrite,
                action: RefCell::new(Some(Box::new(move || {
                    fs::write(path, b"{}").unwrap();
                }))),
            };
            let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
                .refresh_with_factory_and_observer(
                    MAIN,
                    &fixture.custody,
                    &fixture.transaction_id,
                    SUBMITTED_AT + 1,
                    || Ok(FakeReceiptSource::new(body)),
                    &observer,
                );
            assert_eq!(
                result.err(),
                Some(WalletReceiptRefreshError::ActivityUnavailable),
                "target: {target}"
            );
        }
    }

    #[test]
    fn production_lifecycle_paths_revoke_an_active_refresh() {
        enum Event {
            MainWindowDestroyed,
            WalletLock,
            Sleep,
            Shutdown,
        }
        for event in [
            Event::MainWindowDestroyed,
            Event::WalletLock,
            Event::Sleep,
            Event::Shutdown,
        ] {
            let fixture = Fixture::new();
            let body = lookup_body(
                &fixture.transaction,
                &fixture.transaction_id,
                &WalletReceiptObservation::Pending,
            );
            let runtime = Arc::clone(&fixture.runtime);
            let vault_path = fixture.custody.vault_path().to_path_buf();
            let observer = ActionObserver {
                checkpoint: ReceiptRefreshCheckpoint::LookupRead,
                action: RefCell::new(Some(Box::new(move || match event {
                    Event::MainWindowDestroyed => runtime.invalidate_all().unwrap(),
                    Event::WalletLock => {
                        crate::wallet::lifecycle::WalletLifecycleAdapters::for_test(
                            Arc::clone(&runtime),
                            &vault_path,
                        )
                        .lock()
                        .unwrap();
                    }
                    Event::Sleep => {
                        crate::wallet::windows_lifecycle::dispatch_native_security_event_for_test(
                            Arc::clone(&runtime),
                            crate::wallet::windows_lifecycle::WalletNativeSecurityEventForTest::Sleep,
                        );
                    }
                    Event::Shutdown => {
                        crate::wallet::windows_lifecycle::dispatch_native_security_event_for_test(
                            Arc::clone(&runtime),
                            crate::wallet::windows_lifecycle::WalletNativeSecurityEventForTest::Shutdown,
                        );
                    }
                }))),
            };
            let result = WalletReceiptRefreshEngine::new(&fixture.runtime)
                .refresh_with_factory_and_observer(
                    MAIN,
                    &fixture.custody,
                    &fixture.transaction_id,
                    SUBMITTED_AT + 1,
                    || Ok(FakeReceiptSource::new(body)),
                    &observer,
                );
            assert_eq!(
                result.err(),
                Some(WalletReceiptRefreshError::Runtime(
                    WalletRuntimeError::RuntimeUnavailable
                ))
            );
        }
    }

    #[test]
    fn coordinator_has_no_core_write_or_secret_formatting_surface() {
        let source = include_str!("receipt_refresh.rs");
        for prohibited in [
            "CoreWriteOnce",
            "submit_once(",
            "POST /transactions",
            "WalletSeed",
            "WalletPassword",
        ] {
            assert!(!source[..source.find("#[cfg(test)]").unwrap()].contains(prohibited));
        }
        assert!(!source[..source.find("#[cfg(test)]").unwrap()].contains("#[derive(Debug)]"));
    }
}
