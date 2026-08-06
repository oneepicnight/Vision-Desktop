# Wallet Tauri Command Threat Model

## Status

This document specifies a future interface. No wallet command is registered with Tauri, no wallet service wrapper exists in React, and wallet creation, restore, unlock, signing, and submission remain unavailable. The pinned single-instance plugin is active first on Windows, followed by the pinned native dialog plugin for private Rust use. Neither plugin has a WebView permission.

The specification is fail-closed. An implementation must not register a partial subset that weakens the ordering, origin, path, lifecycle, or compatibility gates below.

## Evidence inspected

Before the command ACL migration, the Desktop application:

- registered its existing application commands directly through `tauri::Builder::invoke_handler`;
- had one configured main window but no explicit application capability file or `AppManifest` command list;
- does not load remote scripts or remote application pages;
- currently permits `ipc:` and loopback HTTP connections in its production CSP;
- keeps all frontend Tauri calls in `src/services/coreApi.ts`;
- has no wallet command, frontend password form, frontend-accessible file dialog, clipboard wallet path, or wallet state in the general Desktop reducer;
- keeps vault, recovery, session, signing, submission, receipt, journal, and onboarding code inside the private Rust wallet module.

The current Desktop application now also registers its exact 19-command inventory through `tauri_build::AppManifest` and grants only those generated permissions to the explicitly labelled `main` window on Windows. `docs/TAURI_COMMAND_ACCESS_CONTROL.md` records the active command inventory and its automated drift tests. No wallet permission or plugin permission was added.

Tauri 2 documents that registered application commands are available to all application windows and webviews by default unless an `AppManifest` and capabilities narrow them. Capabilities merge when a window belongs to more than one capability, so wallet permissions must not be spread across overlapping broad capability files. Tauri also exposes native save/open dialogs from Rust through the official dialog plugin. These behaviors are the basis for the proposed boundary:

