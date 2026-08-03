use super::secrets::WalletSeed;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

/// Public Vision account identity derived inside the Rust custody boundary.
///
/// Vision-Core RC2 identifies an account by the 64-character lowercase hex
/// encoding of its 32-byte Ed25519 public key. No secret material is retained
/// or exposed by this value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionAccountIdentity {
    pub public_key: String,
    pub address: String,
}

pub(in crate::wallet) fn derive_account_identity(seed: &WalletSeed) -> VisionAccountIdentity {
    let public_key_bytes = seed.with_exposed(|seed_bytes| {
        SigningKey::from_bytes(seed_bytes)
            .verifying_key()
            .to_bytes()
    });
    let public_key = hex::encode(public_key_bytes);

    VisionAccountIdentity {
        address: public_key.clone(),
        public_key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        recovery::PortableRecoveryArtifact,
        secrets::{WalletPassword, WalletSeed},
    };

    const CORE_RC2_SEED_07_PUBLIC_KEY: &str =
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";

    fn recovery_password() -> WalletPassword {
        WalletPassword::new("independent offline recovery password".to_string())
    }

    #[test]
    fn seed_to_public_key_and_address_matches_core_rc2_vector() {
        let identity = derive_account_identity(&WalletSeed::for_test(7));

        assert_eq!(identity.public_key, CORE_RC2_SEED_07_PUBLIC_KEY);
        assert_eq!(identity.address, CORE_RC2_SEED_07_PUBLIC_KEY);
    }

    #[test]
    fn portable_recovery_restores_the_identical_account_identity() {
        let seed = WalletSeed::for_test(7);
        let original = derive_account_identity(&seed);
        let artifact = PortableRecoveryArtifact::encrypt(
            "vector_wallet",
            1_700_000_000_000,
            &seed,
            &recovery_password(),
        )
        .unwrap();
        let encoded = artifact.to_json().unwrap();
        let restored_artifact = PortableRecoveryArtifact::from_json(&encoded).unwrap();
        let restored_seed = restored_artifact.restore(&recovery_password()).unwrap();
        let restored = derive_account_identity(&restored_seed);

        assert_eq!(restored, original);
        assert_eq!(restored.address, CORE_RC2_SEED_07_PUBLIC_KEY);
    }

    #[test]
    fn different_seeds_produce_different_account_identities() {
        let first = derive_account_identity(&WalletSeed::for_test(7));
        let second = derive_account_identity(&WalletSeed::for_test(8));

        assert_ne!(first, second);
    }

    #[test]
    fn serialized_public_identity_contains_no_secret_fields() {
        let identity = derive_account_identity(&WalletSeed::for_test(7));
        let serialized = serde_json::to_string(&identity).unwrap();

        assert!(serialized.contains(CORE_RC2_SEED_07_PUBLIC_KEY));
        for forbidden in [
            "private_key",
            "secret_key",
            "seed",
            "mnemonic",
            "password",
            "recovery",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }
}
