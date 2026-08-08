#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "local wallet activity remains internal until wallet submission is approved"
    )
)]

#[cfg(windows)]
use super::secure_filesystem::{
    create_new_publishable_file, open_existing_file, publish_open_file, replace_with_open_file,
    DirectoryChainGuard,
};
use super::{
    account::derive_account_identity,
    receipt::WalletReceiptObservation,
    reconciliation::AcceptedSubmissionEvidence,
    secrets::WalletSeed,
    storage_security,
    submission::WalletSubmissionOutcome,
    transaction::{canonical_transaction_id, VisionTransaction},
};
use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

const JOURNAL_SCHEMA: &str = "vision-desktop-wallet-activity";
const JOURNAL_VERSION: u32 = 2;
const JOURNAL_AUTHENTICATION_DOMAIN: &[u8] = b"vision-desktop-wallet-activity-authentication-v1";
const JOURNAL_KEY_DERIVATION_CONTEXT: &str =
    "com.vision.desktop.wallet-activity-journal-authentication-key.v1";
const JOURNAL_HEAD_SCHEMA: &str = "vision-desktop-wallet-activity-head";
const JOURNAL_HEAD_VERSION: u32 = 1;
const JOURNAL_HEAD_AUTHENTICATION_DOMAIN: &[u8] =
    b"vision-desktop-wallet-activity-head-authentication-v1";
const JOURNAL_HEAD_KEY_DERIVATION_CONTEXT: &str =
    "com.vision.desktop.wallet-activity-journal-head-authentication-key.v1";
const AUTHENTICATION_TAG_BYTES: usize = 32;
const MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_JOURNAL_HEAD_BYTES: usize = 4 * 1024;
const MAX_EVENT_BYTES: usize = 16 * 1024;
const MAX_EVENTS: usize = 10_000;
const JOURNAL_STAGING_PREFIX: &str = ".wallet-activity-stage-";
const JOURNAL_HEAD_SUFFIX: &str = ".head.json";

static JOURNAL_WRITE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::wallet) struct WalletActivityRecord {
    pub tx_id: String,
    pub sender_address: String,
    pub recipient_address: String,
    pub amount_raw_units: String,
    pub nonce: u64,
    pub tip_raw_units: u64,
    pub fee_limit_raw_units: u64,
    pub submitted_at_unix_ms: u64,
    pub last_observed_at_unix_ms: Option<u64>,
    pub observation: WalletReceiptObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::wallet) struct WalletActivityJournal {
    wallet_id: String,
    records: Vec<WalletActivityRecord>,
    event_count: usize,
    last_authentication_tag: [u8; AUTHENTICATION_TAG_BYTES],
}

struct LoadedJournal {
    journal: WalletActivityJournal,
    encoded: Vec<u8>,
    head_generation: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalHead {
    schema: String,
    version: u32,
    wallet_id: String,
    generation: u64,
    state: JournalHeadState,
    authentication_tag_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JournalHeadState {
    Committed {
        position: JournalHeadPosition,
    },
    Transition {
        previous: JournalHeadPosition,
        next: JournalHeadPosition,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalHeadPosition {
    sequence: u64,
    authentication_tag_hex: String,
}

impl WalletActivityJournal {
    pub(in crate::wallet) fn wallet_id(&self) -> &str {
        &self.wallet_id
    }

    pub(in crate::wallet) fn records(&self) -> &[WalletActivityRecord] {
        &self.records
    }
}

/// Seed-owned authority for authenticating one wallet's local activity journal.
///
/// It is neither serializable nor cloneable and never exposes the seed. Its
/// wallet identity is checked against every accepted transaction before an
/// event can be authenticated.
pub(in crate::wallet) struct WalletJournalAuthenticator<'wallet> {
    wallet_id: &'wallet str,
    authentication_key: SecretBox<[u8; AUTHENTICATION_TAG_BYTES]>,
    head_authentication_key: SecretBox<[u8; AUTHENTICATION_TAG_BYTES]>,
    sender_address: String,
}

impl<'wallet> WalletJournalAuthenticator<'wallet> {
    pub(in crate::wallet) fn new(
        wallet_id: &'wallet str,
        seed: &WalletSeed,
    ) -> Result<Self, WalletJournalError> {
        validate_wallet_id(wallet_id)?;
        let authentication_key = derive_authentication_key(seed, JOURNAL_KEY_DERIVATION_CONTEXT);
        let head_authentication_key =
            derive_authentication_key(seed, JOURNAL_HEAD_KEY_DERIVATION_CONTEXT);
        Ok(Self {
            wallet_id,
            authentication_key,
            head_authentication_key,
            sender_address: derive_account_identity(seed).address,
        })
    }

    fn authenticate(&self, payload: &[u8]) -> [u8; AUTHENTICATION_TAG_BYTES] {
        authenticate_payload(
            self.authentication_key.expose_secret(),
            JOURNAL_AUTHENTICATION_DOMAIN,
            payload,
        )
    }

    fn authenticate_head(&self, payload: &[u8]) -> [u8; AUTHENTICATION_TAG_BYTES] {
        authenticate_payload(
            self.head_authentication_key.expose_secret(),
            JOURNAL_HEAD_AUTHENTICATION_DOMAIN,
            payload,
        )
    }
}

fn derive_authentication_key(
    seed: &WalletSeed,
    context: &'static str,
) -> SecretBox<[u8; AUTHENTICATION_TAG_BYTES]> {
    SecretBox::<[u8; AUTHENTICATION_TAG_BYTES]>::init_with_mut(|output| {
        let mut hasher = blake3::Hasher::new_derive_key(context);
        seed.with_exposed(|seed_bytes| {
            hasher.update(seed_bytes);
        });
        hasher.finalize_xof().fill(output);
        hasher.reset();
    })
}

fn authenticate_payload(
    key: &[u8; AUTHENTICATION_TAG_BYTES],
    domain: &[u8],
    payload: &[u8],
) -> [u8; AUTHENTICATION_TAG_BYTES] {
    let mut input = Vec::with_capacity(domain.len() + payload.len());
    input.extend_from_slice(domain);
    input.extend_from_slice(payload);
    *blake3::keyed_hash(key, &input).as_bytes()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletJournalError {
    InvalidWalletId,
    InvalidTransaction,
    SubmissionNotAccepted,
    SubmissionMismatch,
    DuplicateTransaction,
    UnknownTransaction,
    InvalidObservation,
    InvalidOrUnsupportedFormat,
    JournalTooLarge,
    StorageUnavailable,
}

impl fmt::Display for WalletJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidWalletId => "wallet activity identifier is invalid",
            Self::InvalidTransaction => "wallet activity transaction is invalid",
            Self::SubmissionNotAccepted => "wallet activity requires an accepted submission",
            Self::SubmissionMismatch => "wallet activity submission does not match the transaction",
            Self::DuplicateTransaction => "wallet activity already contains this transaction",
            Self::UnknownTransaction => "wallet activity does not contain this transaction",
            Self::InvalidObservation => "wallet activity observation is invalid",
            Self::InvalidOrUnsupportedFormat => "wallet activity format is invalid or unsupported",
            Self::JournalTooLarge => "wallet activity has reached its safe storage limit",
            Self::StorageUnavailable => "wallet activity storage is unavailable",
        })
    }
}

