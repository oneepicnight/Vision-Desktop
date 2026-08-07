# Vision Desktop Private Signing Integration Design

## Status and review boundary

This document specifies the next private wallet tranche after acceptance of the native transaction-
confirmation implementation and its six-case physical Microsoft Pinyin/Japanese qualification.

Design basis:

- native-confirmation implementation: `a54bdf06be75b762755b96c87294c82af1dd6920`;
- implementation tree: `42b5332fb163bdd187c44b658f3626a32c31b843`;
- accepted primary-evidence commit: `5e6acbc7c8e7003c11b0520cf42bc4729118d278`;
- accepted evidence tree: `ffd79b1e841fb7c0ae6ae4ac28618b41ab207328`.

This is a documentation-only design. It does not approve or implement signing. The production
signing approval flag remains false. No wallet command, Tauri permission, capability, frontend
wrapper, form, network write, submission path, recovery export, or Vision-Core change is authorized.

Independent design approval may authorize only a private, unregistered Rust implementation of the
confirmation-to-signing bridge described here. A later implementation review remains mandatory.

## Existing implementation facts

The current private tree already provides these reviewed components:

- `WalletRuntimeState` owns the process-wide wallet session, revocation epoch, operation exclusion,
  preview storage, public account binding, and signing activation policy.
- `WalletOperationPermit` validates the main-window owner, operation generation, revocation epoch,
  active-operation slot, and activation scope before and after a sensitive stage.
- `WalletTransactionPreviewEngine` builds one bounded unsigned transfer using authoritative account
  and status reads from a generation-bound `CoreConnectionAuthority`.
- `PendingTransferConfirmation` holds the consumed preview, its still-live runtime permit, and the
  exact Core read authority through native confirmation.
- `NativeConfirmationApproval` is constructible only inside the native-confirmation module after
  verified rendering, focus, IME absence, and accepted physical input.
- `ConfirmedTransferIntent` can be constructed only by consuming that approval capability, but the
  current implementation completes and releases the preview operation before returning the intent.
- `sign_cash_transfer` performs the reviewed RC2 Ed25519 construction and requires a signing-scoped
  `WalletActivationProof`, but it is not connected to confirmation or the unlocked runtime session.
- `WalletSession::with_seed` keeps seed access inside Rust, but it is deliberately not available
  through any Tauri command or frontend state.
- production lifecycle and signing approval flags remain false, and the current Core manifest cannot
  construct production wallet Core authority because the private-loopback peer-binding contract is
  unavailable.

The current pieces are therefore individually useful but are not an end-to-end signing authority.

## Security objective

One verified native confirmation may authorize exactly one signature over exactly the unsigned
transaction that was displayed. The authority must remain continuously bound to:

- the main window;
- the same wallet runtime instance;
- the same operation generation and revocation epoch;
- the same continuously unlocked wallet session and public account;
- the same preview and native approval;
- the same unsigned transaction and canonical identifier;
- the same supervised Core process generation and compatibility fingerprint; and
- the signing activation scope at the deepest authority point.

No lock, unlock, wallet replacement, idle timeout, workstation lifecycle event, main-window loss,
panic, Core exit/restart, compatibility change, or newer wallet operation may occur between approval
and signing without invalidating the operation.

## Required correction to the current handoff

The current confirmation path calls `WalletOperationPermit::complete`, clears the active operation,
and returns `ConfirmedTransferIntent`. A later signing caller would have to start a separate `Sign`
operation. That creates an avoidable handoff interval in which a formerly confirmed intent could be
retained while the runtime session or Core generation changes.

The signing tranche must not build on that release-then-reacquire pattern.

After native approval, the existing `ConsumePreview` permit must be **atomically promoted** to a
signing permit while holding the runtime lock. Promotion must preserve the same operation generation,
owner window, and revocation epoch. The active-operation slot must never become empty between native
approval and signing.

Promotion must fail unless all of the following are true under the same runtime critical section:

