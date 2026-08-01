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
}
