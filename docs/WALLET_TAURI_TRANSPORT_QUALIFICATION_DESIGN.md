# Wallet Tauri Generated-Wrapper and Transport Qualification Design

## Status and scope

This document defines the next test-only qualification tranche after independent approval of the
private, unregistered wallet lifecycle command boundary at commit
`58e0843d57fec6a7b47004ed868d57df910d61d9`, tree
`0498bd1f298c6e8648848f282fd6faa22bc6d4c2`.

The tranche may build a sanctioned non-production harness and collect evidence. It does not
authorize a production `#[tauri::command]`, invoke registration, `AppManifest` entry, generated
permission, capability grant, frontend wrapper, wallet form, activation change, signing or
submission exposure, recovery export, dependency update, Core-manifest relaxation, or Vision-Core
change.

The production `WholeEnvelopeTransportPolicy` must continue to set
`duplicate_key_rejection_proven: false` throughout implementation and qualification. Passing this
matrix may support a later independent decision; it cannot change that blocker by itself.

## Normative relationship

For generated-wrapper and raw-transport qualification, this document is the normative extension of:

- `WALLET_LIFECYCLE_TAURI_EXPOSURE_DESIGN.md`;
- `WALLET_TAURI_COMMAND_THREAT_MODEL.md`; and
- `WALLET_LIFECYCLE_COMMAND_BOUNDARY_HANDOFF.md`.

The command names, request schemas, response schemas, fixed error vocabulary, native-secret policy,
main-window authority, lifecycle semantics, and activation requirements remain governed by
`WALLET_LIFECYCLE_TAURI_EXPOSURE_DESIGN.md`. Any inconsistency fails the qualification and requires a
new documentation review.

## Pinned framework facts

The current lockfile resolves:

- `tauri` `2.11.5`;
- `tauri-macros` `2.6.3`;
- `tauri-runtime` `2.11.3`; and
- `tauri-runtime-wry` `2.11.4`.

For the pinned source:

- an ordinary deserializable command argument is obtained by looking up its named key in an
  already-constructed `InvokeBody::Json(serde_json::Value)`;
- unrelated top-level keys are therefore not rejected by an ordinary typed argument;
- a command with no arguments performs no body deserialization;
- generated wrappers call `CommandArg::from_command` before entering the command function;
- the approved `WalletInvokeRequest` custom argument only borrows the complete command name and
  `InvokeBody`; it performs no parsing, allocation, wallet action, formatting, or secret access;
  and
- the private boundary performs complete-envelope parsing, fixed response conversion, and panic
  containment after that borrow succeeds.

These facts must be source-verified again from the exact Cargo registry sources and checksums used
by the qualification build. A Tauri, macro, runtime, Wry, WebView2, serde, or serde_json change
invalidates the evidence until the affected boundary is re-reviewed and requalified.

## Security question

The qualification must answer both questions independently:

1. Do the actual pinned Tauri generated wrappers pass the complete command name and complete
   `InvokeBody` to the approved custom argument without separately accepting, dropping, or
   transforming wallet fields?
2. Can any supported or attacker-reachable WebView IPC route submit textual JSON containing
   duplicate top-level or nested keys and have Tauri normalize it to an apparently valid
   `InvokeBody::Json` before the custom argument can reject the duplicate?

Passing only the first question is insufficient. Demonstrating that an ordinary JavaScript object
cannot contain duplicate keys is also insufficient. The threat model includes compromised bundled
React code directly exercising every reachable Tauri IPC primitive.

## Threat model

The harness must model:

- compromised code in the bundled main WebView;
- direct calls to documented and internal invoke primitives reachable from that WebView;
- crafted custom-protocol, `fetch`, XHR, binary, string, and object payloads where supported;
- raw bodies labeled as JSON and JSON bodies labeled as binary or another content type;
- duplicate top-level and nested keys with identical, conflicting, wrong-case, and secret-like
  values;
- extra, missing, malformed, oversized, and reordered fields;
- an unexpected command name or wrapper-name mismatch;
- parser, command, response-conversion, and resolver panics;
- replayed, concurrent, and late invokes; and
- non-main windows, remote origins, destroyed windows, and replaced window generations.

The harness uses public canary strings only. Passwords, credentials, seeds, private keys, vaults,
recovery files, wallet sessions, signing permits, and signed transactions must never be loaded.

## Qualification architecture

The implementation must have two separately reviewable layers. Neither layer may be linked into or
shipped with the production Vision Desktop executable.

### Layer A — actual generated-wrapper conformance

A `cfg(test)` Rust module may declare seven qualification-only `#[tauri::command]` functions and
construct an actual `tauri::generate_handler!` table. The generated functions must:

- use the exact seven future external command names;
- accept the existing custom `WalletInvokeRequest` whole-message argument;
- accept only the framework-injected window/state values required by the reviewed future wrapper
  shape;
- invoke a test-only, non-custody classifier or the existing boundary with explicitly test-only
  adapters;