1. the permit is still the active `ConsumePreview` operation;
2. no revocation is pending and the revocation epoch is unchanged;
3. the owner is the exact main window;
4. signing activation is currently satisfied;
5. the wallet session is still unlocked;
6. the active wallet identifier and public account match the consumed preview sender;
7. the preview's exact Core identity fingerprint still validates; and
8. no other preview, confirmation, signing, lifecycle, or recovery operation exists.

The promotion consumes the lifecycle-scoped permit and constructs a distinct signing-scoped permit
with private fields. Failure consumes or drops the confirmed intent and releases no signing result.

## Proposed private types

Names may change during implementation, but the authority structure must remain equivalent.

### `WalletSigningPermit`

A private, linear permit constructed only by the runtime's atomic promotion operation. It retains:

- a reference to the exact `WalletRuntimeState`;
- the unchanged operation generation and revocation epoch;
- the main-window owner binding;
- signing-scoped activation proof;
- the matching wallet identifier and public-account identity; and
- an armed fail-closed state.

It implements neither `Clone`, `Copy`, `Debug`, serialization, nor display formatting. Dropping an
armed permit without successful completion invalidates the operation. Panic while invalidation is
unprovable terminates the process.

### `PendingSigningIntent<S>`

A private, non-cloneable container holding:

- `WalletSigningPermit`;
- the exact consumed transfer intent;
- the same generation-bound Core read source retained through confirmation; and
- no secret bytes.

It has no public fields, no serialization, and no unrestricted formatting. It is the only input to
the private signing engine.

### `SignedTransferArtifact`

A private, single-owner result containing only the exact signed transaction, its canonical unsigned
identifier, wallet identifier, and Core identity fingerprint required by a future submission tranche.
It contains no seed, activation proof, session handle, password, recovery value, or generic runtime
authority.

It implements neither `Clone`, `Debug`, nor serialization as a general-purpose trait. A future
submission module may obtain one bounded request body through a narrowly reviewed consuming method;
no other caller may format or export it. Until submission is separately approved, the artifact may
exist only in focused tests and must be dropped inside the private signing tranche.

## Private orchestration flow

The first implementation must expose one private orchestration path and no lower-level bypass:

1. Consume the opaque preview handle through `WalletTransactionPreviewEngine`.
2. Retain the existing `PendingTransferConfirmation`, operation permit, and Core authority.
3. Present the already-qualified Rust-owned native confirmation.
4. On cancellation, UI failure, expiry, IME failure, focus failure, revocation, or panic, destroy the
   intent and invalidate the runtime as already specified.
5. On physical approval, consume `NativeConfirmationApproval` exactly once.
6. Revalidate the runtime permit and exact Core identity fingerprint.
7. Atomically promote the same permit from `ConsumePreview` to `Sign`; do not clear the active slot.
8. Revalidate the promoted signing permit and Core identity immediately before seed access.
9. Enter one runtime-owned signing operation over the unlocked seed.
10. Re-derive the account identity from the seed and require exact wallet, sender-address, and
    sender-public-key equality with the confirmed intent.
11. Recompute and validate every unsigned transaction invariant and canonical identifier.
12. Sign the exact canonical payload, verify the resulting signature with the derived public key,
    and recompute the unsigned identifier.
13. Revalidate runtime authority and Core identity after signing and before any result escapes.
14. Complete the signing permit once and return only `SignedTransferArtifact` to the private caller.

The native-confirmation engine must not return a reusable `ConfirmedTransferIntent` to ordinary
wallet code. Confirmation and signing must execute inside one panic-contained coordinator. The
coordinator catches unwinding across the entire approval/promotion/signing path, invalidates all
wallet authority, and terminates if invalidation cannot be proven.

## Narrow seed-access rule

The signing integration must not expose a generic `with_seed` closure on `WalletOperationPermit`,
`WalletRuntimeState`, or any future command-facing adapter. A generic closure could allow unrelated
wallet code to return seed-derived material.

Instead, the runtime must provide one purpose-specific consuming operation whose inputs are the
private signing permit and confirmed intent. That operation:

- checks `WalletOperationKind::Sign` and `WalletActivationScope::Signing` internally;
- accesses the seed only while the runtime mutex and operation authority are valid;
- passes the seed only to the exact transaction-signing primitive;
- does not permit the closure to return the seed, a signing key, or an activation proof; and
- rejects the result if a pending revocation was requested during signing.

