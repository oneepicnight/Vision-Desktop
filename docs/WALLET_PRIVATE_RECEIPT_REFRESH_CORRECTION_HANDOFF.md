# Wallet Private Receipt Refresh Correction Handoff

Date: 2026-08-11

Branch: `fix/wallet-signing-adversarial-matrix`

Reviewed implementation: `aa3bd1098439afa8cc5d51cc3065ccb489dcdfae`

Reviewed tree: `b023c9937c34c85549762bcba0a71d1b094f88e7`

Reviewed handoff SHA-256:
`4FDB4D6F91BA84E08DDC372901A8E3E53B15F3928ED82FE58C04CE6C73D831DE`

## Verdict requested

Independent implementation re-review is requested only for the isolated correction of M-01, M-02,
and L-01 from the private, unregistered receipt-refresh review. No exposure, activation, signing,
submission, frontend, Core-manifest, dependency, or Vision-Core authority is requested.

## M-01 — Local eligibility now precedes Core authority

- The nonblocking runtime refresh permit is acquired first.
- The matching unlocked wallet, exact journal-v3 activity record, accepted encrypted envelope,
  transaction identifier, immutable commitment, public transaction fields, signature, digest, and
  compatibility expectation are authenticated before the Core authority factory is called.
- Production `refresh()` and the ordering regressions use the same internal authority-factory
  function. There is no test-only parallel sequence.
- Unknown transactions, missing/corrupt envelopes, corrupt journals, and contention prove that the
  authority factory is called zero times.
- The private boundary regression now returns `wallet_transaction_unknown` for missing local
  evidence even though the production Core manifest cannot issue wallet authority.

## M-02 — Mandatory adversarial matrix completed

Deterministic checkpoints now surround every refresh stage:

- permit acquisition;
- local eligibility authentication;
- Core authority issuance;
- initial identity, status, status identity, lookup, lookup identity, parse, and final identity;
- journal write, authenticated read-back, and completion.

The focused matrix proves:

- panic containment and runtime revocation at all 14 stages;
- Core unavailable/replaced behavior at every Core read or identity checkpoint;
- lifecycle revocation during active refresh through the production explicit-lock adapter, actual
  Windows sleep and shutdown message dispatch, and the exact main-window destruction invalidation
  call used by the production event handler;
- authenticated journal and encrypted-envelope mutation after preparation but before recording;
- zero Core-authority requests for every local failure class; and
- no stale observation, success, signing authority, submission authority, or Core write escapes.

## L-01 — Pending revocation is distinct from contention

- `begin_receipt_refresh` returns `RuntimeUnavailable` when lifecycle invalidation is pending.
- Only an existing operation or path-selection owner returns `OperationInProgress`.
- The race regression captures a valid boundary epoch, starts pending revocation, proves that epoch
  is no longer valid, and then proves refresh issuance returns runtime unavailable rather than
  contention.

## Authority surface remains closed

- No Tauri command, invoke registration, AppManifest entry, permission, or capability was added.
- No frontend wrapper, form, secret state, or custody authority was added.
- All three security approval constants and production duplicate-key proof remain `false`.
- Production Core wallet authority remains unavailable.
- No signing, submission, sending, retry, replacement, recovery export, or publication was enabled.
- No dependency, lockfile, configuration, or Vision-Core file changed.

## Validation completed

- Focused corrected receipt-refresh suite: 12 passed, 0 failed.
- Local-first private-boundary regression: passed.
- Pending-revocation issuance race regression: passed.
- Complete serialized Rust suite: 346 passed, 0 failed, 4 operator-only tests ignored.
- Tauri authority tests: 7 passed.
- WebView isolation tests: 2 passed.
- Strict Clippy and Rust formatting: passed with zero warnings.
- Frontend typecheck, state tests, and production build: passed.
- Git whitespace and final authority-surface checks: passed.

## Required next decision

The exact corrective commit and tree require independent security re-review. Approval of this
private correction would authorize no later tranche by itself. Wallet exposure, activation,
registration, signing, submission, publication, and Vision-Core changes remain prohibited.
