#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the reviewed lifecycle command boundary remains private and unregistered"
    )
)]
#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "unregistered production entry points are exercised after command review"
    )
)]

use super::{
    lifecycle::{WalletLifecycleAdapters, WalletLifecycleError},
    public_request::{WalletCreateRequest, WalletRestoreRequest},
    recovery_selection::{select_recovery_destination, select_recovery_source},
    runtime::{RecoveryPathToken, WalletRuntimeError, WalletRuntimeState},
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use tauri::{
    ipc::{CommandArg, CommandItem, InvokeBody, InvokeError, Response},
    Runtime, Url, WebviewWindow,
};

const MAIN_WINDOW_LABEL: &str = "main";
const BUNDLED_WINDOWS_ORIGIN: &str = "http://tauri.localhost";

const GET_STATUS: &str = "wallet_get_status";
const SELECT_RECOVERY_DESTINATION: &str = "wallet_select_recovery_destination";
const CREATE: &str = "wallet_create";
const SELECT_RECOVERY_SOURCE: &str = "wallet_select_recovery_source";
const RESTORE: &str = "wallet_restore";
const UNLOCK: &str = "wallet_unlock";
const LOCK: &str = "wallet_lock";

/// Whole-message command argument for a future reviewed Tauri wrapper.
///
/// Extraction borrows the complete invoke body without parsing it. Parsing happens only after the
/// private fail-closed boundary is armed. This type deliberately implements neither Serde,
/// `Clone`, nor `Debug`.
pub(in crate::wallet) struct WalletInvokeRequest<'a> {
    declared_command: &'static str,
    invoked_command: &'a str,
    body: &'a InvokeBody,
}

impl<'a, R: Runtime> CommandArg<'a, R> for WalletInvokeRequest<'a> {
    fn from_command(command: CommandItem<'a, R>) -> Result<Self, InvokeError> {
        Ok(Self {
            declared_command: command.name,
            invoked_command: command.message.command(),
            body: command.message.payload(),
        })
    }
}

/// Command-shaped Rust adapter that remains inaccessible outside `crate::wallet`.
///
/// There is intentionally no Tauri command attribute, invoke registration, permission,
/// capability, frontend wrapper, or production exposure constructor in this module.
pub(in crate::wallet) struct WalletLifecycleCommandBoundary {
    runtime: Arc<WalletRuntimeState>,
    adapters: Arc<WalletLifecycleAdapters>,
    expected_main_hwnd: isize,
    transport_policy: WholeEnvelopeTransportPolicy,
    #[cfg(test)]
    panic_checkpoint: Option<BoundaryPanicCheckpoint>,
}

struct WholeEnvelopeTransportPolicy {
    duplicate_key_rejection_proven: bool,
}

struct WalletExposureAuthority {
    revocation_epoch: u64,
}

struct MainWalletWindowAuthority {
    revocation_epoch: u64,
    hwnd: isize,
}

struct BoundaryFailClosedGuard<'a> {
    runtime: &'a WalletRuntimeState,
    armed: bool,
}

enum WalletLifecycleEnvelope {
    GetStatus,
    SelectRecoveryDestination,
    Create(WalletCreateRequest),
    SelectRecoverySource,
    Restore(WalletRestoreRequest),
    Unlock,
    Lock,
}

#[cfg_attr(test, derive(Debug))]
enum BoundaryError {
    Lifecycle(WalletLifecycleError),
    Runtime(WalletRuntimeError),
    InvalidRequest,
    InvalidWindow,
    ActivationUnavailable,
    SerializationUnavailable,
}

#[derive(Serialize)]
struct RecoverySelectionResponse<'a> {
    recovery_selection_handle: &'a str,
}

#[cfg_attr(test, derive(Clone, Copy, PartialEq, Eq))]
enum BoundaryPanicCheckpoint {
    AfterEnvelopeValidation,
    BeforeResponseSerialization,
}

impl WholeEnvelopeTransportPolicy {
    const fn production() -> Self {
        // Tauri 2.11.5 normalizes JSON into a Value before Request extraction. Until an exact
        // transport qualification proves duplicate textual keys unreachable or rejectable,
        // production WalletExposureAuthority issuance is structurally unavailable.
        Self {
            duplicate_key_rejection_proven: false,
        }
    }

    #[cfg(test)]
    const fn approved_for_test() -> Self {
        Self {
            duplicate_key_rejection_proven: true,
        }
    }
}

