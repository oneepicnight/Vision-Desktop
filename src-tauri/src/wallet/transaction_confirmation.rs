#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "native transaction confirmation remains private until its command boundary is approved"
    )
)]
#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "the native modal entry point remains unregistered while its state machine is tested"
    )
)]

use super::{
    amount::format_vision_amount,
    core_client::WalletCoreReadSource,
    preview::{
        PendingTransferConfirmation, TransferConfirmationFields, WalletPreviewError,
        WalletTransactionPreviewEngine,
    },
    runtime::WalletRuntimeState,
    signing::{sign_after_native_approval, WalletPrivateSigningError},
};
use crate::supervisor::SupervisorState;
use std::{
    ffi::c_void,
    mem::size_of,
    panic::{catch_unwind, AssertUnwindSafe},
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, DrawTextW, EndPaint, GetStockObject, GetSysColorBrush, SelectObject, SetBkMode,
        COLOR_WINDOW, DEFAULT_GUI_FONT, DT_CALCRECT, DT_LEFT, DT_SINGLELINE, DT_WORDBREAK,
        PAINTSTRUCT, TRANSPARENT,
    },
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetTickCount},
    UI::{
        Input::{
            GetCurrentInputMessageSource,
            Ime::{ImmAssociateContextEx, ImmGetContext, ImmReleaseContext},
            KeyboardAndMouse::{
                EnableWindow, GetFocus, IsWindowEnabled, SetFocus, VK_RETURN, VK_SPACE,
            },
            IMDT_KEYBOARD, IMDT_MOUSE, IMO_HARDWARE, INPUT_MESSAGE_SOURCE,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageTime,
            GetMessageW, GetWindowLongPtrW, GetWindowRect, IsDialogMessageW, IsWindow, KillTimer,
            LoadCursorW, PostQuitMessage, RegisterClassExW, SendMessageW, SetForegroundWindow,
            SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, UnregisterClassW, BM_CLICK,
            BM_SETSTYLE, BN_CLICKED, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, BS_TYPEMASK, CREATESTRUCTW,
            CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, GWL_STYLE, IDC_ARROW, MSG, SW_SHOW, WM_CHAR,
            WM_CLEAR, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_COPY, WM_CREATE, WM_CUT, WM_GETTEXT,
            WM_GETTEXTLENGTH, WM_IME_CHAR, WM_IME_COMPOSITION, WM_IME_COMPOSITIONFULL,
            WM_IME_CONTROL, WM_IME_ENDCOMPOSITION, WM_IME_KEYDOWN, WM_IME_KEYUP, WM_IME_NOTIFY,
            WM_IME_REQUEST, WM_IME_SELECT, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION,
            WM_INPUTLANGCHANGE, WM_INPUTLANGCHANGEREQUEST, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_PASTE, WM_SETTEXT, WM_TIMER,
            WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME, WS_POPUP, WS_SYSMENU,
            WS_TABSTOP, WS_VISIBLE,
        },
    },
};
use zeroize::{Zeroize, Zeroizing};

#[cfg(test)]
use windows_sys::Win32::{
    System::Threading::GetCurrentProcess,
    UI::HiDpi::{
        AreDpiAwarenessContextsEqual, GetAwarenessFromDpiAwarenessContext,
        GetDpiAwarenessContextForProcess, GetDpiForWindow, GetThreadDpiAwarenessContext,
        GetWindowDpiAwarenessContext, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_PER_MONITOR_AWARE,
    },
};

const DIALOG_WIDTH: i32 = 860;
const DIALOG_HEIGHT: i32 = 650;
const AUTHORITY_TIMER_ID: usize = 1;
const AUTHORITY_TIMER_MS: u32 = 250;
const CONFIRM_BUTTON_ID: usize = 2001;
const CANCEL_BUTTON_ID: usize = 2002;

static CLASS_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
static QUALIFICATION_CONFIRMATION_DPI: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

#[cfg(test)]
static QUALIFICATION_ACCEPTED_INPUT_DEVICE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

#[cfg(test)]
static QUALIFICATION_CONFIRM_FOCUS_VERIFIED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// A non-forgeable proof issued only after this module's native ceremony reports an explicit,
/// post-render hardware approval. Sibling wallet modules can name and consume this type, but its
/// private non-zero-sized field prevents them from constructing it in safe Rust.
pub(in crate::wallet) struct NativeConfirmationApproval {
    _proof: NativeConfirmationApprovalProof,
}

struct NativeConfirmationApprovalProof(u8);

impl NativeConfirmationApproval {
    fn issue() -> Self {
        Self {
            _proof: NativeConfirmationApprovalProof(0xA5),
        }
    }
}

