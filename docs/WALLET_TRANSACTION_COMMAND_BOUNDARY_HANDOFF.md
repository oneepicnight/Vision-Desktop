# Private Wallet Transaction Command Boundary — Review Handoff

## Exact scope

This implementation follows the independently approved design at parent commit
`669e938f7195bcd776527a3cf796c1a87347abc6`, tree
`d10cb6186165161c5afdf3b256fedbdd14d9c7c3`. The approved design document SHA-256 is
`9745821BAAAA742DFA634B8A2FC3563B633E7BFC7B7B06D84F1EAE85C2EC80CB`.

The candidate is the commit containing this handoff. The final report must provide its exact
commit, tree, parent, and this document's SHA-256. The complete candidate requires a new independent
implementation review.

This tranche is private, unregistered Rust infrastructure only. It does not expose wallet custody,
signing, submission, or transaction authority to Tauri or React.

## Implemented boundary

`WalletTransactionCommandBoundary` adds exactly five command-shaped private methods:

- `wallet_prepare_transfer_preview`;
- `wallet_cancel_transfer_preview`;
- `wallet_confirm_and_submit_transfer`;
- `wallet_list_activity`; and
- `wallet_refresh_transaction_observation`.

They reuse the reviewed complete `WalletInvokeRequest`, `WalletExposureAuthority`,
`MainWalletWindowAuthority`, duplicate-key policy, fail-closed guard, and fixed-response machinery.
There is no second exposure authority or state store. One private linear mutex prevents concurrent
transaction-boundary operations and closes the check-to-operation race around durable
reconciliation discovery.

Every envelope requires the exact command name and exact top-level object. Raw bodies, non-objects,
unknown or mismatched commands, extra or wrong-case fields, secret-like fields, non-ASCII or
oversized handles, and noncanonical transaction identifiers fail closed. Protocol-sized integers
are projected as canonical decimal strings. Preview responses omit the unsigned transaction,
canonical argument bytes, Core fingerprint, process identity, signature, and signed body.

## Native confirmation, signing, and one-attempt submission

The confirm-and-submit method consumes the preview through the reviewed Core-generation-validating
path, constructs the reviewed Rust-owned native confirmation window from the already validated main
HWND, and uses the module-private non-forgeable approval constructor. Native confirmation promotes
directly into the reviewed signing and one-attempt submission coordinator. Signed bytes and seed
authority never enter the boundary response.

Private submission outcomes now retain the public transaction identifier and preserve four distinct
states: accepted, accepted-recording-pending, outcome-unknown, and reviewed definitive rejection.
An authority failure before the write is no longer misclassified as ambiguity. Once exact Core
acceptance is proven, journal or reconciliation completion failure returns
`accepted_recording_pending`; it never degrades known acceptance into `outcome_unknown`.

## Durable discovery and spending interlock

`wallet_list_activity` uses one matching-wallet reconciliation permit to authenticate and inspect
the canonical reconciliation store and activity journal. It:

- safely resolves a prepared-but-not-attempted record;
- preserves `MayHaveBeenSubmitted` as `outcome_unknown` when Core is unavailable or exact lookup
  does not prove acceptance;
- may use only the reviewed read-only exact lookup when a supported peer-proven Core authority is
  available;
- completes `AcceptedRecordingPending` through journal-only authority without Core access;
- preserves `accepted_recording_pending` when journal recording remains unavailable;
- returns at most 100 authenticated records, newest first, with `history_complete: false`; and
- blocks both preview creation and confirm-and-submit while any authenticated nonterminal record
  remains.

Repeated activity queries do not clear ambiguity or duplicate an accepted journal record. Frontend
reload state is not an authority input; pending state is re-derived from authenticated Rust storage
on every query.

## Deliberate refresh blocker

The current authenticated journal schema stores public activity and observations but not the exact
signed envelope required by the reviewed receipt boundary. Therefore
`wallet_refresh_transaction_observation` validates an exact, locally authenticated transaction
identifier and then returns the fixed `wallet_transaction_unknown` error. It does not perform an
identifier-only Core lookup or infer acceptance from nonce movement.

This is an intentional activation blocker, not a successful refresh implementation. Atomic wallet
activation remains prohibited until a separately reviewed storage extension retains or reconstructs
an authenticated exact-envelope expectation.

## Adversarial evidence

The new focused tests cover:

- all five exact envelopes;
- raw, non-object, unknown, mismatched, wrong-case, extra, secret-like, malformed, noncanonical, and
  oversized inputs;
- fixed non-emitting error codes and secret canaries;
- decimal-string projection at `u64` and `u128` limits;
- exclusion of private preview authority;
- newest-first 100-record activity bounding and explicit incompleteness;
- distinct accepted-recording-pending and outcome-unknown projections;
- repeated reload-style ambiguity discovery;
- the Rust spending interlock before any Core access;
- journal-only accepted-record completion without Core and without duplication;
- journal failure preserving known acceptance rather than ambiguity; and
- fail-closed refresh when exact signed-envelope storage is unavailable.

The existing submission matrix was extended to prove that proven Core acceptance plus journal
failure produces durable `accepted_recording_pending` and exactly one network write.

## Validation

- Rust formatting: passed.
- Strict Clippy, all targets, warnings denied: passed.
- Rust tests: 305 passed, 0 failed, 4 operator-only ignored.
- Tauri authority tests: 7 passed.
- WebView isolation tests: 2 passed.
- Frontend typecheck: passed.
- Frontend state tests: passed.
- Production frontend build: passed.
- Git whitespace validation: passed.

## Unchanged prohibited surfaces

- no `#[tauri::command]` or production invoke registration;
- no `AppManifest`, permission, capability, managed-state, frontend, or dependency change;
- no supported production wallet Core manifest entry;
- no recovery export or Vision-Core change;
- `duplicate_key_rejection_proven` remains `false`; and
- lifecycle, signing, and submission security approval constants remain `false`.

Independent review must not interpret this private implementation as permission to expose or
activate any wallet command.
