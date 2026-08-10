#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

#[cfg(not(target_os = "windows"))]
compile_error!("the Wallet Layer B transport harness is Windows-only");

use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    ipc::{CommandArg, CommandItem, InvokeBody, InvokeError, Response},
    Runtime, State, WebviewWindow,
};

const QUALIFICATION_ARGUMENT: &str = "--wallet-layer-b-qualification";
const QUALIFICATION_WINDOW: &str = "wallet-transport-qualification";
const QUALIFICATION_HOST: &str = "tauri.localhost";

const GET_STATUS: &str = "wallet_get_status";
const SELECT_RECOVERY_DESTINATION: &str = "wallet_select_recovery_destination";
const CREATE: &str = "wallet_create";
const SELECT_RECOVERY_SOURCE: &str = "wallet_select_recovery_source";
const RESTORE: &str = "wallet_restore";
const UNLOCK: &str = "wallet_unlock";
const LOCK: &str = "wallet_lock";

const NO_INPUT_COMMANDS: &[&str] = &[
    GET_STATUS,
    SELECT_RECOVERY_DESTINATION,
    SELECT_RECOVERY_SOURCE,
    UNLOCK,
    LOCK,
];

const ALLOWED_ROUTES: &[&str] = &[
    "official-invoke",
    "internals-invoke",
    "internals-ipc",
    "internals-post-message",
];

struct QualificationInvokeRequest<'a> {
    declared_command: &'static str,
    invoked_command: &'a str,
    body: &'a InvokeBody,
    route: &'static str,
    panic_requested: bool,
}

impl<'a, R: Runtime> CommandArg<'a, R> for QualificationInvokeRequest<'a> {
    fn from_command(command: CommandItem<'a, R>) -> Result<Self, InvokeError> {
        let route = command
            .message
            .headers()
            .get("x-vision-qualification-route")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| ALLOWED_ROUTES.iter().copied().find(|route| route == &value))
            .unwrap_or("unclassified");
        let panic_requested = command
            .message
            .headers()
            .get("x-vision-qualification-panic")
            .is_some_and(|value| value == "body");
        Ok(Self {
            declared_command: command.name,
            invoked_command: command.message.command(),
            body: command.message.payload(),
            route,
            panic_requested,
        })
    }
}

#[derive(Default)]
struct QualificationState {
    wrapper_entries: AtomicUsize,
    accepted: AtomicUsize,
    rejected: AtomicUsize,
    invalidations: AtomicUsize,
    revoked: AtomicBool,
}

impl QualificationState {
    fn invalidate(&self) {
        if !self.revoked.swap(true, Ordering::AcqRel) {
            self.invalidations.fetch_add(1, Ordering::AcqRel);
        }
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
    top_level_key_count: usize,
    top_level_keys: Vec<String>,
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
    top_level_key_count: usize,
    top_level_keys: &'a [String],
    result: &'static str,
    wrapper_ran: bool,
    command_body_ran: bool,
}

struct BodyMetadata {
    kind: &'static str,
    keys: Vec<String>,
}

fn body_metadata(body: &InvokeBody) -> BodyMetadata {
    match body {
        InvokeBody::Raw(_) => BodyMetadata {
            kind: "raw",
            keys: Vec::new(),
        },
        InvokeBody::Json(Value::Object(object)) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            BodyMetadata { kind: "json", keys }
        }
        InvokeBody::Json(_) => BodyMetadata {
            kind: "json",
            keys: Vec::new(),
        },
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

fn emit_observation(observation: &QualificationObservation<'_>) {
    if let Ok(line) = serde_json::to_string(observation) {
        println!("{line}");
    } else {
        println!("{{\"marker\":\"layer_b_observation_unavailable\"}}");
    }
}

fn qualify(
    expected_command: &'static str,
    request: QualificationInvokeRequest<'_>,
    window: WebviewWindow,
    state: State<'_, QualificationState>,
) -> Result<Response, InvokeError> {
    state.wrapper_entries.fetch_add(1, Ordering::AcqRel);
    let metadata = body_metadata(request.body);
    let mut guard = FailClosedGuard::arm(&state);
    let attempt = catch_unwind(AssertUnwindSafe(|| {
        if state.revoked.load(Ordering::Acquire) {
            return Err("qualification_runtime_unavailable");
        }
        if request.panic_requested {
            panic!("injected Layer B qualification panic");
        }
        let url = window.url().map_err(|_| "qualification_invalid_window")?;
        if window.label() != QUALIFICATION_WINDOW || !is_qualification_origin(&url) {
            return Err("qualification_invalid_window");
        }
        if request.declared_command != expected_command
            || request.invoked_command != expected_command
            || request.route == "unclassified"
            || !valid_envelope(expected_command, request.body)
        {
            return Err("invalid_request");
        }
        let response = QualificationResponse {
            marker: "layer_b_accepted",
            command: expected_command,
            route: request.route,
            body_kind: metadata.kind,
            top_level_key_count: metadata.keys.len(),
            top_level_keys: metadata.keys.clone(),
            wrapper_ran: true,
            command_body_ran: true,
        };
        serialize_response(&response).map_err(|_| "qualification_response_unavailable")
    }));

    match attempt {
        Ok(Ok(response)) => {
            state.accepted.fetch_add(1, Ordering::AcqRel);
            emit_observation(&QualificationObservation {
                marker: "layer_b_observation",
                timestamp_ms: timestamp_ms(),
                command: expected_command,
                route: request.route,
                body_kind: metadata.kind,
                top_level_key_count: metadata.keys.len(),
                top_level_keys: &metadata.keys,
                result: "accepted",
                wrapper_ran: true,
                command_body_ran: true,
            });
            guard.commit();
            Ok(response)
        }
        Ok(Err(code)) => {
            state.rejected.fetch_add(1, Ordering::AcqRel);
            guard.commit();
            emit_observation(&QualificationObservation {
                marker: "layer_b_observation",
                timestamp_ms: timestamp_ms(),
                command: expected_command,
                route: request.route,
                body_kind: metadata.kind,
                top_level_key_count: metadata.keys.len(),
                top_level_keys: &metadata.keys,
                result: code,
                wrapper_ran: true,
                command_body_ran: false,
            });
            Err(fixed_error(code))
        }
        Err(_) => Err(fixed_error("qualification_runtime_unavailable")),
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

fn qualification_mode_requested() -> bool {
    std::env::args().any(|argument| argument == QUALIFICATION_ARGUMENT)
}

fn main() {
    if !qualification_mode_requested() {
        eprintln!("Layer B qualification mode was not explicitly requested");
        std::process::exit(64);
    }
    std::panic::set_hook(Box::new(|_| {
        eprintln!("Layer B qualification panic was contained");
    }));
    tauri::Builder::default()
        .manage(QualificationState::default())
        .invoke_handler(tauri::generate_handler![
            wallet_get_status,
            wallet_select_recovery_destination,
            wallet_create,
            wallet_select_recovery_source,
            wallet_restore,
            wallet_unlock,
            wallet_lock,
        ])
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
    fn metadata_records_names_but_never_values() {
        let metadata = body_metadata(&json(serde_json::json!({
            "request": { "password": "PUBLIC_CANARY_MUST_NOT_ESCAPE" },
            "extra": "PUBLIC_CANARY_MUST_NOT_ESCAPE"
        })));
        assert_eq!(metadata.kind, "json");
        assert_eq!(metadata.keys, ["extra", "request"]);
        assert!(!format!("{:?}", metadata.keys).contains("PUBLIC_CANARY"));
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
}
