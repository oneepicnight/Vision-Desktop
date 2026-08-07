# Vision Desktop Private Signing Implementation Handoff

## Review status

This document records the implementation candidate for independent security review. It does not
approve or activate signing.

Design authority:

- approved design commit: `8b4ab376feea6abb199d32f3f7a33ce262056aa7`;
- approved design tree: `47d52195b95b51b4829f74766846e3bfe31eb325`;
- design finding count: zero open High, Medium, or Low findings.

The implementation remains private and unregistered. Both production approval flags remain false,
the current Core manifest cannot construct production wallet authority, and no wallet command,
permission, capability, frontend wrapper, form, network write, submission path, or Vision-Core
change is included.

## Implemented authority path

The native confirmation engine now owns the only production path into the private signing
coordinator. A successful ceremony creates one non-forgeable `NativeConfirmationApproval`, which is
consumed while promoting the existing `ConsumePreview` permit into `WalletSigningPermit`.

Promotion occurs while the runtime mutex is held and preserves the operation generation,
revocation epoch, main-window owner, wallet identity, and active slot. The old permit is explicitly
disarmed before drop. Direct creation of a `Sign` operation is rejected, including under a fully
satisfied activation policy.

The promoted permit:

- has private fields and no `Clone`, `Debug`, display, or serialization implementation;
- requires signing activation at promotion and again inside the seed-owning method;
- revalidates the unlocked wallet identifier and public account;
- checks pending revocation before seed access and before a result can escape;
- invalidates all wallet authority if an armed permit is dropped; and
- terminates the process if fail-closed invalidation cannot be proven.

## Exact transaction signing

The runtime exposes only an intent-specific seed operation. It signs the exact retained unsigned
transaction and independently verifies:

- the initially empty signature field;
- canonical sender and recipient addresses;
- seed-derived sender address and public-key equality;
- exact `cash` / `transfer` module and method;
- byte-exact canonical JSON arguments;
- nonce, zero tip, charged fee `1`, fee limit `201`, amount, and checked total debit;
- the versioned wallet Core contract and status version `3`; and
- the BLAKE3 identifier over the exact unsigned bincode payload.

The resulting Ed25519 signature is decoded as exactly 64 bytes and verified against the derived
public key. The unsigned transaction identifier is recomputed after signing and must remain
unchanged.

## Core-generation and completion checks

The same generation-bound Core read source remains held through confirmation and signing. Its exact
identity fingerprint is validated:

1. immediately after native approval;
2. after atomic promotion;
3. immediately before seed access;
4. after signature verification; and
5. immediately before signing-permit completion.

Any mismatch or unavailable authority destroys the intent. A signed value created before a failed
post-sign check is discarded.

`SignedTransferArtifact` is private to `wallet/signing.rs`, has no general formatting,
serialization, or cloning authority, and is dropped inside that module. No signed transaction,
signature, or payload escapes this tranche.

## Panic and interruption behavior

The existing confirmation engine's panic boundary encloses native approval, promotion, seed access,
signing, signature verification, Core post-checks, and completion. Panics caught while the runtime
mutex is held first invalidate the in-memory wallet state and are then resumed into the outer
fail-closed coordinator, which invalidates all authority again before returning a fixed error.

Deterministic tests inject panics at:

- promotion;
- pre-seed validation;
- seed/account derivation;
- signature construction;
- signature verification;
- post-sign Core validation; and
- completion.

Every injected panic leaves the wallet authority revoked.

## Adversarial evidence added

Focused tests prove:

- the active-operation slot stays occupied throughout promotion and signing;
- no ordinary caller can begin a signing operation directly;
- exact confirmed transactions sign and independently verify;
- a pre-existing signature is rejected;
- semantically equivalent but byte-different argument JSON is rejected;
- stale contract and status versions are rejected;
- signing changes only the signature field and not the unsigned identifier;
- Core identity replacement after signature construction discards the result and revokes authority;
- all signing-stage panic checkpoints fail closed; and
- the source surface contains no Tauri command, direct network write, or exported signed artifact.

Existing preview tests continue to cover single-use handles, expiry, Core stop/restart and generation
replacement. Existing runtime tests continue to cover lifecycle revocation, idle locking, pending
revocation, process ownership, window binding, and every individual activation requirement.

## Validation baseline

The implementation candidate passed:

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`;
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`;
- full serial Rust suite: 243 passed, 0 failed, 4 operator-only tests ignored;
- Tauri authority suite: 7 passed;
- WebView isolation suite: 2 passed;
- `npm run typecheck`;
- `npm run test:state`; and
- `npm run build`.

## Independent review request

The reviewer should inspect the exact implementation commit and tree and verify conformance with
`WALLET_PRIVATE_SIGNING_INTEGRATION_DESIGN.md`, especially:

- native approval is the sole safe route to signing authority;
- permit promotion never releases the active-operation slot;
- no generic seed or activation-proof escape exists;
- transaction and Core-generation binding are exact;
- panic and revocation paths cannot release a signed result;
- the signed artifact remains private and is destroyed locally;
- production approval flags and current-manifest construction remain unavailable; and
- command, permission, frontend, submission, recovery-export, dependency, and Vision-Core surfaces
  remain unchanged.

Approval of this implementation may authorize only a later design tranche. It must not by itself
authorize wallet exposure, signed-byte transport, transaction submission, sending, recovery export,
approval-flag changes, or Vision-Core modification.
