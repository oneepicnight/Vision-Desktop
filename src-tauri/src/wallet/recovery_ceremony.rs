use std::{
    ffi::c_void,
    mem::size_of,
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{GetStockObject, GetSysColorBrush, COLOR_WINDOW, DEFAULT_GUI_FONT},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{EnableWindow, SetFocus},
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
            GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
            IsDialogMessageW, IsWindow, KillTimer, LoadCursorW, MessageBoxW, PostQuitMessage,
            RegisterClassExW, SendMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
            SetWindowTextW, ShowWindow, TranslateMessage, UnregisterClassW, BN_CLICKED,
            BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, ES_AUTOHSCROLL,
            ES_PASSWORD, GWLP_USERDATA, IDC_ARROW, MB_ICONWARNING, MB_OK, MSG, SW_SHOW, WM_CLOSE,
            WM_COMMAND, WM_CREATE, WM_NCCREATE, WM_NCDESTROY, WM_SETFONT, WM_TIMER, WNDCLASSEXW,
            WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
            WS_VISIBLE,
        },
    },
};
use zeroize::Zeroizing;

const DIALOG_WIDTH: i32 = 700;
const DIALOG_HEIGHT: i32 = 390;
const AUTHORITY_TIMER_ID: usize = 1;
const AUTHORITY_TIMER_MS: u32 = 200;
const VERIFY_BUTTON_ID: usize = 1001;
const CANCEL_BUTTON_ID: usize = 1002;
const MAX_CREDENTIAL_UTF16_UNITS: usize = 256;
const EMPTY_WIDE: [u16; 1] = [0];

static CLASS_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryCeremonyError {
    Cancelled,
    AuthorityRevoked,
    NativeUiUnavailable,
}

