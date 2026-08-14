# Core Compatibility Policy

Vision Desktop must treat Vision Core as an external consensus engine.

## Current Admitted Core

- Core release tag: `vision-core-v1.0.4`
- Consensus tag: `vision-core-consensus-v1.0.3`
- Source commit: `890c98a02c7147e166805fe52002d22d1fcd81f9`
- Source tree: `2ae583bbfc887490b8af1398aead7b916796700c`
- Binary SHA-256: `8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28`
- Binary size: `4,486,144` bytes
- Accepted evidence manifest SHA-256: `35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a`
- Consensus version: `3`
- P2P protocol version: `4`

The complete Desktop runtime-manifest bytes are independently pinned in Rust with SHA-256
`cf713d116acca7a848d0537968f81373ee14a965fb3855a6480a59f4971536ec`. The executable is an ignored
local/release input staged only from the accepted evidence package; it is not stored in Git.
Rebuilding, modifying, re-signing, repackaging, or rerunning qualification against changed bytes
invalidates the acceptance.

Admission follows `CORE_ARTIFACT_INTAKE_CHECKLIST.md` and the approved boundary in
`FROZEN_CORE_ARTIFACT_INTEGRATION_DESIGN.md`. The isolated implementation still requires independent
review before any downstream activation decision.

## Rules

- Desktop verifies Core binary hash before launch.
- Desktop binds Core API to loopback only.
- Desktop never decides whether a block is valid.
- Desktop never modifies Core databases directly.
- Desktop never bypasses Core validation.
- Desktop does not ship private keys in Core config.
- Desktop must show compatibility warnings for consensus-breaking Core updates.

## Long-Term Artifact Strategy

For controlled local development, the exact admitted executable may exist in
`bundled/core/windows-x64` after verified staging. It remains ignored and must not be force-added to
Git.

Preferred long-term strategy: store only manifests in Git and download signed Core artifacts during release packaging. This keeps repository history small and makes binary provenance explicit.