- return only pre-serialized `tauri::ipc::Response` or fixed `InvokeError` values; and
- contain no production registration path.

The seven names are:

- `wallet_get_status`;
- `wallet_select_recovery_destination`;
- `wallet_create`;
- `wallet_select_recovery_source`;
- `wallet_restore`;
- `wallet_unlock`; and
- `wallet_lock`.

The five no-input wrappers must accept only exact `{}`. Create and restore must accept only an exact
one-key `{ "request": ... }` object with the already-reviewed bounded public request schema.

The generated handler must be invoked through Tauri’s actual invoke resolver/test machinery. Calling
the Rust function, `parse_whole_envelope`, or `WalletLifecycleCommandBoundary::execute` directly is
supporting evidence only and cannot satisfy this layer.

All qualification command attributes, generated handlers, state constructors, and approved test
transport policies must be erased by `cfg(test)` or reside in a standalone fixture excluded from
the product workspace and package. Release-source tests must fail if any qualification command can
compile into the normal application binary.

### Layer B — real WebView raw-transport qualification

A standalone harness must exercise the exact Windows Tauri/Wry/WebView2 transport stack resolved by
the product lockfile. It may be a dedicated test fixture or explicitly selected test binary, but it
must:

- have a distinct application identifier, data directory, window label, capability, and executable
  name;
- contain no Vision wallet runtime, vault, recovery, journal, Core client, signing, or submission
  authority;
- expose only a qualification classifier that reports fixed pass/fail markers and non-sensitive
  body classification;
- never be selected by the product Tauri configuration or packaging commands;
- fail its own startup if launched without the explicit qualification mode;
- be absent from release bundles and installer manifests; and
- be removable without changing production source behavior.

The harness page must issue the payload matrix through the official `invoke` API and every lower
level IPC/custom-protocol path reachable to compromised code in that exact bundled WebView. It must
not assume an undocumented route is unreachable merely because the normal React service layer does
not use it.

Instrumentation may record only:

- transport route identifier;
- content-type classification;
- `InvokeBody::Json` versus `InvokeBody::Raw`;
- command name;
- top-level key names and counts after redaction;
- fixed acceptance or rejection code;
- whether the generated wrapper and command body ran; and
- timestamps plus harness commit and build identity needed for evidence.

It must not record request values, complete bodies, account identifiers, paths, window handles,
process identifiers in support output, or any future secret-bearing content.

## Duplicate-key decision rule

The following textual payload families must be attempted at both the top-level envelope and inside
the nested create/restore `request` object:

- two identical keys with identical values;
- two identical keys with conflicting values;
- valid value followed by malformed value;
- malformed value followed by valid value;
- public key followed by a secret-like key spelling;
- exact-case key combined with a wrong-case variant;
- three or more repeated keys;
- escaped key spellings that decode to the same string;
- repeated keys separated by large but bounded whitespace; and
- repeated keys delivered as UTF-8 bytes, a JavaScript string, and an object where the route allows
  each representation.

The gate passes only if one of these independently reviewed conclusions is proven:

1. Every attacker-reachable route carrying arbitrary textual or binary bytes reaches the custom
   argument as `InvokeBody::Raw` and is rejected, while every route producing `InvokeBody::Json`
   necessarily starts from a JavaScript value model that cannot encode duplicate keys; or
2. a separately reviewed parser at the earliest pre-normalization boundary rejects duplicates
   before construction of `serde_json::Value`.

If a crafted duplicate textual key can be normalized to last-key-wins or first-key-wins JSON and
then accepted as an exact envelope, the result is **Failed**. If the raw boundary cannot be observed
or every attacker-reachable route cannot be enumerated, the result is **Inconclusive**. Failed and
Inconclusive both keep production wallet commands structurally unavailable.

A source argument without real transport execution is not proof. Runtime execution without exact
pinned-source analysis is also not proof.

## Generated-wrapper matrix

Every one of the seven actual generated wrappers must be tested with:

- its exact accepted envelope;
- `InvokeBody::Raw`, including empty, JSON-looking, and arbitrary bytes;
- JSON `null`, boolean, number, string, and array;
- an empty object where a request is required;
- a request object where exact empty input is required;
- missing, extra, wrong-case, and secret-like top-level keys;
- wrong invoked command name and declared/generated command-name mismatch;
- malformed, oversized, unknown, and secret-like nested request fields where applicable;
- a non-main window, remote origin, zero or wrong HWND, stale epoch, and replaced window;
- injected parser, command, response-conversion, and fixed-error construction panics; and
- repeated, concurrent, reordered, and post-revocation invokes.

For every rejection, the evidence must prove:

- the returned body is exactly one reviewed fixed error code;
- no stale success or selection handle is returned;
- no native dialog, filesystem, vault, Core, cryptographic, signing, or submission action starts;
- runtime authority is revoked before the error escapes when the reviewed fail-closed contract
  requires revocation;
