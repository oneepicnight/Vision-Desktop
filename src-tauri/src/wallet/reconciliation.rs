use super::{
    account::derive_account_identity,
    envelope_store::PreparedEnvelopeAuthority,
    lifecycle::WalletCustodyPathAuthority,
    runtime::{WalletActivationProof, WalletRuntimeError},
    secrets::WalletSeed,
    secure_filesystem::{
        create_new_publishable_file, open_existing_file, publish_open_file, replace_with_open_file,
        DirectoryChainGuard,
    },
    storage_security,
};
use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

const RECORD_SCHEMA: &str = "vision-desktop-wallet-submission-reconciliation";
const RECORD_VERSION: u32 = 2;
const HEAD_SCHEMA: &str = "vision-desktop-wallet-submission-reconciliation-head";
const HEAD_VERSION: u32 = 2;
const RECORD_FILE: &str = "wallet.submission-reconciliation.json";
const HEAD_FILE: &str = "wallet.submission-reconciliation.head.json";
const STAGING_PREFIX: &str = ".wallet-submission-reconciliation-stage-";
const AUTHENTICATION_BYTES: usize = 32;
const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_HEAD_BYTES: usize = 8 * 1024;
const RECORD_KEY_CONTEXT: &str =
    "com.vision.desktop.wallet-submission-reconciliation-record-key.v1";
const HEAD_KEY_CONTEXT: &str = "com.vision.desktop.wallet-submission-reconciliation-head-key.v1";
const RECORD_DOMAIN: &[u8] = b"vision-desktop.wallet-submission-reconciliation.record.v1";
const HEAD_DOMAIN: &[u8] = b"vision-desktop.wallet-submission-reconciliation.head.v1";

static STORE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(super) struct ReconciliationStore {
    record_path: PathBuf,
    head_path: PathBuf,
}

