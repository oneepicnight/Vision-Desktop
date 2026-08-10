# Wallet Tauri Layer B Implementation Handoff

Date: 2026-08-09
Workstation: Vision Desktop ASUS Windows workstation
Branch: `fix/wallet-signing-adversarial-matrix`
Approved Layer A parent: `9a4af4d1f30374a2743082e350fbcc2fb26b6f50`
Approved Layer A tree: `107aaf7a4c896874bdef8f855c36778b7e06b379`
Approved Layer A handoff SHA-256: `EC017A3BD1ACF63D42F37AA584249DBC148752FCB416C4071071E665DC0613E9`

## Result

The standalone Layer B Windows Tauri/Wry/WebView2 transport harness is implemented and ready for
independent source review.

The complete packaged qualification matrix has not been run. This implementation does not claim a
Passed result. Production `duplicate_key_rejection_proven` remains `false`, and all wallet command,
permission, frontend, activation, signing, and submission surfaces remain closed.

## Isolation model

The harness is an opt-in Cargo binary:

`vision-wallet-transport-qualification`

It compiles only with:

`wallet-layer-b-qualification`

It also refuses startup unless the operator supplies:

`--wallet-layer-b-qualification`

The harness has its own:

- application identifier: `com.vision.desktop.wallet-transport-qualification`;
- executable name: `vision-wallet-transport-qualification`;
- window label: `wallet-transport-qualification`;
- inline test-only capability: `wallet-layer-b-qualification`;
- embedded static assets;
- CSP; and
- Tauri configuration with bundling disabled.

The product `tauri.conf.json`, product capabilities, production invoke table, React application,
and package scripts do not select this binary or configuration.

## No-custody boundary

The binary does not import or link the Vision Desktop library and contains no:

- `WalletRuntimeState`;
- lifecycle adapters;
- wallet vault or recovery implementation;
- recovery or journal filesystem path;
- Core client;
- secret input or seed type;
- signing or signed artifact;
- transaction submission path; or
- product AppManifest authority.

Its state contains only atomic counters and a revoked marker. Payload values are never written to
stdout or returned as observations. The static page uses deterministic public canaries only.

## Generated wrappers

The binary declares exactly seven harness-only generated wrappers:

- `wallet_get_status`;
- `wallet_select_recovery_destination`;
- `wallet_create`;
- `wallet_select_recovery_source`;
- `wallet_restore`;
- `wallet_unlock`; and
- `wallet_lock`.

Each accepts a harness-local whole-message `CommandArg`, the actual invoking Tauri window, and the
non-custody qualification state. The argument borrows the invoked command and complete
`InvokeBody`. Parsing, fixed response creation, and panic containment occur inside the guarded
classifier.

The classifier enforces exact empty objects for the five no-input shapes and the reviewed bounded
public request structure for create and restore. It never invokes the private wallet lifecycle
boundary.

## Real WebView routes implemented

The embedded page exercises:

1. the official global Tauri `core.invoke` API;
2. `window.__TAURI_INTERNALS__.invoke`;
3. `window.__TAURI_INTERNALS__.ipc`;
4. `window.__TAURI_INTERNALS__.postMessage`;
5. a forced custom-protocol failure followed by Tauri's WebView2 `window.ipc.postMessage` fallback;
6. direct custom-protocol `fetch` without the private invoke key; and
7. direct custom-protocol XHR without the private invoke key.

The official and internal routes use the exact pinned Tauri initialization code embedded by the
real Windows runtime. Direct fetch/XHR probes deliberately lack the closure-held invoke key and must
be rejected before the classifier runs.

## Payload matrix implemented

The browser matrix includes:

- exact accepted envelopes for all seven wrappers;
- raw empty, JSON-looking, arbitrary string, and UTF-8 byte bodies;
- JSON null, boolean, number, string, array, and object bodies;
- missing, extra, wrong-case, and secret-like top-level fields;
- missing, malformed, unknown, oversized, wrong-case, and secret-like nested public fields;
- invalid recovery-selection handles;
- an unknown command;
- concurrent invokes;
- injected guarded panic and post-revocation invocation; and
- duplicate textual-key families at the top-level envelope and inside the nested public request.

The duplicate families include:

- identical values;
- conflicting values;
- valid then malformed;
- malformed then valid;
- public then secret-like;
- exact-case plus wrong-case;
- three repeated keys;
- escaped equivalent spellings; and
- large bounded separating whitespace.

