# Wallet Tauri Layer B Corrective Implementation Handoff

Date: 2026-08-10

Workstation: Vision Desktop ASUS Windows workstation

Branch: `fix/wallet-signing-adversarial-matrix`

Correction parent and qualification implementation: `63b524c915dc919450e7f1b09f27cb8a38ad2528`

Qualification implementation tree: `bb0e465617cecc43c2b15fddfc112bb90039a7d7`

## Result

All findings from the prior independent Layer B reviews remain corrected. Two newly authorized
executions of `63b524c` were attempted after that implementation was approved. The first was
interrupted by a workstation crash and has no terminal manifest. The second completed all 562
isolated processes and produced a terminal manifest, but the runner correctly classified the whole
matrix Inconclusive because it lost every clean child exit code and the forced postMessage fallback
terminal report was rejected.

These executions are Inconclusive and do not claim a Passed result. Production
`duplicate_key_rejection_proven` remains `false`, and all production wallet command, permission,
frontend, activation, signing, and submission surfaces remain closed. This correction is limited
to the qualification runner, native report validation, regression tests, and this handoff.

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

## M-04a correction: surviving native window-destruction evidence

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

The destruction case no longer relies on the page being destroyed to report its own result. Before
destruction, the generated wrapper captures and verifies the exact authorized HWND, target label,
origin, page generation, transport, command, and request. Rust then revokes authority, invokes
forced destruction, and continues as the surviving native controller. A passing native record
requires all of the following:

- the forced-destruction call succeeded for the exact authorized target;
- the target was removed from Tauri's live WebView-window registry;
- Win32 `IsWindow` proves that the captured HWND no longer exists;
- wallet-shaped qualification authority remains revoked; and
- the destroyed target is structurally unable to issue a later successful invoke.

The native destruction record preserves the native-proven transport route, bounded body kind,
top-level key-count and shape classifications, and explicit wrapper/body execution booleans. The
selected official-invoke destruction case requires `custom_protocol_proven`; a PostMessage or
inconclusive route cannot pass even if destruction itself succeeds.

The native controller emits one fixed destruction record and one terminal record directly to the
bounded process transcript, then exits with the matching fixed code. It does not wait for browser
primary, post-revocation, or terminal records from a JavaScript context that no longer exists. The
runner has a dedicated destruction classifier that requires exactly those two native records, the
exact command, route, body classification, execution, and count fields, all destruction/revocation
predicates, the loaded WebView2 identity, and the matching exit code. A clean exit, missing record,
surviving HWND, failed destroy, or mismatched field cannot pass.

The terminal record accurately reports zero browser primary records and zero browser
post-revocation records. Separate fixed fields report one native destruction record and the
structural post-revocation proof derived from authority revocation plus both Tauri-registry and
Win32 HWND absence. No browser evidence is synthesized after destroying its source context.

Every browser-only result is sent to the native reporting protocol and written to process stdout
using a bounded, deny-unknown-fields request and fixed allowlisted values. The selected native case
must match the reported case. Arbitrary values, field names, case identifiers, commands, outcomes,
or transport labels are not emitted.

For non-destruction cases, the native terminal record contains the selected case, fixed result, wrapper-entry count, primary
record count, post-revocation record count, and revoked state. The runner parses bounded JSON
stdout and requires exactly one matching browser primary record, one browser terminal record, one
native terminal record, all required post-revocation evidence, the exact native wrapper count, no
report-rejection marker, and the matching exit code. Exit code 0 alone is never sufficient.
Missing, aborted, timed-out, rejected, malformed, or multiply observed evidence is Failed or
Inconclusive, never Passed. Runner self-tests now include complete and incomplete native
destruction evidence without launching the harness.

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
- a genuine generated-wrapper declared/invoked command mismatch;
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

The mismatch case no longer invokes an unknown command that bypasses every wrapper. For each of
the seven cases, the native router deliberately sends the next permitted framework command through
the selected real `#[tauri::command]` wrapper. The wrapper therefore observes its own generated
declared name and a different framework-provided invoked name, rejects the mismatch, revokes
authority, and rejects the same mismatched route again during the post-revocation probe. Runner
evidence requires both native entries to identify the selected generated wrapper.

Both top-level and nested textual duplicate families now run independently against
`wallet_create` and `wallet_restore`. Restore cases use the distinct
`recovery_source_handle` schema; create cases use `recovery_destination_handle`. Each schema runs
all nine families through string, byte, and normalized-object representations over all three
transport APIs.

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
postMessage fallback, and complete create/restore top-level and nested duplicate families.

Duplicate textual payloads are still attempted as strings, UTF-8 bytes, and normalized JavaScript
objects. Normalized object acceptance is recorded as unsafe evidence, not proof that textual
duplicates were rejected. The production duplicate-key blocker remains false.

## Windows PowerShell 5.1 preflight correction

The runner now derives protected-root-relative paths using a local bounded helper built from
`GetFullPath`, a canonical root prefix, ordinal case-insensitive descendant validation, and a
substring only after that validation succeeds. It does not use the unavailable
`System.IO.Path.GetRelativePath` API. Out-of-root paths fail closed.