impl Drop for NativeConfirmationApprovalProof {
    fn drop(&mut self) {
        self.0 = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletConfirmationError {
    Cancelled,
    AuthorityRevoked,
    NativeUiUnavailable,
    PreviewUnavailable,
    CoreUnavailable,
    OperationInProgress,
    RuntimeUnavailable,
    SigningUnavailable,
}

impl WalletConfirmationError {
    pub(in crate::wallet) const fn code(self) -> &'static str {
        match self {
            Self::Cancelled => "wallet_confirmation_cancelled",
            Self::AuthorityRevoked => "wallet_confirmation_authority_revoked",
            Self::NativeUiUnavailable => "wallet_confirmation_ui_unavailable",
            Self::PreviewUnavailable => "wallet_preview_unavailable",
            Self::CoreUnavailable => "wallet_core_unavailable",
            Self::OperationInProgress => "wallet_operation_in_progress",
            Self::RuntimeUnavailable => "wallet_runtime_unavailable",
            Self::SigningUnavailable => "wallet_signing_unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeConfirmationError {
    Cancelled,
    AuthorityRevoked,
    NativeUiUnavailable,
}

trait TransactionConfirmationCeremony: Send + Sync {
    fn present(
        &self,
        fields: TransferConfirmationFields<'_>,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<(), NativeConfirmationError>;
}

pub(in crate::wallet) struct NativeTransactionConfirmationCeremony {
    owner_window: isize,
}

impl NativeTransactionConfirmationCeremony {
    fn new(owner_window: isize) -> Result<Self, NativeConfirmationError> {
        if owner_window == 0 || unsafe { IsWindow(owner_window as HWND) } == 0 {
            return Err(NativeConfirmationError::NativeUiUnavailable);
        }
        Ok(Self { owner_window })
    }
}

impl TransactionConfirmationCeremony for NativeTransactionConfirmationCeremony {
    fn present(
        &self,
        fields: TransferConfirmationFields<'_>,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<(), NativeConfirmationError> {
        run_native_confirmation(self.owner_window as HWND, fields, authority_is_current)
    }
}

pub(in crate::wallet) struct WalletTransactionConfirmationEngine<'a> {
    runtime: &'a WalletRuntimeState,
    ceremony: &'a dyn TransactionConfirmationCeremony,
}

impl<'a> WalletTransactionConfirmationEngine<'a> {
    pub(in crate::wallet) fn new(
        runtime: &'a WalletRuntimeState,
        ceremony: &'a NativeTransactionConfirmationCeremony,
    ) -> Self {
        Self { runtime, ceremony }
    }

    pub(in crate::wallet) fn confirm(
        &self,
        supervisor: &'a SupervisorState,
        owner_window: &str,
        handle: &str,
    ) -> Result<(), WalletConfirmationError> {
        self.run_fail_closed(|| {
            let pending = WalletTransactionPreviewEngine::new(self.runtime)
                .consume(supervisor, owner_window, handle)
                .map_err(map_preview_error)?;
            self.confirm_pending_inner(pending)
        })
    }

    fn confirm_pending<S: WalletCoreReadSource>(
        &self,
        pending: PendingTransferConfirmation<'_, S>,
    ) -> Result<(), WalletConfirmationError> {
        self.run_fail_closed(|| self.confirm_pending_inner(pending))
    }

    fn confirm_pending_inner<S: WalletCoreReadSource>(
        &self,
        pending: PendingTransferConfirmation<'_, S>,
    ) -> Result<(), WalletConfirmationError> {
        let fields = pending.fields();
        self.ceremony
            .present(fields, &|| pending.authority_is_current())
            .map_err(map_native_error)?;
        sign_after_native_approval(pending, NativeConfirmationApproval::issue())
            .map_err(map_signing_error)
    }

    fn run_fail_closed<T>(
        &self,
        operation: impl FnOnce() -> Result<T, WalletConfirmationError>,
    ) -> Result<T, WalletConfirmationError> {
        match catch_unwind(AssertUnwindSafe(operation)) {
            Ok(result) => result,
            Err(_) => {
                invalidate_or_terminate(self.runtime);
                Err(WalletConfirmationError::RuntimeUnavailable)
            }
        }
    }
}

const fn map_signing_error(error: WalletPrivateSigningError) -> WalletConfirmationError {
    match error {
        WalletPrivateSigningError::PreviewUnavailable => {
            WalletConfirmationError::PreviewUnavailable
        }
        WalletPrivateSigningError::RuntimeRevoked => WalletConfirmationError::AuthorityRevoked,
        WalletPrivateSigningError::ActivationUnavailable
        | WalletPrivateSigningError::IntentRejected
        | WalletPrivateSigningError::SignatureUnavailable => {
            WalletConfirmationError::SigningUnavailable
        }
        WalletPrivateSigningError::CoreUnavailable => WalletConfirmationError::CoreUnavailable,
    }
}

fn map_preview_error(error: WalletPreviewError) -> WalletConfirmationError {
    match error {
        WalletPreviewError::OperationInProgress => WalletConfirmationError::OperationInProgress,
        WalletPreviewError::CompatibilityUnavailable | WalletPreviewError::CoreUnavailable => {
            WalletConfirmationError::CoreUnavailable
        }
        WalletPreviewError::RuntimeUnavailable => WalletConfirmationError::AuthorityRevoked,
        WalletPreviewError::InvalidRequest
        | WalletPreviewError::WalletUnavailable
        | WalletPreviewError::CoreRejected
        | WalletPreviewError::CoreRecovering
        | WalletPreviewError::AccountUnavailable
        | WalletPreviewError::InsufficientBalance
        | WalletPreviewError::ArithmeticRejected => WalletConfirmationError::PreviewUnavailable,
    }
}

const fn map_native_error(error: NativeConfirmationError) -> WalletConfirmationError {
    match error {
        NativeConfirmationError::Cancelled => WalletConfirmationError::Cancelled,
        NativeConfirmationError::AuthorityRevoked => WalletConfirmationError::AuthorityRevoked,
        NativeConfirmationError::NativeUiUnavailable => {
            WalletConfirmationError::NativeUiUnavailable
        }
    }
}

fn invalidate_or_terminate(runtime: &WalletRuntimeState) {
    match catch_unwind(AssertUnwindSafe(|| runtime.invalidate_all())) {
        Ok(Ok(())) => {}
        Ok(Err(_)) | Err(_) => std::process::abort(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmationOutcome {
    Pending,
    Confirmed,
    Cancelled,
    AuthorityRevoked,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmationInputDevice {
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmationPress {
    Keyboard(u16),
    Mouse,
}

struct ConfirmationDisplayBuffers {
    sender: Zeroizing<Vec<u16>>,
    recipient: Zeroizing<Vec<u16>>,
    amount: Zeroizing<Vec<u16>>,
    fee: Zeroizing<Vec<u16>>,
    total: Zeroizing<Vec<u16>>,
    nonce: Zeroizing<Vec<u16>>,
    transaction_id: Zeroizing<Vec<u16>>,
}

impl ConfirmationDisplayBuffers {
    fn new(fields: TransferConfirmationFields<'_>) -> Self {
        Self {
            sender: wide_sensitive(fields.sender_address),
            recipient: wide_sensitive(fields.recipient_address),
            amount: wide_sensitive(&format!(
                "{} VISION ({} raw units)",
                format_vision_amount(fields.amount_raw_units),
                fields.amount_raw_units
            )),
            fee: wide_sensitive(&format!(
                "Charged: {} VISION ({} raw)    Maximum: {} VISION ({} raw)",
                format_vision_amount(u128::from(fields.charged_fee_raw_units)),
                fields.charged_fee_raw_units,
                format_vision_amount(u128::from(fields.fee_limit_raw_units)),
                fields.fee_limit_raw_units
            )),
            total: wide_sensitive(&format!(
                "{} VISION ({} raw units)",
                format_vision_amount(fields.total_debit_raw_units),
                fields.total_debit_raw_units
            )),
            nonce: wide_sensitive(&fields.nonce.to_string()),
            transaction_id: wide_sensitive(fields.transaction_id),
        }
    }

    fn wipe(&mut self) {
        self.sender.zeroize();
        self.recipient.zeroize();
        self.amount.zeroize();
        self.fee.zeroize();
        self.total.zeroize();
        self.nonce.zeroize();
        self.transaction_id.zeroize();
    }

    #[cfg(test)]
    fn is_wiped(&self) -> bool {
        [
            self.sender.as_slice(),
            self.recipient.as_slice(),
            self.amount.as_slice(),
            self.fee.as_slice(),
            self.total.as_slice(),
            self.nonce.as_slice(),
            self.transaction_id.as_slice(),
        ]
        .into_iter()
        .all(|buffer| buffer.iter().all(|unit| *unit == 0))
    }
}

struct ConfirmationDialogState {
    display: ConfirmationDisplayBuffers,
    outcome: ConfirmationOutcome,
    confirm_button: HWND,
    cancel_button: HWND,
    display_verified: bool,
    display_verified_at: Option<u32>,
    press_started_after_display: Option<ConfirmationPress>,
    fresh_input_time: Option<u32>,
    fresh_input_device: Option<ConfirmationInputDevice>,
}

impl ConfirmationDialogState {
    fn wipe(&mut self) {
        if !self.confirm_button.is_null() && unsafe { IsWindow(self.confirm_button) } != 0 {
            unsafe { EnableWindow(self.confirm_button, 0) };
        }
        self.display_verified = false;
        self.display_verified_at = None;
        self.press_started_after_display = None;
        self.fresh_input_time = None;
        self.fresh_input_device = None;
        self.display.wipe();
    }
}

fn confirmation_input_contexts_are_absent(dialog: HWND, state: &ConfirmationDialogState) -> bool {
    !dialog.is_null()
        && !state.confirm_button.is_null()
        && !state.cancel_button.is_null()
        && input_context_is_absent(dialog)
        && input_context_is_absent(state.confirm_button)
        && input_context_is_absent(state.cancel_button)
}

#[cfg(test)]
fn is_production_dpi_context(
    context: windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT,
) -> bool {
    !context.is_null()
        && unsafe {
            AreDpiAwarenessContextsEqual(context, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
        } != 0
        && unsafe { GetAwarenessFromDpiAwarenessContext(context) }
            == DPI_AWARENESS_PER_MONITOR_AWARE
}

#[cfg(test)]
fn qualification_dpi_context_label(
    context: windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT,
) -> &'static str {
    if is_production_dpi_context(context) {
        "PerMonitorV2"
    } else {
        "unexpected"
    }
}

#[cfg(test)]
fn establish_production_dpi_context_for_qualification() {
    let cargo_lock = include_str!("../../Cargo.lock");
    let mut lines = cargo_lock.lines();
    let mut pinned_tao = false;
    while let Some(line) = lines.next() {
        if line == "name = \"tao\"" {
            pinned_tao = lines.next() == Some("version = \"0.35.3\"");
            break;
        }
    }
    assert!(
        pinned_tao,
        "qualification DPI contract must be re-reviewed when the pinned TAO version changes"
    );
    let process = unsafe { GetCurrentProcess() };
    let existing = unsafe { GetDpiAwarenessContextForProcess(process) };
    if !is_production_dpi_context(existing) {
        assert_ne!(
            unsafe {
                SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            },
            0,
            "qualification process could not establish TAO's supported PerMonitorV2 context before creating an HWND",
        );
    }
    let process_context = unsafe { GetDpiAwarenessContextForProcess(process) };
    let thread_context = unsafe { GetThreadDpiAwarenessContext() };
    assert!(
        is_production_dpi_context(process_context),
        "qualification process does not match production PerMonitorV2 awareness"
    );
    assert!(
        is_production_dpi_context(thread_context),
        "qualification thread does not inherit production PerMonitorV2 awareness"
    );
}

#[cfg(test)]
fn record_qualification_window_contexts(
    owner: HWND,
    dialog: HWND,
    state: &ConfirmationDialogState,
) -> bool {
    let process_context = unsafe { GetDpiAwarenessContextForProcess(GetCurrentProcess()) };
    let thread_context = unsafe { GetThreadDpiAwarenessContext() };
    let owner_context = unsafe { GetWindowDpiAwarenessContext(owner) };
    let dialog_context = unsafe { GetWindowDpiAwarenessContext(dialog) };
    let confirm_context = unsafe { GetWindowDpiAwarenessContext(state.confirm_button) };
    let cancel_context = unsafe { GetWindowDpiAwarenessContext(state.cancel_button) };
    let owner_dpi = unsafe { GetDpiForWindow(owner) };
    let dialog_dpi = unsafe { GetDpiForWindow(dialog) };
    println!(
        "VISION_WALLET_CONFIRMATION_DPI_CONTEXT process={}:{} thread={}:{} owner={}:{} dialog={}:{} confirm={}:{} cancel={}:{} owner_dpi={owner_dpi} dialog_dpi={dialog_dpi}",
        qualification_dpi_context_label(process_context),
        process_context as isize,
        qualification_dpi_context_label(thread_context),
        thread_context as isize,
        qualification_dpi_context_label(owner_context),
        owner_context as isize,
        qualification_dpi_context_label(dialog_context),
        dialog_context as isize,
        qualification_dpi_context_label(confirm_context),
        confirm_context as isize,
        qualification_dpi_context_label(cancel_context),
        cancel_context as isize,
    );
    let valid = [
        process_context,
        thread_context,
        owner_context,
        dialog_context,
        confirm_context,
        cancel_context,
    ]
    .into_iter()
    .all(is_production_dpi_context)
        && owner_dpi > 0
        && dialog_dpi > 0
        && owner_dpi == dialog_dpi
        && confirmation_input_contexts_are_absent(dialog, state);
    if valid {
        QUALIFICATION_CONFIRMATION_DPI.store(dialog_dpi, Ordering::SeqCst);
    }
    valid
}

fn run_native_confirmation(
    owner_window: HWND,
    fields: TransferConfirmationFields<'_>,
    authority_is_current: &dyn Fn() -> bool,
) -> Result<(), NativeConfirmationError> {
    if !authority_is_current() {
        return Err(NativeConfirmationError::AuthorityRevoked);
    }
    let instance = unsafe { GetModuleHandleW(null()) } as HINSTANCE;
    if instance.is_null() || unsafe { IsWindow(owner_window) } == 0 {
        return Err(NativeConfirmationError::NativeUiUnavailable);
    }

    let sequence = CLASS_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let class_name = wide_null(&format!(
        "VisionDesktopTransactionConfirmation-{}-{sequence}",
        std::process::id()
    ));
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(confirmation_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: null_mut(),
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
        lpszMenuName: null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: null_mut(),
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        return Err(NativeConfirmationError::NativeUiUnavailable);
    }
    let _registration = RegisteredWindowClass {
        instance,
        class_name: &class_name,
    };

    let (x, y) = centered_position(owner_window);
    let mut state = ConfirmationDialogState {
        display: ConfirmationDisplayBuffers::new(fields),
        outcome: ConfirmationOutcome::Pending,
        confirm_button: null_mut(),
        cancel_button: null_mut(),
        display_verified: false,
        display_verified_at: None,
        press_started_after_display: None,
        fresh_input_time: None,
        fresh_input_device: None,
    };
    let title = wide_null("Confirm Vision Transaction");
    let dialog = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            x,
            y,
            DIALOG_WIDTH,
            DIALOG_HEIGHT,
            owner_window,
            null_mut(),
            instance,
            (&mut state as *mut ConfirmationDialogState).cast::<c_void>(),
        )
    };
    if dialog.is_null() {
        state.wipe();
        return Err(NativeConfirmationError::NativeUiUnavailable);
    }

    #[cfg(test)]
    if std::env::var_os("VISION_WALLET_CONFIRMATION_SCENARIO").is_some()
        && !record_qualification_window_contexts(owner_window, dialog, &state)
    {
        state.wipe();
        unsafe { DestroyWindow(dialog) };
        return Err(NativeConfirmationError::NativeUiUnavailable);
    }

    let _modal_owner = DisabledOwner::new(owner_window);
    if unsafe { SetTimer(dialog, AUTHORITY_TIMER_ID, AUTHORITY_TIMER_MS, None) } == 0 {
        state.wipe();
        unsafe { DestroyWindow(dialog) };
        return Err(NativeConfirmationError::NativeUiUnavailable);
    }
    unsafe {
        ShowWindow(dialog, SW_SHOW);
        SetForegroundWindow(dialog);
    }
    if !restore_armed_confirmation_focus(dialog, &state) {
        state.wipe();
        unsafe { DestroyWindow(dialog) };
        return Err(NativeConfirmationError::NativeUiUnavailable);
    }

    let mut message = MSG::default();
    while unsafe { IsWindow(dialog) } != 0 {
        let status = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if status <= 0 {
            state.outcome = ConfirmationOutcome::Failed;
            state.wipe();
            if unsafe { IsWindow(dialog) } != 0 {
                unsafe { DestroyWindow(dialog) };
            }
            if status == 0 {
                unsafe { PostQuitMessage(i32::try_from(message.wParam).unwrap_or_default()) };
            }
            break;
        }
        if message.hwnd == dialog
            && message.message == WM_TIMER
            && message.wParam == AUTHORITY_TIMER_ID
        {
            if !confirmation_input_contexts_are_absent(dialog, &state) {
                state.outcome = ConfirmationOutcome::Failed;
                state.wipe();
                unsafe { DestroyWindow(dialog) };
                continue;
            }
            if unsafe { IsWindow(owner_window) } == 0 || !authority_is_current() {
                state.outcome = ConfirmationOutcome::AuthorityRevoked;
                state.wipe();
                unsafe { DestroyWindow(dialog) };
                continue;
            }
        }
        if record_fresh_confirmation_input(&mut state, &message) {
            unsafe { SendMessageW(state.confirm_button, BM_CLICK, 0, 0) };
            if unsafe { IsWindow(dialog) } != 0 {
                fail_closed_window(dialog, &mut state);
            }
            continue;
        }
        if unsafe { IsDialogMessageW(dialog, &message) } == 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    unsafe { KillTimer(dialog, AUTHORITY_TIMER_ID) };
    state.wipe();

    match state.outcome {
        ConfirmationOutcome::Confirmed
            if unsafe { IsWindow(owner_window) } != 0 && authority_is_current() =>
        {
            Ok(())
        }
        ConfirmationOutcome::Confirmed | ConfirmationOutcome::AuthorityRevoked => {
            Err(NativeConfirmationError::AuthorityRevoked)
        }
        ConfirmationOutcome::Cancelled => Err(NativeConfirmationError::Cancelled),
        ConfirmationOutcome::Pending | ConfirmationOutcome::Failed => {
            Err(NativeConfirmationError::NativeUiUnavailable)
        }
    }
}

unsafe extern "system" fn confirmation_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if create.is_null() || unsafe { (*create).lpCreateParams }.is_null() {
            return 0;
        }
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, (*create).lpCreateParams as isize) };
        return 1;
    }

    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut ConfirmationDialogState;
    if state.is_null() {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state };

    match message {
        WM_CREATE => {
            if !disable_text_services(window) {
                state.outcome = ConfirmationOutcome::Failed;
                return -1;
            }
            let Some((confirm_button, cancel_button)) = create_buttons(window) else {
                state.outcome = ConfirmationOutcome::Failed;
                return -1;
            };
            state.confirm_button = confirm_button;
            state.cancel_button = cancel_button;
            0
        }
        WM_COMMAND => {
            let control_id = wparam & 0xffff;
            let notification = (wparam >> 16) & 0xffff;
            if notification != BN_CLICKED as usize {
                return unsafe { DefWindowProcW(window, message, wparam, lparam) };
            }
            match control_id {
                CONFIRM_BUTTON_ID => {
                    if !confirmation_input_contexts_are_absent(window, state) {
                        return fail_closed_window(window, state);
                    }
                    if consume_fresh_confirmation_command(state, lparam as HWND) {
                        state.outcome = ConfirmationOutcome::Confirmed;
                        state.wipe();
                        unsafe { DestroyWindow(window) };
                        0
                    } else {
                        unsafe { DefWindowProcW(window, message, wparam, lparam) }
                    }
                }
                CANCEL_BUTTON_ID if lparam as HWND == state.cancel_button => {
                    state.outcome = ConfirmationOutcome::Cancelled;
                    state.wipe();
                    unsafe { DestroyWindow(window) };
                    0
                }
                _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
            }
        }
        WM_PAINT => {
            if !paint_confirmation(window, state) || !arm_confirmation(window, state) {
                return fail_closed_window(window, state);
            }
            0
        }
        WM_COPY | WM_CUT | WM_PASTE | WM_CLEAR | WM_CONTEXTMENU | WM_GETTEXT | WM_GETTEXTLENGTH
        | WM_SETTEXT => 0,
        WM_CHAR => fail_closed_window(window, state),
        WM_IME_SETCONTEXT => {
            if input_context_is_absent(window) {
                0
            } else {
                fail_closed_window(window, state)
            }
        }
        message if is_blocked_text_service_message(message) => fail_closed_window(window, state),
        WM_CLOSE => {
            state.outcome = ConfirmationOutcome::Cancelled;
            state.wipe();
            unsafe { DestroyWindow(window) };
            0
        }
        WM_NCDESTROY => {
            state.wipe();
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn fail_closed_window(window: HWND, state: &mut ConfirmationDialogState) -> LRESULT {
    state.outcome = ConfirmationOutcome::Failed;
    state.wipe();
    unsafe { DestroyWindow(window) };
    0
}

fn arm_confirmation(window: HWND, state: &mut ConfirmationDialogState) -> bool {
    if state.display_verified {
        return true;
    }
    if state.confirm_button.is_null()
        || state.cancel_button.is_null()
        || unsafe { IsWindow(state.confirm_button) } == 0
        || unsafe { IsWindow(state.cancel_button) } == 0
        || unsafe { IsWindowEnabled(state.confirm_button) } != 0
    {
        return false;
    }
    unsafe {
        SendMessageW(state.cancel_button, BM_SETSTYLE, BS_PUSHBUTTON as usize, 1);
        SendMessageW(
            state.confirm_button,
            BM_SETSTYLE,
            BS_DEFPUSHBUTTON as usize,
            1,
        );
        EnableWindow(state.confirm_button, 1);
        SetFocus(state.confirm_button);
    }
    let confirm_style = unsafe { GetWindowLongPtrW(state.confirm_button, GWL_STYLE) } as i32;
    let cancel_style = unsafe { GetWindowLongPtrW(state.cancel_button, GWL_STYLE) } as i32;
    if confirm_style & BS_TYPEMASK != BS_DEFPUSHBUTTON
        || cancel_style & BS_TYPEMASK != BS_PUSHBUTTON
        || unsafe { IsWindowEnabled(state.confirm_button) } == 0
        || unsafe { GetFocus() } != state.confirm_button
        || !confirmation_input_contexts_are_absent(window, state)
    {
        return false;
    }
    state.display_verified = true;
    state.display_verified_at = Some(unsafe { GetTickCount() });
    #[cfg(test)]
    if std::env::var_os("VISION_WALLET_CONFIRMATION_SCENARIO").is_some() {
        QUALIFICATION_CONFIRM_FOCUS_VERIFIED.store(true, Ordering::SeqCst);
    }
    true
}

fn restore_armed_confirmation_focus(window: HWND, state: &ConfirmationDialogState) -> bool {
    if !state.display_verified {
        return true;
    }
    unsafe { SetFocus(state.confirm_button) };
    let valid = unsafe { GetFocus() } == state.confirm_button
        && confirmation_input_contexts_are_absent(window, state);
    #[cfg(test)]
    if valid && std::env::var_os("VISION_WALLET_CONFIRMATION_SCENARIO").is_some() {
        QUALIFICATION_CONFIRM_FOCUS_VERIFIED.store(true, Ordering::SeqCst);
    }
    valid
}

fn record_fresh_confirmation_input(state: &mut ConfirmationDialogState, message: &MSG) -> bool {
    let mut source = INPUT_MESSAGE_SOURCE::default();
    let source = if unsafe { GetCurrentInputMessageSource(&mut source) } != 0 {
        Some(source)
    } else {
        None
    };
    apply_confirmation_input_source(state, message, source)
}

fn apply_confirmation_input_source(
    state: &mut ConfirmationDialogState,
    message: &MSG,
    source: Option<INPUT_MESSAGE_SOURCE>,
) -> bool {
    state.fresh_input_time = None;
    state.fresh_input_device = None;
    let Some(source) = source else {
        state.press_started_after_display = None;
        return false;
    };
    if !state.display_verified
        || message.hwnd != state.confirm_button
        || source.originId != IMO_HARDWARE
        || !state
            .display_verified_at
            .is_some_and(|verified_at| tick_is_strictly_after(message.time, verified_at))
    {
        state.press_started_after_display = None;
        return false;
    }

    let keyboard_key = u16::try_from(message.wParam).unwrap_or_default();
    match message.message {
        WM_LBUTTONDOWN if source.deviceType == IMDT_MOUSE => {
            state.press_started_after_display = Some(ConfirmationPress::Mouse);
        }
        WM_LBUTTONUP if source.deviceType == IMDT_MOUSE => {
            if state.press_started_after_display == Some(ConfirmationPress::Mouse) {
                state.fresh_input_time = Some(message.time);
                state.fresh_input_device = Some(ConfirmationInputDevice::Mouse);
            }
            state.press_started_after_display = None;
        }
        WM_KEYDOWN
            if source.deviceType == IMDT_KEYBOARD
                && (keyboard_key == VK_RETURN || keyboard_key == VK_SPACE)
                && (message.lParam & (1_isize << 30)) == 0 =>
        {
            state.press_started_after_display = Some(ConfirmationPress::Keyboard(keyboard_key));
        }
        WM_KEYUP
            if source.deviceType == IMDT_KEYBOARD
                && (keyboard_key == VK_RETURN || keyboard_key == VK_SPACE) =>
        {
            if state.press_started_after_display == Some(ConfirmationPress::Keyboard(keyboard_key))
            {
                state.fresh_input_time = Some(message.time);
                state.fresh_input_device = Some(ConfirmationInputDevice::Keyboard);
            }
            state.press_started_after_display = None;
        }
        _ => state.press_started_after_display = None,
    }
    state.fresh_input_device == Some(ConfirmationInputDevice::Keyboard)
}

const fn tick_is_strictly_after(candidate: u32, baseline: u32) -> bool {
    let elapsed = candidate.wrapping_sub(baseline);
    elapsed != 0 && elapsed < (1_u32 << 31)
}

fn consume_fresh_confirmation_command(state: &mut ConfirmationDialogState, control: HWND) -> bool {
    let fresh_input_time = state.fresh_input_time.take();
    let fresh_input_device = state.fresh_input_device.take();
    let completed_press_was_present = fresh_input_time.is_some() || fresh_input_device.is_some();
    let approved = state.display_verified
        && !state.confirm_button.is_null()
        && control == state.confirm_button
        && unsafe { IsWindow(state.confirm_button) } != 0
        && unsafe { IsWindowEnabled(state.confirm_button) } != 0
        && fresh_input_device.is_some()
        && fresh_input_time == Some(unsafe { GetMessageTime() } as u32);
    if completed_press_was_present {
        state.press_started_after_display = None;
    }
    #[cfg(test)]
    if approved && std::env::var_os("VISION_WALLET_CONFIRMATION_SCENARIO").is_some() {
        let code = match fresh_input_device {
            Some(ConfirmationInputDevice::Keyboard) => 1,
            Some(ConfirmationInputDevice::Mouse) => 2,
            None => 0,
        };
        QUALIFICATION_ACCEPTED_INPUT_DEVICE.store(code, Ordering::SeqCst);
    }
    approved
}

fn paint_confirmation(window: HWND, state: &ConfirmationDialogState) -> bool {
    let mut paint = PAINTSTRUCT::default();
    let device = unsafe { BeginPaint(window, &mut paint) };
    if device.is_null() {
        return false;
    }
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    if font.is_null() {
        unsafe { EndPaint(window, &paint) };
        return false;
    }
    let previous = unsafe { SelectObject(device, font) };
    if previous.is_null()
        || unsafe { SetBkMode(device, i32::try_from(TRANSPARENT).unwrap_or(1)) } == 0
    {
        unsafe { EndPaint(window, &paint) };
        return false;
    }

    let mut rendered = draw_literal(
        device,
        "Verify every value. This confirms only this exact unsigned transaction; it does not sign or submit it.",
        RECT { left: 24, top: 18, right: 816, bottom: 54 },
        DT_LEFT | DT_WORDBREAK,
    );
    rendered &= draw_label(device, "Sender", 70);
    rendered &= draw_buffer(device, &state.display.sender, 92, 132);
    rendered &= draw_label(device, "Recipient", 142);
    rendered &= draw_buffer(device, &state.display.recipient, 164, 204);
    rendered &= draw_label(device, "Amount", 214);
    rendered &= draw_buffer(device, &state.display.amount, 236, 260);
    rendered &= draw_label(device, "Fees", 270);
    rendered &= draw_buffer(device, &state.display.fee, 292, 318);
    rendered &= draw_label(device, "Total debit", 328);
    rendered &= draw_buffer(device, &state.display.total, 350, 374);
    rendered &= draw_label(device, "Nonce", 384);
    rendered &= draw_buffer(device, &state.display.nonce, 406, 430);
    rendered &= draw_label(device, "Transaction identifier", 440);
    rendered &= draw_buffer(device, &state.display.transaction_id, 462, 502);
    rendered &= draw_literal(
        device,
        "Mined transactions can reorganize and are never presented as irreversible.",
        RECT {
            left: 24,
            top: 514,
            right: 816,
            bottom: 544,
        },
        DT_LEFT | DT_WORDBREAK,
    );

    let paint_ended = unsafe {
        SelectObject(device, previous);
        EndPaint(window, &paint)
    } != 0;
    rendered && paint_ended
}

fn draw_label(device: windows_sys::Win32::Graphics::Gdi::HDC, label: &str, top: i32) -> bool {
    draw_literal(
        device,
        label,
        RECT {
            left: 24,
            top,
            right: 816,
            bottom: top + 20,
        },
        DT_LEFT | DT_SINGLELINE,
    )
}

fn draw_literal(
    device: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &str,
    mut bounds: RECT,
    format: u32,
) -> bool {
    let wide: Vec<u16> = text.encode_utf16().collect();
    draw_text_checked(device, &wide, &mut bounds, format)
}

fn draw_buffer(
    device: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &[u16],
    top: i32,
    bottom: i32,
) -> bool {
    let mut bounds = RECT {
        left: 24,
        top,
        right: 816,
        bottom,
    };
    draw_text_checked(device, text, &mut bounds, DT_LEFT | DT_WORDBREAK)
}

fn draw_text_checked(
    device: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &[u16],
    bounds: &mut RECT,
    format: u32,
) -> bool {
    if device.is_null()
        || text.is_empty()
        || bounds.right <= bounds.left
        || bounds.bottom <= bounds.top
    {
        return false;
    }
    let available_width = bounds.right - bounds.left;
    let available_height = bounds.bottom - bounds.top;
    let mut measured = RECT {
        left: 0,
        top: 0,
        right: available_width,
        bottom: available_height,
    };
    let length = i32::try_from(text.len()).unwrap_or(i32::MAX);
    if unsafe {
        DrawTextW(
            device,
            text.as_ptr(),
            length,
            &mut measured,
            format | DT_CALCRECT,
        )
    } <= 0
        || measured.right - measured.left > available_width
        || measured.bottom - measured.top > available_height
    {
        return false;
    }
    (unsafe { DrawTextW(device, text.as_ptr(), length, bounds, format) }) > 0
}

fn create_buttons(window: HWND) -> Option<(HWND, HWND)> {
    let button_class = wide_null("BUTTON");
    let confirm_text = wide_null("Confirm exact transaction");
    let cancel_text = wide_null("Cancel");
    let confirm = create_button(
        window,
        &button_class,
        &confirm_text,
        BS_PUSHBUTTON as u32,
        520,
        558,
        190,
        CONFIRM_BUTTON_ID,
    );
    let cancel = create_button(
        window,
        &button_class,
        &cancel_text,
        BS_DEFPUSHBUTTON as u32,
        724,
        558,
        90,
        CANCEL_BUTTON_ID,
    );
    if confirm.is_null() || cancel.is_null() {
        if !confirm.is_null() {
            unsafe { DestroyWindow(confirm) };
        }
        if !cancel.is_null() {
            unsafe { DestroyWindow(cancel) };
        }
        return None;
    }
    unsafe { EnableWindow(confirm, 0) };
    if unsafe { IsWindowEnabled(confirm) } != 0 {
        unsafe {
            DestroyWindow(confirm);
            DestroyWindow(cancel);
        }
        return None;
    }
    Some((confirm, cancel))
}

#[allow(clippy::too_many_arguments)]
fn create_button(
    parent: HWND,
    class_name: &[u16],
    text: &[u16],
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    control_id: usize,
) -> HWND {
    let button = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
            x,
            y,
            width,
            38,
            parent,
            control_id as *mut c_void,
            GetModuleHandleW(null()) as HINSTANCE,
            null(),
        )
    };
    if button.is_null() {
        return null_mut();
    }
    if !disable_text_services(button) {
        unsafe { DestroyWindow(button) };
        return null_mut();
    }
    button
}

fn disable_text_services(window: HWND) -> bool {
    if window.is_null() || unsafe { ImmAssociateContextEx(window, null_mut(), 0) } == 0 {
        return false;
    }
    input_context_is_absent(window)
}

fn input_context_is_absent(window: HWND) -> bool {
    let context = unsafe { ImmGetContext(window) };
    if context.is_null() {
        return true;
    }
    let _released = unsafe { ImmReleaseContext(window, context) };
    false
}

const fn is_blocked_text_service_message(message: u32) -> bool {
    matches!(
        message,
        WM_IME_STARTCOMPOSITION
            | WM_IME_ENDCOMPOSITION
            | WM_IME_COMPOSITION
            | WM_IME_NOTIFY
            | WM_IME_CONTROL
            | WM_IME_COMPOSITIONFULL
            | WM_IME_SELECT
            | WM_IME_CHAR
            | WM_IME_REQUEST
            | WM_IME_KEYDOWN
            | WM_IME_KEYUP
            | WM_INPUTLANGCHANGEREQUEST
            | WM_INPUTLANGCHANGE
    )
}

fn centered_position(owner: HWND) -> (i32, i32) {
    let mut owner_rect = RECT::default();
    if unsafe { GetWindowRect(owner, &mut owner_rect) } == 0 {
        return (
            windows_sys::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            windows_sys::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
        );
    }
    (
        owner_rect.left + ((owner_rect.right - owner_rect.left - DIALOG_WIDTH) / 2).max(0),
        owner_rect.top + ((owner_rect.bottom - owner_rect.top - DIALOG_HEIGHT) / 2).max(0),
    )
}

struct DisabledOwner(HWND);

impl DisabledOwner {
    fn new(owner: HWND) -> Self {
        unsafe { EnableWindow(owner, 0) };
        Self(owner)
    }
}

impl Drop for DisabledOwner {
    fn drop(&mut self) {
        unsafe {
            EnableWindow(self.0, 1);
            SetForegroundWindow(self.0);
        }
    }
}

struct RegisteredWindowClass<'a> {
    instance: HINSTANCE,
    class_name: &'a [u16],
}

impl Drop for RegisteredWindowClass<'_> {
    fn drop(&mut self) {
        unsafe { UnregisterClassW(self.class_name.as_ptr(), self.instance) };
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_sensitive(value: &str) -> Zeroizing<Vec<u16>> {
    Zeroizing::new(value.encode_utf16().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{
        account::derive_account_identity,
        core_client::{WalletCoreAccountSnapshot, WalletCoreClientError, WalletCoreStatus},
        preview::{bind_consumed_preview_for_test, prepare_with_source_for_test},
        public_request::WalletTransferPreviewRequest,
        runtime::{WalletOperationKind, WalletOperationPermit},
        secrets::{WalletPassword, WalletSeed},
        signing::{
            reset_signed_artifact_drop_count_for_test,
            sign_after_native_approval_with_observer_for_test, signed_artifact_drop_count_for_test,
            SigningCoordinatorObserver, SigningCoordinatorStage,
        },
        vault::EncryptedWalletVault,
    };
    use std::{
        mem::size_of,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };
    use windows_sys::Win32::{
        Graphics::Gdi::{CreateCompatibleDC, DeleteDC},
        UI::Input::{
            Ime::{ImmAssociateContext, ImmCreateContext, ImmDestroyContext},
            KeyboardAndMouse::{
                GetKeyboardLayoutNameW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
                KEYEVENTF_KEYUP,
            },
            IMO_INJECTED,
        },
        UI::WindowsAndMessaging::{FindWindowW, GetForegroundWindow},
    };

    const MAIN: &str = "main";
    const PASSWORD: &str = "correct horse battery staple";

    struct FakeCore {
        address: String,
        identity_calls: AtomicUsize,
        panic_on_identity_call: Option<usize>,
        replace_identity_on_call: Option<usize>,
        identity_error_on_call: Option<(usize, WalletCoreClientError)>,
    }

    impl WalletCoreReadSource for FakeCore {
        fn account_snapshot(
            &self,
            _address: &str,
        ) -> Result<WalletCoreAccountSnapshot, WalletCoreClientError> {
            Ok(WalletCoreAccountSnapshot {
                address: self.address.clone(),
                exists: true,
                balance: 10_000_000_000,
                nonce: 7,
            })
        }

        fn status(&self) -> Result<WalletCoreStatus, WalletCoreClientError> {
            Ok(WalletCoreStatus {
                version: "3".to_string(),
                canonical_tip_height: 42,
                canonical_tip_hash: "a".repeat(64),
                peer_count: 2,
                recovery_state: "normal".to_string(),
            })
        }

        fn validated_identity_fingerprint(&self) -> Result<[u8; 32], WalletCoreClientError> {
            let call = self.identity_calls.fetch_add(1, AtomicOrdering::SeqCst);
            if self.panic_on_identity_call == Some(call) {
                panic!("injected final Core identity panic");
            }
            if let Some((_, error)) = self
                .identity_error_on_call
                .filter(|(error_at, _)| *error_at == call)
            {
                return Err(error);
            }
            if self
                .replace_identity_on_call
                .is_some_and(|replace_at| call >= replace_at)
            {
                Ok([0x43; 32])
            } else {
                Ok([0x42; 32])
            }
        }
    }

    fn unlocked_runtime(seed_byte: u8) -> (WalletRuntimeState, String) {
        let runtime = WalletRuntimeState::for_test();
        let seed = WalletSeed::for_test(seed_byte);
        let identity = derive_account_identity(&seed);
        let password = WalletPassword::for_test(PASSWORD);
        let vault =
            EncryptedWalletVault::encrypt_for_test("primary", 1_700_000_000_000, &seed, &password)
                .unwrap();
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::Unlock)
            .unwrap();
        let status = permit
            .run_authorized(|activation| runtime.unlock_vault(activation, &vault, &password))
            .unwrap()
            .unwrap();
        permit.complete(status).unwrap();
        drop(permit);
        (runtime, identity.address)
    }

    fn request(recipient: &str) -> WalletTransferPreviewRequest {
        serde_json::from_value(serde_json::json!({
            "recipient": recipient,
            "amount": "2.5"
        }))
        .unwrap()
    }

    fn dialog_state_for_test() -> ConfirmationDialogState {
        let sender = "5".repeat(64);
        let recipient = "6".repeat(64);
        let transaction_id = "7".repeat(64);
        ConfirmationDialogState {
            display: ConfirmationDisplayBuffers::new(TransferConfirmationFields {
                sender_address: &sender,
                recipient_address: &recipient,
                amount_raw_units: 1,
                charged_fee_raw_units: 1,
                fee_limit_raw_units: 201,
                total_debit_raw_units: 2,
                nonce: 7,
                transaction_id: &transaction_id,
            }),
            outcome: ConfirmationOutcome::Pending,
            confirm_button: null_mut(),
            cancel_button: null_mut(),
            display_verified: false,
            display_verified_at: None,
            press_started_after_display: None,
            fresh_input_time: None,
            fresh_input_device: None,
        }
    }

    fn hidden_test_window() -> HWND {
        let button_class = wide_null("BUTTON");
        unsafe {
            CreateWindowExW(
                0,
                button_class.as_ptr(),
                null(),
                WS_POPUP,
                0,
                0,
                DIALOG_WIDTH,
                DIALOG_HEIGHT,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null_mut(),
            )
        }
    }

    fn pending<'a>(
        runtime: &'a WalletRuntimeState,
        sender: &str,
        recipient: &str,
    ) -> PendingTransferConfirmation<'a, FakeCore> {
        pending_with_identity_panic(runtime, sender, recipient, None)
    }

    fn pending_with_identity_panic<'a>(
        runtime: &'a WalletRuntimeState,
        sender: &str,
        recipient: &str,
        panic_on_identity_call: Option<usize>,
    ) -> PendingTransferConfirmation<'a, FakeCore> {
        pending_with_core_change(
            runtime,
            sender,
            recipient,
            panic_on_identity_call,
            None,
            None,
        )
    }

