use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use time::OffsetDateTime;
use zip::{write::SimpleFileOptions, ZipWriter};

use crate::{
    config::NodeConfig,
    core_manifest::{bundled_core_binary_path, load_core_manifest, sha256_file, CoreManifest},
    paths::{default_paths, ensure_dir},
};

const SUPPORT_PACKAGE_SCHEMA: &str = "1.1";
const SECURITY_CLASSIFICATION_ERROR: &str =
    "support package content failed security classification";
const LOG_OMISSION_NOTICE: &str =
    "Untrusted process log content is excluded from Vision Desktop support packages.\n";
const COLLECTION_OMISSION_REASON: &str = "omitted_by_support_package_privacy_boundary";
const MAX_SUPPORT_FILE_BYTES: usize = 512 * 1024;
const EXPECTED_SUPPORT_FILES: [&str; 10] = [
    "SUMMARY.md",
    "binary-hash.txt",
    "config-redacted.json",
    "file-manifest-sha256.txt",
    "package-version.json",
    "peer-summary.json",
    "status-samples.jsonl",
    "stderr.log",
    "stdout.log",
    "summary.json",
];
const FORBIDDEN_CONTENT_MARKERS: [&str; 24] = [
    "wallet",
    "vault",
    "private_key",
    "private key",
    "seed_phrase",
    "seed phrase",
    "mnemonic",
    "password",
    "recovery_credential",
    "recovery credential",
    "recovery_secret",
    "recovery secret",
    "vision-recovery-v1",
    ".vision-recovery",
    "device_key",
    "device key",
    "dpapi",
    "session_token",
    "session token",
    "activation_proof",
    "activation proof",
    "ciphertext",
    "miner_reward_address",
    "sender_address",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupportPackageResult {
    pub report_dir: PathBuf,
    pub zip_path: PathBuf,
    pub zip_sha256: String,
    pub assessment: String,
}

#[derive(Debug, Clone)]
struct SupportFile {
    name: &'static str,
    bytes: Vec<u8>,
}

fn timestamp(now: OffsetDateTime) -> String {
    now.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

fn redacted_config_summary(config: Option<&NodeConfig>) -> serde_json::Value {
    match config {
        Some(config) => serde_json::json!({
            "available": true,
            "mode": config.mode,
            "api_port": config.api_port,
            "p2p_port": config.p2p_port,
            "configured_peer_count": config.seed_peers.len(),
            "advertised_endpoint_configured": config.advertised_host.is_some()
                || config.advertised_port.is_some(),
            "mining_enabled": config.mining_enabled,
            "mining_payout_configured": !config.miner_reward_address.trim().is_empty(),
            "data_directory_configured": !config.data_dir.as_os_str().is_empty(),
            "log_directory_configured": !config.log_dir.as_os_str().is_empty(),
        }),
        None => serde_json::json!({ "available": false }),
    }
}

fn omitted_collection() -> serde_json::Value {
    serde_json::json!({
        "collection": "omitted",
        "reason": COLLECTION_OMISSION_REASON,
    })
}

fn json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(value).map_err(|_| SECURITY_CLASSIFICATION_ERROR.to_string())
}

fn manifest_json_bytes(manifest: &CoreManifest) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(manifest).map_err(|_| SECURITY_CLASSIFICATION_ERROR.to_string())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode_upper(Sha256::digest(bytes))
}

