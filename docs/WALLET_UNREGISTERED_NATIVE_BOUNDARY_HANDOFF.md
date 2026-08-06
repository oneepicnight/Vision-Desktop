# Wallet Unregistered Native Boundary — Review Handoff

## Scope

This implementation follows the independently approved design at parent commit `bd5169a` and
corrects the implementation findings reported against commit `d3d3134`.
It is deliberately unreachable from the Tauri command surface and from React.

The exact implementation commit must receive another independent security review before any
Wallet command, permission, capability, frontend wrapper, lifecycle form, or activation flag is
added.

## Implemented boundary

- `SecretInput` no longer implements Serde and cannot be created from Tauri JSON.
- Native UTF-16 input and converted UTF-8 bytes use maximum-sized zeroizing allocations created
  before secret entry. Logical length changes without vector growth or reallocation.
- UTF-16 to UTF-8 conversion uses fixed scratch storage and transfers the existing UTF-8 allocation
  into `WalletPassword` without a copying conversion.
- Create, restore, and unlock secret collection is Rust-owned, modal, and parented to the verified
  main native window handle.
- Secret display and input are owner-drawn. No secret is assigned to `STATIC`, `EDIT`, a window
  title, clipboard, accessibility text, or a WebView value.
- Every secret window disassociates Windows IME/text services before accepting input. Clipboard,
  text retrieval, context-menu, all Win32 IME result/context/request/notification/control/key
  routes (including `WM_IME_CHAR` and `WM_IME_SETCONTEXT`), and input-language changes fail closed,
  wipe the ceremony, and close the native window. There is no standard native or WebView fallback.
- Generated recovery display and acknowledgement use the same fixed-allocation owner-drawn model.
- Recovery acknowledgement mismatch wipes both native operands and cancels the attempt; it does not
  resume a partly completed ceremony.
- The unregistered production create and restore entry points consume only bounded
  `WalletCreateRequest` and `WalletRestoreRequest` objects. Those schemas contain only validated
  `WalletId`, `WalletLabel`, and canonical lowercase `RecoverySelectionHandle` values and reject
  unknown and duplicate fields before runtime, filesystem, capability, native UI, or cryptographic
  work. Raw-string lifecycle entry points exist only under `cfg(test)`.
- Recovery capabilities are consumed before native secret ceremonies begin.
- A non-emitting process panic hook is installed before Tauri builder/plugin setup and before the
  Wallet runtime can initialize. It never formats panic payloads, locations, paths, command data,
  native buffers, Wallet state, or backtraces.
- Status, create, restore, unlock, lock, recovery-selection initiation, and recovery-selection
  callback completion have outer fail-closed boundaries. Uncommitted errors
  invalidate runtime authority. Intercepted panics return only `wallet_runtime_unavailable` after
  invalidation. The process terminates if invalidation cannot be proven.
- Recovery-selection permits are runtime-owned and armed. Dropping any uncommitted permit fully
  invalidates Wallet authority, and a callback panic revokes even a just-completed path token.
- Lowercase capability-handle canonicalization is also enforced in the runtime authority layer.

## Explicitly absent

- No `#[tauri::command]` Wallet function.
- No invoke-handler registration.
- No Wallet `AppManifest` entry or generated permission.
- No Wallet capability grant.
- No frontend Wallet lifecycle wrapper or password/recovery form.
- No secret in JavaScript, DOM, reducer state, browser storage, events, logs, CLI arguments, or
  support packages.
- No true lifecycle or independent-security approval flag.
- No signing, sending, submission, receipt activation, recovery export, hardware-wallet support,
  or Vision-Core change.

## Automated evidence

The implementation adds coverage for:

- fixed-capacity UTF-16 and UTF-8 allocation behavior;
- invalid UTF-16, empty, and oversized input;
- comparison and mismatch zeroization paths;
- absence of `SecretInput` Serde deserialization;
- unknown, duplicate, secret-bearing, oversized, and noncanonical public request fields;
- owner-drawn secret handling and blocked native text/clipboard/IME routes;
- direct `WM_IME_CHAR` and input-language-change injection against both secret-window procedures;
- panic before/after bounded request acceptance, capability consumption, every native secret
  ceremony, cryptographic preparation, recovery acknowledgement/publication/verification, vault
  publication, session installation, selection validation/completion, and success commitment;
- uncommitted selection-permit drop and selection-callback panic;
- fixed generic panic error and post-panic session invalidation; and
- canonical lowercase recovery capability enforcement.

Real operator-driven accessibility and Windows IME/international-keyboard testing, Windows
lock/suspend/teardown, dump,
allocator, and packaged recovery qualification remains part of the final security evidence and is
not replaced by unit tests.

## Review decision required

The next independent reviewer must inspect the exact commit and decide whether this private,
unregistered implementation satisfies the approved design. A positive implementation review still
does not authorize command exposure or wallet activation; those require their own later review.
