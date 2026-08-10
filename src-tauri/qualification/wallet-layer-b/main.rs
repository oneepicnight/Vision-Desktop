#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

#[cfg(not(target_os = "windows"))]
compile_error!("the Wallet Layer B transport harness is Windows-only");

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    ffi::c_void,
    io::{self, Write},
    net::{TcpListener, TcpStream},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering},
        OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    http,
    ipc::{CommandArg, CommandItem, InvokeBody, InvokeError, Response},
    Manager, Runtime, State, WebviewWindow,
};
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindow};

const QUALIFICATION_ARGUMENT: &str = "--wallet-layer-b-qualification";
const QUALIFICATION_CASE_ARGUMENT: &str = "--wallet-layer-b-case=";
const QUALIFICATION_WINDOW: &str = "wallet-transport-qualification";
const QUALIFICATION_OTHER_WINDOW: &str = "wallet-transport-qualification-other";
const QUALIFICATION_CONTROLLER_WINDOW: &str = "wallet-transport-qualification-controller";
const QUALIFICATION_HOST: &str = "tauri.localhost";
const REPORT_PROTOCOL: &str = "qualification-report";
const CONTROL_PROTOCOL: &str = "qualification-control";
const MAX_REPORT_BYTES: usize = 2_048;

const GET_STATUS: &str = "wallet_get_status";
const SELECT_RECOVERY_DESTINATION: &str = "wallet_select_recovery_destination";
const CREATE: &str = "wallet_create";
const SELECT_RECOVERY_SOURCE: &str = "wallet_select_recovery_source";
const RESTORE: &str = "wallet_restore";
const UNLOCK: &str = "wallet_unlock";
const LOCK: &str = "wallet_lock";

const ALL_COMMANDS: &[&str] = &[
    GET_STATUS,
    SELECT_RECOVERY_DESTINATION,
    CREATE,
    SELECT_RECOVERY_SOURCE,
    RESTORE,
    UNLOCK,
    LOCK,
];

const NO_INPUT_COMMANDS: &[&str] = &[
    GET_STATUS,
    SELECT_RECOVERY_DESTINATION,
    SELECT_RECOVERY_SOURCE,
    UNLOCK,
    LOCK,
];

const TAURI_CALLBACK_HEADER: &str = "tauri-callback";
const TAURI_ERROR_HEADER: &str = "tauri-error";
const TAURI_INVOKE_KEY_HEADER: &str = "tauri-invoke-key";
const ORIGIN_HEADER: &str = "origin";
const MAX_OBSERVED_TOP_LEVEL_KEYS: usize = 8;

struct QualificationInvokeRequest<'a> {
    declared_command: &'static str,
    invoked_command: &'a str,
    body: &'a InvokeBody,
    headers: &'a http::HeaderMap,
}

impl<'a, R: Runtime> CommandArg<'a, R> for QualificationInvokeRequest<'a> {
    fn from_command(command: CommandItem<'a, R>) -> Result<Self, InvokeError> {
        Ok(Self {
            declared_command: command.name,
            invoked_command: command.message.command(),
            body: command.message.payload(),
            headers: command.message.headers(),
        })
    }
}

struct QualificationState {
    selected_case: Box<str>,
    invoke_key: OnceLock<Box<str>>,
    loaded_webview2_version: OnceLock<Box<str>>,
    authorized_hwnd: AtomicIsize,
    page_generation: AtomicUsize,
    authorized_generation: AtomicUsize,
    scenario_triggered: AtomicBool,
    destroy_on_invoke: AtomicBool,
    revoke_on_invoke: AtomicBool,
    browser_primary_records: AtomicUsize,
    browser_post_records: AtomicUsize,
    browser_primary_passed: AtomicBool,
    browser_primary_inconclusive: AtomicBool,
    browser_post_required: AtomicBool,
    browser_post_passed: AtomicBool,
    browser_terminal_seen: AtomicBool,
    wrapper_entries: AtomicUsize,
    accepted: AtomicUsize,
    rejected: AtomicUsize,
    invalidations: AtomicUsize,
    revoked: AtomicBool,
}

impl QualificationState {
    fn new(selected_case: Box<str>) -> Self {
        Self {
            selected_case,
            invoke_key: OnceLock::new(),
            loaded_webview2_version: OnceLock::new(),
            authorized_hwnd: AtomicIsize::new(0),
            page_generation: AtomicUsize::new(0),
            authorized_generation: AtomicUsize::new(0),
            scenario_triggered: AtomicBool::new(false),
            destroy_on_invoke: AtomicBool::new(false),
            revoke_on_invoke: AtomicBool::new(false),
            browser_primary_records: AtomicUsize::new(0),
            browser_post_records: AtomicUsize::new(0),
            browser_primary_passed: AtomicBool::new(false),
            browser_primary_inconclusive: AtomicBool::new(false),
            browser_post_required: AtomicBool::new(false),
            browser_post_passed: AtomicBool::new(false),
            browser_terminal_seen: AtomicBool::new(false),
            wrapper_entries: AtomicUsize::new(0),
            accepted: AtomicUsize::new(0),
            rejected: AtomicUsize::new(0),
            invalidations: AtomicUsize::new(0),
            revoked: AtomicBool::new(false),
        }
    }

    fn initialize_invoke_key(&self, invoke_key: &str) -> Result<(), ()> {
        self.invoke_key
            .set(invoke_key.to_owned().into_boxed_str())
            .map_err(|_| ())
    }

    fn invoke_key(&self) -> Option<&str> {
        self.invoke_key.get().map(AsRef::as_ref)
    }

    fn set_loaded_webview2_version(&self, version: String) -> Result<(), ()> {
        self.loaded_webview2_version
            .set(version.into_boxed_str())
            .map_err(|_| ())
    }

    fn loaded_webview2_version(&self) -> Option<&str> {
        self.loaded_webview2_version.get().map(AsRef::as_ref)
    }

    fn set_authorized_hwnd(&self, hwnd: isize) -> Result<(), ()> {
        self.authorized_hwnd
            .compare_exchange(0, hwnd, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| ())
    }

    fn note_page_load(&self, url: &tauri::Url) {
        if is_bootstrap_page(url) {
            return;
        }
        let generation = self.page_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = self.authorized_generation.compare_exchange(
            0,
            generation,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn window_authority_matches(&self, hwnd: isize) -> bool {
        self.authorized_hwnd.load(Ordering::Acquire) == hwnd
            && self.authorized_generation.load(Ordering::Acquire) != 0
            && self.page_generation.load(Ordering::Acquire)
                == self.authorized_generation.load(Ordering::Acquire)
    }

    fn expected_report_window(&self) -> &str {
        if self.selected_case.contains("window-other-local--") {
            QUALIFICATION_OTHER_WINDOW
        } else {
            QUALIFICATION_WINDOW
        }
    }

    fn invalidate(&self) {
        if !self.revoked.swap(true, Ordering::AcqRel) {
            self.invalidations.fetch_add(1, Ordering::AcqRel);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
enum StartupPlan {
    Main,
    OtherLocal,
    RemoteOrigin,
    RecreatedMain,
}

fn startup_plan(selected_case: &str) -> StartupPlan {
    if selected_case.contains("window-other-local--") {
        StartupPlan::OtherLocal
    } else if selected_case.contains("window-remote-origin--") {
        StartupPlan::RemoteOrigin
    } else if selected_case.contains("window-recreated-main--") {
        StartupPlan::RecreatedMain
    } else {
        StartupPlan::Main
    }
}

struct FailClosedGuard<'a> {
    state: &'a QualificationState,
    armed: bool,
}

impl<'a> FailClosedGuard<'a> {
    fn arm(state: &'a QualificationState) -> Self {
        Self { state, armed: true }
    }

    fn commit(&mut self) {
        self.armed = false;
    }
}

impl Drop for FailClosedGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.state.invalidate();
        }
    }
}

#[derive(Serialize)]
struct QualificationResponse {
    marker: &'static str,
    command: &'static str,
    route: &'static str,
    body_kind: &'static str,
    top_level_key_count: &'static str,
    top_level_shape: &'static str,
    wrapper_ran: bool,
    command_body_ran: bool,
}

#[derive(Serialize)]
struct QualificationObservation<'a> {
    marker: &'static str,
    timestamp_ms: u128,
    command: &'a str,
    route: &'a str,
    body_kind: &'static str,
    top_level_key_count: &'static str,
    top_level_shape: &'static str,
    result: &'static str,
    wrapper_ran: bool,
    command_body_ran: bool,
}

struct BodyMetadata {
    kind: &'static str,
    key_count: &'static str,
    shape: &'static str,
}

