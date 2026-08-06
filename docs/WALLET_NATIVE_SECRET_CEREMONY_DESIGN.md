# Wallet Native Secret Ceremony Design

## Status

The design in commit `bd5169a` was independently approved for an unreachable implementation. The
current Rust implementation remains private and unregistered: no Wallet command, permission,
frontend form, service wrapper, or production activation flag is authorized by this document.

Every lifecycle secret remains outside React, the DOM, JavaScript strings, Tauri JSON arguments,
frontend state, browser storage, clipboard APIs, logs, events, support packages, and command-line
arguments.

## Threat boundary

The authorized main WebView is treated as potentially compromised. Tauri capabilities prevent a
different window from invoking a command, but they cannot prevent malicious JavaScript inside the
authorized window from reading its DOM or command payloads. Consequently, React may initiate a
public lifecycle intent but may never capture or transport:

- a local vault password;
- password confirmation;
- a portable recovery credential;
- a seed, private key, derived key, or DPAPI factor; or
- recovery file contents or a selected filesystem path.

Residual operating-system threats remain: screen capture of the displayed recovery credential,
accessibility or input-injection tooling, keyloggers, debuggers, crash/process dumps, pagefile and
hibernation capture, and a compromised Windows account or administrator. Native ceremonies reduce
the WebView boundary; they do not claim to defeat those adversaries.

## Native ceremony set

All ceremonies are Rust-owned, modal, and parented to the actual native handle of the invoking
Tauri `WebviewWindow`. They verify that the window is the local `main` window before opening and
continue checking generation-bound runtime authority while visible.

### Create

The native sequence captures a new local password and exact confirmation. Only after the two values
match and satisfy the Rust password policy may onboarding generate a seed. The existing generated
portable recovery credential is displayed and re-entered in a second native ceremony. No file is
published before both ceremonies complete with current authority.

### Restore

One native sequence captures the exact portable recovery credential, a new local password, and
password confirmation. The recovery-selection capability handle must already have been consumed.
The ceremony returns Rust-owned secret types directly to the private adapter.

### Unlock

One native sequence captures only the local password. Failure uses the existing indistinguishable
wrong-password/damaged-vault result and discloses no backoff timing.

### Generated recovery acknowledgement

The existing recovery display/re-entry ceremony remains conceptually required, but its secret text
handling must be brought under the same buffer rules below before any activation.

## Secret input control

A standard Win32 `EDIT` control retains an implementation-owned text buffer that Rust cannot prove
was overwritten. The reviewed implementation must therefore use an owner-drawn native secret input
control that:

- holds the actual UTF-16 input only in a bounded `Zeroizing<Vec<u16>>` owned by Rust;
- renders only a fixed bullet count and never sets the secret as native window text;
- disables clipboard paste/copy, drag-and-drop, context menus, and automatic text services;
- does not expose a value through accessibility APIs;
- supports deliberate backspace, cancellation, focus, and keyboard navigation behavior;
- converts once into a bounded zeroizing UTF-8 secret by ownership-controlled code;
- overwrites UTF-16 and UTF-8 buffers on success, cancellation, revocation, validation failure,
  native-window destruction, unwind, and every early return; and
- never formats a secret through `Debug`, `Display`, panic text, tracing, or metrics.

If an accessible or international-input implementation cannot satisfy this contract, lifecycle
activation remains blocked pending a revised security review. The design must not silently fall
back to a normal WebView or native edit control.

`SecretInput` becomes a Rust-native ownership wrapper and must not implement Serde deserialization
for lifecycle IPC. Native ceremonies construct it directly after enforcing the existing 1,024-byte
maximum. It remains non-serializable, non-cloneable, redacted, and zeroizing.

## Cancellation and revocation

The ceremony closes and erases its buffers when:

- the operator cancels;
- the main window navigates, reloads, closes, or is destroyed;
- Windows locks, suspends, hibernates, logs off, or shuts down;
- the process Wallet lease is lost;
- another lifecycle invalidation advances the runtime epoch;
- the two-minute public recovery-selection capability expires; or
- native window creation, input handling, or authority polling fails.

Unlock and resume never restore a ceremony or captured secret. The user starts again.

## Fail-closed unwind boundary

Every future lifecycle entry point, including status, selection, create, restore, unlock, and lock,
must be wrapped by a Rust fail-closed boundary before it can be exposed.

The boundary arms a guard before command processing. Unless an authorized successful result is
explicitly committed, guard drop synchronously performs full runtime invalidation: lock the Wallet
session, revoke operations, revoke pending selections and capability handles, advance the epoch,
and clear public operation state. `wallet_lock` always invalidates and needs no success exception.

The entry point catches unwind at the outer Rust boundary, allows the armed guard to invalidate,
discards the panic payload, and returns one fixed generic failure. Production panic hooks and crash
reporting must never format panic payloads, command arguments, native buffers, or Wallet state.
Process termination is acceptable when safe recovery cannot be proven; retaining an unlocked
session after an intercepted panic is not.

Required injected tests panic:

- before and after public request validation;
- before and after recovery capability consumption;
- during every native ceremony stage;
- after cryptographic preparation and every publication checkpoint;
- immediately after the seed enters `WalletSession` during unlock; and
- immediately before public success commitment.

Every case must prove the session is locked, authority is invalidated, no success escapes, errors
are fixed, and only already completed atomic/create-new encrypted filesystem effects may remain.

## Public request schemas

Future request types are public metadata only and use `#[serde(deny_unknown_fields)]` plus custom
bounded deserialization wrappers:

- `WalletId`: 1-64 ASCII bytes, limited to letters, digits, `-`, and `_`;
- `WalletLabel`: 1-64 UTF-8 bytes, no control characters, no leading/trailing whitespace;
- `RecoverySelectionHandle`: exactly 64 lowercase hexadecimal ASCII characters, representing 32
  random bytes.

Malformed types, unknown fields, duplicate fields, invalid characters, noncanonical uppercase
handles, and oversized values map to fixed `invalid_request`. A caller never supplies a window
label; Rust derives the actual invoking `WebviewWindow`.

The selection handle is accurately classified as a short-lived, non-secret, main-window-bound,
purpose-bound, generation-bound, single-use capability that crosses React. Its authorized path,
issue time, purpose, and authority state remain only in Rust. No expiry timestamp or path is
returned. The fixed lifetime is two minutes and expiry yields a fixed error.

## Activation policy

Vision Desktop adopts the full-wallet gate for the first custody release. Native lifecycle
ceremonies may be implemented and reviewed while unreachable, but wallet creation remains disabled
until the private-loopback Core boundary, restore drill, signing, submission, receipt tracking, and
spending path have all passed their independent reviews and end-to-end qualification.

Node B must not mine to a newly generated Desktop address before that gate. It must remain stopped
or use an already controlled and independently spendable address.

## Implementation and review sequence

1. Independently approve this revised design. Completed for design commit `bd5169a`.
2. Implement native secret controls, ceremonies, public bounded types, and unwind guards without
   `#[tauri::command]`, invoke registration, AppManifest entries, permissions, frontend wrappers,
   forms, or true activation flags. Implemented privately; exact-commit re-review remains required.
3. Run adversarial, panic-injection, memory, cancellation, lifecycle, and accessibility tests.
4. Independently review the exact unreachable implementation.
5. Integrate and qualify private-loopback Core, signing, submission, receipts, spending, and clean
   recovery.
6. Obtain a separate activation review before adding commands, permissions, or user-facing UI.

Signing and sending remain separate and are never authorized by lifecycle design approval.