The atomic `pending_revocations` signal must be checked before seed access and again before the
signed result can escape. A lifecycle invalidation waiting on the runtime mutex therefore prevents a
successful return even if the cryptographic calculation already completed.

## Exact transaction binding

The signing primitive must sign the retained unsigned transaction, not rebuild a new transaction
from caller-provided values after confirmation.

Before signing it must independently require:

- sender public key and address are identical canonical lowercase 32-byte hex values;
- the seed-derived public key and address exactly match the confirmed sender;
- recipient is the exact canonical address shown in the native ceremony and differs from sender;
- module is exactly `cash` and method is exactly `transfer`;
- decoded arguments contain exactly the displayed recipient and raw amount with no unknown fields;
- nonce equals the confirmed authoritative nonce;
- tip is exactly `0`;
- charged fee is exactly `1` raw unit under the approved contract;
- fee limit is exactly `201` raw units;
- amount, fee, and total-debit arithmetic is checked and matches the displayed values;
- the stored canonical transaction identifier equals a fresh BLAKE3 calculation over the exact
  unsigned bincode 1.3.3 payload; and
- the stored Core contract and status version match the approved compatibility contract.

After signing, the signature must be decoded as exactly 64 bytes and verified against the same
canonical payload and derived public key. The canonical unsigned identifier must remain unchanged by
the signature field. Any mismatch fails closed and invalidates the runtime operation.

## Secret and memory handling

The existing zeroizing seed remains the sole long-lived signing secret. The implementation must:

- borrow the seed for the shortest possible scope;
- avoid ordinary seed arrays, copies, serialization, heap duplication, and formatted output;
- ensure any expanded signing-key or secret-derived workspace is zeroized on success, error, panic,
  and drop, either through a reviewed dependency guarantee or explicit owned zeroizing storage;
- keep canonical payload and signature data out of logs, panic text, support packages, command-line
  arguments, the frontend reducer, WebView developer tools, and the journal;
- use fixed, non-emitting error codes; and
- add secret-canary tests covering every diagnostic and support-package input.

Public transaction fields and signatures are not custody secrets, but they remain privacy-sensitive
and must not receive unrestricted `Debug` or general logging authority.

## Core-generation checks

The `CoreConnectionAuthority` retained from preview consumption remains mandatory even though the
signing tranche performs no network write.

The private signing coordinator must validate its exact identity fingerprint:

- immediately after native approval;
- immediately before seed access;
- immediately after signature verification; and
- immediately before completing the signing permit.

Core exit, restart, PID or process-creation change, supervisor-generation change, port change,
manifest change, compatibility change, peer-binding failure, or inability to validate the authority
destroys the intent and returns a fixed unavailable error. A signed artifact produced before a failed
post-check must be discarded and must never be reused under a later Core generation.

Production construction remains impossible until the supported private-loopback Core release and
peer-binding contract are present in the compatibility manifest.

## Activation enforcement

Signing activation is enforced at permit promotion and again inside the seed-owning runtime method.
Command wrappers, status booleans, frontend state, or native-confirmation success are not activation
authority.

The implementation must leave these production controls unchanged:

- `INDEPENDENT_SIGNING_SECURITY_REVIEW_APPROVED = false`;
- compatibility approval unavailable;
- private-loopback binding requirement unmet for the current Core manifest; and
- no production caller able to obtain a signing-scoped permit.

Test-only satisfied policies may exercise the private implementation. Negative tests must remove
each activation requirement independently and prove signing cannot occur.

## Error boundary

The private tranche may return only fixed internal categories such as:

- confirmation unavailable or cancelled;
- runtime authority revoked;
- signing activation unavailable;
- wallet locked or changed;
- confirmed intent invalid;
- Core identity unavailable or changed;
- transaction contract mismatch;
- signature construction or verification unavailable; and
- panic-contained runtime failure.

Raw Core text, OS messages, cryptographic errors, paths, PIDs, ports, addresses, nonces, identifiers,
signatures, timing values, and secret-derived details must not enter logs or future IPC errors.