fn body_metadata(body: &InvokeBody) -> BodyMetadata {
    match body {
        InvokeBody::Raw(_) => BodyMetadata {
            kind: "raw",
            key_count: "not_applicable",
            shape: "raw_body",
        },
        InvokeBody::Json(Value::Object(object)) => {
            let key_count = match object.len() {
                0 => "zero",
                1 => "one",
                2..=MAX_OBSERVED_TOP_LEVEL_KEYS => "two_to_eight",
                _ => "over_limit",
            };
            let oversized = object.keys().any(|key| key.len() > 64);
            let shape = if object.len() > MAX_OBSERVED_TOP_LEVEL_KEYS {
                "excessive_key_count"
            } else if oversized {
                "oversized_key"
            } else if object.is_empty() {
                "empty_object"
            } else if object.len() == 1 && object.contains_key("request") {
                "request_only"
            } else {
                "unknown_or_mixed_keys"
            };
            BodyMetadata {
                kind: "json_object",
                key_count,
                shape,
            }
        }
        InvokeBody::Json(_) => BodyMetadata {
            kind: "json_non_object",
            key_count: "not_applicable",
            shape: "non_object",
        },
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FrameworkRoute {
    CustomProtocol,
    PostMessage,
    Inconclusive,
}

impl FrameworkRoute {
    fn label(self) -> &'static str {
        match self {
            Self::CustomProtocol => "custom_protocol_proven",
            Self::PostMessage => "post_message_proven",
            Self::Inconclusive => "transport_route_inconclusive",
        }
    }
}

fn framework_route(headers: &http::HeaderMap, expected_invoke_key: Option<&str>) -> FrameworkRoute {
    let custom_protocol_headers = [
        TAURI_CALLBACK_HEADER,
        TAURI_ERROR_HEADER,
        TAURI_INVOKE_KEY_HEADER,
        ORIGIN_HEADER,
    ];
    let matching_key = expected_invoke_key.is_some_and(|expected| {
        headers
            .get(TAURI_INVOKE_KEY_HEADER)
            .and_then(|value| value.to_str().ok())
            == Some(expected)
    });
    if matching_key
        && custom_protocol_headers
            .iter()
            .all(|header| headers.contains_key(*header))
    {
        return FrameworkRoute::CustomProtocol;
    }
    if custom_protocol_headers
        .iter()
        .all(|header| !headers.contains_key(*header))
    {
        return FrameworkRoute::PostMessage;
    }
    FrameworkRoute::Inconclusive
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PanicPoint {
    None,
    Metadata,
    Body,
    Response,
    FixedError,
    Observation,
}

fn requested_panic(headers: &http::HeaderMap) -> PanicPoint {
    match headers
        .get("x-vision-qualification-panic")
        .and_then(|value| value.to_str().ok())
    {
        Some("metadata") => PanicPoint::Metadata,
        Some("body") => PanicPoint::Body,
        Some("response") => PanicPoint::Response,
        Some("fixed-error") => PanicPoint::FixedError,
        Some("observation") => PanicPoint::Observation,
        _ => PanicPoint::None,
    }
}

fn valid_identifier(value: &Value, max: usize) -> bool {
    value.as_str().is_some_and(|value| {
        !value.is_empty()
            && value.len() <= max
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    })
}

fn valid_label(value: &Value) -> bool {
    value.as_str().is_some_and(|value| {
        !value.is_empty()
            && value.len() <= 64
            && value.trim() == value
            && !value.chars().any(char::is_control)
    })
}

fn valid_handle(value: &Value) -> bool {
    value.as_str().is_some_and(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn valid_public_request(command: &str, request: &Map<String, Value>) -> bool {
    let handle_name = match command {
        CREATE => "recovery_destination_handle",
        RESTORE => "recovery_source_handle",
        _ => return false,
    };
    request.len() == 3
        && valid_identifier(request.get("wallet_id").unwrap_or(&Value::Null), 64)
        && valid_label(request.get("label").unwrap_or(&Value::Null))
        && valid_handle(request.get(handle_name).unwrap_or(&Value::Null))
}

fn valid_envelope(command: &str, body: &InvokeBody) -> bool {
    let InvokeBody::Json(Value::Object(object)) = body else {
        return false;
    };
    if NO_INPUT_COMMANDS.contains(&command) {
        return object.is_empty();
    }
    if !matches!(command, CREATE | RESTORE) || object.len() != 1 {
        return false;
    }
    object
        .get("request")
        .and_then(Value::as_object)
        .is_some_and(|request| valid_public_request(command, request))
}

fn is_qualification_origin(url: &tauri::Url) -> bool {
    url.scheme() == "http"
        && url.host_str() == Some(QUALIFICATION_HOST)
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn is_bootstrap_page(url: &tauri::Url) -> bool {
    is_qualification_origin(url) && url.path() == "/bootstrap.html"
}

fn local_harness_url() -> Result<tauri::Url, &'static str> {
    tauri::Url::parse("http://tauri.localhost/index.html")
        .map_err(|_| "Layer B local harness URL is invalid")
}

fn fixed_error(code: &'static str) -> InvokeError {
    InvokeError(serde_json::json!({ "code": code }))
}

fn serialize_response(value: &QualificationResponse) -> Result<Response, InvokeError> {
    serde_json::to_string(value)
        .map(Response::new)
        .map_err(|_| fixed_error("qualification_response_unavailable"))
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis())
}

#[link(name = "ole32")]
extern "system" {
    fn CoTaskMemFree(memory: *const c_void);
}

fn parse_webview2_runtime_version(raw: *const u16) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    let mut length = 0_usize;
    while length <= 64 {
        if unsafe { *raw.add(length) } == 0 {
            break;
        }
        length += 1;
    }
    if length == 0 || length > 64 {
        return None;
    }
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(raw, length) }).ok()?;
    let mut segments = value.split('.');
    if (0..4).all(|_| {
        segments
            .next()
            .is_some_and(|part| !part.is_empty() && part.len() <= 5 && part.parse::<u32>().is_ok())
    }) && segments.next().is_none()
    {
        Some(value)
    } else {
        None
    }
}

fn capture_loaded_webview2_runtime<R: Runtime>(
    window: &WebviewWindow<R>,
    app_handle: tauri::AppHandle<R>,
) -> tauri::Result<()> {
    window.with_webview(move |webview| {
        let mut raw_version = Default::default();
        let environment = webview.environment();
        let result = unsafe { environment.BrowserVersionString(&mut raw_version) };
        let raw = raw_version.as_ptr();
        let version = result
            .ok()
            .and_then(|()| parse_webview2_runtime_version(raw));
        if !raw.is_null() {
            unsafe { CoTaskMemFree(raw.cast()) };
        }
        let state = app_handle.state::<QualificationState>();
        if version
            .and_then(|version| state.set_loaded_webview2_version(version).ok())
            .is_none()
        {
            state.invalidate();
        }
    })
}

fn emit_observation(observation: &impl Serialize) -> Result<(), ()> {
    let line = serde_json::to_string(observation).map_err(|_| ())?;
    let mut stdout = io::stdout().lock();
    stdout.write_all(line.as_bytes()).map_err(|_| ())?;
    stdout.write_all(b"\n").map_err(|_| ())?;
    stdout.flush().map_err(|_| ())
}

fn guarded_rejection(
    state: &QualificationState,
    command: &'static str,
    route: FrameworkRoute,
    metadata: &BodyMetadata,
    code: &'static str,
) -> Result<InvokeError, InvokeError> {
    state.invalidate();
    let error = fixed_error(code);
    emit_observation(&QualificationObservation {
        marker: "layer_b_observation",
        timestamp_ms: timestamp_ms(),
        command,
        route: route.label(),
        body_kind: metadata.kind,
        top_level_key_count: metadata.key_count,
        top_level_shape: metadata.shape,
        result: code,
        wrapper_ran: true,
        command_body_ran: false,
    })
    .map_err(|_| fixed_error("qualification_runtime_unavailable"))?;
    Ok(error)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserObservationRequest {
    marker: String,
    case: String,
    client_api: String,
    command: String,
    outcome: String,
    expected: String,
    result: String,
    transport_evidence: String,
    fallback_intercepted: bool,
}

#[derive(Serialize)]
struct BrowserObservation<'a> {
    marker: &'static str,
    case: &'a str,
    client_api: &'a str,
    command: &'a str,
    outcome: &'a str,
    expected: &'a str,
    result: &'a str,
    transport_evidence: &'a str,
    fallback_intercepted: bool,
}

