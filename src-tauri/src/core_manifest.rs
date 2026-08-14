use once_cell::sync::OnceCell;
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::os::windows::io::RawHandle;

#[cfg(windows)]
use crate::core_resource::{running_process_image_identity, CoreFileIdentity, GuardedCoreFile};

pub const EXPECTED_CORE_SHA256: &str =
    "8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28";
pub const EXPECTED_CORE_SIZE_BYTES: u64 = 4_486_144;
pub const EXPECTED_RUNTIME_MANIFEST_SHA256: &str =
    "cf713d116acca7a848d0537968f81373ee14a965fb3855a6480a59f4971536ec";
pub const ACCEPTED_EVIDENCE_MANIFEST_SHA256: &str =
    "35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a";
pub const EXPECTED_INTEGRATION_ACCEPTANCE_SHA256: &str =
    "2576e87f46dd7cd878a5aa39daebc11e027d23bbeeeca54e5db6110cec9e3449";
pub const CORE_MANIFEST_RELATIVE: &str = "bundled/core/windows-x64/manifest.json";
pub const CORE_BINARY_RELATIVE: &str = "bundled/core/windows-x64/vision-core.exe";
pub const CORE_INTEGRATION_ACCEPTANCE_RELATIVE: &str =
    "bundled/core/windows-x64/integration-acceptance.json";

const MAX_RUNTIME_MANIFEST_BYTES: usize = 16 * 1024;
const MAX_INTEGRATION_ACCEPTANCE_BYTES: usize = 4 * 1024;
const EXPECTED_SCHEMA_VERSION: u32 = 1;
const EXPECTED_SOURCE_COMMIT: &str = "890c98a02c7147e166805fe52002d22d1fcd81f9";
const EXPECTED_SOURCE_TREE: &str = "2ae583bbfc887490b8af1398aead7b916796700c";