impl WalletExposureAuthority {
    fn issue(
        runtime: &WalletRuntimeState,
        transport_policy: &WholeEnvelopeTransportPolicy,
    ) -> Result<Self, BoundaryError> {
        if !transport_policy.duplicate_key_rejection_proven
            || !runtime.wallet_exposure_scopes_satisfied()
        {
            return Err(BoundaryError::ActivationUnavailable);
        }
        let revocation_epoch = runtime
            .capture_boundary_epoch()
            .map_err(BoundaryError::Runtime)?;
        Ok(Self { revocation_epoch })
    }

    fn validate(&self, runtime: &WalletRuntimeState) -> Result<(), BoundaryError> {
        runtime
            .validate_boundary_epoch(self.revocation_epoch)
            .map_err(BoundaryError::Runtime)
    }
}

impl MainWalletWindowAuthority {
    fn issue<R: Runtime>(
        window: &WebviewWindow<R>,
        expected_main_hwnd: isize,
        exposure: &WalletExposureAuthority,
        runtime: &WalletRuntimeState,
    ) -> Result<Self, BoundaryError> {
        exposure.validate(runtime)?;
        let url = window.url().map_err(|_| BoundaryError::InvalidWindow)?;
        let hwnd = window.hwnd().map_err(|_| BoundaryError::InvalidWindow)?.0 as isize;
        Self::issue_from_parts(
            window.label(),
            &url,
            hwnd,
            expected_main_hwnd,
            exposure,
            runtime,
        )
    }

    fn issue_from_parts(
        label: &str,
        url: &Url,
        hwnd: isize,
        expected_main_hwnd: isize,
        exposure: &WalletExposureAuthority,
        runtime: &WalletRuntimeState,
    ) -> Result<Self, BoundaryError> {
        exposure.validate(runtime)?;
        if label != MAIN_WINDOW_LABEL
            || hwnd == 0
            || hwnd != expected_main_hwnd
            || !is_bundled_windows_url(url)
        {
            return Err(BoundaryError::InvalidWindow);
        }
        let revocation_epoch = runtime
            .capture_boundary_epoch()
            .map_err(BoundaryError::Runtime)?;
        if revocation_epoch != exposure.revocation_epoch {
            return Err(BoundaryError::InvalidWindow);
        }
        exposure.validate(runtime)?;
        Ok(Self {
            revocation_epoch,
            hwnd,
        })
    }

    fn validate(
        &self,
        expected_main_hwnd: isize,
        runtime: &WalletRuntimeState,
    ) -> Result<(), BoundaryError> {
        if self.hwnd == 0 || self.hwnd != expected_main_hwnd {
            return Err(BoundaryError::InvalidWindow);
        }
        runtime
            .validate_boundary_epoch(self.revocation_epoch)
            .map_err(BoundaryError::Runtime)
    }

    const fn owner_label(&self) -> &'static str {
        MAIN_WINDOW_LABEL
    }

    fn renew_after_lock(
        self,
        exposure: &WalletExposureAuthority,
        expected_main_hwnd: isize,
        runtime: &WalletRuntimeState,
    ) -> Result<Self, BoundaryError> {
        if self.hwnd == 0 || self.hwnd != expected_main_hwnd {
            return Err(BoundaryError::InvalidWindow);
        }
        exposure.validate(runtime)?;
        Ok(Self {
            revocation_epoch: exposure.revocation_epoch,
            hwnd: self.hwnd,
        })
    }
}

impl BoundaryFailClosedGuard<'_> {
    fn arm(runtime: &WalletRuntimeState) -> BoundaryFailClosedGuard<'_> {
        BoundaryFailClosedGuard {
            runtime,
            armed: true,
        }
    }

    fn commit(&mut self) {
        self.armed = false;
    }

    fn invalidate_or_terminate(&mut self) {
        if !self.armed {
            return;
        }
        match catch_unwind(AssertUnwindSafe(|| self.runtime.invalidate_all())) {
            Ok(Ok(())) => self.armed = false,
            Ok(Err(_)) | Err(_) => std::process::abort(),
        }
    }
}

impl Drop for BoundaryFailClosedGuard<'_> {
    fn drop(&mut self) {
        self.invalidate_or_terminate();
    }
}

impl BoundaryError {
    const fn code(&self) -> &'static str {
        match self {
            Self::Lifecycle(error) => error.code(),
            Self::Runtime(error) => runtime_error_code(*error),
            Self::InvalidRequest => "invalid_request",
            Self::InvalidWindow => "invalid_window",
            Self::ActivationUnavailable => "wallet_activation_unavailable",
            Self::SerializationUnavailable => "wallet_runtime_unavailable",
        }
    }
}

impl From<WalletLifecycleError> for BoundaryError {
    fn from(error: WalletLifecycleError) -> Self {
        Self::Lifecycle(error)
    }
}

