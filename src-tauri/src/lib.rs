pub mod api;
pub mod commands;
pub mod config;
#[cfg(windows)]
mod core_job;
pub mod core_manifest;
#[cfg(windows)]
mod core_resource;
pub mod network;
pub mod paths;
pub mod reports;
pub mod supervisor;
pub mod wallet;

#[cfg(windows)]
mod single_instance;

use std::sync::Arc;
use supervisor::SupervisorState;
use tauri::Manager;

pub fn run() {
    // Rust invokes its panic hook before any `catch_unwind` boundary. Install the non-emitting
    // policy before builder/plugin setup can initialize private wallet state.
    wallet::install_production_panic_policy();
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

    let builder = builder
        .manage(Arc::new(SupervisorState::default()))
        .setup(|app| {
            let resource_dir = app.path().resource_dir()?;
            core_manifest::initialize_resource_root(resource_dir).map_err(std::io::Error::other)?;
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
                let wallet_local_data = app.path().local_data_dir().map_err(|_| {
                    std::io::Error::other("secure wallet storage location is unavailable")
                })?;
                let main_window = app.get_webview_window("main").ok_or_else(|| {
                    std::io::Error::other("secure wallet owner window is unavailable")
                })?;
                let main_window_handle = main_window.hwnd().map_err(|_| {
                    std::io::Error::other("secure wallet owner window is unavailable")
                })?;
                let recovery_ceremony = Arc::new(
                    wallet::NativeRecoveryCredentialCeremony::new(main_window_handle.0 as isize)
                        .map_err(|_| {
                            std::io::Error::other("secure recovery acknowledgement is unavailable")
                        })?,
                );
                let secret_ceremony = Arc::new(
                    wallet::NativeWalletSecretCeremony::new(main_window_handle.0 as isize)
                        .map_err(|_| {
                            std::io::Error::other("secure wallet secret ceremony is unavailable")
                        })?,
                );
                let wallet_adapters = Arc::new(
                    wallet::WalletLifecycleAdapters::initialize(
                        Arc::clone(&wallet_runtime),
                        &wallet_local_data,
                        recovery_ceremony,
                        secret_ceremony,
                    )
                    .map_err(|_| {
                        std::io::Error::other("secure wallet lifecycle adapters are unavailable")
                    })?,
                );
                let supervisor = Arc::clone(app.state::<Arc<SupervisorState>>().inner());
                let wallet_commands = wallet::exposure::WalletCommandState::initialize(
                    Arc::clone(&wallet_runtime),
                    wallet_adapters,
                    supervisor,
                    main_window_handle.0 as isize,
                );
                if !app.manage(Arc::clone(&wallet_runtime)) {
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
                if !app.manage(wallet_commands) {
                    return Err(std::io::Error::other(
                        "secure wallet command state already exists",
                    )
                    .into());
                }
            }
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
                if let Some(supervisor) = window.try_state::<Arc<SupervisorState>>() {
                    let _ = supervisor.stop();
                }
            }
        });

    #[cfg(windows)]
    let builder = builder.invoke_handler(tauri::generate_handler![
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
        wallet::exposure::wallet_get_status,
        wallet::exposure::wallet_select_recovery_destination,
        wallet::exposure::wallet_create,
        wallet::exposure::wallet_select_recovery_source,
        wallet::exposure::wallet_restore,
        wallet::exposure::wallet_unlock,
        wallet::exposure::wallet_lock,
        wallet::exposure::wallet_prepare_transfer_preview,
        wallet::exposure::wallet_cancel_transfer_preview,
        wallet::exposure::wallet_confirm_and_submit_transfer,
        wallet::exposure::wallet_list_activity,
        wallet::exposure::wallet_refresh_transaction_observation,
    ]);

    #[cfg(not(windows))]
    let builder = builder.invoke_handler(tauri::generate_handler![
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
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("failed to run Vision Desktop");
}
