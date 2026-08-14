# Frozen Core Artifact Admission Handoff

Date: 2026-08-14

Branch: `feat/frozen-core-artifact-admission`

Approved design baseline: `90a0d593c53ca321a4cd18b9ae909716f17f9eaf`

## Exact frozen input

- Evidence package: `C:\Vision\Evidence\Vision-Core\CORE_WALLET_COMPATIBILITY_QUALIFICATION_890C98A_V3_20260813T231415Z`
- Evidence inventory: 1,190 verified files and 348,585,838 verified bytes
- Core source commit: `890c98a02c7147e166805fe52002d22d1fcd81f9`
- Core source tree: `2ae583bbfc887490b8af1398aead7b916796700c`
- Release: Vision Core v1.0.4, Windows x86_64 MSVC
- Executable size: 4,486,144 bytes
- Executable SHA-256: `8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28`
- Accepted evidence manifest SHA-256: `35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a`
- Desktop runtime-manifest SHA-256: `cf713d116acca7a848d0537968f81373ee14a965fb3855a6480a59f4971536ec`
- Separate Desktop-integration acceptance SHA-256: `2576e87f46dd7cd878a5aa39daebc11e027d23bbeeeca54e5db6110cec9e3449`

The accepted package was copied without rebuilding or changing its contents. The complete inventory,
every file size, and every file hash were verified before staging. The former local RC2 executable
was preserved outside the repository under the Desktop evidence directory. The admitted executable
is ignored and is not part of the Git diff.

The original evidence manifest remains unchanged and continues to state that it does not itself
authorize Desktop integration. The separately pinned `integration-acceptance.json` records the
explicit owner authorization for only isolated artifact admission and controlled compatibility
validation. It explicitly acknowledges that the authenticated CI archive covers the earlier
`223e2f745ebb5f7eb0d48c88397684b9037767bc` base rather than the exact candidate and therefore keeps
the exact-candidate runtime and deterministic qualification evidence mandatory. Both staging and
runtime admission require its exact bytes and bindings.

## Implemented admission boundary

The runtime manifest now uses a recursively strict schema. Unknown members, duplicate members,
wrong schema or platform values, altered semantic values, and valid-but-unapproved whole-document
bytes fail closed.

On Windows, Desktop opens the manifest and executable through retained non-reparse handles. It
rejects relative, UNC, device, non-fixed-volume, reparse, non-regular, and multi-hard-link inputs;
denies write, rename, and delete sharing; records volume/file identity; performs bounded handle reads;
and revalidates identity before and after use.

The supervisor launches only the guarded executable and retains all three guards for the process lifetime.
It compares the running process image with the admitted executable identity, requires exactly one
literal `127.0.0.1` listener owned by the exact child PID, and binds Wallet Core authority to the
process handle, creation identity, supervisor generation, manifest fingerprint, and all three
admitted file identities. Stop and restart invalidate older authority.

The supervisor now also retains the acceptance-record guard and a Windows Job Object configured with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Assignment occurs immediately after process creation. Normal
stop preserves supervisor ownership until job termination and child-exit confirmation both succeed;
failed termination or wait leaves the owned record installed. Admission cleanup reports failure,
window teardown requests an explicit stop, and OS handle closure kills the contained process after
forced Desktop termination. IPv6 listener enumeration rejects `::`, `::1`, competing-owner, and
dual-stack listeners on the administrative port.

## Staging and controlled validation

Stage only from the exact evidence package:

```powershell
.\scripts\stage-frozen-core.ps1 `
  -EvidenceRoot C:\Vision\Evidence\Vision-Core\CORE_WALLET_COMPATIBILITY_QUALIFICATION_890C98A_V3_20260813T231415Z
```

The script refuses to replace an existing different executable. It verifies the accepted manifest,
detached checksum, complete evidence inventory, source identity, candidate identity, copied bytes,
and final staged bytes.

The ignored operator test starts the admitted Core twice with isolated temporary data and log
directories, verifies private listener ownership and Wallet Core authority, stops each exact child,
and proves that prior-generation authority fails after stop/restart:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml `
  supervisor::tests::frozen_core_launch_binds_private_api_and_invalidates_prior_generation `
  -- --ignored --exact --test-threads=1
```

The controlled test passed and no Vision Core process remained afterward.

Committed adversarial tests additionally cover directory reparse points, pre-existing write handles,
before-open path substitution, rename/delete races, post-hash write/replacement denial, wrong size and
digest revalidation, unavailable and mismatched process images, manifest/acceptance replacement,
injected job-termination/child-kill/child-wait failure, ordinary job closure, and forced owner-process
termination.

Corrective validation passed on Windows against the exact staged resources:

- full serialized Rust suite: 361 passed, 0 failed, 5 operator-only ignored;
- live exact-Core launch, private listener, restart-generation, and cleanup test: passed;
- strict Clippy and Rust formatting: passed;
- Tauri authority: 7 passed; WebView isolation: 2 passed;
- frontend typecheck, state tests, and production build: passed;
- release-mode Tauri build: passed, with the executable, runtime manifest, and acceptance record
  copied at the exact pinned sizes and SHA-256 values;
- Git whitespace validation and post-test process/listener cleanup: passed.

## Deliberately unchanged blockers

- No wallet Tauri command is registered.
- No wallet permission, capability, AppManifest entry, or frontend invoke exists.
- All three independent-review approval constants remain `false`.
- `duplicate_key_rejection_proven` remains `false`.
- Wallet lifecycle, signing, submission, sending, recovery export, and publication remain disabled.
- No dependency or Vision-Core source change is included.
- The artifact-admission implementation requires independent review before any activation tranche.
