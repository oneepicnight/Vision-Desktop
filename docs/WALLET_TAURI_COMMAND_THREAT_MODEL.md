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
3. The application enforces a single running wallet-owning Desktop process or an equivalent operating-system wallet lock. Completed privately on Windows: `WalletRuntimeState` owns an independent fail-closed kernel mutex in addition to tested Desktop duplicate-launch handling. No custody command is exposed.
4. The production CSP no longer permits general `http://127.0.0.1:*` frontend connections. Development-only connectivity belongs in `devCsp`; production uses IPC only unless a separately reviewed source is required. Completed for the current frontend and protected by automated source and configuration tests.
5. `build.rs` declares every application command through `tauri_build::AppManifest` so custom commands participate in ACL resolution. Completed for the existing command surface; automated tests enforce inventory parity.
6. One explicit capability applies to the `main` window. It lists only individually approved application commands and has no `remote.urls`, wildcard window, shell, generic filesystem, HTTP, clipboard, plugin, or wallet permission. Completed for the existing command surface.
7. The Rust side of the official dialog plugin performs recovery save/open selection. Its JavaScript package is not installed and dialog permissions are not exposed to React.
8. Every secret-bearing request type is non-serializable in the response direction, non-cloneable, non-debuggable, bounded before expensive work, and zeroized on drop. Implemented for the private `SecretInput`; frontend/IPC activation remains gated.
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

Path tokens are random 256-bit opaque values. They are:

- single-use;
- held only in Rust;
- bound to the originating main-window label;
- valid for no more than two minutes;
- invalidated on use, cancel, navigation/reload, window destruction, workstation/session lock, and process shutdown;
- never persisted, logged, included in errors, or accepted from a different window.

The selected path remains in Rust state. A token is not a general filesystem capability: it authorizes exactly one create-new recovery write or one bounded recovery read.

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
- stores the selected path in Rust and returns only a path token and its expiry;
- cancellation returns a fixed `cancelled` code and creates no token.

### `wallet_create`

Input:

- destination token;
- bounded wallet identifier and label;
- local vault password;
- no recovery secret input; Rust generates a 256-bit credential for native presentation.

Behavior:

- consumes and invalidates the destination token before secret work;
- requires the token's original window;
- refuses to run when any compatibility, review, process-lock, vault-exists, or operation-in-progress gate is unmet;
- creates the wallet through the existing recovery-gated onboarding coordinator;
- writes, reads back, decrypts, and identity-verifies the portable artifact before local vault storage;
- returns only locked public metadata with `backup_verified: true`;
- locks and clears all intermediate state on every success, error, panic boundary, cancellation, or window loss.

The command is not idempotent. Duplicate or replayed calls fail because the token is already consumed and the vault/backup paths are create-new only.

### `wallet_select_recovery_source`

Input: none, plus the invoking window supplied by Rust.

Behavior mirrors destination selection but uses a Rust-side open dialog restricted to one regular `.vision-recovery.json` file. It returns only an opaque source token.

### `wallet_restore`

Input:

- source token;
- new bounded wallet identifier and label;
- the exact versioned portable recovery credential;
- a new local vault password.

Behavior:

- consumes the source token before secret work;
- loads and decrypts the bounded artifact inside Rust;
- derives the exact RC2 public identity;
- creates a new current-user-protected local vault without changing the seed;
- never overwrites an existing vault;
- retains the original portable backup and returns locked public metadata;
- never returns the artifact, seed, or selected source path.

### `wallet_unlock`

Input: local wallet password only.

Behavior:

- requires the main window, an existing verified vault, and no conflicting operation;
- applies the existing escalating backoff and indistinguishable wrong-password/corruption error;
- stores the seed only in `WalletSession`;
- returns public metadata, never the seed or vault;
- resets the submitted frontend field regardless of result.

### `wallet_lock`

Input: none.

Behavior:

- is always safe and available when wallet commands are active;
- synchronously drops the unlocked seed and invalidates all secret-operation intents;
- succeeds idempotently without revealing whether secret material was present.

