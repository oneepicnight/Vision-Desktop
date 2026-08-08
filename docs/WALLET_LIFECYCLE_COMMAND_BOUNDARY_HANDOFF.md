# Private Wallet Lifecycle Command Boundary — Review Handoff

## Exact scope

This implementation follows the independently approved design at parent commit
`de120a55399dd7b4842a6dba5920699b062de840`, tree
`baeaa801bc3e47e70984530593d3ca80bf812d7d`. The approved design document SHA-256 is
`D496833E50EBC3343C7AC1BCF35A1A659C8ECFC996CDB59FDF830AD69C088DD3`.

The candidate is the commit containing this handoff. The submitter's final report must provide its
exact commit, tree, parent, and this document's SHA-256. The complete candidate requires a new
independent implementation review.

This tranche is private Rust infrastructure only. It does not register or expose a wallet command.

## Implemented boundary

`src-tauri/src/wallet/lifecycle_command_boundary.rs` adds:

- `WalletInvokeRequest`, a custom non-Serde `CommandArg` that borrows the complete Tauri invoke
  body and both the statically declared and actually invoked command names without parsing;
- exact whole-object parsing for the seven approved lifecycle command names;
- exact `{}` enforcement for status, destination selection, source selection, unlock, and lock;
- exact one-key `{ "request": ... }` enforcement for create and restore;
- nested bounded `WalletCreateRequest` and `WalletRestoreRequest` deserialization only after the
  whole envelope is accepted;
- unconditional rejection of raw bodies, non-object JSON, unknown commands, mismatched declared
  and invoked command names, missing or extra top-level fields, secret-like fields, wrong-case
  fields, malformed values, and oversized values;
- a production `WholeEnvelopeTransportPolicy` that keeps duplicate-key approval false because
  pinned Tauri `2.11.5` normalizes JSON before command extraction;
- a non-constructible `WalletExposureAuthority` that additionally requires every lifecycle,
  signing, submission, and reconciliation activation scope;
- a non-constructible `MainWalletWindowAuthority` bound to the exact `main` label, bundled Windows
  origin `http://tauri.localhost`, nonzero expected native HWND, and current runtime revocation
  epoch;
- pre- and post-operation epoch validation;
- response JSON construction inside the fail-closed panic boundary and return through an already
  constructed `tauri::ipc::Response`;
- fixed `{ "code": ... }` `InvokeError` construction without raw errors;
- an outer fail-closed guard that invalidates all runtime authority on malformed input, ordinary
  error, callback failure, stale authority, or panic and terminates if invalidation cannot be
  proven;
- command-shaped status, create, restore, unlock, and lock dispatch into the existing private
  lifecycle adapters;
- private Rust-owned recovery destination/source dispatch and callback completion; and
- a post-lock authority renewal that treats successful explicit lock as the intentional revocation
  epoch transition it is, while still requiring a fresh exposure proof before returning success.

`WalletRuntimeState` now provides only crate-wallet-visible, non-serializable helpers to prove all
activation scopes, capture a stable boundary epoch, and validate that epoch before or after an
operation. These helpers do not expose a session, seed, operation permit, activation proof, or
process-lock handle.

## Structural registration blocker

Production duplicate-key rejection remains deliberately unproven. The only production constructor
sets `duplicate_key_rejection_proven: false`; no non-test method can change it. Therefore
`WalletExposureAuthority::issue` returns `wallet_activation_unavailable` even if every existing
activation scope were later enabled.

The only approved transport policy exists under `cfg(test)`. Removing this blocker requires the
separate exact generated-wrapper and raw-transport qualification mandated by
`WALLET_LIFECYCLE_TAURI_EXPOSURE_DESIGN.md` and another review.

## Authority and privacy properties

The following types deliberately implement no production `Debug`, `Clone`, serialization, or
ordinary public construction:

- `WalletInvokeRequest`;
- `WalletLifecycleCommandBoundary`;
- `WalletExposureAuthority`;
- `MainWalletWindowAuthority`; and
- `BoundaryFailClosedGuard`.

Their visibility is restricted to `crate::wallet`. `crate::commands`, the Tauri invoke handler,
React, plugins, and ordinary application code cannot construct or call them.

No password, recovery credential, seed, private key, selected path, ciphertext, process identity,
raw operating-system error, or retry interval appears in a request or response. Selection success
contains exactly one opaque 64-character lowercase hexadecimal handle. Selection cancellation and
all failures contain only one fixed code.

## Focused tests

The implementation directly covers:

- exact empty envelopes for all five no-input commands;
- extra and secret-like top-level fields on every no-input command;
- raw bytes, null, arrays, unknown commands, wrong-case keys, and command-name mismatch;
- exact create/restore envelopes, malformed requests, extra keys, wrong schemas, and oversized
  identifiers;
- production duplicate-key policy blocking exposure;
- production activation policy independently blocking exposure;
- fixed production-boundary activation failure;
- exact main label, bundled origin, native handle, and live epoch checks;
- public-only pre-serialized status;
- malformed-input revocation and secret-canary exclusion;
- panic containment after envelope parsing;
- panic containment during response serialization;
- exact storage-independent lock response with post-lock authority renewal;
- opaque selection-handle response with path exclusion; and
- fixed cancellation response with runtime revocation.

The Tauri ACL regression test also proves that this module has no command attribute, that the
custom whole-message argument borrows both the actual command and complete payload, that raw and
non-exact objects are rejected, that the production duplicate-key blocker is false, and that no
authority type is public or cloneable.

## Explicitly absent

- No Tauri wallet command attribute.
- No invoke-handler registration.
- No `AppManifest` wallet command.
- No generated wallet permission.
- No capability change.
- No frontend wrapper, form, route, reducer event, or browser state.
- No dialog permission granted to the WebView.
- No approval flag change.
- No production duplicate-key transport approval.
- No supported private-loopback Core manifest change.
- No signing, submission, receipt, reconciliation, or recovery-export exposure.
- No dependency or lockfile change.
- No Vision-Core change.

## Required independent decision

The reviewer must decide whether the exact candidate correctly implements the approved private
boundary without expanding authority. Approval may authorize only the next separately designed and
reviewed tranche. It must not authorize command registration, permissions, capabilities, frontend
custody UI, production activation, signing/submission exposure, recovery export, Core-manifest
relaxation, or Vision-Core changes.

Generated Tauri wrapper behavior and duplicate-key handling remain explicit future gates. The
test-only approved transport policy is not release evidence and must never be promoted by a simple
flag change.
