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
        ConfirmedTransferIntent, PendingTransferConfirmation, TransferConfirmationFields,
        WalletPreviewError, WalletTransactionPreviewEngine,
    },
    runtime::WalletRuntimeState,
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
        COLOR_WINDOW, DEFAULT_GUI_FONT, DT_LEFT, DT_SINGLELINE, DT_WORDBREAK, PAINTSTRUCT,
        TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::{
            Ime::{ImmAssociateContextEx, ImmGetContext, ImmReleaseContext},
            KeyboardAndMouse::{EnableWindow, SetFocus},
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
            GetWindowLongPtrW, GetWindowRect, IsDialogMessageW, IsWindow, KillTimer, LoadCursorW,
            PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
            ShowWindow, TranslateMessage, UnregisterClassW, BN_CLICKED, BS_DEFPUSHBUTTON,
            BS_PUSHBUTTON, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, IDC_ARROW, MSG,
            SW_SHOW, WM_CHAR, WM_CLEAR, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_COPY, WM_CREATE,
            WM_CUT, WM_GETTEXT, WM_GETTEXTLENGTH, WM_IME_CHAR, WM_IME_COMPOSITION,
            WM_IME_COMPOSITIONFULL, WM_IME_CONTROL, WM_IME_ENDCOMPOSITION, WM_IME_KEYDOWN,
            WM_IME_KEYUP, WM_IME_NOTIFY, WM_IME_REQUEST, WM_IME_SELECT, WM_IME_SETCONTEXT,
            WM_IME_STARTCOMPOSITION, WM_INPUTLANGCHANGE, WM_INPUTLANGCHANGEREQUEST, WM_NCCREATE,
            WM_NCDESTROY, WM_PAINT, WM_PASTE, WM_SETTEXT, WM_TIMER, WNDCLASSEXW, WS_CAPTION,
            WS_CHILD, WS_EX_DLGMODALFRAME, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
        },
    },
};
use zeroize::{Zeroize, Zeroizing};

const DIALOG_WIDTH: i32 = 860;
const DIALOG_HEIGHT: i32 = 650;
const AUTHORITY_TIMER_ID: usize = 1;
const AUTHORITY_TIMER_MS: u32 = 250;
const CONFIRM_BUTTON_ID: usize = 2001;
const CANCEL_BUTTON_ID: usize = 2002;

static CLASS_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletConfirmationError {
    Cancelled,
    AuthorityRevoked,
    NativeUiUnavailable,
    PreviewUnavailable,
    CoreUnavailable,
    OperationInProgress,
    RuntimeUnavailable,
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
    ) -> Result<ConfirmedTransferIntent, WalletConfirmationError> {
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
    ) -> Result<ConfirmedTransferIntent, WalletConfirmationError> {
        self.run_fail_closed(|| self.confirm_pending_inner(pending))
    }

    fn confirm_pending_inner<S: WalletCoreReadSource>(
        &self,
        pending: PendingTransferConfirmation<'_, S>,
    ) -> Result<ConfirmedTransferIntent, WalletConfirmationError> {
        let fields = pending.fields();
        self.ceremony
            .present(fields, &|| pending.authority_is_current())
            .map_err(map_native_error)?;
        pending.confirm().map_err(map_preview_error)
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
}

impl ConfirmationDialogState {
    fn wipe(&mut self) {
        self.display.wipe();
    }
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

    let _modal_owner = DisabledOwner::new(owner_window);
    if unsafe { SetTimer(dialog, AUTHORITY_TIMER_ID, AUTHORITY_TIMER_MS, None) } == 0 {
        state.wipe();
        unsafe { DestroyWindow(dialog) };
        return Err(NativeConfirmationError::NativeUiUnavailable);
    }
    unsafe {
        ShowWindow(dialog, SW_SHOW);
        SetForegroundWindow(dialog);
        SetFocus(dialog);
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
            && (unsafe { IsWindow(owner_window) } == 0 || !authority_is_current())
        {
            state.outcome = ConfirmationOutcome::AuthorityRevoked;
            state.wipe();
            unsafe { DestroyWindow(dialog) };
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
            if !disable_text_services(window) || !create_buttons(window) {
                state.outcome = ConfirmationOutcome::Failed;
                return -1;
            }
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
                    state.outcome = ConfirmationOutcome::Confirmed;
                    state.wipe();
                    unsafe { DestroyWindow(window) };
                    0
                }
                CANCEL_BUTTON_ID => {
                    state.outcome = ConfirmationOutcome::Cancelled;
                    state.wipe();
                    unsafe { DestroyWindow(window) };
                    0
                }
                _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
            }
        }
        WM_PAINT => {
            paint_confirmation(window, state);
            0
        }
        WM_COPY | WM_CUT | WM_PASTE | WM_CLEAR | WM_CONTEXTMENU | WM_GETTEXT | WM_GETTEXTLENGTH
        | WM_SETTEXT => 0,
        WM_CHAR => fail_closed_window(window, state),
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

