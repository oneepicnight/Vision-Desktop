use super::runtime::{WalletRuntimeError, WalletRuntimeState};
use std::{ffi::c_void, mem, os::windows::ffi::OsStrExt, ptr, sync::Arc};
use windows_sys::Win32::{
    Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        Power::{RegisterSuspendResumeNotification, UnregisterSuspendResumeNotification},
        RemoteDesktop::{
            WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
            NOTIFY_FOR_THIS_SESSION,
        },
    },
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, RegisterClassExW,
        SetWindowLongPtrW, UnregisterClassW, CREATESTRUCTW, DEVICE_NOTIFY_WINDOW_HANDLE,
        GWLP_USERDATA, PBT_APMSTANDBY, PBT_APMSUSPEND, WM_ENDSESSION, WM_NCCREATE, WM_NCDESTROY,
        WM_POWERBROADCAST, WM_QUERYENDSESSION, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_OVERLAPPED, WTS_SESSION_LOCK,
    },
};

const CLASS_PREFIX: &str = "VisionDesktopWalletLifecycle";

/// Owns the hidden native window that receives Windows session and power lifecycle messages.
///
/// The handle and class identity are intentionally private. This type implements neither Clone,
/// Debug, nor serialization, and it grants no WebView authority.
pub(crate) struct WindowsWalletLifecycle {
    window: isize,
    instance: isize,
    suspend_resume_notification: isize,
    class_name: Vec<u16>,
}

struct LifecycleWindowData {
    runtime: Arc<WalletRuntimeState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecurityAction {
    Ignore,
    Invalidate,
    InvalidateAndAllowShutdown,
}

impl WindowsWalletLifecycle {
    pub(crate) fn register(runtime: Arc<WalletRuntimeState>) -> Result<Self, WalletRuntimeError> {
        let class_name = wide_null(&format!("{CLASS_PREFIX}.{}", std::process::id()));
        Self::register_with_class(runtime, class_name)
    }

    fn register_with_class(
        runtime: Arc<WalletRuntimeState>,
        class_name: Vec<u16>,
    ) -> Result<Self, WalletRuntimeError> {
        if class_name.len() < 2 || class_name.last() != Some(&0) {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }

        // SAFETY: a null module name asks Windows for this executable's module handle.
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        if instance.is_null() {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }

        let window_class = WNDCLASSEXW {
            cbSize: u32::try_from(mem::size_of::<WNDCLASSEXW>())
                .map_err(|_| WalletRuntimeError::RuntimeUnavailable)?,
            lpfnWndProc: Some(lifecycle_window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..WNDCLASSEXW::default()
        };

        // SAFETY: `window_class` and its null-terminated class name remain valid for the call.
        if unsafe { RegisterClassExW(&window_class) } == 0 {
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }

        let window_data = Arc::new(LifecycleWindowData { runtime });
        // SAFETY: the registered class uses our procedure. The Arc remains alive throughout the
        // call; WM_NCCREATE takes a separate strong reference for the window before storing it.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                class_name.as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                Arc::as_ptr(&window_data).cast::<c_void>(),
            )
        };
        if window.is_null() {
            // SAFETY: this process registered the class with this module handle and no window
            // survived creation.
            unsafe { UnregisterClassW(class_name.as_ptr(), instance) };
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }

        // SAFETY: `window` is a live top-level window owned by this process.
        if unsafe { WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) } == 0 {
            // Destroying the window releases its Arc through WM_NCDESTROY.
            // SAFETY: all handles were created successfully above.
            unsafe {
                DestroyWindow(window);
                UnregisterClassW(class_name.as_ptr(), instance);
            }
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }

        // A window does not automatically receive Desktop Activity Moderator notifications on
        // Modern Standby systems. Opt in explicitly so S0 low-power idle and traditional S3/S4
        // transitions both deliver PBT_APMSUSPEND before desktop execution is paused.
        // SAFETY: `window` is a live top-level window and the notification handle is retained
        // until it is unregistered before window destruction.
        let suspend_resume_notification = unsafe {
            RegisterSuspendResumeNotification(
                window.cast::<c_void>() as HANDLE,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
        };
        if suspend_resume_notification == 0 {
            // SAFETY: session notification registration succeeded and all retained handles are
            // still live. Destroying the window releases its Arc through WM_NCDESTROY.
            unsafe {
                WTSUnRegisterSessionNotification(window);
                DestroyWindow(window);
                UnregisterClassW(class_name.as_ptr(), instance);
            }
            return Err(WalletRuntimeError::RuntimeUnavailable);
        }

        Ok(Self {
            window: window as isize,
            instance: instance as isize,
            suspend_resume_notification,
            class_name,
        })
    }

