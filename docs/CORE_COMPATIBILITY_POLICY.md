# Core Compatibility Policy

Vision Desktop must treat Vision Core as an external consensus engine.

## Current Supported Core

- Core alpha tag: `vision-core-alpha-rc2`
- Consensus tag: `vision-core-consensus-v1.0.3`
- Commit: `6a065df8206b50874029a27ee2b54dffae5e3cdd`
- Binary SHA-256: `41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01`
- Consensus version: `3`
- P2P protocol version: `4`

This manifest is intentionally unchanged. It is a development compatibility baseline whose real
launch is blocked because the frozen binary cannot bind its administrative API to loopback only.

## Pending Wallet-Compatible Core

Vision-Core source commit `223e2f745ebb5f7eb0d48c88397684b9037767bc` contains reviewed
loopback-binding and wallet API source changes. It is not a supported Desktop artifact yet. The
previous Windows build from that source was not accepted by the downstream qualification gate, so
its hash must not be copied into the Desktop manifest and its executable must not replace the
bundled RC2 binary.

Promotion requires the exact procedure in `CORE_ARTIFACT_INTAKE_CHECKLIST.md` and a separately
reviewed Desktop integration commit.

## Rules

- Desktop verifies Core binary hash before launch.
- Desktop binds Core API to loopback only.
- Desktop never decides whether a block is valid.
- Desktop never modifies Core databases directly.
- Desktop never bypasses Core validation.
- Desktop does not ship private keys in Core config.
- Desktop must show compatibility warnings for consensus-breaking Core updates.

## Long-Term Artifact Strategy

For local development, the RC2 binary may exist in `bundled/core/windows-x64` for testing. Before a public repository push or release, the project should decide whether Core binaries are stored with Git LFS, attached to releases, or downloaded by a signed build step.

Preferred long-term strategy: store only manifests in Git and download signed Core artifacts during release packaging. This keeps repository history small and makes binary provenance explicit.