impl WalletLifecycleCommandBoundary {
    pub(in crate::wallet) fn new_private(
        runtime: Arc<WalletRuntimeState>,
        adapters: Arc<WalletLifecycleAdapters>,
        expected_main_hwnd: isize,
    ) -> Self {
        Self {
            runtime,
            adapters,
            expected_main_hwnd,
            transport_policy: WholeEnvelopeTransportPolicy::production(),
            #[cfg(test)]
            panic_checkpoint: None,
        }
    }

    pub(in crate::wallet) fn execute<R: Runtime>(
        &self,
        request: WalletInvokeRequest<'_>,
        window: &WebviewWindow<R>,
    ) -> Result<Response, InvokeError> {
        self.run_fail_closed(request, |exposure| {
            MainWalletWindowAuthority::issue(
                window,
                self.expected_main_hwnd,
                exposure,
                &self.runtime,
            )
        })
    }

    pub(in crate::wallet) fn begin_recovery_selection<R, F>(
        self: &Arc<Self>,
        request: WalletInvokeRequest<'_>,
        window: &WebviewWindow<R>,
        completion: F,
    ) -> Result<(), InvokeError>
    where
        R: Runtime,
        F: FnOnce(Result<Response, InvokeError>) + Send + 'static,
    {
        let mut guard = BoundaryFailClosedGuard::arm(&self.runtime);
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            let envelope = parse_envelope(request)?;
            self.panic_at(BoundaryPanicCheckpoint::AfterEnvelopeValidation);
            let exposure = WalletExposureAuthority::issue(&self.runtime, &self.transport_policy)?;
            let window_authority = MainWalletWindowAuthority::issue(
                window,
                self.expected_main_hwnd,
                &exposure,
                &self.runtime,
            )?;
            let callback_boundary = Arc::clone(self);
            let callback = move |result: Result<RecoveryPathToken, WalletRuntimeError>| {
                let completed =
                    callback_boundary.finish_recovery_selection(result, exposure, window_authority);
                completion(completed);
            };
            match envelope {
                WalletLifecycleEnvelope::SelectRecoveryDestination => {
                    select_recovery_destination(window, Arc::clone(&self.runtime), callback)
                }
                WalletLifecycleEnvelope::SelectRecoverySource => {
                    select_recovery_source(window, Arc::clone(&self.runtime), callback)
                }
                _ => Err(WalletRuntimeError::InvalidRequest),
            }
            .map_err(BoundaryError::Runtime)
        }));

        match attempt {
            Ok(Ok(())) => {
                guard.commit();
                Ok(())
            }
            Ok(Err(error)) => {
                let prepared = fixed_invoke_error(error.code());
                guard.invalidate_or_terminate();
                Err(prepared)
            }
            Err(_) => {
                guard.invalidate_or_terminate();
                Err(build_panic_error_or_terminate())
            }
        }
    }

    fn run_fail_closed(
        &self,
        request: WalletInvokeRequest<'_>,
        issue_window: impl FnOnce(
            &WalletExposureAuthority,
        ) -> Result<MainWalletWindowAuthority, BoundaryError>,
    ) -> Result<Response, InvokeError> {
        let mut guard = BoundaryFailClosedGuard::arm(&self.runtime);
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            let result = (|| {
                let envelope = parse_envelope(request)?;
                self.panic_at(BoundaryPanicCheckpoint::AfterEnvelopeValidation);
                let exposure =
                    WalletExposureAuthority::issue(&self.runtime, &self.transport_policy)?;
                let window = issue_window(&exposure)?;
                exposure.validate(&self.runtime)?;
                window.validate(self.expected_main_hwnd, &self.runtime)?;

                let response = match envelope {
                    WalletLifecycleEnvelope::GetStatus => self
                        .serialize_response(&self.adapters.status().map_err(BoundaryError::from)?),
                    WalletLifecycleEnvelope::Create(request) => self.serialize_response(
                        &self
                            .adapters
                            .create_native(window.owner_label(), request)
                            .map_err(BoundaryError::from)?,
                    ),
                    WalletLifecycleEnvelope::Restore(request) => self.serialize_response(
                        &self
                            .adapters
                            .restore_native(window.owner_label(), request)
                            .map_err(BoundaryError::from)?,
                    ),
                    WalletLifecycleEnvelope::Unlock => self.serialize_response(
                        &self
                            .adapters
                            .unlock_native(window.owner_label())
                            .map_err(BoundaryError::from)?,
                    ),
                    WalletLifecycleEnvelope::Lock => {
                        let response = self.serialize_response(
                            &self.adapters.lock().map_err(BoundaryError::from)?,
                        )?;
                        let renewed_exposure =
                            WalletExposureAuthority::issue(&self.runtime, &self.transport_policy)?;
                        let renewed_window = window.renew_after_lock(
                            &renewed_exposure,
                            self.expected_main_hwnd,
                            &self.runtime,
                        )?;
                        renewed_window.validate(self.expected_main_hwnd, &self.runtime)?;
                        renewed_exposure.validate(&self.runtime)?;
                        return Ok(response);
                    }
                    WalletLifecycleEnvelope::SelectRecoveryDestination
                    | WalletLifecycleEnvelope::SelectRecoverySource => {
                        Err(BoundaryError::InvalidRequest)
                    }
                }?;

                window.validate(self.expected_main_hwnd, &self.runtime)?;
                exposure.validate(&self.runtime)?;
                Ok(response)
            })();
            result.map_err(|error: BoundaryError| fixed_invoke_error(error.code()))
        }));

        match attempt {
            Ok(Ok(response)) => {
                guard.commit();
                Ok(response)
            }
            Ok(Err(error)) => {
                guard.invalidate_or_terminate();
                Err(error)
            }
            Err(_) => {
                guard.invalidate_or_terminate();
                Err(build_panic_error_or_terminate())
            }
        }
    }

    fn finish_recovery_selection(
        &self,
        result: Result<RecoveryPathToken, WalletRuntimeError>,
        exposure: WalletExposureAuthority,
        window: MainWalletWindowAuthority,
    ) -> Result<Response, InvokeError> {
        let mut guard = BoundaryFailClosedGuard::arm(&self.runtime);
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            let result = (|| {
                exposure.validate(&self.runtime)?;
                window.validate(self.expected_main_hwnd, &self.runtime)?;
                let token = result.map_err(BoundaryError::Runtime)?;
                let response = self.serialize_response(&RecoverySelectionResponse {
                    recovery_selection_handle: token.as_str(),
                })?;
                window.validate(self.expected_main_hwnd, &self.runtime)?;
                exposure.validate(&self.runtime)?;
                Ok(response)
            })();
            result.map_err(|error: BoundaryError| fixed_invoke_error(error.code()))
        }));
        match attempt {
            Ok(Ok(response)) => {
                guard.commit();
                Ok(response)
            }
            Ok(Err(error)) => {
                guard.invalidate_or_terminate();
                Err(error)
            }
            Err(_) => {
                guard.invalidate_or_terminate();
                Err(build_panic_error_or_terminate())
            }
        }
    }

    #[cfg(test)]
    fn for_test(
        runtime: Arc<WalletRuntimeState>,
        adapters: Arc<WalletLifecycleAdapters>,
        expected_main_hwnd: isize,
    ) -> Self {
        Self {
            runtime,
            adapters,
            expected_main_hwnd,
            transport_policy: WholeEnvelopeTransportPolicy::approved_for_test(),
            panic_checkpoint: None,
        }
    }

    #[cfg(test)]
    fn with_panic_checkpoint(mut self, checkpoint: BoundaryPanicCheckpoint) -> Self {
        self.panic_checkpoint = Some(checkpoint);
        self
    }

    fn panic_at(&self, checkpoint: BoundaryPanicCheckpoint) {
        #[cfg(test)]
        if self.panic_checkpoint == Some(checkpoint) {
            panic!("injected lifecycle command boundary panic");
        }
        #[cfg(not(test))]
        let _ = checkpoint;
    }

    fn serialize_response<T: Serialize>(&self, value: &T) -> Result<Response, BoundaryError> {
        self.panic_at(BoundaryPanicCheckpoint::BeforeResponseSerialization);
        serialize_response(value)
    }
}

