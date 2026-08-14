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

The accepted package was copied without rebuilding or changing its contents. The complete inventory,
every file size, and every file hash were verified before staging. The former local RC2 executable
was preserved outside the repository under the Desktop evidence directory. The admitted executable
is ignored and is not part of the Git diff.

## Implemented admission boundary

The runtime manifest now uses a recursively strict schema. Unknown members, duplicate members,
wrong schema or platform values, altered semantic values, and valid-but-unapproved whole-document
bytes fail closed.

On Windows, Desktop opens the manifest and executable through retained non-reparse handles. It
rejects relative, UNC, device, non-fixed-volume, reparse, non-regular, and multi-hard-link inputs;
denies write, rename, and delete sharing; records volume/file identity; performs bounded handle reads;
and revalidates identity before and after use.

The supervisor launches only the guarded executable and retains both guards for the process lifetime.
It compares the running process image with the admitted executable identity, requires exactly one
literal `127.0.0.1` listener owned by the exact child PID, and binds Wallet Core authority to the
process handle, creation identity, supervisor generation, manifest fingerprint, and both admitted
file identities. Stop and restart invalidate older authority.

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

## Deliberately unchanged blockers

- No wallet Tauri command is registered.
- No wallet permission, capability, AppManifest entry, or frontend invoke exists.
- All three independent-review approval constants remain `false`.
- `duplicate_key_rejection_proven` remains `false`.
- Wallet lifecycle, signing, submission, sending, recovery export, and publication remain disabled.
- No dependency or Vision-Core source change is included.
- The artifact-admission implementation requires independent review before any activation tranche.