pub(super) struct ReconciliationAuthenticator {
    wallet_id: String,
    sender_address: String,
    record_key: SecretBox<[u8; AUTHENTICATION_BYTES]>,
    head_key: SecretBox<[u8; AUTHENTICATION_BYTES]>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ReconciliationPhase {
    Prepared,
    MayHaveBeenSubmitted,
    AcceptedRecordingPending {
        accepted_tx_id: String,
        accepted_nonce: u64,
        decision: String,
        compatibility_contract_digest_hex: String,
    },
    ResolvedNotAttempted,
    ResolvedRejected {
        http_status: u16,
        rejection_code: String,
        allowlist_digest_hex: String,
    },
    ResolvedRecorded,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ReconciliationRecord {
    schema: String,
    version: u32,
    wallet_id: String,
    attempt_id: String,
    store_generation: u64,
    transaction_id: String,
    sender_address: String,
    recipient_address: String,
    amount_raw_units: String,
    nonce: u64,
    tip_raw_units: u64,
    fee_limit_raw_units: u64,
    signed_body_digest_hex: String,
    envelope_commitment_hex: String,
    parent_head_generation: u64,
    parent_head_authentication_tag_hex: String,
    reserved_prepared_generation: u64,
    original_core_identity_fingerprint_hex: String,
    created_at_unix_ms: u64,
    phase: ReconciliationPhase,
    authentication_tag_hex: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReconciliationHead {
    schema: String,
    version: u32,
    wallet_id: String,
    generation: u64,
    previous_head_tag_hex: String,
    state: ReconciliationHeadState,
    authentication_tag_hex: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ReconciliationHeadState {
    Committed {
        position: ReconciliationPosition,
    },
    Transition {
        previous_generation: u64,
        previous_previous_head_tag_hex: String,
        previous: ReconciliationPosition,
        next: ReconciliationPosition,
    },
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReconciliationPosition {
    record_generation: u64,
    attempt_id: Option<String>,
    phase: Option<ReconciliationPhaseTag>,
    record_digest_hex: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReconciliationPhaseTag {
    Prepared,
    MayHaveBeenSubmitted,
    AcceptedRecordingPending,
    ResolvedNotAttempted,
    ResolvedRejected,
    ResolvedRecorded,
}

enum ExpectedTransition {
    NewAttempt(ReconciliationReservation),
    Exact(ReconciliationPhaseTag),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReconciliationTransitionCheckpoint {
    BeforeHeadTransition,
    HeadTransitionPublished,
    RecordPublished,
    HeadCommitted,
}

pub(super) struct LoadedReconciliation {
    record: Option<ReconciliationRecord>,
    head_generation: Option<u64>,
    head_authentication_tag: [u8; AUTHENTICATION_BYTES],
    head_previous_authentication_tag_hex: String,
}

/// Authenticated predecessor position reserved for exactly one future Prepared record.
/// Fields remain private so ordinary callers cannot forge a store generation.
pub(super) struct ReconciliationReservation {
    parent_generation: u64,
    parent_authentication_tag: [u8; AUTHENTICATION_BYTES],
    prepared_generation: u64,
}

impl ReconciliationReservation {
    pub(super) const fn parent_generation(&self) -> u64 {
        self.parent_generation
    }

    pub(super) fn parent_authentication_tag_hex(&self) -> String {
        hex::encode(self.parent_authentication_tag)
    }

    pub(super) const fn prepared_generation(&self) -> u64 {
        self.prepared_generation
    }
}

/// Linear live-attempt capabilities. None implement Clone, Debug, Display, or serialization.
struct LiveReconciliationProof;
pub(super) struct LiveReconciliationAuthority {
    _proof: LiveReconciliationProof,
}
pub(super) struct PreparedReconciliationAuthority {
    record: ReconciliationRecord,
}
pub(super) struct MayHaveBeenSubmittedAuthority {
    record: ReconciliationRecord,
}
pub(super) struct AcceptedRecordingAuthority {
    record: ReconciliationRecord,
}
struct CoreWriteProof;
pub(super) struct CoreWriteOnce {
    _proof: CoreWriteProof,
}

pub(super) struct SubmissionActivationGrant {
    reconciliation: LiveReconciliationAuthority,
    write: CoreWriteOnce,
}

pub(super) struct WriteReadySubmission {
    reconciliation: MayHaveBeenSubmittedAuthority,
    write: CoreWriteOnce,
}

struct ReconciliationDiscoveryProof;
pub(super) struct ReconciliationDiscoveryPermit {
    _proof: ReconciliationDiscoveryProof,
}
pub(super) struct RestartReconciliationPermit {
    record: ReconciliationRecord,
}

pub(super) struct ReconciliationLookupExpectation {
    attempt_id: String,
    transaction_id: String,
    sender_address: String,
    recipient_address: String,
    amount_raw_units: String,
    nonce: u64,
    tip_raw_units: u64,
    fee_limit_raw_units: u64,
    signed_body_digest_hex: String,
    envelope_commitment_hex: String,
    parent_head_generation: u64,
    parent_head_authentication_tag_hex: String,
    reserved_prepared_generation: u64,
}

pub(super) struct AcceptedSubmissionEvidence {
    attempt_id: String,
    wallet_id: String,
    transaction_id: String,
    sender_address: String,
    recipient_address: String,
    amount_raw_units: String,
    nonce: u64,
    tip_raw_units: u64,
    fee_limit_raw_units: u64,
    envelope_commitment_hex: String,
    parent_head_generation: u64,
    parent_head_authentication_tag_hex: String,
    reserved_prepared_generation: u64,
    submitted_at_unix_ms: u64,
}

/// Immutable authenticated cross-store binding copied only from a verified
/// reconciliation record. Ordinary callers cannot construct or alter it.
pub(super) struct ReconciliationEnvelopeBinding {
    attempt_id: String,
    transaction_id: String,
    commitment_hex: String,
    parent_generation: u64,
    parent_tag_hex: String,
    reserved_prepared_generation: u64,
}

impl ReconciliationEnvelopeBinding {
    pub(super) fn attempt_id(&self) -> &str {
        &self.attempt_id
    }
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }
    pub(super) fn commitment_hex(&self) -> &str {
        &self.commitment_hex
    }
    pub(super) const fn parent_generation(&self) -> u64 {
        self.parent_generation
    }
    pub(super) fn parent_tag_hex(&self) -> &str {
        &self.parent_tag_hex
    }
    pub(super) const fn reserved_prepared_generation(&self) -> u64 {
        self.reserved_prepared_generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReconciliationError {
    InvalidRequest,
    StorageUnavailable,
    AuthenticationFailed,
    InvalidTransition,
    ReconciliationTooLarge,
}

impl ReconciliationStore {
    pub(super) fn for_custody(
        custody: &WalletCustodyPathAuthority,
    ) -> Result<Self, ReconciliationError> {
        let directory = custody
            .vault_path()
            .parent()
            .ok_or(ReconciliationError::InvalidRequest)?;
        Ok(Self {
            record_path: directory.join(RECORD_FILE),
            head_path: directory.join(HEAD_FILE),
        })
    }

    pub(super) fn discover(&self) -> Result<bool, ReconciliationError> {
        let directory = self
            .record_path
            .parent()
            .ok_or(ReconciliationError::StorageUnavailable)?;
        let _guard = match DirectoryChainGuard::open_existing(directory) {
            Ok(guard) => guard,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(ReconciliationError::StorageUnavailable),
        };
        storage_security::verify_directory(directory)
            .map_err(|_| ReconciliationError::StorageUnavailable)?;
        Ok(self.record_path.try_exists().unwrap_or(true)
            || self.head_path.try_exists().unwrap_or(true))
    }

    #[cfg(test)]
    pub(super) fn is_bound_to(&self, custody: &WalletCustodyPathAuthority) -> bool {
        let directory = custody.vault_path().parent();
        self.record_path.parent() == directory && self.head_path.parent() == directory
    }

    pub(super) fn load_authenticated(
        &self,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<LoadedReconciliation, ReconciliationError> {
        let _lock = STORE_LOCK
            .lock()
            .map_err(|_| ReconciliationError::StorageUnavailable)?;
        self.load_authenticated_unlocked(authenticator)
    }

    /// Returns the authenticated current head only when no reconciliation record occupies it.
    /// This is the sole predecessor proof accepted by pre-Prepared envelope orphan cleanup.
    pub(super) fn empty_head_reservation(
        &self,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<Option<ReconciliationReservation>, ReconciliationError> {
        let _lock = STORE_LOCK
            .lock()
            .map_err(|_| ReconciliationError::StorageUnavailable)?;
        let loaded = self.load_authenticated_unlocked(authenticator)?;
        if loaded.record.is_some() {
            return Ok(None);
        }
        let Some(parent_generation) = loaded.head_generation else {
            return Ok(None);
        };
        let prepared_generation = parent_generation
            .checked_add(1)
            .ok_or(ReconciliationError::InvalidTransition)?;
        Ok(Some(ReconciliationReservation {
            parent_generation,
            parent_authentication_tag: loaded.head_authentication_tag,
            prepared_generation,
        }))
    }

    pub(super) fn reserve_prepared(
        &self,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<ReconciliationReservation, ReconciliationError> {
        let _lock = STORE_LOCK
            .lock()
            .map_err(|_| ReconciliationError::StorageUnavailable)?;
        let mut loaded = self.load_authenticated_unlocked(authenticator)?;
        if loaded.head_generation.is_none() {
            let mut genesis = ReconciliationHead {
                schema: HEAD_SCHEMA.to_string(),
                version: HEAD_VERSION,
                wallet_id: authenticator.wallet_id.clone(),
                generation: 0,
                previous_head_tag_hex: "00".repeat(AUTHENTICATION_BYTES),
                state: ReconciliationHeadState::Committed {
                    position: empty_position(),
                },
                authentication_tag_hex: String::new(),
            };
            authenticate_head(authenticator, &mut genesis)?;
            persist_json(&self.head_path, &genesis, true, MAX_HEAD_BYTES)?;
            loaded.head_generation = Some(0);
            loaded.head_authentication_tag = decode_tag(&genesis.authentication_tag_hex)?;
            loaded.head_previous_authentication_tag_hex = "00".repeat(AUTHENTICATION_BYTES);
        }
        if loaded
            .record
            .as_ref()
            .is_some_and(|record| !record.phase.tag().is_terminal())
        {
            return Err(ReconciliationError::InvalidTransition);
        }
        let parent_generation = loaded
            .head_generation
            .ok_or(ReconciliationError::AuthenticationFailed)?;
        let prepared_generation = parent_generation
            .checked_add(1)
            .ok_or(ReconciliationError::InvalidTransition)?;
        Ok(ReconciliationReservation {
            parent_generation,
            parent_authentication_tag: loaded.head_authentication_tag,
            prepared_generation,
        })
    }

    fn transition(
        &self,
        authenticator: &ReconciliationAuthenticator,
        expected: ExpectedTransition,
        next: ReconciliationRecord,
    ) -> Result<LoadedReconciliation, ReconciliationError> {
        self.transition_with_checkpoint(authenticator, expected, next, |_| Ok(()))
    }

    fn transition_with_checkpoint(
        &self,
        authenticator: &ReconciliationAuthenticator,
        expected: ExpectedTransition,
        mut next: ReconciliationRecord,
        mut checkpoint: impl FnMut(
            ReconciliationTransitionCheckpoint,
        ) -> Result<(), ReconciliationError>,
    ) -> Result<LoadedReconciliation, ReconciliationError> {
        let _lock = STORE_LOCK
            .lock()
            .map_err(|_| ReconciliationError::StorageUnavailable)?;
        let loaded = self.load_authenticated_unlocked(authenticator)?;
        let current_tag = loaded.record.as_ref().map(|record| record.phase.tag());
        let expected_matches = match &expected {
            ExpectedTransition::NewAttempt(reservation) => {
                (current_tag.is_none()
                    || current_tag.is_some_and(ReconciliationPhaseTag::is_terminal))
                    && loaded.head_generation == Some(reservation.parent_generation)
                    && loaded.head_authentication_tag == reservation.parent_authentication_tag
            }
            ExpectedTransition::Exact(expected) => current_tag == Some(*expected),
        };
        if !expected_matches
            || !valid_transition(current_tag, next.phase.tag())
            || next.wallet_id != authenticator.wallet_id
            || next.sender_address != authenticator.sender_address
        {
            return Err(ReconciliationError::InvalidTransition);
        }
        let next_generation = loaded
            .head_generation
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ReconciliationError::InvalidTransition)?;
        if let ExpectedTransition::NewAttempt(reservation) = &expected {
            if next_generation != reservation.prepared_generation
                || next.parent_head_generation != reservation.parent_generation
                || next.parent_head_authentication_tag_hex
                    != hex::encode(reservation.parent_authentication_tag)
                || next.reserved_prepared_generation != reservation.prepared_generation
            {
                return Err(ReconciliationError::InvalidTransition);
            }
        }
        next.store_generation = next_generation;
        if let Some(current) = loaded.record.as_ref() {
            if !current.phase.tag().is_terminal() && !same_attempt(current, &next) {
                return Err(ReconciliationError::InvalidTransition);
            }
            if current.phase.tag().is_terminal()
                && (next.phase.tag() != ReconciliationPhaseTag::Prepared
                    || current.attempt_id == next.attempt_id)
            {
                return Err(ReconciliationError::InvalidTransition);
            }
        } else if next.phase.tag() != ReconciliationPhaseTag::Prepared {
            return Err(ReconciliationError::InvalidTransition);
        }
        validate_record(&next, authenticator)?;
        authenticate_record(authenticator, &mut next)?;
        let encoded = encode_record(&next)?;
        let next_position = position_for_record(&next, &encoded);
        let previous_position = loaded
            .record
            .as_ref()
            .map(|record| encode_record(record).map(|bytes| position_for_record(record, &bytes)))
            .transpose()?
            .unwrap_or_else(empty_position);
        let previous_head_tag_hex = hex::encode(loaded.head_authentication_tag);
        let mut transition = ReconciliationHead {
            schema: HEAD_SCHEMA.to_string(),
            version: HEAD_VERSION,
            wallet_id: authenticator.wallet_id.clone(),
            generation: next_generation,
            previous_head_tag_hex: previous_head_tag_hex.clone(),
            state: ReconciliationHeadState::Transition {
                previous_generation: loaded
                    .head_generation
                    .ok_or(ReconciliationError::AuthenticationFailed)?,
                previous_previous_head_tag_hex: loaded.head_previous_authentication_tag_hex.clone(),
                previous: previous_position,
                next: next_position.clone(),
            },
            authentication_tag_hex: String::new(),
        };
        authenticate_head(authenticator, &mut transition)?;
        checkpoint(ReconciliationTransitionCheckpoint::BeforeHeadTransition)?;
        persist_json(&self.head_path, &transition, false, MAX_HEAD_BYTES)?;
        checkpoint(ReconciliationTransitionCheckpoint::HeadTransitionPublished)?;
        persist_bytes(
            &self.record_path,
            &encoded,
            loaded.record.is_none(),
            MAX_RECORD_BYTES,
        )?;
        checkpoint(ReconciliationTransitionCheckpoint::RecordPublished)?;
        let mut committed = ReconciliationHead {
            schema: HEAD_SCHEMA.to_string(),
            version: HEAD_VERSION,
            wallet_id: authenticator.wallet_id.clone(),
            generation: next_generation,
            previous_head_tag_hex,
            state: ReconciliationHeadState::Committed {
                position: next_position,
            },
            authentication_tag_hex: String::new(),
        };
        authenticate_head(authenticator, &mut committed)?;
        persist_json(&self.head_path, &committed, false, MAX_HEAD_BYTES)?;
        checkpoint(ReconciliationTransitionCheckpoint::HeadCommitted)?;
        Ok(LoadedReconciliation {
            record: Some(next),
            head_generation: Some(next_generation),
            head_authentication_tag: decode_tag(&committed.authentication_tag_hex)?,
            head_previous_authentication_tag_hex: committed.previous_head_tag_hex,
        })
    }

    fn load_authenticated_unlocked(
        &self,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<LoadedReconciliation, ReconciliationError> {
        let record_bytes = read_protected(&self.record_path, MAX_RECORD_BYTES)?;
        let head_bytes = read_protected(&self.head_path, MAX_HEAD_BYTES)?;
        match (record_bytes, head_bytes) {
            (None, None) => Ok(LoadedReconciliation {
                record: None,
                head_generation: None,
                head_authentication_tag: [0_u8; AUTHENTICATION_BYTES],
                head_previous_authentication_tag_hex: "00".repeat(AUTHENTICATION_BYTES),
            }),
            (record_bytes, Some(head_bytes)) => {
                let record = record_bytes
                    .as_deref()
                    .map(|bytes| decode_record(bytes, authenticator))
                    .transpose()?;
                let mut head: ReconciliationHead = serde_json::from_slice(&head_bytes)
                    .map_err(|_| ReconciliationError::AuthenticationFailed)?;
                verify_head(authenticator, &head)?;
                let actual = record
                    .as_ref()
                    .map(|record| {
                        encode_record(record).map(|bytes| position_for_record(record, &bytes))
                    })
                    .transpose()?
                    .unwrap_or_else(empty_position);
                let committed_position = match &head.state {
                    ReconciliationHeadState::Committed { position } if *position == actual => {
                        return Ok(LoadedReconciliation {
                            record,
                            head_generation: Some(head.generation),
                            head_authentication_tag: decode_tag(&head.authentication_tag_hex)?,
                            head_previous_authentication_tag_hex: head
                                .previous_head_tag_hex
                                .clone(),
                        });
                    }
                    ReconciliationHeadState::Transition {
                        previous_generation,
                        previous_previous_head_tag_hex,
                        previous,
                        ..
                    } if *previous == actual => {
                        head.generation = *previous_generation;
                        head.previous_head_tag_hex = previous_previous_head_tag_hex.clone();
                        previous.clone()
                    }
                    ReconciliationHeadState::Transition { next, .. } if *next == actual => {
                        next.clone()
                    }
                    _ => return Err(ReconciliationError::AuthenticationFailed),
                };
                head.state = ReconciliationHeadState::Committed {
                    position: committed_position,
                };
                head.authentication_tag_hex.clear();
                authenticate_head(authenticator, &mut head)?;
                persist_json(&self.head_path, &head, false, MAX_HEAD_BYTES)?;
                Ok(LoadedReconciliation {
                    record,
                    head_generation: Some(head.generation),
                    head_authentication_tag: decode_tag(&head.authentication_tag_hex)?,
                    head_previous_authentication_tag_hex: head.previous_head_tag_hex.clone(),
                })
            }
            (Some(_), None) => Err(ReconciliationError::AuthenticationFailed),
        }
    }
}

impl LoadedReconciliation {
    pub(super) fn into_record(self) -> Option<ReconciliationRecord> {
        self.record
    }
}

impl SubmissionActivationGrant {
    pub(super) fn new(activation: &WalletActivationProof) -> Result<Self, WalletRuntimeError> {
        activation.require_submission()?;
        Ok(Self::new_unchecked())
    }

    const fn new_unchecked() -> Self {
        Self {
            reconciliation: LiveReconciliationAuthority {
                _proof: LiveReconciliationProof,
            },
            write: CoreWriteOnce {
                _proof: CoreWriteProof,
            },
        }
    }

    pub(super) fn split(self) -> (LiveReconciliationAuthority, CoreWriteOnce) {
        (self.reconciliation, self.write)
    }
}

impl LiveReconciliationAuthority {
    #[cfg(test)]
    pub(super) fn publish_prepared(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
        mut record: ReconciliationRecord,
    ) -> Result<PreparedReconciliationAuthority, ReconciliationError> {
        let reservation = store.reserve_prepared(authenticator)?;
        record.bind_envelope_for_test(&reservation);
        self.publish_prepared_reserved(store, authenticator, record, reservation)
    }

    pub(super) fn publish_prepared_reserved(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
        record: ReconciliationRecord,
        reservation: ReconciliationReservation,
    ) -> Result<PreparedReconciliationAuthority, ReconciliationError> {
        let loaded = store.transition(
            authenticator,
            ExpectedTransition::NewAttempt(reservation),
            record,
        )?;
        let record = loaded
            .into_record()
            .ok_or(ReconciliationError::InvalidTransition)?;
        Ok(PreparedReconciliationAuthority { record })
    }
}

impl PreparedReconciliationAuthority {
    pub(super) fn verify_envelope_binding(
        &self,
        envelope: &PreparedEnvelopeAuthority,
    ) -> Result<(), ReconciliationError> {
        if self.record.attempt_id != envelope.attempt_id()
            || self.record.transaction_id != envelope.transaction_id()
            || self.record.envelope_commitment_hex != envelope.commitment_hex()
            || self.record.parent_head_generation != envelope.reconciliation_parent_generation()
            || self.record.parent_head_authentication_tag_hex
                != envelope.reconciliation_parent_tag_hex()
            || self.record.reserved_prepared_generation != envelope.reserved_prepared_generation()
        {
            return Err(ReconciliationError::AuthenticationFailed);
        }
        Ok(())
    }

    pub(super) fn publish_may_have_been_submitted(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<MayHaveBeenSubmittedAuthority, ReconciliationError> {
        let next = self
            .record
            .into_phase(ReconciliationPhase::MayHaveBeenSubmitted);
        let loaded = store.transition(
            authenticator,
            ExpectedTransition::Exact(ReconciliationPhaseTag::Prepared),
            next,
        )?;
        let record = loaded
            .into_record()
            .ok_or(ReconciliationError::InvalidTransition)?;
        Ok(MayHaveBeenSubmittedAuthority { record })
    }

    pub(super) fn resolve_not_attempted(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<(), ReconciliationError> {
        let next = self
            .record
            .into_phase(ReconciliationPhase::ResolvedNotAttempted);
        store.transition(
            authenticator,
            ExpectedTransition::Exact(ReconciliationPhaseTag::Prepared),
            next,
        )?;
        Ok(())
    }
}

impl MayHaveBeenSubmittedAuthority {
    pub(super) fn combine(self, write: CoreWriteOnce) -> WriteReadySubmission {
        WriteReadySubmission {
            reconciliation: self,
            write,
        }
    }

    pub(super) fn publish_accepted(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
        accepted_tx_id: String,
        accepted_nonce: u64,
        compatibility_contract_digest_hex: String,
    ) -> Result<AcceptedRecordingAuthority, ReconciliationError> {
        let next = self
            .record
            .into_phase(ReconciliationPhase::AcceptedRecordingPending {
                accepted_tx_id,
                accepted_nonce,
                decision: "accept".to_string(),
                compatibility_contract_digest_hex,
            });
        let loaded = store.transition(
            authenticator,
            ExpectedTransition::Exact(ReconciliationPhaseTag::MayHaveBeenSubmitted),
            next,
        )?;
        let record = loaded
            .into_record()
            .ok_or(ReconciliationError::InvalidTransition)?;
        Ok(AcceptedRecordingAuthority { record })
    }

    pub(super) fn resolve_rejected(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
        http_status: u16,
        rejection_code: String,
        allowlist_digest_hex: String,
    ) -> Result<(), ReconciliationError> {
        let next = self
            .record
            .into_phase(ReconciliationPhase::ResolvedRejected {
                http_status,
                rejection_code,
                allowlist_digest_hex,
            });
        store.transition(
            authenticator,
            ExpectedTransition::Exact(ReconciliationPhaseTag::MayHaveBeenSubmitted),
            next,
        )?;
        Ok(())
    }
}

impl WriteReadySubmission {
    pub(super) fn into_parts(self) -> (MayHaveBeenSubmittedAuthority, CoreWriteOnce) {
        (self.reconciliation, self.write)
    }
}

impl AcceptedRecordingAuthority {
    pub(super) fn evidence(&self) -> Result<AcceptedSubmissionEvidence, ReconciliationError> {
        record_to_evidence(&self.record)
    }

    pub(super) fn resolve_recorded(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<(), ReconciliationError> {
        let next = self
            .record
            .into_phase(ReconciliationPhase::ResolvedRecorded);
        store.transition(
            authenticator,
            ExpectedTransition::Exact(ReconciliationPhaseTag::AcceptedRecordingPending),
            next,
        )?;
        Ok(())
    }
}

impl ReconciliationDiscoveryPermit {
    pub(super) fn new(activation: &WalletActivationProof) -> Result<Self, WalletRuntimeError> {
        activation.require_reconciliation()?;
        Ok(Self {
            _proof: ReconciliationDiscoveryProof,
        })
    }

    pub(super) fn discover(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<Option<RestartReconciliationPermit>, ReconciliationError> {
        if !store.discover()? {
            return Ok(None);
        }
        let record = store.load_authenticated(authenticator)?.into_record();
        Ok(record.map(|record| RestartReconciliationPermit { record }))
    }
}

#[cfg(test)]
pub(super) const fn core_write_once_for_test() -> CoreWriteOnce {
    CoreWriteOnce {
        _proof: CoreWriteProof,
    }
}

#[cfg(test)]
pub(super) fn publish_prepared_for_test(
    store: &ReconciliationStore,
    authenticator: &ReconciliationAuthenticator,
    record: ReconciliationRecord,
) -> Result<(), ReconciliationError> {
    SubmissionActivationGrant::new_unchecked()
        .split()
        .0
        .publish_prepared(store, authenticator, record)?;
    Ok(())
}

#[cfg(test)]
pub(super) fn publish_accepted_for_test(
    store: &ReconciliationStore,
    authenticator: &ReconciliationAuthenticator,
    record: ReconciliationRecord,
) -> Result<(), ReconciliationError> {
    let (live, _write) = SubmissionActivationGrant::new_unchecked().split();
    live.publish_prepared(store, authenticator, record)?
        .publish_may_have_been_submitted(store, authenticator)?
        .publish_accepted(store, authenticator, "22".repeat(32), 7, "66".repeat(32))?;
    Ok(())
}

#[cfg(test)]
pub(super) fn publish_may_have_been_submitted_for_test(
    store: &ReconciliationStore,
    authenticator: &ReconciliationAuthenticator,
    record: ReconciliationRecord,
) -> Result<(), ReconciliationError> {
    SubmissionActivationGrant::new_unchecked()
        .split()
        .0
        .publish_prepared(store, authenticator, record)?
        .publish_may_have_been_submitted(store, authenticator)?;
    Ok(())
}

impl RestartReconciliationPermit {
    pub(super) fn phase_tag(&self) -> ReconciliationPhaseTag {
        self.record.phase_tag()
    }

    pub(super) fn transaction_id(&self) -> &str {
        self.record.transaction_id.as_str()
    }

    pub(super) fn envelope_binding(&self) -> ReconciliationEnvelopeBinding {
        binding_for_record(&self.record)
    }

    pub(super) fn resolve_prepared(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
    ) -> Result<(), ReconciliationError> {
        if self.record.phase_tag() != ReconciliationPhaseTag::Prepared {
            return Err(ReconciliationError::InvalidTransition);
        }
        PreparedReconciliationAuthority {
            record: self.record,
        }
        .resolve_not_attempted(store, authenticator)
    }

    pub(super) fn accepted_evidence(
        self,
    ) -> Result<(AcceptedRecordingAuthority, AcceptedSubmissionEvidence), ReconciliationError> {
        if self.record.phase_tag() != ReconciliationPhaseTag::AcceptedRecordingPending {
            return Err(ReconciliationError::InvalidTransition);
        }
        let evidence = record_to_evidence(&self.record)?;
        Ok((
            AcceptedRecordingAuthority {
                record: self.record,
            },
            evidence,
        ))
    }

    pub(super) fn lookup_expectation(
        self,
    ) -> Result<(Self, ReconciliationLookupExpectation), ReconciliationError> {
        if self.record.phase_tag() != ReconciliationPhaseTag::MayHaveBeenSubmitted {
            return Err(ReconciliationError::InvalidTransition);
        }
        let expectation = ReconciliationLookupExpectation {
            attempt_id: self.record.attempt_id.clone(),
            transaction_id: self.record.transaction_id.clone(),
            sender_address: self.record.sender_address.clone(),
            recipient_address: self.record.recipient_address.clone(),
            amount_raw_units: self.record.amount_raw_units.clone(),
            nonce: self.record.nonce,
            tip_raw_units: self.record.tip_raw_units,
            fee_limit_raw_units: self.record.fee_limit_raw_units,
            signed_body_digest_hex: self.record.signed_body_digest_hex.clone(),
            envelope_commitment_hex: self.record.envelope_commitment_hex.clone(),
            parent_head_generation: self.record.parent_head_generation,
            parent_head_authentication_tag_hex: self
                .record
                .parent_head_authentication_tag_hex
                .clone(),
            reserved_prepared_generation: self.record.reserved_prepared_generation,
        };
        Ok((self, expectation))
    }

    pub(super) fn publish_reconciled_acceptance(
        self,
        store: &ReconciliationStore,
        authenticator: &ReconciliationAuthenticator,
        proof: crate::wallet::receipt::ExactAcceptedLookup,
        compatibility_contract_digest_hex: String,
    ) -> Result<AcceptedRecordingAuthority, ReconciliationError> {
        if self.record.phase_tag() != ReconciliationPhaseTag::MayHaveBeenSubmitted
            || proof.transaction_id() != self.record.transaction_id
            || proof.nonce() != self.record.nonce
        {
            return Err(ReconciliationError::InvalidTransition);
        }
        MayHaveBeenSubmittedAuthority {
            record: self.record,
        }
        .publish_accepted(
            store,
            authenticator,
            proof.transaction_id().to_string(),
            proof.nonce(),
            compatibility_contract_digest_hex,
        )
    }
}

impl ReconciliationLookupExpectation {
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }
    pub(super) fn sender_address(&self) -> &str {
        &self.sender_address
    }
    pub(super) fn recipient_address(&self) -> &str {
        &self.recipient_address
    }
    pub(super) fn amount_raw_units(&self) -> &str {
        &self.amount_raw_units
    }
    pub(super) const fn nonce(&self) -> u64 {
        self.nonce
    }
    pub(super) const fn tip_raw_units(&self) -> u64 {
        self.tip_raw_units
    }
    pub(super) const fn fee_limit_raw_units(&self) -> u64 {
        self.fee_limit_raw_units
    }
    pub(super) fn signed_body_digest_hex(&self) -> &str {
        &self.signed_body_digest_hex
    }

    pub(super) fn envelope_binding(&self) -> ReconciliationEnvelopeBinding {
        ReconciliationEnvelopeBinding {
            attempt_id: self.attempt_id.clone(),
            transaction_id: self.transaction_id.clone(),
            commitment_hex: self.envelope_commitment_hex.clone(),
            parent_generation: self.parent_head_generation,
            parent_tag_hex: self.parent_head_authentication_tag_hex.clone(),
            reserved_prepared_generation: self.reserved_prepared_generation,
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn for_test(
        transaction_id: String,
        sender_address: String,
        recipient_address: String,
        amount_raw_units: String,
        nonce: u64,
        tip_raw_units: u64,
        fee_limit_raw_units: u64,
        signed_body_digest_hex: String,
    ) -> Self {
        Self {
            attempt_id: "dd".repeat(32),
            transaction_id,
            sender_address,
            recipient_address,
            amount_raw_units,
            nonce,
            tip_raw_units,
            fee_limit_raw_units,
            signed_body_digest_hex,
            envelope_commitment_hex: "ee".repeat(32),
            parent_head_generation: 0,
            parent_head_authentication_tag_hex: "00".repeat(32),
            reserved_prepared_generation: 1,
        }
    }
}

impl ReconciliationAuthenticator {
    pub(super) fn new(wallet_id: &str, seed: &WalletSeed) -> Result<Self, ReconciliationError> {
        validate_wallet_id(wallet_id)?;
        Ok(Self {
            wallet_id: wallet_id.to_string(),
            sender_address: derive_account_identity(seed).address,
            record_key: derive_key(seed, RECORD_KEY_CONTEXT),
            head_key: derive_key(seed, HEAD_KEY_CONTEXT),
        })
    }
}

impl ReconciliationRecord {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepared_bound(
        wallet_id: String,
        attempt_id: String,
        transaction_id: String,
        sender_address: String,
        recipient_address: String,
        amount_raw_units: String,
        nonce: u64,
        tip_raw_units: u64,
        fee_limit_raw_units: u64,
        signed_body_digest_hex: String,
        envelope_commitment_hex: String,
        reservation: &ReconciliationReservation,
        original_core_identity_fingerprint_hex: String,
        created_at_unix_ms: u64,
    ) -> Self {
        Self {
            schema: RECORD_SCHEMA.to_string(),
            version: RECORD_VERSION,
            wallet_id,
            attempt_id,
            store_generation: 0,
            transaction_id,
            sender_address,
            recipient_address,
            amount_raw_units,
            nonce,
            tip_raw_units,
            fee_limit_raw_units,
            signed_body_digest_hex,
            envelope_commitment_hex,
            parent_head_generation: reservation.parent_generation,
            parent_head_authentication_tag_hex: hex::encode(reservation.parent_authentication_tag),
            reserved_prepared_generation: reservation.prepared_generation,
            original_core_identity_fingerprint_hex,
            created_at_unix_ms,
            phase: ReconciliationPhase::Prepared,
            authentication_tag_hex: String::new(),
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepared(
        wallet_id: String,
        attempt_id: String,
        transaction_id: String,
        sender_address: String,
        recipient_address: String,
        amount_raw_units: String,
        nonce: u64,
        tip_raw_units: u64,
        fee_limit_raw_units: u64,
        signed_body_digest_hex: String,
        original_core_identity_fingerprint_hex: String,
        created_at_unix_ms: u64,
    ) -> Self {
        let reservation = ReconciliationReservation {
            parent_generation: 0,
            parent_authentication_tag: [0; AUTHENTICATION_BYTES],
            prepared_generation: 1,
        };
        Self::prepared_bound(
            wallet_id,
            attempt_id,
            transaction_id,
            sender_address,
            recipient_address,
            amount_raw_units,
            nonce,
            tip_raw_units,
            fee_limit_raw_units,
            signed_body_digest_hex,
            "ee".repeat(32),
            &reservation,
            original_core_identity_fingerprint_hex,
            created_at_unix_ms,
        )
    }

    #[cfg(test)]
    fn bind_envelope_for_test(&mut self, reservation: &ReconciliationReservation) {
        self.parent_head_generation = reservation.parent_generation;
        self.parent_head_authentication_tag_hex =
            hex::encode(reservation.parent_authentication_tag);
        self.reserved_prepared_generation = reservation.prepared_generation;
    }

    pub(super) fn phase_tag(&self) -> ReconciliationPhaseTag {
        self.phase.tag()
    }

    pub(super) fn into_phase(mut self, phase: ReconciliationPhase) -> Self {
        self.phase = phase;
        self.authentication_tag_hex.clear();
        self
    }
}

fn record_to_evidence(
    record: &ReconciliationRecord,
) -> Result<AcceptedSubmissionEvidence, ReconciliationError> {
    if record.phase.tag() != ReconciliationPhaseTag::AcceptedRecordingPending {
        return Err(ReconciliationError::InvalidTransition);
    }
    Ok(AcceptedSubmissionEvidence {
        attempt_id: record.attempt_id.clone(),
        wallet_id: record.wallet_id.clone(),
        transaction_id: record.transaction_id.clone(),
        sender_address: record.sender_address.clone(),
        recipient_address: record.recipient_address.clone(),
        amount_raw_units: record.amount_raw_units.clone(),
        nonce: record.nonce,
        tip_raw_units: record.tip_raw_units,
        fee_limit_raw_units: record.fee_limit_raw_units,
        envelope_commitment_hex: record.envelope_commitment_hex.clone(),
        parent_head_generation: record.parent_head_generation,
        parent_head_authentication_tag_hex: record.parent_head_authentication_tag_hex.clone(),
        reserved_prepared_generation: record.reserved_prepared_generation,
        submitted_at_unix_ms: record.created_at_unix_ms,
    })
}

fn binding_for_record(record: &ReconciliationRecord) -> ReconciliationEnvelopeBinding {
    ReconciliationEnvelopeBinding {
        attempt_id: record.attempt_id.clone(),
        transaction_id: record.transaction_id.clone(),
        commitment_hex: record.envelope_commitment_hex.clone(),
        parent_generation: record.parent_head_generation,
        parent_tag_hex: record.parent_head_authentication_tag_hex.clone(),
        reserved_prepared_generation: record.reserved_prepared_generation,
    }
}

impl ReconciliationPhase {
    pub(super) const fn tag(&self) -> ReconciliationPhaseTag {
        match self {
            Self::Prepared => ReconciliationPhaseTag::Prepared,
            Self::MayHaveBeenSubmitted => ReconciliationPhaseTag::MayHaveBeenSubmitted,
            Self::AcceptedRecordingPending { .. } => {
                ReconciliationPhaseTag::AcceptedRecordingPending
            }
            Self::ResolvedNotAttempted => ReconciliationPhaseTag::ResolvedNotAttempted,
            Self::ResolvedRejected { .. } => ReconciliationPhaseTag::ResolvedRejected,
            Self::ResolvedRecorded => ReconciliationPhaseTag::ResolvedRecorded,
        }
    }
}

impl ReconciliationPhaseTag {
    const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::ResolvedNotAttempted | Self::ResolvedRejected | Self::ResolvedRecorded
        )
    }
}

impl AcceptedSubmissionEvidence {
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn for_journal_test(
        wallet_id: String,
        transaction_id: String,
        sender_address: String,
        recipient_address: String,
        envelope_commitment_hex: String,
    ) -> Self {
        Self {
            attempt_id: "aa".repeat(32),
            wallet_id,
            transaction_id,
            sender_address,
            recipient_address,
            amount_raw_units: "1".to_string(),
            nonce: 7,
            tip_raw_units: 0,
            fee_limit_raw_units: 201,
            envelope_commitment_hex,
            parent_head_generation: 0,
            parent_head_authentication_tag_hex: "00".repeat(32),
            reserved_prepared_generation: 1,
            submitted_at_unix_ms: 1,
        }
    }

    pub(super) fn wallet_id(&self) -> &str {
        &self.wallet_id
    }
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }
    pub(super) fn sender_address(&self) -> &str {
        &self.sender_address
    }
    pub(super) fn recipient_address(&self) -> &str {
        &self.recipient_address
    }
    pub(super) fn amount_raw_units(&self) -> &str {
        &self.amount_raw_units
    }
    pub(super) const fn nonce(&self) -> u64 {
        self.nonce
    }
    pub(super) const fn tip_raw_units(&self) -> u64 {
        self.tip_raw_units
    }
    pub(super) const fn fee_limit_raw_units(&self) -> u64 {
        self.fee_limit_raw_units
    }
    pub(super) fn envelope_commitment_hex(&self) -> &str {
        &self.envelope_commitment_hex
    }
    pub(super) const fn submitted_at_unix_ms(&self) -> u64 {
        self.submitted_at_unix_ms
    }

    pub(super) fn envelope_binding(&self) -> ReconciliationEnvelopeBinding {
        ReconciliationEnvelopeBinding {
            attempt_id: self.attempt_id.clone(),
            transaction_id: self.transaction_id.clone(),
            commitment_hex: self.envelope_commitment_hex.clone(),
            parent_generation: self.parent_head_generation,
            parent_tag_hex: self.parent_head_authentication_tag_hex.clone(),
            reserved_prepared_generation: self.reserved_prepared_generation,
        }
    }
}

fn valid_transition(current: Option<ReconciliationPhaseTag>, next: ReconciliationPhaseTag) -> bool {
    matches!(
        (current, next),
        (None, ReconciliationPhaseTag::Prepared)
            | (
                Some(
                    ReconciliationPhaseTag::ResolvedNotAttempted
                        | ReconciliationPhaseTag::ResolvedRejected
                        | ReconciliationPhaseTag::ResolvedRecorded
                ),
                ReconciliationPhaseTag::Prepared
            )
            | (
                Some(ReconciliationPhaseTag::Prepared),
                ReconciliationPhaseTag::MayHaveBeenSubmitted
                    | ReconciliationPhaseTag::ResolvedNotAttempted
            )
            | (
                Some(ReconciliationPhaseTag::MayHaveBeenSubmitted),
                ReconciliationPhaseTag::AcceptedRecordingPending
                    | ReconciliationPhaseTag::ResolvedRejected
            )
            | (
                Some(ReconciliationPhaseTag::AcceptedRecordingPending),
                ReconciliationPhaseTag::ResolvedRecorded
            )
    )
}

fn same_attempt(left: &ReconciliationRecord, right: &ReconciliationRecord) -> bool {
    left.wallet_id == right.wallet_id
        && left.attempt_id == right.attempt_id
        && left.transaction_id == right.transaction_id
        && left.sender_address == right.sender_address
        && left.recipient_address == right.recipient_address
        && left.amount_raw_units == right.amount_raw_units
        && left.nonce == right.nonce
        && left.tip_raw_units == right.tip_raw_units
        && left.fee_limit_raw_units == right.fee_limit_raw_units
        && left.signed_body_digest_hex == right.signed_body_digest_hex
        && left.envelope_commitment_hex == right.envelope_commitment_hex
        && left.parent_head_generation == right.parent_head_generation
        && left.parent_head_authentication_tag_hex == right.parent_head_authentication_tag_hex
        && left.reserved_prepared_generation == right.reserved_prepared_generation
        && left.original_core_identity_fingerprint_hex
            == right.original_core_identity_fingerprint_hex
        && left.created_at_unix_ms == right.created_at_unix_ms
}

fn validate_record(
    record: &ReconciliationRecord,
    authenticator: &ReconciliationAuthenticator,
) -> Result<(), ReconciliationError> {
    let valid = record.schema == RECORD_SCHEMA
        && record.version == RECORD_VERSION
        && record.wallet_id == authenticator.wallet_id
        && record.sender_address == authenticator.sender_address
        && validate_wallet_id(&record.wallet_id).is_ok()
        && is_lower_hex(&record.attempt_id, 32)
        && is_lower_hex(&record.transaction_id, 32)
        && is_lower_hex(&record.sender_address, 32)
        && is_lower_hex(&record.recipient_address, 32)
        && record.sender_address != record.recipient_address
        && record
            .amount_raw_units
            .parse::<u128>()
            .is_ok_and(|amount| amount != 0)
        && is_lower_hex(&record.signed_body_digest_hex, 32)
        && is_lower_hex(&record.envelope_commitment_hex, 32)
        && is_lower_hex(&record.parent_head_authentication_tag_hex, 32)
        && record.reserved_prepared_generation
            == record.parent_head_generation.checked_add(1).unwrap_or(0)
        && record.store_generation >= record.reserved_prepared_generation
        && (record.phase.tag() != ReconciliationPhaseTag::Prepared
            || record.store_generation == record.reserved_prepared_generation)
        && is_lower_hex(&record.original_core_identity_fingerprint_hex, 32)
        && record.store_generation != 0;
    if !valid {
        return Err(ReconciliationError::InvalidRequest);
    }
    match &record.phase {
        ReconciliationPhase::AcceptedRecordingPending {
            accepted_tx_id,
            accepted_nonce,
            decision,
            compatibility_contract_digest_hex,
        } => {
            if accepted_tx_id != &record.transaction_id
                || accepted_nonce != &record.nonce
                || decision != "accept"
                || !is_lower_hex(compatibility_contract_digest_hex, 32)
            {
                return Err(ReconciliationError::InvalidRequest);
            }
        }
        ReconciliationPhase::ResolvedRejected {
            http_status,
            rejection_code,
            allowlist_digest_hex,
        } if !matches!(http_status, 400 | 422)
            || rejection_code.is_empty()
            || rejection_code.len() > 64
            || !rejection_code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            || !is_lower_hex(allowlist_digest_hex, 32) =>
        {
            return Err(ReconciliationError::InvalidRequest);
        }
        _ => {}
    }
    Ok(())
}

fn validate_wallet_id(wallet_id: &str) -> Result<(), ReconciliationError> {
    if wallet_id.is_empty()
        || wallet_id.len() > 64
        || !wallet_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Err(ReconciliationError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn is_lower_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn derive_key(seed: &WalletSeed, context: &'static str) -> SecretBox<[u8; AUTHENTICATION_BYTES]> {
    SecretBox::<[u8; AUTHENTICATION_BYTES]>::init_with_mut(|output| {
        let mut hasher = blake3::Hasher::new_derive_key(context);
        seed.with_exposed(|bytes| {
            hasher.update(bytes);
        });
        hasher.finalize_xof().fill(output);
        hasher.reset();
    })
}

fn authenticate(key: &[u8; AUTHENTICATION_BYTES], domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(key);
    hasher.update(domain);
    hasher.update(&[0]);
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

fn authenticate_record(
    authenticator: &ReconciliationAuthenticator,
    record: &mut ReconciliationRecord,
) -> Result<(), ReconciliationError> {
    record.authentication_tag_hex.clear();
    let payload = serde_json::to_vec(record).map_err(|_| ReconciliationError::InvalidRequest)?;
    record.authentication_tag_hex = hex::encode(authenticate(
        authenticator.record_key.expose_secret(),
        RECORD_DOMAIN,
        &payload,
    ));
    Ok(())
}

fn verify_record(
    authenticator: &ReconciliationAuthenticator,
    record: &ReconciliationRecord,
) -> Result<(), ReconciliationError> {
    validate_record(record, authenticator)?;
    let supplied = decode_tag(&record.authentication_tag_hex)?;
    let mut payload_record = record_without_tag(record)?;
    payload_record.authentication_tag_hex.clear();
    let payload = serde_json::to_vec(&payload_record)
        .map_err(|_| ReconciliationError::AuthenticationFailed)?;
    let expected = authenticate(
        authenticator.record_key.expose_secret(),
        RECORD_DOMAIN,
        &payload,
    );
    if constant_time_equal(&supplied, &expected) {
        Ok(())
    } else {
        Err(ReconciliationError::AuthenticationFailed)
    }
}

fn authenticate_head(
    authenticator: &ReconciliationAuthenticator,
    head: &mut ReconciliationHead,
) -> Result<(), ReconciliationError> {
    head.authentication_tag_hex.clear();
    let payload = serde_json::to_vec(head).map_err(|_| ReconciliationError::InvalidRequest)?;
    head.authentication_tag_hex = hex::encode(authenticate(
        authenticator.head_key.expose_secret(),
        HEAD_DOMAIN,
        &payload,
    ));
    Ok(())
}

fn verify_head(
    authenticator: &ReconciliationAuthenticator,
    head: &ReconciliationHead,
) -> Result<(), ReconciliationError> {
    if head.schema != HEAD_SCHEMA
        || head.version != HEAD_VERSION
        || head.wallet_id != authenticator.wallet_id
        || !is_tag_or_zero(&head.previous_head_tag_hex)
    {
        return Err(ReconciliationError::AuthenticationFailed);
    }
    if let ReconciliationHeadState::Transition {
        previous_generation,
        previous_previous_head_tag_hex,
        previous,
        next,
    } = &head.state
    {
        if previous_generation.checked_add(1) != Some(head.generation)
            || !is_tag_or_zero(previous_previous_head_tag_hex)
            || previous.record_generation != *previous_generation
            || next.record_generation != head.generation
        {
            return Err(ReconciliationError::AuthenticationFailed);
        }
    }
    let supplied = decode_tag(&head.authentication_tag_hex)?;
    let mut unsigned = head_without_tag(head)?;
    unsigned.authentication_tag_hex.clear();
    let payload =
        serde_json::to_vec(&unsigned).map_err(|_| ReconciliationError::AuthenticationFailed)?;
    let expected = authenticate(
        authenticator.head_key.expose_secret(),
        HEAD_DOMAIN,
        &payload,
    );
    if !constant_time_equal(&supplied, &expected) {
        return Err(ReconciliationError::AuthenticationFailed);
    }
    if head.generation == 0
        && (!matches!(
            &head.state,
            ReconciliationHeadState::Committed { position } if *position == empty_position()
        ) || head.previous_head_tag_hex != "00".repeat(AUTHENTICATION_BYTES))
    {
        return Err(ReconciliationError::AuthenticationFailed);
    }
    Ok(())
}

fn record_without_tag(
    record: &ReconciliationRecord,
) -> Result<ReconciliationRecord, ReconciliationError> {
    let bytes = serde_json::to_vec(record).map_err(|_| ReconciliationError::InvalidRequest)?;
    serde_json::from_slice(&bytes).map_err(|_| ReconciliationError::InvalidRequest)
}

fn head_without_tag(head: &ReconciliationHead) -> Result<ReconciliationHead, ReconciliationError> {
    let bytes = serde_json::to_vec(head).map_err(|_| ReconciliationError::InvalidRequest)?;
    serde_json::from_slice(&bytes).map_err(|_| ReconciliationError::InvalidRequest)
}

fn encode_record(record: &ReconciliationRecord) -> Result<Vec<u8>, ReconciliationError> {
    let bytes = serde_json::to_vec(record).map_err(|_| ReconciliationError::InvalidRequest)?;
    if bytes.is_empty() || bytes.len() > MAX_RECORD_BYTES {
        Err(ReconciliationError::ReconciliationTooLarge)
    } else {
        Ok(bytes)
    }
}

fn decode_record(
    bytes: &[u8],
    authenticator: &ReconciliationAuthenticator,
) -> Result<ReconciliationRecord, ReconciliationError> {
    let record: ReconciliationRecord =
        serde_json::from_slice(bytes).map_err(|_| ReconciliationError::AuthenticationFailed)?;
    verify_record(authenticator, &record)?;
    Ok(record)
}

fn position_for_record(record: &ReconciliationRecord, bytes: &[u8]) -> ReconciliationPosition {
    ReconciliationPosition {
        record_generation: record.store_generation,
        attempt_id: Some(record.attempt_id.clone()),
        phase: Some(record.phase.tag()),
        record_digest_hex: blake3::hash(bytes).to_hex().to_string(),
    }
}

fn empty_position() -> ReconciliationPosition {
    ReconciliationPosition {
        record_generation: 0,
        attempt_id: None,
        phase: None,
        record_digest_hex: "00".repeat(32),
    }
}

fn decode_tag(value: &str) -> Result<[u8; AUTHENTICATION_BYTES], ReconciliationError> {
    if !is_lower_hex(value, AUTHENTICATION_BYTES) {
        return Err(ReconciliationError::AuthenticationFailed);
    }
    let bytes = hex::decode(value).map_err(|_| ReconciliationError::AuthenticationFailed)?;
    let mut tag = [0_u8; AUTHENTICATION_BYTES];
    tag.copy_from_slice(&bytes);
    Ok(tag)
}

fn is_tag_or_zero(value: &str) -> bool {
    is_lower_hex(value, AUTHENTICATION_BYTES)
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn persist_json<T: Serialize>(
    path: &Path,
    value: &T,
    create_new: bool,
    maximum: usize,
) -> Result<(), ReconciliationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ReconciliationError::InvalidRequest)?;
    persist_bytes(path, &bytes, create_new, maximum)
}

fn persist_bytes(
    path: &Path,
    bytes: &[u8],
    create_new: bool,
    maximum: usize,
) -> Result<(), ReconciliationError> {
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(ReconciliationError::ReconciliationTooLarge);
    }
    let parent = path
        .parent()
        .ok_or(ReconciliationError::StorageUnavailable)?;
    let _directories =
        DirectoryChainGuard::ensure(parent).map_err(|_| ReconciliationError::StorageUnavailable)?;
    storage_security::protect_directory(parent)
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    let mut suffix = [0_u8; 16];
    getrandom::fill(&mut suffix).map_err(|_| ReconciliationError::StorageUnavailable)?;
    let staging_path = parent.join(format!("{STAGING_PREFIX}{}.tmp", hex::encode(suffix)));
    let mut staging = create_new_publishable_file(&staging_path)
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    storage_security::protect_open_file(&staging)
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    staging
        .write_all(bytes)
        .and_then(|_| staging.sync_all())
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    storage_security::verify_open_file(&staging)
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    if create_new {
        publish_open_file(&staging, path).map_err(|_| ReconciliationError::StorageUnavailable)
    } else {
        let existing =
            open_existing_file(path).map_err(|_| ReconciliationError::StorageUnavailable)?;
        storage_security::verify_open_file(&existing)
            .map_err(|_| ReconciliationError::StorageUnavailable)?;
        drop(existing);
        replace_with_open_file(&staging, path).map_err(|_| ReconciliationError::StorageUnavailable)
    }
}

fn read_protected(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>, ReconciliationError> {
    let parent = path
        .parent()
        .ok_or(ReconciliationError::StorageUnavailable)?;
    let _directories = match DirectoryChainGuard::open_existing(parent) {
        Ok(guard) => guard,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ReconciliationError::StorageUnavailable),
    };
    storage_security::verify_directory(parent)
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    let file = match open_existing_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ReconciliationError::StorageUnavailable),
    };
    storage_security::verify_open_file(&file)
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    let size = usize::try_from(
        file.metadata()
            .map_err(|_| ReconciliationError::StorageUnavailable)?
            .len(),
    )
    .map_err(|_| ReconciliationError::ReconciliationTooLarge)?;
    if size == 0 || size > maximum {
        return Err(ReconciliationError::ReconciliationTooLarge);
    }
    let mut bytes = Vec::with_capacity(size);
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReconciliationError::StorageUnavailable)?;
    if bytes.len() > maximum {
        return Err(ReconciliationError::ReconciliationTooLarge);
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::lifecycle::WalletCustodyPathAuthority;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    fn temp_wallet_path() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "vision-wallet-reconciliation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        storage_security::protect_directory(&path).unwrap();
        path.join("wallet.vault.json")
    }

    fn fixture() -> (
        PathBuf,
        ReconciliationStore,
        ReconciliationAuthenticator,
        ReconciliationRecord,
    ) {
        let vault_path = temp_wallet_path();
        let seed = WalletSeed::for_test(0x31);
        let authenticator = ReconciliationAuthenticator::new("wallet-primary", &seed).unwrap();
        let sender = derive_account_identity(&seed).address;
        let custody = WalletCustodyPathAuthority::issue_for_test(&vault_path);
        let store = ReconciliationStore::for_custody(&custody).unwrap();
        let record = ReconciliationRecord::prepared(
            "wallet-primary".to_string(),
            "11".repeat(32),
            "22".repeat(32),
            sender,
            "33".repeat(32),
            "42".to_string(),
            7,
            0,
            201,
            "44".repeat(32),
            "55".repeat(32),
            1_700_000_000_000,
        );
        (vault_path, store, authenticator, record)
    }

    #[test]
    fn linear_capabilities_commit_each_authenticated_phase() {
        let (vault_path, store, authenticator, record) = fixture();
        let grant = SubmissionActivationGrant::new_unchecked();
        let (live, write) = grant.split();
        let prepared = live
            .publish_prepared(&store, &authenticator, record)
            .unwrap();
        assert!(
            store
                .load_authenticated(&authenticator)
                .unwrap()
                .into_record()
                .unwrap()
                .phase_tag()
                == ReconciliationPhaseTag::Prepared
        );
        let may = prepared
            .publish_may_have_been_submitted(&store, &authenticator)
            .unwrap();
        let (may, _write) = may.combine(write).into_parts();
        let accepted = may
            .publish_accepted(&store, &authenticator, "22".repeat(32), 7, "66".repeat(32))
            .unwrap();
        let evidence = accepted.evidence().unwrap();
        assert_eq!(evidence.transaction_id(), "22".repeat(32));
        assert_eq!(evidence.amount_raw_units(), "42");
        accepted.resolve_recorded(&store, &authenticator).unwrap();
        assert!(
            store
                .load_authenticated(&authenticator)
                .unwrap()
                .into_record()
                .unwrap()
                .phase_tag()
                == ReconciliationPhaseTag::ResolvedRecorded
        );
        let _ = fs::remove_dir_all(vault_path.parent().unwrap());
    }

    #[test]
    fn phase_transition_cannot_rewrite_commitment_parent_or_reserved_generation() {
        for case in 0..4 {
            let (vault_path, store, authenticator, record) = fixture();
            publish_prepared_for_test(&store, &authenticator, record).unwrap();
            let current = store
                .load_authenticated(&authenticator)
                .unwrap()
                .into_record()
                .unwrap();
            let mut next = current.into_phase(ReconciliationPhase::MayHaveBeenSubmitted);
            match case {
                0 => next.envelope_commitment_hex = "99".repeat(32),
                1 => next.parent_head_generation = next.parent_head_generation.saturating_add(1),
                2 => next.parent_head_authentication_tag_hex = "99".repeat(32),
                _ => {
                    next.reserved_prepared_generation =
                        next.reserved_prepared_generation.saturating_add(1)
                }
            }
            assert_eq!(
                store
                    .transition(
                        &authenticator,
                        ExpectedTransition::Exact(ReconciliationPhaseTag::Prepared),
                        next,
                    )
                    .err(),
                Some(ReconciliationError::InvalidTransition)
            );
            let _ = fs::remove_dir_all(vault_path.parent().unwrap());
        }
    }

    #[test]
    fn independently_derived_authenticators_open_the_same_store() {
        let vault_path = temp_wallet_path();
        let seed = WalletSeed::for_test(0x41);
        let sender = derive_account_identity(&seed).address;
        let first = ReconciliationAuthenticator::new("primary", &seed).unwrap();
        let custody = WalletCustodyPathAuthority::issue_for_test(&vault_path);
        let store = ReconciliationStore::for_custody(&custody).unwrap();
        let record = ReconciliationRecord::prepared(
            "primary".to_string(),
            "11".repeat(32),
            "22".repeat(32),
            sender,
            "33".repeat(32),
            "42".to_string(),
            7,
            0,
            201,
            "44".repeat(32),
            "55".repeat(32),
            1,
        );
        SubmissionActivationGrant::new_unchecked()
            .split()
            .0
            .publish_prepared(&store, &first, record)
            .unwrap();
        let second = ReconciliationAuthenticator::new("primary", &seed).unwrap();
        assert!(store.load_authenticated(&second).is_ok());
        let _ = fs::remove_dir_all(vault_path.parent().unwrap());
    }

    #[test]
    fn duplicate_post_responses_cannot_create_rejection_authority() {
        let policy = crate::wallet::submission::SubmissionRejectionPolicy::for_test(&[
            (
                422,
                crate::wallet::submission::WalletSubmissionRejection::DuplicateCanonicalTxId,
            ),
            (
                422,
                crate::wallet::submission::WalletSubmissionRejection::DuplicateSenderNonce,
            ),
        ]);
        for code in ["duplicate_canonical_tx_id", "duplicate_sender_nonce"] {
            let body = format!(
                "{{\"status\":\"rejected\",\"tx_id\":\"{}\",\"current_nonce\":7,\"error\":{{\"code\":\"{code}\",\"message\":\"duplicate\"}}}}",
                "22".repeat(32)
            );
            assert!(matches!(
                crate::wallet::submission::classify_submission_response(
                    422,
                    body.as_bytes(),
                    &"22".repeat(32),
                    7,
                    &policy,
                ),
                crate::wallet::submission::PrivateSubmissionResponseDisposition::OutcomeUnknown
            ));
        }
    }

    #[test]
    fn wrong_wallet_and_tampered_record_fail_authentication() {
        let (vault_path, store, authenticator, record) = fixture();
        SubmissionActivationGrant::new_unchecked()
            .split()
            .0
            .publish_prepared(&store, &authenticator, record)
            .unwrap();
        let wrong_seed = WalletSeed::for_test(0x32);
        let wrong = ReconciliationAuthenticator::new("wallet-primary", &wrong_seed).unwrap();
        assert!(matches!(
            store.load_authenticated(&wrong),
            Err(ReconciliationError::AuthenticationFailed | ReconciliationError::InvalidRequest)
        ));
        let bytes = fs::read(&store.record_path).unwrap();
        let mut wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        wire["amount_raw_units"] = serde_json::Value::String("43".to_string());
        fs::write(&store.record_path, serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(
            store.load_authenticated(&authenticator).err(),
            Some(ReconciliationError::AuthenticationFailed)
        );
        let _ = fs::remove_dir_all(vault_path.parent().unwrap());
    }

    #[test]
    fn publication_interruptions_recover_only_the_authenticated_old_or_new_state() {
        for checkpoint in [
            ReconciliationTransitionCheckpoint::BeforeHeadTransition,
            ReconciliationTransitionCheckpoint::HeadTransitionPublished,
            ReconciliationTransitionCheckpoint::RecordPublished,
            ReconciliationTransitionCheckpoint::HeadCommitted,
        ] {
            let (vault_path, store, authenticator, record) = fixture();
            let reservation = store.reserve_prepared(&authenticator).unwrap();
            let mut record = record;
            record.bind_envelope_for_test(&reservation);
            let result = store.transition_with_checkpoint(
                &authenticator,
                ExpectedTransition::NewAttempt(reservation),
                record,
                |observed| {
                    if observed == checkpoint {
                        Err(ReconciliationError::StorageUnavailable)
                    } else {
                        Ok(())
                    }
                },
            );
            assert_eq!(result.err(), Some(ReconciliationError::StorageUnavailable));
            let recovered = store.load_authenticated(&authenticator).unwrap();
            let recovered_phase = recovered.into_record().map(|record| record.phase_tag());
            if matches!(
                checkpoint,
                ReconciliationTransitionCheckpoint::BeforeHeadTransition
                    | ReconciliationTransitionCheckpoint::HeadTransitionPublished
            ) {
                assert!(recovered_phase.is_none());
            } else {
                assert!(recovered_phase == Some(ReconciliationPhaseTag::Prepared));
            }
            let _ = fs::remove_dir_all(vault_path.parent().unwrap());
        }
    }

    #[test]
    fn every_terminal_head_interruption_preserves_the_old_or_new_phase_and_retry_boundary() {
        for terminal_case in 0..3 {
            for checkpoint in [
                ReconciliationTransitionCheckpoint::BeforeHeadTransition,
                ReconciliationTransitionCheckpoint::HeadTransitionPublished,
                ReconciliationTransitionCheckpoint::RecordPublished,
                ReconciliationTransitionCheckpoint::HeadCommitted,
            ] {
                let (vault_path, store, authenticator, record) = fixture();
                publish_prepared_for_test(&store, &authenticator, record).unwrap();
                let (expected, terminal, next) = match terminal_case {
                    0 => {
                        let current = store
                            .load_authenticated(&authenticator)
                            .unwrap()
                            .into_record()
                            .unwrap();
                        (
                            ReconciliationPhaseTag::Prepared,
                            ReconciliationPhaseTag::ResolvedNotAttempted,
                            current.into_phase(ReconciliationPhase::ResolvedNotAttempted),
                        )
                    }
                    1 => {
                        let current = store
                            .load_authenticated(&authenticator)
                            .unwrap()
                            .into_record()
                            .unwrap();
                        store
                            .transition(
                                &authenticator,
                                ExpectedTransition::Exact(ReconciliationPhaseTag::Prepared),
                                current.into_phase(ReconciliationPhase::MayHaveBeenSubmitted),
                            )
                            .unwrap();
                        let current = store
                            .load_authenticated(&authenticator)
                            .unwrap()
                            .into_record()
                            .unwrap();
                        (
                            ReconciliationPhaseTag::MayHaveBeenSubmitted,
                            ReconciliationPhaseTag::ResolvedRejected,
                            current.into_phase(ReconciliationPhase::ResolvedRejected {
                                http_status: 422,
                                rejection_code: "stale_nonce".to_string(),
                                allowlist_digest_hex: "77".repeat(32),
                            }),
                        )
                    }
                    _ => {
                        let current = store
                            .load_authenticated(&authenticator)
                            .unwrap()
                            .into_record()
                            .unwrap();
                        store
                            .transition(
                                &authenticator,
                                ExpectedTransition::Exact(ReconciliationPhaseTag::Prepared),
                                current.into_phase(ReconciliationPhase::MayHaveBeenSubmitted),
                            )
                            .unwrap();
                        let current = store
                            .load_authenticated(&authenticator)
                            .unwrap()
                            .into_record()
                            .unwrap();
                        store
                            .transition(
                                &authenticator,
                                ExpectedTransition::Exact(
                                    ReconciliationPhaseTag::MayHaveBeenSubmitted,
                                ),
                                current.into_phase(ReconciliationPhase::AcceptedRecordingPending {
                                    accepted_tx_id: "22".repeat(32),
                                    accepted_nonce: 7,
                                    decision: "accept".to_string(),
                                    compatibility_contract_digest_hex: "66".repeat(32),
                                }),
                            )
                            .unwrap();
                        let current = store
                            .load_authenticated(&authenticator)
                            .unwrap()
                            .into_record()
                            .unwrap();
                        (
                            ReconciliationPhaseTag::AcceptedRecordingPending,
                            ReconciliationPhaseTag::ResolvedRecorded,
                            current.into_phase(ReconciliationPhase::ResolvedRecorded),
                        )
                    }
                };
                let result = store.transition_with_checkpoint(
                    &authenticator,
                    ExpectedTransition::Exact(expected),
                    next,
                    |observed| {
                        if observed == checkpoint {
                            Err(ReconciliationError::StorageUnavailable)
                        } else {
                            Ok(())
                        }
                    },
                );
                assert_eq!(result.err(), Some(ReconciliationError::StorageUnavailable));
                let recovered = store
                    .load_authenticated(&authenticator)
                    .unwrap()
                    .into_record()
                    .unwrap()
                    .phase_tag();
                let terminal_was_published = matches!(
                    checkpoint,
                    ReconciliationTransitionCheckpoint::RecordPublished
                        | ReconciliationTransitionCheckpoint::HeadCommitted
                );
                assert!(
                    recovered
                        == if terminal_was_published {
                            terminal
                        } else {
                            expected
                        }
                );
                let next_attempt = publish_prepared_for_test(
                    &store,
                    &authenticator,
                    replacement_record_for_test(),
                );
                assert_eq!(next_attempt.is_ok(), terminal_was_published);
                let _ = fs::remove_dir_all(vault_path.parent().unwrap());
            }
        }
    }

    fn replacement_record_for_test() -> ReconciliationRecord {
        let seed = WalletSeed::for_test(0x31);
        ReconciliationRecord::prepared(
            "wallet-primary".to_string(),
            "99".repeat(32),
            "88".repeat(32),
            derive_account_identity(&seed).address,
            "33".repeat(32),
            "43".to_string(),
            8,
            0,
            201,
            "44".repeat(32),
            "55".repeat(32),
            2,
        )
    }

    #[test]
    fn missing_mismatched_oversized_and_unknown_store_data_fail_closed() {
        for remove_record in [true, false] {
            let (vault_path, store, authenticator, record) = fixture();
            publish_prepared_for_test(&store, &authenticator, record).unwrap();
            if remove_record {
                fs::remove_file(&store.record_path).unwrap();
            } else {
                fs::remove_file(&store.head_path).unwrap();
            }
            assert!(store.load_authenticated(&authenticator).is_err());
            let _ = fs::remove_dir_all(vault_path.parent().unwrap());
        }

        let (vault_path, store, authenticator, record) = fixture();
        publish_prepared_for_test(&store, &authenticator, record).unwrap();
        fs::write(&store.record_path, vec![b'x'; MAX_RECORD_BYTES + 1]).unwrap();
        assert_eq!(
            store.load_authenticated(&authenticator).err(),
            Some(ReconciliationError::ReconciliationTooLarge)
        );
        let _ = fs::remove_dir_all(vault_path.parent().unwrap());

        let (vault_path, store, authenticator, record) = fixture();
        publish_prepared_for_test(&store, &authenticator, record).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&store.record_path).unwrap()).unwrap();
        value["version"] = serde_json::json!(999);
        value["unexpected"] = serde_json::json!(true);
        fs::write(&store.record_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            store.load_authenticated(&authenticator).err(),
            Some(ReconciliationError::AuthenticationFailed)
        );
        let _ = fs::remove_dir_all(vault_path.parent().unwrap());
    }

    #[test]
    fn fixed_store_discovery_has_no_write_or_signing_capability() {
        let source = include_str!("reconciliation.rs");
        assert!(!source.contains(&["PO", "ST /transactions"].concat()));
        assert!(!source.contains(&["sign_confirmed_", "cash_transfer"].concat()));
        assert!(!source.contains(&["WalletSeed", ") -> &"].concat()));
        assert!(!source.contains(&["#[tauri", "::command]"].concat()));
        assert!(source.contains("ReconciliationDiscoveryPermit"));
        assert!(source.contains("RestartReconciliationPermit"));
    }
}
