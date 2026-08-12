# Wallet Authenticated Envelope Storage Correction Handoff

Date: 2026-08-11

Branch: `fix/wallet-signing-adversarial-matrix`

Reviewed implementation commit: `77154b4a6e2d71464277c6f16297ea40e5fc6efd`

Reviewed implementation tree: `6e41593b45b7cfeac65c2c9f4aa38d265d260da9`

Approved design commit: `4a3dcb57a1e7a7b3ca6f242a77b3bf25acd26daa`

## Verdict requested

Independent implementation re-review is requested for this isolated correction to the private,
unregistered authenticated-envelope storage tranche. The prior review reported zero High, three
Medium, and one Low findings. This handoff does not request receipt-refresh implementation,
command registration, wallet activation, frontend integration, signing or submission exposure, or
publication.

## Finding disposition

### M-01 — Collision and storage-health checks

- An absent canonical envelope container no longer bypasses storage-directory health validation.
  Orphan staging state, a noncanonical directory, or invalid directory protection fails closed.
- Preview collision checks authenticate both the encrypted envelope store and journal v3, then
  reject a transaction identifier present in either store.
- The combined envelope/journal collision check is repeated immediately before envelope
  publication under the submission permit.
- A collision discovered at that final boundary prevents envelope publication and Core write.
- Regression coverage includes staging-only state with no container, a journal-only accepted
  transaction, and a collision introduced after preview but before publication.

### M-02 — Secret-derived serialization buffers

- Exact signed transactions and complete plaintext envelope containers serialize directly into
  `Zeroizing<Vec<u8>>` through `serde_json::to_writer`.
- The success, error, and unwind paths therefore retain zeroizing ownership of serializer output.
- Subprocess-isolated canary tests cover serializer panic and poisoned-lock failures without
  contaminating the parent test process.
- Source-level privacy assertions prohibit reintroducing ordinary `serde_json::to_vec` buffers for
  retained signed transactions.

### M-03 — Adversarial and interruption matrix

- Deterministic checkpoints cover publication before head transition, after transition
  publication, after container publication, and after head commit.
- First and subsequent publications recover only an exact authenticated old or new state; a
  partial container is never accepted.
- Independent mutation matrices cover wrapper/head metadata, immutable entry fields, authenticated
  plaintext container metadata, reconciliation binding fields, and journal-v3 transaction and
  commitment associations.
- Malformed duplicate and unknown fields, excessive entry counts, oversized ciphertext metadata,
  oversized exact bodies, staging files, reparse/ACL failures, wrong keys, lock contention, and
  lifecycle/reconciliation mismatches fail closed.
- Existing accepted-recording tests now inject journal failure after the final pre-write health
  checks, preserving coverage for proven Core acceptance followed by recording failure.

### L-01 — Refresh comment

The private refresh boundary comment now accurately states that journal v3 and encrypted envelope
storage exist while the separately reviewed read-only refresh coordinator remains unimplemented
and unapproved.

## Validation

The final candidate passed:

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`: 330 passed,
  0 failed, and 4 operator-only tests ignored.
- Tauri authority tests: 7 passed.
- WebView isolation tests: 2 passed.
- `npm run typecheck`
- `npm run test:state`
- `npm run build`
- `git diff --check`

An initial full run exposed two outdated test setups that sabotaged journal health before the new
pre-publication check. Those test-only failure injections were moved to the simulated post-write
stage. Both focused regressions and the complete final suite then passed.

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

This exact corrective commit and tree require independent security re-review. The private read-only
refresh coordinator remains blocked until the storage implementation and these corrections are
explicitly approved. Exposure, activation, registration, signing/submission authority, and
publication remain prohibited.