Runner self-tests execute under the workstation's Windows PowerShell 5.1 runtime and prove both a
nested relative path and rejection of an outside sibling path. That correction was independently
approved before the incomplete first-case attempt described below.

## Preserved incomplete execution evidence

Evidence directory:

`C:\Vision\wallet-layer-b-evidence-9184b0c-20260810`

Only these immutable primary files exist:

- stdout: 276 bytes, SHA-256 `516257D6C86E1C86D231124DD3F47B9D87E24E8F6865E84DF36640F33479CABC`;
- stderr: 37 bytes, SHA-256 `EDB08204292949C170670BBDF4D779567B3796334E49D3F13605A15B96C1432C`.

Stdout contains one bounded native `layer_b_observation` showing the exact
`wallet_get_status` wrapper accepted the empty JSON object through
`custom_protocol_proven`. Stderr contains only the fixed
`layer_b_browser_observation_rejected` marker. There is no browser primary record, native
terminal record, manifest, or Passed result.

The Desktop worktree remained clean. The four pre-existing Vision-Core working-tree entries were
preserved. The wallet-custody root remained absent, and no qualification process remained after
controlled cleanup.

## First-execution correction

The browser report now uses CORS-safelisted `text/plain;charset=UTF-8` for its cross-origin
custom-protocol fetch. The body remains the same bounded JSON and still undergoes strict
deny-unknown-fields parsing and fixed-value validation in Rust. This avoids an unauthoritative
OPTIONS preflight reaching the POST-only observation handler.

Timeout cleanup now uses the Windows PowerShell 5.1-compatible parameterless `Kill()` method,
then always performs a parameterless `WaitForExit()` to prove process exit and flush redirected
stdout/stderr before transcript classification. A still-live exact process fails closed. Real
Windows PowerShell self-tests launch a bounded timeout probe and a successful redirected-output
probe, proving both termination and transcript flushing through the same helper used by the runner.

## Preserved crash-interrupted `63b524c` execution

Evidence directory:

`C:\Vision\wallet-layer-b-evidence-63b524c-20260810`

The workstation crashed while the authorized matrix was running. The directory contains 1,124
partial transcript files: 562 stdout and 562 stderr files. It contains no terminal evidence
manifest. No qualification process survived the reboot, the Desktop repository remained clean at
the exact approved commit and tree, and the older `9184b0c` evidence hashes remained unchanged.

This directory is permanently Inconclusive. Its individual files must not be combined with another
run or used to claim case or matrix acceptance.

## Preserved complete Inconclusive retry

Evidence directory:

`C:\Vision\wallet-layer-b-evidence-63b524c-20260810-retry1`

Terminal manifest:

`layer-b-evidence-manifest.json`

Manifest SHA-256:

`169C5D1EC12DB0DEB00897A31A7FA2197EB64CCF3526C8C80A91A8799418EC14`

The runner completed all 562 isolated cases and classified all 562 Inconclusive:

- 536 `exit_or_result_mismatch`;
- 14 `missing_or_duplicate_primary_terminal_record`;
- 7 `native_destruction_exit_mismatch`; and
- 5 `case_timeout`.

The manifest proves repository, product, custody, Core, binary, and runtime-inventory integrity was
preserved. Vision Desktop remained at commit `63b524c915dc919450e7f1b09f27cb8a38ad2528`
and tree `bb0e465617cecc43c2b15fddfc112bb90039a7d7`; the wallet-custody root remained absent.

The first case transcript contains the complete native wrapper record, passing browser primary
record, native terminal record, matrix-complete record, and loaded WebView2 version
`151.0.4129.72`. The runner nevertheless recorded a null exit code and discarded those records as
`exit_or_result_mismatch`. A direct Windows PowerShell 5.1 reproduction proved that
`Start-Process -PassThru` on this workstation leaves `ExitCode` null unless the native process
handle is acquired before the child exits.

The forced postMessage fallback transcript separately proves its custom-protocol interception,
`post_message_proven` wrapper execution, and passing primary browser observation. Its terminal
matrix report was rejected because the native validator incorrectly required the terminal
matrix-controller record itself to repeat the primary record's fallback-interception and transport
fields. The process then reached the bounded deadline.

This retry is permanently Inconclusive and must not be rerun or reclassified in place.

## Current execution correction

`Wait-QualificationProcess` now acquires the native child-process handle before waiting. The same
helper remains responsible for bounded termination, proven exit, and redirected-output flushing.
Its Windows PowerShell 5.1 self-tests now assert both a retained successful exit code and a retained
nonzero exit code, in addition to timeout and output-flush behavior.

The forced postMessage fallback requirement now applies only to the primary transport observation.
The matrix-controller terminal record remains subject to the common strict terminal checks but is
not required to claim a transport route or fallback interception that belongs to the primary
operation. The focused Rust regression proves both rejection of an unproven primary record and
acceptance of the correctly bounded terminal record.

No packaged matrix was run after these source changes.

## Source-only validation performed

The following checks passed without starting the harness:

- Layer B Rust unit tests: 20 passed;
- runner self-tests: 24 passed (16 transcript, 3 provenance, and 5 Windows PowerShell compatibility checks);
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
- `src-tauri/qualification/wallet-layer-b/run-layer-b-qualification.ps1`

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
