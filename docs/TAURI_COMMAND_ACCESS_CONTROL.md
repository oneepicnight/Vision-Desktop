# Tauri Command Access Control

## Active boundary

Vision Desktop registers its 19 current application commands with `tauri_build::AppManifest` and grants their generated allow permissions through one capability: `src-tauri/capabilities/main-desktop.json`.

That capability:

- applies only to the explicitly labelled `main` window;
- applies only to the admitted Windows target;
- contains no remote-origin grant;
- contains no wildcard, shell, filesystem, HTTP, clipboard, dialog, single-instance, or wallet permission;
- grants each current application command individually.

The frontend continues to call Tauri only through `src/services/coreApi.ts`. The ACL is an additional runtime boundary, not a replacement for Rust-side input validation or the service boundary.

## Allowed commands

The main window may invoke:

- Core verification and lifecycle: `verify_core_binary`, `get_core_manifest`, `start_core`, `stop_core`, `restart_core`, and `get_core_process_state`.
- Diagnostics and local operator paths: `get_core_stdout_tail`, `get_core_stderr_tail`, `open_logs_directory`, `open_data_directory`, `generate_support_package`, and `run_network_diagnostics`.
- Read-only data: `get_dashboard_snapshot`, `get_mock_dashboard_snapshot`, `lookup_explorer_address`, `lookup_explorer_transaction`, and `get_default_paths`.
- Desktop configuration: `save_node_config` and `get_node_config_snapshot`.

`generate_support_package_command` remains an internal Rust helper. Its former `#[tauri::command]` annotation was removed because it is not registered or intended to be callable from the WebView.

## Build and runtime enforcement

`src-tauri/build.rs` supplies the exact command inventory to `tauri_build::AppManifest`. This causes Tauri to generate one `allow-*` and one `deny-*` application permission for each listed command. `src-tauri/tauri.conf.json` explicitly selects only the `main-desktop` capability and explicitly labels the single application window `main`.

`src-tauri/tests/tauri_acl.rs` fails when:

- the `#[tauri::command]` functions, invoke handler, AppManifest, and capability diverge;
- the capability is expanded beyond the `main` Windows window;
- a remote-origin grant appears;
- a broad or namespaced plugin permission appears;
- the pinned dialog or single-instance plugin is initialized;
- a wallet permission is added.

Adding or removing an application command therefore requires one reviewed change across command registration, the AppManifest, the capability, and this documentation.

## Deliberately inactive plugins and wallet surface

`tauri-plugin-dialog` and `tauri-plugin-single-instance` remain exact-version Windows dependencies only. Neither plugin is initialized, and neither has a capability permission. There are no wallet Tauri commands or wallet permissions.

Plugin initialization and custody commands remain later security gates. They require their own implementation, lifecycle tests, and review; this migration does not activate them.

## WebView network boundary

The production CSP is now restricted to Tauri IPC. General loopback HTTP and the Vite hot-reload WebSocket are confined to `devCsp`, and automated tests prevent direct frontend network access or additional Tauri core imports. `docs/WEBVIEW_NETWORK_SECURITY.md` records this complementary boundary.
