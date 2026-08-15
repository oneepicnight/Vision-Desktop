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
        "wallet_get_status",
        "wallet_select_recovery_destination",
        "wallet_create",
        "wallet_select_recovery_source",
        "wallet_restore",
        "wallet_unlock",
        "wallet_lock",
        "wallet_prepare_transfer_preview",
        "wallet_cancel_transfer_preview",
        "wallet_confirm_and_submit_transfer",
        "wallet_list_activity",
        "wallet_refresh_transaction_observation",
    ];

    let layer_a = std::env::var_os("CARGO_FEATURE_WALLET_LAYER_A_QUALIFICATION").is_some();
    let layer_b = std::env::var_os("CARGO_FEATURE_WALLET_LAYER_B_QUALIFICATION").is_some();

    #[cfg(target_os = "windows")]
    if layer_a {
        // Generated-wrapper tests link Tauri's dialog path into a Rust test executable. Compile
        // exactly one Common Controls v6 resource specifically for test targets; injecting linker
        // manifest switches alongside Tauri's resource produces a duplicate resource (CVT1100).
        embed_resource::compile_for_everything(
            "windows-test-common-controls.rc",
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("failed to compile the Layer A Windows test manifest");
    }

    let app_manifest = tauri_build::AppManifest::new().commands(APPLICATION_COMMANDS);
    let app_manifest = if layer_b {
        app_manifest.permissions_path_pattern("qualification/wallet-layer-b/permissions/*.toml")
    } else {
        app_manifest
    };
    let attributes = tauri_build::Attributes::new().app_manifest(app_manifest);
    #[cfg(target_os = "windows")]
    let attributes = if layer_a {
        // The Layer A resource above supplies the same Common Controls v6 manifest to every
        // qualification target. Keep Tauri's icon/version resource but omit its second manifest.
        attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        attributes
    };

    tauri_build::try_build(attributes).expect("failed to build Vision Desktop Tauri context");
}
