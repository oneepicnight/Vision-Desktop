# Wallet Authenticated Envelope Storage Implementation Handoff

Date: 2026-08-11

Branch: `fix/wallet-signing-adversarial-matrix`

Approved design commit: `4a3dcb57a1e7a7b3ca6f242a77b3bf25acd26daa`

Approved design tree: `c3cbcb09260814a680c2e591ef251acbdd47f246`

Approved design SHA-256: `CAC234BF32CB65DC98B1F0767B6A03F6BF14395A0A1E353C6A61DC1C83DFB2EB`

## Verdict requested

Independent implementation review is requested for only the first private, unregistered storage
tranche approved by the design above. This handoff does not request command registration, wallet
activation, read-only receipt refresh, frontend integration, signing or submission exposure, or
publication.

## Implemented scope

- Added a private Rust exact signed-envelope store under the canonical custody directory.
- Encrypts the complete container with XChaCha20-Poly1305 using a seed-derived, domain-separated
  key and a fresh 24-byte nonce for every publication.
- Authenticates an independent head with a separately derived key and supports only a provable old
  or new state across interrupted head/container transitions.
- Freezes the reviewed immutable envelope commitment over the exact canonical fields and excludes
  mutable retention and container state.
- Revised reconciliation to schema/head version 2 with an authenticated genesis head, exact parent
  head, reserved next `Prepared` generation, and immutable envelope commitment.
- Requires an exact, non-forgeable two-way binding between the stored envelope authority and the
  authenticated reconciliation `Prepared` authority before Core-write authority can proceed.
- Implements linear `prepared` to `ambiguous` to `accepted` retention. Accepted or ambiguous
  entries cannot be overwritten or removed through pre-write cleanup.
- Rejects duplicate attempt identifiers, commitments, and canonical transaction identifiers.
  Accepted transaction-identifier uniqueness is permanent while retained.
- Adds preview-time collision refusal before native confirmation and a second collision check during
  signed-envelope publication.
- Revises the authenticated activity journal to schema version 3 and permanently associates each
  accepted transaction identifier with exactly one immutable envelope commitment.
- Requires restart reconciliation to authenticate the exact attempt, transaction, commitment,
  reconciliation parent, parent tag, and reserved generation before retention repair or journal
  completion.
- Adds no-Core orphan cleanup only when the reconciliation head is still the exact authenticated
  parent, no `Prepared` record occupies the reservation, and the authenticated journal contains
  neither the transaction identifier nor commitment.
- Completes authenticated terminal cleanup after interruption while refusing to remove accepted
  history.
- Treats orphan staging files, missing stores, mismatched cross-store fields, wrong wallet keys,
  damaged ciphertext, invalid transitions, and lock contention as fixed fail-closed failures.

## Storage files and authority

The new files are fixed beneath the directory represented by `WalletCustodyPathAuthority`:

- `wallet.signed-envelopes.v1.enc`
- `wallet.signed-envelopes.v1.head.json`

No caller supplies these paths. The encrypted store, reconciliation record/head, activity journal,
and vault remain bound to the same canonical custody authority. The store authority is derived only
while the unlocked seed is held inside the Rust runtime. It is not cloneable, serializable, or
format-capable.

## Interruption and recovery rules

- Head transitions retain the exact predecessor generation and predecessor-of-predecessor tag so
  old-state recovery reconstructs the precise old authenticated head rather than a synthetic head.
- Partial or unexpected staging state blocks authenticated reads and new spending.
- A valid envelope published immediately before reconciliation `Prepared` is removed only through
  the purpose-specific prepared-orphan authority and only after reconciliation and journal absence
  are authenticated.
- A reconciliation phase that proves ambiguity or acceptance can advance a lagging envelope label,
  but an envelope label can never downgrade the reconciliation phase.
- `ResolvedNotAttempted` and reviewed `ResolvedRejected` cleanup is idempotent after the exact
  terminal record is authenticated. `ResolvedRecorded` leaves the accepted envelope retained.
- Missing or mismatched accepted-envelope evidence keeps `accepted_recording_pending`; it never
  fabricates journal completion or downgrades known acceptance to ambiguity.

## Security and privacy properties

- Exact signed bodies and signatures are encrypted at rest and never copied into reconciliation,
  journal, errors, diagnostics, support packages, frontend state, or IPC.
- Production `Debug` is absent from envelope authorities and privacy-sensitive journal records.
- Plaintext container and entry fields are explicitly zeroized on drop; decryption uses zeroizing
  buffers.
- Exact body size and canonical hexadecimal bounds are checked before decoding or allocating the
  decoded body.
- Store access is handle-bound, reparse-aware, ACL-checked, bounded, and protected by immediate
  nonblocking in-process contention rejection.

## Tests added or extended

- Encrypted first publication, read-back, restart, and plaintext-absence checks.
- Wrong seed and ciphertext mutation rejection.
- Exact old/new head-transition recovery.
- Linear retention and accepted-entry non-removability.
- Duplicate transaction identifier and lock-contention refusal.
- Exact prepared-orphan discovery and cleanup.
- Refusal to classify ambiguous or accepted entries as orphans.
- Orphan staging-file refusal.
- Reconciliation reservation, cross-store fields, schema, transition, and interruption coverage.
- Accepted journal commitment association and updated journal-v3 fixtures.
- Submission and restart paths now exercise real encrypted-envelope publication and retention.
- Missing accepted envelope remains visibly `accepted_recording_pending` and cannot create activity.

## Validation

The final candidate passed:

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`: 316 passed,
  0 failed, and 4 operator-only tests ignored.
- Tauri authority tests: 7 passed.
- WebView isolation tests: 2 passed.
- `npm run typecheck`
- `npm run test:state`
- `npm run build`
- `git diff --check`

The final report accompanying the commit records the handoff-file SHA-256.

## Authority surface remains closed

- No wallet Tauri command or invoke registration was added.
- No AppManifest entry, permission, capability, frontend wrapper, form, or custody state was added.
- No approval constant changed; lifecycle, signing, and submission review constants remain `false`.
- `duplicate_key_rejection_proven` remains `false` in production policy.
- Receipt refresh remains fail-closed and unregistered.
- No production Core authority or compatibility-manifest support was added.
- No dependency or lockfile changed.
- Vision-Core was not modified.

## Required next decision

This exact implementation commit and tree require independent security review. Only after explicit
approval may the separately staged private read-only refresh coordinator be implemented. Exposure,
activation, registration, signing/submission authority, and publication remain prohibited.
