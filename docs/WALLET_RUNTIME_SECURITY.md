# Private Wallet Runtime Security

## Status

Vision Desktop now creates a private Rust `WalletRuntimeState` during Windows application setup.
It is not a wallet feature activation: no wallet command is registered, no capability grants wallet
access, no frontend service calls it, no password form exists, and the native dialog plugin remains
uninitialized.

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
so a stale completion cannot clear newer work. Runtime mutex poisoning invalidates the session,
operation, and recovery authorization and then permanently returns `wallet_runtime_unavailable`.

No operation adapter or Tauri command uses these permits yet.

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

## Recovery authorization framework

The runtime can hold one Rust-only recovery path authorization in preparation for native dialogs.
It uses a random 256-bit opaque token, a two-minute monotonic expiry, exact main-window and
destination/source purpose binding, single-use removal, fixed-size token validation, and zeroizing
token buffers. Issuing a replacement revokes the prior authorization.

No dialog or path-selection command exists in this change. No path or token crosses Tauri. The next
native-dialog slice must validate the selected local path before storing it here and must return
only the opaque token to the reviewed main-window command.

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

Errors contain no path, token, password, wallet material, operating-system detail, or backoff state.

## Validation

Automated tests cover process-lock exclusion and reacquisition, main-window ownership, mutual
exclusion, stale permit behavior, mutex poisoning, random and fixed token paths, expiry, single use,
lifecycle invalidation, native session/power/end-session message classification and dispatch, fixed
errors, bounded secret deserialization, Tauri command absence, capability absence, and frontend
service absence. Unlock and resume are tested as non-restoring events. The complete existing wallet
cryptographic and storage suite remains required.

Executable validation on 2026-08-01 also proved that a normal duplicate exits successfully without
disturbing the primary runtime; a launch made while an external process owns the exact wallet mutex
fails closed with a nonzero exit and leaves no Desktop process alive; and a fresh launch becomes the
sole runtime owner after that mutex is released.