- no raw parser/framework error crosses IPC; and
- no canary appears in diagnostics, panic output, support-package inputs, or response bodies.

## Parsing and panic boundary

`WalletInvokeRequest::from_command` must remain a non-fallible borrow of framework-owned command
metadata and the complete `InvokeBody`. It must not parse JSON or format an error outside the wallet
fail-closed boundary.

The generated wrapper’s only fallible response work after the private boundary returns must be the
already-reviewed Tauri delivery of a preconstructed `Response` or `InvokeError`. Qualification must
pin and inspect the expanded macro/source path and inject failures at every locally controllable
stage. A panic before the private guard, a second serialization after guard commit, or framework
formatting that can disclose a request rejects the design and keeps registration blocked.

## Window and origin proof

The test harness must prove that the eventual wrapper cannot trust caller-supplied window strings or
handles. Window authority must derive from framework state and the actual invoking WebView. The
matrix includes:

- exact `main` bundled-window origin;
- another local window;
- a remote origin;
- a destroyed window;
- a recreated main window with a new native identity;
- navigation/reload generation change; and
- an invoke racing lock, suspend, shutdown, or window destruction.

Qualification-only windows and capabilities do not authorize production wallet access.

## Static release-closure checks

Automated tests must prove normal source and release builds contain none of the harness authority:

- no wallet `#[tauri::command]` in non-test source;
- no wallet entry in the production invoke handler or `AppManifest`;
- no generated wallet permission;
- no wallet command in `main-desktop` or any other product capability;
- no React wrapper, direct invoke, form, secret field, or wallet browser state;
- no production constructor that can set `duplicate_key_rejection_proven` to `true`;
- all three independent security approval flags remain `false`;
- no signing/submission transport or recovery export;
- no qualification application identifier, capability, asset, binary, or data directory in the
  release bundle; and
- no dependency or lockfile change unless separately approved before implementation.

The release-closure checks must run against both debug/default and optimized production build
artifacts where applicable.

## Required primary evidence

The evidence commit must preserve, for every automated and operator run:

- exact implementation commit, tree, parent, and clean-worktree proof;
- exact Cargo.lock SHA-256 and resolved Tauri, macro, runtime, Wry, serde, and serde_json versions;
- relevant crate checksums and the SHA-256 of inspected source files;
- harness source/configuration SHA-256 and built executable SHA-256;
- Windows version, WebView2 runtime version, architecture, and operator identity;
- UTC start/end timestamps, complete console output, exact command, and exit code;
- a case identifier for every wrapper and payload family;
- the transport route, observed body classification, fixed result, and whether wrapper/body code
  executed;
- complete success/failure totals with no omitted, retried, or silently replaced run;
- SHA-256 of every transcript and optional screenshot; and
- explicit proof that the product application, wallet files, Vision-Core, and Git state were not
  modified by qualification.

Evidence must use deterministic public canaries. Screenshots are optional; if none are captured,
the record must say so. Summaries without primary transcripts and hashes are non-qualifying.

## Result classification

The final result is exactly one of:

- **Passed** — both generated-wrapper and raw-transport layers satisfy every mandatory case and the
  release-closure checks pass;
- **Failed** — any case accepts a prohibited envelope, loses a fixed error, leaks a canary, reaches
  wallet authority, or packages harness authority; or
- **Inconclusive** — a supported route, framework transformation, source identity, transcript, or
  runtime property cannot be proven.

Only Passed may be submitted for independent acceptance. Even an accepted Pass does not authorize
registration or change `duplicate_key_rejection_proven`; it authorizes only the next separately
reviewed design or implementation decision. Failed or Inconclusive requires correction or a safer
pre-normalization architecture.

## Staged execution plan

1. Obtain independent approval of this documentation design.
2. Implement Layer A only, keeping every command and approved policy under `cfg(test)`, then submit
   its exact commit for review.
3. Implement the standalone Layer B harness without product authority, dependency drift, or release
   inclusion, then submit its exact commit for review.
4. Run the automated generated-wrapper matrix.
5. Run the real packaged Windows/WebView2 raw-transport matrix and preserve primary transcripts.
6. Commit evidence separately and obtain independent acceptance of the exact implementation and
   evidence.
7. If and only if the result is accepted as Passed, prepare a new atomic exposure design. Do not
   register commands or alter the production blocker in the qualification tranche.

At every stage, a finding or ambiguous result returns the product to its existing private,
unregistered state.

## Explicitly prohibited

This design does not authorize:

- production or hidden wallet command registration;
- lifecycle-only beta exposure;
- permissions, capabilities, AppManifest changes, or frontend invokes;
- passwords, credentials, paths, seeds, or signing authority in React;
- activation-flag changes or a production `approved_for_test` equivalent;
- signing, signed-byte transport, submission, sending, retries, or replacement transactions;
- recovery export or arbitrary filesystem access;
- dependency upgrades made merely to bypass a failed result;
- relaxing the private-loopback Core compatibility contract; or
- any Vision-Core modification.
