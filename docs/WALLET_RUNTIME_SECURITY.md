# Private Wallet Runtime Security

## Status

Vision Desktop now creates a private Rust `WalletRuntimeState` during Windows application setup.
It is not a wallet feature activation: no wallet command is registered, no capability grants wallet
access, no frontend service calls it, and no password form exists. The pinned native dialog plugin
is initialized on Windows for private Rust use, but no dialog or wallet permission is granted to
the WebView and no wallet command can invoke it yet.

The runtime exists so lifecycle and exclusion controls are established before any secret-bearing
command is designed or exposed.

## Independent process ownership

The runtime atomically acquires the Windows kernel mutex
`Local\com.vision.desktop.wallet-runtime.v1`. It retains the owning handle for the runtime's entire
lifetime. A second process that reaches application setup cannot create wallet runtime state and
fails closed with a fixed, non-sensitive startup error.

This lock is independent of the Tauri single-instance plugin and closes the reviewed interval
between that plugin's Windows mutex creation and hidden receiver-window creation. Normal duplicates
still use the plugin's friendly main-window activation path; the wallet mutex is the final custody
exclusion boundary.

Windows releases the process-owned kernel object after process termination. The lock carries no
secret data and grants no filesystem or wallet access. The default Windows object security can let
another same-session process deny startup by pre-claiming the name; that is a fail-closed denial of
service, not a path to signing authority.

## Runtime contents

`WalletRuntimeState` owns:

- the existing locked-by-default `WalletSession`;
- the independent process lock;
- one optional active secret operation;
- one optional recovery path authorization;
- a monotonic runtime clock and operation generation;
- no serialized, cloneable, or unrestricted-debug representation.

The state is managed directly by Rust/Tauri and never enters React's `DesktopState`, reducer,
events, browser storage, logs, or support packages.

## Operation exclusion and ownership

Only the exact `main` window label can begin a wallet operation. Create, restore, unlock, and future
sign operations share one exclusion slot. A generation-bound permit clears only its own operation,
so a stale completion cannot clear newer work. The private lifecycle adapters now prove that their
permit remains current around cryptographic and filesystem stages. Runtime mutex poisoning
invalidates the session, operation, and recovery authorization and then permanently returns
`wallet_runtime_unavailable`.

The adapters are managed only as private Rust state. No Tauri command exposes them.

## Secret input

`SecretInput` is a dedicated custom-deserialized Rust type with a 1,024-byte UTF-8 ceiling. It:

- accepts only a string value;
- owns a zeroizing buffer;
- moves that buffer into the existing `WalletPassword` wrapper;
- implements no response serialization, clone, display, or debug interface;
- returns no submitted value, length, or content in its validation error.

This bounds the Rust-owned command value but cannot erase copies created by JavaScript, WebView IPC,
or the upstream JSON parser. Frontend custody remains disabled until isolated password forms and
immediate clearing receive independent review.

## Recovery selection and authorization

The pinned native dialog plugin is initialized after single-instance enforcement and before
application setup. Private Rust adapters create parented save/open dialogs for the exact `main`
window. The dialog JavaScript package is absent, the main-window capability grants no dialog
permission, `coreApi.ts` has no wallet wrapper, and no recovery-selection or wallet command is
registered. React therefore cannot open these dialogs or receive a selected path in this release.

Destination and source adapters accept only the plugin's native `FilePath::Path` result. URI forms,
UNC paths, verbatim/device paths, relative paths, parent traversal, alternate data streams, and any
directory chain containing a Windows reparse point are rejected. Destinations are normalized to
the exact `.vision-recovery.json` suffix and must not exist. Sources must already be nonempty,
bounded regular files with that exact suffix. The final recovery writer still uses create-new
storage, and the parser repeats its own bounded regular-file validation, so dialog validation is
not the sole filesystem defense.

Opening a selection revokes any older path authorization and reserves one generation-bound pending
selection. Wallet operations and overlapping dialogs are excluded until completion or cancellation.
Late callbacks, window/session invalidation, and stale permits cannot authorize a path or clear a
newer selection.

After validation, the runtime stores the path only in Rust and produces a random 256-bit opaque
token with a two-minute monotonic expiry, exact main-window and destination/source purpose binding,
single-use removal, fixed-size validation, and zeroizing token buffers. Cancellation clears pending
authority and returns a fixed error. No selected path or token crosses Tauri yet; a future reviewed
main-window command may return only the opaque token, never the path.

## Lifecycle invalidation

The runtime synchronously locks its session and revokes operations and path authorization on:

- explicit internal invalidation;
- main-window close request;
- main-window destruction;
- main WebView page load or reload;
- Windows user-session lock;
- Windows suspend or standby notification;
- Windows logoff or shutdown query and confirmed end-session notification;
- runtime drop and application teardown;
- mutex poison or internal synchronization failure.

Windows lifecycle events arrive through a hidden Rust-owned native notification window registered
for the current session. The window owns only an `Arc` to the private runtime, exposes no Tauri
command or WebView capability, opens no network or filesystem boundary, and adds no polling loop.
Application setup fails closed with a fixed error if the native listener or session notification
registration cannot be established.

Unlock and resume notifications do not restore wallet authority. The user must explicitly unlock
again after a session lock or suspend. Minimize and focus loss do not lock the wallet because they
do not prove workstation abandonment. Native window teardown also performs a final synchronous
invalidation before releasing its runtime reference.

## Fixed error contract

The internal runtime uses fixed codes and operator-safe messages only:

- `wallet_process_lock_unavailable`
- `wallet_runtime_unavailable`
- `invalid_window`
- `operation_in_progress`
- `invalid_request`
- `secure_random_unavailable`
- `path_authorization_invalid`
- `path_authorization_expired`
- `recovery_selection_cancelled`
- `recovery_destination_invalid`
- `recovery_destination_exists`
- `recovery_source_invalid`

Errors contain no path, token, password, wallet material, operating-system detail, or backoff state.

The connected private lifecycle adds its own fixed error mapping and deliberately omits unlock
retry duration. `docs/WALLET_LIFECYCLE_ADAPTERS.md` records the storage, ordering, metadata, and
failure contracts.

## Validation

Automated tests cover process-lock exclusion and reacquisition, main-window ownership, mutual
exclusion, stale permit behavior, mutex poisoning, random and fixed token paths, expiry, single use,
lifecycle invalidation, native session/power/end-session message classification and dispatch, fixed
errors, bounded secret deserialization, Tauri command absence, capability absence, and frontend
service absence. Unlock and resume are tested as non-restoring events. The complete existing wallet
cryptographic and storage suite remains required.

Private lifecycle tests additionally cover create-backup-verify-store ordering, restore to a new
current-user-protected local vault, address equality, locked completion, unlock and idempotent lock, restart
metadata conservatism, non-overwrite behavior, single-use selection, stale-operation invalidation,
and fixed non-disclosing failures.

Executable validation on 2026-08-01 also proved that a normal duplicate exits successfully without
disturbing the primary runtime; a launch made while an external process owns the exact wallet mutex
fails closed with a nonzero exit and leaves no Desktop process alive; and a fresh launch becomes the
sole runtime owner after that mutex is released.