#[derive(Serialize)]
struct TerminalObservation<'a> {
    marker: &'static str,
    case: &'a str,
    result: &'a str,
    wrapper_entries: usize,
    primary_records: usize,
    post_records: usize,
    revoked: bool,
    webview2_runtime_version: &'a str,
}

#[derive(Serialize)]
struct NativeDestructionObservation<'a> {
    marker: &'static str,
    case: &'a str,
    command: &'static str,
    route: &'static str,
    body_kind: &'static str,
    top_level_key_count: &'static str,
    top_level_shape: &'static str,
    result: &'static str,
    wrapper_ran: bool,
    command_body_ran: bool,
    native_destruction_records: usize,
    exact_authorized_hwnd: bool,
    exact_target_window: bool,
    destroy_call_succeeded: bool,
    target_window_absent: bool,
    native_hwnd_absent: bool,
    authority_revoked: bool,
    structural_post_revocation_proven: bool,
}

struct NativeDestructionRequest<'a> {
    expected_command: &'static str,
    invoked_command: &'a str,
    route: FrameworkRoute,
    body: &'a InvokeBody,
    metadata: &'a BodyMetadata,
    url: &'a tauri::Url,
    hwnd: isize,
}

fn fixed_member<'a>(value: &'a str, allowed: &[&str]) -> Option<&'a str> {
    allowed
        .iter()
        .any(|candidate| candidate == &value)
        .then_some(value)
}

fn valid_case_name(value: &str) -> bool {
    let fixed = [
        "window-other-local",
        "window-remote-origin",
        "window-recreated-main",
        "window-reloaded-generation",
        "window-destruction-race",
        "window-revocation-race",
        "exact-status-internals-invoke",
        "exact-status-internals-ipc",
        "raw-empty",
        "raw-json-looking",
        "raw-arbitrary",
        "raw-bytes",
        "json-null",
        "json-boolean",
        "json-number",
        "json-string",
        "json-array",
        "extra-top-level",
        "wrong-case-top-level",
        "secret-like-top-level",
        "key-name-canary-top-level",
        "oversized-key-top-level",
        "excessive-key-count-top-level",
        "create-empty",
        "create-request-empty",
        "create-request-wrong-type",
        "create-unknown-field",
        "create-secret-like-field",
        "create-invalid-handle",
        "restore-wrong-handle-name",
        "unknown-command",
        "direct-fetch-text-missing-invoke-key",
        "direct-fetch-bytes-missing-invoke-key",
        "direct-xhr-missing-invoke-key",
        "forced-post-message-fallback",
        "contained-metadata-panic",
        "contained-body-panic",
        "contained-response-panic",
        "contained-observation-panic",
        "contained-fixed-error-panic",
        "concurrent-batch",
    ];
    if fixed.contains(&value) {
        return true;
    }
    if wrapper_case_parts(value).is_some() {
        return true;
    }
    if let Some(command) = value.strip_prefix("exact-") {
        return [
            GET_STATUS,
            SELECT_RECOVERY_DESTINATION,
            CREATE,
            SELECT_RECOVERY_SOURCE,
            RESTORE,
            UNLOCK,
            LOCK,
        ]
        .contains(&command);
    }
    let duplicate = value
        .strip_prefix("duplicate-")
        .or_else(|| value.strip_prefix("nested-duplicate-"));
    let Some(duplicate) = duplicate else {
        return false;
    };
    let duplicate = duplicate
        .strip_prefix("create-")
        .or_else(|| duplicate.strip_prefix("restore-"));
    let Some(duplicate) = duplicate else {
        return false;
    };
    let representations = ["string", "bytes", "object-normalized"];
    let families = [
        "identical",
        "conflicting",
        "valid-then-malformed",
        "malformed-then-valid",
        "public-then-secret-like",
        "exact-and-wrong-case",
        "three-repeated",
        "escaped-equivalent",
        "bounded-whitespace",
    ];
    representations.iter().any(|representation| {
        duplicate
            .strip_suffix(&format!("-{representation}"))
            .is_some_and(|family| families.contains(&family))
    })
}

fn wrapper_case_parts(value: &str) -> Option<(&'static str, &str)> {
    let value = value.strip_prefix("wrapper-")?;
    let families = [
        "exact",
        "raw-empty",
        "raw-json-looking",
        "raw-arbitrary",
        "raw-bytes",
        "json-null",
        "json-boolean",
        "json-number",
        "json-string",
        "json-array",
        "shape-mismatch",
        "missing-top-level",
        "extra-top-level",
        "wrong-case-top-level",
        "secret-like-top-level",
        "wrong-command-envelope",
        "declared-invoked-mismatch",
        "malformed-nested",
        "oversized-nested",
        "unknown-nested",
        "secret-like-nested",
        "window-other-local",
        "window-remote-origin",
        "window-recreated-main",
        "window-reloaded-generation",
        "window-destruction-race",
        "window-revocation-race",
        "panic-metadata",
        "panic-body",
        "panic-response",
        "panic-observation",
        "panic-fixed-error",
        "sequential-repeat",
        "concurrent-batch",
        "reordered-invoke",
        "post-revocation",
    ];
    ALL_COMMANDS.iter().find_map(|command| {
        value
            .strip_prefix(command)
            .and_then(|suffix| suffix.strip_prefix('-'))
            .filter(|family| families.contains(family))
            .map(|family| (*command, family))
    })
}

fn mismatch_declared_command(selected_case: &str, invoked_command: &str) -> Option<&'static str> {
    let (declared_command, family) = selected_case
        .rsplit_once("--")
        .and_then(|(case, _)| wrapper_case_parts(case))?;
    if family != "declared-invoked-mismatch" {
        return None;
    }
    let index = ALL_COMMANDS
        .iter()
        .position(|command| *command == declared_command)?;
    (invoked_command == ALL_COMMANDS[(index + 1) % ALL_COMMANDS.len()]).then_some(declared_command)
}

fn valid_case_selector(value: &str) -> bool {
    let Some((case, route)) = value.rsplit_once("--") else {
        return false;
    };
    valid_case_name(case)
        && matches!(
            route,
            "official-invoke"
                | "internals-invoke"
                | "internals-ipc"
                | "internals-post-message"
                | "direct-fetch-text"
                | "direct-fetch-bytes"
                | "direct-xhr"
        )
}

fn validated_browser_observation<'a>(
    selected_case: &str,
    request: &'a BrowserObservationRequest,
) -> Option<BrowserObservation<'a>> {
    if request.marker != "layer_b_browser_observation"
        || (request.case != selected_case
            && request.case != format!("{selected_case}-post-revocation-proof"))
    {
        return None;
    }
    if request.command != "matrix" {
        if let Some((expected_command, family)) = selected_case
            .rsplit_once("--")
            .and_then(|(case, _)| wrapper_case_parts(case))
        {
            let expected_reported_command = if family == "declared-invoked-mismatch" {
                let index = ALL_COMMANDS
                    .iter()
                    .position(|command| *command == expected_command)?;
                ALL_COMMANDS[(index + 1) % ALL_COMMANDS.len()]
            } else {
                expected_command
            };
            if request.command != expected_reported_command {
                return None;
            }
        }
    }
    let client_api = fixed_member(
        &request.client_api,
        &[
            "official-invoke",
            "internals-invoke",
            "internals-ipc",
            "internals-post-message",
            "direct-fetch-text",
            "direct-fetch-bytes",
            "direct-xhr",
            "matrix-controller",
        ],
    )?;
    let command = fixed_member(
        &request.command,
        &[
            GET_STATUS,
            SELECT_RECOVERY_DESTINATION,
            CREATE,
            SELECT_RECOVERY_SOURCE,
            RESTORE,
            UNLOCK,
            LOCK,
            "wallet_unknown",
            "matrix",
        ],
    )?;
    let outcomes = [
        "layer_b_accepted",
        "invalid_request",
        "qualification_invalid_window",
        "qualification_response_unavailable",
        "qualification_runtime_unavailable",
        "qualification_transport_inconclusive",
        "framework_error_redacted",
        "framework_rejection",
        "transport_rejection",
        "unexpected_success",
        "unexpected_success_shape",
        "unclassified_error",
        "case_not_observed",
        "matrix_complete",
        "matrix_inconclusive",
    ];
    let outcome = fixed_member(&request.outcome, &outcomes)?;
    let expected = fixed_member(&request.expected, &outcomes)?;
    let result = fixed_member(&request.result, &["passed", "failed", "inconclusive"])?;
    let transport_evidence = fixed_member(
        &request.transport_evidence,
        &[
            "custom_protocol_proven",
            "post_message_proven",
            "transport_route_inconclusive",
            "framework_rejected_before_wrapper",
            "not_applicable",
        ],
    )?;
    if request.result == "passed" && request.outcome != request.expected {
        return None;
    }
    if request.result == "inconclusive"
        && !matches!(
            request.outcome.as_str(),
            "qualification_transport_inconclusive" | "case_not_observed" | "matrix_inconclusive"
        )
    {
        return None;
    }
    if request.outcome == "layer_b_accepted"
        && !matches!(
            transport_evidence,
            "custom_protocol_proven" | "post_message_proven"
        )
    {
        return None;
    }
    if selected_case.starts_with("forced-post-message-fallback--")
        && request.command != "matrix"
        && request.result == "passed"
        && (!request.fallback_intercepted || transport_evidence != "post_message_proven")
    {
        return None;
    }
    Some(BrowserObservation {
        marker: "layer_b_browser_observation",
        case: &request.case,
        client_api,
        command,
        outcome,
        expected,
        result,
        transport_evidence,
        fallback_intercepted: request.fallback_intercepted,
    })
}