## Secret request representation

Command requests must use a dedicated Rust `SecretInput` type with these properties:

- custom deserialization with a strict byte limit;
- no `Serialize`, `Clone`, `Display`, or derived `Debug`;
- redacted manual `Debug` only if required;
- zeroization of the owned buffer on drop;
- one conversion into `WalletPassword` by ownership transfer;
- no inclusion in validation text, tracing fields, panic messages, or metrics.

React cannot guarantee that JavaScript or IPC serialization buffers are zeroized. The UI must therefore minimize exposure:

- use isolated password fields local to the wallet form;
- never store secrets in shared state, reducer events, context, browser storage, query strings, attributes, analytics, or error objects;
- disable spellcheck, autocomplete persistence where platform behavior permits, and copy controls;
- submit once, clear fields immediately in a `finally` path, and unmount the form;
- disable devtools and debug logging in production custody builds.

This limitation must be disclosed in the independent review. Hardware-wallet support remains the preferred future way to keep signing keys outside the WebView-hosted application process.

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

## Production WebView hardening

Before activation:

- production `connect-src` is reduced to Tauri IPC endpoints only; loopback development sources move to `devCsp`;
- no remote content, CDN script, remote module, `eval`, arbitrary HTML injection, or wallet WebView navigation is permitted;
- explicit capabilities select only the main window and contain no remote URL grants;
- all dependencies are locked, audited, and included in the release provenance;
- the official dialog and single-instance plugins are Rust-side only unless an independently reviewed need proves otherwise;
- support packages continue to exclude wallet directories, activity files, paths, and secret-bearing errors.

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
6. Implement `SecretInput`, `WalletRuntimeState`, process/window binding, operation exclusion, and lifecycle locking with no registered wallet commands. Completed for process ownership, main-window page/close/destruction events, Windows session lock, suspend/standby, logoff/shutdown, teardown, and poison. Unlock and resume do not restore authority.
7. Implement and test destination/source token selection in Rust. Completed privately: native dialogs are main-window-parented; local paths are validated against URI, UNC/device, traversal, alternate-stream, reparse, overwrite, suffix, type, and size hazards; generation-bound cancellation and stale completion are tested; no wallet command or WebView permission is active.
8. Implement create/restore/unlock/lock lifecycle adapters but keep them unregistered behind the activation policy. Completed privately: the fixed local vault, token-authorized recovery files, locked create/restore completion, session unlock, idempotent lock, generation checks, restart-safe public status, and fixed errors are connected and tested. No Tauri command exists.
9. Add the isolated frontend onboarding UI and service wrappers; perform secret-leak and accessibility review.
10. Integrate a supported loopback-only Core release through the separate compatibility workflow.
11. Register only the reviewed wallet commands and run adversarial, recovery, packaging, and signed-release validation.

## Decision record

Approved on 2026-08-01:

- native Rust-side dialog selection rather than a frontend-supplied path;
- the existing local wallet password policy;
- a Rust-generated 256-bit portable recovery credential with exact version and checksum validation;
- one backend-controlled onboarding operation after destination authorization;
- immediate frontend field clearing with no shared-state or persistence;
- no user-facing wallet creation until private loopback operation is available;
- no mnemonic, clipboard recovery, automatic cloud backup, arbitrary filesystem access, or send activation.
- exact-version, Windows-only Rust dependencies for official Tauri dialog and single-instance support, with no JavaScript packages, plugin permissions, or wallet commands;
- explicit AppManifest registration and a single main-window-only Windows capability for the existing non-wallet application commands before either plugin is initialized.
- Windows single-instance enforcement registered before all other startup work; duplicate arguments and working directories are discarded and the existing main window is activated best-effort.
- Rust-only Windows lifecycle monitoring that fails startup closed, clears wallet authority on session lock, suspend/standby, logoff/shutdown, and teardown, and never restores authority on unlock or resume.

Still requiring explicit review before implementation:

- independent cryptographic and application-security approval.