fn parse_envelope(
    request: WalletInvokeRequest<'_>,
) -> Result<WalletLifecycleEnvelope, BoundaryError> {
    let InvokeBody::Json(Value::Object(object)) = request.body else {
        return Err(BoundaryError::InvalidRequest);
    };
    if request.declared_command != request.invoked_command {
        return Err(BoundaryError::InvalidRequest);
    }
    match request.declared_command {
        GET_STATUS => require_empty(object).map(|()| WalletLifecycleEnvelope::GetStatus),
        SELECT_RECOVERY_DESTINATION => {
            require_empty(object).map(|()| WalletLifecycleEnvelope::SelectRecoveryDestination)
        }
        CREATE => deserialize_request(object).map(WalletLifecycleEnvelope::Create),
        SELECT_RECOVERY_SOURCE => {
            require_empty(object).map(|()| WalletLifecycleEnvelope::SelectRecoverySource)
        }
        RESTORE => deserialize_request(object).map(WalletLifecycleEnvelope::Restore),
        UNLOCK => require_empty(object).map(|()| WalletLifecycleEnvelope::Unlock),
        LOCK => require_empty(object).map(|()| WalletLifecycleEnvelope::Lock),
        _ => Err(BoundaryError::InvalidRequest),
    }
}

fn require_empty(object: &Map<String, Value>) -> Result<(), BoundaryError> {
    if object.is_empty() {
        Ok(())
    } else {
        Err(BoundaryError::InvalidRequest)
    }
}