fn post_revocation_required(expected: &str, outcome: &str) -> bool {
    outcome == "qualification_transport_inconclusive"
        || matches!(
            expected,
            "invalid_request"
                | "qualification_invalid_window"
                | "qualification_response_unavailable"
                | "qualification_runtime_unavailable"
        )
}

fn expected_wrapper_entries(selected_case: &str, post_required: bool) -> usize {
    let wrapper_family = selected_case
        .rsplit_once("--")
        .and_then(|(case, _)| wrapper_case_parts(case))
        .map(|(_, family)| family);
    if selected_case.starts_with("direct-") || selected_case.starts_with("unknown-command--") {
        0
    } else if matches!(
        wrapper_family,
        Some("sequential-repeat" | "reordered-invoke")
    ) {
        2
    } else if wrapper_family == Some("concurrent-batch")
        || selected_case.starts_with("concurrent-batch--")
    {
        8 + usize::from(post_required)
    } else if post_required {
        2
    } else {
        1
    }
}

fn valid_native_destruction_request(
    selected_case: &str,
    expected_command: &'static str,
    invoked_command: &str,
    route: FrameworkRoute,
    body: &InvokeBody,
) -> bool {
    let selected_wrapper = selected_case
        .rsplit_once("--")
        .filter(|(_, selected_route)| *selected_route == "official-invoke")
        .and_then(|(case, _)| wrapper_case_parts(case));
    selected_wrapper == Some((expected_command, "window-destruction-race"))
        && route == FrameworkRoute::CustomProtocol
        && invoked_command == expected_command
        && valid_envelope(expected_command, body)
}

fn complete_native_destruction(
    window: &WebviewWindow,
    state: &QualificationState,
    request: NativeDestructionRequest<'_>,
) -> Result<Response, InvokeError> {
    let app = window.app_handle().clone();
    let exact_authorized_hwnd = state.authorized_hwnd.load(Ordering::Acquire) == request.hwnd;
    let exact_target_window = window.label() == QUALIFICATION_WINDOW
        && is_qualification_origin(request.url)
        && state.window_authority_matches(request.hwnd);
    let request_valid = valid_native_destruction_request(
        &state.selected_case,
        request.expected_command,
        request.invoked_command,
        request.route,
        request.body,
    );
    state.invalidate();
    let destroy_call_succeeded =
        exact_authorized_hwnd && exact_target_window && request_valid && window.destroy().is_ok();
    let target_window_absent = app.get_webview_window(QUALIFICATION_WINDOW).is_none();
    let native_hwnd_absent = unsafe { IsWindow(request.hwnd as HWND) } == 0;
    let authority_revoked = state.revoked.load(Ordering::Acquire);
    let structural_post_revocation_proven =
        authority_revoked && target_window_absent && native_hwnd_absent;
    let passed = destroy_call_succeeded
        && target_window_absent
        && native_hwnd_absent
        && structural_post_revocation_proven;
    let result = if passed { "passed" } else { "failed" };
    let runtime_version = state.loaded_webview2_version().unwrap_or("unavailable");
    let emitted = emit_observation(&NativeDestructionObservation {
        marker: "layer_b_native_destruction_observation",
        case: &state.selected_case,
        command: request.expected_command,
        route: request.route.label(),
        body_kind: request.metadata.kind,
        top_level_key_count: request.metadata.key_count,
        top_level_shape: request.metadata.shape,
        result,
        wrapper_ran: true,
        command_body_ran: true,
        native_destruction_records: 1,
        exact_authorized_hwnd,
        exact_target_window,
        destroy_call_succeeded,
        target_window_absent,
        native_hwnd_absent,
        authority_revoked,
        structural_post_revocation_proven,
    })
    .and_then(|()| {
        emit_observation(&TerminalObservation {
            marker: "layer_b_terminal_observation",
            case: &state.selected_case,
            result,
            wrapper_entries: state.wrapper_entries.load(Ordering::Acquire),
            primary_records: 0,
            post_records: 0,
            revoked: authority_revoked,
            webview2_runtime_version: runtime_version,
        })
    });
    app.exit(if passed && emitted.is_ok() { 0 } else { 2 });
    Err(fixed_error("qualification_runtime_unavailable"))
}

fn report_protocol<R: Runtime>(
    context: tauri::UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    let state = context.app_handle().state::<QualificationState>();
    let mut guard = FailClosedGuard::arm(&state);
    let processed = catch_unwind(AssertUnwindSafe(|| {
        if context.webview_label() != state.expected_report_window()
            || request.method() != http::Method::POST
            || request.body().len() > MAX_REPORT_BYTES
        {
            return Err(());
        }
        let request =
            serde_json::from_slice::<BrowserObservationRequest>(request.body()).map_err(|_| ())?;
        let observation = validated_browser_observation(&state.selected_case, &request).ok_or(())?;
        let is_terminal = request.command == "matrix";
        let is_post = request.case.ends_with("-post-revocation-proof");
        if is_terminal {
            let webview2_runtime_version = state.loaded_webview2_version().ok_or(())?;
            let wrapper_entries = state.wrapper_entries.load(Ordering::Acquire);
            let expected_wrapper_entries = expected_wrapper_entries(
                &state.selected_case,
                state.browser_post_required.load(Ordering::Acquire),
            );
            if state.browser_terminal_seen.swap(true, Ordering::AcqRel)
                || state.browser_primary_records.load(Ordering::Acquire) != 1
                || wrapper_entries != expected_wrapper_entries
                || state.browser_primary_passed.load(Ordering::Acquire)
                    != (request.result == "passed")
                || state.browser_primary_inconclusive.load(Ordering::Acquire)
                    != (request.result == "inconclusive")
                || (state.browser_post_required.load(Ordering::Acquire)
                    && (state.browser_post_records.load(Ordering::Acquire) != 1
                        || !state.browser_post_passed.load(Ordering::Acquire)))
                || (!state.browser_post_required.load(Ordering::Acquire)
                    && state.browser_post_records.load(Ordering::Acquire) != 0)
            {
                return Err(());
            }
            emit_observation(&TerminalObservation {
                marker: "layer_b_terminal_observation",
                case: &state.selected_case,
                result: &request.result,
                wrapper_entries,
                primary_records: state.browser_primary_records.load(Ordering::Acquire),
                post_records: state.browser_post_records.load(Ordering::Acquire),
                revoked: state.revoked.load(Ordering::Acquire),
                webview2_runtime_version,
            })?;
        } else if is_post {
            if state.browser_post_records.fetch_add(1, Ordering::AcqRel) != 0
                || !state.revoked.load(Ordering::Acquire)
            {
                return Err(());
            }
            state
                .browser_post_passed
                .store(request.result == "passed", Ordering::Release);
        } else {
            if state.browser_primary_records.fetch_add(1, Ordering::AcqRel) != 0 {
                return Err(());
            }
            state
                .browser_primary_passed
                .store(request.result == "passed", Ordering::Release);
            state
                .browser_primary_inconclusive
                .store(request.result == "inconclusive", Ordering::Release);
            state.browser_post_required.store(
                post_revocation_required(&request.expected, &request.outcome),
                Ordering::Release,
            );
        }
        emit_observation(&observation)?;
        Ok::<Option<i32>, ()>(is_terminal.then_some(match request.result.as_str() {
            "passed" => 0,
            "inconclusive" => 3,
            _ => 2,
        }))
    }));
    let (accepted, exit_code) = match processed {
        Ok(Ok(exit_code)) => (true, exit_code),
        _ => (false, None),
    };
    if !accepted {
        state.invalidate();
        let _ = io::stderr().write_all(b"layer_b_browser_observation_rejected\n");
    } else {
        guard.commit();
    }
    if let Some(exit_code) = exit_code {
        let app_handle = context.app_handle().clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            app_handle.exit(exit_code);
        });
    }
    http::Response::builder()
        .status(if accepted {
            http::StatusCode::NO_CONTENT
        } else {
            http::StatusCode::BAD_REQUEST
        })
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Vec::new())
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

