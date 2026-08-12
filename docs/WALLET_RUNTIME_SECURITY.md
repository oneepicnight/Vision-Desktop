# Private Wallet Runtime Security

## Status

Vision Desktop now creates a private Rust `WalletRuntimeState` during Windows application setup.
It is not a wallet feature activation: no wallet command is registered, no capability grants wallet
access, no frontend service calls it, and no password form exists. The pinned native dialog plugin
is initialized on Windows for private Rust use, but no dialog or wallet permission is granted to
the WebView and no wallet command can invoke it yet.

The runtime exists so lifecycle and exclusion controls are established before any secret-bearing
command is designed or exposed.

## Supported Windows host matrix

Wallet custody is limited to one interactive session per Windows account on these Windows 11
release families:

- 24H2, build family 26100;
- 25H2, build family 26200; and
- 26H1, build family 28000.

Within those build families, the exact non-evaluation edition allowlist is:

- Home, Home N, Home China, and Home Single Language;
- Pro and Pro N;
- Pro for Workstations and Pro for Workstations N;
- Pro Education and Pro Education N;
- Enterprise and Enterprise N;
- Enterprise LTSC and Enterprise LTSC N; and
- Education and Education N.

Windows 10, evaluation editions, Enterprise E/G, Pro Single Language, Windows SE, Cloud editions,
Server/RDS, Enterprise multi-session, IoT, unlisted editions, unknown values, and future Windows
versions or build families are deliberately unsupported. Supporting another host requires a code,
test, documentation, and independent-review update; it is never inferred from a broad Client label.

## Independent process ownership

The runtime atomically creates a per-user Windows kernel mutex in the global namespace:
`Global\com.vision.desktop.wallet-runtime.v2.<BLAKE3-user-SID>`. It retains the non-inheritable
handle for the runtime's entire lifetime. The SID is hashed before it enters the name, and the
object DACL grants access only to the current user, Local System, and built-in administrators. A
second process for the same Windows user cannot create wallet runtime state and fails closed with a
fixed, non-sensitive startup error. The kernel object is global across Windows sessions as defense
in depth, but the supported product boundary is the exact matrix above. Concurrent same-account
Windows Server/RDS and other multi-session environments are unsupported.

This lock is independent of the Tauri single-instance plugin and closes the reviewed interval
between that plugin's Windows mutex creation and hidden receiver-window creation. Normal duplicates
still use the plugin's friendly main-window activation path; the wallet mutex is the final custody
exclusion boundary.

The mutex is a process-held named-object lease and is deliberately not thread-owned. Normal drop or
Windows process termination closes the handle and releases the name without relying on teardown
from the creating thread. The lock carries no secret data and grants no filesystem or wallet
access. Another same-user process, System, or an administrator can still deny startup by
pre-claiming or retaining the name; that is a fail-closed denial of service, not a path to signing
authority. `docs/WALLET_CROSS_SESSION_OWNERSHIP.md` records the exact contract and remaining manual
Windows-session qualification.

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
so a stale completion cannot clear newer work. Each permit also captures an atomic revocation
epoch. Invalidation increments a pending counter and advances that epoch before waiting for the runtime
mutex, then clears the session, operation, and path authority. Sensitive lifecycle stages validate
the epoch before execution and again before their result can escape; final success consumes the
operation slot while rechecking the same epoch under the runtime mutex. A completed atomic file
publication may remain after revocation, but no credential, decrypted result, status, or success
response from that operation is accepted afterward. Runtime mutex poisoning performs the same
fail-closed revocation and returns `wallet_runtime_unavailable`.

The adapters are managed only as private Rust state. No Tauri command exposes them.

## Secret input

`SecretInput` is a Rust-native ownership type with a 1,024-byte UTF-8 ceiling. It is constructed
only by a fixed-allocation native ceremony and:

- has no Serde implementation and cannot be constructed from Tauri JSON;
- preallocates the maximum UTF-8 buffer before controlled UTF-16 conversion and never grows it;
- moves the controlled allocation into the existing `WalletPassword` wrapper;
- implements no response serialization, clone, display, or debug interface;
- returns no submitted value, length, or content in its validation error.

No secret crosses JavaScript, WebView IPC, the upstream JSON parser, frontend state, or DOM controls.
Frontend custody remains disabled until the native ceremony implementation and complete lifecycle
boundary receive independent review.

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

Automated tests cover per-user global naming, restrictive object security, process-lock exclusion,
forced-child-termination reacquisition, main-window ownership, mutual
exclusion, stale permit behavior, atomic epoch revocation while a sensitive stage is running,
queued and overlapping invalidation while the runtime mutex is held, rejection of authority while revocation is
pending, final-success suppression, mutex poisoning, random and fixed token paths, expiry, single use,
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

Automated validation on 2026-08-03 replaced the session-local mutex with the per-user global
process lease. A spawned child process acquired the real protected object, the parent failed closed
while that child was alive, the child was forcibly terminated, and the parent then acquired the
same name successfully. Release qualification requires the same exclusion, normal release, and
forced-termination recovery in the supported single interactive session. Concurrent same-account
console/RDP, Fast User Switching, Windows Server/RDS, and multi-session virtual desktop operation
remain unsupported rather than silently qualified.

Runtime initialization obtains the actual Windows major, minor, build, service-pack, and product
family through native `RtlGetVersion`, requires the workstation family and one of the three reviewed
build families, and passes the actual version into `GetProductInfo`. Only the exact named edition
allowlist above is accepted. This prevents a future release from entering custody through
`GetProductInfo` backward product mapping. Any API failure or unlisted identity returns the fixed
`unsupported_windows_host` error before the wallet process lease or custody state is created.
