#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "transaction signing remains internal until all submission gates pass"
    )
)]

use super::{account::derive_account_identity, runtime::WalletSigningPermit, secrets::WalletSeed};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use std::fmt;

const CASH_MODULE: &str = "cash";
const TRANSFER_METHOD: &str = "transfer";
const MIN_CASH_TRANSFER_FEE_LIMIT: u64 = 201;
const CASH_TRANSFER_BASE_FEE: u64 = 1;
const DEFAULT_CASH_TRANSFER_TIP: u64 = 0;

/// Exact Vision-Core RC2 transaction envelope.
///
/// Field order is consensus-relevant because RC2 signs the bincode 1.3.3
/// serialization of this structure after clearing `sig`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::wallet) struct VisionTransaction {
    pub nonce: u64,
    pub sender_pubkey: String,
    pub module: String,
    pub method: String,
    pub args: Vec<u8>,
    pub tip: u64,
    pub fee_limit: u64,
    pub sig: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::wallet) struct CashTransferDraft {
    pub nonce: u64,
    pub recipient: String,
    pub amount_raw_units: u128,
    pub tip_raw_units: u64,
    pub fee_limit_raw_units: u64,
}

impl CashTransferDraft {
    /// Safe first-send policy: use the exact canonical nonce, no replacement
    /// tip, and the RC2 minimum authorized transfer fee limit.
    pub(in crate::wallet) fn for_current_nonce(
        nonce: u64,
        recipient: String,
        amount_raw_units: u128,
    ) -> Self {
        Self {
            nonce,
            recipient,
            amount_raw_units,
            tip_raw_units: DEFAULT_CASH_TRANSFER_TIP,
            fee_limit_raw_units: MIN_CASH_TRANSFER_FEE_LIMIT,
        }
    }
}

#[derive(Serialize)]
struct CashTransferArgs<'a> {
    to: &'a str,
    amount: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletTransactionError {
    SigningUnavailable,
    InvalidRecipient,
    ZeroAmount,
    TransferToSelf,
    FeeLimitTooLow,
    FeeArithmeticOverflow,
    FeeExceedsLimit,
    SerializationUnavailable,
}

impl fmt::Display for WalletTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::SigningUnavailable => "wallet signing authority is unavailable",
            Self::InvalidRecipient => "recipient account address is invalid",
            Self::ZeroAmount => "transfer amount must be greater than zero",
            Self::TransferToSelf => "transfer recipient must differ from the sender",
            Self::FeeLimitTooLow => "transfer fee limit is below the Core minimum",
            Self::FeeArithmeticOverflow => "transfer fee arithmetic overflowed",
            Self::FeeExceedsLimit => "transfer fee exceeds the authorized limit",
            Self::SerializationUnavailable => "transaction serialization is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for WalletTransactionError {}

/// Produces the exact RC2 unsigned signing bytes without exposing the seed.
pub(in crate::wallet) fn canonical_unsigned_payload(
    transaction: &VisionTransaction,
) -> Result<Vec<u8>, WalletTransactionError> {
    let mut unsigned = transaction.clone();
    unsigned.sig.clear();
    bincode::serialize(&unsigned).map_err(|_| WalletTransactionError::SerializationUnavailable)
}

/// RC2 transaction identifier: lowercase BLAKE3 of the unsigned payload.
pub(in crate::wallet) fn canonical_transaction_id(
    transaction: &VisionTransaction,
) -> Result<String, WalletTransactionError> {
    let payload = canonical_unsigned_payload(transaction)?;
    Ok(hex::encode(blake3::hash(&payload).as_bytes()))
}