fn control_response(status: http::StatusCode, phase: &'static str) -> http::Response<Vec<u8>> {
    let body = format!("{{\"phase\":\"{phase}\"}}").into_bytes();
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

fn schedule_window_replacement<R: Runtime>(app_handle: tauri::AppHandle<R>, reload_only: bool) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let app_for_main = app_handle.clone();
        if app_handle
            .run_on_main_thread(move || {
                let replacement = (|| -> Result<(), ()> {
                    let window = app_for_main
                        .get_webview_window(QUALIFICATION_WINDOW)
                        .ok_or(())?;
                    if reload_only {
                        window.reload().map_err(|_| ())?;
                        return Ok(());
                    }
                    if app_for_main
                        .get_webview_window(QUALIFICATION_CONTROLLER_WINDOW)
                        .is_none()
                    {
                        return Err(());
                    }
                    window.destroy().map_err(|_| ())?;
                    tauri::WebviewWindowBuilder::new(
                        &app_for_main,
                        QUALIFICATION_WINDOW,
                        tauri::WebviewUrl::App("index.html".into()),
                    )
                    .title("Vision Wallet Transport Qualification - Recreated")
                    .build()
                    .map_err(|_| ())?;
                    if let Some(controller) =
                        app_for_main.get_webview_window(QUALIFICATION_CONTROLLER_WINDOW)
                    {
                        controller.destroy().map_err(|_| ())?;
                    }
                    Ok(())
                })();
                if replacement.is_err() {
                    app_for_main.state::<QualificationState>().invalidate();
                    let _ = io::stderr().write_all(b"layer_b_window_replacement_failed\n");
                    app_for_main.exit(2);
                }
            })
            .is_err()
        {
            app_handle.state::<QualificationState>().invalidate();
            let _ = io::stderr().write_all(b"layer_b_window_replacement_failed\n");
            app_handle.exit(2);
        }
    });
}

fn write_remote_response(
    mut stream: TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn start_remote_origin_server(selected_case: &str) -> Result<tauri::Url, &'static str> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "Layer B remote-origin listener could not bind")?;
    let address = listener
        .local_addr()
        .map_err(|_| "Layer B remote-origin address is unavailable")?;
    let selected_case = serde_json::to_string(selected_case)
        .map_err(|_| "Layer B remote-origin case could not be encoded")?;
    let index = include_str!("assets/index.html").replace(
        "<script src=\"harness.js\"></script>",
        &format!(
            "<script>Object.defineProperty(window,'__VISION_LAYER_B_CASE__',{{value:{selected_case},writable:false,configurable:false}});</script><script src=\"harness.js\"></script>"
        ),
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0_u8; 2_048];
            let Ok(read) = std::io::Read::read(&mut stream, &mut request) else {
                continue;
            };
            let path = std::str::from_utf8(&request[..read])
                .ok()
                .and_then(|request| request.lines().next())
                .and_then(|line| line.split_ascii_whitespace().nth(1))
                .unwrap_or("/invalid");
            let _ = match path {
                "/" | "/index.html" => write_remote_response(
                    stream,
                    "200 OK",
                    "text/html; charset=utf-8",
                    index.as_bytes(),
                ),
                "/harness.js" => write_remote_response(
                    stream,
                    "200 OK",
                    "text/javascript; charset=utf-8",
                    include_bytes!("assets/harness.js"),
                ),
                "/harness.css" => write_remote_response(
                    stream,
                    "200 OK",
                    "text/css; charset=utf-8",
                    include_bytes!("assets/harness.css"),
                ),
                _ => write_remote_response(stream, "404 Not Found", "text/plain", b"not found"),
            };
        }
    });
    tauri::Url::parse(&format!("http://{address}/"))
        .map_err(|_| "Layer B remote-origin URL is invalid")
}

fn control_protocol<R: Runtime>(
    context: tauri::UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    let state = context.app_handle().state::<QualificationState>();
    if context.webview_label() != state.expected_report_window()
        || request.method() != http::Method::POST
    {
        state.invalidate();
        return control_response(http::StatusCode::BAD_REQUEST, "rejected");
    }
    match (request.uri().path(), state.selected_case.as_ref()) {
        ("/recreate", case) if case.contains("window-recreated-main--") => {
            if state.scenario_triggered.swap(true, Ordering::AcqRel) {
                control_response(http::StatusCode::OK, "ready")
            } else {
                schedule_window_replacement(context.app_handle().clone(), false);
                control_response(http::StatusCode::OK, "reloading")
            }
        }
        ("/reload", case) if case.contains("window-reloaded-generation--") => {
            if state.scenario_triggered.swap(true, Ordering::AcqRel) {
                control_response(http::StatusCode::OK, "ready")
            } else {
                schedule_window_replacement(context.app_handle().clone(), true);
                control_response(http::StatusCode::OK, "reloading")
            }
        }
        ("/destroy-race", case) if case.contains("window-destruction-race--") => {
            state.destroy_on_invoke.store(true, Ordering::Release);
            control_response(http::StatusCode::OK, "ready")
        }
        ("/revocation-race", case) if case.contains("window-revocation-race--") => {
            state.revoke_on_invoke.store(true, Ordering::Release);
            control_response(http::StatusCode::OK, "ready")
        }
        _ => {
            state.invalidate();
            control_response(http::StatusCode::BAD_REQUEST, "rejected")
        }
    }
}

fn qualify(
    expected_command: &'static str,
    request: QualificationInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, QualificationState>,
) -> Result<Response, InvokeError> {
    let mut guard = FailClosedGuard::arm(&state);
    let attempt = catch_unwind(AssertUnwindSafe(|| {
        state.wrapper_entries.fetch_add(1, Ordering::AcqRel);
        let panic_point = requested_panic(request.headers);
        if panic_point == PanicPoint::Metadata {
            panic!("injected Layer B metadata panic");
        }
        let metadata = body_metadata(request.body);
        let route = framework_route(request.headers, state.invoke_key());
        if state.revoked.load(Ordering::Acquire) {
            return Err(guarded_rejection(
                &state,
                expected_command,
                route,
                &metadata,
                "qualification_runtime_unavailable",
            )?);
        }
        if panic_point == PanicPoint::Body {
            panic!("injected Layer B qualification panic");
        }
        let url = window
            .url()
            .map_err(|_| fixed_error("qualification_invalid_window"))?;
        let hwnd = window
            .hwnd()
            .map_err(|_| fixed_error("qualification_invalid_window"))?
            .0 as isize;
        if state.revoke_on_invoke.swap(false, Ordering::AcqRel) {
            state.invalidate();
        }
        if state.destroy_on_invoke.swap(false, Ordering::AcqRel) {
            return complete_native_destruction(
                &window,
                &state,
                NativeDestructionRequest {
                    expected_command,
                    invoked_command: request.invoked_command,
                    route,
                    body: request.body,
                    metadata: &metadata,
                    url: &url,
                    hwnd,
                },
            );
        }
        if state.revoked.load(Ordering::Acquire) {
            return Err(guarded_rejection(
                &state,
                expected_command,
                route,
                &metadata,
                "qualification_runtime_unavailable",
            )?);
        }
        if window.label() != QUALIFICATION_WINDOW
            || !is_qualification_origin(&url)
            || !state.window_authority_matches(hwnd)
        {
            return Err(guarded_rejection(
                &state,
                expected_command,
                route,
                &metadata,
                "qualification_invalid_window",
            )?);
        }
        if route == FrameworkRoute::Inconclusive {
            return Err(guarded_rejection(
                &state,
                expected_command,
                route,
                &metadata,
                "qualification_transport_inconclusive",
            )?);
        }
        if request.declared_command != expected_command
            || request.invoked_command != expected_command
            || !valid_envelope(expected_command, request.body)
        {
            state.invalidate();
            if panic_point == PanicPoint::FixedError {
                panic!("injected Layer B fixed-error panic");
            }
            if panic_point == PanicPoint::Observation {
                panic!("injected Layer B observation panic");
            }
            return Err(guarded_rejection(
                &state,
                expected_command,
                route,
                &metadata,
                "invalid_request",
            )?);
        }
        if panic_point == PanicPoint::Response {
            panic!("injected Layer B response panic");
        }
        let response = QualificationResponse {
            marker: "layer_b_accepted",
            command: expected_command,
            route: route.label(),
            body_kind: metadata.kind,
            top_level_key_count: metadata.key_count,
            top_level_shape: metadata.shape,
            wrapper_ran: true,
            command_body_ran: true,
        };
        let response = serialize_response(&response)?;
        let observation = QualificationObservation {
            marker: "layer_b_observation",
            timestamp_ms: timestamp_ms(),
            command: expected_command,
            route: route.label(),
            body_kind: metadata.kind,
            top_level_key_count: metadata.key_count,
            top_level_shape: metadata.shape,
            result: "accepted",
            wrapper_ran: true,
            command_body_ran: true,
        };
        if panic_point == PanicPoint::Observation {
            panic!("injected Layer B observation panic");
        }
        emit_observation(&observation)
            .map_err(|_| fixed_error("qualification_runtime_unavailable"))?;
        Ok(response)
    }));

    match attempt {
        Ok(Ok(response)) => {
            state.accepted.fetch_add(1, Ordering::AcqRel);
            guard.commit();
            Ok(response)
        }
        Ok(Err(error)) => {
            state.rejected.fetch_add(1, Ordering::AcqRel);
            state.invalidate();
            Err(error)
        }
        Err(_) => {
            state.invalidate();
            Err(catch_unwind(AssertUnwindSafe(|| {
                fixed_error("qualification_runtime_unavailable")
            }))
            .unwrap_or(InvokeError(Value::Null)))
        }
    }
}

