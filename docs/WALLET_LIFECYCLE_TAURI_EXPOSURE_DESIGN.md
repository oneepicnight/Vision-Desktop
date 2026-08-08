# Wallet Lifecycle Tauri Exposure Design

## Status and review boundary

This document is a design candidate only. It does not authorize or implement a Tauri command,
permission, capability, frontend wrapper, password form, signing path, submission path, approval
flag, Core manifest change, or Vision-Core change.

The parent implementation is commit `31c95a156bc98a3e7b1bb7d549d2b03670864129`, tree
`32a44de596f446c139a4e2cb0df343da51af6580`. Its private, unregistered transaction-submission
implementation received independent approval with no open findings. The lifecycle, native secret
ceremony, native transaction confirmation, private Core client, preview, signing, submission,
reconciliation, and journal implementations remain unreachable from the WebView. All three
production security approval flags remain `false`, and the current Core compatibility manifest
cannot construct production wallet authority.

The next permitted implementation after an independent review of this design is only a private,
unregistered command-boundary adapter with no `#[tauri::command]`, invoke registration, generated
permission, capability grant, React wrapper, or activation change. Actual user-facing activation
must wait for the atomic full-wallet release gate defined below.

## Objective

Define the narrow lifecycle boundary through which the exact bundled `main` WebView may eventually
request public wallet status, Rust-owned recovery selection, create, restore, unlock, and lock.
React may express public intent. Rust owns the window authority, native secret ceremonies, selected
paths, secrets, vault, session, operation scheduling, and fixed error translation.

The lifecycle surface is exactly:

- `wallet_get_status`;
- `wallet_select_recovery_destination`;
- `wallet_create`;
- `wallet_select_recovery_source`;
- `wallet_restore`;
- `wallet_unlock`; and
- `wallet_lock`.

There is no separate address command. A public address appears only inside the existing lifecycle
status or completion response when Rust has derived it. The surface contains no preview, confirm,
sign, submit, reconcile, receipt, recovery-export, arbitrary-file, arbitrary-address, mining-config,
clipboard, shell, filesystem, HTTP, or generic dialog command.

## Atomic full-wallet release gate

Lifecycle implementation approval is not custody activation approval. Vision Desktop must not let
a user create, restore, unlock, or fund a wallet that cannot complete the separately reviewed
spending path. A generated address creates a reasonable expectation that deposits and mining
rewards are recoverable and spendable.

Production exposure therefore requires a non-forgeable `WalletExposureAuthority` (name
illustrative) issued only by the Rust startup authority after every release condition is true:

1. The exact supported Vision-Core release is in the Desktop compatibility manifest and provides
   the reviewed private-loopback and peer-binding contract.
2. Lifecycle, native secret UI, Core client, preview, native confirmation, signing, submission,
   reconciliation, receipts, journal, support-package, and recovery boundaries have exact-commit
   independent approval.
3. The lifecycle, signing, and submission security approval flags are all true for the reviewed
   release tree; no individual flag is sufficient.
4. Packaged Windows lifecycle, recovery, transaction, interruption, and end-to-end spending
   qualification has passed on the supported host profile.
5. The exact Tauri command inventory, permissions, capability, frontend surface, and release
   artifacts pass their final authority review.

The authority has private fields and implements neither `Clone`, `Copy`, `Debug`, serialization,
nor ordinary construction. The command boundary must validate it at entry and inside the deepest
runtime operation. A public status boolean is not authority. The current manifest and false flags
make issuance impossible.

The activation change must be atomic: the reviewed lifecycle and send command inventory, generated
permissions, existing `main-desktop` capability extension, frontend routes, service wrappers,
approval flags, and supported Core manifest land as one reviewed release tranche. A partial
lifecycle-only registration is prohibited. Rollback removes the complete wallet WebView surface.

Node B must not mine to a newly generated Desktop address before this gate passes. Choosing or
writing a mining reward address is a separate explicit operator action and is not part of any
lifecycle response.

## Rust command authority

Future Tauri wrappers receive managed `WalletLifecycleCommandState` and the actual invoking
`tauri::WebviewWindow` from Tauri. They never accept a caller-supplied window label, origin, owner
handle, path, port, PID, compatibility identity, activation proof, or filesystem location.

Before any status disclosure, selection, prompt, or runtime mutation, the boundary must prove:

- the injected window label is exactly `main`;
- the window is the configured local bundled application WebView, not a remote URL;
- the native owner handle is the live main window associated with the managed runtime;
- the current window generation has not been destroyed, navigated, or replaced;
- the process-wide wallet lease and single-instance boundary remain held;
- the full `WalletExposureAuthority` is current; and
- the requested command has its exact `main-desktop` permission.

