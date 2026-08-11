# Wallet Authenticated Envelope Storage Second Correction Handoff

Date: 2026-08-11

Branch: `fix/wallet-signing-adversarial-matrix`

Reviewed corrective commit: `86b8dbf353c03a69697b560d774f88288ed676fd`

Reviewed corrective tree: `565c8dadb0889bd46a59d4877a1305412b252b1e`

Reviewed corrective handoff SHA-256:
`2DA12EF6D3AC09632C40E174B96ACDED535549B95407876724C2EEB375D8CA81`

Approved design commit: `4a3dcb57a1e7a7b3ca6f242a77b3bf25acd26daa`

## Verdict requested

Independent implementation re-review is requested for this second isolated correction to the
private, unregistered authenticated-envelope storage tranche. The review of the preceding
corrective commit reported zero High, two Medium, and zero Low findings. This handoff does not
request receipt-refresh implementation, command registration, wallet activation, frontend
integration, signing or submission exposure, or publication.

## Finding disposition

### M-01 — Directory enumeration now fails closed

- Directory entries are consumed one at a time as `io::Result<OsString>` values.
- Any individual enumeration error immediately returns fixed `StorageUnavailable`.
- No `filter_map(Result::ok)` or other error-discarding path remains.
- A deterministic injected `PermissionDenied` regression proves that a successful entry followed
  by an enumeration failure cannot be classified as a healthy directory.
- Existing staging-file detection and canonical-directory, ACL, and reparse checks remain intact.

### M-03 — Capacity is enforced before preview and persistence interruption is granular

- The authenticated store now exposes one purpose-specific acceptance check that verifies storage
  health, authenticates existing state, rejects transaction-identifier collisions, and rejects a
  full store.
- Preview invokes this check before installing a preview handle, native confirmation, seed access,
  signing, envelope publication, or Core write.
- Submission repeats the same capacity/collision check immediately before envelope publication;
  the publication method independently enforces the limit again.
- Production capacity remains fixed at 10,000 entries. A `cfg(test)`-only custody constructor can
  reduce the limit so the real preview path can prove full-store rejection without creating 10,000
  signed test transactions. That test-only field and constructor do not exist in production code.
- Atomic persistence now exposes independent checkpoints after staging write, staging flush,
  handle-bound publication, and handle-bound read-back verification.
- Read-back uses a cloned view of the exact validated publication handle, not a pathname reopen.
  It verifies file protection, exact length, bounds, and byte equality.
- The granular checkpoints are wired through transition-head, encrypted-container, and committed-
  head persistence rather than surrounding only the complete persistence calls.
- Twelve target/phase combinations are exercised for both first and subsequent publication. The
  24 cases prove that staging interruptions block reads and that cleanup/restart recovers only a
  complete authenticated old or new state.

## Validation

The final candidate passed:

- Focused encrypted-envelope storage suite: 20 passed, 0 failed.
- Real preview full-store regression: passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`: 334 passed,
  0 failed, and 4 operator-only tests ignored.
- Tauri authority tests: 7 passed.
- WebView isolation tests: 2 passed.
- `npm run typecheck`
- `npm run test:state`
- `npm run build`
- `git diff --check`

## Authority surface remains closed

- No wallet Tauri command or invoke registration was added.
- No AppManifest entry, permission, capability, frontend invoke, form, or custody state was added.
- Lifecycle, signing, and submission approval constants remain `false`.
- Production duplicate-key proof remains `false`.
- Receipt refresh remains fail-closed and unregistered.
- No production Core authority or compatibility-manifest support was added.
- No dependency, manifest, or lockfile changed.
- Vision-Core was not modified.

## Required next decision

This exact second corrective commit and tree require independent security re-review. The private
read-only refresh coordinator remains blocked until the storage implementation and both corrective
tranches are explicitly approved. Exposure, activation, registration, signing/submission
authority, and publication remain prohibited.
