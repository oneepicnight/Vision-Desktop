# Atomic Wallet Exposure Candidate — Implementation Handoff

Status date: 2026-08-15

## Review target

- Branch: `feat/atomic-wallet-exposure`
- Parent: `a0a5c7f6cfc2caf7a58c7e81ca850323f5ca29ab`
- Parent tree: `5a03a3f63a46a48774ea1c7ffb185dbe40fe5d81`
- Candidate commit and tree: supply from the clean committed review target; this document is not a
  substitute for independent Git identity verification
- Scope: unpublished source candidate only

The parent is the independently approved frozen-Core artifact-admission implementation. Controlled
compatibility evidence for that exact parent and frozen Core artifact was independently accepted
before this candidate began.

## Implemented atomic surface

Exactly twelve wallet commands are added together:

1. `wallet_get_status`
2. `wallet_select_recovery_destination`
3. `wallet_create`
4. `wallet_select_recovery_source`
5. `wallet_restore`
6. `wallet_unlock`
7. `wallet_lock`
8. `wallet_prepare_transfer_preview`
9. `wallet_cancel_transfer_preview`
10. `wallet_confirm_and_submit_transfer`
11. `wallet_list_activity`
12. `wallet_refresh_transaction_observation`

The same inventory is required in the generated Tauri wrappers, Windows invoke handler,
`AppManifest`, generated permissions, `main-desktop` capability, and `coreApi.ts`. The Linux mock
capability receives no wallet permission. No dialog or broad plugin permission is granted.

## Authority and custody boundary

- `src-tauri/src/wallet/exposure.rs` is the only production wallet command module.
- Each wrapper forwards the whole `WalletInvokeRequest` to the previously reviewed lifecycle or
  transaction boundary; it contains no custody, signing, submission, or response-projection logic.
- The accepted Layer B duplicate-key transport proof and all three independent security-review
  gates are enabled only as part of this complete candidate.
- The accepted Layer B evidence qualifies the raw Tauri/Wry transport behavior. The reduced
  create/restore business schema does not reinterpret that evidence; Layer A independently
  exercises the corrected generated-wrapper shapes, and no Layer B source-level qualification is
  claimed for fields that no longer cross IPC.
- The admitted Core connection remains bound to the exact supervised process, creation identity,
  generation, manifest, running image, loopback socket, and owning PID before and after operations.
- Core stop, restart, page load, window teardown, session lock, suspend, logoff, shutdown, panic,
  or runtime invalidation revokes wallet authority.
- Passwords, recovery credentials, seeds, private keys, signatures, and signed envelopes remain
  native Rust values and never enter React, the general Desktop reducer, logs, or support packages.
- Recovery paths and their bounded, generation-bound, single-use capabilities stay in Rust. React
  receives only `{ "selected": true }` after a native selection succeeds.
- Final transfer approval remains the physically qualified native Windows confirmation ceremony.
- Submission permits exactly one write attempt. Unknown results and accepted-recording-pending
  results remain durably authenticated, survive restart, block new spending, and never trigger an
  automatic retry or replacement.

## Frontend boundary

`WalletPanel` owns only public feature-local presentation state. It provides create, restore,
unlock, lock, preview, cancel, native confirm-and-submit, activity, and receipt-refresh controls.
It displays only allowlisted fixed error copy and clears public wallet presentation on lock, reload,
navigation/unmount, hidden visibility, window blur, and teardown. The shared Desktop state and
event/reducer pipeline no longer retain the legacy configured-address wallet projection.

Every native failure clears the complete public wallet presentation. A monotonic frontend epoch is
captured by each request; blur, visibility, teardown, explicit clearing, or a native failure advances
that epoch so a stale asynchronous completion cannot repopulate unlocked or spending state.

The frontend contains no password, recovery-credential, seed, private-key, signature, signed-byte,
filesystem-path, path capability, clipboard, browser-storage, direct network, or direct Tauri-core
custody path.

## Frozen compatibility identities

- Core executable SHA-256:
  `8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28`
- Core evidence manifest SHA-256:
  `35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a`
- Desktop owner-authorization record SHA-256:
  `2576e87f46dd7cd878a5aa39daebc11e027d23bbeeeca54e5db6110cec9e3449`

No Core bytes, manifest identity, compatibility behavior, dependency, or Vision-Core source is
changed by this candidate.

## Required independent review

The reviewer must verify at the exact candidate commit and tree:

- twelve-command parity and absence of any thirteenth wallet command;
- whole-invoke parsing and main-window authority on every command;
- one permission per command and no remote, dialog, shell, filesystem, HTTP, or broad grant;
- secrets remain absent from frontend types, wrappers, state, forms, errors, diagnostics, and
  support-package inputs;
- native create/restore/unlock/confirmation ceremonies remain mandatory and fail closed;
- operation exclusion, lifecycle revocation, Core-generation checks, one-write submission, durable
  reconciliation, authenticated envelope/activity storage, and receipt-reorganization behavior;
- exact admitted Core and frozen evidence identities;
- complete validation results and clean candidate worktree.

## Publication boundary

This commit is not authorization to run real funds, publish, distribute, or promote an artifact.
After independent source approval, the exact unchanged tree must be packaged and qualified on the
supported Windows matrix, including clean-device create, backup, restore, unlock, send,
reconciliation, receipt, session lifecycle, abnormal termination, and uninstall/retention cases.
The sealed evidence then requires independent acceptance and an explicit publication decision.

Any source, dependency, configuration, permission, manifest, artifact, or Core-identity change
invalidates that chain. Partial exposure is prohibited.
