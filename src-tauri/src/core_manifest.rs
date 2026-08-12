use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const EXPECTED_CORE_SHA256: &str =
    "41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01";
pub const CORE_MANIFEST_RELATIVE: &str = "bundled/core/windows-x64/manifest.json";
pub const CORE_BINARY_RELATIVE: &str = "bundled/core/windows-x64/vision-core.exe";

static RESOURCE_ROOT: OnceCell<PathBuf> = OnceCell::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoreManifest {
    pub core_tag: String,
    pub consensus_tag: String,
    pub source_commit: String,
    pub binary_sha256: String,
    pub consensus_version: u64,
    pub p2p_protocol_version: u64,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoreVerification {
    pub binary_path: PathBuf,
    pub expected_sha256: String,
    pub actual_sha256: String,
    pub matches: bool,
}

#[derive(Deserialize)]
struct WalletManifestEnvelope {
    wallet_core_api: Option<WalletCoreApiManifest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WalletCoreApiManifest {
    contract: String,
    bind_host: String,
    peer_binding: String,
    fee_policy: WalletFeePolicyManifest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WalletFeePolicyManifest {
    tip_raw: u128,
    charged_base_raw: u128,
    fee_limit_raw: u128,
}

pub(crate) struct WalletCoreCompatibility {
    manifest_sha256: [u8; 32],
}

pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn initialize_resource_root(root: PathBuf) -> Result<(), String> {
    RESOURCE_ROOT
        .set(root)
        .map_err(|_| "Desktop resource directory was already initialized".to_string())
}

fn resource_root() -> PathBuf {
    RESOURCE_ROOT.get().cloned().unwrap_or_else(repository_root)
}

fn bundled_path(root: &Path, relative: &str) -> PathBuf {
    root.join(relative)
}

pub fn bundled_core_binary_path() -> PathBuf {
    bundled_path(&resource_root(), CORE_BINARY_RELATIVE)
}

pub fn bundled_core_manifest_path() -> PathBuf {
    bundled_path(&resource_root(), CORE_MANIFEST_RELATIVE)
}

pub fn load_core_manifest_from(path: &Path) -> Result<CoreManifest, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read core manifest: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid core manifest json: {e}"))
}

pub fn load_core_manifest() -> Result<CoreManifest, String> {
    load_core_manifest_from(&bundled_core_manifest_path())
}

fn parse_wallet_core_compatibility(bytes: &[u8]) -> Result<WalletCoreCompatibility, ()> {
    let envelope: WalletManifestEnvelope = serde_json::from_slice(bytes).map_err(|_| ())?;
    let contract = envelope.wallet_core_api.ok_or(())?;
    if contract.contract != "vision-wallet-read-v1"
        || contract.bind_host != "127.0.0.1"
        || contract.peer_binding != "windows_tcp_owner_pid_v1"
        || contract.fee_policy.tip_raw != 0
        || contract.fee_policy.charged_base_raw != 1
        || contract.fee_policy.fee_limit_raw != 201
    {
        return Err(());
    }

    let digest = Sha256::digest(bytes);
    let mut manifest_sha256 = [0_u8; 32];
    manifest_sha256.copy_from_slice(&digest);
    Ok(WalletCoreCompatibility { manifest_sha256 })
}

pub(crate) fn load_wallet_core_compatibility() -> Result<WalletCoreCompatibility, ()> {
    let bytes = fs::read(bundled_core_manifest_path()).map_err(|_| ())?;
    parse_wallet_core_compatibility(&bytes)
}

impl WalletCoreCompatibility {
    pub(crate) fn manifest_sha256(&self) -> [u8; 32] {
        self.manifest_sha256
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("failed to open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|e| format!("failed to hash {}: {e}", path.display()))?;
    Ok(hex::encode_upper(hasher.finalize()))
}

pub fn verify_core_binary_at(
    path: &Path,
    manifest: &CoreManifest,
) -> Result<CoreVerification, String> {
    let actual = sha256_file(path)?;
    let expected = manifest.binary_sha256.to_uppercase();
    Ok(CoreVerification {
        binary_path: path.to_path_buf(),
        expected_sha256: expected.clone(),
        actual_sha256: actual.clone(),
        matches: actual == expected && actual == EXPECTED_CORE_SHA256,
    })
}

pub fn verify_bundled_core_binary() -> Result<CoreVerification, String> {
    let manifest = load_core_manifest()?;
    verify_core_binary_at(&bundled_core_binary_path(), &manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn manifest_parses() {
        let manifest = load_core_manifest().expect("manifest");
        assert_eq!(manifest.core_tag, "vision-core-alpha-rc2");
        assert_eq!(manifest.consensus_version, 3);
        assert_eq!(manifest.p2p_protocol_version, 4);
    }

    #[test]
    fn hash_verification_detects_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("vision-core.exe");
        std::fs::File::create(&file)
            .unwrap()
            .write_all(b"not core")
            .unwrap();
        let manifest = load_core_manifest().unwrap();
        let result = verify_core_binary_at(&file, &manifest).unwrap();
        assert!(!result.matches);
    }

    #[test]
    fn bundled_paths_are_rooted_under_runtime_resource_directory() {
        let root = Path::new("C:/Program Files/Vision Desktop");
        assert_eq!(
            bundled_path(root, CORE_BINARY_RELATIVE),
            root.join("bundled/core/windows-x64/vision-core.exe")
        );
        assert_eq!(
            bundled_path(root, CORE_MANIFEST_RELATIVE),
            root.join("bundled/core/windows-x64/manifest.json")
        );
    }

    #[test]
    fn current_manifest_does_not_enable_wallet_core_authority() {
        assert!(load_wallet_core_compatibility().is_err());
    }

    #[test]
    fn wallet_core_contract_requires_exact_peer_and_fee_policy() {
        let valid = br#"{
            "wallet_core_api": {
                "contract": "vision-wallet-read-v1",
                "bind_host": "127.0.0.1",
                "peer_binding": "windows_tcp_owner_pid_v1",
                "fee_policy": {
                    "tip_raw": 0,
                    "charged_base_raw": 1,
                    "fee_limit_raw": 201
                }
            }
        }"#;
        assert!(parse_wallet_core_compatibility(valid).is_ok());

        for invalid in [
            String::from_utf8(valid.to_vec())
                .unwrap()
                .replace("127.0.0.1", "localhost"),
            String::from_utf8(valid.to_vec())
                .unwrap()
                .replace("windows_tcp_owner_pid_v1", "pid_only"),
            String::from_utf8(valid.to_vec())
                .unwrap()
                .replace("\"fee_limit_raw\": 201", "\"fee_limit_raw\": 202"),
        ] {
            assert!(parse_wallet_core_compatibility(invalid.as_bytes()).is_err());
        }
    }
}
