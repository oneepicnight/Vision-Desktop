use super::{
    account::derive_account_identity, runtime::WalletActivationProof, secrets::WalletSeed,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
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
#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq, Eq)]
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

    pub(in crate::wallet) fn charged_fee_raw_units(&self) -> Result<u64, WalletTransactionError> {
        CASH_TRANSFER_BASE_FEE
            .checked_add(self.tip_raw_units)
            .ok_or(WalletTransactionError::FeeArithmeticOverflow)
    }

    pub(in crate::wallet) const fn fee_limit_raw_units(&self) -> u64 {
        self.fee_limit_raw_units
    }
}

#[derive(Serialize)]
struct CashTransferArgs<'a> {
    to: &'a str,
    amount: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletTransactionError {
    ActivationUnavailable,
    InvalidRecipient,
    ZeroAmount,
    TransferToSelf,
    FeeLimitTooLow,
    FeeArithmeticOverflow,
    FeeExceedsLimit,
    InvalidSender,
    ConfirmedIntentMismatch,
    SignatureUnavailable,
    SignatureVerificationFailed,
    SerializationUnavailable,
}

impl fmt::Display for WalletTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ActivationUnavailable => "wallet signing activation is unavailable",
            Self::InvalidRecipient => "recipient account address is invalid",
            Self::ZeroAmount => "transfer amount must be greater than zero",
            Self::TransferToSelf => "transfer recipient must differ from the sender",
            Self::FeeLimitTooLow => "transfer fee limit is below the Core minimum",
            Self::FeeArithmeticOverflow => "transfer fee arithmetic overflowed",
            Self::FeeExceedsLimit => "transfer fee exceeds the authorized limit",
            Self::InvalidSender => "sender account identity is invalid",
            Self::ConfirmedIntentMismatch => "confirmed transaction intent is invalid",
            Self::SignatureUnavailable => "transaction signature is unavailable",
            Self::SignatureVerificationFailed => "transaction signature verification failed",
            Self::SerializationUnavailable => "transaction serialization is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for WalletTransactionError {}

/// Borrowed exact intent accepted by the private confirmation-to-signing bridge.
///
/// This type intentionally implements neither Clone, Debug, nor serialization. It carries only
/// public transaction material; possession is not signing authority.
pub(in crate::wallet) struct ConfirmedCashTransfer<'a> {
    pub unsigned_transaction: &'a VisionTransaction,
    pub sender_address: &'a str,
    pub recipient_address: &'a str,
    pub amount_raw_units: u128,
    pub charged_fee_raw_units: u64,
    pub fee_limit_raw_units: u64,
    pub total_debit_raw_units: u128,
    pub nonce: u64,
    pub transaction_id: &'a str,
    pub core_contract: &'a str,
    pub status_version: &'a str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum TransactionSigningStage {
    SeedAccountDerivation,
    SignatureConstruction,
    SignatureVerification,
}

pub(in crate::wallet) trait TransactionSigningObserver {
    fn checkpoint(&self, stage: TransactionSigningStage);
}

#[cfg(test)]
pub(in crate::wallet) struct NoopTransactionSigningObserver;

#[cfg(test)]
impl TransactionSigningObserver for NoopTransactionSigningObserver {
    fn checkpoint(&self, _stage: TransactionSigningStage) {}
}

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