static RESOURCE_ROOT: OnceCell<PathBuf> = OnceCell::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CoreManifest {
    pub schema_version: u32,
    pub core_tag: String,
    pub release_version: String,
    pub consensus_tag: String,
    pub source_commit: String,
    pub source_tree: String,
    pub binary_sha256: String,
    pub binary_size_bytes: u64,
    pub platform: String,
    pub consensus_version: u64,
    pub p2p_protocol_version: u64,
    pub accepted_evidence_manifest_sha256: String,
    pub api: CoreApiManifest,
    pub wallet_core_api: WalletCoreApiManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CoreApiManifest {
    pub bind_host: String,
    pub bind_policy: String,
    pub http_port_environment: String,
    pub peer_binding: String,
    pub status_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletCoreApiManifest {
    pub contract: String,
    pub identifier_format: String,
    pub routes: WalletRoutesManifest,
    pub fee_policy: WalletFeePolicyManifest,
    pub submission_semantics: WalletSubmissionSemanticsManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletRoutesManifest {
    pub balance: String,
    pub nonce: String,
    pub status: String,
    pub transaction_lookup: String,
    pub submission: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletFeePolicyManifest {
    pub tip_raw: u128,
    pub charged_base_raw: u128,
    pub fee_limit_raw: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletSubmissionSemanticsManifest {
    pub definitive_non_mutating_rejections: Vec<String>,
    pub ambiguous_duplicate_codes: Vec<String>,
    pub automatic_retry: bool,
    pub replacement_transactions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CoreIntegrationAcceptance {
    schema_version: u32,
    record_type: String,
    decision_date: String,
    vision_desktop_integration_authorized: bool,
    authorized_scope: String,
    authorization_basis: String,
    authorizing_principal: String,
    authorization_reference: String,
    source_commit: String,
    source_tree: String,
    candidate_sha256: String,
    candidate_size_bytes: u64,
    evidence_manifest_sha256: String,
    runtime_manifest_sha256: String,
    accepted_evidence_limitations: Vec<String>,
    conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoreVerification {
    pub binary_path: PathBuf,
    pub expected_sha256: String,
    pub actual_sha256: String,
    pub matches: bool,
}

#[cfg(windows)]
pub(crate) struct VerifiedCoreResources {
    acceptance_file: GuardedCoreFile,
    manifest_file: GuardedCoreFile,
    executable_file: GuardedCoreFile,
    acceptance: CoreIntegrationAcceptance,
    acceptance_size_bytes: u64,
    manifest: CoreManifest,
    manifest_sha256: [u8; 32],
    manifest_size_bytes: u64,
}

pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Tauri crate must have a repository parent")
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

pub fn bundled_core_integration_acceptance_path() -> PathBuf {
    bundled_path(&resource_root(), CORE_INTEGRATION_ACCEPTANCE_RELATIVE)
}

#[cfg(windows)]
fn load_guarded_acceptance_from(
    path: &Path,
) -> Result<(GuardedCoreFile, CoreIntegrationAcceptance), String> {
    let mut file = GuardedCoreFile::open(path)
        .map_err(|_| "Core integration acceptance is unavailable".to_string())?;
    let bytes = file
        .read_bounded(MAX_INTEGRATION_ACCEPTANCE_BYTES)
        .map_err(|_| "Core integration acceptance is unavailable".to_string())?;
    let acceptance = parse_pinned_integration_acceptance(&bytes)?;
    let size = u64::try_from(bytes.len())
        .map_err(|_| "Core integration acceptance is unavailable".to_string())?;
    file.revalidate(size, EXPECTED_INTEGRATION_ACCEPTANCE_SHA256)
        .map_err(|_| "Core integration acceptance identity changed".to_string())?;
    Ok((file, acceptance))
}

#[cfg(windows)]
fn load_guarded_manifest_from(
    path: &Path,
) -> Result<(GuardedCoreFile, CoreManifest, [u8; 32]), String> {
    let mut file = GuardedCoreFile::open(path)
        .map_err(|_| "Core runtime manifest is unavailable".to_string())?;
    let bytes = file
        .read_bounded(MAX_RUNTIME_MANIFEST_BYTES)
        .map_err(|_| "Core runtime manifest is unavailable".to_string())?;
    let manifest = parse_pinned_core_manifest(&bytes)?;
    let size = u64::try_from(bytes.len())
        .map_err(|_| "Core runtime manifest is unavailable".to_string())?;
    file.revalidate(size, EXPECTED_RUNTIME_MANIFEST_SHA256)
        .map_err(|_| "Core runtime manifest identity changed".to_string())?;
    Ok((file, manifest, digest_array(&bytes)))
}

pub fn load_core_manifest_from(path: &Path) -> Result<CoreManifest, String> {
    #[cfg(windows)]
    {
        load_guarded_manifest_from(path).map(|(_, manifest, _)| manifest)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Core artifact admission requires Windows".to_string())
    }
}

pub fn load_core_manifest() -> Result<CoreManifest, String> {
    load_core_manifest_from(&bundled_core_manifest_path())
}

#[cfg(windows)]
pub(crate) fn admit_bundled_core_resources() -> Result<VerifiedCoreResources, String> {
    let (acceptance_file, acceptance) =
        load_guarded_acceptance_from(&bundled_core_integration_acceptance_path())?;
    let acceptance_size_bytes = acceptance_file.size_bytes();
    let (manifest_file, manifest, manifest_sha256) =
        load_guarded_manifest_from(&bundled_core_manifest_path())?;
    let manifest_size_bytes = manifest_file.size_bytes();
    let mut executable_file = GuardedCoreFile::open(&bundled_core_binary_path())
        .map_err(|_| "Frozen Core executable is unavailable".to_string())?;
    executable_file
        .revalidate(manifest.binary_size_bytes, &manifest.binary_sha256)
        .map_err(|_| "Frozen Core executable identity did not match".to_string())?;
    Ok(VerifiedCoreResources {
        acceptance_file,
        manifest_file,
        executable_file,
        acceptance,
        acceptance_size_bytes,
        manifest,
        manifest_sha256,
        manifest_size_bytes,
    })
}

#[cfg(windows)]
impl VerifiedCoreResources {
    pub(crate) fn manifest(&self) -> &CoreManifest {
        &self.manifest
    }

    pub(crate) fn executable_path(&self) -> &Path {
        self.executable_file.path()
    }

    pub(crate) fn manifest_sha256(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub(crate) fn executable_identity(&self) -> CoreFileIdentity {
        self.executable_file.identity()
    }

    pub(crate) fn manifest_identity(&self) -> CoreFileIdentity {
        self.manifest_file.identity()
    }

    pub(crate) fn acceptance_identity(&self) -> CoreFileIdentity {
        self.acceptance_file.identity()
    }

    pub(crate) fn verify_running_process_image(&self, raw: RawHandle) -> Result<(), String> {
        let running = running_process_image_identity(raw)
            .map_err(|_| "Running Core image identity is unavailable".to_string())?;
        if running != self.executable_file.identity() {
            return Err("Running Core image does not match the frozen executable".to_string());
        }
        Ok(())
    }

    pub(crate) fn revalidate(&mut self) -> Result<(), String> {
        self.acceptance_file
            .revalidate(
                self.acceptance_size_bytes,
                EXPECTED_INTEGRATION_ACCEPTANCE_SHA256,
            )
            .map_err(|_| "Core integration acceptance identity changed".to_string())?;
        let acceptance_bytes = self
            .acceptance_file
            .read_bounded(MAX_INTEGRATION_ACCEPTANCE_BYTES)
            .map_err(|_| "Core integration acceptance is unavailable".to_string())?;
        if parse_pinned_integration_acceptance(&acceptance_bytes)? != self.acceptance {
            return Err("Core integration acceptance identity changed".to_string());
        }
        self.manifest_file
            .revalidate(self.manifest_size_bytes, EXPECTED_RUNTIME_MANIFEST_SHA256)
            .map_err(|_| "Core runtime manifest identity changed".to_string())?;
        let bytes = self
            .manifest_file
            .read_bounded(MAX_RUNTIME_MANIFEST_BYTES)
            .map_err(|_| "Core runtime manifest is unavailable".to_string())?;
        let current = parse_pinned_core_manifest(&bytes)?;
        if current != self.manifest || digest_array(&bytes) != self.manifest_sha256 {
            return Err("Core runtime manifest identity changed".to_string());
        }
        self.executable_file
            .revalidate(
                self.manifest.binary_size_bytes,
                &self.manifest.binary_sha256,
            )
            .map_err(|_| "Frozen Core executable identity changed".to_string())?;
        Ok(())
    }
}

pub fn verify_core_binary_at(
    path: &Path,
    manifest: &CoreManifest,
) -> Result<CoreVerification, String> {
    #[cfg(windows)]
    {
        let mut file = GuardedCoreFile::open(path)
            .map_err(|_| "Frozen Core executable is unavailable".to_string())?;
        let actual = file
            .sha256_lower()
            .map_err(|_| "Frozen Core executable cannot be hashed".to_string())?;
        let expected = manifest.binary_sha256.to_lowercase();
        Ok(CoreVerification {
            binary_path: path.to_path_buf(),
            expected_sha256: expected.clone(),
            actual_sha256: actual.clone(),
            matches: actual == expected
                && actual == EXPECTED_CORE_SHA256
                && file.size_bytes() == manifest.binary_size_bytes
                && file.size_bytes() == EXPECTED_CORE_SIZE_BYTES,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (path, manifest);
        Err("Core artifact admission requires Windows".to_string())
    }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    #[cfg(windows)]
    {
        let mut file =
            GuardedCoreFile::open(path).map_err(|_| "Core resource is unavailable".to_string())?;
        file.sha256_lower()
            .map(|hash| hash.to_uppercase())
            .map_err(|_| "Core resource cannot be hashed".to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Core artifact admission requires Windows".to_string())
    }
}

pub fn verify_bundled_core_binary() -> Result<CoreVerification, String> {
    let manifest = load_core_manifest()?;
    verify_core_binary_at(&bundled_core_binary_path(), &manifest)
}

fn parse_pinned_core_manifest(bytes: &[u8]) -> Result<CoreManifest, String> {
    if hex::encode(Sha256::digest(bytes)) != EXPECTED_RUNTIME_MANIFEST_SHA256 {
        return Err("Core runtime manifest digest is not independently approved".to_string());
    }
    parse_core_manifest_bytes(bytes)
}

fn parse_pinned_integration_acceptance(bytes: &[u8]) -> Result<CoreIntegrationAcceptance, String> {
    if hex::encode(Sha256::digest(bytes)) != EXPECTED_INTEGRATION_ACCEPTANCE_SHA256 {
        return Err("Core integration acceptance is not independently pinned".to_string());
    }
    parse_integration_acceptance_bytes(bytes)
}

fn parse_integration_acceptance_bytes(bytes: &[u8]) -> Result<CoreIntegrationAcceptance, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let StrictJson(value) = StrictJson::deserialize(&mut deserializer)
        .map_err(|_| "Core integration acceptance JSON is invalid".to_string())?;
    deserializer
        .end()
        .map_err(|_| "Core integration acceptance JSON is invalid".to_string())?;
    let acceptance: CoreIntegrationAcceptance = serde_json::from_value(value)
        .map_err(|_| "Core integration acceptance schema is invalid".to_string())?;
    validate_integration_acceptance(&acceptance)?;
    Ok(acceptance)
}

fn validate_integration_acceptance(acceptance: &CoreIntegrationAcceptance) -> Result<(), String> {
    let expected_conditions = [
        "original_evidence_package_remains_unmodified",
        "candidate_bytes_must_not_change",
        "wallet_authority_remains_disabled",
        "independent_desktop_implementation_acceptance_required_before_activation",
    ];
    let expected_limitations = [
        "authenticated_ci_archive_covers_223e2f745ebb5f7eb0d48c88397684b9037767bc_not_the_exact_candidate",
        "exact_candidate_runtime_and_deterministic_compatibility_qualification_remain_required",
    ];
    if acceptance.schema_version != 1
        || acceptance.record_type != "vision-desktop-core-integration-acceptance-v1"
        || acceptance.decision_date != "2026-08-14"
        || !acceptance.vision_desktop_integration_authorized
        || acceptance.authorized_scope
            != "isolated_artifact_admission_and_controlled_compatibility_validation"
        || acceptance.authorization_basis != "explicit_vision_desktop_owner_authorization"
        || acceptance.authorizing_principal != "Vision Desktop owner"
        || acceptance.authorization_reference
            != "explicit integration authorization following frozen artifact acceptance"
        || acceptance.source_commit != EXPECTED_SOURCE_COMMIT
        || acceptance.source_tree != EXPECTED_SOURCE_TREE
        || acceptance.candidate_sha256 != EXPECTED_CORE_SHA256
        || acceptance.candidate_size_bytes != EXPECTED_CORE_SIZE_BYTES
        || acceptance.evidence_manifest_sha256 != ACCEPTED_EVIDENCE_MANIFEST_SHA256
        || acceptance.runtime_manifest_sha256 != EXPECTED_RUNTIME_MANIFEST_SHA256
        || acceptance.accepted_evidence_limitations != expected_limitations
        || acceptance.conditions != expected_conditions
    {
        return Err("Core integration acceptance does not authorize this identity".to_string());
    }
    Ok(())
}

fn parse_core_manifest_bytes(bytes: &[u8]) -> Result<CoreManifest, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let StrictJson(value) = StrictJson::deserialize(&mut deserializer)
        .map_err(|_| "Core runtime manifest JSON is invalid".to_string())?;
    deserializer
        .end()
        .map_err(|_| "Core runtime manifest JSON is invalid".to_string())?;
    let manifest: CoreManifest = serde_json::from_value(value)
        .map_err(|_| "Core runtime manifest schema is invalid".to_string())?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &CoreManifest) -> Result<(), String> {
    if manifest.schema_version != EXPECTED_SCHEMA_VERSION
        || manifest.core_tag != "vision-core-v1.0.4"
        || manifest.release_version != "1.0.4"
        || manifest.consensus_tag != "vision-core-consensus-v1.0.3"
        || manifest.source_commit != EXPECTED_SOURCE_COMMIT
        || manifest.source_tree != EXPECTED_SOURCE_TREE
        || manifest.binary_sha256 != EXPECTED_CORE_SHA256
        || manifest.binary_size_bytes != EXPECTED_CORE_SIZE_BYTES
        || manifest.platform != "windows-x86_64-msvc"
        || manifest.consensus_version != 3
        || manifest.p2p_protocol_version != 4
        || manifest.accepted_evidence_manifest_sha256 != ACCEPTED_EVIDENCE_MANIFEST_SHA256
        || manifest.api.bind_host != "127.0.0.1"
        || manifest.api.bind_policy != "compiled_literal_ipv4_loopback_v1"
        || manifest.api.http_port_environment != "VISION_HTTP_PORT"
        || manifest.api.peer_binding != "windows_tcp_owner_pid_v1"
        || manifest.api.status_version != "3"
    {
        return Err("Core runtime manifest is not an admitted compatibility contract".to_string());
    }
    validate_wallet_contract(manifest)
}

fn validate_wallet_contract(manifest: &CoreManifest) -> Result<(), String> {
    let wallet = &manifest.wallet_core_api;
    if wallet.contract != "vision-wallet-read-v1"
        || wallet.identifier_format != "lowercase_hex_64"
        || wallet.routes.balance != "/balance/{address}"
        || wallet.routes.nonce != "/nonce/{address}"
        || wallet.routes.status != "/status"
        || wallet.routes.transaction_lookup != "/transaction/{transaction_id}"
        || wallet.routes.submission != "/transactions"
        || wallet.fee_policy.tip_raw != 0
        || wallet.fee_policy.charged_base_raw != 1
        || wallet.fee_policy.fee_limit_raw != 201
        || !wallet
            .submission_semantics
            .definitive_non_mutating_rejections
            .is_empty()
        || wallet.submission_semantics.ambiguous_duplicate_codes
            != ["duplicate_canonical_tx_id", "duplicate_sender_nonce"]
        || wallet.submission_semantics.automatic_retry
        || wallet.submission_semantics.replacement_transactions
    {
        return Err("Core wallet compatibility contract is unsupported".to_string());
    }
    Ok(())
}

fn digest_array(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut result = [0_u8; 32];
    result.copy_from_slice(&digest);
    result
}

struct StrictJson(Value);

impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = StrictJson;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictJson(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictJson(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictJson(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .map(StrictJson)
            .ok_or_else(|| E::custom("invalid JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(value.to_string())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictJson(Value::String(value)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictJson(Value::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictJson(Value::Null))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        StrictJson::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(StrictJson(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(StrictJson(Value::Array(values)))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON object member"));
            }
            let StrictJson(value) = object.next_value()?;
            values.insert(key, value);
        }
        Ok(StrictJson(Value::Object(values)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn manifest_bytes() -> Vec<u8> {
        std::fs::read(bundled_core_manifest_path()).unwrap()
    }

    fn acceptance_bytes() -> Vec<u8> {
        std::fs::read(bundled_core_integration_acceptance_path()).unwrap()
    }

    #[test]
    fn separately_pinned_acceptance_authorizes_only_the_exact_frozen_identity() {
        let bytes = acceptance_bytes();
        let acceptance = parse_pinned_integration_acceptance(&bytes).unwrap();
        assert!(acceptance.vision_desktop_integration_authorized);
        assert_eq!(acceptance.source_commit, EXPECTED_SOURCE_COMMIT);
        assert_eq!(acceptance.source_tree, EXPECTED_SOURCE_TREE);
        assert_eq!(acceptance.candidate_sha256, EXPECTED_CORE_SHA256);
        assert_eq!(
            hex::encode(Sha256::digest(bytes)),
            EXPECTED_INTEGRATION_ACCEPTANCE_SHA256
        );
    }

    #[test]
    fn acceptance_rejects_duplicates_unknown_fields_and_valid_but_unapproved_bytes() {
        let text = String::from_utf8(acceptance_bytes()).unwrap();
        let duplicate = text.replacen(
            "{\n",
            "{\n  \"vision_desktop_integration_authorized\": true,\n",
            1,
        );
        assert!(parse_integration_acceptance_bytes(duplicate.as_bytes()).is_err());

        let mut unknown: Value = serde_json::from_str(&text).unwrap();
        unknown["reviewer"] = Value::String("unapproved".to_string());
        assert!(
            parse_integration_acceptance_bytes(&serde_json::to_vec(&unknown).unwrap()).is_err()
        );

        let acceptance = parse_integration_acceptance_bytes(text.as_bytes()).unwrap();
        let compact = serde_json::to_vec(&acceptance).unwrap();
        assert!(parse_integration_acceptance_bytes(&compact).is_ok());
        assert!(parse_pinned_integration_acceptance(&compact).is_err());
    }

    #[test]
    fn admitted_manifest_parses_with_exact_identity() {
        let bytes = manifest_bytes();
        let manifest = parse_pinned_core_manifest(&bytes).unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.source_commit, EXPECTED_SOURCE_COMMIT);
        assert_eq!(manifest.binary_sha256, EXPECTED_CORE_SHA256);
        assert_eq!(
            hex::encode(Sha256::digest(bytes)),
            EXPECTED_RUNTIME_MANIFEST_SHA256
        );
    }

    #[test]
    fn duplicate_root_and_nested_members_fail_before_typed_construction() {
        let text = String::from_utf8(manifest_bytes()).unwrap();
        let duplicate_root = text.replacen("{\n", "{\n  \"schema_version\": 1,\n", 1);
        assert!(parse_core_manifest_bytes(duplicate_root.as_bytes()).is_err());

        let duplicate_nested = text.replacen(
            "\"bind_host\": \"127.0.0.1\",",
            "\"bind_host\": \"127.0.0.1\",\n    \"bind_host\": \"127.0.0.1\",",
            1,
        );
        assert!(parse_core_manifest_bytes(duplicate_nested.as_bytes()).is_err());
    }

    #[test]
    fn unknown_root_and_nested_members_are_rejected() {
        let mut root: Value = serde_json::from_slice(&manifest_bytes()).unwrap();
        root.as_object_mut()
            .unwrap()
            .insert("channel".to_string(), Value::String("latest".to_string()));
        assert!(parse_core_manifest_bytes(&serde_json::to_vec(&root).unwrap()).is_err());

        let mut nested: Value = serde_json::from_slice(&manifest_bytes()).unwrap();
        nested["api"]["url"] = Value::String("http://localhost".to_string());
        assert!(parse_core_manifest_bytes(&serde_json::to_vec(&nested).unwrap()).is_err());
    }

    #[test]
    fn valid_but_unapproved_complete_manifest_bytes_fail_the_digest_pin() {
        let manifest = parse_core_manifest_bytes(&manifest_bytes()).unwrap();
        let compact = serde_json::to_vec(&manifest).unwrap();
        assert!(parse_core_manifest_bytes(&compact).is_ok());
        assert!(parse_pinned_core_manifest(&compact).is_err());
    }

    #[test]
    fn exact_schema_and_platform_values_are_required() {
        let bytes = manifest_bytes();
        for (field, value) in [
            ("schema_version", serde_json::json!(2)),
            ("platform", serde_json::json!("windows-x64")),
            ("source_commit", serde_json::json!("00".repeat(20))),
        ] {
            let mut document: Value = serde_json::from_slice(&bytes).unwrap();
            document[field] = value;
            assert!(parse_core_manifest_bytes(&serde_json::to_vec(&document).unwrap()).is_err());
        }
    }

    #[test]
    fn hash_verification_detects_mismatch() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("vision-core.exe");
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
        assert_eq!(
            bundled_path(root, CORE_INTEGRATION_ACCEPTANCE_RELATIVE),
            root.join("bundled/core/windows-x64/integration-acceptance.json")
        );
    }

    #[test]
    fn admitted_manifest_contains_the_exact_private_core_contract() {
        let manifest = load_core_manifest().unwrap();
        validate_wallet_contract(&manifest).unwrap();
    }

    #[test]
    fn admitted_resources_block_manifest_replacement_and_reject_wrong_process_images() {
        use std::fs::OpenOptions;
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let mut resources = admit_bundled_core_resources().unwrap();
        assert!(std::fs::rename(
            bundled_core_manifest_path(),
            bundled_core_manifest_path().with_extension("moved")
        )
        .is_err());
        assert!(std::fs::rename(
            bundled_core_integration_acceptance_path(),
            bundled_core_integration_acceptance_path().with_extension("moved")
        )
        .is_err());
        assert!(OpenOptions::new()
            .write(true)
            .open(bundled_core_manifest_path())
            .is_err());
        resources.revalidate().unwrap();
        assert!(resources
            .verify_running_process_image(unsafe { GetCurrentProcess() } as RawHandle)
            .is_err());
    }
}
