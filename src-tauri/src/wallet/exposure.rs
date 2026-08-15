//! Atomic production Tauri surface for the reviewed Wallet boundaries.
//!
//! This module is intentionally the only place where wallet commands receive Tauri attributes.
//! Every wrapper forwards the complete invoke message to the existing Rust authority boundary;
//! wrappers perform no request parsing, custody work, signing, submission, or response projection.

use super::{
    lifecycle::WalletLifecycleAdapters,
    lifecycle_command_boundary::{
        WalletInvokeRequest, WalletLifecycleCommandBoundary, WalletTransactionCommandBoundary,
    },
    runtime::WalletRuntimeState,
};
use crate::supervisor::SupervisorState;
use std::sync::{mpsc, Arc};
use tauri::{
    ipc::{InvokeError, Response},
    State, WebviewWindow,
};

/// One managed production wallet surface shared by all twelve wrappers.
///
/// The constituent boundaries own the same runtime, lifecycle adapters, supervisor, and exact
/// main-window identity. This type deliberately implements no cloning, formatting, or Serde traits.
pub(crate) struct WalletCommandState {
    lifecycle: Arc<WalletLifecycleCommandBoundary>,
    transaction: WalletTransactionCommandBoundary,
}

impl WalletCommandState {
    pub(crate) fn initialize(
        runtime: Arc<WalletRuntimeState>,
        adapters: Arc<WalletLifecycleAdapters>,
        supervisor: Arc<SupervisorState>,
        main_hwnd: isize,
    ) -> Self {
        Self {
            lifecycle: Arc::new(WalletLifecycleCommandBoundary::new_private(
                Arc::clone(&runtime),
                Arc::clone(&adapters),
                main_hwnd,
            )),
            transaction: WalletTransactionCommandBoundary::new_private(
                runtime, adapters, supervisor, main_hwnd,
            ),
        }
    }
}

fn execute_lifecycle(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    state.lifecycle.execute(request, &window)
}

fn execute_transaction(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    state.transaction.execute(request, &window)
}

async fn execute_recovery_selection(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    state
        .lifecycle
        .begin_recovery_selection(request, &window, move |result| {
            // A receiver can disappear only when the invoking WebView is gone. The reviewed
            // callback boundary has already revalidated window/runtime authority and the normal
            // page/window lifecycle synchronously revokes the runtime. No result is retained.
            let _ = sender.send(result);
        })?;

    match tauri::async_runtime::spawn_blocking(move || receiver.recv()).await {
        Ok(Ok(result)) => result,
        // Losing the internal completion channel means the wrapper cannot prove a fail-closed
        // response. Termination is safer than manufacturing a response outside the reviewed guard.
        Ok(Err(_)) | Err(_) => std::process::abort(),
    }
}

#[tauri::command]
pub(crate) fn wallet_get_status(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_lifecycle(request, window, state)
}

#[tauri::command]
pub(crate) async fn wallet_select_recovery_destination(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_recovery_selection(request, window, state).await
}

#[tauri::command]
pub(crate) fn wallet_create(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_lifecycle(request, window, state)
}

#[tauri::command]
pub(crate) async fn wallet_select_recovery_source(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_recovery_selection(request, window, state).await
}

#[tauri::command]
pub(crate) fn wallet_restore(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_lifecycle(request, window, state)
}

#[tauri::command]
pub(crate) fn wallet_unlock(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_lifecycle(request, window, state)
}

#[tauri::command]
pub(crate) fn wallet_lock(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_lifecycle(request, window, state)
}

#[tauri::command]
pub(crate) fn wallet_prepare_transfer_preview(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_transaction(request, window, state)
}

#[tauri::command]
pub(crate) fn wallet_cancel_transfer_preview(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_transaction(request, window, state)
}

#[tauri::command]
pub(crate) fn wallet_confirm_and_submit_transfer(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_transaction(request, window, state)
}

#[tauri::command]
pub(crate) fn wallet_list_activity(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_transaction(request, window, state)
}

#[tauri::command]
pub(crate) fn wallet_refresh_transaction_observation(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, WalletCommandState>,
) -> Result<Response, InvokeError> {
    execute_transaction(request, window, state)
}