This check produces a short-lived, non-formatting `MainWalletWindowAuthority` used by the native
adapter. A string equal to `main` cannot construct it. Window destruction, navigation/reload,
session lock, suspend, process-lock loss, or runtime invalidation revokes it and all derived path or
operation authority.

Every wrapper contains unwinding across request validation, authority validation, native callback
coordination, lifecycle invocation, response conversion, and completion. An uncommitted error or
panic invalidates the complete wallet runtime. If invalidation cannot be proven, the process
terminates under the reviewed fail-closed policy. Raw panic text and operating-system errors never
cross IPC.

### Whole-invoke-envelope boundary

The pinned Tauri `2.11.5` generated wrapper does not provide this property for an ordinary Serde
argument. It looks up each named argument independently in the top-level payload; unrelated keys
are ignored. A no-argument command performs no payload deserialization. Argument extraction also
precedes entry into the command function. Consequently, none of the lifecycle commands may use an
ordinary typed command parameter as its security boundary.

Each future lifecycle command instead accepts an injected `tauri::ipc::Request<'_>` (or an
independently reviewed equivalent whole-message argument) plus framework-injected window and state
authority. `Request` extraction itself performs no payload parsing. Inside the first
`catch_unwind`/fail-closed guard, one shared `WalletInvokeEnvelope` parser must:

- reject `InvokeBody::Raw` unconditionally;
- require a JSON object as the complete body;
- verify the exact command name and exact allowed top-level key set before deserializing a value;
- require exactly `{}` for every command documented as `Input: none`: `wallet_get_status`, both
  recovery-selection commands, `wallet_unlock`, and `wallet_lock`;
- require exactly one top-level key named `request` for `wallet_create` and `wallet_restore`;
- deserialize the nested value into the exact bounded request type with unknown and duplicate field
  rejection;
- reject missing, extra, repeated, wrong-case, wrong-type, oversized, noncanonical, and
  secret-like fields with only `invalid_request`; and
- finish all response and fixed-error serialization before the guard commits.

Tauri's JSON `Value` normalization may make a duplicate textual key unobservable by the time an
injected `Request` is constructed. The implementation must prove that the supported invoke
transport cannot deliver such an encoding, or add a reviewed pre-normalization duplicate-detecting
parser at the earliest raw JSON boundary. If neither can be proven with the pinned release, wallet
commands remain unregistered. Silently accepting last-key-wins normalization is prohibited.

The wrapper returns a `tauri::ipc::Response` containing JSON that was fully serialized inside the
guard, or a preconstructed fixed `InvokeError`. Tauri's outer generated response step may only emit
that already converted body; it may not perform fallible wallet response serialization after the
fail-closed guard commits. The implementation must pin and source-test this behavior for the exact
Tauri version used by the release.

## Exact request and response contract

After whole-envelope validation, all accepted public request values are bounded, typed, and
`#[serde(deny_unknown_fields)]`. Missing, duplicate, unknown, wrong-type, oversized, or
noncanonical fields fail before window prompting, token consumption, vault inspection, filesystem
access, or cryptographic work.

### `wallet_get_status`

Input: none.

Output: the existing `WalletLifecycleStatus` only:

- `vault_exists`;
- `locked`; and
- optional `WalletAccountSummary` containing wallet identifier, optional label, public key, public
  address, creation time, and optional backup-verification state.

After restart, account metadata remains absent until successful unlock derives it. The Desktop must
not invent label, backup state, address ownership, or custody based on configuration or Explorer
data.

### `wallet_select_recovery_destination`

Input: none.

Output after the Rust-owned asynchronous dialog completes: exactly
`{ "recovery_selection_handle": "<64 lowercase hexadecimal characters>" }`.

The selected path stays in Rust. Cancel returns fixed code `recovery_selection_cancelled` and no
handle. The handle is non-secret but volatile, purpose-bound, main-window-bound, generation-bound,
single-use, expires within two minutes, and authorizes only one create-new recovery destination.

### `wallet_create`

Input: exactly one top-level `request` object containing the existing `WalletCreateRequest` only:

- bounded `wallet_id`;
- bounded `label`; and
- `recovery_destination_handle`.

No password, confirmation, generated recovery credential, raw path, secret, seed, or owner value is
an IPC field. Rust consumes the handle before opening the native password and recovery-credential
ceremonies. Success returns the existing secret-free locked lifecycle status with public account
metadata and verified backup state. It never auto-copies the address or edits node configuration.

### `wallet_select_recovery_source`

Input: none.

Output: the same single-field response as destination selection, but the handle is purpose-bound to
one bounded read of the selected existing recovery artifact. A source handle cannot authorize a
destination write or vice versa.