fn build_support_files(
    run_id: &str,
    now: OffsetDateTime,
    manifest: &CoreManifest,
    binary_sha: &str,
    config: Option<&NodeConfig>,
) -> Result<Vec<SupportFile>, String> {
    let generated_at = timestamp(now);
    let summary = serde_json::json!({
        "schema_version": SUPPORT_PACKAGE_SCHEMA,
        "run_id": run_id,
        "package_tag": manifest.core_tag,
        "source_commit": manifest.source_commit,
        "binary_sha256": manifest.binary_sha256,
        "assessment": "INCOMPLETE",
        "warnings": [
            "Desktop-generated local support package; cross-node review required for network conclusions.",
            "Untrusted process logs and private operational data are excluded."
        ],
        "generated_at_utc": generated_at,
    });
    let omitted = omitted_collection();
    let mut files = vec![
        SupportFile {
            name: "package-version.json",
            bytes: manifest_json_bytes(manifest)?,
        },
        SupportFile {
            name: "binary-hash.txt",
            bytes: binary_sha.as_bytes().to_vec(),
        },
        SupportFile {
            name: "status-samples.jsonl",
            bytes: json_bytes(&omitted)?,
        },
        SupportFile {
            name: "peer-summary.json",
            bytes: json_bytes(&omitted)?,
        },
        SupportFile {
            name: "config-redacted.json",
            bytes: json_bytes(&redacted_config_summary(config))?,
        },
        SupportFile {
            name: "stdout.log",
            bytes: LOG_OMISSION_NOTICE.as_bytes().to_vec(),
        },
        SupportFile {
            name: "stderr.log",
            bytes: LOG_OMISSION_NOTICE.as_bytes().to_vec(),
        },
        SupportFile {
            name: "summary.json",
            bytes: json_bytes(&summary)?,
        },
        SupportFile {
            name: "SUMMARY.md",
            bytes: format!(
                "# Vision Desktop Support Package\n\nGenerated: {generated_at}\nAssessment: INCOMPLETE\n\nThis package uses an exact content allowlist. Untrusted process logs, private operational data, and security-sensitive material are excluded.\n"
            )
            .into_bytes(),
        },
    ];
    let manifest_lines = files
        .iter()
        .map(|file| format!("{}  {}", sha256_bytes(&file.bytes), file.name))
        .collect::<Vec<_>>()
        .join("\n");
    files.push(SupportFile {
        name: "file-manifest-sha256.txt",
        bytes: manifest_lines.into_bytes(),
    });
    validate_support_files(&files)?;
    Ok(files)
}

fn validate_support_files(files: &[SupportFile]) -> Result<(), String> {
    let names = files.iter().map(|file| file.name).collect::<BTreeSet<_>>();
    let expected = EXPECTED_SUPPORT_FILES.into_iter().collect::<BTreeSet<_>>();
    if files.len() != EXPECTED_SUPPORT_FILES.len() || names != expected {
        return Err(SECURITY_CLASSIFICATION_ERROR.to_string());
    }

    for file in files {
        if file.bytes.len() > MAX_SUPPORT_FILE_BYTES {
            return Err(SECURITY_CLASSIFICATION_ERROR.to_string());
        }
        let text = std::str::from_utf8(&file.bytes)
            .map_err(|_| SECURITY_CLASSIFICATION_ERROR.to_string())?;
        let lower = text.to_ascii_lowercase();
        if FORBIDDEN_CONTENT_MARKERS
            .iter()
            .any(|marker| lower.contains(marker))
        {
            return Err(SECURITY_CLASSIFICATION_ERROR.to_string());
        }
        if file.name.ends_with(".json") || file.name.ends_with(".jsonl") {
            serde_json::from_str::<serde_json::Value>(text)
                .map_err(|_| SECURITY_CLASSIFICATION_ERROR.to_string())?;
        }
    }

    let binary_hash = files
        .iter()
        .find(|file| file.name == "binary-hash.txt")
        .ok_or_else(|| SECURITY_CLASSIFICATION_ERROR.to_string())?;
    if binary_hash.bytes.len() != 64
        || !binary_hash
            .bytes
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_lowercase())
    {
        return Err(SECURITY_CLASSIFICATION_ERROR.to_string());
    }
    for log_name in ["stdout.log", "stderr.log"] {
        let log = files
            .iter()
            .find(|file| file.name == log_name)
            .ok_or_else(|| SECURITY_CLASSIFICATION_ERROR.to_string())?;
        if log.bytes != LOG_OMISSION_NOTICE.as_bytes() {
            return Err(SECURITY_CLASSIFICATION_ERROR.to_string());
        }
    }
    Ok(())
}