/// Builds and signs only RC2 `cash::transfer` transactions inside Rust.
///
/// This is not registered as a Tauri command. Amount, nonce, and fee policy is
/// verified internally, but no UI may reach signing until every remaining
/// compatibility and security gate passes.
pub(in crate::wallet) fn sign_cash_transfer(
    permit: &WalletSigningPermit<'_>,
    seed: &WalletSeed,
    draft: &CashTransferDraft,
) -> Result<VisionTransaction, WalletTransactionError> {
    permit
        .ensure_current()
        .map_err(|_| WalletTransactionError::SigningUnavailable)?;
    if !is_lowercase_hex_32_bytes(&draft.recipient) {
        return Err(WalletTransactionError::InvalidRecipient);
    }
    if draft.amount_raw_units == 0 {
        return Err(WalletTransactionError::ZeroAmount);
    }
    if draft.fee_limit_raw_units < MIN_CASH_TRANSFER_FEE_LIMIT {
        return Err(WalletTransactionError::FeeLimitTooLow);
    }
    let charged_fee = CASH_TRANSFER_BASE_FEE
        .checked_add(draft.tip_raw_units)
        .ok_or(WalletTransactionError::FeeArithmeticOverflow)?;
    if charged_fee > draft.fee_limit_raw_units {
        return Err(WalletTransactionError::FeeExceedsLimit);
    }

    let identity = derive_account_identity(seed);
    if draft.recipient == identity.address {
        return Err(WalletTransactionError::TransferToSelf);
    }
    let args = serde_json::to_vec(&CashTransferArgs {
        to: &draft.recipient,
        amount: draft.amount_raw_units,
    })
    .map_err(|_| WalletTransactionError::SerializationUnavailable)?;
    let mut transaction = VisionTransaction {
        nonce: draft.nonce,
        sender_pubkey: identity.public_key,
        module: CASH_MODULE.to_string(),
        method: TRANSFER_METHOD.to_string(),
        args,
        tip: draft.tip_raw_units,
        fee_limit: draft.fee_limit_raw_units,
        sig: String::new(),
    };
    let payload = canonical_unsigned_payload(&transaction)?;
    transaction.sig = seed.with_exposed(|seed_bytes| {
        let signing_key = SigningKey::from_bytes(seed_bytes);
        hex::encode(signing_key.sign(&payload).to_bytes())
    });
    Ok(transaction)
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
    use crate::wallet::runtime::WalletRuntimeState;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    const CORE_SAMPLE_PAYLOAD_HEX: &str = concat!(
        "0100000000000000",
        "4000000000000000",
        "61616161616161616161616161616161",
        "61616161616161616161616161616161",
        "61616161616161616161616161616161",
        "61616161616161616161616161616161",
        "0400000000000000",
        "63617368",
        "0800000000000000",
        "7472616e73666572",
        "0400000000000000",
        "deadbeef",
        "6400000000000000",
        "1027000000000000",
        "0000000000000000",
    );
    const CORE_SAMPLE_TX_ID: &str =
        "a7fc34bf3332fec96623ea7f5ddb638aaad51f039091d2d5bf94adb76a26f0dd";
    const SIGNED_VECTOR_PUBLIC_KEY: &str =
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
    const SIGNED_VECTOR_PAYLOAD_HEX: &str = "010000000000000040000000000000006561346136633633653239633532306162656635353037623133326563356639393534373736616562656265376239323432316565613639313434366432326304000000000000006361736808000000000000007472616e7366657255000000000000007b22746f223a2232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232222c22616d6f756e74223a34327d0200000000000000c9000000000000000000000000000000";
    const SIGNED_VECTOR_SIGNATURE: &str = concat!(
        "9e6e02196b7dd976f71fcb34c2e420a4",
        "cf1b70731e96dcffbe7223969ae760a7",
        "eee386e0490d8dbe9a0bdb3056bbfdb3",
        "5b17e98b189b1288d6ce813df9c82008",
    );

    fn core_sample_transaction() -> VisionTransaction {
        VisionTransaction {
            nonce: 1,
            sender_pubkey: "aa".repeat(32),
            module: CASH_MODULE.to_string(),
            method: TRANSFER_METHOD.to_string(),
            args: vec![0xde, 0xad, 0xbe, 0xef],
            tip: 100,
            fee_limit: 10_000,
            sig: "11".repeat(64),
        }
    }

    fn signed_vector_draft() -> CashTransferDraft {
        CashTransferDraft {
            nonce: 1,
            recipient: "22".repeat(32),
            amount_raw_units: 42,
            tip_raw_units: 2,
            fee_limit_raw_units: MIN_CASH_TRANSFER_FEE_LIMIT,
        }
    }

    fn sign_vector(
        runtime: &WalletRuntimeState,
        seed: &WalletSeed,
        draft: &CashTransferDraft,
    ) -> Result<VisionTransaction, WalletTransactionError> {
        let permit = runtime.begin_signing_operation("main").unwrap();
        sign_cash_transfer(&permit, seed, draft)
    }

    #[test]
    fn canonical_payload_matches_exact_core_rc2_vector() {
        let payload = canonical_unsigned_payload(&core_sample_transaction()).unwrap();

        assert_eq!(hex::encode(payload), CORE_SAMPLE_PAYLOAD_HEX);
    }

    #[test]
    fn canonical_transaction_id_matches_exact_core_rc2_vector() {
        let transaction = core_sample_transaction();

        assert_eq!(
            canonical_transaction_id(&transaction).unwrap(),
            CORE_SAMPLE_TX_ID
        );
        let mut changed_signature = transaction;
        changed_signature.sig = "ff".repeat(64);
        assert_eq!(
            canonical_transaction_id(&changed_signature).unwrap(),
            CORE_SAMPLE_TX_ID
        );
    }

    #[test]
    fn cash_transfer_signature_matches_independent_fixed_vector() {
        let runtime = WalletRuntimeState::for_test();
        let transaction = sign_vector(
            &runtime,
            &WalletSeed::from_bytes([7; 32]),
            &signed_vector_draft(),
        )
        .unwrap();

        assert_eq!(transaction.sender_pubkey, SIGNED_VECTOR_PUBLIC_KEY);
        assert_eq!(
            hex::encode(canonical_unsigned_payload(&transaction).unwrap()),
            SIGNED_VECTOR_PAYLOAD_HEX
        );
        assert_eq!(transaction.sig, SIGNED_VECTOR_SIGNATURE);

        let public_key_bytes: [u8; 32] = hex::decode(&transaction.sender_pubkey)
            .unwrap()
            .try_into()
            .unwrap();
        let signature_bytes: [u8; 64] = hex::decode(&transaction.sig).unwrap().try_into().unwrap();
        VerifyingKey::from_bytes(&public_key_bytes)
            .unwrap()
            .verify(
                &canonical_unsigned_payload(&transaction).unwrap(),
                &Signature::from_bytes(&signature_bytes),
            )
            .unwrap();
    }

    #[test]
    fn signing_refuses_runtime_authority_revoked_after_admission() {
        let runtime = WalletRuntimeState::for_test();
        let permit = runtime.begin_signing_operation("main").unwrap();
        runtime.invalidate_all().unwrap();

        assert_eq!(
            sign_cash_transfer(
                &permit,
                &WalletSeed::from_bytes([7; 32]),
                &signed_vector_draft(),
            )
            .unwrap_err(),
            WalletTransactionError::SigningUnavailable
        );
    }

    #[test]
    fn cash_transfer_builder_rejects_unsafe_shapes_before_signing() {
        let runtime = WalletRuntimeState::for_test();
        let seed = WalletSeed::from_bytes([7; 32]);
        let mut draft = signed_vector_draft();
        draft.recipient = "AA".repeat(32);
        assert_eq!(
            sign_vector(&runtime, &seed, &draft).unwrap_err(),
            WalletTransactionError::InvalidRecipient
        );

        let mut draft = signed_vector_draft();
        draft.amount_raw_units = 0;
        assert_eq!(
            sign_vector(&runtime, &seed, &draft).unwrap_err(),
            WalletTransactionError::ZeroAmount
        );

        let mut draft = signed_vector_draft();
        draft.fee_limit_raw_units = MIN_CASH_TRANSFER_FEE_LIMIT - 1;
        assert_eq!(
            sign_vector(&runtime, &seed, &draft).unwrap_err(),
            WalletTransactionError::FeeLimitTooLow
        );

        let mut draft = signed_vector_draft();
        draft.tip_raw_units = u64::MAX;
        assert_eq!(
            sign_vector(&runtime, &seed, &draft).unwrap_err(),
            WalletTransactionError::FeeArithmeticOverflow
        );

        let mut draft = signed_vector_draft();
        draft.tip_raw_units = MIN_CASH_TRANSFER_FEE_LIMIT;
        assert_eq!(
            sign_vector(&runtime, &seed, &draft).unwrap_err(),
            WalletTransactionError::FeeExceedsLimit
        );

        let mut draft = signed_vector_draft();
        draft.recipient = derive_account_identity(&seed).address;
        assert_eq!(
            sign_vector(&runtime, &seed, &draft).unwrap_err(),
            WalletTransactionError::TransferToSelf
        );
    }

    #[test]
    fn safe_draft_uses_current_nonce_and_non_replacing_fee_policy() {
        let runtime = WalletRuntimeState::for_test();
        let draft = CashTransferDraft::for_current_nonce(7, "22".repeat(32), 42);

        assert_eq!(draft.nonce, 7);
        assert_eq!(draft.tip_raw_units, 0);
        assert_eq!(draft.fee_limit_raw_units, 201);
        sign_vector(&runtime, &WalletSeed::from_bytes([7; 32]), &draft).unwrap();
    }

    #[test]
    fn signed_transaction_json_contains_no_secret_material() {
        let runtime = WalletRuntimeState::for_test();
        let transaction = sign_vector(
            &runtime,
            &WalletSeed::from_bytes([7; 32]),
            &signed_vector_draft(),
        )
        .unwrap();
        let json = serde_json::to_string(&transaction).unwrap();

        for forbidden in [
            "private_key",
            "secret_key",
            "seed",
            "mnemonic",
            "password",
            "recovery",
        ] {
            assert!(!json.contains(forbidden));
        }
        assert!(json.contains(SIGNED_VECTOR_PUBLIC_KEY));
        assert!(json.contains(SIGNED_VECTOR_SIGNATURE));
    }
}
