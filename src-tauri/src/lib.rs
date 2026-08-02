pub mod api;
pub mod commands;
pub mod config;
pub mod core_manifest;
pub mod network;
pub mod paths;
pub mod reports;
pub mod supervisor;
pub mod wallet;

#[cfg(windows)]
mod single_instance;

#[cfg(windows)]
use std::sync::Arc;
use supervisor::SupervisorState;
use tauri::Manager;

pub fn run() {
    let builder = tauri::Builder::default();

    // This must remain the first plugin so duplicate processes are rejected before any future
    // wallet- or dialog-capable plugin can initialize.
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(
        |app, _duplicate_arguments, _duplicate_working_directory| {
            single_instance::activate_main_window(app);
        },
    ));

    // Native recovery selection is invoked only by private Rust adapters. The main-window
    // capability grants no dialog plugin command to the WebView.
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    builder
        .manage(SupervisorState::default())
        .setup(|app| {
            #[cfg(windows)]
            {
                let wallet_runtime =
                    Arc::new(wallet::WalletRuntimeState::initialize().map_err(|_| {
                        std::io::Error::other("secure wallet runtime is unavailable")
                    })?);
                let wallet_lifecycle = wallet::WindowsWalletLifecycle::register(Arc::clone(
                    &wallet_runtime,
                ))
                .map_err(|_| {
                    std::io::Error::other("secure wallet lifecycle monitoring is unavailable")
                })?;
                let wallet_adapters =
                    wallet::WalletLifecycleAdapters::initialize(Arc::clone(&wallet_runtime))
                        .map_err(|_| {
                            std::io::Error::other(
                                "secure wallet lifecycle adapters are unavailable",
                            )
                        })?;
                if !app.manage(wallet_runtime) {
                    return Err(std::io::Error::other(
                        "secure wallet runtime state already exists",
                    )
                    .into());
                }
                if !app.manage(wallet_lifecycle) {
                    return Err(std::io::Error::other(
                        "secure wallet lifecycle monitoring already exists",
                    )
                    .into());
                }
                if !app.manage(wallet_adapters) {
                    return Err(std::io::Error::other(
                        "secure wallet lifecycle adapters already exist",
                    )
                    .into());
                }
            }
            let resource_dir = app.path().resource_dir()?;
            core_manifest::initialize_resource_root(resource_dir).map_err(std::io::Error::other)?;
            Ok(())
        })
        .on_page_load(|webview, _payload| {
            #[cfg(windows)]
            if webview.label() == "main" {
                if let Some(runtime) = webview.try_state::<Arc<wallet::WalletRuntimeState>>() {
                    let _ = runtime.invalidate_all();
                }
            }
        })
        .on_window_event(|window, event| {
            #[cfg(windows)]
            if window.label() == "main"
                && matches!(
                    event,
                    tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
                )
            {
                if let Some(runtime) = window.try_state::<Arc<wallet::WalletRuntimeState>>() {
                    let _ = runtime.invalidate_all();
                }
            }
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