fn deserialize_request<T>(object: &Map<String, Value>) -> Result<T, BoundaryError>
where
    T: serde::de::DeserializeOwned,
{
    if object.len() != 1 {
        return Err(BoundaryError::InvalidRequest);
    }
    let value = object.get("request").ok_or(BoundaryError::InvalidRequest)?;
    serde_json::from_value(value.clone()).map_err(|_| BoundaryError::InvalidRequest)
}

fn serialize_response<T: Serialize>(value: &T) -> Result<Response, BoundaryError> {
    serde_json::to_string(value)
        .map(Response::new)
        .map_err(|_| BoundaryError::SerializationUnavailable)
}

fn fixed_invoke_error(code: &'static str) -> InvokeError {
    InvokeError(serde_json::json!({ "code": code }))
}

fn build_panic_error_or_terminate() -> InvokeError {
    match catch_unwind(AssertUnwindSafe(|| {
        fixed_invoke_error("wallet_runtime_unavailable")
    })) {
        Ok(error) => error,
        Err(_) => std::process::abort(),
    }
}

const fn runtime_error_code(error: WalletRuntimeError) -> &'static str {
    match error {
        WalletRuntimeError::InvalidWindow => "invalid_window",
        WalletRuntimeError::ActivationUnavailable => "wallet_activation_unavailable",
        WalletRuntimeError::OperationInProgress => "operation_in_progress",
        WalletRuntimeError::InvalidRequest => "invalid_request",
        WalletRuntimeError::PathAuthorizationInvalid => "path_authorization_invalid",
        WalletRuntimeError::PathAuthorizationExpired => "path_authorization_expired",
        WalletRuntimeError::SecureRandomUnavailable => "secure_random_unavailable",
        WalletRuntimeError::RecoverySelectionCancelled => "recovery_selection_cancelled",
        WalletRuntimeError::RecoveryDestinationExists => "recovery_destination_exists",
        WalletRuntimeError::RecoveryDestinationInvalid
        | WalletRuntimeError::RecoverySourceInvalid => "recovery_storage_unavailable",
        WalletRuntimeError::ProcessLockUnavailable
        | WalletRuntimeError::UnsupportedWindowsHost
        | WalletRuntimeError::RuntimeUnavailable
        | WalletRuntimeError::ReconciliationUnavailable => "wallet_runtime_unavailable",
    }
}

