# Wallet Tauri Layer B Corrective Implementation Handoff

Date: 2026-08-10

Workstation: Vision Desktop ASUS Windows workstation

Branch: `fix/wallet-signing-adversarial-matrix`

Rejected Layer B corrective commit: `3602698c413cda747f1b33956dadbf336cdf9888`

Rejected Layer B corrective tree: `c155dc6d37afbe1e4251f33f0c6da7270d2c9ce2`

## Result

The two remaining findings from the third independent Layer B review have been addressed in the
standalone, non-custody harness source, runner, and source-only tests. The prior guarded-metadata,
linear-revocation, transport-proof, transcript, and genuine-concurrency corrections remain intact.

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

The runner remains reserved for later independently authorized qualification. Only its
non-launching self-test mode was executed in this tranche.

## M-03 correction: inconclusive transport remains inconclusive

Caller-supplied route labels are no longer authoritative in Rust.

The classifier retains Tauri's private invoke key in non-formatting harness state and derives
transport evidence from the actual framework request:

- a custom-protocol request requires the exact private invoke-key header plus the framework
  callback, error, and origin headers;
- a postMessage request requires absence of the custom-protocol framework header set; and
- any mixed, spoofed, missing-key, or otherwise unprovable shape produces the distinct fixed
  `qualification_transport_inconclusive` outcome and cannot be accepted or mistaken for an
  ordinary `invalid_request` rejection.

The browser transcript describes the JavaScript API exercised as a client-side case attribute; it
is not reused as native transport proof. The forced-fallback case must independently establish both
that the custom-protocol fetch was intercepted and that Rust observed `post_message_proven`.
Unproven transport requires a same-process post-revocation proof, a native terminal record, browser
and native Inconclusive results, and process exit code 3. The runner cannot convert that outcome to
Passed.

## M-04a correction: transcript evidence, not exit code, controls acceptance

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

The native terminal record contains the selected case, fixed result, wrapper-entry count, primary
record count, post-revocation record count, and revoked state. The runner parses bounded JSON
stdout and requires exactly one matching browser primary record, one browser terminal record, one
native terminal record, all required post-revocation evidence, the exact native wrapper count, no
report-rejection marker, and the matching exit code. Exit code 0 alone is never sufficient.
Missing, aborted, timed-out, rejected, malformed, or multiply observed evidence is Failed or
Inconclusive, never Passed. Six runner transcript self-tests cover a clean early exit, complete
pass, complete and incomplete concurrency evidence, transport Inconclusive, and native report
rejection without launching the harness.

## M-04b correction: actual eight-invoke concurrency

The former eight separately selected `concurrent-N` processes have been removed. One isolated
`concurrent-batch` case creates all eight official invoke promises before awaiting any result. It
accepts only eight exact accepted results with one consistent native-proven transport. The native terminal
record independently requires eight generated-wrapper entries, or nine when an inconclusive route
also requires the post-revocation probe.

## M-04c correction: actual runtime and complete product identity

The mandatory runner now captures before execution:

- the exact repository commit, tree, parent, and clean-worktree proof;
- the Layer B binary hash;
- the complete installed WebView2 executable inventory, rather than selecting the highest version;
- locked resolved versions for Tauri, Tauri macros, Tauri Wry runtime, Wry, Serde, and Serde JSON;
- Cargo.lock checksums, package-manifest hashes, and the reviewed source-file hashes for those
  dependencies; and
- hashes for the harness, runner, configuration, permission, generated-wrapper, build, lockfile,
  test, and handoff sources.

The native harness now obtains `BrowserVersionString` from the exact WebView2 environment backing
the selected test window. That fixed, bounded version is included in the native terminal record for
every process. A case with no valid loaded-runtime identity is Inconclusive. Final evidence requires
one consistent loaded version across all cases and corroboration against the installed runtime
inventory; an inferred "highest installed" runtime can no longer qualify.

The sanctioned runner now requires the authoritative wallet-custody root, Vision-Core Git root,
production executable, production installation root, and production data root as explicit inputs.
It requires the production executable to reside within the supplied installation. It records
aggregate custody, installed-product, product-data, and Vision-Core identity before and after the
matrix without publishing protected filenames. Evidence output must be disjoint from every
protected root. After execution the runner rechecks the Vision Desktop Git state, installed
WebView2 inventory, Layer B binary, production executable, installation tree, and product data.
Any product, custody, Core, Git, runtime-inventory, or binary identity change forces Failed. Every
case retains its process ID, exact bounded launch arguments, loaded runtime version, bounded full
stdout/stderr, timestamps, exit code, native record counts, classification reason, transcript
sizes, and transcript hashes in the new evidence directory.

## M-05 correction: complete generated-wrapper adversarial matrix

Every one of the seven generated wrappers now receives its own isolated process cases for:

- its exact accepted envelope;
- raw empty, JSON-looking, arbitrary, and byte payloads;
- JSON null, Boolean, number, string, and array payloads;
- missing, extra, wrong-case, secret-like, shape-mismatched, and wrong-command envelopes;
- wrong invoked-command handling;
- all six window, origin, generation, destruction, and revocation scenarios;
- metadata, body, response, observation, and fixed-error panic points;
- sequential repeat, a genuine eight-invoke concurrent batch, reordered invocation, and an
  explicit post-revocation probe.

Create and restore additionally receive malformed, oversized, unknown, and secret-like nested
request cases because those are the only wrappers with nested public request data. Reordered cases
invoke the next generated wrapper followed by the selected wrapper, and the runner independently
requires that exact native order. Concurrent cases issue all eight promises before awaiting and
require eight native entries for the selected wrapper. The runner treats missing or incorrect
sequence/concurrency evidence as Inconclusive.

## Generated-wrapper and payload coverage

The harness still declares exactly seven feature-gated generated Tauri wrappers:

- `wallet_get_status`;
- `wallet_select_recovery_destination`;
- `wallet_create`;
- `wallet_select_recovery_source`;
- `wallet_restore`;
- `wallet_unlock`; and
- `wallet_lock`.

The isolated process list covers every wrapper as described above, plus direct fetch/XHR, forced
postMessage fallback, comprehensive top-level duplicate families, and nested duplicate families.

Duplicate textual payloads are still attempted as strings, UTF-8 bytes, and normalized JavaScript
objects. Normalized object acceptance is recorded as unsafe evidence, not proof that textual
duplicates were rejected. The production duplicate-key blocker remains false.

## Source-only validation performed

The following checks passed without starting the harness:

- Layer B Rust unit tests: 17 passed;
- runner self-tests: 12 passed (9 transcript and 3 provenance checks);
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
