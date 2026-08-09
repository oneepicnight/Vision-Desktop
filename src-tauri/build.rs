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

    #[cfg(target_os = "windows")]
    if std::env::var_os("CARGO_FEATURE_WALLET_LAYER_A_QUALIFICATION").is_some() {
        let test_manifest =
            std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
                .join("windows-test-common-controls.manifest");
        println!("cargo:rerun-if-changed={}", test_manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            test_manifest.display()
        );
    }

    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(APPLICATION_COMMANDS));

    tauri_build::try_build(attributes).expect("failed to build Vision Desktop Tauri context");
}