    fn pending_with_core_change<'a>(
        runtime: &'a WalletRuntimeState,
        sender: &str,
        recipient: &str,
        panic_on_identity_call: Option<usize>,
        replace_identity_on_call: Option<usize>,
        identity_error_on_call: Option<(usize, WalletCoreClientError)>,
    ) -> PendingTransferConfirmation<'a, FakeCore> {
        let prepare_source = FakeCore {
            address: sender.to_string(),
            identity_calls: AtomicUsize::new(0),
            panic_on_identity_call: None,
            replace_identity_on_call: None,
            identity_error_on_call: None,
        };
        let prepare_permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        let preview =
            prepare_with_source_for_test(&prepare_permit, request(recipient), &prepare_source)
                .unwrap();

        let consume_permit: WalletOperationPermit<'a> = runtime
            .begin_operation(MAIN, WalletOperationKind::ConsumePreview)
            .unwrap();
        let intent = consume_permit
            .consume_transaction_preview(&preview.handle)
            .unwrap();
        bind_consumed_preview_for_test(
            consume_permit,
            intent,
            FakeCore {
                address: sender.to_string(),
                identity_calls: AtomicUsize::new(0),
                panic_on_identity_call,
                replace_identity_on_call,
                identity_error_on_call,
            },
        )
        .unwrap()
    }

    struct ConfirmingCeremony;

    impl TransactionConfirmationCeremony for ConfirmingCeremony {
        fn present(
            &self,
            fields: TransferConfirmationFields<'_>,
            authority_is_current: &dyn Fn() -> bool,
        ) -> Result<(), NativeConfirmationError> {
            assert!(authority_is_current());
            assert_eq!(fields.amount_raw_units, 2_500_000_000);
            assert_eq!(fields.charged_fee_raw_units, 1);
            assert_eq!(fields.fee_limit_raw_units, 201);
            assert_eq!(fields.total_debit_raw_units, 2_500_000_001);
            assert_eq!(fields.nonce, 7);
            assert_eq!(fields.transaction_id.len(), 64);
            Ok(())
        }
    }

    struct CancellingCeremony;

    impl TransactionConfirmationCeremony for CancellingCeremony {
        fn present(
            &self,
            _fields: TransferConfirmationFields<'_>,
            authority_is_current: &dyn Fn() -> bool,
        ) -> Result<(), NativeConfirmationError> {
            assert!(authority_is_current());
            Err(NativeConfirmationError::Cancelled)
        }
    }

    struct RevokingCeremony<'a> {
        runtime: &'a WalletRuntimeState,
    }

    impl TransactionConfirmationCeremony for RevokingCeremony<'_> {
        fn present(
            &self,
            _fields: TransferConfirmationFields<'_>,
            authority_is_current: &dyn Fn() -> bool,
        ) -> Result<(), NativeConfirmationError> {
            self.runtime.invalidate_all().unwrap();
            assert!(!authority_is_current());
            Err(NativeConfirmationError::AuthorityRevoked)
        }
    }

    struct PanickingCeremony;

    impl TransactionConfirmationCeremony for PanickingCeremony {
        fn present(
            &self,
            _fields: TransferConfirmationFields<'_>,
            _authority_is_current: &dyn Fn() -> bool,
        ) -> Result<(), NativeConfirmationError> {
            panic!("injected native confirmation panic")
        }
    }

    struct PanickingSigningObserver {
        stage: SigningCoordinatorStage,
    }

    impl SigningCoordinatorObserver for PanickingSigningObserver {
        fn checkpoint(&self, stage: SigningCoordinatorStage) {
            if stage == self.stage {
                panic!("injected private signing coordinator panic");
            }
        }
    }

    struct OccupiedSigningSlotObserver<'a> {
        runtime: &'a WalletRuntimeState,
    }

    impl SigningCoordinatorObserver for OccupiedSigningSlotObserver<'_> {
        fn checkpoint(&self, stage: SigningCoordinatorStage) {
            if matches!(
                stage,
                SigningCoordinatorStage::SeedAccountDerivation
                    | SigningCoordinatorStage::SignatureConstruction
                    | SigningCoordinatorStage::SignatureVerification
            ) {
                return;
            }
            assert_eq!(
                self.runtime
                    .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                    .err(),
                Some(super::super::runtime::WalletRuntimeError::OperationInProgress)
            );
        }
    }

    struct RevokingSigningObserver {
        runtime: Arc<WalletRuntimeState>,
        stage: SigningCoordinatorStage,
        worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    }

    impl RevokingSigningObserver {
        fn new(runtime: Arc<WalletRuntimeState>, stage: SigningCoordinatorStage) -> Self {
            Self {
                runtime,
                stage,
                worker: Mutex::new(None),
            }
        }

        fn join(&self) {
            if let Some(worker) = self.worker.lock().unwrap().take() {
                worker.join().unwrap();
            }
        }
    }

    impl SigningCoordinatorObserver for RevokingSigningObserver {
        fn checkpoint(&self, stage: SigningCoordinatorStage) {
            if stage != self.stage {
                return;
            }
            if matches!(
                stage,
                SigningCoordinatorStage::SeedAccountDerivation
                    | SigningCoordinatorStage::SignatureConstruction
                    | SigningCoordinatorStage::SignatureVerification
            ) {
                let runtime = Arc::clone(&self.runtime);
                let worker = thread::spawn(move || runtime.invalidate_all().unwrap());
                *self.worker.lock().unwrap() = Some(worker);
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                while !self.runtime.revocation_is_pending_for_test() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "revocation did not become pending while the signing mutex was held"
                    );
                    thread::yield_now();
                }
            } else {
                self.runtime.invalidate_all().unwrap();
            }
        }
    }

    #[test]
    fn explicit_native_approval_signs_and_destroys_only_the_exact_confirmed_intent() {
        let (runtime, sender) = unlocked_runtime(31);
        let recipient = "d".repeat(64);
        let ceremony = ConfirmingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };

        engine
            .confirm_pending(pending(&runtime, &sender, &recipient))
            .unwrap();
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        permit.ensure_current().unwrap();
    }

    #[test]
    fn cancellation_consumes_the_intent_and_releases_the_operation_slot() {
        let (runtime, sender) = unlocked_runtime(32);
        let ceremony = CancellingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };

        assert_eq!(
            engine
                .confirm_pending(pending(&runtime, &sender, &"e".repeat(64)))
                .err()
                .unwrap(),
            WalletConfirmationError::Cancelled
        );
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        permit.ensure_current().unwrap();
    }

    #[test]
    fn runtime_revocation_during_the_modal_window_suppresses_confirmation() {
        let (runtime, sender) = unlocked_runtime(33);
        let ceremony = RevokingCeremony { runtime: &runtime };
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };

        assert_eq!(
            engine
                .confirm_pending(pending(&runtime, &sender, &"f".repeat(64)))
                .err()
                .unwrap(),
            WalletConfirmationError::AuthorityRevoked
        );
    }

    #[test]
    fn ceremony_panic_is_contained_and_invalidates_wallet_authority() {
        let (runtime, sender) = unlocked_runtime(34);
        let ceremony = PanickingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };

        assert_eq!(
            engine
                .confirm_pending(pending(&runtime, &sender, &"1".repeat(64)))
                .err()
                .unwrap(),
            WalletConfirmationError::RuntimeUnavailable
        );
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        assert!(permit.current_public_account().is_err());
    }

    #[test]
    fn final_core_identity_panic_is_contained_and_invalidates_wallet_authority() {
        let (runtime, sender) = unlocked_runtime(35);
        let ceremony = ConfirmingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };
        let pending = pending_with_identity_panic(&runtime, &sender, &"8".repeat(64), Some(3));

        assert_eq!(
            engine.confirm_pending(pending).err().unwrap(),
            WalletConfirmationError::RuntimeUnavailable
        );
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        assert!(permit.current_public_account().is_err());
    }

    #[test]
    fn core_replacement_after_signing_discards_the_result_and_revokes_authority() {
        let (runtime, sender) = unlocked_runtime(36);
        let ceremony = ConfirmingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };
        let pending =
            pending_with_core_change(&runtime, &sender, &"7".repeat(64), None, Some(6), None);

        assert_eq!(
            engine.confirm_pending(pending),
            Err(WalletConfirmationError::CoreUnavailable)
        );
        let permit = runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
        assert!(permit.current_public_account().is_err());
    }

    #[test]
    fn every_private_signing_stage_panic_is_contained_and_revokes_authority() {
        for (index, stage) in [
            SigningCoordinatorStage::Promoted,
            SigningCoordinatorStage::BeforeSeedAccess,
            SigningCoordinatorStage::SeedAccountDerivation,
            SigningCoordinatorStage::SignatureConstruction,
            SigningCoordinatorStage::SignatureVerification,
            SigningCoordinatorStage::AfterSignatureVerification,
            SigningCoordinatorStage::BeforeCompletion,
        ]
        .into_iter()
        .enumerate()
        {
            let (runtime, sender) = unlocked_runtime(80_u8.saturating_add(index as u8));
            let ceremony = ConfirmingCeremony;
            let engine = WalletTransactionConfirmationEngine {
                runtime: &runtime,
                ceremony: &ceremony,
            };
            let observer = PanickingSigningObserver { stage };
            let pending = pending(&runtime, &sender, &"9".repeat(64));

            let result = engine.run_fail_closed(|| {
                sign_after_native_approval_with_observer_for_test(
                    pending,
                    NativeConfirmationApproval::issue(),
                    &observer,
                )
                .map_err(map_signing_error)
            });
            assert_eq!(result, Err(WalletConfirmationError::RuntimeUnavailable));
            let permit = runtime
                .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                .unwrap();
            assert!(permit.current_public_account().is_err());
        }
    }

    #[test]
    fn promotion_keeps_the_operation_slot_occupied_until_signing_completion() {
        let (runtime, sender) = unlocked_runtime(89);
        let ceremony = ConfirmingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };
        let observer = OccupiedSigningSlotObserver { runtime: &runtime };
        let pending = pending(&runtime, &sender, &"8".repeat(64));

        engine
            .run_fail_closed(|| {
                sign_after_native_approval_with_observer_for_test(
                    pending,
                    NativeConfirmationApproval::issue(),
                    &observer,
                )
                .map_err(map_signing_error)
            })
            .unwrap();

        runtime
            .begin_operation(MAIN, WalletOperationKind::PreparePreview)
            .unwrap();
    }

    #[test]
    fn lifecycle_revocation_at_every_signing_transition_suppresses_the_result() {
        for (index, stage) in [
            SigningCoordinatorStage::Promoted,
            SigningCoordinatorStage::BeforeSeedAccess,
            SigningCoordinatorStage::SeedAccountDerivation,
            SigningCoordinatorStage::SignatureConstruction,
            SigningCoordinatorStage::SignatureVerification,
            SigningCoordinatorStage::AfterSignatureVerification,
            SigningCoordinatorStage::BeforeCompletion,
        ]
        .into_iter()
        .enumerate()
        {
            let (runtime, sender) = unlocked_runtime(100_u8.saturating_add(index as u8));
            let runtime = Arc::new(runtime);
            let ceremony = ConfirmingCeremony;
            let engine = WalletTransactionConfirmationEngine {
                runtime: &runtime,
                ceremony: &ceremony,
            };
            let observer = RevokingSigningObserver::new(Arc::clone(&runtime), stage);
            let pending = pending(&runtime, &sender, &"a".repeat(64));

            let result = engine.run_fail_closed(|| {
                sign_after_native_approval_with_observer_for_test(
                    pending,
                    NativeConfirmationApproval::issue(),
                    &observer,
                )
                .map_err(map_signing_error)
            });
            observer.join();
            assert!(result.is_err());
            let permit = runtime
                .begin_operation(MAIN, WalletOperationKind::PreparePreview)
                .unwrap();
            assert!(permit.current_public_account().is_err());
        }
    }

    #[test]
    fn every_required_signing_core_checkpoint_rejects_stop_or_generation_replacement() {
        for identity_call in 3..=7 {
            for fault in [
                Ok([0x43; 32]),
                Err(WalletCoreClientError::CoreUnavailable),
                Err(WalletCoreClientError::CoreIdentityChanged),
                Err(WalletCoreClientError::PeerIdentityRejected),
            ] {
                let (runtime, sender) =
                    unlocked_runtime(120_u8.saturating_add(identity_call as u8));
                let ceremony = ConfirmingCeremony;
                let engine = WalletTransactionConfirmationEngine {
                    runtime: &runtime,
                    ceremony: &ceremony,
                };
                let replacement = fault.is_ok().then_some(identity_call);
                let identity_error = fault.err().map(|error| (identity_call, error));
                let pending = pending_with_core_change(
                    &runtime,
                    &sender,
                    &"b".repeat(64),
                    None,
                    replacement,
                    identity_error,
                );

                assert_eq!(
                    engine.confirm_pending(pending),
                    Err(WalletConfirmationError::CoreUnavailable)
                );
            }
        }
    }

    #[test]
    fn approval_completion_and_signed_artifact_are_single_use() {
        reset_signed_artifact_drop_count_for_test();
        let (runtime, sender) = unlocked_runtime(150);
        let ceremony = ConfirmingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };

        engine
            .confirm_pending(pending(&runtime, &sender, &"c".repeat(64)))
            .unwrap();
        assert_eq!(signed_artifact_drop_count_for_test(), 1);

        let confirmation_source = include_str!("transaction_confirmation.rs");
        let normalized_confirmation_source = confirmation_source.replace("\r\n", "\n");
        let confirmation_production = normalized_confirmation_source
            .split("\n#[cfg(test)]\nmod tests {")
            .next()
            .unwrap();
        assert_eq!(
            confirmation_production
                .match_indices("NativeConfirmationApproval::issue()")
                .count(),
            1
        );
        assert!(confirmation_production
            .contains("pub(in crate::wallet) struct NativeConfirmationApproval {"));
        assert!(!confirmation_production.contains("impl Clone for NativeConfirmationApproval"));

        let runtime_source = include_str!("runtime.rs");
        let normalized_runtime_source = runtime_source.replace("\r\n", "\n");
        assert!(normalized_runtime_source.contains("pub(in crate::wallet) fn complete<T>(mut self"));
        let signing_source = include_str!("signing.rs");
        assert!(signing_source.contains("permit.complete(artifact)"));
        assert!(signing_source.contains("drop(artifact)"));
        assert!(!signing_source.contains("pub struct SignedTransferArtifact"));
        assert!(!signing_source.contains("Result<SignedTransferArtifact"));
    }

    #[test]
    fn signing_errors_panics_and_diagnostics_exclude_privacy_canaries() {
        let privacy_canaries = [
            "ab".repeat(32),
            "cd".repeat(32),
            "ef".repeat(32),
            "12".repeat(64),
            "wallet_nonce=18446744073709551615".to_string(),
        ];
        for error in [
            WalletConfirmationError::Cancelled,
            WalletConfirmationError::AuthorityRevoked,
            WalletConfirmationError::NativeUiUnavailable,
            WalletConfirmationError::PreviewUnavailable,
            WalletConfirmationError::CoreUnavailable,
            WalletConfirmationError::OperationInProgress,
            WalletConfirmationError::RuntimeUnavailable,
            WalletConfirmationError::SigningUnavailable,
        ] {
            for canary in &privacy_canaries {
                assert!(!error.code().contains(canary));
            }
        }

        let signing_source = include_str!("signing.rs");
        for forbidden_sink in ["println!", "eprintln!", "dbg!", "tracing::", "log::"] {
            assert!(!signing_source.contains(forbidden_sink));
        }
        let panic_source = include_str!("panic_policy.rs");
        for canary in &privacy_canaries {
            assert!(!panic_source.contains(canary));
        }
    }

    #[test]
    fn native_display_buffers_are_explicitly_wiped() {
        let sender = "2".repeat(64);
        let recipient = "3".repeat(64);
        let transaction_id = "4".repeat(64);
        let fields = TransferConfirmationFields {
            sender_address: &sender,
            recipient_address: &recipient,
            amount_raw_units: 2_500_000_000,
            charged_fee_raw_units: 1,
            fee_limit_raw_units: 201,
            total_debit_raw_units: 2_500_000_001,
            nonce: 7,
            transaction_id: &transaction_id,
        };
        let mut display = ConfirmationDisplayBuffers::new(fields);
        assert!(!display.is_wiped());
        display.wipe();
        assert!(display.is_wiped());
    }

    #[test]
    fn native_window_rejects_missing_owner_and_dangerous_text_service_routes() {
        assert_eq!(
            NativeTransactionConfirmationCeremony::new(0).err().unwrap(),
            NativeConfirmationError::NativeUiUnavailable
        );
        for message in [
            WM_IME_STARTCOMPOSITION,
            WM_IME_ENDCOMPOSITION,
            WM_IME_COMPOSITION,
            WM_IME_NOTIFY,
            WM_IME_CONTROL,
            WM_IME_COMPOSITIONFULL,
            WM_IME_SELECT,
            WM_IME_CHAR,
            WM_IME_REQUEST,
            WM_IME_KEYDOWN,
            WM_IME_KEYUP,
            WM_INPUTLANGCHANGEREQUEST,
            WM_INPUTLANGCHANGE,
        ] {
            assert!(is_blocked_text_service_message(message));
        }
        assert!(!is_blocked_text_service_message(WM_IME_SETCONTEXT));
    }

    #[test]
    fn unexpected_character_input_wipes_and_closes_the_native_confirmation() {
        let window = hidden_test_window();
        assert!(!window.is_null());

        let mut state = dialog_state_for_test();
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                (&mut state as *mut ConfirmationDialogState) as isize,
            );
            confirmation_window_proc(window, WM_CHAR, usize::from(b'x'), 0);
        }
        assert_eq!(state.outcome, ConfirmationOutcome::Failed);
        assert!(state.display.is_wiped());
        assert_eq!(unsafe { IsWindow(window) }, 0);
    }

    #[test]
    fn ime_focus_transition_requires_the_window_to_remain_disassociated() {
        let window = hidden_test_window();
        assert!(!window.is_null());
        assert!(disable_text_services(window));
        let mut state = dialog_state_for_test();
        let (confirm, cancel) = create_buttons(window).unwrap();
        state.confirm_button = confirm;
        state.cancel_button = cancel;
        assert!(confirmation_input_contexts_are_absent(window, &state));

        let child_context = unsafe { ImmCreateContext() };
        assert!(!child_context.is_null());
        assert!(unsafe { ImmAssociateContext(confirm, child_context) }.is_null());
        assert!(!confirmation_input_contexts_are_absent(window, &state));
        assert_eq!(
            unsafe { ImmAssociateContext(confirm, null_mut()) },
            child_context
        );
        assert_ne!(unsafe { ImmDestroyContext(child_context) }, 0);
        assert!(confirmation_input_contexts_are_absent(window, &state));
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                (&mut state as *mut ConfirmationDialogState) as isize,
            );
            confirmation_window_proc(window, WM_IME_SETCONTEXT, 1, 0);
        }
        assert_eq!(state.outcome, ConfirmationOutcome::Pending);
        assert_ne!(unsafe { IsWindow(window) }, 0);
        unsafe { DestroyWindow(window) };

        let associated_window = hidden_test_window();
        assert!(!associated_window.is_null());
        assert!(disable_text_services(associated_window));
        let input_context = unsafe { ImmCreateContext() };
        assert!(!input_context.is_null());
        assert!(unsafe { ImmAssociateContext(associated_window, input_context) }.is_null());
        let mut associated_state = dialog_state_for_test();
        unsafe {
            SetWindowLongPtrW(
                associated_window,
                GWLP_USERDATA,
                (&mut associated_state as *mut ConfirmationDialogState) as isize,
            );
            confirmation_window_proc(associated_window, WM_IME_SETCONTEXT, 1, 0);
        }
        assert_eq!(associated_state.outcome, ConfirmationOutcome::Failed);
        assert_eq!(unsafe { IsWindow(associated_window) }, 0);
        assert_ne!(unsafe { ImmDestroyContext(input_context) }, 0);
    }

    #[test]
    fn confirmation_stays_disabled_until_verified_display_and_exact_fresh_command() {
        let window = hidden_test_window();
        assert!(!window.is_null());
        let (confirm, cancel) = create_buttons(window).unwrap();
        let mut state = dialog_state_for_test();
        state.confirm_button = confirm;
        state.cancel_button = cancel;

        assert_eq!(unsafe { IsWindowEnabled(confirm) }, 0);
        state.fresh_input_time = Some(unsafe { GetMessageTime() } as u32);
        state.fresh_input_device = Some(ConfirmationInputDevice::Keyboard);
        assert!(!consume_fresh_confirmation_command(&mut state, confirm));

        state.display_verified = true;
        state.display_verified_at = Some(100);
        unsafe { EnableWindow(confirm, 1) };
        state.fresh_input_time = Some(unsafe { GetMessageTime() } as u32);
        state.fresh_input_device = Some(ConfirmationInputDevice::Keyboard);
        assert!(!consume_fresh_confirmation_command(&mut state, cancel));
        state.fresh_input_time = Some(unsafe { GetMessageTime() } as u32);
        state.fresh_input_device = Some(ConfirmationInputDevice::Keyboard);
        assert!(consume_fresh_confirmation_command(&mut state, confirm));
        assert!(state.fresh_input_time.is_none());
        assert!(state.fresh_input_device.is_none());

        unsafe { DestroyWindow(window) };
    }

    #[test]
    fn arming_fails_closed_when_focus_has_an_associated_input_context() {
        let window = hidden_test_window();
        assert!(!window.is_null());
        assert!(disable_text_services(window));
        let (confirm, cancel) = create_buttons(window).unwrap();
        let mut state = dialog_state_for_test();
        state.confirm_button = confirm;
        state.cancel_button = cancel;

        let input_context = unsafe { ImmCreateContext() };
        assert!(!input_context.is_null());
        assert!(unsafe { ImmAssociateContext(confirm, input_context) }.is_null());
        assert!(!arm_confirmation(window, &mut state));
        assert!(!state.display_verified);
        assert!(state.display_verified_at.is_none());

        fail_closed_window(window, &mut state);
        assert_eq!(state.outcome, ConfirmationOutcome::Failed);
        assert!(state.display.is_wiped());
        assert_eq!(unsafe { IsWindow(window) }, 0);
        assert_ne!(unsafe { ImmDestroyContext(input_context) }, 0);
    }

    #[test]
    fn final_confirm_fails_closed_when_context_appears_after_the_last_poll() {
        let window = hidden_test_window();
        assert!(!window.is_null());
        assert!(disable_text_services(window));
        let (confirm, cancel) = create_buttons(window).unwrap();
        let mut state = dialog_state_for_test();
        state.confirm_button = confirm;
        state.cancel_button = cancel;
        state.display_verified = true;
        state.display_verified_at = Some(unsafe { GetTickCount() }.wrapping_sub(1));
        state.fresh_input_time = Some(unsafe { GetMessageTime() } as u32);
        unsafe { EnableWindow(confirm, 1) };
        assert!(confirmation_input_contexts_are_absent(window, &state));

        let input_context = unsafe { ImmCreateContext() };
        assert!(!input_context.is_null());
        assert!(unsafe { ImmAssociateContext(window, input_context) }.is_null());
        assert!(!confirmation_input_contexts_are_absent(window, &state));
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                (&mut state as *mut ConfirmationDialogState) as isize,
            );
            confirmation_window_proc(
                window,
                WM_COMMAND,
                CONFIRM_BUTTON_ID | ((BN_CLICKED as usize) << 16),
                confirm as LPARAM,
            );
        }

        assert_eq!(state.outcome, ConfirmationOutcome::Failed);
        assert!(state.display.is_wiped());
        assert_eq!(unsafe { IsWindow(window) }, 0);
        assert_ne!(unsafe { ImmDestroyContext(input_context) }, 0);
    }

    #[test]
    fn only_a_complete_post_display_hardware_press_creates_fresh_input() {
        let window = hidden_test_window();
        assert!(!window.is_null());
        let (confirm, cancel) = create_buttons(window).unwrap();
        let mut state = dialog_state_for_test();
        state.confirm_button = confirm;
        state.cancel_button = cancel;

        let hardware_keyboard = INPUT_MESSAGE_SOURCE {
            deviceType: IMDT_KEYBOARD,
            originId: IMO_HARDWARE,
        };
        let injected_mouse = INPUT_MESSAGE_SOURCE {
            deviceType: IMDT_MOUSE,
            originId: IMO_INJECTED,
        };
        let hardware_mouse = INPUT_MESSAGE_SOURCE {
            deviceType: IMDT_MOUSE,
            originId: IMO_HARDWARE,
        };
        let mut message = MSG {
            hwnd: confirm,
            message: WM_KEYDOWN,
            wParam: usize::from(VK_RETURN),
            lParam: 0,
            time: 101,
            ..MSG::default()
        };

        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        message.message = WM_KEYUP;
        message.time = 102;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert!(state.fresh_input_time.is_none());

        state.display_verified = true;
        state.display_verified_at = Some(100);
        message.message = WM_KEYDOWN;
        message.lParam = 0;
        message.time = 99;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        message.message = WM_KEYUP;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert!(state.fresh_input_time.is_none());

        message.message = WM_KEYDOWN;
        message.lParam = 1_isize << 30;
        message.time = 101;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        message.message = WM_KEYUP;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert!(state.fresh_input_time.is_none());

        message.message = WM_LBUTTONDOWN;
        message.lParam = 0;
        apply_confirmation_input_source(&mut state, &message, Some(injected_mouse));
        message.message = WM_LBUTTONUP;
        apply_confirmation_input_source(&mut state, &message, Some(injected_mouse));
        assert!(state.fresh_input_time.is_none());

        message.message = WM_KEYDOWN;
        message.wParam = usize::from(VK_RETURN);
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        message.message = WM_LBUTTONUP;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_mouse));
        assert!(state.fresh_input_time.is_none());

        message.message = WM_LBUTTONDOWN;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_mouse));
        message.message = WM_KEYUP;
        message.wParam = usize::from(VK_RETURN);
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert!(state.fresh_input_time.is_none());

        message.message = WM_KEYDOWN;
        message.wParam = usize::from(VK_RETURN);
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        message.message = WM_KEYUP;
        message.wParam = usize::from(VK_SPACE);
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert!(state.fresh_input_time.is_none());

        message.message = WM_KEYDOWN;
        message.wParam = usize::from(VK_SPACE);
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        message.message = WM_KEYUP;
        message.wParam = usize::from(VK_RETURN);
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert!(state.fresh_input_time.is_none());

        message.message = WM_KEYDOWN;
        message.wParam = usize::from(VK_RETURN);
        message.time = 201;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        message.message = WM_KEYUP;
        message.time = 202;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert_eq!(state.fresh_input_time, Some(202));

        message.hwnd = cancel;
        message.message = WM_KEYDOWN;
        apply_confirmation_input_source(&mut state, &message, Some(hardware_keyboard));
        assert!(state.fresh_input_time.is_none());

        unsafe { DestroyWindow(window) };
    }

    #[test]
    fn completed_hardware_keyboard_release_triggers_exact_confirmation() {
        let window = hidden_test_window();
        assert!(!window.is_null());
        assert!(disable_text_services(window));
        let (confirm, cancel) = create_buttons(window).unwrap();
        let mut state = dialog_state_for_test();
        state.confirm_button = confirm;
        state.cancel_button = cancel;
        state.display_verified = true;
        let message_time = unsafe { GetMessageTime() } as u32;
        state.display_verified_at = Some(message_time.wrapping_sub(1));
        unsafe {
            EnableWindow(confirm, 1);
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                (&mut state as *mut ConfirmationDialogState) as isize,
            );
        }
        let hardware_keyboard = INPUT_MESSAGE_SOURCE {
            deviceType: IMDT_KEYBOARD,
            originId: IMO_HARDWARE,
        };
        let mut message = MSG {
            hwnd: confirm,
            message: WM_KEYDOWN,
            wParam: usize::from(VK_RETURN),
            lParam: 0,
            time: message_time,
            ..MSG::default()
        };
        assert!(!apply_confirmation_input_source(
            &mut state,
            &message,
            Some(hardware_keyboard)
        ));
        assert!(!consume_fresh_confirmation_command(&mut state, confirm));
        assert!(state.press_started_after_display == Some(ConfirmationPress::Keyboard(VK_RETURN)));
        message.message = WM_KEYUP;
        assert!(apply_confirmation_input_source(
            &mut state,
            &message,
            Some(hardware_keyboard)
        ));

        unsafe {
            confirmation_window_proc(
                window,
                WM_COMMAND,
                CONFIRM_BUTTON_ID | ((BN_CLICKED as usize) << 16),
                confirm as LPARAM,
            );
        }
        assert_eq!(state.outcome, ConfirmationOutcome::Confirmed);
        assert!(state.display.is_wiped());
        assert_eq!(unsafe { IsWindow(window) }, 0);
    }

    #[test]
    fn text_rendering_rejects_content_that_does_not_fit_its_verified_bounds() {
        let device = unsafe { CreateCompatibleDC(null_mut()) };
        assert!(!device.is_null());
        let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
        assert!(!font.is_null());
        let previous = unsafe { SelectObject(device, font) };
        assert!(!previous.is_null());
        let text: Vec<u16> = "Complete verified transaction value"
            .encode_utf16()
            .collect();
        let mut too_small = RECT {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        };
        assert!(!draw_text_checked(
            device,
            &text,
            &mut too_small,
            DT_LEFT | DT_SINGLELINE
        ));
        let mut sufficient = RECT {
            left: 0,
            top: 0,
            right: 800,
            bottom: 40,
        };
        assert!(draw_text_checked(
            device,
            &text,
            &mut sufficient,
            DT_LEFT | DT_SINGLELINE
        ));
        unsafe {
            SelectObject(device, previous);
            DeleteDC(device);
        }
    }

    #[test]
    fn complete_confirmation_layout_fits_and_arms_on_windows() {
        let device = unsafe { CreateCompatibleDC(null_mut()) };
        assert!(!device.is_null());
        let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
        let previous = unsafe { SelectObject(device, font) };
        assert!(!previous.is_null());
        assert_ne!(
            unsafe { SetBkMode(device, i32::try_from(TRANSPARENT).unwrap()) },
            0
        );
        let state = dialog_state_for_test();
        assert!(draw_literal(
            device,
            "Verify every value. This confirms only this exact unsigned transaction; it does not sign or submit it.",
            RECT { left: 24, top: 18, right: 816, bottom: 54 },
            DT_LEFT | DT_WORDBREAK,
        ));
        for (label, top) in [
            ("Sender", 70),
            ("Recipient", 142),
            ("Amount", 214),
            ("Fees", 270),
            ("Total debit", 328),
            ("Nonce", 384),
            ("Transaction identifier", 440),
        ] {
            assert!(draw_label(device, label, top), "label did not fit: {label}");
        }
        for (name, text, top, bottom) in [
            ("sender", state.display.sender.as_slice(), 92, 132),
            ("recipient", state.display.recipient.as_slice(), 164, 204),
            ("amount", state.display.amount.as_slice(), 236, 260),
            ("fee", state.display.fee.as_slice(), 292, 318),
            ("total", state.display.total.as_slice(), 350, 374),
            ("nonce", state.display.nonce.as_slice(), 406, 430),
            (
                "transaction_id",
                state.display.transaction_id.as_slice(),
                462,
                502,
            ),
        ] {
            assert!(
                draw_buffer(device, text, top, bottom),
                "value did not fit: {name}"
            );
        }
        assert!(draw_literal(
            device,
            "Mined transactions can reorganize and are never presented as irreversible.",
            RECT {
                left: 24,
                top: 514,
                right: 816,
                bottom: 544
            },
            DT_LEFT | DT_WORDBREAK,
        ));
        unsafe {
            SelectObject(device, previous);
            DeleteDC(device);
        }

        let window = hidden_test_window();
        assert!(!window.is_null());
        unsafe { ShowWindow(window, SW_SHOW) };
        let (confirm, cancel) = create_buttons(window).unwrap();
        let mut state = dialog_state_for_test();
        state.confirm_button = confirm;
        state.cancel_button = cancel;
        assert!(disable_text_services(window));
        assert!(arm_confirmation(window, &mut state));
        unsafe { DestroyWindow(window) };
    }

    #[test]
    fn post_foreground_step_restores_only_an_already_armed_confirm_control() {
        let window = hidden_test_window();
        assert!(!window.is_null());
        assert!(disable_text_services(window));
        let (confirm, cancel) = create_buttons(window).unwrap();
        let mut state = dialog_state_for_test();
        state.confirm_button = confirm;
        state.cancel_button = cancel;

        unsafe {
            ShowWindow(window, SW_SHOW);
            SetFocus(window);
        }
        assert!(restore_armed_confirmation_focus(window, &state));
        assert_eq!(unsafe { GetFocus() }, window);

        assert!(arm_confirmation(window, &mut state));
        assert_eq!(unsafe { GetFocus() }, confirm);
        unsafe { SetFocus(window) };
        assert_eq!(unsafe { GetFocus() }, window);
        assert!(restore_armed_confirmation_focus(window, &state));
        assert_eq!(unsafe { GetFocus() }, confirm);
        assert!(confirmation_input_contexts_are_absent(window, &state));
        unsafe { DestroyWindow(window) };
    }

    /// Manual, non-production qualification probe for the exact native confirmation window.
    ///
    /// The harness uses only fixed public test values and the private `cfg(test)` entry point. It
    /// cannot create or unlock a wallet, access a seed, sign, submit, or acquire Tauri authority.
    #[test]
    #[ignore = "requires a human operator to qualify real Windows confirmation input and layout"]
    fn real_windows_transaction_confirmation_operator_harness() {
        let scenario = std::env::var("VISION_WALLET_CONFIRMATION_SCENARIO").expect(
            "set VISION_WALLET_CONFIRMATION_SCENARIO to mouse, keyboard, held-enter, injected-enter, cancel, or revoke",
        );
        assert!(matches!(
            scenario.as_str(),
            "mouse" | "keyboard" | "held-enter" | "injected-enter" | "cancel" | "revoke"
        ));
        let evidence_label = std::env::var("VISION_WALLET_CONFIRMATION_EVIDENCE_LABEL")
            .ok()
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
            .expect("set a 1-64 character alphanumeric, dash, or underscore evidence label");
        let input_profile = std::env::var("VISION_WALLET_CONFIRMATION_INPUT_PROFILE").expect(
            "set VISION_WALLET_CONFIRMATION_INPUT_PROFILE to us, microsoft-pinyin, or microsoft-japanese",
        );
        let expected_layout = match input_profile.as_str() {
            "us" => "00000409",
            "microsoft-pinyin" => "00000804",
            "microsoft-japanese" => "00000411",
            _ => panic!("unsupported qualification input profile"),
        };

        establish_production_dpi_context_for_qualification();
        QUALIFICATION_CONFIRMATION_DPI.store(0, AtomicOrdering::SeqCst);
        QUALIFICATION_ACCEPTED_INPUT_DEVICE.store(0, AtomicOrdering::SeqCst);
        QUALIFICATION_CONFIRM_FOCUS_VERIFIED.store(false, AtomicOrdering::SeqCst);

        let mut keyboard_layout = [0_u16; 9];
        assert_ne!(
            unsafe { GetKeyboardLayoutNameW(keyboard_layout.as_mut_ptr()) },
            0,
            "active keyboard layout could not be recorded",
        );
        let keyboard_layout = String::from_utf16(&keyboard_layout[..8]).unwrap();
        assert!(
            keyboard_layout.eq_ignore_ascii_case(expected_layout),
            "active keyboard layout does not match the declared qualification input profile"
        );

        let owner = hidden_test_window();
        assert!(!owner.is_null());
        let owner_title = wide_null("Vision Wallet Confirmation Qualification Harness");
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW(
                owner,
                owner_title.as_ptr(),
            );
            ShowWindow(owner, SW_SHOW);
        }

        println!(
            "VISION_WALLET_CONFIRMATION_QUALIFICATION_READY scenario={scenario} label={evidence_label} input_profile={input_profile} keyboard_layout={keyboard_layout} expected_dpi_context=PerMonitorV2 pid={}",
            std::process::id()
        );
        match scenario.as_str() {
            "mouse" => println!(
                "Inspect every value and button for clipping, then click Confirm exact transaction once with a physical mouse."
            ),
            "keyboard" => println!(
                "Inspect every value and button for clipping, then press and release Enter once on the focused Confirm control."
            ),
            "held-enter" => println!(
                "Keep the command-launch Enter key held through dialog display. Repeats and release must not confirm. Then press and release Enter once freshly."
            ),
            "injected-enter" => println!(
                "Do not touch mouse or keyboard until VISION_WALLET_INJECTED_INPUT_REJECTED appears; then confirm once with physical input."
            ),
            "cancel" => println!("Inspect the complete dialog, then click Cancel or close the window."),
            "revoke" => println!(
                "Do not interact. Test authority will be revoked and the dialog must close without confirmation."
            ),
            _ => unreachable!(),
        }

        let authority_current = Arc::new(AtomicBool::new(true));
        let dialog_completed = Arc::new(AtomicBool::new(false));
        let injected_input_accepted = Arc::new(AtomicBool::new(false));
        let helper_failed = Arc::new(AtomicBool::new(false));
        let helper = match scenario.as_str() {
            "revoke" => {
                let authority_current = Arc::clone(&authority_current);
                Some(thread::spawn(move || {
                    thread::sleep(Duration::from_secs(2));
                    authority_current.store(false, AtomicOrdering::SeqCst);
                }))
            }
            "injected-enter" => {
                let dialog_completed = Arc::clone(&dialog_completed);
                let injected_input_accepted = Arc::clone(&injected_input_accepted);
                let helper_failed = Arc::clone(&helper_failed);
                let authority_current = Arc::clone(&authority_current);
                Some(thread::spawn(move || {
                    thread::sleep(Duration::from_secs(2));
                    let title = wide_null("Confirm Vision Transaction");
                    let dialog = unsafe { FindWindowW(null(), title.as_ptr()) };
                    if dialog.is_null() || unsafe { GetForegroundWindow() } != dialog {
                        helper_failed.store(true, AtomicOrdering::SeqCst);
                        authority_current.store(false, AtomicOrdering::SeqCst);
                        return;
                    }
                    let inputs = [
                        INPUT {
                            r#type: INPUT_KEYBOARD,
                            Anonymous: INPUT_0 {
                                ki: KEYBDINPUT {
                                    wVk: VK_RETURN,
                                    wScan: 0,
                                    dwFlags: 0,
                                    time: 0,
                                    dwExtraInfo: 0,
                                },
                            },
                        },
                        INPUT {
                            r#type: INPUT_KEYBOARD,
                            Anonymous: INPUT_0 {
                                ki: KEYBDINPUT {
                                    wVk: VK_RETURN,
                                    wScan: 0,
                                    dwFlags: KEYEVENTF_KEYUP,
                                    time: 0,
                                    dwExtraInfo: 0,
                                },
                            },
                        },
                    ];
                    let expected = u32::try_from(inputs.len()).unwrap();
                    if unsafe {
                        SendInput(
                            expected,
                            inputs.as_ptr(),
                            i32::try_from(size_of::<INPUT>()).unwrap(),
                        )
                    } != expected
                    {
                        helper_failed.store(true, AtomicOrdering::SeqCst);
                        authority_current.store(false, AtomicOrdering::SeqCst);
                        return;
                    }
                    thread::sleep(Duration::from_secs(3));
                    if dialog_completed.load(AtomicOrdering::SeqCst) {
                        injected_input_accepted.store(true, AtomicOrdering::SeqCst);
                    } else {
                        println!(
                            "VISION_WALLET_INJECTED_INPUT_REJECTED physical_input_now_allowed"
                        );
                    }
                }))
            }
            _ => None,
        };

        let sender = "1".repeat(64);
        let recipient = "2".repeat(64);
        let transaction_id = "3".repeat(64);
        let result = run_native_confirmation(
            owner,
            TransferConfirmationFields {
                sender_address: &sender,
                recipient_address: &recipient,
                amount_raw_units: 1_234_567_890,
                charged_fee_raw_units: 1,
                fee_limit_raw_units: 201,
                total_debit_raw_units: 1_234_567_891,
                nonce: 42,
                transaction_id: &transaction_id,
            },
            &|| authority_current.load(AtomicOrdering::SeqCst),
        );
        dialog_completed.store(true, AtomicOrdering::SeqCst);
        if let Some(helper) = helper {
            helper.join().expect("qualification helper failed");
        }
        unsafe { DestroyWindow(owner) };
        let confirmation_dpi = QUALIFICATION_CONFIRMATION_DPI.load(AtomicOrdering::SeqCst);
        assert!(
            confirmation_dpi > 0,
            "actual confirmation-window DPI was not recorded under the production context"
        );

        assert!(
            !injected_input_accepted.load(AtomicOrdering::SeqCst),
            "ordinary non-UIAccess SendInput completed confirmation"
        );
        assert!(
            !helper_failed.load(AtomicOrdering::SeqCst),
            "qualification helper could not identify the foreground dialog or deliver input"
        );
        let accepted_input_device = QUALIFICATION_ACCEPTED_INPUT_DEVICE.load(Ordering::SeqCst);
        assert!(
            QUALIFICATION_CONFIRM_FOCUS_VERIFIED.load(Ordering::SeqCst),
            "exact Confirm control focus was never verified after foreground activation"
        );
        match scenario.as_str() {
            "mouse" => {
                assert_eq!(result, Ok(()));
                assert_eq!(
                    accepted_input_device, 2,
                    "mouse scenario accepted non-mouse input"
                );
            }
            "keyboard" | "held-enter" => {
                assert_eq!(result, Ok(()));
                assert_eq!(
                    accepted_input_device, 1,
                    "keyboard scenario accepted non-keyboard input"
                );
            }
            "injected-enter" => {
                assert_eq!(result, Ok(()));
                assert_ne!(
                    accepted_input_device, 0,
                    "physical completion input was not recorded"
                );
            }
            "cancel" => {
                assert_eq!(result, Err(NativeConfirmationError::Cancelled));
                assert_eq!(accepted_input_device, 0);
            }
            "revoke" => {
                assert_eq!(result, Err(NativeConfirmationError::AuthorityRevoked));
                assert_eq!(accepted_input_device, 0);
            }
            _ => unreachable!(),
        }
        println!(
            "VISION_WALLET_CONFIRMATION_QUALIFICATION_PASS scenario={scenario} label={evidence_label} input_profile={input_profile} keyboard_layout={keyboard_layout} dpi_context=PerMonitorV2 confirmation_dpi={confirmation_dpi} confirm_focus_verified=true accepted_input_device={accepted_input_device}"
        );
    }

    #[test]
    fn confirmation_source_has_no_editable_control_or_forbidden_authority() {
        let source = include_str!("transaction_confirmation.rs");
        let normalized_source = source.replace("\r\n", "\n");
        let production = normalized_source
            .split("\n#[cfg(test)]\nmod tests {")
            .next()
            .unwrap();
        assert!(!production.contains(&["#[tauri", "::command]"].concat()));
        assert!(!production.contains("generate_handler"));
        assert!(!production.contains("WalletSeed"));
        assert!(!production.contains("sign_cash_transfer"));
        assert!(!production.contains(&["PO", "ST "].concat()));
        assert!(!production.contains("WalletSubmission"));
        assert!(!production.contains("TcpStream"));
        assert!(!production.contains(&["PO", "ST /transactions"].concat()));
        assert!(!production.contains("wide_null(\"EDIT\")"));
        assert!(!production.contains("wide_null(\"STATIC\")"));
        assert!(!production.contains("IACE_CHILDREN"));
        assert!(production.contains("ImmAssociateContextEx"));
        assert!(production.contains("ImmGetContext"));
        assert!(production.contains("ImmReleaseContext"));
        assert!(production.contains("NativeConfirmationApproval::issue()"));
        assert!(!production.contains("SetFocus(dialog)"));
        assert!(production.contains("restore_armed_confirmation_focus(dialog, &state)"));
        let preview_source = include_str!("preview.rs");
        assert!(!preview_source.contains("pub(in crate::wallet) fn confirm("));
        assert!(preview_source
            .contains("approval: super::transaction_confirmation::NativeConfirmationApproval"));
        assert!(!preview_source.contains("ConfirmedTransferIntent"));
        let signing_source = include_str!("signing.rs");
        assert!(!signing_source.contains(&["#[tauri", "::command]"].concat()));
        assert!(!signing_source.contains(&["PO", "ST /transactions"].concat()));
        assert!(!signing_source.contains("TcpStream"));
        assert!(!signing_source.contains("pub(crate)"));
    }
}
