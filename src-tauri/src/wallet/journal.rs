#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "local wallet activity remains internal until wallet submission is approved"
    )
)]

use super::{
    receipt::WalletReceiptObservation,
    storage_security,
    submission::WalletSubmissionOutcome,
    transaction::{canonical_transaction_id, VisionTransaction},
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};

const JOURNAL_SCHEMA: &str = "vision-desktop-wallet-activity";
const JOURNAL_VERSION: u32 = 1;
const MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 16 * 1024;
const MAX_EVENTS: usize = 10_000;

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
}

impl WalletActivityJournal {
    pub(in crate::wallet) fn wallet_id(&self) -> &str {
        &self.wallet_id
    }

    pub(in crate::wallet) fn records(&self) -> &[WalletActivityRecord] {
        &self.records
    }
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
pub(in crate::wallet) fn append_accepted_submission(
    path: &Path,
    wallet_id: &str,
    transaction: &VisionTransaction,
    outcome: &WalletSubmissionOutcome,
    submitted_at_unix_ms: u64,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    validate_wallet_id(wallet_id)?;
    let mut journal = load_journal_unlocked(path, wallet_id)?;
    let (tx_id, submitted) = accepted_submission(transaction, outcome)?;
    if journal.records.iter().any(|record| record.tx_id == tx_id) {
        return Err(WalletJournalError::DuplicateTransaction);
    }
    let event = JournalEvent {
        schema: JOURNAL_SCHEMA.to_string(),
        version: JOURNAL_VERSION,
        wallet_id: wallet_id.to_string(),
        sequence: next_sequence(journal.event_count)?,
        tx_id,
        recorded_at_unix_ms: submitted_at_unix_ms,
        event: JournalEventData::Submitted(submitted),
    };
    append_event(path, &event, journal.event_count == 0)?;
    apply_event(&mut journal, &event)?;
    Ok(journal)
}

/// Records the newest validated Core observation for a locally submitted
/// transaction. Repeated identical observations do not grow the journal.
pub(in crate::wallet) fn append_receipt_observation(
    path: &Path,
    wallet_id: &str,
    tx_id: &str,
    observation: &WalletReceiptObservation,
    observed_at_unix_ms: u64,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    validate_wallet_id(wallet_id)?;
    validate_tx_id(tx_id)?;
    validate_observation(observation)?;
    let mut journal = load_journal_unlocked(path, wallet_id)?;
    let record = journal
        .records
        .iter()
        .find(|record| record.tx_id == tx_id)
        .ok_or(WalletJournalError::UnknownTransaction)?;
    if record.observation == *observation {
        return Ok(journal);
    }
    let event = JournalEvent {
        schema: JOURNAL_SCHEMA.to_string(),
        version: JOURNAL_VERSION,
        wallet_id: wallet_id.to_string(),
        sequence: next_sequence(journal.event_count)?,
        tx_id: tx_id.to_string(),
        recorded_at_unix_ms: observed_at_unix_ms,
        event: JournalEventData::Observation(ObservationEvent {
            observation: observation.clone(),
        }),
    };
    append_event(path, &event, false)?;
    apply_event(&mut journal, &event)?;
    Ok(journal)
}

pub(in crate::wallet) fn load_activity_journal(
    path: &Path,
    wallet_id: &str,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let _guard = JOURNAL_WRITE_LOCK
        .lock()
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    validate_wallet_id(wallet_id)?;
    load_journal_unlocked(path, wallet_id)
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
    expected_wallet_id: &str,
) -> Result<WalletActivityJournal, WalletJournalError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WalletActivityJournal {
                wallet_id: expected_wallet_id.to_string(),
                records: Vec::new(),
                event_count: 0,
            });
        }
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
    if file_size == 0 || file_size > MAX_JOURNAL_BYTES {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    let mut bytes = Vec::with_capacity(file_size);
    fs::File::open(path)
        .map_err(|_| WalletJournalError::StorageUnavailable)?
        .take(MAX_JOURNAL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(WalletJournalError::JournalTooLarge);
    }
    if bytes.last() != Some(&b'\n') {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }

    let mut journal = WalletActivityJournal {
        wallet_id: expected_wallet_id.to_string(),
        records: Vec::new(),
        event_count: 0,
    };
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
            || event.wallet_id != expected_wallet_id
            || event.sequence != next_sequence(journal.event_count)?
        {
            return Err(WalletJournalError::InvalidOrUnsupportedFormat);
        }
        apply_event(&mut journal, &event)?;
    }
    if journal.event_count == 0 {
        return Err(WalletJournalError::InvalidOrUnsupportedFormat);
    }
    Ok(journal)
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
    event: &JournalEvent,
    create_new: bool,
) -> Result<(), WalletJournalError> {
    let mut encoded =
        serde_json::to_vec(event).map_err(|_| WalletJournalError::InvalidOrUnsupportedFormat)?;
    encoded.push(b'\n');
    if encoded.len() > MAX_EVENT_BYTES {
        return Err(WalletJournalError::JournalTooLarge);
    }
    let parent = path
        .parent()
        .ok_or(WalletJournalError::StorageUnavailable)?;
    if create_new {
        fs::create_dir_all(parent).map_err(|_| WalletJournalError::StorageUnavailable)?;
        storage_security::protect_directory(parent)
            .map_err(|_| WalletJournalError::StorageUnavailable)?;
        write_new_journal(path, &encoded)?;
        storage_security::protect_file(path)
            .and_then(|_| storage_security::verify_file(path))
            .map_err(|_| WalletJournalError::StorageUnavailable)?;
        return Ok(());
    }

    storage_security::verify_directory(parent)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    storage_security::verify_file(path).map_err(|_| WalletJournalError::StorageUnavailable)?;
    let metadata =
        fs::symlink_metadata(path).map_err(|_| WalletJournalError::StorageUnavailable)?;
    let existing =
        usize::try_from(metadata.len()).map_err(|_| WalletJournalError::JournalTooLarge)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || existing
            .checked_add(encoded.len())
            .is_none_or(|size| size > MAX_JOURNAL_BYTES)
    {
        return Err(WalletJournalError::JournalTooLarge);
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| WalletJournalError::StorageUnavailable)
}

