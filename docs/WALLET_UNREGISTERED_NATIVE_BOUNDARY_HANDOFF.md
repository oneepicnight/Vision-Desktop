# Wallet Unregistered Native Boundary — Review Handoff

## Scope

This implementation follows the independently approved design at parent commit `bd5169a`.
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
- Clipboard, text retrieval, context-menu, and IME message routes fail closed. There is no standard
  native or WebView fallback.
- Generated recovery display and acknowledgement use the same fixed-allocation owner-drawn model.
- Recovery acknowledgement mismatch wipes both native operands and cancels the attempt; it does not
  resume a partly completed ceremony.
- Public create and restore schemas contain only bounded `WalletId`, `WalletLabel`, and canonical
  lowercase `RecoverySelectionHandle` values and reject unknown and duplicate fields.
- Recovery capabilities are consumed before native secret ceremonies begin.
- Status, create, restore, unlock, and lock have an outer fail-closed boundary. Uncommitted errors
  invalidate runtime authority. Intercepted panics return only `wallet_runtime_unavailable` after
  invalidation. The process terminates if invalidation cannot be proven.
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
- panic immediately after an unlocked seed enters the session;
- panic before request processing;
- fixed generic panic error and post-panic session invalidation; and
- canonical lowercase recovery capability enforcement.

Real operator-driven accessibility, international keyboard, Windows lock/suspend/teardown, dump,
allocator, and packaged recovery qualification remains part of the final security evidence and is
not replaced by unit tests.

## Review decision required

The next independent reviewer must inspect the exact commit and decide whether this private,
unregistered implementation satisfies the approved design. A positive implementation review still
does not authorize command exposure or wallet activation; those require their own later review.
