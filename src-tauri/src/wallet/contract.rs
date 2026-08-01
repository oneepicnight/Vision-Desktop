use serde::{Deserialize, Serialize};

/// Public wallet information that may safely cross the Tauri boundary.
///
/// Secret material, password-derived keys, recovery phrases, and encrypted
/// vault payloads are intentionally absent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletPublicMetadata {
    pub wallet_id: String,
    pub label: String,
    pub public_key: String,
    pub address: String,
    pub created_at_unix_ms: u64,
    pub locked: bool,
    pub backup_verified: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalletContractRequirement {
    KeyDerivation,
    AddressEncoding,
    AmountDenomination,
    TransactionSerialization,
    SignatureVector,
    FeeAndNonceRules,
    SubmissionResponse,
    ReceiptAndHistory,
    PrivateLoopbackBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletCompatibilityGate {
    pub signing_enabled: bool,
    pub unmet_requirements: Vec<WalletContractRequirement>,
}

/// Returns the fail-closed compatibility state for the first custody slice.
///
/// Signing remains disabled until each item is backed by an approved Core
/// contract and deterministic cross-implementation test vectors.
pub fn wallet_contract_gate() -> WalletCompatibilityGate {
    WalletCompatibilityGate {
        signing_enabled: false,
        unmet_requirements: vec![WalletContractRequirement::PrivateLoopbackBinding],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_metadata_serialization_excludes_secret_fields() {
        let metadata = WalletPublicMetadata {
            wallet_id: "wallet-1".to_string(),
            label: "Primary".to_string(),
            public_key: "11".repeat(32),
            address: "22".repeat(32),
            created_at_unix_ms: 1_700_000_000_000,
            locked: true,
            backup_verified: true,
        };

        let serialized = serde_json::to_string(&metadata).unwrap();
        for forbidden in [
            "private_key",
            "secret_key",
            "seed",
            "mnemonic",
            "recovery_phrase",
            "password",
            "keystore",
        ] {
            assert!(!serialized.contains(forbidden));
        }
        assert!(serialized.contains("public_key"));
        assert!(serialized.contains("address"));
    }

    #[test]
    fn signing_is_fail_closed_until_every_contract_is_verified() {
        let gate = wallet_contract_gate();

        assert!(!gate.signing_enabled);
        assert_eq!(gate.unmet_requirements.len(), 1);
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::KeyDerivation));
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::AddressEncoding));
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::TransactionSerialization));
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::SignatureVector));
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::AmountDenomination));
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::FeeAndNonceRules));
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::SubmissionResponse));
        assert!(!gate
            .unmet_requirements
            .contains(&WalletContractRequirement::ReceiptAndHistory));
        assert!(gate
            .unmet_requirements
            .contains(&WalletContractRequirement::PrivateLoopbackBinding));
    }
}
