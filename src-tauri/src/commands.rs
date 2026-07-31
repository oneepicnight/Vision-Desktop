use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    api::{
        fetch_dashboard, fetch_explorer_address, fetch_explorer_transaction, mock_dashboard,
        DashboardSnapshot, ExplorerAddressResult, ExplorerTransactionResult,
    },
    config::{
        load_node_config_snapshot, load_or_create_default_config,
        save_node_config as persist_node_config, NodeConfig, NodeConfigSnapshot,
    },
    core_manifest::{
        load_core_manifest, verify_bundled_core_binary, CoreManifest, CoreVerification,
    },
    network::{diagnose, NetworkDiagnostics},
    paths::{default_paths, ensure_dir},
    reports::{generate_support_package as build_support_package, SupportPackageResult},
    supervisor::{
        dir_size, process_resources, tail_file, CoreProcessState, StartCoreRequest, SupervisorState,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveNodeConfigRequest {
    pub config: NodeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDiagnosticsRequest {
    pub seed: Option<String>,
    pub api_bind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerQueryRequest {
    pub query: String,
}

#[tauri::command]
pub fn verify_core_binary() -> Result<CoreVerification, String> {
    let verification = verify_bundled_core_binary()?;
    if !verification.matches {
        return Err(format!(
            "Core binary hash mismatch: {}",
            verification.actual_sha256
        ));
    }
    Ok(verification)
}

#[tauri::command]
pub fn get_core_manifest() -> Result<CoreManifest, String> {
    load_core_manifest()
}

#[tauri::command]
pub fn start_core(
    state: State<SupervisorState>,
    request: Option<StartCoreRequest>,
) -> Result<CoreProcessState, String> {
    state.start(request.unwrap_or(StartCoreRequest { config: None }))
}

#[tauri::command]
pub fn stop_core(state: State<SupervisorState>) -> Result<CoreProcessState, String> {
    state.stop()
}

#[tauri::command]
pub fn restart_core(
    state: State<SupervisorState>,
    request: Option<StartCoreRequest>,
) -> Result<CoreProcessState, String> {
    state.restart(request.unwrap_or(StartCoreRequest { config: None }))
}

#[tauri::command]
pub fn get_core_process_state(state: State<SupervisorState>) -> Result<CoreProcessState, String> {
    state.current_state()
}

#[tauri::command]
pub fn get_core_stdout_tail(state: State<SupervisorState>) -> Result<String, String> {
    let (_, _, stdout, _) = state.log_paths()?;
    tail_file(&stdout, 32 * 1024)
}

#[tauri::command]
pub fn get_core_stderr_tail(state: State<SupervisorState>) -> Result<String, String> {
    let (_, _, _, stderr) = state.log_paths()?;
    tail_file(&stderr, 32 * 1024)
}

#[tauri::command]
pub fn open_logs_directory(state: State<SupervisorState>) -> Result<(), String> {
    let (_, logs, _, _) = state.log_paths()?;
    ensure_dir(&logs)?;
    opener::open(logs).map_err(|e| format!("failed to open logs directory: {e}"))
}

#[tauri::command]
pub fn open_data_directory(state: State<SupervisorState>) -> Result<(), String> {
    let (data, _, _, _) = state.log_paths()?;
    ensure_dir(&data)?;
    opener::open(data).map_err(|e| format!("failed to open data directory: {e}"))
}

#[tauri::command]
pub fn get_dashboard_snapshot(state: State<SupervisorState>) -> Result<DashboardSnapshot, String> {
    let process = state.current_state()?;
    let data_size = dir_size(&process.data_dir);
    let log_size = dir_size(&process.log_dir);
    let mut snapshot = if let Some(api_port) = process.api_port {
        fetch_dashboard(api_port, process.state.clone(), data_size, log_size)
    } else {
        DashboardSnapshot {
            process_state: process.state.clone(),
            status: None,
            mining: None,
            peers: Vec::new(),
            api_error: Some("Core is not running".to_string()),
            core_cpu: None,
            core_memory_bytes: None,
            data_dir_size_bytes: data_size,
            log_dir_size_bytes: log_size,
            mock_mode: false,
        }
    };
    if let Some(pid) = process.pid {
        let (cpu, memory) = process_resources(pid);
        snapshot.core_cpu = cpu;
        snapshot.core_memory_bytes = memory;
    }
    Ok(snapshot)
}

#[tauri::command]
pub fn get_mock_dashboard_snapshot() -> DashboardSnapshot {
    mock_dashboard()
}

#[tauri::command]
pub fn lookup_explorer_address(
    state: State<SupervisorState>,
    request: ExplorerQueryRequest,
) -> Result<ExplorerAddressResult, String> {
    let process = state.current_state()?;
    let Some(api_port) = process.api_port else {
        return Err("Core is not running".to_string());
    };
    let query = request.query.trim();
    if query.is_empty() {
        return Err("address lookup requires a non-empty query".to_string());
    }
    fetch_explorer_address(api_port, query)
}

#[tauri::command]
pub fn lookup_explorer_transaction(
    state: State<SupervisorState>,
    request: ExplorerQueryRequest,
) -> Result<ExplorerTransactionResult, String> {
    let process = state.current_state()?;
    let Some(api_port) = process.api_port else {
        return Err("Core is not running".to_string());
    };
    let query = request.query.trim();
    if query.is_empty() {
        return Err("transaction lookup requires a non-empty query".to_string());
    }
    fetch_explorer_transaction(api_port, query)
}

#[tauri::command]
pub fn save_node_config(request: SaveNodeConfigRequest) -> Result<NodeConfig, String> {
    persist_node_config(&request.config)?;
    Ok(request.config)
}

#[tauri::command]
pub fn get_node_config_snapshot() -> Result<NodeConfigSnapshot, String> {
    load_node_config_snapshot()
}

#[tauri::command]
pub fn generate_support_package_command(
    state: State<SupervisorState>,
) -> Result<SupportPackageResult, String> {
    let process = state.current_state()?;
    let stdout = tail_file(&process.stdout_log, 128 * 1024).unwrap_or_default();
    let stderr = tail_file(&process.stderr_log, 128 * 1024).unwrap_or_default();
    let config = load_or_create_default_config()
        .ok()
        .and_then(|c| serde_json::to_string_pretty(&c).ok());
    build_support_package(None, None, config, stdout, stderr)
}

#[tauri::command]
pub fn generate_support_package(
    state: State<SupervisorState>,
) -> Result<SupportPackageResult, String> {
    generate_support_package_command(state)
}

#[tauri::command]
pub fn run_network_diagnostics(request: NetworkDiagnosticsRequest) -> NetworkDiagnostics {
    diagnose(
        request.seed,
        request
            .api_bind
            .unwrap_or_else(|| "127.0.0.1:0".to_string()),
    )
}

#[tauri::command]
pub fn get_default_paths() -> crate::paths::AppPaths {
    default_paths()
}