### `wallet_restore`

Input: exactly one top-level `request` object containing the existing `WalletRestoreRequest` only:

- bounded `wallet_id`;
- bounded `label`; and
- `recovery_source_handle`.

The recovery credential, new local password, and confirmation are captured only by the Rust-owned
native ceremony. Success returns locked, public lifecycle status. Restore never returns or modifies
the source recovery artifact and never overwrites a vault.

### `wallet_unlock`

Input: none.

The local password is captured only by the Rust-owned native ceremony. Success returns public
`WalletLifecycleStatus`; the seed remains in the zeroizing Rust session. Wrong password and damaged
encrypted data retain one indistinguishable fixed failure.

### `wallet_lock`

Input: none.

Output: the existing `WalletLockResult`, exactly `{ "locked": true }` after synchronous runtime
invalidation. Lock remains storage-independent and idempotent. It must remain available through the
native application lifecycle even when WebView exposure authority is unavailable; IPC availability
must never be the only way to revoke secret authority.

## Secret and privacy boundary

React never renders, receives, owns, submits, validates, stores, or clears a wallet password,
recovery credential, seed, private key, decrypted vault, signing authority, or selected path. The
native ceremonies use fixed-allocation owner-drawn Windows controls and the separately qualified
IME, DPI, focus, input-origin, zeroization, panic, and lifecycle protections.

Public wallet metadata is not cryptographic secret material, but it is private financial telemetry.
It must not enter the general `DesktopState`, reducer event stream, developer transition tracing,
URLs, query strings, browser storage, analytics, logs, crash messages, clipboard, or support
packages. Feature-local memory may retain only the minimum public response needed to render the
current wallet view. Reload clears it. Recovery handles are cleared immediately after one attempt,
success or failure, and are never persisted.

`coreApi.ts` remains the sole frontend Tauri invocation boundary. A future wallet section in that
file may expose only the seven typed methods above. No component imports `invoke`, dialog APIs, HTTP,
filesystem, shell, clipboard, or plugin APIs directly. React forms contain only the public wallet
identifier and label; browser password controls are prohibited.

## Scheduling and command semantics

The existing `WalletRuntimeState` remains the single authority and scheduler. Create, restore,
unlock, selection, and all transaction operations are mutually exclusive under its reviewed
operation/generation/epoch model. Duplicate or conflicting IPC requests return
`operation_in_progress`; they are not queued, retried, coalesced, or automatically replayed.

Selection callbacks must complete through the reviewed Rust callback boundary. The Tauri wrapper
must not keep a raw WebView responder that can outlive window authority. Completion revalidates the
exact window generation before returning a handle. Late, cancelled, duplicated, or post-revocation
callbacks invalidate their authority and return no usable result.

Create and restore are non-idempotent. Consuming a handle, create-new filesystem semantics, and the
existing vault check prevent replay. Unlock does not reveal throttling duration. Lock can run from
native lifecycle invalidation even if another operation is waiting and must revoke it according to
the reviewed epoch protocol.

Command success reports only that the exact Rust lifecycle operation completed. It does not claim
Core connectivity, balance, spendability, synchronization, receipt finality, mining readiness, or
runtime configuration changes.

## Fixed error contract

IPC errors contain only a stable object such as `{ "code": "wallet_unavailable" }`. React maps the
code to reviewed local copy. Rust does not serialize `Display`, `Debug`, source errors, paths,
retry durations, Windows errors, manifest values, process identity, ciphertext, or account data
inside failures.

The lifecycle error vocabulary is closed to the existing fixed codes:

- `wallet_runtime_unavailable`;
- `wallet_activation_unavailable`;
- `invalid_window`;
- `operation_in_progress`;
- `invalid_request`;
- `path_authorization_invalid`;
- `path_authorization_expired`;
- `wallet_already_exists`;
- `wallet_unavailable`;
- `invalid_label`;
- `password_policy`;
- `invalid_password_or_damage`;
- `unlock_temporarily_blocked`;
- `secure_random_unavailable`;
- `recovery_protection_unavailable`;
- `recovery_acknowledgement_cancelled`;
- `recovery_acknowledgement_unavailable`;
- `recovery_destination_exists`;
- `recovery_storage_unavailable`;
- `recovery_backup_mismatch`;
- `vault_protection_unavailable`;
- `vault_storage_unavailable`; and
- `clock_unavailable`.

Recovery dialog cancellation may use the already defined fixed runtime code
`recovery_selection_cancelled`. Any future change to this allowlist requires boundary review.

## Tauri ACL and manifest design

At final atomic activation, `src-tauri/build.rs` adds exactly the seven lifecycle command names to
the existing `tauri_build::AppManifest`. Tauri generates one permission per command. The existing
Windows `main-desktop` capability is extended with exactly those seven permissions and continues to
target only window label `main`.

