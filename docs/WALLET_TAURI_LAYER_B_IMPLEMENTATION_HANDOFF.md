# Wallet Tauri Layer B Corrective Implementation Handoff

Date: 2026-08-09

Workstation: Vision Desktop ASUS Windows workstation

Branch: `fix/wallet-signing-adversarial-matrix`

Rejected Layer B commit: `7e5791e43c7f836e8e47b12aef71c6adc7c99036`

Rejected Layer B tree: `a73d856794b04f5f5209da7d1ff8e75fa324bbf3`

## Result

The four findings from the independent Layer B review have been addressed in the standalone,
non-custody harness source and source-only tests.

The harness and packaged qualification matrix were not launched. This corrective implementation
does not claim a Passed result. Production `duplicate_key_rejection_proven` remains `false`, and
all production wallet command, permission, frontend, activation, signing, and submission surfaces
remain closed.

## Isolation remains unchanged

The harness remains an opt-in Cargo binary named:

`vision-wallet-transport-qualification`

It compiles only with `wallet-layer-b-qualification`, requires the explicit
`--wallet-layer-b-qualification` launch argument, and now additionally requires exactly one
bounded `--wallet-layer-b-case=<case>--<transport>` selector.

It has a distinct application identifier, data directory, configuration, capability, executable,
window labels, CSP, embedded assets, and disabled bundle. It does not import the Vision Desktop
library or contain custody, vault, recovery, Core, seed, signing, submission, or product
`AppManifest` authority.

## M-01 correction: guarded and redacted metadata

The fail-closed guard is armed before any request metadata classification or allocation performed
by the classifier.

Attacker-controlled key names are no longer cloned, sorted, returned, or printed. Metadata is
reduced to fixed allowlisted values only:

- raw, JSON object, or non-object JSON;
- zero, one, two-to-eight, over-limit, or not-applicable key-count bucket; and
- empty object, exact `request` key, unknown/mixed keys, oversized key, excessive key count, raw
  body, or non-object shape.

Response creation, fixed-error creation, observation serialization, and stdout emission execute
inside fail-closed panic containment. Output failures revoke authority. Source-only regressions
cover a key-name canary, value canary, excessive key count, oversized keys, and fixed
classification. The isolated browser matrix includes metadata, response, observation, body, and
fixed-error panic cases.

## M-02 correction: linear per-process authority

Ordinary classifier rejection no longer commits the guard. The runtime is explicitly invalidated
before a fixed rejection is serialized, observed, or returned. Guard drop provides an additional
idempotent invalidation path.

Every malformed or fail-closed case performs a same-process post-revocation probe. A later exact
invoke must return `qualification_runtime_unavailable`; stale success fails the case.

The checked-in runner launches every selected case in a new independently identified process,
with separate stdout, stderr, exit code, start/end times, time limit, classification, and SHA-256
records. A timeout is Inconclusive, never Passed. Existing evidence directories are never
overwritten.

The runner was added for later independently authorized qualification only. It was not executed in
this tranche.

## M-03 correction: framework-derived transport evidence

Caller-supplied route labels are no longer authoritative in Rust.

The classifier retains Tauri's private invoke key in non-formatting harness state and derives
transport evidence from the actual framework request:

- a custom-protocol request requires the exact private invoke-key header plus the framework
  callback, error, and origin headers;
- a postMessage request requires absence of the custom-protocol framework header set; and
- any mixed, spoofed, missing-key, or otherwise unprovable shape is
  `transport_route_inconclusive` and cannot be accepted.

The browser transcript describes the JavaScript API exercised as a client-side case attribute; it
is not reused as native transport proof. The forced-fallback case must independently establish both
that the custom-protocol fetch was intercepted and that Rust observed `post_message_proven`.

## M-04 correction: executable scenarios and primary transcript

The harness now implements deterministic cases for:

- the authorized local main window;
- another local window label;
- a separately served literal-loopback remote origin;
- destruction and recreation of the main window with a new native identity;
- reload/navigation generation change;
- an invoke racing window destruction; and
- an invoke racing lifecycle-style authority revocation.

Native authority binds the original HWND and first page generation. Recreated windows and later
page generations cannot silently inherit it. Qualification-only control and reporting protocols
are separate from the seven generated wallet-shaped wrappers and contain no custody action.

Every browser-only result is sent to the native reporting protocol and written to process stdout
using a bounded, deny-unknown-fields request and fixed allowlisted values. The selected native case
must match the reported case. Arbitrary values, field names, case identifiers, commands, outcomes,
or transport labels are not emitted.

The per-process runner preserves complete stdout and stderr transcripts, exit codes, timestamps,
case classifications, and transcript hashes. Missing, aborted, timed-out, rejected, or multiply
observed cases are Failed or Inconclusive, never Passed. A destruction scenario whose response
cannot be observed after the target WebView is destroyed therefore fails closed as Inconclusive
rather than being counted as success.

## Generated-wrapper and payload coverage

The harness still declares exactly seven feature-gated generated Tauri wrappers:

- `wallet_get_status`;
- `wallet_select_recovery_destination`;
- `wallet_create`;
- `wallet_select_recovery_source`;
- `wallet_restore`;
- `wallet_unlock`; and
- `wallet_lock`.

The isolated process list covers their exact shapes, raw values, JSON primitives and collections,
missing/extra/wrong-case/secret-like fields, bounded and oversized metadata, malformed nested
requests, unknown commands, concurrency, direct fetch/XHR, forced postMessage fallback, panic
points, top-level duplicate families, and nested duplicate families.

Duplicate textual payloads are still attempted as strings, UTF-8 bytes, and normalized JavaScript
objects. Normalized object acceptance is recorded as unsafe evidence, not proof that textual
duplicates were rejected. The production duplicate-key blocker remains false.

## Source-only validation performed

The following checks passed without starting the harness:

- Layer B Rust unit tests: 13 passed;
- complete Rust baseline: 293 passed, 4 operator-only ignored;
- Tauri authority tests: 7 passed;
- WebView isolation tests: 2 passed;
- strict production and Layer B Clippy;
- Rust formatting;
- JavaScript and PowerShell syntax validation;
- frontend typecheck, state tests, and optimized build;
- locked optimized Layer B linkage build;
- locked optimized production Rust build;
- production executable Layer B marker scan; and
- Git whitespace validation.

The packaged harness and physical matrix remain prohibited until independent review of the exact
corrective commit and tree authorizes execution.

## Corrective files

- `docs/WALLET_TAURI_LAYER_B_IMPLEMENTATION_HANDOFF.md`
- `src-tauri/qualification/wallet-layer-b/main.rs`
- `src-tauri/qualification/wallet-layer-b/tauri.conf.json`
- `src-tauri/qualification/wallet-layer-b/assets/index.html`
- `src-tauri/qualification/wallet-layer-b/assets/harness.js`
- `src-tauri/qualification/wallet-layer-b/run-layer-b-qualification.ps1`
- `src-tauri/tests/tauri_acl.rs`

No dependency or lockfile change is required.

## Prohibited and unchanged

This correction does not authorize or add:

- a transport Passed verdict;
- product wallet command registration;
- product permissions, capabilities, or invoke entries;
- React wallet invokes, forms, or custody state;
- production activation or any approval-flag change;
- signing or submission exposure;
- recovery export;
- Core-manifest relaxation; or
- Vision-Core changes.

## Required next action

Complete the source-only validation gate, inspect the exact diff, commit and push the isolated
correction, and submit that exact commit and tree for independent Layer B implementation review.
Do not launch the harness or run the qualification runner unless that exact implementation is
approved for execution.