/// Constructs the complete unsigned RC2 cash transfer from Rust-authoritative fields.
///
/// The result remains private Rust state. This function performs no signing and no network write.
pub(in crate::wallet) fn build_unsigned_cash_transfer(
    sender_public_key: String,
    sender_address: &str,
    draft: &CashTransferDraft,
) -> Result<VisionTransaction, WalletTransactionError> {
    if !is_lowercase_hex_32_bytes(&sender_public_key)
        || !is_lowercase_hex_32_bytes(sender_address)
        || sender_public_key != sender_address
    {
        return Err(WalletTransactionError::InvalidSender);
    }
    if !is_lowercase_hex_32_bytes(&draft.recipient) {
        return Err(WalletTransactionError::InvalidRecipient);
    }
    if draft.amount_raw_units == 0 {
        return Err(WalletTransactionError::ZeroAmount);
    }
    if draft.recipient == sender_address {
        return Err(WalletTransactionError::TransferToSelf);
    }
    if draft.fee_limit_raw_units < MIN_CASH_TRANSFER_FEE_LIMIT {
        return Err(WalletTransactionError::FeeLimitTooLow);
    }
    let charged_fee = draft.charged_fee_raw_units()?;
    if charged_fee > draft.fee_limit_raw_units {
        return Err(WalletTransactionError::FeeExceedsLimit);
    }

    let args = serde_json::to_vec(&CashTransferArgs {
        to: &draft.recipient,
        amount: draft.amount_raw_units,
    })
    .map_err(|_| WalletTransactionError::SerializationUnavailable)?;
    Ok(VisionTransaction {
        nonce: draft.nonce,
        sender_pubkey: sender_public_key,
        module: CASH_MODULE.to_string(),
        method: TRANSFER_METHOD.to_string(),
        args,
        tip: draft.tip_raw_units,
        fee_limit: draft.fee_limit_raw_units,
        sig: String::new(),
    })
}

/// Builds and signs only RC2 cash transfer transactions inside Rust.
///
/// This is not registered as a Tauri command. Amount, nonce, and fee policy is
/// verified internally, but no UI may reach signing until every remaining
/// compatibility and security gate passes.
#[cfg(test)]
pub(in crate::wallet) fn sign_cash_transfer(
    activation: &WalletActivationProof,
    seed: &WalletSeed,
    draft: &CashTransferDraft,
) -> Result<VisionTransaction, WalletTransactionError> {
    activation
        .require_signing()
        .map_err(|_| WalletTransactionError::ActivationUnavailable)?;
    let identity = derive_account_identity(seed);
    let mut transaction =
        build_unsigned_cash_transfer(identity.public_key, &identity.address, draft)?;
    let payload = canonical_unsigned_payload(&transaction)?;
    transaction.sig = seed.with_exposed(|seed_bytes| {
        let signing_key = SigningKey::from_bytes(seed_bytes);
        hex::encode(signing_key.sign(&payload).to_bytes())
    });
    Ok(transaction)
}

/// Signs only the exact unsigned transfer retained through native confirmation.
///
/// The caller must additionally hold the promoted runtime signing permit. This function performs
/// no network write and exposes no secret material.
pub(in crate::wallet) fn sign_confirmed_cash_transfer(
    activation: &WalletActivationProof,
    seed: &WalletSeed,
    confirmed: ConfirmedCashTransfer<'_>,
    expected_core_contract: &str,
    expected_status_version: &str,
    observer: &dyn TransactionSigningObserver,
) -> Result<VisionTransaction, WalletTransactionError> {
    activation
        .require_signing()
        .map_err(|_| WalletTransactionError::ActivationUnavailable)?;
    validate_confirmed_cash_transfer(&confirmed, expected_core_contract, expected_status_version)?;

    observer.checkpoint(TransactionSigningStage::SeedAccountDerivation);
    let identity = derive_account_identity(seed);
    if identity.address != confirmed.sender_address
        || identity.public_key != confirmed.unsigned_transaction.sender_pubkey
    {
        return Err(WalletTransactionError::InvalidSender);
    }

    let payload = canonical_unsigned_payload(confirmed.unsigned_transaction)?;
    observer.checkpoint(TransactionSigningStage::SignatureConstruction);
    let signature_hex = seed.with_exposed(|seed_bytes| {
        let signing_key = SigningKey::from_bytes(seed_bytes);
        hex::encode(signing_key.sign(&payload).to_bytes())
    });
    verify_signature(&signature_hex, &payload, &identity.public_key, observer)?;

    let mut signed = confirmed.unsigned_transaction.clone();
    signed.sig = signature_hex;
    if canonical_transaction_id(&signed)? != confirmed.transaction_id {
        return Err(WalletTransactionError::ConfirmedIntentMismatch);
    }
    Ok(signed)
}