macro_rules! qualification_command {
    ($name:ident, $command:expr) => {
        #[tauri::command]
        fn $name(
            request: QualificationInvokeRequest<'_>,
            window: WebviewWindow,
            state: State<'_, QualificationState>,
        ) -> Result<Response, InvokeError> {
            qualify($command, request, window, state)
        }
    };
}

qualification_command!(wallet_get_status, GET_STATUS);
qualification_command!(
    wallet_select_recovery_destination,
    SELECT_RECOVERY_DESTINATION
);
qualification_command!(wallet_create, CREATE);
qualification_command!(wallet_select_recovery_source, SELECT_RECOVERY_SOURCE);
qualification_command!(wallet_restore, RESTORE);
qualification_command!(wallet_unlock, UNLOCK);
qualification_command!(wallet_lock, LOCK);

fn qualification_invoke_handler(invoke: tauri::ipc::Invoke<tauri::Wry>) -> bool {
    let mismatch = {
        let webview = invoke.message.webview();
        let state = webview.state::<QualificationState>();
        mismatch_declared_command(&state.selected_case, invoke.message.command())
    };
    match mismatch {
        Some(GET_STATUS) => __cmd__wallet_get_status!(wallet_get_status, invoke),
        Some(SELECT_RECOVERY_DESTINATION) => {
            __cmd__wallet_select_recovery_destination!(wallet_select_recovery_destination, invoke)
        }
        Some(CREATE) => __cmd__wallet_create!(wallet_create, invoke),
        Some(SELECT_RECOVERY_SOURCE) => {
            __cmd__wallet_select_recovery_source!(wallet_select_recovery_source, invoke)
        }
        Some(RESTORE) => __cmd__wallet_restore!(wallet_restore, invoke),
        Some(UNLOCK) => __cmd__wallet_unlock!(wallet_unlock, invoke),
        Some(LOCK) => __cmd__wallet_lock!(wallet_lock, invoke),
        Some(_) => false,
        None => match invoke.message.command() {
            GET_STATUS => __cmd__wallet_get_status!(wallet_get_status, invoke),
            SELECT_RECOVERY_DESTINATION => {
                __cmd__wallet_select_recovery_destination!(
                    wallet_select_recovery_destination,
                    invoke
                )
            }
            CREATE => __cmd__wallet_create!(wallet_create, invoke),
            SELECT_RECOVERY_SOURCE => {
                __cmd__wallet_select_recovery_source!(wallet_select_recovery_source, invoke)
            }
            RESTORE => __cmd__wallet_restore!(wallet_restore, invoke),
            UNLOCK => __cmd__wallet_unlock!(wallet_unlock, invoke),
            LOCK => __cmd__wallet_lock!(wallet_lock, invoke),
            _ => false,
        },
    }
}

fn qualification_mode_requested() -> bool {
    std::env::args().any(|argument| argument == QUALIFICATION_ARGUMENT)
}

fn selected_case() -> Option<Box<str>> {
    std::env::args()
        .find_map(|argument| {
            argument
                .strip_prefix(QUALIFICATION_CASE_ARGUMENT)
                .map(str::to_owned)
        })
        .filter(|case| valid_case_selector(case))
        .map(String::into_boxed_str)
}

