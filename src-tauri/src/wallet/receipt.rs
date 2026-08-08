#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "receipt observation remains internal until wallet submission is approved"
    )
)]

use super::{
    reconciliation::ReconciliationLookupExpectation,
    transaction::{canonical_transaction_id, VisionTransaction},
};
use serde::{Deserialize, Serialize};
use std::fmt;

const HIGH_CONFIDENCE_CONFIRMATIONS: u64 = 50;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::wallet) enum WalletReceiptObservation {
    NotFound,
    Pending,
    Mined {
        block_hash: String,
        block_height: u64,
        tx_index: usize,
        confirmations: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletReceiptChange {
    FirstObservation,
    Unchanged,
    PendingToMined,
    ConfirmationsAdvanced,
    Reorganized,
    ObservationLost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletReceiptConfidence {
    Observed,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletReceiptPresentation {
    NotObserved,
    Pending,
    Mined {
        confirmations: u64,
        confidence: WalletReceiptConfidence,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletReceiptError {
    InvalidResponse,
    TransactionIdMismatch,
    InvalidBlockReference,
    SignedEnvelopeMismatch,
}

impl fmt::Display for WalletReceiptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidResponse => "Core returned an invalid transaction observation",
            Self::TransactionIdMismatch => "Core returned a different transaction",
            Self::InvalidBlockReference => "Core returned an invalid canonical block reference",
            Self::SignedEnvelopeMismatch => "Core returned a different signed transaction",
        })
    }
}

/// Parses a read-only reconciliation lookup and proves the exact signed JSON envelope, including
/// its signature, matches the body that may have been submitted. NotFound remains non-authoritative.
pub(in crate::wallet) fn parse_exact_signed_receipt_observation(
    body: &[u8],
    expected_transaction: &VisionTransaction,
    expected_signed_body_digest_hex: &str,
    canonical_tip_height: u64,
) -> Result<WalletReceiptObservation, WalletReceiptError> {
    let expected_tx_id = canonical_transaction_id(expected_transaction)
        .map_err(|_| WalletReceiptError::InvalidResponse)?;
    let wire: TransactionLookupWire =
        serde_json::from_slice(body).map_err(|_| WalletReceiptError::InvalidResponse)?;
    if wire.tx_id != expected_tx_id {
        return Err(WalletReceiptError::TransactionIdMismatch);
    }
    if !wire.found {
        if wire.block_hash.is_none()
            && wire.block_height.is_none()
            && wire.tx_index.is_none()
            && wire.tx.is_none()
        {
            return Ok(WalletReceiptObservation::NotFound);
        }
        return Err(WalletReceiptError::InvalidResponse);
    }
    let observed = wire
        .tx
        .as_ref()
        .ok_or(WalletReceiptError::SignedEnvelopeMismatch)?;
    if observed != expected_transaction {
        return Err(WalletReceiptError::SignedEnvelopeMismatch);
    }
    let exact_body =
        serde_json::to_vec(observed).map_err(|_| WalletReceiptError::InvalidResponse)?;
    let mut hasher =
        blake3::Hasher::new_derive_key("com.vision.desktop.wallet-signed-envelope-digest.v1");
    hasher.update(&exact_body);
    let observed_digest = hasher.finalize().to_hex().to_string();
    if observed_digest != expected_signed_body_digest_hex {
        return Err(WalletReceiptError::SignedEnvelopeMismatch);
    }
    parse_receipt_observation(body, &expected_tx_id, canonical_tip_height)
}

impl std::error::Error for WalletReceiptError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionLookupWire {
    tx_id: String,
    found: bool,
    block_hash: Option<String>,
    block_height: Option<u64>,
    tx_index: Option<usize>,
    tx: Option<VisionTransaction>,
}

/// Parses the exact RC2 `GET /transaction/:txid` snapshot.
///
/// Confirmations are observations of current canonical depth, not a promise of
/// deterministic finality. RC2 permits higher-work reorganization at any depth.
pub(in crate::wallet) fn parse_receipt_observation(
    body: &[u8],
    expected_tx_id: &str,
    canonical_tip_height: u64,
) -> Result<WalletReceiptObservation, WalletReceiptError> {
    let wire: TransactionLookupWire =
        serde_json::from_slice(body).map_err(|_| WalletReceiptError::InvalidResponse)?;
    if wire.tx_id != expected_tx_id {
        return Err(WalletReceiptError::TransactionIdMismatch);
    }

    match (
        wire.found,
        wire.block_hash,
        wire.block_height,
        wire.tx_index,
        wire.tx,
    ) {
        (false, None, None, None, None) => Ok(WalletReceiptObservation::NotFound),
        (true, None, None, None, Some(transaction)) => {
            verify_observed_transaction(&transaction, expected_tx_id)?;
            Ok(WalletReceiptObservation::Pending)
        }
        (true, Some(block_hash), Some(block_height), Some(tx_index), Some(transaction)) => {
            verify_observed_transaction(&transaction, expected_tx_id)?;
            if !is_lowercase_hex_32_bytes(&block_hash) || block_height > canonical_tip_height {
                return Err(WalletReceiptError::InvalidBlockReference);
            }
            let confirmations = canonical_tip_height
                .checked_sub(block_height)
                .and_then(|depth| depth.checked_add(1))
                .ok_or(WalletReceiptError::InvalidBlockReference)?;
            Ok(WalletReceiptObservation::Mined {
                block_hash,
                block_height,
                tx_index,
                confirmations,
            })
        }
        _ => Err(WalletReceiptError::InvalidResponse),
    }
}

/// Classifies changes without declaring probabilistic confirmations final.
pub(in crate::wallet) fn classify_receipt_change(
    previous: Option<&WalletReceiptObservation>,
    current: &WalletReceiptObservation,
) -> WalletReceiptChange {
    let Some(previous) = previous else {
        return WalletReceiptChange::FirstObservation;
    };

    match (previous, current) {
        (WalletReceiptObservation::NotFound, WalletReceiptObservation::NotFound)
        | (WalletReceiptObservation::Pending, WalletReceiptObservation::Pending) => {
            WalletReceiptChange::Unchanged
        }
        (WalletReceiptObservation::Pending, WalletReceiptObservation::Mined { .. }) => {
            WalletReceiptChange::PendingToMined
        }
        (
            WalletReceiptObservation::Mined {
                block_hash: previous_hash,
                block_height: previous_height,
                confirmations: previous_confirmations,
                ..
            },
            WalletReceiptObservation::Mined {
                block_hash: current_hash,
                block_height: current_height,
                confirmations: current_confirmations,
                ..
            },
        ) if previous_hash == current_hash
            && previous_height == current_height
            && current_confirmations > previous_confirmations =>
        {
            WalletReceiptChange::ConfirmationsAdvanced
        }
        (previous, current) if previous == current => WalletReceiptChange::Unchanged,
        (WalletReceiptObservation::Mined { .. }, WalletReceiptObservation::Pending)
        | (WalletReceiptObservation::Mined { .. }, WalletReceiptObservation::Mined { .. }) => {
            WalletReceiptChange::Reorganized
        }
        (WalletReceiptObservation::Pending, WalletReceiptObservation::NotFound)
        | (WalletReceiptObservation::Mined { .. }, WalletReceiptObservation::NotFound) => {
            WalletReceiptChange::ObservationLost
        }
        (WalletReceiptObservation::NotFound, _) => WalletReceiptChange::FirstObservation,
    }
}

/// Applies the approved Desktop presentation policy without claiming
/// deterministic finality. The 50-confirmation threshold is diagnostic only.
pub(in crate::wallet) fn receipt_presentation(
    observation: &WalletReceiptObservation,
) -> WalletReceiptPresentation {
    match observation {
        WalletReceiptObservation::NotFound => WalletReceiptPresentation::NotObserved,
        WalletReceiptObservation::Pending => WalletReceiptPresentation::Pending,
        WalletReceiptObservation::Mined { confirmations, .. } => WalletReceiptPresentation::Mined {
            confirmations: *confirmations,
            confidence: if *confirmations >= HIGH_CONFIDENCE_CONFIRMATIONS {
                WalletReceiptConfidence::High
            } else {
                WalletReceiptConfidence::Observed
            },
        },
    }
}

fn verify_observed_transaction(
    transaction: &VisionTransaction,
    expected_tx_id: &str,
) -> Result<(), WalletReceiptError> {
    let observed_tx_id =
        canonical_transaction_id(transaction).map_err(|_| WalletReceiptError::InvalidResponse)?;
    if observed_tx_id != expected_tx_id {
        return Err(WalletReceiptError::TransactionIdMismatch);
    }
    Ok(())
}

fn is_lowercase_hex_32_bytes(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_transaction() -> VisionTransaction {
        VisionTransaction {
            nonce: 0,
            sender_pubkey: "11".repeat(32),
            module: "cash".to_string(),
            method: "transfer".to_string(),
            args: serde_json::to_vec(&serde_json::json!({
                "to": "22".repeat(32),
                "amount": 42_u128,
            }))
            .unwrap(),
            tip: 0,
            fee_limit: 201,
            sig: "33".repeat(64),
        }
    }

    fn transaction_id(transaction: &VisionTransaction) -> String {
        canonical_transaction_id(transaction).unwrap()
    }

    fn snapshot_json(
        transaction: Option<&VisionTransaction>,
        found: bool,
        block_hash: Option<String>,
        block_height: Option<u64>,
        tx_index: Option<usize>,
    ) -> Vec<u8> {
        let tx_id = transaction
            .map(transaction_id)
            .unwrap_or_else(|| "44".repeat(32));
        serde_json::to_vec(&serde_json::json!({
            "tx_id": tx_id,
            "found": found,
            "block_hash": block_hash,
            "block_height": block_height,
            "tx_index": tx_index,
            "tx": transaction,
        }))
        .unwrap()
    }

    #[test]
    fn parses_missing_pending_and_canonical_mined_snapshots() {
        let transaction = sample_transaction();
        let tx_id = transaction_id(&transaction);

        let missing = snapshot_json(None, false, None, None, None);
        assert_eq!(
            parse_receipt_observation(&missing, &"44".repeat(32), 20),
            Ok(WalletReceiptObservation::NotFound)
        );

        let pending = snapshot_json(Some(&transaction), true, None, None, None);
        assert_eq!(
            parse_receipt_observation(&pending, &tx_id, 20),
            Ok(WalletReceiptObservation::Pending)
        );

        let mined = snapshot_json(
            Some(&transaction),
            true,
            Some("aa".repeat(32)),
            Some(18),
            Some(1),
        );
        assert_eq!(
            parse_receipt_observation(&mined, &tx_id, 20),
            Ok(WalletReceiptObservation::Mined {
                block_hash: "aa".repeat(32),
                block_height: 18,
                tx_index: 1,
                confirmations: 3,
            })
        );
    }

    #[test]
    fn rejects_inconsistent_or_noncanonical_observations() {
        let transaction = sample_transaction();
        let tx_id = transaction_id(&transaction);

        let partial = snapshot_json(
            Some(&transaction),
            true,
            Some("aa".repeat(32)),
            None,
            Some(1),
        );
        assert_eq!(
            parse_receipt_observation(&partial, &tx_id, 20),
            Err(WalletReceiptError::InvalidResponse)
        );

        let future = snapshot_json(
            Some(&transaction),
            true,
            Some("aa".repeat(32)),
            Some(21),
            Some(1),
        );
        assert_eq!(
            parse_receipt_observation(&future, &tx_id, 20),
            Err(WalletReceiptError::InvalidBlockReference)
        );

        let malformed_hash = snapshot_json(
            Some(&transaction),
            true,
            Some("AA".repeat(32)),
            Some(18),
            Some(1),
        );
        assert_eq!(
            parse_receipt_observation(&malformed_hash, &tx_id, 20),
            Err(WalletReceiptError::InvalidBlockReference)
        );
    }

    #[test]
    fn verifies_returned_transaction_matches_requested_canonical_id() {
        let transaction = sample_transaction();
        let pending = snapshot_json(Some(&transaction), true, None, None, None);

        assert_eq!(
            parse_receipt_observation(&pending, &"ff".repeat(32), 20),
            Err(WalletReceiptError::TransactionIdMismatch)
        );

        let expected_tx_id = transaction_id(&transaction);
        let mut different_transaction = transaction;
        different_transaction.nonce = 1;
        let inconsistent = serde_json::to_vec(&serde_json::json!({
            "tx_id": expected_tx_id,
            "found": true,
            "block_hash": null,
            "block_height": null,
            "tx_index": null,
            "tx": different_transaction,
        }))
        .unwrap();
        assert_eq!(
            parse_receipt_observation(&inconsistent, &expected_tx_id, 20),
            Err(WalletReceiptError::TransactionIdMismatch)
        );
    }

    #[test]
    fn detects_confirmation_progress_and_reorganizations() {
        let pending = WalletReceiptObservation::Pending;
        let mined_one = WalletReceiptObservation::Mined {
            block_hash: "aa".repeat(32),
            block_height: 20,
            tx_index: 1,
            confirmations: 1,
        };
        let mined_three = WalletReceiptObservation::Mined {
            block_hash: "aa".repeat(32),
            block_height: 20,
            tx_index: 1,
            confirmations: 3,
        };
        let moved = WalletReceiptObservation::Mined {
            block_hash: "bb".repeat(32),
            block_height: 22,
            tx_index: 2,
            confirmations: 1,
        };

        assert_eq!(
            classify_receipt_change(Some(&pending), &mined_one),
            WalletReceiptChange::PendingToMined
        );
        assert_eq!(
            classify_receipt_change(Some(&mined_one), &mined_three),
            WalletReceiptChange::ConfirmationsAdvanced
        );
        assert_eq!(
            classify_receipt_change(Some(&mined_three), &pending),
            WalletReceiptChange::Reorganized
        );
        assert_eq!(
            classify_receipt_change(Some(&mined_three), &moved),
            WalletReceiptChange::Reorganized
        );
        assert_eq!(
            classify_receipt_change(Some(&pending), &WalletReceiptObservation::NotFound),
            WalletReceiptChange::ObservationLost
        );
    }

    #[test]
    fn presentation_uses_confirmations_without_declaring_finality() {
        assert_eq!(
            receipt_presentation(&WalletReceiptObservation::NotFound),
            WalletReceiptPresentation::NotObserved
        );
        assert_eq!(
            receipt_presentation(&WalletReceiptObservation::Pending),
            WalletReceiptPresentation::Pending
        );
        let mined = |confirmations| WalletReceiptObservation::Mined {
            block_hash: "aa".repeat(32),
            block_height: 10,
            tx_index: 0,
            confirmations,
        };
        assert_eq!(
            receipt_presentation(&mined(49)),
            WalletReceiptPresentation::Mined {
                confirmations: 49,
                confidence: WalletReceiptConfidence::Observed,
            }
        );
        assert_eq!(
            receipt_presentation(&mined(50)),
            WalletReceiptPresentation::Mined {
                confirmations: 50,
                confidence: WalletReceiptConfidence::High,
            }
        );
    }

    #[test]
    fn reconciliation_requires_the_exact_signature_and_signed_body_digest() {
        let transaction = sample_transaction();
        let pending = snapshot_json(Some(&transaction), true, None, None, None);
        let exact_body = serde_json::to_vec(&transaction).unwrap();
        let mut hasher =
            blake3::Hasher::new_derive_key("com.vision.desktop.wallet-signed-envelope-digest.v1");
        hasher.update(&exact_body);
        let digest = hasher.finalize().to_hex().to_string();
        assert_eq!(
            parse_exact_signed_receipt_observation(&pending, &transaction, &digest, 20),
            Ok(WalletReceiptObservation::Pending)
        );

        let mut alternate_signature = transaction.clone();
        alternate_signature.sig = "44".repeat(64);
        let alternate = snapshot_json(Some(&alternate_signature), true, None, None, None);
        assert_eq!(
            parse_exact_signed_receipt_observation(&alternate, &transaction, &digest, 20),
            Err(WalletReceiptError::SignedEnvelopeMismatch)
        );
        assert_eq!(
            parse_exact_signed_receipt_observation(&pending, &transaction, &"00".repeat(32), 20,),
            Err(WalletReceiptError::SignedEnvelopeMismatch)
        );
    }
}

pub(super) struct ExactAcceptedLookup {
    transaction_id: String,
    nonce: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationCashTransferArgs {
    to: String,
    amount: u128,
}

impl ExactAcceptedLookup {
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }
    pub(super) const fn nonce(&self) -> u64 {
        self.nonce
    }
}

pub(super) fn prove_exact_reconciliation_lookup(
    body: &[u8],
    expected: &ReconciliationLookupExpectation,
) -> Result<Option<ExactAcceptedLookup>, WalletReceiptError> {
    let wire: TransactionLookupWire =
        serde_json::from_slice(body).map_err(|_| WalletReceiptError::InvalidResponse)?;
    if wire.tx_id != expected.transaction_id() {
        return Err(WalletReceiptError::TransactionIdMismatch);
    }
    if !wire.found {
        return if wire.block_hash.is_none()
            && wire.block_height.is_none()
            && wire.tx_index.is_none()
            && wire.tx.is_none()
        {
            Ok(None)
        } else {
            Err(WalletReceiptError::InvalidResponse)
        };
    }
    let transaction = wire.tx.ok_or(WalletReceiptError::SignedEnvelopeMismatch)?;
    let args: ReconciliationCashTransferArgs = serde_json::from_slice(&transaction.args)
        .map_err(|_| WalletReceiptError::SignedEnvelopeMismatch)?;
    let amount = expected
        .amount_raw_units()
        .parse::<u128>()
        .map_err(|_| WalletReceiptError::SignedEnvelopeMismatch)?;
    if canonical_transaction_id(&transaction).ok().as_deref() != Some(expected.transaction_id())
        || transaction.sender_pubkey != expected.sender_address()
        || transaction.module != "cash"
        || transaction.method != "transfer"
        || args.to != expected.recipient_address()
        || args.amount != amount
        || transaction.nonce != expected.nonce()
        || transaction.tip != expected.tip_raw_units()
        || transaction.fee_limit != expected.fee_limit_raw_units()
        || transaction.sig.len() != 128
        || !transaction
            .sig
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(WalletReceiptError::SignedEnvelopeMismatch);
    }
    let exact_body =
        serde_json::to_vec(&transaction).map_err(|_| WalletReceiptError::SignedEnvelopeMismatch)?;
    let mut hasher =
        blake3::Hasher::new_derive_key("com.vision.desktop.wallet-signed-envelope-digest.v1");
    hasher.update(&exact_body);
    if hasher.finalize().to_hex().as_str() != expected.signed_body_digest_hex() {
        return Err(WalletReceiptError::SignedEnvelopeMismatch);
    }
    Ok(Some(ExactAcceptedLookup {
        transaction_id: expected.transaction_id().to_string(),
        nonce: expected.nonce(),
    }))
}