- [Tauri capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri permissions](https://v2.tauri.app/security/permissions/)
- [Tauri runtime authority](https://v2.tauri.app/security/runtime-authority/)
- [Tauri dialog plugin](https://v2.tauri.app/plugin/dialog/)
- [Tauri content security policy](https://v2.tauri.app/security/csp/)

## Protected assets

The design protects:

- the 32-byte Ed25519 wallet seed;
- local wallet passwords and generated portable recovery credentials;
- password-derived keys and the current-user DPAPI-protected local factor;
- decrypted vault contents and unlocked session state;
- transaction signing authority;
- selected recovery paths and short-lived path authorizations;
- transaction intent integrity, nonce, recipient, amount, fee, and reviewed transaction identifier;
- the distinction between locally observed activity and canonical Core state.

Public addresses, balances, nonces, transaction identifiers, block references, and local activity metadata are not secrets, but they remain private operational data and must not enter logs or support packages by default.

## Threat actors and assumptions

The interface must contain:

1. Compromised or buggy React code attempting arbitrary IPC calls.
2. Cross-site scripting in the bundled WebView.
3. A second Desktop window or unintended webview invoking commands.
4. Replayed, duplicated, stale, or reordered IPC requests.
5. A local same-user process racing wallet files or starting another Desktop instance.
6. A malicious or inconsistent Core response.
7. Stolen local vault, stolen recovery artifact, or copied activity journal.
8. Crash reports, logs, diagnostics, support packages, devtools, or reducer state capturing secrets.
9. Path substitution, symlink, overwrite, removable-media, and damaged-backup failures.
10. Process suspension, window destruction, workstation locking, clock regression, and idle sessions.

Tauri capabilities do not protect against malicious Rust code, overly broad scopes, compromised build systems, WebView zero-days, or supply-chain compromise. The release still requires independent review and controlled signed builds.

## Non-negotiable activation gates

No custody command may be registered until all of these are true:

1. The supported Vision-Core release provides loopback-only HTTP binding and the Desktop manifest accepts that exact release.
2. An independent review approves the vault, recovery, session, onboarding, transaction, receipt, and journal boundaries.
3. The application enforces a single running wallet-owning Desktop process or an equivalent operating-system wallet lock. Completed privately on Windows: `WalletRuntimeState` owns a restrictive per-user `Global\` kernel process lease that spans console, fast-user-switching, and RDP sessions in addition to tested Desktop duplicate-launch handling. No custody command is exposed.
4. The production CSP no longer permits general `http://127.0.0.1:*` frontend connections. Development-only connectivity belongs in `devCsp`; production uses IPC only unless a separately reviewed source is required. Completed for the current frontend and protected by automated source and configuration tests.
5. `build.rs` declares every application command through `tauri_build::AppManifest` so custom commands participate in ACL resolution. Completed for the existing command surface; automated tests enforce inventory parity.
6. One explicit capability applies to the `main` window. It lists only individually approved application commands and has no `remote.urls`, wildcard window, shell, generic filesystem, HTTP, clipboard, plugin, or wallet permission. Completed for the existing command surface.
7. The Rust side of the official dialog plugin performs recovery save/open selection. Its JavaScript package is not installed and dialog permissions are not exposed to React.
8. Lifecycle secrets are not request fields. `SecretInput` is a Rust-native, non-Serde,
   non-cloneable, non-debuggable ownership type created only by the fixed-allocation native
   ceremony. Frontend/IPC activation remains gated.
9. Wallet state is held in a dedicated Rust `WalletRuntimeState`; it never enters `DesktopState`, reducer events, browser storage, URL state, or support packages. Implemented privately with no registered command or capability.
10. Create, restore, lock, unlock, duplicate invocation, cancellation, crash, corrupt backup, wrong password, path race, stale token, window mismatch, and process shutdown tests pass.

## Capability design

The active `main-desktop` capability targets exactly the `main` window label on Windows and grants the current non-wallet application commands individually. No wallet permission is part of it or any default or wildcard permission set. Future wallet permissions require an explicit reviewed extension of this same fail-closed boundary; overlapping broad capabilities are prohibited.

Planned application permissions:

- `allow-wallet-get-status`
- `allow-wallet-select-recovery-destination`
- `allow-wallet-create`
- `allow-wallet-select-recovery-source`
- `allow-wallet-restore`
- `allow-wallet-unlock`
- `allow-wallet-lock`

Transaction review, signing, and submission permissions are intentionally absent. They require a later threat model after private loopback integration.

The dialog plugin is initialized in Rust, but its `dialog:allow-save` and `dialog:allow-open` commands are not granted to the WebView. Private Rust adapters call `DialogExt` directly; no wallet application command invokes them yet. React never receives or submits a raw filesystem path.

## Dedicated Rust runtime state

`WalletRuntimeState` now owns privately:

- the locked/unlocked `WalletSession`;
- an optional in-progress operation marker;
- short-lived recovery destination/source tokens;
- window ownership for each token;
- token creation and expiry times based on a monotonic clock;
- a process-wide wallet lock or single-instance guard.

Public wallet metadata is not yet stored in the runtime. Future lifecycle adapters may add only the
reviewed non-secret metadata needed by status responses.

It must not implement `Serialize`, `Clone`, or unrestricted `Debug`. Lock acquisition must fail closed if poisoned. Only one create, restore, unlock, or future signing operation may run at a time.

Recovery selection handles are random 256-bit opaque values encoded as exactly 64 lowercase
hexadecimal characters. The non-secret handle crosses React; the selected path and authorization
record remain only in Rust. A handle is:

- single-use;
- bound to the actual invoking main `WebviewWindow` derived by Rust, never a caller-supplied label;
- purpose- and generation-bound;
- valid for no more than two minutes;
- invalidated on use, cancel, navigation/reload, window destruction, workstation/session lock, and process shutdown;
- never persisted, logged, included in errors, or accepted from a different window.

The handle is a narrow capability, not a secret or general filesystem token. It authorizes exactly
one create-new recovery write or one bounded recovery read. No path, issue time, or expiry timestamp
is returned to React.

## Proposed command interface

### `wallet_get_status`

Input: none.

Returns public state only:

- feature availability and fail-closed reason codes;
- whether a local vault exists;
- locked state;
- public wallet metadata only when available;
- whether a recovery-gated onboarding operation is active.

It never returns paths, encrypted artifacts, timestamps useful for throttling bypass, passwords, secrets, or raw internal errors.

### `wallet_select_recovery_destination`

Input: none, plus the invoking Tauri window supplied by Rust.

Behavior:

- requires the main window and no active onboarding operation;
- opens a Rust-side native save dialog;
- filters and normalizes the extension to `.vision-recovery.json`;
- rejects an existing destination, directories, unsupported URI forms, and non-local paths for the first Windows release;
- stores the selected path in Rust and returns only one canonical recovery selection handle;
- cancellation returns a fixed `cancelled` code and creates no token.

### `wallet_create`

Input:

- destination selection handle;
- `WalletId`: 1-64 ASCII bytes using letters, digits, `-`, or `_`;
- `WalletLabel`: 1-64 UTF-8 bytes, trimmed, with no control characters;
- no secret input.

Behavior:

- derives the actual invoking `WebviewWindow`, requires local `main`, and never accepts a caller
  window label;
- consumes and invalidates the destination handle before prompting or secret work;
- opens a Rust-owned native ceremony for local password and exact confirmation;
- generates the 256-bit portable recovery credential in Rust and uses the existing native display
  and exact re-entry ceremony;
- refuses to run when any compatibility, review, process-lock, vault-exists, or operation-in-progress gate is unmet;
- creates the wallet through the existing recovery-gated onboarding coordinator;
- writes, reads back, decrypts, and identity-verifies the portable artifact before local vault storage;
- returns only locked public metadata with `backup_verified: true`;
- locks and clears all intermediate state on every success, error, panic boundary, cancellation, or window loss.

The command is not idempotent. Duplicate or replayed calls fail because the handle is already consumed and the vault/backup paths are create-new only.

### `wallet_select_recovery_source`

Input: none, plus the invoking window supplied by Rust.

Behavior mirrors destination selection but uses a Rust-side open dialog restricted to one regular `.vision-recovery.json` file. It returns only one canonical source selection handle.

### `wallet_restore`

Input:

- source selection handle;
- the same bounded `WalletId` and `WalletLabel` public types;
- no secret input.

Behavior:

- consumes the source handle before prompting or secret work;
- captures the exact recovery credential, new local password, and password confirmation in one
  Rust-owned native ceremony;
- loads and decrypts the bounded artifact inside Rust;
- derives the exact RC2 public identity;
- creates a new current-user-protected local vault without changing the seed;
- never overwrites an existing vault;
- retains the original portable backup and returns locked public metadata;
- never returns the artifact, seed, or selected source path.

### `wallet_unlock`

Input: none.

Behavior:

- captures the local password in a Rust-owned native ceremony;
- requires the main window, an existing verified vault, and no conflicting operation;
- applies the existing escalating backoff and indistinguishable wrong-password/corruption error;
- stores the seed only in `WalletSession`;
- returns public metadata, never the seed or vault;
- erases all native and Rust ceremony buffers regardless of result.

### `wallet_lock`

Input: none.

Behavior:

- is always safe and available when wallet commands are active;
- synchronously drops the unlocked seed and invalidates all secret-operation intents;
- succeeds idempotently without revealing whether secret material was present.

## Request and secret representation

Every public request structure uses `#[serde(deny_unknown_fields)]`. `WalletId`, `WalletLabel`, and
`RecoverySelectionHandle` use custom bounded deserialization and reject malformed types, duplicate
or unknown fields, oversized input, and noncanonical handle encoding as fixed `invalid_request`.
No request contains a window label or secret.

Lifecycle secrets are captured only by the Rust-owned native ceremonies specified in
`WALLET_NATIVE_SECRET_CEREMONY_DESIGN.md`. Native input transfers directly into the existing
bounded, non-serializable, non-cloneable, redacted, zeroizing secret wrappers. `SecretInput` must not
implement Serde for lifecycle IPC. React never sees a password, confirmation, recovery credential,
seed, ciphertext, selected path, or secret-bearing error.

Hardware-wallet support remains the preferred future way to keep signing keys outside the Desktop
process, but it is not part of this lifecycle design.

## Error contract

Commands return stable enumerated codes with short operator-safe messages. They never return formatted filesystem errors, selected paths, wallet contents, passwords, Core response bodies, ciphertext, or backoff internals.

Required codes include:

- `wallet_unavailable`
- `security_review_required`
- `private_loopback_required`
- `operation_in_progress`
- `invalid_request`
- `password_policy`
- `invalid_recovery_credential`
- `destination_cancelled`
- `destination_expired`
- `destination_invalid`
- `destination_exists`
- `backup_verification_failed`
- `vault_exists`
- `invalid_password_or_vault`
- `temporarily_locked`
- `storage_unavailable`

Frontend messages map from codes locally. Unknown codes display one generic failure and do not expose the raw payload.

## Lifecycle requirements

The Rust wallet session locks and all path tokens are invalidated on:

- explicit lock;
- five minutes without a successful secret operation;
- backward movement of the time source;
- main-window destruction or navigation/reload;
- operating-system session lock or suspend where the platform exposes the event;
- application exit and panic recovery boundaries;
- failure to retain the single-instance/process wallet lock.

Minimizing or losing focus alone does not prove workstation abandonment and is not currently selected as an automatic-lock trigger. The UI may offer a user-configurable stricter policy later.

The current Windows implementation receives session lock, suspend/standby, and logoff/shutdown
messages through a hidden Rust-owned native window. It invalidates authority synchronously and does
not restore it on unlock or resume. Listener registration is a fail-closed startup requirement. No
listener handle, operating-system detail, or lifecycle command is exposed to React.

Every future lifecycle entry point also requires the fail-closed unwind boundary defined in
`WALLET_NATIVE_SECRET_CEREMONY_DESIGN.md`. Its guard is armed before request processing and performs
full runtime invalidation unless an authorized success is explicitly committed. An outer Rust
`catch_unwind` discards panic payloads and returns one fixed generic failure only after the guard
locks the session and revokes operations, selections, and handles. Panic injection immediately
after session unlock is a mandatory gate.

## Production WebView hardening

Before activation:

- production `connect-src` is reduced to Tauri IPC endpoints only; loopback development sources move to `devCsp`;
- no remote content, CDN script, remote module, `eval`, arbitrary HTML injection, or wallet WebView navigation is permitted;
- explicit capabilities select only the main window and contain no remote URL grants;
- all dependencies are locked, audited, and included in the release provenance;
- the official dialog and single-instance plugins are Rust-side only unless an independently reviewed need proves otherwise;
- support packages use an exact in-memory file allowlist, omit raw logs and complete state/configuration objects, exclude wallet directories, activity files, public account activity, paths, and secret-bearing errors, and fail before writing when content cannot be classified.

## Rejected alternatives

- Passing raw backup paths from React: rejected because a compromised frontend could target arbitrary user files or locations.
- Granting JavaScript `dialog:default`, filesystem, shell, HTTP, or clipboard permissions: rejected as unnecessarily broad.
- Keeping registered wallet commands available to every window by default: rejected; application commands must enter Tauri ACL resolution.
- Returning encrypted vault or recovery bytes to React: rejected; encrypted artifacts remain security-sensitive and invite accidental logging or browser persistence.
- Persisting path tokens or onboarding state: rejected; tokens are short-lived process memory only.
- Enabling create/receive while send is blocked: rejected for the first release because users could deposit funds that the Desktop cannot yet spend.
- Treating 50 confirmations as deterministic finality: rejected by the verified RC2 behavior.

## Implementation sequence

1. Obtain independent review of this interface and the existing Rust custody modules.
2. Add and pin the official Tauri dialog and single-instance plugins in a dependency-only commit with provenance review. Completed.
3. Add `AppManifest`, explicit application permissions, and one main-window capability; migrate existing commands without changing behavior. Completed; the 19-command inventory is protected by automated parity tests, while dialog permissions and wallet commands remain inactive.
4. Split production CSP from `devCsp` and prove no frontend direct-Core requests are required. Completed; production permits only Tauri IPC transports and the frontend source boundary is tested.
5. Initialize the single-instance plugin first, discard duplicate launch data, restore the primary main window, and prove ordinary duplicate-launch and process-lock recovery behavior. Completed on Windows; source review retains a dedicated custody-lock requirement for step 6.
6. Implement the Rust-native `SecretInput`, `WalletRuntimeState`, process/window binding, operation
   exclusion, and lifecycle locking with no registered wallet commands. Completed for process
   ownership, main-window page/close/destruction events, Windows session lock, suspend/standby,
   logoff/shutdown, teardown, and poison. Unlock and resume do not restore authority.
7. Implement and test destination/source token selection in Rust. Completed privately: native dialogs are main-window-parented; local paths are validated against URI, UNC/device, traversal, alternate-stream, reparse, overwrite, suffix, type, and size hazards; generation-bound cancellation and stale completion are tested; no wallet command or WebView permission is active.
8. Implement create/restore/unlock/lock lifecycle adapters but keep them unregistered behind the activation policy. Completed privately: the fixed local vault, token-authorized recovery files, locked create/restore completion, session unlock, idempotent lock, generation checks, restart-safe public status, and fixed errors are connected and tested. No Tauri command exists.
9. Independently approve the native-secret, bounded-request, and unwind-guard design. The first
   WebView-secret design was rejected; `WALLET_NATIVE_SECRET_CEREMONY_DESIGN.md` is the correction
   offered for re-review.
10. Implement native secret controls, ceremonies, bounded public request types, and unwind guards
    while keeping the code without `#[tauri::command]`, invoke registration, AppManifest entries,
    generated permissions, capability grants, frontend wrappers/forms, or true activation flags.
11. Independently review the exact unreachable implementation and its adversarial evidence.
12. Integrate and qualify a supported private-loopback Core release, signing, submission, receipt
    tracking, recovery, and a complete spending path through their separate reviews.
13. Obtain explicit activation review, then register only the exact approved commands and
    permissions and run adversarial, recovery, packaging, and signed-release validation.

## Decision record

Approved on 2026-08-01:

- native Rust-side dialog selection rather than a frontend-supplied path;
- the existing local wallet password policy;
- a Rust-generated 256-bit portable recovery credential with exact version and checksum validation;
- one backend-controlled onboarding operation after destination authorization;
- no frontend secret fields; passwords and recovery credentials remain inside Rust-owned native
  ceremonies and fixed-allocation zeroizing buffers;
- no user-facing wallet creation until private loopback operation is available;
- no mnemonic, clipboard recovery, automatic cloud backup, arbitrary filesystem access, or send activation.
- exact-version, Windows-only Rust dependencies for official Tauri dialog and single-instance support, with no JavaScript packages, plugin permissions, or wallet commands;
- explicit AppManifest registration and a single main-window-only Windows capability for the existing non-wallet application commands before either plugin is initialized.
- Windows single-instance enforcement registered before all other startup work; duplicate arguments and working directories are discarded and the existing main window is activated best-effort.
- Rust-only Windows lifecycle monitoring that fails startup closed, clears wallet authority on session lock, suspend/standby, logoff/shutdown, and teardown, and never restores authority on unlock or resume.
- the full-wallet activation gate: lifecycle implementation may advance privately, but user-facing
  creation remains disabled until recovery and spending are qualified end to end.

Still requiring explicit review before implementation:

- the Rust-owned native secret ceremonies, fail-closed unwind guard, and exact bounded public
  request schemas in `WALLET_NATIVE_SECRET_CEREMONY_DESIGN.md`.
