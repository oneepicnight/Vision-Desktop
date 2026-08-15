# Wallet Readiness Status

Status date: 2026-08-15

This is the operational summary for the unpublished atomic wallet-exposure candidate. Historical
designs, reviews, handoffs, and qualification evidence retain their original commit-specific
meaning.

## Accepted foundations

The private Rust wallet foundation on `main` provides Windows-scoped custody and lifecycle
authority, encrypted vault and portable recovery storage, Rust-owned recovery and transaction
ceremonies, peer-bound Core access, preview/sign/submit/reconcile flows, and authenticated activity,
reconciliation, and exact-envelope stores.

The frozen Vision Core artifact is admitted only by its exact executable, runtime-manifest,
acceptance-record, and evidence identities. Controlled Desktop compatibility evidence for that
artifact has been independently accepted. The admitted process is launched inside a kill-on-close
Windows Job Object and wallet Core authority remains bound to the exact supervised process,
generation, running image, literal loopback listener, and socket owner.

## Unpublished atomic candidate

Branch `feat/atomic-wallet-exposure` prepares the complete twelve-command candidate as one review
unit:

- seven lifecycle commands: status, recovery destination, create, recovery source, restore, unlock,
  and lock;
- five transaction commands: prepare, cancel, native confirm-and-submit, list activity, and refresh
  observation;
- one generated permission per command, granted only to the Windows `main` WebView;
- matching AppManifest and invoke-handler registration;
- typed wrappers only in `src/services/coreApi.ts`;
- feature-local public React state, cleared on lock, reload, navigation, window loss, and teardown;
- native Rust password, recovery-credential, final-confirmation, seed, signing, and submission
  authority;
- authenticated pending reconciliation discovery that blocks new spending until resolved;
- all three independently reviewed activation scopes and the accepted duplicate-key transport proof.

React receives public account data, public previews, public transaction identifiers, public activity,
and fixed error codes. It does not receive passwords, recovery credentials, selected filesystem
paths, seeds, private keys, canonical signed envelopes, or signatures. Core never receives wallet
seeds or private keys.

## Current release state

This source tree is not yet an approved or published wallet release. It must remain unpublished until:

1. the exact candidate commit receives independent source review;
2. the unchanged candidate is packaged as an ordinary production artifact;
3. the complete supported-Windows and clean-device wallet qualification matrix passes;
4. the exact artifact and evidence receive independent acceptance;
5. an explicit publication decision authorizes distribution of that unchanged artifact.

Failure or modification at any stage closes the gate. Lifecycle-only, transaction-only, or
partially permissioned exposure is prohibited. Recovery export, automatic retry, transaction
replacement, hardware-wallet support, broad dialog permissions, Core-manifest relaxation, and
Vision-Core changes are outside this candidate.

## Frozen identities

- Core executable SHA-256:
  `8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28`
- accepted Core evidence manifest SHA-256:
  `35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a`
- Desktop owner-authorization record SHA-256:
  `2576e87f46dd7cd878a5aa39daebc11e027d23bbeeeca54e5db6110cec9e3449`

The earlier artifact beginning `586a04b3` remains ineligible and must not be reused.