    #[cfg(test)]
    fn window(&self) -> HWND {
        self.window as HWND
    }
}

impl Drop for WindowsWalletLifecycle {
    fn drop(&mut self) {
        let window = self.window as HWND;
        let instance = self.instance as HINSTANCE;
        if !window.is_null() {
            // These cleanup calls are best effort. WM_NCDESTROY performs the final synchronous
            // invalidation and releases the window-owned runtime reference.
            // SAFETY: the handles and class name were retained unchanged from registration.
            unsafe {
                UnregisterSuspendResumeNotification(self.suspend_resume_notification);
                WTSUnRegisterSessionNotification(window);
                DestroyWindow(window);
                UnregisterClassW(self.class_name.as_ptr(), instance);
            }
            self.window = 0;
            self.suspend_resume_notification = 0;
        }
    }
}

unsafe extern "system" fn lifecycle_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if create.is_null() {
            return 0;
        }
        // SAFETY: Windows supplies a valid CREATESTRUCTW during WM_NCCREATE. The pointer refers
        // to the live Arc passed to CreateWindowExW. Incrementing transfers one strong reference
        // to the window, which WM_NCDESTROY later releases.
        let data = unsafe { (*create).lpCreateParams.cast::<LifecycleWindowData>() };
        if data.is_null() {
            return 0;
        }
        unsafe {
            Arc::increment_strong_count(data);
            SetWindowLongPtrW(window, GWLP_USERDATA, data as isize);
        }
        // A zero previous value is also the normal successful return from SetWindowLongPtrW, so
        // verify the stored value directly and release the provisional window reference on error.
        if unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } != data as isize {
            // SAFETY: the strong count above has not been transferred to the window.
            unsafe { Arc::decrement_strong_count(data) };
            return 0;
        }
        return 1;
    }

    match classify_security_message(message, wparam) {
        SecurityAction::Invalidate => invalidate_runtime(window),
        SecurityAction::InvalidateAndAllowShutdown => {
            invalidate_runtime(window);
            return 1;
        }
        SecurityAction::Ignore => {}
    }

    if message == WM_NCDESTROY {
        // Remove the pointer before releasing the Arc so re-entrant/default handling cannot use it.
        // SAFETY: this window procedure is the sole owner of the GWLP_USERDATA lifecycle.
        let data =
            unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const LifecycleWindowData;
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
        if !data.is_null() {
            // SAFETY: WM_NCCREATE created exactly one window-owned strong reference.
            let owned = unsafe { Arc::from_raw(data) };
            let _ = owned.runtime.invalidate_all();
            drop(owned);
        }
    }

    // SAFETY: unhandled messages are delegated to the system default window procedure.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn classify_security_message(message: u32, wparam: WPARAM) -> SecurityAction {
    match message {
        WM_WTSSESSION_CHANGE if wparam == WTS_SESSION_LOCK as usize => SecurityAction::Invalidate,
        WM_POWERBROADCAST
            if wparam == PBT_APMSUSPEND as usize || wparam == PBT_APMSTANDBY as usize =>
        {
            SecurityAction::Invalidate
        }
        WM_QUERYENDSESSION => SecurityAction::InvalidateAndAllowShutdown,
        WM_ENDSESSION if wparam != 0 => SecurityAction::Invalidate,
        _ => SecurityAction::Ignore,
    }
}

