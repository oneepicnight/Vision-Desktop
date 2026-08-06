#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "native modal entry points cannot be opened by automated unit tests"
    )
)]

use super::native_secret_buffer::FixedSecretUtf16;
use std::{
    ffi::c_void,
    mem::size_of,
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, DrawTextW, EndPaint, GetStockObject, GetSysColorBrush, InvalidateRect,
        SelectObject, SetBkMode, COLOR_WINDOW, DEFAULT_GUI_FONT, DT_LEFT, DT_SINGLELINE,
        DT_WORDBREAK, PAINTSTRUCT, TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::{
            Ime::{ImmAssociateContextEx, IACE_CHILDREN, IACE_IGNORENOCONTEXT},
            KeyboardAndMouse::{EnableWindow, SetFocus},
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
            GetWindowLongPtrW, GetWindowRect, IsDialogMessageW, IsWindow, KillTimer, LoadCursorW,
            MessageBoxW, PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetTimer,
            SetWindowLongPtrW, ShowWindow, TranslateMessage, UnregisterClassW, BN_CLICKED,
            BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA,
            IDC_ARROW, MB_ICONWARNING, MB_OK, MSG, SW_SHOW, WM_CHAR, WM_CLEAR, WM_CLOSE,
            WM_COMMAND, WM_CONTEXTMENU, WM_COPY, WM_CREATE, WM_CUT, WM_GETTEXT, WM_GETTEXTLENGTH,
            WM_IME_CHAR, WM_IME_COMPOSITION, WM_IME_COMPOSITIONFULL, WM_IME_CONTROL,
            WM_IME_ENDCOMPOSITION, WM_IME_KEYDOWN, WM_IME_KEYUP, WM_IME_NOTIFY, WM_IME_REQUEST,
            WM_IME_SELECT, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION, WM_INPUTLANGCHANGE,
            WM_INPUTLANGCHANGEREQUEST, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_PASTE, WM_SETTEXT,
            WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME, WS_POPUP, WS_SYSMENU,
            WS_TABSTOP, WS_VISIBLE,
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
const FIXED_BULLET_COUNT: usize = 24;

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

/// Rust-owned recovery acknowledgement that never assigns either secret operand to a native
/// `STATIC`, `EDIT`, title, clipboard, or accessibility value buffer.
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
        let expected = FixedSecretUtf16::from_ascii(encoded_credential.as_bytes())
            .map_err(|_| RecoveryCeremonyError::NativeUiUnavailable)?;
        run_native_ceremony(self.owner_window as HWND, expected, authority_is_current)
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
    expected: FixedSecretUtf16,
    input: FixedSecretUtf16,
    outcome: CeremonyOutcome,
}

impl CeremonyDialogState {
    fn wipe(&mut self) {
        self.expected.wipe();
        self.input.wipe();
    }
}

fn run_native_ceremony(
    owner_window: HWND,
    expected: FixedSecretUtf16,
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

    let (x, y) = centered_position(owner_window);
    let mut state = CeremonyDialogState {
        expected,
        input: FixedSecretUtf16::empty(),
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
        state.wipe();
        return Err(RecoveryCeremonyError::NativeUiUnavailable);
    }
    let _modal_owner = DisabledOwner::new(owner_window);
    if unsafe { SetTimer(dialog, AUTHORITY_TIMER_ID, AUTHORITY_TIMER_MS, None) } == 0 {
        state.wipe();
        unsafe { DestroyWindow(dialog) };
        return Err(RecoveryCeremonyError::NativeUiUnavailable);
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
            state.outcome = CeremonyOutcome::Failed;
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
            && !authority_is_current()
        {
            state.outcome = CeremonyOutcome::AuthorityRevoked;
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
            if !disable_text_services(window) || !create_dialog_buttons(window) {
                state.outcome = CeremonyOutcome::Failed;
                return -1;
            }
            0
        }
        WM_CHAR => {
            let unit = u16::try_from(wparam).unwrap_or_default();
            match unit {
                8 => state.input.pop_unit(),
                0x20..=0x7e => {
                    if state.input.push_unit(unit).is_err() {
                        state.outcome = CeremonyOutcome::Failed;
                        state.wipe();
                        unsafe { DestroyWindow(window) };
                        return 0;
                    }
                }
                _ => {
                    // Recovery credentials are canonical ASCII. An unsupported IME or
                    // international input route fails closed; there is no standard-control fallback.
                    state.outcome = CeremonyOutcome::Failed;
                    state.wipe();
                    unsafe { DestroyWindow(window) };
                    return 0;
                }
            }
            unsafe { InvalidateRect(window, null(), 0) };
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
                    let matched = state.expected.matches_and_wipe(&mut state.input);
                    if matched {
                        state.outcome = CeremonyOutcome::Verified;
                    } else {
                        state.outcome = CeremonyOutcome::Cancelled;
                        let text = wide_null(
                            "The recovery credential did not match. Wallet creation was cancelled; start again.",
                        );
                        let caption = wide_null("Recovery credential mismatch");
                        unsafe {
                            MessageBoxW(
                                window,
                                text.as_ptr(),
                                caption.as_ptr(),
                                MB_OK | MB_ICONWARNING,
                            )
                        };
                    }
                    unsafe { DestroyWindow(window) };
                    0
                }
                CANCEL_BUTTON_ID => {
                    state.outcome = CeremonyOutcome::Cancelled;
                    state.wipe();
                    unsafe { DestroyWindow(window) };
                    0
                }
                _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
            }
        }
        WM_PAINT => {
            paint_dialog(window, state);
            0
        }
        WM_COPY | WM_CUT | WM_PASTE | WM_CLEAR | WM_CONTEXTMENU | WM_GETTEXT | WM_GETTEXTLENGTH
        | WM_SETTEXT => 0,
        message if is_blocked_text_service_message(message) => {
            state.outcome = CeremonyOutcome::Failed;
            state.wipe();
            unsafe { DestroyWindow(window) };
            0
        }
        WM_CLOSE => {
            state.outcome = CeremonyOutcome::Cancelled;
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

fn paint_dialog(window: HWND, state: &CeremonyDialogState) {
    let mut paint = PAINTSTRUCT::default();
    let device = unsafe { BeginPaint(window, &mut paint) };
    if device.is_null() {
        return;
    }
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    let previous = unsafe { SelectObject(device, font) };
    unsafe { SetBkMode(device, i32::try_from(TRANSPARENT).unwrap_or(1)) };

    draw_public_text(
        device,
        "Write this recovery credential down and store it offline. It is required to restore this wallet.",
        RECT { left: 24, top: 22, right: 664, bottom: 62 },
        DT_LEFT | DT_WORDBREAK,
    );
    draw_secret_text(
        device,
        state.expected.as_units(),
        RECT {
            left: 24,
            top: 76,
            right: 664,
            bottom: 116,
        },
    );
    draw_public_text(
        device,
        "Re-enter the complete credential using the keyboard. Clipboard and accessibility text access are disabled.",
        RECT { left: 24, top: 136, right: 664, bottom: 176 },
        DT_LEFT | DT_WORDBREAK,
    );
    let bullets = [0x2022_u16; FIXED_BULLET_COUNT];
    draw_secret_text(
        device,
        &bullets,
        RECT {
            left: 24,
            top: 198,
            right: 664,
            bottom: 232,
        },
    );
    if state.input.is_empty() {
        draw_public_text(
            device,
            "Input is hidden; the fixed bullets do not reveal its length.",
            RECT {
                left: 24,
                top: 236,
                right: 664,
                bottom: 258,
            },
            DT_LEFT | DT_SINGLELINE,
        );
    }

    unsafe {
        SelectObject(device, previous);
        EndPaint(window, &paint);
    }
}

fn draw_public_text(
    device: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &str,
    mut bounds: RECT,
    format: u32,
) {
    let wide = wide_without_nul(text);
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

fn draw_secret_text(
    device: windows_sys::Win32::Graphics::Gdi::HDC,
    text: &[u16],
    mut bounds: RECT,
) {
    unsafe {
        DrawTextW(
            device,
            text.as_ptr(),
            i32::try_from(text.len()).unwrap_or(i32::MAX),
            &mut bounds,
            DT_LEFT | DT_SINGLELINE,
        )
    };
}

fn create_dialog_buttons(window: HWND) -> bool {
    let button_class = wide_null("BUTTON");
    let verify = wide_null("Verify and create wallet");
    let cancel = wide_null("Cancel");
    let verify_button = create_button(
        window,
        &button_class,
        &verify,
        BS_DEFPUSHBUTTON as u32,
        370,
        290,
        190,
        VERIFY_BUTTON_ID,
    );
    let cancel_button = create_button(
        window,
        &button_class,
        &cancel,
        BS_PUSHBUTTON as u32,
        574,
        290,
        90,
        CANCEL_BUTTON_ID,
    );
    !verify_button.is_null() && !cancel_button.is_null()
}

#[allow(clippy::too_many_arguments)]
fn create_button(
    parent: HWND,
    class_name: &[u16],
    text: &[u16],
    button_style: u32,
    x: i32,
    y: i32,
    width: i32,
    control_id: usize,
) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | button_style,
            x,
            y,
            width,
            38,
            parent,
            control_id as *mut c_void,
            GetModuleHandleW(null()) as HINSTANCE,
            null(),
        )
    }
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

fn wide_without_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSecretCeremonyError {
    Cancelled,
    AuthorityRevoked,
    InvalidInput,
    NativeUiUnavailable,
}

pub(in crate::wallet) struct NativeCreateSecrets {
    pub(in crate::wallet) wallet_password: super::secret_input::SecretInput,
}

pub(in crate::wallet) struct NativeRestoreSecrets {
    pub(in crate::wallet) wallet_password: super::secret_input::SecretInput,
    pub(in crate::wallet) recovery_credential: super::secret_input::SecretInput,
}

pub(in crate::wallet) trait WalletSecretCeremony: Send + Sync {
    fn capture_create(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<NativeCreateSecrets, NativeSecretCeremonyError>;

    fn capture_restore(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<NativeRestoreSecrets, NativeSecretCeremonyError>;

    fn capture_unlock(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<super::secret_input::SecretInput, NativeSecretCeremonyError>;
}

pub(crate) struct NativeWalletSecretCeremony {
    owner_window: isize,
}

impl NativeWalletSecretCeremony {
    pub(crate) fn new(owner_window: isize) -> Result<Self, NativeSecretCeremonyError> {
        if owner_window == 0 || unsafe { IsWindow(owner_window as HWND) } == 0 {
            return Err(NativeSecretCeremonyError::NativeUiUnavailable);
        }
        Ok(Self { owner_window })
    }

    fn capture_confirmed_password(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<super::secret_input::SecretInput, NativeSecretCeremonyError> {
        let password = run_native_secret_capture(
            self.owner_window as HWND,
            SecretCapturePurpose::NewPassword,
            authority_is_current,
        )?;
        let confirmation = run_native_secret_capture(
            self.owner_window as HWND,
            SecretCapturePurpose::ConfirmPassword,
            authority_is_current,
        )?;
        let password = password
            .confirm_with(confirmation)
            .map_err(|_| NativeSecretCeremonyError::InvalidInput)?;
        if password.byte_len() < 16 {
            return Err(NativeSecretCeremonyError::InvalidInput);
        }
        Ok(password)
    }
}

impl WalletSecretCeremony for NativeWalletSecretCeremony {
    fn capture_create(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<NativeCreateSecrets, NativeSecretCeremonyError> {
        Ok(NativeCreateSecrets {
            wallet_password: self.capture_confirmed_password(authority_is_current)?,
        })
    }

    fn capture_restore(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<NativeRestoreSecrets, NativeSecretCeremonyError> {
        let recovery_credential = run_native_secret_capture(
            self.owner_window as HWND,
            SecretCapturePurpose::RecoveryCredential,
            authority_is_current,
        )?;
        let wallet_password = self.capture_confirmed_password(authority_is_current)?;
        Ok(NativeRestoreSecrets {
            wallet_password,
            recovery_credential,
        })
    }

    fn capture_unlock(
        &self,
        authority_is_current: &dyn Fn() -> bool,
    ) -> Result<super::secret_input::SecretInput, NativeSecretCeremonyError> {
        run_native_secret_capture(
            self.owner_window as HWND,
            SecretCapturePurpose::UnlockPassword,
            authority_is_current,
        )
    }
}

#[derive(Clone, Copy)]
enum SecretCapturePurpose {
    NewPassword,
    ConfirmPassword,
    RecoveryCredential,
    UnlockPassword,
}

impl SecretCapturePurpose {
    const fn title(self) -> &'static str {
        match self {
            Self::NewPassword => "Vision Wallet Password",
            Self::ConfirmPassword => "Confirm Vision Wallet Password",
            Self::RecoveryCredential => "Vision Wallet Recovery Credential",
            Self::UnlockPassword => "Unlock Vision Wallet",
        }
    }

    const fn instruction(self) -> &'static str {
        match self {
            Self::NewPassword => "Enter a new local wallet password of at least 16 UTF-8 bytes.",
            Self::ConfirmPassword => "Re-enter the new local wallet password exactly.",
            Self::RecoveryCredential => "Enter the complete portable recovery credential.",
            Self::UnlockPassword => "Enter the local wallet password.",
        }
    }
}

struct SecretCaptureState {
    input: FixedSecretUtf16,
    captured: Option<super::secret_input::SecretInput>,
    purpose: SecretCapturePurpose,
    outcome: CeremonyOutcome,
}

impl SecretCaptureState {
    fn wipe(&mut self) {
        self.input.wipe();
        self.captured = None;
    }
}

fn run_native_secret_capture(
    owner_window: HWND,
    purpose: SecretCapturePurpose,
    authority_is_current: &dyn Fn() -> bool,
) -> Result<super::secret_input::SecretInput, NativeSecretCeremonyError> {
    if !authority_is_current() {
        return Err(NativeSecretCeremonyError::AuthorityRevoked);
    }
    let instance = unsafe { GetModuleHandleW(null()) } as HINSTANCE;
    if instance.is_null() || unsafe { IsWindow(owner_window) } == 0 {
        return Err(NativeSecretCeremonyError::NativeUiUnavailable);
    }
    let sequence = CLASS_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let class_name = wide_null(&format!(
        "VisionDesktopNativeSecretCapture-{}-{sequence}",
        std::process::id()
    ));
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(secret_capture_window_proc),
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
        return Err(NativeSecretCeremonyError::NativeUiUnavailable);
    }
    let _registration = RegisteredWindowClass {
        instance,
        class_name: &class_name,
    };
    let (x, y) = centered_position(owner_window);
    let mut state = SecretCaptureState {
        input: FixedSecretUtf16::empty(),
        captured: None,
        purpose,
        outcome: CeremonyOutcome::Pending,
    };
    let title = wide_null(purpose.title());
    let dialog = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            x,
            y,
            DIALOG_WIDTH,
            290,
            owner_window,
            null_mut(),
            instance,
            (&mut state as *mut SecretCaptureState).cast::<c_void>(),
        )
    };
    if dialog.is_null() {
        state.wipe();
        return Err(NativeSecretCeremonyError::NativeUiUnavailable);
    }
    let _modal_owner = DisabledOwner::new(owner_window);
    if unsafe { SetTimer(dialog, AUTHORITY_TIMER_ID, AUTHORITY_TIMER_MS, None) } == 0 {
        state.wipe();
        unsafe { DestroyWindow(dialog) };
        return Err(NativeSecretCeremonyError::NativeUiUnavailable);
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
            state.outcome = CeremonyOutcome::Failed;
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
            && !authority_is_current()
        {
            state.outcome = CeremonyOutcome::AuthorityRevoked;
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
    match state.outcome {
        CeremonyOutcome::Verified if authority_is_current() => state
            .captured
            .take()
            .ok_or(NativeSecretCeremonyError::NativeUiUnavailable),
        CeremonyOutcome::Verified | CeremonyOutcome::AuthorityRevoked => {
            state.wipe();
            Err(NativeSecretCeremonyError::AuthorityRevoked)
        }
        CeremonyOutcome::Cancelled => {
            state.wipe();
            Err(NativeSecretCeremonyError::Cancelled)
        }
        CeremonyOutcome::Pending | CeremonyOutcome::Failed => {
            state.wipe();
            Err(NativeSecretCeremonyError::NativeUiUnavailable)
        }
    }
}

unsafe extern "system" fn secret_capture_window_proc(
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
    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut SecretCaptureState;
    if state.is_null() {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let state = unsafe { &mut *state };
    match message {
        WM_CREATE => {
            if !disable_text_services(window) || !create_capture_buttons(window) {
                state.outcome = CeremonyOutcome::Failed;
                return -1;
            }
            0
        }
        WM_CHAR => {
            let unit = u16::try_from(wparam).unwrap_or_default();
            match unit {
                8 => state.input.pop_unit(),
                0x20..=0xffff => {
                    if state.input.push_unit(unit).is_err() {
                        state.outcome = CeremonyOutcome::Failed;
                        state.wipe();
                        unsafe { DestroyWindow(window) };
                        return 0;
                    }
                }
                _ => {
                    state.outcome = CeremonyOutcome::Failed;
                    state.wipe();
                    unsafe { DestroyWindow(window) };
                    return 0;
                }
            }
            unsafe { InvalidateRect(window, null(), 0) };
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
                    let input = std::mem::replace(&mut state.input, FixedSecretUtf16::empty());
                    match input.into_secret_input() {
                        Ok(secret) => {
                            state.captured = Some(secret);
                            state.outcome = CeremonyOutcome::Verified;
                        }
                        Err(_) => {
                            state.wipe();
                            state.outcome = CeremonyOutcome::Failed;
                        }
                    }
                    unsafe { DestroyWindow(window) };
                    0
                }
                CANCEL_BUTTON_ID => {
                    state.outcome = CeremonyOutcome::Cancelled;
                    state.wipe();
                    unsafe { DestroyWindow(window) };
                    0
                }
                _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
            }
        }
        WM_PAINT => {
            paint_secret_capture(window, state);
            0
        }
        WM_COPY | WM_CUT | WM_PASTE | WM_CLEAR | WM_CONTEXTMENU | WM_GETTEXT | WM_GETTEXTLENGTH
        | WM_SETTEXT => 0,
        message if is_blocked_text_service_message(message) => {
            state.outcome = CeremonyOutcome::Failed;
            state.wipe();
            unsafe { DestroyWindow(window) };
            0
        }
        WM_CLOSE => {
            state.outcome = CeremonyOutcome::Cancelled;
            state.wipe();
            unsafe { DestroyWindow(window) };
            0
        }
        WM_NCDESTROY => {
            state.input.wipe();
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn paint_secret_capture(window: HWND, state: &SecretCaptureState) {
    let mut paint = PAINTSTRUCT::default();
    let device = unsafe { BeginPaint(window, &mut paint) };
    if device.is_null() {
        return;
    }
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    let previous = unsafe { SelectObject(device, font) };
    unsafe { SetBkMode(device, i32::try_from(TRANSPARENT).unwrap_or(1)) };
    draw_public_text(
        device,
        state.purpose.instruction(),
        RECT {
            left: 24,
            top: 26,
            right: 664,
            bottom: 68,
        },
        DT_LEFT | DT_WORDBREAK,
    );
    let bullets = [0x2022_u16; FIXED_BULLET_COUNT];
    draw_secret_text(
        device,
        &bullets,
        RECT {
            left: 24,
            top: 92,
            right: 664,
            bottom: 126,
        },
    );
    draw_public_text(
        device,
        "Input stays in Rust-owned fixed memory. The fixed bullets do not reveal its length.",
        RECT {
            left: 24,
            top: 132,
            right: 664,
            bottom: 168,
        },
        DT_LEFT | DT_WORDBREAK,
    );
    unsafe {
        SelectObject(device, previous);
        EndPaint(window, &paint);
    }
}

fn create_capture_buttons(window: HWND) -> bool {
    let button_class = wide_null("BUTTON");
    let submit = wide_null("Continue");
    let cancel = wide_null("Cancel");
    let submit_button = create_button(
        window,
        &button_class,
        &submit,
        BS_DEFPUSHBUTTON as u32,
        470,
        190,
        90,
        VERIFY_BUTTON_ID,
    );
    let cancel_button = create_button(
        window,
        &button_class,
        &cancel,
        BS_PUSHBUTTON as u32,
        574,
        190,
        90,
        CANCEL_BUTTON_ID,
    );
    !submit_button.is_null() && !cancel_button.is_null()
}

fn disable_text_services(window: HWND) -> bool {
    // A null input context disassociates the IME. Applying it to children prevents a future
    // standard child or text service from silently inheriting an input context.
    unsafe { ImmAssociateContextEx(window, null_mut(), IACE_CHILDREN | IACE_IGNORENOCONTEXT) != 0 }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_controls_are_owner_drawn_and_have_no_standard_text_storage() {
        let source = include_str!("recovery_ceremony.rs");
        assert!(!source.contains(concat!("wide_null(\"", "EDIT\")")));
        assert!(!source.contains(concat!("wide_null(\"", "STATIC\")")));
        assert!(!source.contains(concat!("GetWindow", "TextW")));
        assert!(!source.contains(concat!("SetWindow", "TextW")));
        assert!(!source.contains(concat!("ES_", "PASSWORD")));
    }

    #[test]
    fn secret_message_routes_are_explicitly_blocked() {
        let source = include_str!("recovery_ceremony.rs");
        for blocked in [
            "WM_COPY",
            "WM_CUT",
            "WM_PASTE",
            "WM_CONTEXTMENU",
            "WM_GETTEXT",
            "WM_GETTEXTLENGTH",
            "WM_IME_STARTCOMPOSITION",
            "WM_IME_CHAR",
            "WM_IME_SETCONTEXT",
            "WM_INPUTLANGCHANGE",
        ] {
            assert!(source.contains(blocked), "missing {blocked} protection");
        }
        assert!(source.contains("ImmAssociateContextEx"));
    }

    #[test]
    fn every_ime_and_input_language_route_is_classified_fail_closed() {
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
        assert!(!is_blocked_text_service_message(WM_CHAR));
    }

    #[test]
    fn ime_result_injection_wipes_and_closes_both_secret_windows() {
        let class_name = wide_null("BUTTON");
        let window = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
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

        let expected = FixedSecretUtf16::from_ascii(b"expected-secret").unwrap();
        let input = FixedSecretUtf16::from_ascii(b"typed-secret").unwrap();
        let mut ceremony_state = CeremonyDialogState {
            expected,
            input,
            outcome: CeremonyOutcome::Pending,
        };
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                (&mut ceremony_state as *mut CeremonyDialogState) as isize,
            );
            ceremony_window_proc(window, WM_IME_CHAR, usize::from(b'x'), 0);
        }
        assert_eq!(ceremony_state.outcome, CeremonyOutcome::Failed);
        assert!(ceremony_state.expected.is_empty());
        assert!(ceremony_state.input.is_empty());
        assert_eq!(unsafe { IsWindow(window) }, 0);

        let capture_window = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
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
        assert!(!capture_window.is_null());
        let mut capture_state = SecretCaptureState {
            input: FixedSecretUtf16::from_ascii(b"typed-password").unwrap(),
            captured: None,
            purpose: SecretCapturePurpose::UnlockPassword,
            outcome: CeremonyOutcome::Pending,
        };
        unsafe {
            SetWindowLongPtrW(
                capture_window,
                GWLP_USERDATA,
                (&mut capture_state as *mut SecretCaptureState) as isize,
            );
            secret_capture_window_proc(capture_window, WM_INPUTLANGCHANGE, 0, 0);
        }
        assert_eq!(capture_state.outcome, CeremonyOutcome::Failed);
        assert!(capture_state.input.is_empty());
        assert!(capture_state.captured.is_none());
        assert_eq!(unsafe { IsWindow(capture_window) }, 0);
    }
}