impl std::error::Error for WalletJournalError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalEvent {
    schema: String,
    version: u32,
    wallet_id: String,
    sequence: u64,
    tx_id: String,
    recorded_at_unix_ms: u64,
    event: JournalEventData,
    previous_authentication_tag_hex: String,
    authentication_tag_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
enum JournalEventData {
    Submitted(SubmittedEvent),
    Observation(ObservationEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SubmittedEvent {
    sender_address: String,
    recipient_address: String,
    amount_raw_units: String,
    nonce: u64,
    tip_raw_units: u64,
    fee_limit_raw_units: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ObservationEvent {
    observation: WalletReceiptObservation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CashTransferArgs {
    to: String,
    amount: u128,
}

/// Appends public metadata only after the exact Core response parser has
/// classified a submission as accepted. Signed bytes and signatures are not
/// persisted in this journal.
#[cfg(test)]
pub(in crate::wallet) fn append_accepted_submission(
    path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
    transaction: &VisionTransaction,
    outcome: &WalletSubmissionOutcome,
    submitted_at_unix_ms: u64,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    let loaded = load_journal_unlocked(path, authenticator)?;
    let mut journal = loaded.journal;
    let (tx_id, submitted) = accepted_submission(transaction, outcome)?;
    if submitted.sender_address != authenticator.sender_address {
        return Err(WalletJournalError::SubmissionMismatch);
    }
    if journal.records.iter().any(|record| record.tx_id == tx_id) {
        return Err(WalletJournalError::DuplicateTransaction);
    }
    let mut event = JournalEvent {
        schema: JOURNAL_SCHEMA.to_string(),
        version: JOURNAL_VERSION,
        wallet_id: authenticator.wallet_id.to_string(),
        sequence: next_sequence(journal.event_count)?,
        tx_id,
        recorded_at_unix_ms: submitted_at_unix_ms,
        event: JournalEventData::Submitted(submitted),
        previous_authentication_tag_hex: hex::encode(journal.last_authentication_tag),
        authentication_tag_hex: String::new(),
    };
    authenticate_event(authenticator, &mut event)?;
    append_event(
        path,
        authenticator,
        loaded.encoded.as_slice(),
        &event,
        journal.event_count == 0,
        loaded.head_generation,
    )?;
    apply_authenticated_event(&mut journal, &event)?;
    Ok(journal)
}

/// Appends only a non-forgeable, reconciliation-authenticated acceptance capability.
pub(in crate::wallet) fn append_accepted_evidence(
    path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
    evidence: &AcceptedSubmissionEvidence,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    if evidence.wallet_id() != authenticator.wallet_id
        || evidence.sender_address() != authenticator.sender_address
    {
        return Err(WalletJournalError::SubmissionMismatch);
    }
    validate_tx_id(evidence.transaction_id())?;
    if !is_lowercase_hex_32_bytes(evidence.recipient_address())
        || evidence.recipient_address() == evidence.sender_address()
        || evidence
            .amount_raw_units()
            .parse::<u128>()
            .ok()
            .is_none_or(|amount| amount == 0)
    {
        return Err(WalletJournalError::InvalidTransaction);
    }
    let loaded = load_journal_unlocked(path, authenticator)?;
    let mut journal = loaded.journal;
    let submitted = SubmittedEvent {
        sender_address: evidence.sender_address().to_string(),
        recipient_address: evidence.recipient_address().to_string(),
        amount_raw_units: evidence.amount_raw_units().to_string(),
        nonce: evidence.nonce(),
        tip_raw_units: evidence.tip_raw_units(),
        fee_limit_raw_units: evidence.fee_limit_raw_units(),
    };
    if let Some(existing) = journal
        .records
        .iter()
        .find(|record| record.tx_id == evidence.transaction_id())
    {
        let exact = existing.sender_address == submitted.sender_address
            && existing.recipient_address == submitted.recipient_address
            && existing.amount_raw_units == submitted.amount_raw_units
            && existing.nonce == submitted.nonce
            && existing.tip_raw_units == submitted.tip_raw_units
            && existing.fee_limit_raw_units == submitted.fee_limit_raw_units;
        return if exact {
            Ok(journal)
        } else {
            Err(WalletJournalError::SubmissionMismatch)
        };
    }
    let mut event = JournalEvent {
        schema: JOURNAL_SCHEMA.to_string(),
        version: JOURNAL_VERSION,
        wallet_id: authenticator.wallet_id.to_string(),
        sequence: next_sequence(journal.event_count)?,
        tx_id: evidence.transaction_id().to_string(),
        recorded_at_unix_ms: evidence.submitted_at_unix_ms(),
        event: JournalEventData::Submitted(submitted),
        previous_authentication_tag_hex: hex::encode(journal.last_authentication_tag),
        authentication_tag_hex: String::new(),
    };
    authenticate_event(authenticator, &mut event)?;
    append_event(
        path,
        authenticator,
        loaded.encoded.as_slice(),
        &event,
        journal.event_count == 0,
        loaded.head_generation,
    )?;
    apply_authenticated_event(&mut journal, &event)?;
    Ok(journal)
}

/// Records the newest validated Core observation for a locally submitted
/// transaction. Repeated identical observations do not grow the journal.
pub(in crate::wallet) fn append_receipt_observation(
    path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
    tx_id: &str,
    observation: &WalletReceiptObservation,
    observed_at_unix_ms: u64,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    validate_tx_id(tx_id)?;
    validate_observation(observation)?;
    let loaded = load_journal_unlocked(path, authenticator)?;
    let mut journal = loaded.journal;
    let record = journal
        .records
        .iter()
        .find(|record| record.tx_id == tx_id)
        .ok_or(WalletJournalError::UnknownTransaction)?;
    if record.observation == *observation {
        return Ok(journal);
    }
    let mut event = JournalEvent {
        schema: JOURNAL_SCHEMA.to_string(),
        version: JOURNAL_VERSION,
        wallet_id: authenticator.wallet_id.to_string(),
        sequence: next_sequence(journal.event_count)?,
        tx_id: tx_id.to_string(),
        recorded_at_unix_ms: observed_at_unix_ms,
        event: JournalEventData::Observation(ObservationEvent {
            observation: observation.clone(),
        }),
        previous_authentication_tag_hex: hex::encode(journal.last_authentication_tag),
        authentication_tag_hex: String::new(),
    };
    authenticate_event(authenticator, &mut event)?;
    append_event(
        path,
        authenticator,
        loaded.encoded.as_slice(),
        &event,
        false,
        loaded.head_generation,
    )?;
    apply_authenticated_event(&mut journal, &event)?;
    Ok(journal)
}

pub(in crate::wallet) fn load_activity_journal(
    path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    load_journal_unlocked(path, authenticator).map(|loaded| loaded.journal)
}

fn accepted_submission(
    transaction: &VisionTransaction,
    outcome: &WalletSubmissionOutcome,
) -> Result<(String, SubmittedEvent), WalletJournalError> {
    let WalletSubmissionOutcome::Accepted {
        tx_id,
        current_nonce,
    } = outcome
    else {
        return Err(WalletJournalError::SubmissionNotAccepted);
    };
    validate_tx_id(tx_id)?;
    let canonical_id = canonical_transaction_id(transaction)
        .map_err(|_| WalletJournalError::InvalidTransaction)?;
    if canonical_id != *tx_id || *current_nonce != transaction.nonce {
        return Err(WalletJournalError::SubmissionMismatch);
    }
    if transaction.module != "cash"
        || transaction.method != "transfer"
        || !is_lowercase_hex_32_bytes(&transaction.sender_pubkey)
        || !is_lowercase_hex_64_bytes(&transaction.sig)
    {
        return Err(WalletJournalError::InvalidTransaction);
    }
    let args: CashTransferArgs = serde_json::from_slice(&transaction.args)
        .map_err(|_| WalletJournalError::InvalidTransaction)?;
    if !is_lowercase_hex_32_bytes(&args.to)
        || args.to == transaction.sender_pubkey
        || args.amount == 0
    {
        return Err(WalletJournalError::InvalidTransaction);
    }
    Ok((
        tx_id.clone(),
        SubmittedEvent {
            sender_address: transaction.sender_pubkey.clone(),
            recipient_address: args.to,
            amount_raw_units: args.amount.to_string(),
            nonce: transaction.nonce,
            tip_raw_units: transaction.tip,
            fee_limit_raw_units: transaction.fee_limit,
        },
    ))
}

fn load_journal_unlocked(
    path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
) -> Result<LoadedJournal, WalletJournalError> {
    let mut journal = WalletActivityJournal {
        wallet_id: authenticator.wallet_id.to_string(),
        records: Vec::new(),
        event_count: 0,
        last_authentication_tag: [0_u8; AUTHENTICATION_TAG_BYTES],
    };
    let bytes = read_journal_bytes(path)?.unwrap_or_default();
    if !bytes.is_empty() {
        if bytes.last() != Some(&b'\n') {
            return Err(WalletJournalError::InvalidOrUnsupportedFormat);
        }
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            if line.len() > MAX_EVENT_BYTES || journal.event_count >= MAX_EVENTS {
                return Err(WalletJournalError::JournalTooLarge);
            }
            let event: JournalEvent = serde_json::from_slice(line)
                .map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)?;
            if event.schema != JOURNAL_SCHEMA
                || event.version != JOURNAL_VERSION
                || event.wallet_id != authenticator.wallet_id
                || event.sequence != next_sequence(journal.event_count)?
            {
                return Err(WalletJournalError::InvalidOrUnsupportedFormat);
            }
            verify_event_authentication(authenticator, &journal, &event)?;
            apply_authenticated_event(&mut journal, &event)?;
        }
        if journal.event_count == 0 {
            return Err(WalletJournalError::InvalidOrUnsupportedFormat);
        }
    }
    let head_generation = verify_or_recover_journal_head(path, authenticator, &journal)?;
    Ok(LoadedJournal {
        journal,
        encoded: bytes,
        head_generation,
    })
}

fn verify_or_recover_journal_head(
    journal_path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
    journal: &WalletActivityJournal,
) -> Result<Option<u64>, WalletJournalError> {
    let head_path = journal_head_path(journal_path)?;
    let Some(bytes) = read_journal_head_bytes(&head_path)? else {
        return if journal.event_count == 0 {
            Ok(None)
        } else {
            Err(WalletJournalError::InvalidOrUnsupportedFormat)
        };
    };
    let head: JournalHead = serde_json::from_slice(&bytes)
        .map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)?;
    verify_journal_head(authenticator, &head)?;
    let actual = journal_head_position(journal)?;
    let committed_position = match &head.state {
        JournalHeadState::Committed { position } => {
            if *position != actual {
                return Err(WalletJournalError::InvalidOrUnsupportedFormat);
            }
            return Ok(Some(head.generation));
        }
        JournalHeadState::Transition { previous, next } if *previous == actual => previous.clone(),
        JournalHeadState::Transition { next, .. } if *next == actual => next.clone(),
        JournalHeadState::Transition { .. } => {
            return Err(WalletJournalError::InvalidOrUnsupportedFormat)
        }
    };

    let mut recovered = JournalHead {
        schema: JOURNAL_HEAD_SCHEMA.to_string(),
        version: JOURNAL_HEAD_VERSION,
        wallet_id: authenticator.wallet_id.to_string(),
        generation: head.generation,
        state: JournalHeadState::Committed {
            position: committed_position,
        },
        authentication_tag_hex: String::new(),
    };
    authenticate_journal_head(authenticator, &mut recovered)?;
    persist_journal_head(&head_path, &recovered, false)?;
    Ok(Some(head.generation))
}

fn journal_head_path(journal_path: &Path) -> Result<PathBuf, WalletJournalError> {
    let file_name = journal_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(WalletJournalError::StorageUnavailable)?;
    if file_name.is_empty() || file_name.contains(':') {
        return Err(WalletJournalError::StorageUnavailable);
    }
    Ok(journal_path.with_file_name(format!("{file_name}{JOURNAL_HEAD_SUFFIX}")))
}

fn journal_head_position(
    journal: &WalletActivityJournal,
) -> Result<JournalHeadPosition, WalletJournalError> {
    Ok(JournalHeadPosition {
        sequence: u64::try_from(journal.event_count)
            .map_err(|_| WalletJournalError::JournalTooLarge)?,
        authentication_tag_hex: hex::encode(journal.last_authentication_tag),
    })
}

fn authenticate_journal_head(
    authenticator: &WalletJournalAuthenticator<'_>,
    head: &mut JournalHead,
) -> Result<(), WalletJournalError> {
    let payload = journal_head_authentication_payload(head)?;
    head.authentication_tag_hex = hex::encode(authenticator.authenticate_head(&payload));
    Ok(())
}

fn verify_journal_head(
    authenticator: &WalletJournalAuthenticator<'_>,
    head: &JournalHead,
) -> Result<(), WalletJournalError> {
    if head.schema != JOURNAL_HEAD_SCHEMA
        || head.version != JOURNAL_HEAD_VERSION
        || head.wallet_id != authenticator.wallet_id
        || head.generation == 0
    {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    let positions = match &head.state {
        JournalHeadState::Committed { position } => [Some(position), None],
        JournalHeadState::Transition { previous, next } => {
            if next.sequence
                != previous
                    .sequence
                    .checked_add(1)
                    .ok_or(WalletJournalError::InvalidOrUnsupportedFormat)?
            {
                return Err(WalletJournalError::InvalidOrUnsupportedFormat);
            }
            [Some(previous), Some(next)]
        }
    };
    for position in positions.into_iter().flatten() {
        let tag = decode_authentication_tag(&position.authentication_tag_hex)?;
        if position.sequence > MAX_EVENTS as u64
            || position.sequence > head.generation
            || (position.sequence == 0 && tag != [0_u8; AUTHENTICATION_TAG_BYTES])
            || (position.sequence != 0 && tag == [0_u8; AUTHENTICATION_TAG_BYTES])
        {
            return Err(WalletJournalError::InvalidOrUnsupportedFormat);
        }
    }
    let supplied = decode_authentication_tag(&head.authentication_tag_hex)?;
    let expected = authenticator.authenticate_head(&journal_head_authentication_payload(head)?);
    if !constant_time_equal(&supplied, &expected) {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    Ok(())
}

fn journal_head_authentication_payload(head: &JournalHead) -> Result<Vec<u8>, WalletJournalError> {
    serde_json::to_vec(&(
        &head.schema,
        head.version,
        &head.wallet_id,
        head.generation,
        &head.state,
    ))
    .map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)
}

#[cfg(windows)]
fn read_journal_bytes(path: &Path) -> Result<Option<Vec<u8>>, WalletJournalError> {
    read_protected_file_bytes(path, MAX_JOURNAL_BYTES)
}

#[cfg(windows)]
fn read_journal_head_bytes(path: &Path) -> Result<Option<Vec<u8>>, WalletJournalError> {
    read_protected_file_bytes(path, MAX_JOURNAL_HEAD_BYTES)
}

#[cfg(windows)]
fn read_protected_file_bytes(
    path: &Path,
    maximum_bytes: usize,
) -> Result<Option<Vec<u8>>, WalletJournalError> {
    let parent = path
        .parent()
        .ok_or(WalletJournalError::StorageUnavailable)?;
    let _directories = match DirectoryChainGuard::open_existing(parent) {
        Ok(guard) => guard,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(WalletJournalError::StorageUnavailable),
    };
    storage_security::verify_directory(parent)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    let file = match open_existing_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(WalletJournalError::StorageUnavailable),
    };
    storage_security::verify_open_file(&file)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    let file_size =
        usize::try_from(metadata.len()).map_err(|_| WalletJournalError::JournalTooLarge)?;
    if file_size == 0 || file_size > maximum_bytes {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    let mut bytes = Vec::with_capacity(file_size);
    file.take(maximum_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    if bytes.len() > maximum_bytes {
        return Err(WalletJournalError::JournalTooLarge);
    }
    Ok(Some(bytes))
}

#[cfg(not(windows))]
fn read_journal_bytes(path: &Path) -> Result<Option<Vec<u8>>, WalletJournalError> {
    read_protected_file_bytes(path, MAX_JOURNAL_BYTES)
}

#[cfg(not(windows))]
fn read_journal_head_bytes(path: &Path) -> Result<Option<Vec<u8>>, WalletJournalError> {
    read_protected_file_bytes(path, MAX_JOURNAL_HEAD_BYTES)
}

#[cfg(not(windows))]
fn read_protected_file_bytes(
    path: &Path,
    maximum_bytes: usize,
) -> Result<Option<Vec<u8>>, WalletJournalError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(WalletJournalError::StorageUnavailable),
    };
    let parent = path
        .parent()
        .ok_or(WalletJournalError::StorageUnavailable)?;
    storage_security::verify_directory(parent)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    storage_security::verify_file(path).map_err(|_| WalletJournalError::StorageUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(WalletJournalError::StorageUnavailable);
    }
    let file_size =
        usize::try_from(metadata.len()).map_err(|_| WalletJournalError::JournalTooLarge)?;
    if file_size == 0 || file_size > maximum_bytes {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    let mut bytes = Vec::with_capacity(file_size);
    fs::File::open(path)
        .map_err(|_| WalletJournalError::StorageUnavailable)?
        .take(maximum_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    if bytes.len() > maximum_bytes {
        return Err(WalletJournalError::JournalTooLarge);
    }
    Ok(Some(bytes))
}

fn authenticate_event(
    authenticator: &WalletJournalAuthenticator<'_>,
    event: &mut JournalEvent,
) -> Result<(), WalletJournalError> {
    let payload = event_authentication_payload(event)?;
    event.authentication_tag_hex = hex::encode(authenticator.authenticate(&payload));
    Ok(())
}

fn verify_event_authentication(
    authenticator: &WalletJournalAuthenticator<'_>,
    journal: &WalletActivityJournal,
    event: &JournalEvent,
) -> Result<(), WalletJournalError> {
    let previous = decode_authentication_tag(&event.previous_authentication_tag_hex)?;
    if !constant_time_equal(&previous, &journal.last_authentication_tag) {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    let supplied = decode_authentication_tag(&event.authentication_tag_hex)?;
    let expected = authenticator.authenticate(&event_authentication_payload(event)?);
    if !constant_time_equal(&supplied, &expected) {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    Ok(())
}

fn event_authentication_payload(event: &JournalEvent) -> Result<Vec<u8>, WalletJournalError> {
    serde_json::to_vec(&(
        &event.schema,
        event.version,
        &event.wallet_id,
        event.sequence,
        &event.tx_id,
        event.recorded_at_unix_ms,
        &event.event,
        &event.previous_authentication_tag_hex,
    ))
    .map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)
}

fn decode_authentication_tag(
    encoded: &str,
) -> Result<[u8; AUTHENTICATION_TAG_BYTES], WalletJournalError> {
    if encoded.len() != AUTHENTICATION_TAG_BYTES * 2
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    let mut tag = [0_u8; AUTHENTICATION_TAG_BYTES];
    hex::decode_to_slice(encoded, &mut tag)
        .map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)?;
    Ok(tag)
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn apply_authenticated_event(
    journal: &mut WalletActivityJournal,
    event: &JournalEvent,
) -> Result<(), WalletJournalError> {
    apply_event(journal, event)?;
    journal.last_authentication_tag = decode_authentication_tag(&event.authentication_tag_hex)?;
    Ok(())
}

fn apply_event(
    journal: &mut WalletActivityJournal,
    event: &JournalEvent,
) -> Result<(), WalletJournalError> {
    validate_tx_id(&event.tx_id)?;
    match &event.event {
        JournalEventData::Submitted(submitted) => {
            validate_submitted_event(submitted)?;
            if journal
                .records
                .iter()
                .any(|record| record.tx_id == event.tx_id)
            {
                return Err(WalletJournalError::DuplicateTransaction);
            }
            journal.records.push(WalletActivityRecord {
                tx_id: event.tx_id.clone(),
                sender_address: submitted.sender_address.clone(),
                recipient_address: submitted.recipient_address.clone(),
                amount_raw_units: submitted.amount_raw_units.clone(),
                nonce: submitted.nonce,
                tip_raw_units: submitted.tip_raw_units,
                fee_limit_raw_units: submitted.fee_limit_raw_units,
                submitted_at_unix_ms: event.recorded_at_unix_ms,
                last_observed_at_unix_ms: None,
                observation: WalletReceiptObservation::NotFound,
            });
        }
        JournalEventData::Observation(observation) => {
            validate_observation(&observation.observation)?;
            let record = journal
                .records
                .iter_mut()
                .find(|record| record.tx_id == event.tx_id)
                .ok_or(WalletJournalError::UnknownTransaction)?;
            record.observation = observation.observation.clone();
            record.last_observed_at_unix_ms = Some(event.recorded_at_unix_ms);
        }
    }
    journal.event_count = journal
        .event_count
        .checked_add(1)
        .ok_or(WalletJournalError::JournalTooLarge)?;
    Ok(())
}

fn validate_submitted_event(event: &SubmittedEvent) -> Result<(), WalletJournalError> {
    if !is_lowercase_hex_32_bytes(&event.sender_address)
        || !is_lowercase_hex_32_bytes(&event.recipient_address)
        || event.sender_address == event.recipient_address
    {
        return Err(WalletJournalError::InvalidTransaction);
    }
    let amount = event
        .amount_raw_units
        .parse::<u128>()
        .map_err(|_| WalletJournalError::InvalidTransaction)?;
    if amount == 0 || amount.to_string() != event.amount_raw_units {
        return Err(WalletJournalError::InvalidTransaction);
    }
    Ok(())
}

fn validate_observation(observation: &WalletReceiptObservation) -> Result<(), WalletJournalError> {
    match observation {
        WalletReceiptObservation::NotFound | WalletReceiptObservation::Pending => Ok(()),
        WalletReceiptObservation::Mined {
            block_hash,
            confirmations,
            ..
        } if is_lowercase_hex_32_bytes(block_hash) && *confirmations > 0 => Ok(()),
        WalletReceiptObservation::Mined { .. } => Err(WalletJournalError::InvalidObservation),
    }
}

fn append_event(
    path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
    existing: &[u8],
    event: &JournalEvent,
    create_new: bool,
    head_generation: Option<u64>,
) -> Result<(), WalletJournalError> {
    append_event_with_checkpoint(
        path,
        authenticator,
        existing,
        event,
        create_new,
        head_generation,
        |_| Ok(()),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JournalTransactionCheckpoint {
    HeadTransitionPublished,
    JournalPublished,
    HeadCommitted,
}

#[allow(clippy::too_many_arguments)]
fn append_event_with_checkpoint<F>(
    path: &Path,
    authenticator: &WalletJournalAuthenticator<'_>,
    existing: &[u8],
    event: &JournalEvent,
    create_new: bool,
    head_generation: Option<u64>,
    mut checkpoint: F,
) -> Result<(), WalletJournalError>
where
    F: FnMut(JournalTransactionCheckpoint) -> Result<(), WalletJournalError>,
{
    let mut encoded =
        serde_json::to_vec(event).map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)?;
    encoded.push(b'\n');
    if encoded.len() > MAX_EVENT_BYTES {
        return Err(WalletJournalError::JournalTooLarge);
    }
    if create_new != existing.is_empty() {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    let new_size = existing
        .len()
        .checked_add(encoded.len())
        .ok_or(WalletJournalError::JournalTooLarge)?;
    if new_size > MAX_JOURNAL_BYTES {
        return Err(WalletJournalError::JournalTooLarge);
    }
    let mut replacement = Vec::with_capacity(new_size);
    replacement.extend_from_slice(existing);
    replacement.extend_from_slice(&encoded);

    let previous = JournalHeadPosition {
        sequence: event
            .sequence
            .checked_sub(1)
            .ok_or(WalletJournalError::InvalidOrUnsupportedFormat)?,
        authentication_tag_hex: event.previous_authentication_tag_hex.clone(),
    };
    let next = JournalHeadPosition {
        sequence: event.sequence,
        authentication_tag_hex: event.authentication_tag_hex.clone(),
    };
    let generation = head_generation
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(WalletJournalError::JournalTooLarge)?;
    let mut transition = JournalHead {
        schema: JOURNAL_HEAD_SCHEMA.to_string(),
        version: JOURNAL_HEAD_VERSION,
        wallet_id: authenticator.wallet_id.to_string(),
        generation,
        state: JournalHeadState::Transition {
            previous,
            next: next.clone(),
        },
        authentication_tag_hex: String::new(),
    };
    authenticate_journal_head(authenticator, &mut transition)?;
    let head_path = journal_head_path(path)?;
    persist_journal_head(&head_path, &transition, head_generation.is_none())?;
    checkpoint(JournalTransactionCheckpoint::HeadTransitionPublished)?;

    persist_journal_bytes(path, &replacement, create_new)?;
    checkpoint(JournalTransactionCheckpoint::JournalPublished)?;

    let mut committed = JournalHead {
        schema: JOURNAL_HEAD_SCHEMA.to_string(),
        version: JOURNAL_HEAD_VERSION,
        wallet_id: authenticator.wallet_id.to_string(),
        generation,
        state: JournalHeadState::Committed { position: next },
        authentication_tag_hex: String::new(),
    };
    authenticate_journal_head(authenticator, &mut committed)?;
    persist_journal_head(&head_path, &committed, false)?;
    checkpoint(JournalTransactionCheckpoint::HeadCommitted)?;
    Ok(())
}

fn persist_journal_head(
    path: &Path,
    head: &JournalHead,
    create_new: bool,
) -> Result<(), WalletJournalError> {
    let bytes =
        serde_json::to_vec(head).map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)?;
    if bytes.is_empty() || bytes.len() > MAX_JOURNAL_HEAD_BYTES {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    persist_journal_bytes(path, &bytes, create_new)
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JournalPersistenceCheckpoint {
    StagingFileSecured,
    StagingFileFlushed,
}

#[cfg(windows)]
fn persist_journal_bytes(
    path: &Path,
    bytes: &[u8],
    create_new: bool,
) -> Result<(), WalletJournalError> {
    persist_journal_bytes_with_checkpoint(path, bytes, create_new, |_| Ok(()))
}

#[cfg(windows)]
fn persist_journal_bytes_with_checkpoint<F>(
    path: &Path,
    bytes: &[u8],
    create_new: bool,
    mut checkpoint: F,
) -> Result<(), WalletJournalError>
where
    F: FnMut(JournalPersistenceCheckpoint) -> Result<(), WalletJournalError>,
{
    let parent = path
        .parent()
        .ok_or(WalletJournalError::StorageUnavailable)?;
    let _directories =
        DirectoryChainGuard::ensure(parent).map_err(|_| WalletJournalError::StorageUnavailable)?;
    storage_security::protect_directory(parent)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;

    let (_staging_path, mut staging_file) = create_journal_staging_file(parent)?;
    storage_security::protect_open_file(&staging_file)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    checkpoint(JournalPersistenceCheckpoint::StagingFileSecured)?;
    staging_file
        .write_all(bytes)
        .and_then(|_| staging_file.sync_all())
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    storage_security::verify_open_file(&staging_file)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    checkpoint(JournalPersistenceCheckpoint::StagingFileFlushed)?;

    if create_new {
        publish_open_file(&staging_file, path)
            .map_err(|_| WalletJournalError::StorageUnavailable)?;
    } else {
        let existing =
            open_existing_file(path).map_err(|_| WalletJournalError::StorageUnavailable)?;
        storage_security::verify_open_file(&existing)
            .map_err(|_| WalletJournalError::StorageUnavailable)?;
        drop(existing);
        replace_with_open_file(&staging_file, path)
            .map_err(|_| WalletJournalError::StorageUnavailable)?;
    }
    Ok(())
}

#[cfg(windows)]
fn create_journal_staging_file(parent: &Path) -> Result<(PathBuf, fs::File), WalletJournalError> {
    let mut suffix = [0_u8; 16];
    getrandom::fill(&mut suffix).map_err(|_| WalletJournalError::StorageUnavailable)?;
    let path = parent.join(format!(
        "{JOURNAL_STAGING_PREFIX}{}.tmp",
        hex::encode(suffix)
    ));
    let file =
        create_new_publishable_file(&path).map_err(|_| WalletJournalError::StorageUnavailable)?;
    Ok((path, file))
}

#[cfg(not(windows))]
fn persist_journal_bytes(
    path: &Path,
    bytes: &[u8],
    create_new: bool,
) -> Result<(), WalletJournalError> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .ok_or(WalletJournalError::StorageUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| WalletJournalError::StorageUnavailable)?;
    storage_security::protect_directory(parent)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    let mut suffix = [0_u8; 16];
    getrandom::fill(&mut suffix).map_err(|_| WalletJournalError::StorageUnavailable)?;
    let staging_path = parent.join(format!(
        "{JOURNAL_STAGING_PREFIX}{}.tmp",
        hex::encode(suffix)
    ));
    let mut staging = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&staging_path)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    staging
        .write_all(bytes)
        .and_then(|_| staging.sync_all())
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    drop(staging);
    let published = if create_new {
        fs::hard_link(&staging_path, path)
    } else {
        fs::rename(&staging_path, path)
    };
    if published.is_ok() && create_new {
        let _ = fs::remove_file(&staging_path);
    }
    published.map_err(|_| WalletJournalError::StorageUnavailable)
}

fn next_sequence(event_count: usize) -> Result<u64, WalletJournalError> {
    if event_count >= MAX_EVENTS {
        return Err(WalletJournalError::JournalTooLarge);
    }
    u64::try_from(event_count)
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or(WalletJournalError::JournalTooLarge)
}

fn validate_wallet_id(wallet_id: &str) -> Result<(), WalletJournalError> {
    if wallet_id.is_empty()
        || wallet_id.len() > 64
        || !wallet_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(WalletJournalError::InvalidWalletId);
    }
    Ok(())
}

fn validate_tx_id(tx_id: &str) -> Result<(), WalletJournalError> {
    if is_lowercase_hex_32_bytes(tx_id) {
        Ok(())
    } else {
        Err(WalletJournalError::InvalidTransaction)
    }
}

fn is_lowercase_hex_32_bytes(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn is_lowercase_hex_64_bytes(value: &str) -> bool {
    value.len() == 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        secrets::WalletSeed,
        transaction::{sign_cash_transfer_for_test, CashTransferDraft},
    };

    const WALLET_ID: &str = "primary";
    static TEST_SEED: Lazy<WalletSeed> = Lazy::new(|| WalletSeed::for_test(7));

    fn authenticator() -> WalletJournalAuthenticator<'static> {
        WalletJournalAuthenticator::new(WALLET_ID, &TEST_SEED).unwrap()
    }

    fn other_wallet_authenticator() -> WalletJournalAuthenticator<'static> {
        WalletJournalAuthenticator::new("another", &TEST_SEED).unwrap()
    }

    fn wrong_seed_authenticator() -> WalletJournalAuthenticator<'static> {
        static WRONG_SEED: Lazy<WalletSeed> = Lazy::new(|| WalletSeed::for_test(8));
        WalletJournalAuthenticator::new(WALLET_ID, &WRONG_SEED).unwrap()
    }

    fn accepted_transaction() -> (VisionTransaction, WalletSubmissionOutcome) {
        let seed = WalletSeed::for_test(7);
        let draft = CashTransferDraft::for_current_nonce(3, "22".repeat(32), 42);
        let transaction = sign_cash_transfer_for_test(&seed, &draft).unwrap();
        let tx_id = canonical_transaction_id(&transaction).unwrap();
        let outcome = WalletSubmissionOutcome::Accepted {
            tx_id,
            current_nonce: 3,
        };
        (transaction, outcome)
    }

    fn journal_path(directory: &tempfile::TempDir) -> std::path::PathBuf {
        directory
            .path()
            .join("wallets")
            .join("primary-activity.jsonl")
    }

    fn pending_event(
        loaded: &LoadedJournal,
        authenticator: &WalletJournalAuthenticator<'_>,
        tx_id: &str,
    ) -> JournalEvent {
        let mut event = JournalEvent {
            schema: JOURNAL_SCHEMA.to_string(),
            version: JOURNAL_VERSION,
            wallet_id: WALLET_ID.to_string(),
            sequence: next_sequence(loaded.journal.event_count).unwrap(),
            tx_id: tx_id.to_string(),
            recorded_at_unix_ms: 110,
            event: JournalEventData::Observation(ObservationEvent {
                observation: WalletReceiptObservation::Pending,
            }),
            previous_authentication_tag_hex: hex::encode(loaded.journal.last_authentication_tag),
            authentication_tag_hex: String::new(),
        };
        authenticate_event(authenticator, &mut event).unwrap();
        event
    }

    #[test]
    fn accepted_submission_round_trips_public_metadata_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();

        let journal =
            append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100)
                .unwrap();
        assert_eq!(journal.wallet_id(), WALLET_ID);
        assert_eq!(journal.records().len(), 1);
        let record = &journal.records()[0];
        assert_eq!(record.sender_address, transaction.sender_pubkey);
        assert_eq!(record.recipient_address, "22".repeat(32));
        assert_eq!(record.amount_raw_units, "42");
        assert_eq!(record.nonce, 3);
        assert_eq!(record.observation, WalletReceiptObservation::NotFound);