## Required implementation changes

An approved private implementation is expected to remain limited to:

- `src-tauri/src/wallet/runtime.rs`
  - add atomic `ConsumePreview`-to-`Sign` permit promotion;
  - add the narrow intent-specific seed operation;
  - preserve continuous generation and revocation binding;
- `src-tauri/src/wallet/preview.rs`
  - preserve wallet/session binding through confirmation;
  - replace the released generic confirmed intent with the promoted signing handoff;
- `src-tauri/src/wallet/transaction_confirmation.rs`
  - consume native approval and invoke the private signing bridge inside the existing fail-closed
    panic boundary;
- `src-tauri/src/wallet/transaction.rs`
  - sign and verify the exact retained unsigned transaction;
  - narrow production signing visibility and remove general-purpose signed-envelope exposure;
- `src-tauri/src/wallet/signing.rs` (new, private)
  - own the confirmation-to-signing coordinator and private signed artifact;
- `src-tauri/src/wallet/mod.rs`
  - declare the private module without re-exporting signing types; and
- focused Rust tests and security documentation required by the accepted design.

No Tauri command module, permission file, capability, frontend service, React component, shared
Desktop state, event, reducer, plugin, dependency, lockfile, or Vision-Core file belongs in this
tranche.

## Required adversarial tests

The implementation review must include deterministic tests for:

### Authority and replay

- native confirmation is the sole safe constructor of approval;
- a preview handle, identifier, lifecycle proof, or fabricated intent cannot sign;
- one approval produces at most one signing attempt;
- replay, duplicate completion, and reused signed artifacts fail;
- the active-operation slot never becomes empty during promotion;
- another operation cannot interleave between confirmation and signing.

### Runtime and session revocation

- explicit lock before approval, during promotion, during seed access, after signing, and before
  completion;
- idle timeout at each stage;
- same-wallet lock/re-unlock does not preserve prior confirmation;
- wallet replacement and public-account mismatch fail;
- workstation lock, suspend, main-window destruction/reload, process-lock loss, shutdown, and panic
  revoke the operation;
- pending revocation while waiting on the runtime mutex prevents the signed result from escaping.

### Core identity

- Core stop/restart at every transition;
- supervisor generation and fingerprint replacement;
- manifest, compatibility, PID, process-creation, port, and peer-binding changes;
- inability to obtain or validate current authority before and after signing; and
- no signed artifact survives a failed post-sign Core check.

### Transaction integrity

- exact independent payload, identifier, and Ed25519 signature vectors;
- mutation of every sender, recipient, amount, nonce, fee, method, module, argument, contract, status,
  and identifier field;
- self-transfer, zero amount, overflow, malformed addresses, unsupported fee, and stale contract;
- signature verification failure and signature-length mismatch; and
- proof that signing changes only the signature field and not the unsigned identifier.

### Privacy and surface closure

- seed and secret canaries absent from errors, logs, panic output, support packages, serialized
  values, command-line state, and frontend artifacts;
- private capability types lack `Clone`, `Debug`, display, and serialization;
- no wallet command, permission, capability, AppManifest entry, frontend wrapper, or form exists;
- no `POST /transactions` or other network write is reachable; and
- production activation and current-manifest construction remain unavailable.

## Independent review gate

Before implementation, an independent reviewer must approve this exact design or identify required
corrections. Design approval authorizes only the private, unregistered signing bridge.

After implementation, a separate independent review must inspect the exact commit and tree, reproduce
the focused and full validation gates, verify the authority surface remains closed, and explicitly
decide whether a later submission-design tranche may begin.

Even a fully approved private signing implementation does not authorize:

- registering wallet lifecycle, preview, confirmation, or signing commands;
- granting wallet or dialog permissions to React;
- returning signed transaction bytes or signatures through IPC;
- sending a transaction to Core;
- enabling submission, retry, reconciliation, receipt tracking, or recovery export;
- changing either approval flag;
- claiming wallet creation or sending is production-ready; or
- starting the three-node internet mining test with a Desktop-generated address.

Those remain separate reviewed gates.
