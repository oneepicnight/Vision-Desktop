use tauri::{AppHandle, Manager, Runtime};

const MAIN_WINDOW_LABEL: &str = "main";

/// Best-effort activation of the only wallet-capable application window.
///
/// Duplicate-launch arguments and working directories are intentionally not accepted here. They
/// are untrusted process input and must never influence navigation, wallet operations, or logs.
pub(crate) fn activate_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    // A focus failure must not crash the primary process. The plugin still terminates the
    // duplicate, preserving the single-process security boundary.
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}