fn write_support_package_at(
    reports_dir: &Path,
    manifest: &CoreManifest,
    binary_sha: &str,
    config: Option<&NodeConfig>,
    now: OffsetDateTime,
) -> Result<SupportPackageResult, String> {
    let run_id = format!("vision-desktop-report-{}", now.unix_timestamp_nanos());
    let files = build_support_files(&run_id, now, manifest, binary_sha, config)?;
    fs::create_dir_all(reports_dir).map_err(|e| e.to_string())?;
    let report_dir = reports_dir.join(&run_id);
    fs::create_dir(&report_dir).map_err(|e| e.to_string())?;
    for support_file in &files {
        fs::write(report_dir.join(support_file.name), &support_file.bytes)
            .map_err(|e| e.to_string())?;
    }

    let zip_path = reports_dir.join(format!("{run_id}.zip"));
    let file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for support_file in &files {
        zip.start_file(support_file.name, options)
            .map_err(|e| e.to_string())?;
        zip.write_all(&support_file.bytes)
            .map_err(|e| e.to_string())?;
    }
    let zip_file = zip.finish().map_err(|e| e.to_string())?;
    zip_file.sync_all().map_err(|e| e.to_string())?;
    let zip_sha256 = sha256_file(&zip_path)?;
    Ok(SupportPackageResult {
        report_dir,
        zip_path,
        zip_sha256,
        assessment: "INCOMPLETE".to_string(),
    })
}