fn write_new_journal(path: &Path, bytes: &[u8]) -> Result<(), WalletJournalError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| WalletJournalError::StorageUnavailable)?;
    let result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| WalletJournalError::StorageUnavailable);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
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

    #[test]
    fn accepted_submission_round_trips_public_metadata_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();

        let journal =
            append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 100).unwrap();
        assert_eq!(journal.wallet_id(), WALLET_ID);
        assert_eq!(journal.records().len(), 1);
        let record = &journal.records()[0];
        assert_eq!(record.sender_address, transaction.sender_pubkey);
        assert_eq!(record.recipient_address, "22".repeat(32));
        assert_eq!(record.amount_raw_units, "42");
        assert_eq!(record.nonce, 3);
        assert_eq!(record.observation, WalletReceiptObservation::NotFound);

        let loaded = load_activity_journal(&path, WALLET_ID).unwrap();
        assert_eq!(loaded, journal);
        let stored = fs::read_to_string(path).unwrap();
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
            append_accepted_submission(&path, WALLET_ID, &transaction, &rejected, 100).unwrap_err(),
            WalletJournalError::SubmissionNotAccepted
        );
        assert!(!path.exists());

        let mismatch = WalletSubmissionOutcome::Accepted {
            tx_id: "aa".repeat(32),
            current_nonce: 3,
        };
        assert_eq!(
            append_accepted_submission(&path, WALLET_ID, &transaction, &mismatch, 100).unwrap_err(),
            WalletJournalError::SubmissionMismatch
        );
        assert!(!path.exists());
    }

    #[test]
    fn duplicate_transactions_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 100).unwrap();
        assert_eq!(
            append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 101).unwrap_err(),
            WalletJournalError::DuplicateTransaction
        );
    }

    #[test]
    fn receipt_observations_update_only_known_local_transactions() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        let tx_id = canonical_transaction_id(&transaction).unwrap();
        append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 100).unwrap();

        let pending = WalletReceiptObservation::Pending;
        let journal = append_receipt_observation(&path, WALLET_ID, &tx_id, &pending, 110).unwrap();
        assert_eq!(journal.records()[0].observation, pending);
        assert_eq!(journal.records()[0].last_observed_at_unix_ms, Some(110));
        let size_after_pending = fs::metadata(&path).unwrap().len();
        append_receipt_observation(&path, WALLET_ID, &tx_id, &pending, 111).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), size_after_pending);

        let mined = WalletReceiptObservation::Mined {
            block_hash: "bb".repeat(32),
            block_height: 20,
            tx_index: 1,
            confirmations: 2,
        };
        let journal = append_receipt_observation(&path, WALLET_ID, &tx_id, &mined, 120).unwrap();
        assert_eq!(journal.records()[0].observation, mined);

        let unknown = "cc".repeat(32);
        assert_eq!(
            append_receipt_observation(&path, WALLET_ID, &unknown, &pending, 130).unwrap_err(),
            WalletJournalError::UnknownTransaction
        );
    }

    #[test]
    fn reorganization_and_lost_observation_remain_uncertain_activity_states() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        let tx_id = canonical_transaction_id(&transaction).unwrap();
        append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 100).unwrap();
        let mined = WalletReceiptObservation::Mined {
            block_hash: "bb".repeat(32),
            block_height: 20,
            tx_index: 1,
            confirmations: 9,
        };
        append_receipt_observation(&path, WALLET_ID, &tx_id, &mined, 110).unwrap();
        append_receipt_observation(
            &path,
            WALLET_ID,
            &tx_id,
            &WalletReceiptObservation::Pending,
            120,
        )
        .unwrap();
        let journal = append_receipt_observation(
            &path,
            WALLET_ID,
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
            append_receipt_observation(&path, WALLET_ID, &"aa".repeat(32), &invalid, 1)
                .unwrap_err(),
            WalletJournalError::InvalidObservation
        );
    }

    #[test]
    fn truncated_or_unknown_event_data_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 100).unwrap();

        let mut truncated = fs::read(&path).unwrap();
        truncated.pop();
        fs::write(&path, truncated).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, WALLET_ID).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        fs::remove_file(&path).unwrap();
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 100).unwrap();
        let stored = fs::read_to_string(&path).unwrap();
        let changed = stored.replacen("\"version\":1", "\"version\":1,\"secret\":\"x\"", 1);
        fs::write(&path, changed).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, WALLET_ID).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );
    }

    #[test]
    fn wallet_identity_and_sequence_are_bound_to_the_journal() {
        let directory = tempfile::tempdir().unwrap();
        let path = journal_path(&directory);
        let (transaction, outcome) = accepted_transaction();
        append_accepted_submission(&path, WALLET_ID, &transaction, &outcome, 100).unwrap();
        assert_eq!(
            load_activity_journal(&path, "another").unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );

        let stored = fs::read_to_string(&path).unwrap();
        let changed = stored.replacen("\"sequence\":1", "\"sequence\":2", 1);
        fs::write(&path, changed).unwrap();
        storage_security::protect_file(&path).unwrap();
        assert_eq!(
            load_activity_journal(&path, WALLET_ID).unwrap_err(),
            WalletJournalError::InvalidOrUnsupportedFormat
        );
    }
}