fn verify_signature(
    signature_hex: &str,
    payload: &[u8],
    public_key_hex: &str,
    observer: &dyn TransactionSigningObserver,
) -> Result<(), WalletTransactionError> {
    let signature_bytes: [u8; 64] = hex::decode(signature_hex)
        .map_err(|_| WalletTransactionError::SignatureUnavailable)?
        .try_into()
        .map_err(|_| WalletTransactionError::SignatureUnavailable)?;
    let public_key_bytes: [u8; 32] = hex::decode(public_key_hex)
        .map_err(|_| WalletTransactionError::SignatureUnavailable)?
        .try_into()
        .map_err(|_| WalletTransactionError::SignatureUnavailable)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|_| WalletTransactionError::SignatureUnavailable)?;
    observer.checkpoint(TransactionSigningStage::SignatureVerification);
    verifying_key
        .verify(payload, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| WalletTransactionError::SignatureVerificationFailed)
}

fn validate_confirmed_cash_transfer(
    confirmed: &ConfirmedCashTransfer<'_>,
    expected_core_contract: &str,
    expected_status_version: &str,
) -> Result<(), WalletTransactionError> {
    let transaction = confirmed.unsigned_transaction;
    if !transaction.sig.is_empty()
        || confirmed.core_contract != expected_core_contract
        || confirmed.status_version != expected_status_version
        || transaction.sender_pubkey != confirmed.sender_address
        || !is_lowercase_hex_32_bytes(confirmed.sender_address)
        || !is_lowercase_hex_32_bytes(&transaction.sender_pubkey)
        || !is_lowercase_hex_32_bytes(confirmed.recipient_address)
        || confirmed.recipient_address == confirmed.sender_address
        || transaction.module != CASH_MODULE
        || transaction.method != TRANSFER_METHOD
        || transaction.nonce != confirmed.nonce
        || transaction.tip != DEFAULT_CASH_TRANSFER_TIP
        || transaction.fee_limit != MIN_CASH_TRANSFER_FEE_LIMIT
        || confirmed.fee_limit_raw_units != MIN_CASH_TRANSFER_FEE_LIMIT
        || confirmed.charged_fee_raw_units != CASH_TRANSFER_BASE_FEE
        || confirmed.amount_raw_units == 0
    {
        return Err(WalletTransactionError::ConfirmedIntentMismatch);
    }
    let expected_total = confirmed
        .amount_raw_units
        .checked_add(u128::from(CASH_TRANSFER_BASE_FEE))
        .ok_or(WalletTransactionError::FeeArithmeticOverflow)?;
    if expected_total != confirmed.total_debit_raw_units {
        return Err(WalletTransactionError::ConfirmedIntentMismatch);
    }
    let expected_args = serde_json::to_vec(&CashTransferArgs {
        to: confirmed.recipient_address,
        amount: confirmed.amount_raw_units,
    })
    .map_err(|_| WalletTransactionError::SerializationUnavailable)?;
    if transaction.args != expected_args
        || canonical_transaction_id(transaction)? != confirmed.transaction_id
    {
        return Err(WalletTransactionError::ConfirmedIntentMismatch);
    }
    Ok(())
}

