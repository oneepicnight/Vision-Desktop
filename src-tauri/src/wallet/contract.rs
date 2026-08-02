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

/// Public account details known by the current Rust wallet runtime.
///
/// The label and backup state are optional because the encrypted vault deliberately
/// persists neither value. After an application restart they remain unknown until a
/// reviewed metadata store exists; the runtime never reconstructs or guesses them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletAccountSummary {
    pub wallet_id: String,
    pub label: Option<String>,
    pub public_key: String,
    pub address: String,
    pub created_at_unix_ms: u64,
    pub backup_verified: Option<bool>,
}

/// Secret-free lifecycle status suitable for a future reviewed command boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletLifecycleStatus {
    pub vault_exists: bool,
    pub locked: bool,
    pub account: Option<WalletAccountSummary>,
}

impl From<WalletPublicMetadata> for WalletAccountSummary {
    fn from(metadata: WalletPublicMetadata) -> Self {
        Self {
            wallet_id: metadata.wallet_id,
            label: Some(metadata.label),
            public_key: metadata.public_key,
            address: metadata.address,
            created_at_unix_ms: metadata.created_at_unix_ms,
            backup_verified: Some(metadata.backup_verified),
        }
    }
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
    fn lifecycle_status_serialization_is_public_only_and_allows_unknown_metadata() {
        let status = WalletLifecycleStatus {
            vault_exists: true,
            locked: true,
            account: Some(WalletAccountSummary {
                wallet_id: "wallet-1".to_string(),
                label: None,
                public_key: "11".repeat(32),
                address: "22".repeat(32),
                created_at_unix_ms: 1_700_000_000_000,
                backup_verified: None,
            }),
        };
        let serialized = serde_json::to_string(&status).unwrap();
        for forbidden in ["seed", "mnemonic", "password", "private_key", "secret_key"] {
            assert!(!serialized.contains(forbidden));
        }
        assert!(serialized.contains("\"label\":null"));
        assert!(serialized.contains("\"backup_verified\":null"));
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