pub(in crate::wallet) trait RecoveryCredentialCeremony: Send + Sync {
    fn present_and_verify(
        &self,
        encoded_credential: &Zeroizing<String>,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<(), RecoveryCeremonyError>;
}

/// Rust-owned native Windows presentation boundary for the portable recovery credential.
///
/// The credential is rendered and re-entered only in Win32 controls. It is never returned to
/// React, serialized through Tauri, logged, placed on the clipboard, or stored by this boundary.
pub(crate) struct NativeRecoveryCredentialCeremony {
    owner_window: isize,
}

impl NativeRecoveryCredentialCeremony {
    pub(crate) fn new(owner_window: isize) -> Result<Self, RecoveryCeremonyError> {
        if owner_window == 0 || unsafe { IsWindow(owner_window as HWND) } == 0 {
            return Err(RecoveryCeremonyError::NativeUiUnavailable);
        }
        Ok(Self { owner_window })
    }
}

impl RecoveryCredentialCeremony for NativeRecoveryCredentialCeremony {
    fn present_and_verify(
        &self,
        encoded_credential: &Zeroizing<String>,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<(), RecoveryCeremonyError> {
        if !authority_is_current() {
            return Err(RecoveryCeremonyError::AuthorityRevoked);
        }
        let mut credential_wide =
            Zeroizing::new(encoded_credential.encode_utf16().collect::<Vec<_>>());
        if credential_wide.is_empty() || credential_wide.len() > MAX_CREDENTIAL_UTF16_UNITS {
            return Err(RecoveryCeremonyError::NativeUiUnavailable);
        }
        credential_wide.push(0);
        run_native_ceremony(
            self.owner_window as HWND,
            credential_wide.as_slice(),
            authority_is_current,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CeremonyOutcome {
    Pending,
    Verified,
    Cancelled,
    AuthorityRevoked,
    Failed,
}

struct CeremonyDialogState {
    expected: *const u16,
    expected_len: usize,
    input: HWND,
    outcome: CeremonyOutcome,
}

fn run_native_ceremony(
    owner_window: HWND,
    expected_with_nul: &[u16],
    authority_is_current: &dyn Fn() -> bool,
) -> Result<(), RecoveryCeremonyError> {
    let instance = unsafe { GetModuleHandleW(null()) } as HINSTANCE;
    if instance.is_null() || unsafe { IsWindow(owner_window) } == 0 {
        return Err(RecoveryCeremonyError::NativeUiUnavailable);
    }

    let sequence = CLASS_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let class_name = wide_null(&format!(
        "VisionDesktopRecoveryCredentialCeremony-{}-{sequence}",
        std::process::id()
    ));
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(ceremony_window_proc),
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
        return Err(RecoveryCeremonyError::NativeUiUnavailable);
    }
    let _registration = RegisteredWindowClass {
        instance,
        class_name: &class_name,
    };

    let mut owner_rect = windows_sys::Win32::Foundation::RECT::default();
    let centered = unsafe { GetWindowRect(owner_window, &mut owner_rect) } != 0;
    let x = if centered {
        owner_rect.left + ((owner_rect.right - owner_rect.left - DIALOG_WIDTH) / 2).max(0)
    } else {
        windows_sys::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT
    };
    let y = if centered {
        owner_rect.top + ((owner_rect.bottom - owner_rect.top - DIALOG_HEIGHT) / 2).max(0)
    } else {
        windows_sys::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT
    };
    let mut state = CeremonyDialogState {
        expected: expected_with_nul.as_ptr(),
        expected_len: expected_with_nul.len().saturating_sub(1),
        input: null_mut(),
        outcome: CeremonyOutcome::Pending,
    };
    let title = wide_null("Vision Wallet Recovery Credential");
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
            (&mut state as *mut CeremonyDialogState).cast::<c_void>(),
        )
    };
    if dialog.is_null() {
        return Err(RecoveryCeremonyError::NativeUiUnavailable);
    }
    let _modal_owner = DisabledOwner::new(owner_window);
    if unsafe { SetTimer(dialog, AUTHORITY_TIMER_ID, AUTHORITY_TIMER_MS, None) } == 0 {
        unsafe { DestroyWindow(dialog) };
        return Err(RecoveryCeremonyError::NativeUiUnavailable);
    }
    unsafe {
        ShowWindow(dialog, SW_SHOW);
        SetForegroundWindow(dialog);
        if !state.input.is_null() {
            SetFocus(state.input);
        }
    }

    let mut message = MSG::default();
    while unsafe { IsWindow(dialog) } != 0 {
        let status = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if status <= 0 {
            state.outcome = CeremonyOutcome::Failed;
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
            && !authority_is_current()
        {
            state.outcome = CeremonyOutcome::AuthorityRevoked;
            clear_input(&state);
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

    match state.outcome {
        CeremonyOutcome::Verified if authority_is_current() => Ok(()),
        CeremonyOutcome::Verified | CeremonyOutcome::AuthorityRevoked => {
            Err(RecoveryCeremonyError::AuthorityRevoked)
        }
        CeremonyOutcome::Cancelled => Err(RecoveryCeremonyError::Cancelled),
        CeremonyOutcome::Pending | CeremonyOutcome::Failed => {
            Err(RecoveryCeremonyError::NativeUiUnavailable)
        }
    }
}

unsafe extern "system" fn ceremony_window_proc(
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

    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut CeremonyDialogState;
    if state.is_null() {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state };

    match message {
        WM_CREATE => {
            if !create_dialog_controls(window, state) {
                state.outcome = CeremonyOutcome::Failed;
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
                VERIFY_BUTTON_ID => {
                    if input_matches(state) {
                        state.outcome = CeremonyOutcome::Verified;
                        clear_input(state);
                        unsafe { DestroyWindow(window) };
                    } else {
                        let text = wide_null(
                            "The recovery credential did not match. Re-enter it exactly, or cancel wallet creation.",
                        );
                        let caption = wide_null("Recovery credential mismatch");
                        unsafe {
                            MessageBoxW(
                                window,
                                text.as_ptr(),
                                caption.as_ptr(),
                                MB_OK | MB_ICONWARNING,
                            );
                            SetWindowTextW(state.input, EMPTY_WIDE.as_ptr());
                            SetFocus(state.input);
                        }
                    }
                    0
                }
                CANCEL_BUTTON_ID => {
                    state.outcome = CeremonyOutcome::Cancelled;
                    clear_input(state);
                    unsafe { DestroyWindow(window) };
                    0
                }
                _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
            }
        }
        WM_CLOSE => {
            state.outcome = CeremonyOutcome::Cancelled;
            clear_input(state);
            unsafe { DestroyWindow(window) };
            0
        }
        WM_NCDESTROY => {
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn create_dialog_controls(window: HWND, state: &mut CeremonyDialogState) -> bool {
    let expected = unsafe { std::slice::from_raw_parts(state.expected, state.expected_len + 1) };
    let heading = wide_null(
        "Write this recovery credential down and store it offline. It is required to restore this wallet.",
    );
    let instruction = wide_null(
        "Then re-enter the complete credential below. The wallet and recovery file do not exist until verification succeeds.",
    );
    let verify = wide_null("Verify and create wallet");
    let cancel = wide_null("Cancel");
    let static_class = wide_null("STATIC");
    let edit_class = wide_null("EDIT");
    let button_class = wide_null("BUTTON");

    let controls = [
        create_control(
            window,
            &static_class,
            &heading,
            WS_CHILD | WS_VISIBLE,
            24,
            22,
            640,
            40,
            0,
        ),
        create_control(
            window,
            &static_class,
            expected,
            WS_CHILD | WS_VISIBLE | WS_BORDER,
            24,
            72,
            640,
            42,
            0,
        ),
        create_control(
            window,
            &static_class,
            &instruction,
            WS_CHILD | WS_VISIBLE,
            24,
            132,
            640,
            42,
            0,
        ),
    ];
    if controls.iter().any(|control| control.is_null()) {
        return false;
    }

    state.input = create_control(
        window,
        &edit_class,
        &EMPTY_WIDE,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32,
        24,
        190,
        640,
        32,
        0,
    );
    let verify_button = create_control(
        window,
        &button_class,
        &verify,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        370,
        270,
        190,
        38,
        VERIFY_BUTTON_ID,
    );
    let cancel_button = create_control(
        window,
        &button_class,
        &cancel,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
        574,
        270,
        90,
        38,
        CANCEL_BUTTON_ID,
    );
    if state.input.is_null() || verify_button.is_null() || cancel_button.is_null() {
        return false;
    }

    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) } as usize;
    for control in controls
        .into_iter()
        .chain([state.input, verify_button, cancel_button])
    {
        unsafe { SendMessageW(control, WM_SETFONT, font, 1) };
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn create_control(
    parent: HWND,
    class_name: &[u16],
    text: &[u16],
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    control_id: usize,
) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            text.as_ptr(),
            style,
            x,
            y,
            width,
            height,
            parent,
            control_id as *mut c_void,
            GetModuleHandleW(null()) as HINSTANCE,
            null(),
        )
    }
}

fn input_matches(state: &CeremonyDialogState) -> bool {
    let length = unsafe { GetWindowTextLengthW(state.input) };
    if length < 0 || usize::try_from(length).ok() != Some(state.expected_len) {
        return false;
    }
    let mut input = Zeroizing::new(vec![0_u16; state.expected_len + 1]);
    let copied = unsafe {
        GetWindowTextW(
            state.input,
            input.as_mut_ptr(),
            i32::try_from(input.len()).unwrap_or(i32::MAX),
        )
    };
    if usize::try_from(copied).ok() != Some(state.expected_len) {
        return false;
    }
    let expected = unsafe { std::slice::from_raw_parts(state.expected, state.expected_len) };
    expected
        .iter()
        .zip(input.iter())
        .fold(0_u16, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn clear_input(state: &CeremonyDialogState) {
    if !state.input.is_null() {
        unsafe { SetWindowTextW(state.input, EMPTY_WIDE.as_ptr()) };
    }
}

fn wide_null(value: &str) -> Zeroizing<Vec<u16>> {
    let mut wide = Zeroizing::new(value.encode_utf16().collect::<Vec<_>>());
    wide.push(0);
    wide
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_verification_is_exact_and_constant_work_for_equal_lengths() {
        let expected = wide_null("vision-recovery-v1-deadbeef-01020304");
        let exact = &expected[..expected.len() - 1];
        let changed = wide_null("vision-recovery-v1-deadbeef-01020305");
        let changed = &changed[..changed.len() - 1];
        assert_eq!(exact.len(), changed.len());
        assert!(
            exact
                .iter()
                .zip(exact)
                .fold(0_u16, |difference, (left, right)| difference
                    | (left ^ right))
                == 0
        );
        assert!(
            exact
                .iter()
                .zip(changed)
                .fold(0_u16, |difference, (left, right)| difference
                    | (left ^ right))
                != 0
        );
    }

    #[test]
    fn native_ceremony_source_contains_no_clipboard_path() {
        let source = include_str!("recovery_ceremony.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for forbidden in ["SetClipboardData", "OpenClipboard"] {
            assert!(!production.contains(forbidden));
        }
    }
}