No second or overlapping wallet capability is created. No wildcard, default command set,
`remote.urls`, plugin dialog permission, filesystem permission, HTTP permission, shell permission,
clipboard permission, or broad scope is added. The Rust dialog plugin stays permissionless to the
WebView and is invoked only inside Rust.

Automated ACL tests must fail if invoke registration, `AppManifest`, generated permission files,
the `main-desktop` capability, documented inventory, or `coreApi.ts` wrappers disagree. They must
also prove that a second window, remote origin, and every command not explicitly listed are denied.

The tests must invoke the actual generated wrappers, not only the private parser. For every command
they exercise an exact valid JSON envelope, raw bytes, non-object JSON, extra and missing top-level
keys, wrong command envelopes, secret-like keys, empty-body requirements, malformed nested values,
and any duplicate-key representation reachable through the supported IPC transport. They also
prove that parsing, lifecycle execution, success serialization, and fixed-error construction are
inside the fail-closed panic boundary. A unit test calling the Rust function directly is
insufficient acceptance evidence.

## Required adversarial evidence

Before any registration or activation, the exact implementation must prove:

- all generated command wrappers reject raw bodies and unknown, duplicate, missing, malformed,
  oversized, and secret-like top-level or nested fields;
- every no-input generated wrapper accepts only an exact empty JSON object;
- injected parser and response-conversion panics invalidate authority and return no stale success;
- a caller cannot supply or forge window, path, exposure, runtime, process, Core, or operation
  authority;
- every lifecycle command rejects non-main, remote, destroyed, replaced, reloaded, and stale window
  generations;
- destination/source handles reject wrong purpose, wrong window, wrong generation, expiry, replay,
  duplicate completion, and use after lock, suspend, navigation, or shutdown;
- concurrent and reordered status, selection, create, restore, unlock, lock, preview, signing, and
  submission attempts preserve one runtime authority and fail closed;
- native cancellation, window loss, workstation lock, suspend, panic, callback panic, and process
  lease loss return no secret and no stale success;
- create and restore return locked public status only, never auto-configure mining, and never expose
  paths or credentials;
- restart status does not invent account metadata; unlock derives and returns only the reviewed
  public identity;
- lock revokes the session even with missing, damaged, replaced, or inaccessible storage;
- error and response serialization passes secret, path, process, timing, account, and ciphertext
  canaries;
- static source tests find no wallet secrets, password controls, browser persistence, direct
  `invoke`, dialog, HTTP, filesystem, shell, or clipboard access outside the approved boundary;
- support packages, diagnostics, logs, panic handling, and crash material exclude wallet telemetry
  and secret canaries; and
- exact production packages pass clean-device create, backup acknowledgement, restart, unlock,
  fund, preview, native confirmation, sign, submit, receipt/reorganization tracking, lock, restore,
  and spend-after-restore drills against the supported private-loopback Core.

The packaged Windows matrix must include console lock, sleep, supported single interactive session,
main-window destruction, ordinary shutdown, forced termination at each publication phase, power
loss recovery, process lease loss, DPI/layout/IME coverage, and interruption at every transaction
authority transition. The standard product does not claim unsupported concurrent Windows session
custody.

## Staged implementation and review plan

1. Obtain independent approval of this exact documentation design.
2. Implement a private `WalletLifecycleCommandBoundary` and private window/exposure authority types
   with command-shaped methods and focused Rust tests. Do not use `#[tauri::command]`, register an
   invoke handler, create permissions, change capabilities, add frontend code, or change flags.
3. Submit that exact private implementation for independent review and correct every finding.
4. Integrate and qualify the supported private-loopback Core release through the separate Core
   compatibility workflow without changing Vision-Core here.
5. Complete the final packaged Windows and clean-device end-to-end wallet matrix, including a real
   funded send and spend-after-restore drill.
6. Obtain final independent review of the combined lifecycle and transaction command contract,
   permissions, frontend public-intent UI, binaries, evidence, and support-package exclusions.
7. Only then land the one atomic activation tranche described above.

Any failed gate returns to the private, unreachable state. There is no temporary lifecycle-only
exposure, beta custody address, hidden command, developer bypass, mock production authority, or
manual flag override.

## Decisions deliberately deferred

This design does not decide or authorize hardware wallets, external signers, multiple local
wallets, arbitrary account lookup in Wallet, recovery export or rotation, wallet deletion, password
change, metadata persistence, auto-lock configuration, clipboard copy, address-book storage,
mining reward-address editing, fee customization, transaction replacement, automatic retry, or
remote Core operation. Each requires a separate threat model and review.