fn invalidate_runtime(window: HWND) {
    // SAFETY: the pointer is either null or the Arc-backed LifecycleWindowData installed during
    // WM_NCCREATE and retained until WM_NCDESTROY on this same window thread.
    let data = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const LifecycleWindowData;
    if !data.is_null() {
        let _ = unsafe { &*data }.runtime.invalidate_all();
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::runtime::{RecoveryPathPurpose, WalletOperationKind};
    use std::path::PathBuf;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, SendMessageW, TranslateMessage, MSG,
        PBT_APMRESUMEAUTOMATIC, PM_REMOVE, WTS_SESSION_UNLOCK,
    };

    #[test]
    fn security_message_policy_locks_only_on_fail_closed_events() {
        assert_eq!(
            classify_security_message(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize),
            SecurityAction::Invalidate
        );
        assert_eq!(
            classify_security_message(WM_POWERBROADCAST, PBT_APMSUSPEND as usize),
            SecurityAction::Invalidate
        );
        assert_eq!(
            classify_security_message(WM_POWERBROADCAST, PBT_APMSTANDBY as usize),
            SecurityAction::Invalidate
        );
        assert_eq!(
            classify_security_message(WM_QUERYENDSESSION, 0),
            SecurityAction::InvalidateAndAllowShutdown
        );
        assert_eq!(
            classify_security_message(WM_ENDSESSION, 1),
            SecurityAction::Invalidate
        );
        assert_eq!(
            classify_security_message(WM_ENDSESSION, 0),
            SecurityAction::Ignore
        );
        assert_eq!(
            classify_security_message(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK as usize),
            SecurityAction::Ignore
        );
        assert_eq!(
            classify_security_message(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize),
            SecurityAction::Ignore
        );
    }

    #[test]
    fn native_security_messages_revoke_runtime_authority() {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let class_name = wide_null(&format!(
            "{CLASS_PREFIX}.test.{}.native",
            std::process::id()
        ));
        let lifecycle =
            WindowsWalletLifecycle::register_with_class(Arc::clone(&runtime), class_name).unwrap();

        for (message, wparam) in [
            (WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize),
            (WM_POWERBROADCAST, PBT_APMSUSPEND as usize),
            (WM_POWERBROADCAST, PBT_APMSTANDBY as usize),
            (WM_QUERYENDSESSION, 0),
            (WM_ENDSESSION, 1),
        ] {
            let permit = runtime
                .begin_operation("main", WalletOperationKind::Unlock)
                .unwrap();
            // SAFETY: the guard owns a live hidden window and SendMessageW dispatches
            // synchronously to its registered procedure on this test thread.
            let result = unsafe { SendMessageW(lifecycle.window(), message, wparam, 0) };
            if message == WM_QUERYENDSESSION {
                assert_eq!(result, 1);
            }
            drop(permit);
            assert!(runtime
                .begin_operation("main", WalletOperationKind::Create)
                .is_ok());
            runtime.invalidate_all().unwrap();
        }

        let permit = runtime
            .begin_recovery_path_selection("main", RecoveryPathPurpose::Source)
            .unwrap();
        let token = runtime
            .complete_recovery_path_selection(
                permit,
                PathBuf::from(r"C:\wallet\backup.vision-recovery.json"),
            )
            .unwrap();
        // SAFETY: the guard owns a live hidden window and dispatch is synchronous.
        unsafe {
            SendMessageW(
                lifecycle.window(),
                WM_WTSSESSION_CHANGE,
                WTS_SESSION_LOCK as usize,
                0,
            )
        };
        assert_eq!(
            runtime.consume_recovery_path("main", RecoveryPathPurpose::Source, token.as_str()),
            Err(WalletRuntimeError::PathAuthorizationInvalid)
        );
    }

    /// Manual real-Windows qualification probe.
    ///
    /// Run this ignored release-profile test interactively, wait for the READY line, then perform
    /// exactly one real Windows session lock or suspend/hibernate cycle. The hidden listener and
    /// message pump are the production implementations; only the synthetic pre-existing authority
    /// is test-only. No secret, vault, command, or WebView permission is involved.
    #[test]
    #[ignore = "requires an operator to trigger a real Windows lock or power transition"]
    fn real_windows_security_event_revokes_runtime_authority() {
        let expected_event = std::env::var("VISION_WALLET_QUALIFICATION_EVENT")
            .expect("set VISION_WALLET_QUALIFICATION_EVENT to session_lock, suspend, or hibernate");
        assert!(
            matches!(
                expected_event.as_str(),
                "session_lock" | "suspend" | "hibernate"
            ),
            "unsupported qualification event"
        );
        let timeout_seconds = std::env::var("VISION_WALLET_QUALIFICATION_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| (30..=1_800).contains(value))
            .unwrap_or(600);

        let runtime = Arc::new(WalletRuntimeState::for_test());
        let lifecycle = WindowsWalletLifecycle::register(Arc::clone(&runtime)).unwrap();
        let authority = runtime
            .begin_operation("main", WalletOperationKind::Unlock)
            .unwrap();
        authority.ensure_current().unwrap();

        println!(
            "VISION_WALLET_QUALIFICATION_READY event={expected_event} pid={} timeout_seconds={timeout_seconds}",
            std::process::id()
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_seconds);
        while authority.ensure_current().is_ok() {
            assert!(
                std::time::Instant::now() < deadline,
                "real Windows security event did not revoke wallet authority before timeout"
            );
            let mut message = MSG::default();
            // SAFETY: this test owns the thread's hidden lifecycle window and pumps only messages
            // already queued by Windows for this thread.
            while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
            {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        assert_eq!(
            authority.ensure_current(),
            Err(WalletRuntimeError::RuntimeUnavailable)
        );
        drop(authority);
        runtime
            .begin_operation("main", WalletOperationKind::Create)
            .expect("runtime must remain locked and usable after explicit reauthorization");
        runtime.invalidate_all().unwrap();
        drop(lifecycle);
        println!("VISION_WALLET_QUALIFICATION_PASS event={expected_event}");
    }
}
