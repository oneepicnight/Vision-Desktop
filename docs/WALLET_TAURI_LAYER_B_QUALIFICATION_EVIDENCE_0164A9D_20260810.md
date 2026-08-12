# Wallet Tauri Layer B Qualification Evidence — `0164a9d`

## Verdict produced by the packaged runner

**Passed**

This document records the fresh, independently authorized execution of the complete packaged
Wallet Layer B transport qualification matrix. It does not approve production wallet exposure.
Independent acceptance of this evidence remains required.

## Reviewed implementation identity

- Commit: `0164a9d327c392a36a61e941cbd497aae1d4e7d7`
- Tree: `1bfd394eb26581c15ae3b344e76c982b71df6f6a`
- Parent: `0387362a1e14250395ca49078a6782d56557b964`
- Branch: `fix/wallet-signing-adversarial-matrix`
- Implementation handoff SHA-256:
  `1E45ACABA29CF6FC227940EBB7B609324C0C316F92216FBE6C24F6F7B9D3BEEC`

The tracked worktree was clean before execution. The runner independently recorded the same commit,
tree, and clean status before and after the matrix.

## Execution authorization and isolation

Independent review reported zero High, Medium, or Low findings and authorized one fresh execution
of the complete 562-case matrix against the exact identity above.

The fresh evidence directory was absent before execution:

`C:\Vision\wallet-layer-b-evidence-0164a9d-20260810`

No qualification host was running before execution. Earlier crash-interrupted, retry, and
Inconclusive evidence directories were not reused, combined, overwritten, or reclassified.

## Primary evidence

- Evidence directory: `C:\Vision\wallet-layer-b-evidence-0164a9d-20260810`
- Manifest: `layer-b-evidence-manifest.json`
- Manifest SHA-256:
  `432C41AAEF4059F39235584B08B849DC6923811D15015A7FC36B26D161FA2D8B`
- Evidence files: 1,125
- Evidence bytes: 1,856,631
- Screenshots: none; bounded stdout and stderr transcripts are the primary evidence

The directory contains one stdout and one stderr transcript for every case plus the manifest. The
manifest records each transcript's SHA-256, process identity, launch arguments, start and end time,
exit code, timeout state, classification, terminal result, wrapper/record counts, and loaded
WebView2 version.

## Environment and timing

- Operator: `LAPTOP-SNC6K2SH\bighe`
- Operating system: `Microsoft Windows NT 10.0.26200.0`
- Architecture: `X64`
- First case start: `2026-08-10T19:29:19.9994481Z`
- Last case end: `2026-08-10T19:33:46.3878071Z`
- Matrix span: 266.388 seconds
- Per-case deadline: 60 seconds
- Loaded WebView2 runtime: `151.0.4129.72`
- Loaded WebView2 runtime proven: `true`

## Matrix result

- Total: 562
- Passed: 562
- Failed: 0
- Inconclusive: 0
- Timed out: 0
- Nonzero process exits: 0
- Non-passing terminal records: 0

Classification reasons:

- `complete_matching_terminal_evidence`: 555
- `verified_native_target_destruction`: 7

All seven generated command shapes passed every applicable transport, malformed-envelope,
duplicate-key, window/origin/generation, concurrency, ordering, panic-containment, revocation, and
post-revocation case selected by the reviewed runner.

## Lifecycle correction evidence

Every one of the seven native destruction records independently reported:

- `route: custom_protocol_proven`
- `wrapper_ran: true`
- `command_body_ran: true`
- exact authorized HWND: `true`
- exact target window: `true`
- destroy call succeeded: `true`
- Tauri target window absent: `true`
- native HWND absent: `true`
- authority revoked: `true`
- structural post-revocation proof: `true`
- result: `passed`

All seven recreated-main cases produced complete matching terminal evidence. No recreation case
emitted the former `layer_b_window_replacement_failed` marker or lost its primary/terminal records.

## Binary, dependency, and source identity

- Qualification binary SHA-256 before and after:
  `C6D6087431BA1C765F93795044D4C02AE814D038F13CF8B52B172385B7460455`
- Production executable SHA-256 before and after:
  `FEB7691A6B61E64C487E530DAF058C8667FB8C9DA1227966DA7CC3D167DB8F3A`
- `Cargo.lock` SHA-256:
  `0401EAFBA6B662E7452A0304AE0F019E94DFE2B40A647D3C26BC07DD403BFB8B`

The manifest contains resolved versions, Cargo checksums, manifests, and inspected-source hashes for
Tauri `2.11.5`, Tauri macros `2.6.3`, Tauri Runtime Wry `2.11.4`, Wry `0.55.1`, Serde `1.0.229`,
and Serde JSON `1.0.151`. It also contains 14 mandatory harness source/configuration hashes.

## Integrity result

The manifest reports `integrity_preserved: true`.

Before/after records match for:

- the reviewed Vision Desktop repository identity and clean status;
- the qualification binary;
- the installed production executable;
- the production installation tree;
- production application data;
- wallet custody absence; and
- the pre-existing Vision-Core repository state.

Post-run verification also confirmed:

- no qualification process remains;
- the wallet custody root remains absent;
- the Vision Desktop tracked worktree remains clean at the reviewed implementation commit;
- Vision-Core retains the same preflight status; and
- no product command, permission, capability, frontend authority, or approval flag changed.

## Security gates remain closed

This Passed matrix does not itself activate custody. At the completion of execution:

- `duplicate_key_rejection_proven` remains `false` in the production policy;
- independent lifecycle security approval remains `false`;
- independent signing security approval remains `false`;
- independent submission security approval remains `false`;
- no wallet Tauri command is registered;
- no wallet permission or capability is granted;
- no frontend custody invocation exists; and
- Vision-Core was not modified.

## Required next action

Submit the exact implementation identity, this documentation-only evidence commit, the manifest
SHA-256, and the preserved primary evidence directory for independent acceptance review. Do not set
`duplicate_key_rejection_proven` or any security approval flag until a separate reviewed atomic
exposure tranche explicitly authorizes it.