fn is_bundled_windows_url(url: &Url) -> bool {
    url.origin().ascii_serialization() == BUNDLED_WINDOWS_ORIGIN
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tauri::ipc::IpcResponse;
    use tempfile::TempDir;

    const TEST_HWND: isize = 0x1234;

    fn request<'a>(command: &'static str, body: &'a InvokeBody) -> WalletInvokeRequest<'a> {
        WalletInvokeRequest {
            declared_command: command,
            invoked_command: command,
            body,
        }
    }

    fn json_body(value: Value) -> InvokeBody {
        InvokeBody::Json(value)
    }

    fn runtime_and_boundary(
        directory: &Path,
    ) -> (Arc<WalletRuntimeState>, WalletLifecycleCommandBoundary) {
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapters = Arc::new(WalletLifecycleAdapters::for_test(
            Arc::clone(&runtime),
            &directory.join("wallet.vault.json"),
        ));
        let boundary =
            WalletLifecycleCommandBoundary::for_test(Arc::clone(&runtime), adapters, TEST_HWND);
        (runtime, boundary)
    }

    fn main_window_authority(
        boundary: &WalletLifecycleCommandBoundary,
        exposure: &WalletExposureAuthority,
    ) -> Result<MainWalletWindowAuthority, BoundaryError> {
        MainWalletWindowAuthority::issue_from_parts(
            MAIN_WINDOW_LABEL,
            &Url::parse("http://tauri.localhost/").unwrap(),
            TEST_HWND,
            boundary.expected_main_hwnd,
            exposure,
            &boundary.runtime,
        )
    }

    fn execute_for_test(
        boundary: &WalletLifecycleCommandBoundary,
        command: &'static str,
        body: &InvokeBody,
    ) -> Result<Response, InvokeError> {
        boundary.run_fail_closed(request(command, body), |exposure| {
            main_window_authority(boundary, exposure)
        })
    }

    fn response_json(response: Response) -> Value {
        let body = response.body().unwrap();
        body.deserialize().unwrap()
    }

    fn error_json(error: InvokeError) -> Value {
        error.0
    }

    #[test]
    fn exact_empty_envelopes_cover_all_five_no_input_commands() {
        let empty = json_body(serde_json::json!({}));
        for (command, expected) in [
            (GET_STATUS, "status"),
            (SELECT_RECOVERY_DESTINATION, "destination"),
            (SELECT_RECOVERY_SOURCE, "source"),
            (UNLOCK, "unlock"),
            (LOCK, "lock"),
        ] {
            let parsed = parse_envelope(request(command, &empty)).unwrap();
            let actual = match parsed {
                WalletLifecycleEnvelope::GetStatus => "status",
                WalletLifecycleEnvelope::SelectRecoveryDestination => "destination",
                WalletLifecycleEnvelope::SelectRecoverySource => "source",
                WalletLifecycleEnvelope::Unlock => "unlock",
                WalletLifecycleEnvelope::Lock => "lock",
                WalletLifecycleEnvelope::Create(_) | WalletLifecycleEnvelope::Restore(_) => {
                    "request"
                }
            };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn no_input_commands_reject_extra_and_secret_like_top_level_fields() {
        for command in [
            GET_STATUS,
            SELECT_RECOVERY_DESTINATION,
            SELECT_RECOVERY_SOURCE,
            UNLOCK,
            LOCK,
        ] {
            for body in [
                json_body(serde_json::json!({ "extra": true })),
                json_body(serde_json::json!({ "password": "secret-canary" })),
                json_body(serde_json::json!({ "seed": "seed-canary" })),
            ] {
                assert!(matches!(
                    parse_envelope(request(command, &body)),
                    Err(BoundaryError::InvalidRequest)
                ));
            }
        }
    }

    #[test]
    fn whole_envelope_rejects_raw_non_object_unknown_and_wrong_case_inputs() {
        let bodies = [
            InvokeBody::Raw(b"{}".to_vec()),
            json_body(Value::Null),
            json_body(serde_json::json!([])),
            json_body(serde_json::json!({ "Request": {} })),
        ];
        for body in &bodies {
            assert!(matches!(
                parse_envelope(request(CREATE, body)),
                Err(BoundaryError::InvalidRequest)
            ));
        }
        let empty = json_body(serde_json::json!({}));
        assert!(matches!(
            parse_envelope(request("wallet_unknown", &empty)),
            Err(BoundaryError::InvalidRequest)
        ));
        let mismatched = WalletInvokeRequest {
            declared_command: GET_STATUS,
            invoked_command: LOCK,
            body: &empty,
        };
        assert!(matches!(
            parse_envelope(mismatched),
            Err(BoundaryError::InvalidRequest)
        ));
    }

    #[test]
    fn create_and_restore_require_one_exact_nested_request() {
        let handle = "ab".repeat(32);
        let create = json_body(serde_json::json!({
            "request": {
                "wallet_id": "wallet-1",
                "label": "Primary",
                "recovery_destination_handle": handle,
            }
        }));
        assert!(matches!(
            parse_envelope(request(CREATE, &create)),
            Ok(WalletLifecycleEnvelope::Create(_))
        ));

        let source_handle = "cd".repeat(32);
        let restore = json_body(serde_json::json!({
            "request": {
                "wallet_id": "wallet-2",
                "label": "Restored",
                "recovery_source_handle": source_handle,
            }
        }));
        assert!(matches!(
            parse_envelope(request(RESTORE, &restore)),
            Ok(WalletLifecycleEnvelope::Restore(_))
        ));

        for body in [
            json_body(serde_json::json!({})),
            json_body(serde_json::json!({ "request": {}, "extra": true })),
            json_body(serde_json::json!({ "request": {
                "wallet_id": "wallet-1",
                "label": "Primary",
                "recovery_destination_handle": "ab".repeat(32),
                "password": "secret-canary",
            }})),
        ] {
            assert!(matches!(
                parse_envelope(request(CREATE, &body)),
                Err(BoundaryError::InvalidRequest)
            ));
        }

        let oversized = json_body(serde_json::json!({
            "request": {
                "wallet_id": "w".repeat(65),
                "label": "Primary",
                "recovery_destination_handle": "ab".repeat(32),
            }
        }));
        assert!(matches!(
            parse_envelope(request(CREATE, &oversized)),
            Err(BoundaryError::InvalidRequest)
        ));

        let wrong_command = json_body(serde_json::json!({
            "request": {
                "wallet_id": "wallet-1",
                "label": "Primary",
                "recovery_destination_handle": "ab".repeat(32),
            }
        }));
        assert!(matches!(
            parse_envelope(request(RESTORE, &wrong_command)),
            Err(BoundaryError::InvalidRequest)
        ));
    }

    #[test]
    fn production_transport_policy_structurally_blocks_exposure() {
        let runtime = WalletRuntimeState::for_test();
        let policy = WholeEnvelopeTransportPolicy::production();
        assert!(matches!(
            WalletExposureAuthority::issue(&runtime, &policy),
            Err(BoundaryError::ActivationUnavailable)
        ));
    }

    #[test]
    fn production_activation_policy_also_blocks_exposure() {
        let runtime = WalletRuntimeState::for_test_with_production_activation();
        let policy = WholeEnvelopeTransportPolicy::approved_for_test();
        assert!(matches!(
            WalletExposureAuthority::issue(&runtime, &policy),
            Err(BoundaryError::ActivationUnavailable)
        ));
    }

    #[test]
    fn private_production_boundary_returns_only_activation_unavailable() {
        let directory = TempDir::new().unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapters = Arc::new(WalletLifecycleAdapters::for_test(
            Arc::clone(&runtime),
            &directory.path().join("wallet.vault.json"),
        ));
        let boundary =
            WalletLifecycleCommandBoundary::new_private(Arc::clone(&runtime), adapters, TEST_HWND);
        let empty = json_body(serde_json::json!({}));
        let error = match boundary.run_fail_closed(request(GET_STATUS, &empty), |exposure| {
            main_window_authority(&boundary, exposure)
        }) {
            Ok(_) => panic!("production boundary must remain unavailable"),
            Err(error) => error,
        };
        assert_eq!(
            error_json(error),
            serde_json::json!({ "code": "wallet_activation_unavailable" })
        );
    }

    #[test]
    fn window_authority_requires_exact_label_origin_handle_and_live_epoch() {
        let runtime = WalletRuntimeState::for_test();
        let policy = WholeEnvelopeTransportPolicy::approved_for_test();
        let exposure = WalletExposureAuthority::issue(&runtime, &policy).unwrap();
        let trusted = Url::parse("http://tauri.localhost/").unwrap();
        assert!(MainWalletWindowAuthority::issue_from_parts(
            MAIN_WINDOW_LABEL,
            &trusted,
            TEST_HWND,
            TEST_HWND,
            &exposure,
            &runtime,
        )
        .is_ok());

        for (label, url, hwnd) in [
            ("secondary", "http://tauri.localhost/", TEST_HWND),
            (MAIN_WINDOW_LABEL, "https://tauri.localhost/", TEST_HWND),
            (MAIN_WINDOW_LABEL, "http://127.0.0.1/", TEST_HWND),
            (MAIN_WINDOW_LABEL, "http://tauri.localhost/", 0),
            (MAIN_WINDOW_LABEL, "http://tauri.localhost/", TEST_HWND + 1),
        ] {
            assert!(matches!(
                MainWalletWindowAuthority::issue_from_parts(
                    label,
                    &Url::parse(url).unwrap(),
                    hwnd,
                    TEST_HWND,
                    &exposure,
                    &runtime,
                ),
                Err(BoundaryError::InvalidWindow)
            ));
        }

        runtime.invalidate_all().unwrap();
        assert!(exposure.validate(&runtime).is_err());
    }

    #[test]
    fn public_status_is_pre_serialized_inside_the_boundary() {
        let directory = TempDir::new().unwrap();
        let (_runtime, boundary) = runtime_and_boundary(directory.path());
        let empty = json_body(serde_json::json!({}));
        let response = execute_for_test(&boundary, GET_STATUS, &empty).unwrap();
        assert_eq!(
            response_json(response),
            serde_json::json!({
                "vault_exists": false,
                "locked": true,
                "account": null,
            })
        );
    }

    #[test]
    fn malformed_input_returns_only_fixed_error_and_revokes_runtime() {
        let directory = TempDir::new().unwrap();
        let (runtime, boundary) = runtime_and_boundary(directory.path());
        let exposure = WalletExposureAuthority::issue(
            &runtime,
            &WholeEnvelopeTransportPolicy::approved_for_test(),
        )
        .unwrap();
        let invalid = json_body(serde_json::json!({ "password": "secret-canary" }));
        let error = match execute_for_test(&boundary, GET_STATUS, &invalid) {
            Ok(_) => panic!("malformed input must fail"),
            Err(error) => error,
        };
        let serialized = error_json(error);
        assert_eq!(serialized, serde_json::json!({ "code": "invalid_request" }));
        assert!(!serialized.to_string().contains("secret-canary"));
        assert!(exposure.validate(&runtime).is_err());
    }

    #[test]
    fn boundary_panic_returns_fixed_error_and_revokes_runtime() {
        let directory = TempDir::new().unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapters = Arc::new(WalletLifecycleAdapters::for_test(
            Arc::clone(&runtime),
            &directory.path().join("wallet.vault.json"),
        ));
        let boundary =
            WalletLifecycleCommandBoundary::for_test(Arc::clone(&runtime), adapters, TEST_HWND)
                .with_panic_checkpoint(BoundaryPanicCheckpoint::AfterEnvelopeValidation);
        let exposure = WalletExposureAuthority::issue(
            &runtime,
            &WholeEnvelopeTransportPolicy::approved_for_test(),
        )
        .unwrap();
        let empty = json_body(serde_json::json!({}));
        let error = match execute_for_test(&boundary, GET_STATUS, &empty) {
            Ok(_) => panic!("injected boundary panic must fail"),
            Err(error) => error,
        };
        assert_eq!(
            error_json(error),
            serde_json::json!({ "code": "wallet_runtime_unavailable" })
        );
        assert!(exposure.validate(&runtime).is_err());
    }

    #[test]
    fn response_serialization_panic_returns_fixed_error_and_revokes_runtime() {
        let directory = TempDir::new().unwrap();
        let runtime = Arc::new(WalletRuntimeState::for_test());
        let adapters = Arc::new(WalletLifecycleAdapters::for_test(
            Arc::clone(&runtime),
            &directory.path().join("wallet.vault.json"),
        ));
        let boundary =
            WalletLifecycleCommandBoundary::for_test(Arc::clone(&runtime), adapters, TEST_HWND)
                .with_panic_checkpoint(BoundaryPanicCheckpoint::BeforeResponseSerialization);
        let exposure = WalletExposureAuthority::issue(
            &runtime,
            &WholeEnvelopeTransportPolicy::approved_for_test(),
        )
        .unwrap();
        let empty = json_body(serde_json::json!({}));
        let error = match execute_for_test(&boundary, GET_STATUS, &empty) {
            Ok(_) => panic!("injected serialization panic must fail"),
            Err(error) => error,
        };
        assert_eq!(
            error_json(error),
            serde_json::json!({ "code": "wallet_runtime_unavailable" })
        );
        assert!(exposure.validate(&runtime).is_err());
    }

    #[test]
    fn completed_selection_returns_only_the_opaque_handle() {
        use super::super::runtime::RecoveryPathPurpose;

        let directory = TempDir::new().unwrap();
        let (runtime, boundary) = runtime_and_boundary(directory.path());
        let exposure = WalletExposureAuthority::issue(
            &runtime,
            &WholeEnvelopeTransportPolicy::approved_for_test(),
        )
        .unwrap();
        let window = main_window_authority(&boundary, &exposure).unwrap();
        let permit = runtime
            .begin_recovery_path_selection(MAIN_WINDOW_LABEL, RecoveryPathPurpose::Destination)
            .unwrap();
        let token = runtime
            .complete_recovery_path_selection(permit, directory.path().join("backup.json"))
            .unwrap();
        let response = boundary
            .finish_recovery_selection(Ok(token), exposure, window)
            .unwrap();
        let value = response_json(response);
        let handle = value["recovery_selection_handle"].as_str().unwrap();
        assert_eq!(value.as_object().map(Map::len), Some(1));
        assert_eq!(handle.len(), 64);
        assert!(handle
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert!(!value.to_string().contains("backup.json"));
        assert!(!value
            .to_string()
            .contains(directory.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn selection_cancellation_returns_fixed_error_and_revokes_authority() {
        let directory = TempDir::new().unwrap();
        let (runtime, boundary) = runtime_and_boundary(directory.path());
        let exposure = WalletExposureAuthority::issue(
            &runtime,
            &WholeEnvelopeTransportPolicy::approved_for_test(),
        )
        .unwrap();
        let epoch = exposure.revocation_epoch;
        let window = main_window_authority(&boundary, &exposure).unwrap();
        let error = match boundary.finish_recovery_selection(
            Err(WalletRuntimeError::RecoverySelectionCancelled),
            exposure,
            window,
        ) {
            Ok(_) => panic!("cancelled selection must not succeed"),
            Err(error) => error,
        };
        assert_eq!(
            error_json(error),
            serde_json::json!({ "code": "recovery_selection_cancelled" })
        );
        assert!(runtime.validate_boundary_epoch(epoch).is_err());
    }

    #[test]
    fn lock_response_is_exact_and_storage_independent() {
        let directory = TempDir::new().unwrap();
        let (_runtime, boundary) = runtime_and_boundary(directory.path());
        let empty = json_body(serde_json::json!({}));
        let response = execute_for_test(&boundary, LOCK, &empty).unwrap();
        assert_eq!(
            response_json(response),
            serde_json::json!({ "locked": true })
        );
    }
}
