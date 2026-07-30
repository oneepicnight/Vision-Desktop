use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};
use time::OffsetDateTime;
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, ZipWriter};

use crate::{
    core_manifest::{bundled_core_binary_path, load_core_manifest, sha256_file},
    paths::{default_paths, ensure_dir},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupportPackageResult {
    pub report_dir: PathBuf,
    pub zip_path: PathBuf,
    pub zip_sha256: String,
    pub assessment: String,
}

fn timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn redact_config(input: &str) -> String {
    input
        .lines()
        .map(|line| {
            let lower = line.to_lowercase();
            if lower.contains("private")
                || lower.contains("seed_phrase")
                || lower.contains("secret")
                || lower.contains("password")
            {
                "<redacted>".to_string()
            } else if lower.trim_start().starts_with("p2p_advertised_host") {
                "p2p_advertised_host = \"<redacted-host>\"".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn generate_support_package(
    status_json: Option<String>,
    peer_json: Option<String>,
    config_json: Option<String>,
    stdout_tail: String,
    stderr_tail: String,
) -> Result<SupportPackageResult, String> {
    let paths = default_paths();
    ensure_dir(&paths.reports)?;
    let run_id = format!(
        "vision-desktop-report-{}",
        OffsetDateTime::now_utc().unix_timestamp()
    );
    let report_dir = paths.reports.join(&run_id);
    ensure_dir(&report_dir)?;
    let manifest = load_core_manifest()?;
    let binary_sha = sha256_file(&bundled_core_binary_path())?;
    fs::write(
        report_dir.join("package-version.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(report_dir.join("binary-hash.txt"), binary_sha).map_err(|e| e.to_string())?;
    fs::write(
        report_dir.join("status-samples.jsonl"),
        status_json.unwrap_or_else(|| "null".to_string()),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        report_dir.join("peer-summary.json"),
        peer_json.unwrap_or_else(|| "null".to_string()),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        report_dir.join("config-redacted.json"),
        redact_config(&config_json.unwrap_or_else(|| "{}".to_string())),
    )
    .map_err(|e| e.to_string())?;
    fs::write(report_dir.join("stdout.log"), stdout_tail).map_err(|e| e.to_string())?;
    fs::write(report_dir.join("stderr.log"), stderr_tail).map_err(|e| e.to_string())?;
    let summary = serde_json::json!({
        "schema_version": "1.0",
        "run_id": run_id,
        "package_tag": "vision-core-alpha-rc2",
        "source_commit": manifest.source_commit,
        "binary_sha256": manifest.binary_sha256,
        "assessment": "INCOMPLETE",
        "warnings": ["Desktop-generated local support package; cross-node review required for network conclusions."],
        "generated_at_utc": timestamp()
    });
    fs::write(
        report_dir.join("summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(report_dir.join("SUMMARY.md"), format!("# Vision Desktop Support Package\n\nGenerated: {}\nAssessment: INCOMPLETE\n\nThis package redacts secrets and public endpoint host values by default.\n", timestamp())).map_err(|e| e.to_string())?;

    let mut manifest_lines = Vec::new();
    for entry in WalkDir::new(&report_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if entry.file_name() == "file-manifest-sha256.txt" {
            continue;
        }
        let hash = sha256_file(entry.path())?;
        let rel = entry
            .path()
            .strip_prefix(&report_dir)
            .unwrap_or(entry.path())
            .to_string_lossy();
        manifest_lines.push(format!("{hash}  {rel}"));
    }
    fs::write(
        report_dir.join("file-manifest-sha256.txt"),
        manifest_lines.join("\n"),
    )
    .map_err(|e| e.to_string())?;

    let zip_path = paths.reports.join(format!("{run_id}.zip"));
    let file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for entry in WalkDir::new(&report_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let rel = entry
            .path()
            .strip_prefix(&report_dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(rel, options).map_err(|e| e.to_string())?;
        let bytes = fs::read(entry.path()).map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    let zip_sha256 = sha256_file(&zip_path)?;
    Ok(SupportPackageResult {
        report_dir,
        zip_path,
        zip_sha256,
        assessment: "INCOMPLETE".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_removes_secret_like_fields() {
        let redacted = redact_config("private_key = \"abc\"\np2p_advertised_host = \"1.2.3.4\"");
        assert!(redacted.contains("<redacted>"));
        assert!(redacted.contains("<redacted-host>"));
        assert!(!redacted.contains("1.2.3.4"));
    }
}
