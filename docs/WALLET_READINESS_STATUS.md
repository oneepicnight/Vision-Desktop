# Wallet Readiness Status

Status date: 2026-08-14

This document is the current operational summary. Reviewed design documents, implementation
handoffs, qualification transcripts, and historical audits retain their original commit-specific
meaning and are not rewritten by this summary.

## Desktop baseline

The reviewed private wallet foundation was merged to Vision Desktop `main` in merge commit
`6b6f64291c43540ecd19190607b7813238b55d2e` with tree
`92389526612565e23a92ce624f8830138f53ae32`.

The merged foundation includes private Rust implementations for:

- Windows-scoped custody and lifecycle authority;
- encrypted local vault and portable recovery storage;
- native recovery and transaction-confirmation ceremonies;
- Core peer-bound read-only account and status access;
- preview, signing, submission, reconciliation, and receipt observation;
- authenticated activity, reconciliation, and exact-envelope storage;
- private lifecycle and transaction command-boundary adapters;
- generated-wrapper and packaged WebView transport qualification evidence.

The merged test baseline was 346 Rust tests passed, zero failed, and four operator-only tests
ignored. Tauri ACL, WebView isolation, frontend typechecking, frontend state tests, the production
frontend build, Rust formatting, and strict Clippy also passed before merge.

## Production authority remains closed

The implementation is a private foundation, not an active wallet release:

- no wallet Tauri command is registered;
- no wallet permission, capability, or AppManifest entry exists;
- React has no custody, password, recovery, signing, or submission invoke path;
- lifecycle, signing, and submission independent-review constants remain `false`;
- production `duplicate_key_rejection_proven` remains `false`;
- the admitted Core can create only the private generation-bound Core connection authority; it does
  not create wallet lifecycle, signing, submission, command, or frontend authority;
- wallet activation, publication, sending, and recovery export remain prohibited.

## Core dependency state

The earlier Windows artifact with SHA-256
`586a04b311da41adcf5e41ffb390f5d99c6f3732c43083c24602f373bc3721c1` remains ineligible and must
not be reused.

A later independently accepted Windows artifact is frozen with candidate SHA-256
`8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28` and accepted Core
evidence/release manifest SHA-256
`35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a`. The original evidence
correctly retains `vision_desktop_integration_authorized: false`; it was not edited or relabeled.
A separate explicit owner-authorization record with SHA-256
`2576e87f46dd7cd878a5aa39daebc11e027d23bbeeeca54e5db6110cec9e3449` binds the exact candidate,
source commit/tree, evidence manifest, runtime manifest, allowed admission/validation scope, and
continued wallet-disable conditions.

The isolated Desktop artifact-admission implementation now exists on
`feat/frozen-core-artifact-admission`. It stages the ignored executable only after both immutable
records and the complete evidence inventory validate; pins the runtime manifest; retains guarded
resource handles; verifies running-image identity; rejects IPv4/IPv6 exposure; contains the child in
a kill-on-close Windows Job Object; and invalidates Core authority across stop/restart. This
implementation remains pending independent corrective acceptance and is not wallet activation.

## Next hard gate

The next step is independent re-review of the exact corrective artifact-admission commit and its
validation evidence. Desktop must continue to execute `CORE_ARTIFACT_INTAKE_CHECKLIST.md` without
rebuilding, signing, substituting, or otherwise changing the artifact.

Only after artifact admission receives independent acceptance may a separate atomic exposure change:

1. prepare the complete unpublished atomic twelve-command candidate without partial exposure;
2. independently review its commands, permissions, capabilities, frontend, and activation policy;
3. qualify the exact unchanged packaged artifact on the supported Windows matrix;
4. accept the resulting evidence before any publication decision.

No step automatically enables the wallet. Every authority change remains a separately reviewed,
all-or-nothing release gate.
