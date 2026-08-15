use super::*;
use serde::Serialize;
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
};
use tauri::{
    ipc::CallbackFn,
    test::{mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    App, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

const NO_PANIC: u8 = 0;
const PANIC_BEFORE_ENVELOPE: u8 = 1;
const PANIC_AFTER_ENVELOPE: u8 = 2;
const PANIC_BEFORE_RESPONSE: u8 = 3;
const PANIC_DURING_FIXED_ERROR: u8 = 4;

struct LayerAQualificationState {
    command_entries: AtomicUsize,
    accepted_responses: AtomicUsize,
    invalidations: AtomicUsize,
    revoked: AtomicBool,
    panic_checkpoint: AtomicU8,
}

impl Default for LayerAQualificationState {
    fn default() -> Self {
        Self {
            command_entries: AtomicUsize::new(0),
            accepted_responses: AtomicUsize::new(0),
            invalidations: AtomicUsize::new(0),
            revoked: AtomicBool::new(false),
            panic_checkpoint: AtomicU8::new(NO_PANIC),
        }
    }
}

impl LayerAQualificationState {
    fn invalidate(&self) {
        if !self.revoked.swap(true, Ordering::AcqRel) {
            self.invalidations.fetch_add(1, Ordering::AcqRel);
        }
    }

    fn panic_at(&self, checkpoint: u8) {
        if self.panic_checkpoint.load(Ordering::Acquire) == checkpoint {
            panic!("injected Layer A qualification panic");
        }
    }
}

struct LayerAFailClosedGuard<'a> {
    state: &'a LayerAQualificationState,
    armed: bool,
}

impl<'a> LayerAFailClosedGuard<'a> {
    fn arm(state: &'a LayerAQualificationState) -> Self {
        Self { state, armed: true }
    }

    fn commit(&mut self) {
        self.armed = false;
    }
}

impl Drop for LayerAFailClosedGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.state.invalidate();
        }
    }
}

#[derive(Serialize)]
struct LayerAQualifiedResponse {
    qualified_command: &'static str,
}

fn envelope_command(envelope: &WalletLifecycleEnvelope) -> &'static str {
    match envelope {
        WalletLifecycleEnvelope::GetStatus => GET_STATUS,
        WalletLifecycleEnvelope::SelectRecoveryDestination => SELECT_RECOVERY_DESTINATION,
        WalletLifecycleEnvelope::Create(_) => CREATE,
        WalletLifecycleEnvelope::SelectRecoverySource => SELECT_RECOVERY_SOURCE,
        WalletLifecycleEnvelope::Restore(_) => RESTORE,
        WalletLifecycleEnvelope::Unlock => UNLOCK,
        WalletLifecycleEnvelope::Lock => LOCK,
    }
}