#[cfg(test)]
pub(in crate::wallet) fn sign_cash_transfer_for_test(
    seed: &WalletSeed,
    draft: &CashTransferDraft,
) -> Result<VisionTransaction, WalletTransactionError> {
    super::runtime::WalletRuntimeState::with_activation_proof_for_test(
        super::runtime::WalletOperationKind::Sign,
        |activation| sign_cash_transfer(activation, seed, draft),
    )
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
    use crate::wallet::runtime::{WalletOperationKind, WalletRuntimeState};
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
        let transaction =
            sign_cash_transfer_for_test(&WalletSeed::for_test(7), &signed_vector_draft()).unwrap();

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
    fn lifecycle_activation_proof_cannot_authorize_signing() {
        let result = super::super::runtime::WalletRuntimeState::with_activation_proof_for_test(
            super::super::runtime::WalletOperationKind::Create,
            |activation| {
                sign_cash_transfer(activation, &WalletSeed::for_test(7), &signed_vector_draft())
            },
        );

        assert_eq!(result, Err(WalletTransactionError::ActivationUnavailable));
    }

    #[test]
    fn cash_transfer_builder_rejects_unsafe_shapes_before_signing() {
        let seed = WalletSeed::for_test(7);
        assert_eq!(
            build_unsigned_cash_transfer("11".repeat(32), &"22".repeat(32), &signed_vector_draft(),),
            Err(WalletTransactionError::InvalidSender)
        );
        let mut draft = signed_vector_draft();
        draft.recipient = "AA".repeat(32);
        assert_eq!(
            sign_cash_transfer_for_test(&seed, &draft).unwrap_err(),
            WalletTransactionError::InvalidRecipient
        );

        let mut draft = signed_vector_draft();
        draft.amount_raw_units = 0;
        assert_eq!(
            sign_cash_transfer_for_test(&seed, &draft).unwrap_err(),
            WalletTransactionError::ZeroAmount
        );

        let mut draft = signed_vector_draft();
        draft.fee_limit_raw_units = MIN_CASH_TRANSFER_FEE_LIMIT - 1;
        assert_eq!(
            sign_cash_transfer_for_test(&seed, &draft).unwrap_err(),
            WalletTransactionError::FeeLimitTooLow
        );

        let mut draft = signed_vector_draft();
        draft.tip_raw_units = u64::MAX;
        assert_eq!(
            sign_cash_transfer_for_test(&seed, &draft).unwrap_err(),
            WalletTransactionError::FeeArithmeticOverflow
        );

        let mut draft = signed_vector_draft();
        draft.tip_raw_units = MIN_CASH_TRANSFER_FEE_LIMIT;
        assert_eq!(
            sign_cash_transfer_for_test(&seed, &draft).unwrap_err(),
            WalletTransactionError::FeeExceedsLimit
        );

        let mut draft = signed_vector_draft();
        draft.recipient = derive_account_identity(&seed).address;
        assert_eq!(
            sign_cash_transfer_for_test(&seed, &draft).unwrap_err(),
            WalletTransactionError::TransferToSelf
        );
    }

    #[test]
    fn safe_draft_uses_current_nonce_and_non_replacing_fee_policy() {
        let draft = CashTransferDraft::for_current_nonce(7, "22".repeat(32), 42);

        assert_eq!(draft.nonce, 7);
        assert_eq!(draft.tip_raw_units, 0);
        assert_eq!(draft.fee_limit_raw_units, 201);
        sign_cash_transfer_for_test(&WalletSeed::for_test(7), &draft).unwrap();
    }

    #[test]
    fn signed_transaction_json_contains_no_secret_material() {
        let transaction =
            sign_cash_transfer_for_test(&WalletSeed::for_test(7), &signed_vector_draft()).unwrap();
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

    fn confirmed_signing_fixture() -> (WalletSeed, VisionTransaction, String, String, String) {
        let seed = WalletSeed::for_test(7);
        let sender = derive_account_identity(&seed).address;
        let recipient = "22".repeat(32);
        let draft = CashTransferDraft::for_current_nonce(9, recipient.clone(), 42);
        let transaction = build_unsigned_cash_transfer(sender.clone(), &sender, &draft).unwrap();
        let transaction_id = canonical_transaction_id(&transaction).unwrap();
        (seed, transaction, sender, recipient, transaction_id)
    }

    fn confirmed_view<'a>(
        transaction: &'a VisionTransaction,
        sender: &'a str,
        recipient: &'a str,
        transaction_id: &'a str,
    ) -> ConfirmedCashTransfer<'a> {
        ConfirmedCashTransfer {
            unsigned_transaction: transaction,
            sender_address: sender,
            recipient_address: recipient,
            amount_raw_units: 42,
            charged_fee_raw_units: 1,
            fee_limit_raw_units: 201,
            total_debit_raw_units: 43,
            nonce: 9,
            transaction_id,
            core_contract: "vision-wallet-read-v1",
            status_version: "3",
        }
    }

    #[test]
    fn exact_confirmed_transaction_is_signed_and_verified_without_changing_its_identifier() {
        let (seed, transaction, sender, recipient, transaction_id) = confirmed_signing_fixture();
        let signed = super::super::runtime::WalletRuntimeState::with_activation_proof_for_test(
            super::super::runtime::WalletOperationKind::Sign,
            |activation| {
                sign_confirmed_cash_transfer(
                    activation,
                    &seed,
                    confirmed_view(&transaction, &sender, &recipient, &transaction_id),
                    "vision-wallet-read-v1",
                    "3",
                    &NoopTransactionSigningObserver,
                )
            },
        )
        .unwrap();

        assert_eq!(canonical_transaction_id(&signed).unwrap(), transaction_id);
        assert_eq!(signed.nonce, transaction.nonce);
        assert_eq!(signed.sender_pubkey, transaction.sender_pubkey);
        assert_eq!(signed.module, transaction.module);
        assert_eq!(signed.method, transaction.method);
        assert_eq!(signed.args, transaction.args);
        assert_eq!(signed.tip, transaction.tip);
        assert_eq!(signed.fee_limit, transaction.fee_limit);
        assert_eq!(signed.sig.len(), 128);
    }

    #[test]
    fn confirmed_signing_rejects_mutated_or_semantically_reencoded_intents() {
        let (seed, transaction, sender, recipient, transaction_id) = confirmed_signing_fixture();
        let assert_rejected = |candidate: &VisionTransaction,
                               candidate_sender: &str,
                               candidate_recipient: &str,
                               candidate_id: &str,
                               contract: &str,
                               version: &str| {
            let result = super::super::runtime::WalletRuntimeState::with_activation_proof_for_test(
                super::super::runtime::WalletOperationKind::Sign,
                |activation| {
                    sign_confirmed_cash_transfer(
                        activation,
                        &seed,
                        confirmed_view(
                            candidate,
                            candidate_sender,
                            candidate_recipient,
                            candidate_id,
                        ),
                        contract,
                        version,
                        &NoopTransactionSigningObserver,
                    )
                },
            );
            assert_eq!(result, Err(WalletTransactionError::ConfirmedIntentMismatch));
        };

        let mut signed_before_confirmation = transaction.clone();
        signed_before_confirmation.sig = "00".repeat(64);
        assert_rejected(
            &signed_before_confirmation,
            &sender,
            &recipient,
            &transaction_id,
            "vision-wallet-read-v1",
            "3",
        );

        let mut reencoded_args = transaction.clone();
        reencoded_args.args = format!("{{\"amount\":42,\"to\":\"{recipient}\"}}").into_bytes();
        let reencoded_id = canonical_transaction_id(&reencoded_args).unwrap();
        assert_rejected(
            &reencoded_args,
            &sender,
            &recipient,
            &reencoded_id,
            "vision-wallet-read-v1",
            "3",
        );

        assert_rejected(
            &transaction,
            &sender,
            &recipient,
            &transaction_id,
            "stale-contract",
            "3",
        );
        assert_rejected(
            &transaction,
            &sender,
            &recipient,
            &transaction_id,
            "vision-wallet-read-v1",
            "2",
        );
    }

    struct OwnedConfirmedFixture {
        seed: WalletSeed,
        transaction: VisionTransaction,
        sender: String,
        recipient: String,
        amount_raw_units: u128,
        charged_fee_raw_units: u64,
        fee_limit_raw_units: u64,
        total_debit_raw_units: u128,
        nonce: u64,
        transaction_id: String,
        core_contract: String,
        status_version: String,
    }

    impl OwnedConfirmedFixture {
        fn new() -> Self {
            let (seed, transaction, sender, recipient, transaction_id) =
                confirmed_signing_fixture();
            Self {
                seed,
                transaction,
                sender,
                recipient,
                amount_raw_units: 42,
                charged_fee_raw_units: 1,
                fee_limit_raw_units: 201,
                total_debit_raw_units: 43,
                nonce: 9,
                transaction_id,
                core_contract: "vision-wallet-read-v1".to_string(),
                status_version: "3".to_string(),
            }
        }

        fn view(&self) -> ConfirmedCashTransfer<'_> {
            ConfirmedCashTransfer {
                unsigned_transaction: &self.transaction,
                sender_address: &self.sender,
                recipient_address: &self.recipient,
                amount_raw_units: self.amount_raw_units,
                charged_fee_raw_units: self.charged_fee_raw_units,
                fee_limit_raw_units: self.fee_limit_raw_units,
                total_debit_raw_units: self.total_debit_raw_units,
                nonce: self.nonce,
                transaction_id: &self.transaction_id,
                core_contract: &self.core_contract,
                status_version: &self.status_version,
            }
        }

        fn sign(
            &self,
            observer: &dyn TransactionSigningObserver,
        ) -> Result<VisionTransaction, WalletTransactionError> {
            WalletRuntimeState::with_activation_proof_for_test(
                WalletOperationKind::Sign,
                |activation| {
                    sign_confirmed_cash_transfer(
                        activation,
                        &self.seed,
                        self.view(),
                        "vision-wallet-read-v1",
                        "3",
                        observer,
                    )
                },
            )
        }
    }

    #[derive(Clone, Copy)]
    enum RetainedIntentMutation {
        SenderEcho,
        SenderTransaction,
        SeedAccountMismatch,
        Recipient,
        Amount,
        ZeroAmount,
        NonceEcho,
        NonceTransaction,
        Tip,
        TransactionFeeLimit,
        ConfirmedFeeLimit,
        ChargedFee,
        TotalDebit,
        Module,
        Method,
        Arguments,
        ArgumentRecipient,
        ArgumentAmount,
        ArgumentUnknownField,
        Contract,
        Status,
        Identifier,
        ExistingSignature,
    }

    fn mutate_retained_intent(
        fixture: &mut OwnedConfirmedFixture,
        mutation: RetainedIntentMutation,
    ) {
        match mutation {
            RetainedIntentMutation::SenderEcho => fixture.sender = "3".repeat(64),
            RetainedIntentMutation::SenderTransaction => {
                fixture.transaction.sender_pubkey = "3".repeat(64)
            }
            RetainedIntentMutation::SeedAccountMismatch => {
                fixture.sender = "3".repeat(64);
                fixture.transaction.sender_pubkey = fixture.sender.clone();
                fixture.transaction_id = canonical_transaction_id(&fixture.transaction).unwrap();
            }
            RetainedIntentMutation::Recipient => fixture.recipient = "4".repeat(64),
            RetainedIntentMutation::Amount => fixture.amount_raw_units += 1,
            RetainedIntentMutation::ZeroAmount => fixture.amount_raw_units = 0,
            RetainedIntentMutation::NonceEcho => fixture.nonce += 1,
            RetainedIntentMutation::NonceTransaction => fixture.transaction.nonce += 1,
            RetainedIntentMutation::Tip => fixture.transaction.tip = 1,
            RetainedIntentMutation::TransactionFeeLimit => fixture.transaction.fee_limit = 202,
            RetainedIntentMutation::ConfirmedFeeLimit => fixture.fee_limit_raw_units = 202,
            RetainedIntentMutation::ChargedFee => fixture.charged_fee_raw_units = 2,
            RetainedIntentMutation::TotalDebit => fixture.total_debit_raw_units += 1,
            RetainedIntentMutation::Module => fixture.transaction.module = "stake".to_string(),
            RetainedIntentMutation::Method => fixture.transaction.method = "mint".to_string(),
            RetainedIntentMutation::Arguments => {
                fixture.transaction.args =
                    format!("{{\"amount\":42,\"to\":\"{}\"}}", fixture.recipient).into_bytes();
                fixture.transaction_id = canonical_transaction_id(&fixture.transaction).unwrap();
            }
            RetainedIntentMutation::ArgumentRecipient => {
                fixture.transaction.args =
                    format!("{{\"to\":\"{}\",\"amount\":42}}", "5".repeat(64)).into_bytes();
                fixture.transaction_id = canonical_transaction_id(&fixture.transaction).unwrap();
            }
            RetainedIntentMutation::ArgumentAmount => {
                fixture.transaction.args =
                    format!("{{\"to\":\"{}\",\"amount\":43}}", fixture.recipient).into_bytes();
                fixture.transaction_id = canonical_transaction_id(&fixture.transaction).unwrap();
            }
            RetainedIntentMutation::ArgumentUnknownField => {
                fixture.transaction.args = format!(
                    "{{\"to\":\"{}\",\"amount\":42,\"memo\":\"unexpected\"}}",
                    fixture.recipient
                )
                .into_bytes();
                fixture.transaction_id = canonical_transaction_id(&fixture.transaction).unwrap();
            }
            RetainedIntentMutation::Contract => {
                fixture.core_contract = "stale-contract".to_string()
            }
            RetainedIntentMutation::Status => fixture.status_version = "2".to_string(),
            RetainedIntentMutation::Identifier => fixture.transaction_id = "f".repeat(64),
            RetainedIntentMutation::ExistingSignature => fixture.transaction.sig = "00".repeat(64),
        }
    }

    #[test]
    fn every_retained_transaction_field_mutation_is_rejected_before_release() {
        for mutation in [
            RetainedIntentMutation::SenderEcho,
            RetainedIntentMutation::SenderTransaction,
            RetainedIntentMutation::SeedAccountMismatch,
            RetainedIntentMutation::Recipient,
            RetainedIntentMutation::Amount,
            RetainedIntentMutation::ZeroAmount,
            RetainedIntentMutation::NonceEcho,
            RetainedIntentMutation::NonceTransaction,
            RetainedIntentMutation::Tip,
            RetainedIntentMutation::TransactionFeeLimit,
            RetainedIntentMutation::ConfirmedFeeLimit,
            RetainedIntentMutation::ChargedFee,
            RetainedIntentMutation::TotalDebit,
            RetainedIntentMutation::Module,
            RetainedIntentMutation::Method,
            RetainedIntentMutation::Arguments,
            RetainedIntentMutation::ArgumentRecipient,
            RetainedIntentMutation::ArgumentAmount,
            RetainedIntentMutation::ArgumentUnknownField,
            RetainedIntentMutation::Contract,
            RetainedIntentMutation::Status,
            RetainedIntentMutation::Identifier,
            RetainedIntentMutation::ExistingSignature,
        ] {
            let mut fixture = OwnedConfirmedFixture::new();
            mutate_retained_intent(&mut fixture, mutation);
            assert!(fixture.sign(&NoopTransactionSigningObserver).is_err());
        }
    }

    #[test]
    fn signature_length_and_verification_faults_fail_closed() {
        let fixture = OwnedConfirmedFixture::new();
        let payload = canonical_unsigned_payload(&fixture.transaction).unwrap();
        let identity = derive_account_identity(&fixture.seed);
        let mut signature_hex = fixture.seed.with_exposed(|seed_bytes| {
            let signing_key = SigningKey::from_bytes(seed_bytes);
            hex::encode(signing_key.sign(&payload).to_bytes())
        });
        let mut short_signature = signature_hex.clone();
        short_signature.truncate(126);
        assert_eq!(
            verify_signature(
                &short_signature,
                &payload,
                &identity.public_key,
                &NoopTransactionSigningObserver,
            ),
            Err(WalletTransactionError::SignatureUnavailable)
        );
        let replacement = if signature_hex.starts_with("00") {
            "ff"
        } else {
            "00"
        };
        signature_hex.replace_range(0..2, replacement);
        assert_eq!(
            verify_signature(
                &signature_hex,
                &payload,
                &identity.public_key,
                &NoopTransactionSigningObserver,
            ),
            Err(WalletTransactionError::SignatureVerificationFailed)
        );
    }
}
