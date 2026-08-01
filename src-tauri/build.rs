fn main() {
    const APPLICATION_COMMANDS: &[&str] = &[
        "verify_core_binary",
        "get_core_manifest",
        "start_core",
        "stop_core",
        "restart_core",
        "get_core_process_state",
        "get_core_stdout_tail",
        "get_core_stderr_tail",
        "open_logs_directory",
        "open_data_directory",
        "get_dashboard_snapshot",
        "lookup_explorer_address",
        "lookup_explorer_transaction",
        "save_node_config",
        "get_node_config_snapshot",
        "generate_support_package",
        "run_network_diagnostics",
        "get_mock_dashboard_snapshot",
        "get_default_paths",
    ];

    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(APPLICATION_COMMANDS));

    tauri_build::try_build(attributes).expect("failed to build Vision Desktop Tauri context");
}