fn paint_confirmation(window: HWND, state: &ConfirmationDialogState) {
    let mut paint = PAINTSTRUCT::default();
    let device = unsafe { BeginPaint(window, &mut paint) };
    if device.is_null() {
        return;
    }
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    let previous = unsafe { SelectObject(device, font) };
    unsafe { SetBkMode(device, i32::try_from(TRANSPARENT).unwrap_or(1)) };

    draw_literal(
        device,
        "Verify every value. This confirms only this exact unsigned transaction; it does not sign or submit it.",
        RECT { left: 24, top: 18, right: 816, bottom: 54 },
        DT_LEFT | DT_WORDBREAK,
    );
    draw_label(device, "Sender", 70);
    draw_buffer(device, &state.display.sender, 92, 132);
    draw_label(device, "Recipient", 142);
    draw_buffer(device, &state.display.recipient, 164, 204);
    draw_label(device, "Amount", 214);
    draw_buffer(device, &state.display.amount, 236, 260);
    draw_label(device, "Fees", 270);
    draw_buffer(device, &state.display.fee, 292, 318);
    draw_label(device, "Total debit", 328);
    draw_buffer(device, &state.display.total, 350, 374);
    draw_label(device, "Nonce", 384);
    draw_buffer(device, &state.display.nonce, 406, 430);
    draw_label(device, "Transaction identifier", 440);
    draw_buffer(device, &state.display.transaction_id, 462, 502);
    draw_literal(
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

    unsafe {
        SelectObject(device, previous);
        EndPaint(window, &paint);
    }
}

fn draw_label(device: windows_sys::Win32::Graphics::Gdi::HDC, label: &str, top: i32) {
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
    );
}

fn draw_literal(
    device: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &str,
    mut bounds: RECT,
    format: u32,
) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        DrawTextW(
            device,
            wide.as_ptr(),
            i32::try_from(wide.len()).unwrap_or(i32::MAX),
            &mut bounds,
            format,
        )
    };
}

fn draw_buffer(
    device: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &[u16],
    top: i32,
    bottom: i32,
) {
    let mut bounds = RECT {
        left: 24,
        top,
        right: 816,
        bottom,
    };
    unsafe {
        DrawTextW(
            device,
            text.as_ptr(),
            i32::try_from(text.len()).unwrap_or(i32::MAX),
            &mut bounds,
            DT_LEFT | DT_WORDBREAK,
        )
    };
}

fn create_buttons(window: HWND) -> bool {
    let button_class = wide_null("BUTTON");
    let confirm_text = wide_null("Confirm exact transaction");
    let cancel_text = wide_null("Cancel");
    let confirm = create_button(
        window,
        &button_class,
        &confirm_text,
        BS_DEFPUSHBUTTON as u32,
        520,
        558,
        190,
        CONFIRM_BUTTON_ID,
    );
    let cancel = create_button(
        window,
        &button_class,
        &cancel_text,
        BS_PUSHBUTTON as u32,
        724,
        558,
        90,
        CANCEL_BUTTON_ID,
    );
    !confirm.is_null() && !cancel.is_null()
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
            | WM_IME_SETCONTEXT
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
        vault::EncryptedWalletVault,
    };
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    const MAIN: &str = "main";
    const PASSWORD: &str = "correct horse battery staple";

    struct FakeCore {
        address: String,
        identity_calls: AtomicUsize,
        panic_on_identity_call: Option<usize>,
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
            Ok([0x42; 32])
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
        let prepare_source = FakeCore {
            address: sender.to_string(),
            identity_calls: AtomicUsize::new(0),
            panic_on_identity_call: None,
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

    #[test]
    fn explicit_native_approval_releases_only_the_exact_confirmed_intent() {
        let (runtime, sender) = unlocked_runtime(31);
        let recipient = "d".repeat(64);
        let ceremony = ConfirmingCeremony;
        let engine = WalletTransactionConfirmationEngine {
            runtime: &runtime,
            ceremony: &ceremony,
        };

        let confirmed = engine
            .confirm_pending(pending(&runtime, &sender, &recipient))
            .unwrap();
        let fields = confirmed.fields_for_test();
        assert_eq!(fields.sender_address, sender);
        assert_eq!(fields.recipient_address, recipient);
        assert_eq!(fields.amount_raw_units, 2_500_000_000);
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
    fn native_window_rejects_missing_owner_and_all_text_service_routes() {
        assert_eq!(
            NativeTransactionConfirmationCeremony::new(0).err().unwrap(),
            NativeConfirmationError::NativeUiUnavailable
        );
        for message in [
            WM_IME_STARTCOMPOSITION,
            WM_IME_ENDCOMPOSITION,
            WM_IME_COMPOSITION,
            WM_IME_SETCONTEXT,
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
    }

    #[test]
    fn unexpected_character_input_wipes_and_closes_the_native_confirmation() {
        let button_class = wide_null("BUTTON");
        let window = unsafe {
            CreateWindowExW(
                0,
                button_class.as_ptr(),
                null(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null_mut(),
            )
        };
        assert!(!window.is_null());

        let sender = "5".repeat(64);
        let recipient = "6".repeat(64);
        let transaction_id = "7".repeat(64);
        let mut state = ConfirmationDialogState {
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
        };
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
    fn confirmation_source_has_no_editable_control_or_forbidden_authority() {
        let source = include_str!("transaction_confirmation.rs");
        let production = source.split("\n#[cfg(test)]\nmod tests {").next().unwrap();
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
    }
}
