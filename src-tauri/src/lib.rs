pub mod api;
pub mod commands;
pub mod config;
pub mod core_manifest;
pub mod network;
pub mod paths;
pub mod reports;
pub mod supervisor;
pub mod wallet;

use supervisor::SupervisorState;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .manage(SupervisorState::default())
        .setup(|app| {
            let resource_dir = app.path().resource_dir()?;
            core_manifest::initialize_resource_root(resource_dir).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::verify_core_binary,
            commands::get_core_manifest,
            commands::start_core,
            commands::stop_core,
            commands::restart_core,
            commands::get_core_process_state,
            commands::get_core_stdout_tail,
            commands::get_core_stderr_tail,
            commands::open_logs_directory,
            commands::open_data_directory,
            commands::get_dashboard_snapshot,
            commands::lookup_explorer_address,
            commands::lookup_explorer_transaction,
            commands::save_node_config,
            commands::get_node_config_snapshot,
            commands::generate_support_package,
            commands::run_network_diagnostics,
            commands::get_mock_dashboard_snapshot,
            commands::get_default_paths,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Vision Desktop");
}
