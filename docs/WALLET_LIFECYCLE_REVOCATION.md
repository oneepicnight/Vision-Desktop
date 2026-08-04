# Wallet Lifecycle Revocation

## Status

Vision Desktop's private Rust wallet lifecycle now treats lock and operating-system lifecycle
invalidation as an atomic authority revocation event. No wallet command or permission is registered,
React cannot reach this implementation, and this correction does not activate custody.

## Authority model

Each wallet operation captures the runtime's current monotonically increasing revocation epoch.
Invalidation performs these steps in order:

1. Increment an atomic pending-revocation counter.
2. Advance the revocation epoch before waiting for the runtime mutex.
3. Lock and clear the secret session, active operation, pending native selection, and recovery path
   authorization.
4. Decrement the counter after invalidation is complete. Overlapping invalidations keep it nonzero
   until all have completed.

New authority is refused while any invalidation is pending. An old permit fails as soon as its captured
epoch differs from the runtime epoch, including when invalidation is queued behind a long-running
operation that currently holds the runtime mutex.

## Sensitive stages

Lifecycle adapters no longer receive a reusable activation-proof reference. They invoke sensitive
work through `run_authorized`, which validates the operation and epoch before the stage and again
before its return value can leave the runtime-controlled boundary. Create, restore, and unlock use
this boundary for cryptographic preparation, recovery storage and verification, vault loading and
publication, session unlock, and public metadata acceptance.

Successful lifecycle completion uses a separate `complete` operation that rechecks the pending
latch, epoch, and active-operation identity while holding the runtime mutex, then consumes the
operation slot. A revocation already requested cannot produce a successful lifecycle result.

## Side-effect contract

Revocation is fail-closed for authority and results; it is not an unsafe attempt to interrupt an
arbitrary Windows call or cryptographic function midway. If a create-new write or atomic publication
has already completed, its encrypted output may remain. The associated credential, decrypted value,
metadata result, unlocked-status result, and success response are suppressed after revocation. A
later operation must inspect and reconcile storage through the normal validated lifecycle.

## Windows events

The existing Rust-owned native listener calls the same invalidation path for workstation/session
lock, suspend or standby, logoff, shutdown, and native teardown. Main-window close, destruction,
page load or reload, explicit lock, runtime drop, and synchronization failure also revoke authority.
Unlock and resume never restore a session.

## Automated evidence

The Rust suite includes true concurrent tests in addition to deterministic checkpoints. Tests prove:

- invalidation advances the epoch before a held runtime mutex is released;
- the pending counter is nonzero during queued invalidation;
- new operation and recovery-selection authority is refused while revocation is pending;
- a sensitive stage may finish internally but its result cannot escape after revocation;
- final lifecycle completion cannot return success after revocation;
- stale permits cannot clear or complete newer work;
- mutex poisoning clears all authority and fails closed.

Static Tauri authority tests require the epoch, pending counter, runtime-controlled stage closure,
three final lifecycle completion checks, and absence of a production activation-proof accessor.

## Remaining qualification

Before command registration, independent re-review must confirm this correction and real Windows
qualification must cover session lock, suspend/hibernate, fast-user switching, RDP, process kill,
shutdown, and interruption during filesystem publication. Power loss and process termination may
leave completed encrypted files or protected staging artifacts; they must never expose plaintext or
restore authority automatically.
