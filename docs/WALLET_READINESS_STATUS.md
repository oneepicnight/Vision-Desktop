# Wallet Readiness Status

Status date: 2026-08-13

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
- the current bundled Core manifest cannot create production wallet authority;
- wallet activation, publication, sending, and recovery export remain prohibited.

## Core dependency state

Vision-Core `main` contains wallet compatibility source commit
`223e2f745ebb5f7eb0d48c88397684b9037767bc`. The source work includes loopback-only API support
and the reviewed wallet route contract.

The previously built Windows artifact from that source reported SHA-256
`586a04b311da41adcf5e41ffb390f5d99c6f3732c43083c24602f373bc3721c1`, but it is not an accepted
Desktop input. Downstream review found the qualification inconclusive because deterministic source
coverage did not prove the complete pending/mined/reorganization/observation-loss lookup lifecycle
and the associated no-retry invariants. A new reviewed Core source commit, fresh locked artifact,
and fresh independent evidence are required.

## Next hard gate

Vision Desktop must wait for a final Core artifact verdict that explicitly authorizes downstream
Desktop integration. When it arrives, Desktop must execute `CORE_ARTIFACT_INTAKE_CHECKLIST.md`
without rebuilding or substituting the artifact.

Only after artifact intake and independent acceptance may a separate Desktop integration change:

1. update the compatibility manifest and verified binary identity;
2. prove production loopback and peer/process-generation binding;
3. rerun the full Windows application and wallet security suites;
4. prepare the complete unpublished atomic twelve-command candidate;
5. qualify that exact frozen candidate before any publication decision.

No step automatically enables the wallet. Every authority change remains a separately reviewed,
all-or-nothing release gate.
