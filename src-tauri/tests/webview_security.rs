use std::{fs, path::Path};

use serde_json::Value;

const PRODUCTION_CSP: &str = "default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' asset:; style-src 'self' 'unsafe-inline'";
const DEVELOPMENT_CSP: &str = "default-src 'self'; connect-src 'self' ipc: http://ipc.localhost http://127.0.0.1:* ws://localhost:1420 ws://127.0.0.1:1420; img-src 'self' asset:; style-src 'self' 'unsafe-inline'";

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read_source(path: &Path, relative: &str) -> String {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("failed to read {relative}: {error}"));
    if bytes.starts_with(&[0xff, 0xfe]) {
        assert_eq!(bytes[2..].len() % 2, 0, "truncated UTF-16LE {relative}");
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16(&units)
            .unwrap_or_else(|error| panic!("failed to decode UTF-16LE {relative}: {error}"))
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        assert_eq!(bytes[2..].len() % 2, 0, "truncated UTF-16BE {relative}");
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16(&units)
            .unwrap_or_else(|error| panic!("failed to decode UTF-16BE {relative}: {error}"))
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn collect_frontend_sources(directory: &Path, sources: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(directory).expect("frontend source directory is readable") {
        let entry = entry.expect("frontend source entry is readable");
        let path = entry.path();
        if path.is_dir() {
            collect_frontend_sources(&path, sources);
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("ts" | "tsx")
        ) {
            let relative = path
                .strip_prefix(manifest_dir().parent().expect("Tauri crate has a parent"))
                .expect("frontend source is inside the repository")
                .to_string_lossy()
                .replace('\\', "/");
            let source = read_source(&path, &relative);
            sources.push((relative, source));
        }
    }
}

#[test]
fn production_webview_connectivity_is_ipc_only() {
    let config: Value = serde_json::from_str(
        &fs::read_to_string(manifest_dir().join("tauri.conf.json"))
            .expect("Tauri config is readable"),
    )
    .expect("Tauri config is valid JSON");
    let security = &config["app"]["security"];

    assert_eq!(security["csp"], PRODUCTION_CSP);
    assert_eq!(security["devCsp"], DEVELOPMENT_CSP);

    let production = security["csp"]
        .as_str()
        .expect("production CSP is a string");
    assert!(!production.contains("127.0.0.1"));
    assert!(!production.contains("localhost:1420"));
    assert!(!production.contains("ws:"));
    assert!(!production.contains("wss:"));

    let development = security["devCsp"]
        .as_str()
        .expect("development CSP is a string");
    assert!(development.contains("http://127.0.0.1:*"));
    assert!(development.contains("ws://localhost:1420"));
    assert!(development.contains("ws://127.0.0.1:1420"));
}

#[test]
fn frontend_network_access_remains_centralized_through_tauri_ipc() {
    let frontend_root = manifest_dir()
        .parent()
        .expect("Tauri crate has a parent")
        .join("src");
    let mut sources = Vec::new();
    collect_frontend_sources(&frontend_root, &mut sources);

    let forbidden_network_apis = [
        "fetch(",
        "fetch (",
        "XMLHttpRequest",
        "new WebSocket",
        "new EventSource",
        "navigator.sendBeacon",
        "__TAURI__",
        "http://127.0.0.1",
        "http://localhost",
    ];
    for (path, source) in &sources {
        for forbidden in forbidden_network_apis {
            assert!(
                !source.contains(forbidden),
                "frontend source {path} contains forbidden direct network access: {forbidden}"
            );
        }
    }

    let tauri_api_imports: Vec<&str> = sources
        .iter()
        .filter_map(|(path, source)| source.contains("@tauri-apps/api").then_some(path.as_str()))
        .collect();
    assert_eq!(tauri_api_imports, ["src/services/coreApi.ts"]);
}