Each family is attempted as a JavaScript string, UTF-8 bytes, and an explicitly normalized
JavaScript object where supported. The first two must remain raw and be rejected. The normalized
object outcome is recorded rather than treated as proof of textual duplicate rejection.

The complete window/origin/generation operator matrix remains part of the packaged evidence run and
must be confirmed during independent harness review before execution.

## Instrumentation boundary

Rust stdout observations contain only:

- a fixed marker;
- a millisecond timestamp;
- command name;
- allowlisted transport route;
- JSON-versus-raw classification;
- sorted top-level key names and key count;
- a fixed acceptance or rejection code; and
- whether the generated wrapper and guarded body ran.

The browser transcript contains only case identifiers, routes, command names, fixed outcomes,
expectations, and pass/fail booleans. Arbitrary framework errors are reduced to
`framework_error_redacted`; request values are never formatted.

## Build and execution boundary

Source-review compile command:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --features wallet-layer-b-qualification --bin vision-wallet-transport-qualification
```

The sanctioned harness must later be built from the exact independently approved commit and run
with the explicit argument. Running it before that review is outside this tranche.

Normal `cargo build`, the production Tauri build, and installer packaging do not enable the marker
feature. The harness config also sets `bundle.active` to `false`.

`build.rs` reads the seven explicit harness permissions from the isolated qualification directory
only while the Layer B feature is enabled. The normal AppManifest remains the original nineteen
production node-manager commands, and a Layer B compile does not generate wallet permissions in the
product permission tree. The existing Common Controls test manifest remains Layer A-only; Layer B
uses solely the resource generated from its own Tauri configuration.

## Static release closure

The Tauri authority test verifies:

- the feature-gated binary declaration;
- the exact seven generated wrapper invocations;
- explicit startup-mode enforcement;
- real whole-body and generated-handler use;
- absence of wallet, Core, signing, and submission authority types;
- distinct harness identifier, window, capability, and disabled bundle;
- absence of Layer B selectors from the product Tauri configuration;
- inclusion of official, internal, fallback, direct-fetch/XHR, and duplicate test routes; and
- continued production duplicate-key blocking.

Optimized production build and marker-scan evidence must be regenerated after this implementation
and preserved with the later qualification evidence.

## Validation

Passed without launching the harness or running the physical qualification matrix:

- Layer B pure classifier tests: 5 passed, 0 failed;
- complete Rust suite: 293 passed, 0 failed, 4 operator-only ignored;
- Tauri authority tests: 7 passed, 0 failed;
- WebView isolation tests: 2 passed, 0 failed;
- strict Clippy for the opt-in Layer B binary;
- ordinary strict Clippy for all production targets;
- Rust formatting check;
- Layer B JavaScript syntax check;
- frontend TypeScript typecheck;
- frontend state tests;
- optimized frontend build;
- locked Layer B binary build;
- locked optimized production Rust build;
- production executable Layer B marker scan: no matches;
- Git whitespace check; and
- no `Cargo.lock` change.

The harness executable was built for source and linkage verification only. It was not started.

## Files in this tranche

- `docs/WALLET_TAURI_LAYER_B_IMPLEMENTATION_HANDOFF.md`
- `src-tauri/Cargo.toml`
- `src-tauri/build.rs`
- `src-tauri/qualification/wallet-layer-b/main.rs`
- `src-tauri/qualification/wallet-layer-b/tauri.conf.json`
- `src-tauri/qualification/wallet-layer-b/assets/index.html`
- `src-tauri/qualification/wallet-layer-b/assets/harness.css`
- `src-tauri/qualification/wallet-layer-b/assets/harness.js`
- `src-tauri/qualification/wallet-layer-b/permissions/wallet-layer-b.toml`
- `src-tauri/tests/tauri_acl.rs`

`Cargo.lock` and application dependencies are unchanged.

## Prohibited and unchanged

This implementation does not authorize or add:

- a transport Passed verdict;
- product wallet command registration;
- product AppManifest, permission, or capability changes;
- React invokes, forms, or custody state;
- a production constructor that can set duplicate-key proof to true;
- approval-flag changes;
- signing or submission exposure;
- recovery export;
- Core-manifest relaxation; or
- Vision-Core changes.

## Required next action

Validate the source-only harness and static release closure, commit the implementation separately,
and submit the exact commit and tree for independent Layer B implementation review. Do not run or
claim the complete packaged qualification matrix until that exact implementation is approved.
