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
    runtime::{WalletRuntimeError, WalletRuntimeState},
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
        let source = WalletCoreReadClient::from_supervisor(supervisor).map_err(map_core_error)?;
        self.refresh_with_source(
            owner_window,
            custody,
            transaction_id,
            observed_at_unix_ms,
            &source,
        )
    }

    fn refresh_with_source(
        &self,
        owner_window: &str,
        custody: &WalletCustodyPathAuthority,
        transaction_id: &str,
        observed_at_unix_ms: u64,
        source: &impl WalletCoreReceiptSource,
    ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError> {
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            self.refresh_with_source_inner(
                owner_window,
                custody,
                transaction_id,
                observed_at_unix_ms,
                source,
            )
        }));
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

    fn refresh_with_source_inner(
        &self,
        owner_window: &str,
        custody: &WalletCustodyPathAuthority,
        transaction_id: &str,
        observed_at_unix_ms: u64,
        source: &impl WalletCoreReceiptSource,
    ) -> Result<PrivateReceiptRefreshResult, WalletReceiptRefreshError> {
        let permit = self
            .runtime
            .begin_receipt_refresh(owner_window)
            .map_err(map_runtime_error)?;
        let prepared = permit
            .prepare(custody, transaction_id)
            .map_err(map_runtime_error)?
            .ok_or(WalletReceiptRefreshError::TransactionUnknown)?;
        permit.ensure_current().map_err(map_runtime_error)?;

        let initial_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(map_core_error)?;
        let status = source.status().map_err(map_core_error)?;
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

        let body = source
            .transaction_lookup(prepared.transaction_id())
            .map_err(map_core_error)?;
        permit.ensure_current().map_err(map_runtime_error)?;
        let lookup_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(map_core_error)?;
        if lookup_fingerprint != initial_fingerprint {
            return Err(WalletReceiptRefreshError::CoreUnavailable);
        }
        let observation = parse_exact_signed_receipt_observation(
            body.as_slice(),
            prepared.transaction(),
            prepared.signed_body_digest_hex(),
            status.canonical_tip_height,
        )
        .map_err(|_| WalletReceiptRefreshError::CoreResponseRejected)?;
        permit.ensure_current().map_err(map_runtime_error)?;
        let final_fingerprint = source
            .validated_identity_fingerprint()
            .map_err(map_core_error)?;
        if final_fingerprint != initial_fingerprint {
            return Err(WalletReceiptRefreshError::CoreUnavailable);
        }
        let change = classify_receipt_change(Some(prepared.previous_observation()), &observation);
        permit.ensure_current().map_err(map_runtime_error)?;
        let record = permit
            .record_observation(custody, prepared, &observation, observed_at_unix_ms)
            .map_err(map_runtime_error)?;
        permit
            .complete(PrivateReceiptRefreshResult { record, change })
            .map_err(map_runtime_error)
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
    }

    impl FakeReceiptSource {
        fn new(body: Vec<u8>) -> Self {
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
            _transaction_id: &str,
        ) -> Result<Zeroizing<Vec<u8>>, WalletCoreClientError> {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            if self.revoke_on_lookup.get() {
                self.revoke_now();
            }
            Ok(Zeroizing::new(self.body.borrow().clone()))
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
        let source = FakeReceiptSource::new(lookup_body(
            &fixture.transaction,
            &fixture.transaction_id,
            &pending,
        ));
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
            &WalletReceiptObservation::NotFound,
        ));
        assert_eq!(
            fixture.refresh(&source, SUBMITTED_AT + 5).unwrap().change,
            WalletReceiptChange::ObservationLost
        );
        assert_eq!(source.lookup_calls.get(), 5);
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
