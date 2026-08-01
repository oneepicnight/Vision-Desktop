#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "receipt observation remains internal until wallet submission is approved"
    )
)]

use super::transaction::{canonical_transaction_id, VisionTransaction};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub(in crate::wallet) enum WalletReceiptError {
    InvalidResponse,
    TransactionIdMismatch,
    InvalidBlockReference,
}

impl fmt::Display for WalletReceiptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidResponse => "Core returned an invalid transaction observation",
            Self::TransactionIdMismatch => "Core returned a different transaction",
            Self::InvalidBlockReference => "Core returned an invalid canonical block reference",
        })
    }
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
}