fn main() {
    if !qualification_mode_requested() {
        eprintln!("Layer B qualification mode was not explicitly requested");
        std::process::exit(64);
    }
    let Some(selected_case) = selected_case() else {
        eprintln!("One valid Layer B qualification case is required");
        std::process::exit(64);
    };
    let initialization_script = format!(
        "Object.defineProperty(window,'__VISION_LAYER_B_CASE__',{{value:{},writable:false,configurable:false}});",
        serde_json::to_string(selected_case.as_ref()).unwrap_or_else(|_| "null".to_owned())
    );
    std::panic::set_hook(Box::new(|_| {
        eprintln!("Layer B qualification panic was contained");
    }));
    tauri::Builder::default()
        .append_invoke_initialization_script(initialization_script)
        .register_uri_scheme_protocol(REPORT_PROTOCOL, report_protocol)
        .register_uri_scheme_protocol(CONTROL_PROTOCOL, control_protocol)
        .manage(QualificationState::new(selected_case))
        .on_page_load(|webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                webview
                    .state::<QualificationState>()
                    .note_page_load(payload.url());
            }
        })
        .setup(|app| {
            let state = app.state::<QualificationState>();
            state
                .initialize_invoke_key(app.handle().invoke_key())
                .map_err(|_| "Layer B invoke-key state was already initialized")?;
            let main = app
                .get_webview_window(QUALIFICATION_WINDOW)
                .ok_or("Layer B main qualification window was not created")?;
            state
                .set_authorized_hwnd(
                    main.hwnd()
                        .map_err(|_| "Layer B main window has no native identity")?
                        .0 as isize,
                )
                .map_err(|_| "Layer B native identity was already initialized")?;
            match startup_plan(&state.selected_case) {
                StartupPlan::OtherLocal => {
                    let other = tauri::WebviewWindowBuilder::new(
                        app,
                        QUALIFICATION_OTHER_WINDOW,
                        tauri::WebviewUrl::App("index.html".into()),
                    )
                    .title("Vision Wallet Transport Qualification - Other Window")
                    .build()?;
                    capture_loaded_webview2_runtime(&other, app.handle().clone())?;
                    main.destroy()
                        .map_err(|_| "Layer B could not retire the bootstrap window")?;
                }
                StartupPlan::RemoteOrigin => {
                    capture_loaded_webview2_runtime(&main, app.handle().clone())?;
                    main.navigate(start_remote_origin_server(&state.selected_case)?)?;
                }
                StartupPlan::RecreatedMain => {
                    capture_loaded_webview2_runtime(&main, app.handle().clone())?;
                    tauri::WebviewWindowBuilder::new(
                        app,
                        QUALIFICATION_CONTROLLER_WINDOW,
                        tauri::WebviewUrl::App("bootstrap.html".into()),
                    )
                    .title("Vision Wallet Transport Qualification Controller")
                    .visible(false)
                    .build()?;
                    main.navigate(local_harness_url()?)?;
                }
                StartupPlan::Main => {
                    capture_loaded_webview2_runtime(&main, app.handle().clone())?;
                    main.navigate(local_harness_url()?)?;
                }
            }
            Ok(())
        })
        .invoke_handler(qualification_invoke_handler)
        .run(tauri::generate_context!(
            "qualification/wallet-layer-b/tauri.conf.json"
        ))
        .expect("Layer B qualification runtime failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANDLE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn json(value: Value) -> InvokeBody {
        InvokeBody::Json(value)
    }

    #[test]
    fn no_input_commands_require_exact_empty_json_objects() {
        for command in NO_INPUT_COMMANDS {
            assert!(valid_envelope(command, &json(serde_json::json!({}))));
            assert!(!valid_envelope(
                command,
                &json(serde_json::json!({ "extra": true }))
            ));
            assert!(!valid_envelope(command, &InvokeBody::Raw(b"{}".to_vec())));
        }
    }

    #[test]
    fn create_and_restore_accept_only_bounded_public_requests() {
        let create = serde_json::json!({ "request": {
            "wallet_id": "operator_1", "label": "Operator Wallet",
            "recovery_destination_handle": HANDLE
        }});
        let restore = serde_json::json!({ "request": {
            "wallet_id": "operator_1", "label": "Operator Wallet",
            "recovery_source_handle": HANDLE
        }});
        assert!(valid_envelope(CREATE, &json(create)));
        assert!(valid_envelope(RESTORE, &json(restore)));
    }

    #[test]
    fn secret_like_unknown_and_malformed_fields_are_rejected() {
        for value in [
            serde_json::json!({ "request": { "wallet_id": "a", "label": "A", "recovery_destination_handle": HANDLE, "password": "canary" }}),
            serde_json::json!({ "request": { "wallet_id": "bad id", "label": "A", "recovery_destination_handle": HANDLE }}),
            serde_json::json!({ "Request": { "wallet_id": "a", "label": "A", "recovery_destination_handle": HANDLE }}),
        ] {
            assert!(!valid_envelope(CREATE, &json(value)));
        }
    }

    #[test]
    fn metadata_uses_only_fixed_allowlisted_classifications() {
        let metadata = body_metadata(&json(serde_json::json!({
            "PUBLIC_SECRET_KEY_NAME_CANARY": "PUBLIC_VALUE_CANARY",
            "request": { "password": "PUBLIC_VALUE_CANARY" }
        })));
        assert_eq!(metadata.kind, "json_object");
        assert_eq!(metadata.key_count, "two_to_eight");
        assert_eq!(metadata.shape, "unknown_or_mixed_keys");
        let serialized = serde_json::to_string(&QualificationObservation {
            marker: "layer_b_observation",
            timestamp_ms: 0,
            command: CREATE,
            route: "custom_protocol_proven",
            body_kind: metadata.kind,
            top_level_key_count: metadata.key_count,
            top_level_shape: metadata.shape,
            result: "invalid_request",
            wrapper_ran: true,
            command_body_ran: false,
        })
        .unwrap();
        assert!(!serialized.contains("PUBLIC_SECRET_KEY_NAME_CANARY"));
        assert!(!serialized.contains("PUBLIC_VALUE_CANARY"));
    }

    #[test]
    fn metadata_bounds_excessive_counts_and_oversized_keys() {
        let mut excessive = Map::new();
        for index in 0..=MAX_OBSERVED_TOP_LEVEL_KEYS {
            excessive.insert(format!("field_{index}"), Value::Null);
        }
        let metadata = body_metadata(&json(Value::Object(excessive)));
        assert_eq!(metadata.key_count, "over_limit");
        assert_eq!(metadata.shape, "excessive_key_count");

        let oversized = "x".repeat(65);
        let mut object = Map::new();
        object.insert(oversized, Value::Null);
        let metadata = body_metadata(&json(Value::Object(object)));
        assert_eq!(metadata.key_count, "one");
        assert_eq!(metadata.shape, "oversized_key");
    }

    #[test]
    fn route_is_derived_from_framework_evidence_not_caller_labels() {
        let invoke_key = "private-framework-key";
        let mut custom = http::HeaderMap::new();
        custom.insert(TAURI_CALLBACK_HEADER, "1".parse().unwrap());
        custom.insert(TAURI_ERROR_HEADER, "2".parse().unwrap());
        custom.insert(TAURI_INVOKE_KEY_HEADER, invoke_key.parse().unwrap());
        custom.insert(ORIGIN_HEADER, "http://tauri.localhost".parse().unwrap());
        custom.insert(
            "x-vision-qualification-route",
            "caller-asserted-and-ignored".parse().unwrap(),
        );
        assert!(matches!(
            framework_route(&custom, Some(invoke_key)),
            FrameworkRoute::CustomProtocol
        ));
        assert!(matches!(
            framework_route(&custom, Some("wrong-key")),
            FrameworkRoute::Inconclusive
        ));

        let mut post_message = http::HeaderMap::new();
        post_message.insert(
            "x-vision-qualification-route",
            "caller-asserted-and-ignored".parse().unwrap(),
        );
        assert!(matches!(
            framework_route(&post_message, Some(invoke_key)),
            FrameworkRoute::PostMessage
        ));
    }

    #[test]
    fn panic_points_are_classified_without_formatting_header_values() {
        for (name, expected) in [
            ("metadata", PanicPoint::Metadata),
            ("body", PanicPoint::Body),
            ("response", PanicPoint::Response),
            ("fixed-error", PanicPoint::FixedError),
            ("observation", PanicPoint::Observation),
        ] {
            let mut headers = http::HeaderMap::new();
            headers.insert("x-vision-qualification-panic", name.parse().unwrap());
            assert!(requested_panic(&headers) == expected);
        }
    }

    #[test]
    fn fail_closed_guard_revokes_when_rejection_is_not_committed() {
        let state = QualificationState::new("case--official-invoke".into());
        {
            let _guard = FailClosedGuard::arm(&state);
        }
        assert!(state.revoked.load(Ordering::Acquire));
        assert_eq!(state.invalidations.load(Ordering::Acquire), 1);
    }

    #[test]
    fn report_validation_rejects_canaries_and_accepts_only_the_selected_case() {
        let selected = "exact-wallet_get_status--official-invoke";
        let valid = BrowserObservationRequest {
            marker: "layer_b_browser_observation".into(),
            case: selected.into(),
            client_api: "official-invoke".into(),
            command: GET_STATUS.into(),
            outcome: "layer_b_accepted".into(),
            expected: "layer_b_accepted".into(),
            result: "passed".into(),
            transport_evidence: "custom_protocol_proven".into(),
            fallback_intercepted: false,
        };
        assert!(validated_browser_observation(selected, &valid).is_some());

        let mut canary = valid;
        canary.case = "PASSWORD_CANARY".into();
        assert!(validated_browser_observation(selected, &canary).is_none());

        let wrapper_selected = "wrapper-wallet_lock-exact--official-invoke";
        let wrong_wrapper = BrowserObservationRequest {
            marker: "layer_b_browser_observation".into(),
            case: wrapper_selected.into(),
            client_api: "official-invoke".into(),
            command: GET_STATUS.into(),
            outcome: "layer_b_accepted".into(),
            expected: "layer_b_accepted".into(),
            result: "passed".into(),
            transport_evidence: "custom_protocol_proven".into(),
            fallback_intercepted: false,
        };
        assert!(validated_browser_observation(wrapper_selected, &wrong_wrapper).is_none());
    }

    #[test]
    fn browser_pass_requires_matching_outcome_and_proven_transport() {
        let selected = "exact-wallet_get_status--official-invoke";
        let mut request = BrowserObservationRequest {
            marker: "layer_b_browser_observation".into(),
            case: selected.into(),
            client_api: "official-invoke".into(),
            command: GET_STATUS.into(),
            outcome: "invalid_request".into(),
            expected: "layer_b_accepted".into(),
            result: "passed".into(),
            transport_evidence: "custom_protocol_proven".into(),
            fallback_intercepted: false,
        };
        assert!(validated_browser_observation(selected, &request).is_none());
        request.outcome = "layer_b_accepted".into();
        request.transport_evidence = "transport_route_inconclusive".into();
        assert!(validated_browser_observation(selected, &request).is_none());
    }

    #[test]
    fn unproven_transport_is_distinct_and_requires_revocation_evidence() {
        let selected = "exact-wallet_get_status--official-invoke";
        let request = BrowserObservationRequest {
            marker: "layer_b_browser_observation".into(),
            case: selected.into(),
            client_api: "official-invoke".into(),
            command: GET_STATUS.into(),
            outcome: "qualification_transport_inconclusive".into(),
            expected: "layer_b_accepted".into(),
            result: "inconclusive".into(),
            transport_evidence: "transport_route_inconclusive".into(),
            fallback_intercepted: false,
        };
        assert!(validated_browser_observation(selected, &request).is_some());
        assert!(post_revocation_required(
            &request.expected,
            &request.outcome
        ));
        assert_eq!(expected_wrapper_entries(selected, true), 2);
    }

    #[test]
    fn concurrency_batch_requires_all_eight_wrapper_entries() {
        let selected = "wrapper-wallet_get_status-concurrent-batch--official-invoke";
        assert!(valid_case_selector(selected));
        assert_eq!(expected_wrapper_entries(selected, false), 8);
        assert_eq!(expected_wrapper_entries(selected, true), 9);
        assert!(!valid_case_selector("concurrent-0--official-invoke"));
    }

    #[test]
    fn every_wrapper_accepts_the_complete_applicable_case_matrix() {
        let common = [
            "exact",
            "raw-empty",
            "raw-json-looking",
            "raw-arbitrary",
            "raw-bytes",
            "json-null",
            "json-boolean",
            "json-number",
            "json-string",
            "json-array",
            "shape-mismatch",
            "missing-top-level",
            "extra-top-level",
            "wrong-case-top-level",
            "secret-like-top-level",
            "wrong-command-envelope",
            "declared-invoked-mismatch",
            "window-other-local",
            "window-remote-origin",
            "window-recreated-main",
            "window-reloaded-generation",
            "window-destruction-race",
            "window-revocation-race",
            "panic-metadata",
            "panic-body",
            "panic-response",
            "panic-observation",
            "panic-fixed-error",
            "sequential-repeat",
            "concurrent-batch",
            "reordered-invoke",
            "post-revocation",
        ];
        for command in ALL_COMMANDS {
            for family in common {
                let selected = format!("wrapper-{command}-{family}--official-invoke");
                assert!(valid_case_selector(&selected), "missing {selected}");
            }
        }
        for command in [CREATE, RESTORE] {
            for family in [
                "malformed-nested",
                "oversized-nested",
                "unknown-nested",
                "secret-like-nested",
            ] {
                assert!(valid_case_selector(&format!(
                    "wrapper-{command}-{family}--official-invoke"
                )));
            }
        }
    }

    #[test]
    fn declared_invoked_mismatch_routes_each_real_generated_wrapper() {
        for (index, declared) in ALL_COMMANDS.iter().enumerate() {
            let invoked = ALL_COMMANDS[(index + 1) % ALL_COMMANDS.len()];
            let selected = format!("wrapper-{declared}-declared-invoked-mismatch--official-invoke");
            assert_eq!(
                mismatch_declared_command(&selected, invoked),
                Some(*declared)
            );
            assert_eq!(mismatch_declared_command(&selected, declared), None);
            assert_eq!(expected_wrapper_entries(&selected, true), 2);
        }
    }

    #[test]
    fn native_destruction_requires_the_exact_selected_transport_and_envelope() {
        let selected = "wrapper-wallet_get_status-window-destruction-race--official-invoke";
        let body = json(serde_json::json!({}));
        assert!(valid_native_destruction_request(
            selected,
            GET_STATUS,
            GET_STATUS,
            FrameworkRoute::CustomProtocol,
            &body,
        ));
        assert!(!valid_native_destruction_request(
            selected,
            GET_STATUS,
            GET_STATUS,
            FrameworkRoute::PostMessage,
            &body,
        ));
        assert!(!valid_native_destruction_request(
            selected,
            GET_STATUS,
            LOCK,
            FrameworkRoute::CustomProtocol,
            &body,
        ));
        assert!(!valid_native_destruction_request(
            "wrapper-wallet_get_status-window-destruction-race--internals-post-message",
            GET_STATUS,
            GET_STATUS,
            FrameworkRoute::PostMessage,
            &body,
        ));
    }

    #[test]
    fn duplicate_case_names_cover_create_and_restore_schemas() {
        for command in ["create", "restore"] {
            for prefix in ["duplicate", "nested-duplicate"] {
                assert!(valid_case_selector(&format!(
                    "{prefix}-{command}-conflicting-string--official-invoke"
                )));
                assert!(valid_case_selector(&format!(
                    "{prefix}-{command}-escaped-equivalent-bytes--internals-ipc"
                )));
            }
        }
    }

    #[test]
    fn loaded_webview2_versions_are_strict_and_bounded() {
        fn wide(value: &str) -> Vec<u16> {
            value.encode_utf16().chain(std::iter::once(0)).collect()
        }
        let valid = wide("151.0.4129.72");
        assert_eq!(
            parse_webview2_runtime_version(valid.as_ptr()).as_deref(),
            Some("151.0.4129.72")
        );
        for invalid in [
            "151.0.4129",
            "151.0.4129.72.1",
            "151.0.beta.72",
            "151..4129.72",
            "123456.0.0.0",
        ] {
            let invalid = wide(invalid);
            assert!(parse_webview2_runtime_version(invalid.as_ptr()).is_none());
        }
        assert!(parse_webview2_runtime_version(std::ptr::null()).is_none());
    }

    #[test]
    fn forced_fallback_requires_both_interception_and_native_post_message_proof() {
        let selected = "forced-post-message-fallback--internals-post-message";
        let mut request = BrowserObservationRequest {
            marker: "layer_b_browser_observation".into(),
            case: selected.into(),
            client_api: "internals-post-message".into(),
            command: GET_STATUS.into(),
            outcome: "layer_b_accepted".into(),
            expected: "layer_b_accepted".into(),
            result: "passed".into(),
            transport_evidence: "post_message_proven".into(),
            fallback_intercepted: false,
        };
        assert!(validated_browser_observation(selected, &request).is_none());
        request.fallback_intercepted = true;
        assert!(validated_browser_observation(selected, &request).is_some());

        request.client_api = "matrix-controller".into();
        request.command = "matrix".into();
        request.outcome = "matrix_complete".into();
        request.expected = "matrix_complete".into();
        request.transport_evidence = "not_applicable".into();
        request.fallback_intercepted = false;
        assert!(validated_browser_observation(selected, &request).is_some());
    }

    #[test]
    fn case_selection_is_bounded_and_requires_an_explicit_transport() {
        assert!(valid_case_selector(
            "exact-wallet_get_status--official-invoke"
        ));
        assert!(!valid_case_selector("exact-wallet_get_status"));
        assert!(!valid_case_selector("secret=canary--official-invoke"));
        assert!(!valid_case_selector("password-canary--official-invoke"));
    }

    #[test]
    fn qualification_origin_is_exact_and_path_tolerant() {
        assert!(is_qualification_origin(
            &"http://tauri.localhost/".parse().unwrap()
        ));
        assert!(is_qualification_origin(
            &"http://tauri.localhost/index.html".parse().unwrap()
        ));
        for rejected in [
            "https://tauri.localhost/",
            "http://tauri.localhost.evil/",
            "http://tauri.localhost:81/",
            "http://user@tauri.localhost/",
            "http://tauri.localhost/?remote=true",
        ] {
            assert!(!is_qualification_origin(&rejected.parse().unwrap()));
        }
    }

    #[test]
    fn bootstrap_page_is_non_executable_and_cannot_consume_window_authority() {
        let bootstrap = include_str!("assets/bootstrap.html");
        assert!(!bootstrap.contains("<script"));
        assert!(!bootstrap.contains("harness.js"));
        assert!(!bootstrap.contains("__TAURI"));

        let config: Value = serde_json::from_str(include_str!("tauri.conf.json")).unwrap();
        assert_eq!(
            config["app"]["windows"][0]["url"].as_str(),
            Some("bootstrap.html")
        );
        let permitted_windows = config["app"]["security"]["capabilities"][0]["windows"]
            .as_array()
            .unwrap();
        assert!(!permitted_windows
            .iter()
            .any(|window| { window.as_str() == Some(QUALIFICATION_CONTROLLER_WINDOW) }));

        let state =
            QualificationState::new("wrapper-wallet_get_status-exact--official-invoke".into());
        state.note_page_load(&"http://tauri.localhost/bootstrap.html".parse().unwrap());
        assert_eq!(state.page_generation.load(Ordering::Acquire), 0);
        assert_eq!(state.authorized_generation.load(Ordering::Acquire), 0);
        state.note_page_load(&"http://tauri.localhost/index.html".parse().unwrap());
        assert_eq!(state.page_generation.load(Ordering::Acquire), 1);
        assert_eq!(state.authorized_generation.load(Ordering::Acquire), 1);
    }

    #[test]
    fn special_window_cases_have_deterministic_native_startup_plans() {
        for command in ALL_COMMANDS {
            assert_eq!(
                startup_plan(&format!(
                    "wrapper-{command}-window-other-local--official-invoke"
                )),
                StartupPlan::OtherLocal
            );
            assert_eq!(
                startup_plan(&format!(
                    "wrapper-{command}-window-remote-origin--official-invoke"
                )),
                StartupPlan::RemoteOrigin
            );
            assert_eq!(
                startup_plan(&format!(
                    "wrapper-{command}-window-recreated-main--official-invoke"
                )),
                StartupPlan::RecreatedMain
            );
            assert_eq!(
                startup_plan(&format!("wrapper-{command}-exact--official-invoke")),
                StartupPlan::Main
            );
        }
    }
}
