# Wallet Private Receipt Refresh Implementation Handoff

Date: 2026-08-11

Branch: `fix/wallet-signing-adversarial-matrix`

Approved storage base: `239f64f0c4c8fb483ea1d1be8a2163ea9ee206db`

Approved design: `4a3dcb57a1e7a7b3ca6f242a77b3bf25acd26daa`

## Verdict requested

Independent implementation review is requested only for the private, unregistered, read-only
receipt-refresh coordinator and its exact supporting authority changes. This handoff does not
request command registration, permissions, capabilities, frontend integration, production
activation, signing or submission exposure, publication, Core-manifest relaxation, or Vision-Core
modification.

## Implemented authority path

1. The existing private transaction boundary accepts the already reviewed exact bounded
   transaction-identifier request.
2. The runtime uses non-blocking acquisition to issue one purpose-specific `Refresh` operation for
   the main-window owner and currently unlocked wallet.
3. Under seed-owned authentication, the runtime requires exactly one matching journal-v3 activity
   record and one accepted encrypted envelope with the identical transaction identifier and
   commitment.
4. It revalidates the exact sender, recipient, amount, nonce, tip, fee limit, submission time,
   compatibility digest, canonical transaction identifier, and immutable signed-envelope digest.
5. A read-only receipt trait constructs a fresh supervisor-bound Core client, proves the active Core
   fingerprint, reads exact supported status, and performs exactly one bounded transaction lookup.
6. The exact returned signed transaction and signature are checked against the retained envelope.
7. Runtime and Core identity are revalidated before the observation can reach authenticated local
   storage.
8. The journal append is authenticated and read back before the result escapes. An identical
   observation does not grow the journal or change its prior observation timestamp.

## Structural exclusions

- The receipt source trait has no `submit_once` method and cannot accept `CoreWriteOnce`.
- No POST, retry, replacement, signing, seed export, recovery export, or raw-envelope projection is
  reachable from the coordinator.
- Refresh result and authority types have no production `Debug`, `Clone`, formatting,
  serialization, or unrestricted constructor.
- The existing private transaction boundary remains unregistered.
- All wallet approval constants and `duplicate_key_rejection_proven` remain `false`.
- The current production Core manifest still cannot construct wallet production authority.

## Adversarial evidence

Focused tests use a genuinely signed transaction, encrypted authenticated envelope storage, a
journal-v3 accepted record, and an unlocked wallet runtime. They cover:

- pending, mined, advancing-confirmation, reorganized, and lost observations;
- unchanged observations without journal growth or timestamp fabrication;
- malformed responses, wrong signed envelopes, and Core fingerprint replacement;
- immediate contention rejection, unknown identifiers, Core recovery, and panic containment;
- lifecycle revocation at initial identity, status, lookup, post-lookup identity, and final identity
  checkpoints;
- deterministic journal-write failure preserving the prior authenticated observation; and
- static absence of Core-write, secret, formatting, and submission authority.

Existing receipt-parser and journal suites continue to cover strict response shapes, signature and
transaction-identifier mismatch, malformed block references, confidence presentation, authenticated
journal mutation, crash-safe persistence, and support-package privacy canaries.

## Validation completed

- Focused private receipt-refresh tests: 7 passed, 0 failed.
- Complete serialized Rust suite: 341 passed, 0 failed, 4 operator-only tests ignored.
- Strict Rust formatting and Clippy: passed with zero warnings.
- Tauri authority tests: 7 passed.
- WebView isolation tests: 2 passed.
- Frontend typecheck and state tests: passed.
- Production frontend build: passed.
- Git whitespace validation: passed.
- Exact-scope, approval-flag, command-registration, dependency, and worktree checks are required
  again after the isolated commit is created.

## Required next decision

The exact implementation commit and tree must receive independent security review. Even approval of
this private tranche does not authorize wallet commands, registration, permissions, capabilities,
frontend custody authority, activation, signing, submission, sending, publication, recovery export,
Core-manifest changes, or Vision-Core changes. A later atomic-exposure candidate remains a separate
review and frozen-artifact qualification decision.