        let loaded = load_activity_journal(&path, &authenticator()).unwrap();
        assert_eq!(loaded, journal);
        let stored = fs::read_to_string(&path).unwrap();
        let stored_head = fs::read_to_string(journal_head_path(&path).unwrap()).unwrap();
        assert!(!stored.contains(&transaction.sig));
        for forbidden in [
            "private_key",
            "secret_key",
            "mnemonic",
            "recovery_phrase",
            "password",
            "seed",
            "signature",
        ] {
            assert!(!stored.contains(forbidden));
            assert!(!stored_head.contains(forbidden));
        }
    }

    #[test]
    fn rejected_or_mismatched_submissions_are_never_recorded() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, _) = accepted_transaction();
        let rejected = WalletSubmissionOutcome::Rejected {
            tx_id: canonical_transaction_id(&transaction).unwrap(),
            current_nonce: 3,
            code: crate::wallet::submission::WalletSubmissionRejection::StaleNonce,
        };
        assert_eq!(
            append_accepted_submission(&path, &authenticator(), &transaction, &rejected, 100)
                .unwrap_err(),
            WalletJournalError::SubmissionNotAccepted
        );
        assert!(!path.exists());

        let mismatch = WalletSubmissionOutcome::Accepted {
            tx_id: "aa".repeat(32),
            current_nonce: 3,
        };
        assert_eq!(
            append_accepted_submission(&path, &authenticator(), &transaction, &mismatch, 100)
                .unwrap_err(),
            WalletJournalError::SubmissionMismatch
        );
        assert!(!path.exists());
    }

    #[test]
    fn duplicate_transactions_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();
        assert_eq!(
            append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 101)
                .unwrap_err(),
            WalletJournalError::DuplicateTransaction
        );
    }

    #[test]
    fn receipt_observations_update_only_known_local_transactions() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        let tx_id = canonical_transaction_id(&transaction).unwrap();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();

        let pending = WalletReceiptObservation::Pending;
        let journal =
            append_receipt_observation(&path, &authenticator(), &tx_id, &pending, 110).unwrap();
        assert_eq!(journal.records()[0].observation, pending);
        assert_eq!(journal.records()[0].last_observed_at_unix_ms, Some(110));
        let size_after_pending = fs::metadata(&path).unwrap().len();
        append_receipt_observation(&path, &authenticator(), &tx_id, &pending, 111).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), size_after_pending);

        let mined = WalletReceiptObservation::Mined {
            block_hash: "bb".repeat(32),
            block_height: 20,
            tx_index: 1,
            confirmations: 2,
        };
        let journal =
            append_receipt_observation(&path, &authenticator(), &tx_id, &mined, 120).unwrap();
        assert_eq!(journal.records()[0].observation, mined);

        let unknown = "cc".repeat(32);
        assert_eq!(
            append_receipt_observation(&path, &authenticator(), &unknown, &pending, 130)
                .unwrap_err(),
            WalletJournalError::UnknownTransaction
        );
    }

    #[test]
    fn reorganization_and_lost_observation_remain_uncertain_activity_states() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        let tx_id = canonical_transaction_id(&transaction).unwrap();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();
        let mined = WalletReceiptObservation::Mined {
            block_hash: "bb".repeat(32),
            block_height: 20,
            tx_index: 1,
            confirmations: 9,
        };
        append_receipt_observation(&path, &authenticator(), &tx_id, &mined, 110).unwrap();
        append_receipt_observation(
            &path,
            &authenticator(),
            &tx_id,
            &WalletReceiptObservation::Pending,
            120,
        )
        .unwrap();
        let journal = append_receipt_observation(
            &path,
            &authenticator(),
            &tx_id,
            &WalletReceiptObservation::NotFound,
            130,
        )
        .unwrap();
        assert_eq!(
            journal.records()[0].observation,
            WalletReceiptObservation::NotFound
        );
    }

    #[test]
    fn invalid_observations_fail_before_storage() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let invalid = WalletReceiptObservation::Mined {
            block_hash: "not-a-block".to_string(),
            block_height: 1,
            tx_index: 0,
            confirmations: 0,
        };
        assert_eq!(
            append_receipt_observation(&path, &authenticator(), &"aa".repeat(32), &invalid, 1)
                .unwrap_err(),
            WalletJournalError::InvalidObservation
        );
    }

    #[test]
    fn truncated_or_unknown_event_data_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();

        let mut truncated = fs::read(&path).unwrap();
        truncated.pop();
        fs::write(&path, truncated).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        fs::remove_file(&path).unwrap();
        fs::remove_file(journal_head_path(&path).unwrap()).unwrap();
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();
        let stored = fs::read_to_string(&path).unwrap();
        let changed = stored.replacen("\"version\":2", "\"version\":2,\"secret\":\"x\"", 1);
        fs::write(&path, changed).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );
    }

    #[test]
    fn wallet_identity_and_sequence_are_bound_to_the_journal() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();
        assert_eq!(
            load_activity_journal(&path, &other_wallet_authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        let stored = fs::read_to_string(&path).unwrap();
        let changed = stored.replacen("\"sequence\":1", "\"sequence\":2", 1);
        fs::write(&path, changed).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );
    }

    #[test]
    fn authentication_rejects_content_tampering_wrong_seed_and_reordered_chains() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        let tx_id = canonical_transaction_id(&transaction).unwrap();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();
        let one_event = fs::read_to_string(&path).unwrap();
        assert!(one_event.contains("previous_authentication_tag_hex"));
        assert!(one_event.contains("authentication_tag_hex"));
        assert!(!one_event.contains(&"07".repeat(32)));

        let tampered = one_event.replacen(
            "\"amount_raw_units\":\"42\"",
            "\"amount_raw_units\":\"43\"",
            1,
        );
        assert_ne!(tampered, one_event);
        fs::write(&path, tampered).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        fs::write(&path, &one_event).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &wrong_seed_authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        append_receipt_observation(
            &path,
            &authenticator(),
            &tx_id,
            &WalletReceiptObservation::Pending,
            110,
        )
        .unwrap();
        let two_events = fs::read_to_string(&path).unwrap();
        let mut lines = two_events.lines();
        let first = lines.next().unwrap();
        let second = lines.next().unwrap();
        assert!(lines.next().is_none());
        fs::write(&path, format!("{second}\n{first}\n")).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );
    }

    #[test]
    fn independent_head_rejects_valid_prefix_rollback_missing_and_tampered_anchors() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let head_path = journal_head_path(&path).unwrap();
        let (transaction, outcome) = accepted_transaction();
        let tx_id = canonical_transaction_id(&transaction).unwrap();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();
        let authentic_prefix = fs::read(&path).unwrap();

        append_receipt_observation(
            &path,
            &authenticator(),
            &tx_id,
            &WalletReceiptObservation::Pending,
            110,
        )
        .unwrap();
        let current_journal = fs::read(&path).unwrap();
        let current_head = fs::read(&head_path).unwrap();

        fs::write(&path, &authentic_prefix).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        fs::write(&path, &current_journal).unwrap();
        storage_security::protect_file(&path).unwrap();
        fs::remove_file(&head_path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        fs::write(&head_path, &current_head).unwrap();
        storage_security::protect_file(&head_path).unwrap();
        let mut tampered: serde_json::Value = serde_json::from_slice(&current_head).unwrap();
        tampered["generation"] = serde_json::json!(999);
        fs::write(&head_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        storage_security::protect_file(&head_path).unwrap();
        assert_eq!(
            load_activity_journal(&path, &authenticator()).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );
    }

    #[cfg(windows)]
    #[test]
    fn head_transaction_recovers_each_interruption_boundary_without_accepting_mismatch() {
        for interruption in [
            JournalTransactionCheckpoint::HeadTransitionPublished,
            JournalTransactionCheckpoint::JournalPublished,
            JournalTransactionCheckpoint::HeadCommitted,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = journal_path(&directory);
            let (transaction, outcome) = accepted_transaction();
            let tx_id = canonical_transaction_id(&transaction).unwrap();
            let auth = authenticator();
            append_accepted_submission(&path, &auth, &transaction, &outcome, 100).unwrap();
            let original_journal = fs::read(&path).unwrap();
            let loaded = load_journal_unlocked(&path, &auth).unwrap();
            let event = pending_event(&loaded, &auth, &tx_id);

            let error = append_event_with_checkpoint(
                &path,
                &auth,
                loaded.encoded.as_slice(),
                &event,
                false,
                loaded.head_generation,
                |checkpoint| {
                    if checkpoint == interruption {
                        Err(WalletJournalError::StorageUnavailable)
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
            assert_eq!(error, WalletJournalError::StorageUnavailable);

            let recovered = load_activity_journal(&path, &auth).unwrap();
            let expected_observation =
                if interruption == JournalTransactionCheckpoint::HeadTransitionPublished {
                    assert_eq!(fs::read(&path).unwrap(), original_journal);
                    WalletReceiptObservation::NotFound
                } else {
                    WalletReceiptObservation::Pending
                };
            assert_eq!(recovered.records()[0].observation, expected_observation);

            let recovered_head: JournalHead =
                serde_json::from_slice(&fs::read(journal_head_path(&path).unwrap()).unwrap())
                    .unwrap();
            assert!(matches!(
                recovered_head.state,
                JournalHeadState::Committed { .. }
            ));
            verify_journal_head(&auth, &recovered_head).unwrap();
        }
    }

    #[cfg(windows)]
    #[test]
    fn interrupted_first_head_transition_recovers_empty_state_and_allows_safe_retry() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let auth = authenticator();
        let loaded = load_journal_unlocked(&path, &auth).unwrap();
        let (transaction, outcome) = accepted_transaction();
        let (tx_id, submitted) = accepted_submission(&transaction, &outcome).unwrap();
        let mut event = JournalEvent {
            schema: JOURNAL_SCHEMA.to_string(),
            version: JOURNAL_VERSION,
            wallet_id: WALLET_ID.to_string(),
            sequence: 1,
            tx_id,
            recorded_at_unix_ms: 100,
            event: JournalEventData::Submitted(submitted),
            previous_authentication_tag_hex: hex::encode([0_u8; AUTHENTICATION_TAG_BYTES]),
            authentication_tag_hex: String::new(),
        };
        authenticate_event(&auth, &mut event).unwrap();

        let error = append_event_with_checkpoint(
            &path,
            &auth,
            loaded.encoded.as_slice(),
            &event,
            true,
            loaded.head_generation,
            |checkpoint| {
                if checkpoint == JournalTransactionCheckpoint::HeadTransitionPublished {
                    Err(WalletJournalError::StorageUnavailable)
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert_eq!(error, WalletJournalError::StorageUnavailable);
        assert!(!path.exists());
        assert!(journal_head_path(&path).unwrap().exists());

        let recovered = load_activity_journal(&path, &auth).unwrap();
        assert!(recovered.records().is_empty());
        append_accepted_submission(&path, &auth, &transaction, &outcome, 100).unwrap();
        let retried = load_activity_journal(&path, &auth).unwrap();
        assert_eq!(retried.records().len(), 1);
    }

    #[test]
    fn accepted_submission_must_belong_to_the_authenticating_seed() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let foreign_seed = WalletSeed::for_test(8);
        let draft = CashTransferDraft::for_current_nonce(3, "22".repeat(32), 42);
        let transaction = sign_cash_transfer_for_test(&foreign_seed, &draft).unwrap();
        let outcome = WalletSubmissionOutcome::Accepted {
            tx_id: canonical_transaction_id(&transaction).unwrap(),
            current_nonce: 3,
        };

        assert_eq!(
            append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100)
                .unwrap_err(),
            WalletJournalError::SubmissionMismatch
        );
        assert!(!path.exists());
    }

    #[cfg(windows)]
    #[test]
    fn interrupted_copy_on_write_never_changes_the_live_journal() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, &authenticator(), &transaction, &outcome, 100).unwrap();
        let original = fs::read(&path).unwrap();
        let mut unpublished = original.clone();
        unpublished.extend_from_slice(b"incomplete-untrusted-event\n");

        for interruption in [
            JournalPersistenceCheckpoint::StagingFileSecured,
            JournalPersistenceCheckpoint::StagingFileFlushed,
        ] {
            let error =
                persist_journal_bytes_with_checkpoint(&path, &unpublished, false, |checkpoint| {
                    if checkpoint == interruption {
                        Err(WalletJournalError::StorageUnavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap_err();
            assert_eq!(error, WalletJournalError::StorageUnavailable);
            assert_eq!(fs::read(&path).unwrap(), original);
            load_activity_journal(&path, &authenticator()).unwrap();
        }
    }

    #[cfg(windows)]
    #[test]
    fn interrupted_first_write_never_publishes_a_partial_journal() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        storage_security::protect_directory(parent).unwrap();

        let error = persist_journal_bytes_with_checkpoint(
            &path,
            b"not-a-complete-journal\n",
            true,
            |checkpoint| {
                if checkpoint == JournalPersistenceCheckpoint::StagingFileFlushed {
                    Err(WalletJournalError::StorageUnavailable)
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert_eq!(error, WalletJournalError::StorageUnavailable);
        assert!(!path.exists());
    }

    #[cfg(windows)]
    #[test]
    fn journal_reader_rejects_reparse_point_files() {
        use std::os::windows::fs::symlink_file;

        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("wallets");
        fs::create_dir(&parent).unwrap();
        storage_security::protect_directory(&parent).unwrap();
        let target = parent.join("target.jsonl");
        fs::write(&target, b"attacker-controlled\n").unwrap();
        let link = parent.join("primary-activity.jsonl");

        // Creating symlinks can be disabled by Windows policy. When it is available, the reader
        // must reject the reparse point rather than following it to the target.
        if symlink_file(&target, &link).is_ok() {
            assert_eq!(
                load_activity_journal(&link, &authenticator()).unwrap_err(),
                WalletJournalError::StorageUnavailable
            );
        }

        let protected_directory = tempfile::tempdir().unwrap();
        let protected_path = journal_path(&protected_directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(
            &protected_path,
            &authenticator(),
            &transaction,
            &outcome,
            100,
        )
        .unwrap();
        let protected_head = journal_head_path(&protected_path).unwrap();
        let head_target = protected_head.with_file_name("attacker-head-target.json");
        fs::rename(&protected_head, &head_target).unwrap();
        if symlink_file(&head_target, &protected_head).is_ok() {
            assert_eq!(
                load_activity_journal(&protected_path, &authenticator()).unwrap_err(),
                WalletJournalError::StorageUnavailable
            );
        }
    }
}