fn qualify_generated_wrapper(
    expected_command: &'static str,
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    state.command_entries.fetch_add(1, Ordering::AcqRel);
    if state.revoked.load(Ordering::Acquire) {
        return Err(fixed_invoke_error("wallet_runtime_unavailable"));
    }

    let mut guard = LayerAFailClosedGuard::arm(&state);
    let attempt = catch_unwind(AssertUnwindSafe(|| {
        state.panic_at(PANIC_BEFORE_ENVELOPE);
        let result = (|| {
            if window.label() != MAIN_WINDOW_LABEL {
                return Err(BoundaryError::InvalidWindow);
            }
            let url = window.url().map_err(|_| BoundaryError::InvalidWindow)?;
            if !is_bundled_windows_url(&url) {
                return Err(BoundaryError::InvalidWindow);
            }

            let envelope = parse_envelope(request)?;
            if envelope_command(&envelope) != expected_command {
                return Err(BoundaryError::InvalidRequest);
            }
            state.panic_at(PANIC_AFTER_ENVELOPE);
            state.panic_at(PANIC_BEFORE_RESPONSE);
            let response = serialize_response(&LayerAQualifiedResponse {
                qualified_command: expected_command,
            })?;
            state.accepted_responses.fetch_add(1, Ordering::AcqRel);
            Ok(response)
        })();
        result.map_err(|error: BoundaryError| {
            state.panic_at(PANIC_DURING_FIXED_ERROR);
            fixed_invoke_error(error.code())
        })
    }));

    match attempt {
        Ok(Ok(response)) => {
            guard.commit();
            Ok(response)
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err(fixed_invoke_error("wallet_runtime_unavailable")),
    }
}

#[tauri::command]
fn wallet_get_status(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    qualify_generated_wrapper(GET_STATUS, request, window, state)
}

#[tauri::command]
fn wallet_select_recovery_destination(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    qualify_generated_wrapper(SELECT_RECOVERY_DESTINATION, request, window, state)
}

#[tauri::command]
fn wallet_create(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    qualify_generated_wrapper(CREATE, request, window, state)
}

#[tauri::command]
fn wallet_select_recovery_source(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    qualify_generated_wrapper(SELECT_RECOVERY_SOURCE, request, window, state)
}

#[tauri::command]
fn wallet_restore(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    qualify_generated_wrapper(RESTORE, request, window, state)
}

#[tauri::command]
fn wallet_unlock(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    qualify_generated_wrapper(UNLOCK, request, window, state)
}

#[tauri::command]
fn wallet_lock(
    request: WalletInvokeRequest<'_>,
    window: WebviewWindow<MockRuntime>,
    state: State<'_, LayerAQualificationState>,
) -> Result<Response, InvokeError> {
    qualify_generated_wrapper(LOCK, request, window, state)
}

struct LayerAHarness {
    webview: WebviewWindow<MockRuntime>,
    app: App<MockRuntime>,
}

struct LayerAObservation {
    result: Result<Value, Value>,
    command_entries: usize,
    accepted_responses: usize,
    invalidations: usize,
    revoked: bool,
}

impl LayerAHarness {
    fn trusted() -> Self {
        Self::new(MAIN_WINDOW_LABEL, BUNDLED_WINDOWS_ORIGIN)
    }

    fn new(label: &str, url: &str) -> Self {
        let app = mock_builder()
            .manage(LayerAQualificationState::default())
            .invoke_handler(tauri::generate_handler![
                wallet_get_status,
                wallet_select_recovery_destination,
                wallet_create,
                wallet_select_recovery_source,
                wallet_restore,
                wallet_unlock,
                wallet_lock,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let external = Url::parse(url).unwrap();
        let webview = WebviewWindowBuilder::new(&app, label, WebviewUrl::External(external))
            .build()
            .unwrap();
        Self { webview, app }
    }

    fn set_panic_checkpoint(&self, checkpoint: u8) {
        self.app
            .state::<LayerAQualificationState>()
            .panic_checkpoint
            .store(checkpoint, Ordering::Release);
    }

    fn invoke(&self, command: &str, body: InvokeBody) -> LayerAObservation {
        let result = tauri::test::get_ipc_response(
            &self.webview,
            InvokeRequest {
                cmd: command.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: self.webview.url().unwrap(),
                body,
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap());
        let state = self.app.state::<LayerAQualificationState>();
        LayerAObservation {
            result,
            command_entries: state.command_entries.load(Ordering::Acquire),
            accepted_responses: state.accepted_responses.load(Ordering::Acquire),
            invalidations: state.invalidations.load(Ordering::Acquire),
            revoked: state.revoked.load(Ordering::Acquire),
        }
    }
}

fn json_body(value: Value) -> InvokeBody {
    InvokeBody::Json(value)
}

fn valid_body(command: &str) -> InvokeBody {
    match command {
        CREATE => json_body(serde_json::json!({
            "request": {
                "wallet_id": "wallet-layer-a",
                "label": "Layer A",
            }
        })),
        RESTORE => json_body(serde_json::json!({
            "request": {
                "wallet_id": "wallet-layer-a",
                "label": "Layer A",
            }
        })),
        _ => json_body(serde_json::json!({})),
    }
}

fn invalid_bodies(command: &str) -> Vec<InvokeBody> {
    let mut bodies = vec![
        InvokeBody::Raw(Vec::new()),
        InvokeBody::Raw(br#"{"request":{}}"#.to_vec()),
        InvokeBody::Raw(vec![0, 159, 146, 150, 255]),
        json_body(Value::Null),
        json_body(Value::Bool(true)),
        json_body(serde_json::json!(1)),
        json_body(serde_json::json!("object-required")),
        json_body(serde_json::json!([])),
        json_body(serde_json::json!({ "extra": true })),
        json_body(serde_json::json!({ "password": "layer-a-secret-canary" })),
        json_body(serde_json::json!({ "Request": {} })),
    ];
    match command {
        CREATE => {
            bodies.push(json_body(serde_json::json!({})));
            bodies.extend([
                json_body(serde_json::json!({ "request": null })),
                json_body(serde_json::json!({ "request": [] })),
                json_body(serde_json::json!({ "request": "invalid" })),
                json_body(serde_json::json!({
                    "request": {
                        "wallet_id": 7,
                        "label": "Layer A",
                    }
                })),
                json_body(serde_json::json!({
                    "request": {
                        "wallet_id": "wallet-layer-a",
                        "label": "x".repeat(4096),
                    }
                })),
                json_body(serde_json::json!({
                    "request": {
                        "wallet_id": "wallet-layer-a",
                        "label": "Layer A",
                        "recovery_destination_handle": "ab".repeat(32),
                    }
                })),
            ]);
            bodies.push(json_body(serde_json::json!({
                "request": {
                    "wallet_id": "wallet-layer-a",
                    "label": "Layer A",
                    "seed": "layer-a-secret-canary",
                }
            })));
        }
        RESTORE => {
            bodies.push(json_body(serde_json::json!({})));
            bodies.extend([
                json_body(serde_json::json!({ "request": null })),
                json_body(serde_json::json!({ "request": {} })),
                json_body(serde_json::json!({ "request": true })),
                json_body(serde_json::json!({
                    "request": {
                        "wallet_id": "wallet-layer-a",
                        "Label": "Layer A",
                    }
                })),
                json_body(serde_json::json!({
                    "request": {
                        "wallet_id": "x".repeat(4096),
                        "label": "Layer A",
                    }
                })),
                json_body(serde_json::json!({
                    "request": {
                        "wallet_id": "wallet-layer-a",
                        "label": "Layer A",
                        "recovery_source_handle": "cd".repeat(32),
                    }
                })),
            ]);
            bodies.push(json_body(serde_json::json!({
                "request": {
                    "wallet_id": "wallet-layer-a",
                    "label": "Layer A",
                    "password": "layer-a-secret-canary",
                }
            })));
        }
        _ => bodies.push(json_body(serde_json::json!({ "request": {} }))),
    }
    bodies
}

const COMMANDS: [&str; 7] = [
    GET_STATUS,
    SELECT_RECOVERY_DESTINATION,
    CREATE,
    SELECT_RECOVERY_SOURCE,
    RESTORE,
    UNLOCK,
    LOCK,
];

#[test]
fn actual_generated_wrappers_accept_all_seven_exact_envelopes() {
    let harness = LayerAHarness::trusted();
    for (index, command) in COMMANDS.into_iter().enumerate() {
        let observation = harness.invoke(command, valid_body(command));
        assert_eq!(
            observation.result,
            Ok(serde_json::json!({ "qualified_command": command }))
        );
        assert_eq!(observation.command_entries, index + 1);
        assert_eq!(observation.accepted_responses, index + 1);
        assert_eq!(observation.invalidations, 0);
        assert!(!observation.revoked);
    }
}

#[test]
fn actual_generated_wrappers_reject_complete_body_matrix_with_fixed_error() {
    for command in COMMANDS {
        for body in invalid_bodies(command) {
            let observation = LayerAHarness::trusted().invoke(command, body);
            assert_eq!(
                observation.result,
                Err(serde_json::json!({ "code": "invalid_request" }))
            );
            assert_eq!(observation.command_entries, 1);
            assert_eq!(observation.accepted_responses, 0);
            assert_eq!(observation.invalidations, 1);
            assert!(observation.revoked);
        }
    }
}

#[test]
fn generated_dispatch_rejects_wrong_command_schema_and_unknown_command() {
    for command in COMMANDS {
        let wrong_body = if matches!(command, CREATE | RESTORE) {
            valid_body(GET_STATUS)
        } else {
            valid_body(CREATE)
        };
        let wrong_schema = LayerAHarness::trusted().invoke(command, wrong_body);
        assert_eq!(
            wrong_schema.result,
            Err(serde_json::json!({ "code": "invalid_request" }))
        );
        assert_eq!(wrong_schema.command_entries, 1);
        assert_eq!(wrong_schema.invalidations, 1);
    }

    let unknown =
        LayerAHarness::trusted().invoke("wallet_unknown", json_body(serde_json::json!({})));
    assert!(unknown.result.is_err());
    assert_eq!(unknown.command_entries, 0);
    assert_eq!(unknown.accepted_responses, 0);
    assert_eq!(unknown.invalidations, 0);
    assert!(!unknown.revoked);
}

#[test]
fn generated_wrapper_binds_framework_window() {
    let observation = LayerAHarness::new("secondary", BUNDLED_WINDOWS_ORIGIN)
        .invoke(GET_STATUS, valid_body(GET_STATUS));
    assert_eq!(
        observation.result,
        Err(serde_json::json!({ "code": "invalid_window" }))
    );
    assert_eq!(observation.command_entries, 1);
    assert_eq!(observation.accepted_responses, 0);
    assert_eq!(observation.invalidations, 1);
    assert!(observation.revoked);
}

#[test]
fn generated_resolver_rejects_remote_origin_before_command_entry() {
    let observation = LayerAHarness::new(MAIN_WINDOW_LABEL, "https://example.invalid")
        .invoke(GET_STATUS, valid_body(GET_STATUS));
    assert_eq!(
        observation.result,
        Err(Value::String(
            "wallet_get_status not allowed. Plugin not found".into()
        ))
    );
    assert_eq!(observation.command_entries, 0);
    assert_eq!(observation.accepted_responses, 0);
    assert_eq!(observation.invalidations, 0);
    assert!(!observation.revoked);
}

#[test]
fn parser_and_response_panics_are_fixed_and_fail_closed() {
    for checkpoint in [
        PANIC_BEFORE_ENVELOPE,
        PANIC_AFTER_ENVELOPE,
        PANIC_BEFORE_RESPONSE,
        PANIC_DURING_FIXED_ERROR,
    ] {
        let harness = LayerAHarness::trusted();
        harness.set_panic_checkpoint(checkpoint);
        let body = if checkpoint == PANIC_DURING_FIXED_ERROR {
            json_body(serde_json::json!({ "extra": true }))
        } else {
            valid_body(GET_STATUS)
        };
        let observation = harness.invoke(GET_STATUS, body);
        assert_eq!(
            observation.result,
            Err(serde_json::json!({ "code": "wallet_runtime_unavailable" }))
        );
        assert_eq!(observation.command_entries, 1);
        assert_eq!(observation.accepted_responses, 0);
        assert_eq!(observation.invalidations, 1);
        assert!(observation.revoked);
    }
}

#[test]
fn wrapper_replay_after_failure_cannot_return_stale_success() {
    let harness = LayerAHarness::trusted();
    let rejected = harness.invoke(
        GET_STATUS,
        json_body(serde_json::json!({ "seed": "layer-a-secret-canary" })),
    );
    assert_eq!(
        rejected.result,
        Err(serde_json::json!({ "code": "invalid_request" }))
    );
    let replay = harness.invoke(GET_STATUS, valid_body(GET_STATUS));
    assert_eq!(
        replay.result,
        Err(serde_json::json!({ "code": "wallet_runtime_unavailable" }))
    );
    assert_eq!(replay.command_entries, 2);
    assert_eq!(replay.accepted_responses, 0);
    assert_eq!(replay.invalidations, 1);
    assert!(replay.revoked);
}

#[test]
fn layer_a_observes_normalized_duplicates_while_production_uses_accepted_layer_b_proof() {
    let duplicate_top_level = serde_json::from_str::<Value>(
        r#"{"request":{"wallet_id":"first","label":"Layer A"},"request":{"wallet_id":"second","label":"Layer A"}}"#,
    )
    .unwrap();
    let observation = LayerAHarness::trusted().invoke(CREATE, json_body(duplicate_top_level));
    assert_eq!(
        observation.result,
        Ok(serde_json::json!({ "qualified_command": CREATE }))
    );

    let policy = WholeEnvelopeTransportPolicy::production();
    assert!(policy.duplicate_key_rejection_proven);
}