pub fn generate_support_package(
    config: Option<NodeConfig>,
) -> Result<SupportPackageResult, String> {
    let paths = default_paths();
    ensure_dir(&paths.reports)?;
    let manifest = load_core_manifest()?;
    let binary_sha = sha256_file(&bundled_core_binary_path())?;
    write_support_package_at(
        &paths.reports,
        &manifest,
        &binary_sha,
        config.as_ref(),
        OffsetDateTime::now_utc(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    const SECRET_CANARIES: [&str; 6] = [
        "CANARY-PRIVATE-KEY-0942",
        "CANARY-RECOVERY-CREDENTIAL-1837",
        "CANARY-DPAPI-BLOB-2711",
        "CANARY-SESSION-TOKEN-3919",
        "CANARY-ACTIVATION-PROOF-4823",
        "CANARY-ACTIVITY-JOURNAL-5741",
    ];

    fn manifest() -> CoreManifest {
        CoreManifest {
            core_tag: "vision-core-alpha-rc2".to_string(),
            consensus_tag: "vision-consensus-rc2".to_string(),
            source_commit: "0123456789abcdef".to_string(),
            binary_sha256: "A".repeat(64),
            consensus_version: 3,
            p2p_protocol_version: 4,
            platform: "windows-x64".to_string(),
        }
    }

    fn canary_config() -> NodeConfig {
        NodeConfig {
            node_name: SECRET_CANARIES[0].to_string(),
            seed_peers: vec![SECRET_CANARIES[1].to_string()],
            advertised_host: Some(SECRET_CANARIES[2].to_string()),
            miner_reward_address: SECRET_CANARIES[3].to_string(),
            data_dir: PathBuf::from(SECRET_CANARIES[4]),
            log_dir: PathBuf::from(SECRET_CANARIES[5]),
            ..NodeConfig::default()
        }
    }

    fn fixed_time() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn assert_no_canaries(bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        for canary in SECRET_CANARIES {
            assert!(!text.contains(canary));
        }
    }

    #[test]
    fn configuration_summary_exposes_only_non_identifying_shape() {
        let summary = redacted_config_summary(Some(&canary_config()));
        let serialized = serde_json::to_vec(&summary).unwrap();

        assert_no_canaries(&serialized);
        assert_eq!(summary["configured_peer_count"], 1);
        assert_eq!(summary["advertised_endpoint_configured"], true);
        assert_eq!(summary["mining_payout_configured"], true);
        assert!(summary.get("node_name").is_none());
        assert!(summary.get("seed_peers").is_none());
        assert!(summary.get("advertised_host").is_none());
        assert!(summary.get("miner_reward_address").is_none());
        assert!(summary.get("data_dir").is_none());
        assert!(summary.get("log_dir").is_none());
    }

    #[test]
    fn package_builder_uses_an_exact_allowlist_and_omits_logs() {
        let files = build_support_files(
            "vision-desktop-report-test",
            fixed_time(),
            &manifest(),
            &"B".repeat(64),
            Some(&canary_config()),
        )
        .unwrap();
        let names = files.iter().map(|file| file.name).collect::<BTreeSet<_>>();

        assert_eq!(
            names,
            EXPECTED_SUPPORT_FILES.into_iter().collect::<BTreeSet<_>>()
        );
        for file in &files {
            assert_no_canaries(&file.bytes);
        }
        for log_name in ["stdout.log", "stderr.log"] {
            assert_eq!(
                files
                    .iter()
                    .find(|file| file.name == log_name)
                    .unwrap()
                    .bytes,
                LOG_OMISSION_NOTICE.as_bytes()
            );
        }
    }

    #[test]
    fn classification_rejects_a_secret_canary_in_every_included_file() {
        let files = build_support_files(
            "vision-desktop-report-test",
            fixed_time(),
            &manifest(),
            &"B".repeat(64),
            None,
        )
        .unwrap();

        for index in 0..files.len() {
            let mut contaminated = files.clone();
            contaminated[index]
                .bytes
                .extend_from_slice(b"\nrecovery_credential=CANARY");
            assert_eq!(
                validate_support_files(&contaminated),
                Err(SECURITY_CLASSIFICATION_ERROR.to_string()),
                "{} accepted a secret canary",
                contaminated[index].name
            );
        }
    }

    #[test]
    fn classification_rejects_unexpected_or_duplicate_files() {
        let files = build_support_files(
            "vision-desktop-report-test",
            fixed_time(),
            &manifest(),
            &"B".repeat(64),
            None,
        )
        .unwrap();
        let mut unexpected = files.clone();
        unexpected.push(SupportFile {
            name: "unexpected.log",
            bytes: b"unexpected".to_vec(),
        });
        assert_eq!(
            validate_support_files(&unexpected),
            Err(SECURITY_CLASSIFICATION_ERROR.to_string())
        );
        let mut duplicate = files;
        duplicate[0].name = duplicate[1].name;
        assert_eq!(
            validate_support_files(&duplicate),
            Err(SECURITY_CLASSIFICATION_ERROR.to_string())
        );
    }

    #[test]
    fn written_directory_and_zip_contain_only_classified_canary_free_files() {
        let directory = tempfile::tempdir().unwrap();
        let result = write_support_package_at(
            directory.path(),
            &manifest(),
            &"B".repeat(64),
            Some(&canary_config()),
            fixed_time(),
        )
        .unwrap();

        let disk_names = fs::read_dir(&result.report_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            disk_names,
            EXPECTED_SUPPORT_FILES
                .into_iter()
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        );
        for name in &disk_names {
            assert_no_canaries(&fs::read(result.report_dir.join(name)).unwrap());
        }

        let zip_file = fs::File::open(&result.zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(zip_file).unwrap();
        assert_eq!(archive.len(), EXPECTED_SUPPORT_FILES.len());
        let mut zip_names = BTreeSet::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            zip_names.insert(file.name().to_string());
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            assert_no_canaries(&bytes);
        }
        assert_eq!(
            zip_names,
            EXPECTED_SUPPORT_FILES
                .into_iter()
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        );
    }
}
