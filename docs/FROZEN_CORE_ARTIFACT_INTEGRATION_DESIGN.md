# Frozen Vision-Core Artifact Integration Design

Status date: 2026-08-14

This document defines the next Vision Desktop integration boundary for the independently accepted
Vision-Core Windows artifact. It is a design only. It does not copy or execute the artifact, change
the Desktop compatibility manifest, remove the current launch block, enable wallet authority,
register a command, or authorize distribution.

## Frozen acceptance identity

The acceptance record supplied to the Desktop workflow freezes these values:

- candidate executable SHA-256:
  `8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28`;
- accepted Core evidence/release manifest SHA-256:
  `35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a`.

These are two distinct trust objects. The accepted Core evidence/release manifest is not assumed to
be the same file as Desktop's runtime manifest at
`bundled/core/windows-x64/manifest.json`. The runtime manifest is a Desktop-owned compatibility
record whose exact bytes form part of `CoreConnectionAuthority`.

The accepted executable and evidence manifest are not present in the current Desktop checkout and
were not locally rehashed while preparing this design. Before implementation, the immutable
acceptance package must also supply and prove the source commit, tree, artifact filename and byte
size, platform, build identity, evidence inventory, and explicit downstream-integration verdict.
Those values must be read from the accepted package; they must not be inferred from a mutable branch
tip or reconstructed from the executable hash.

Any rebuild, relink, signing operation, resource edit, compression that changes the admitted file,
patch, reconfiguration, or replacement creates a different artifact and invalidates this design's
acceptance identity. Qualification must not be rerun merely to reproduce the frozen artifact.

## Objective

Admit exactly the frozen artifact as a supported private-loopback Core candidate while preserving
all current Desktop wallet and release blockers. The resulting integration may make a supervised
Core launch and private `CoreConnectionAuthority` construction possible for controlled validation.
It must not make the wallet user-accessible.

The integration is complete only when Desktop can prove all of the following for the same process
generation:

1. the packaged executable bytes match the frozen candidate hash;
2. the Desktop runtime manifest matches the exact independently reviewed compatibility record;
3. Desktop launched the process whose held Windows process identity it records;
4. that process owns the connected literal `127.0.0.1` TCP endpoint;
5. the versioned wallet response and transaction contracts match the reviewed release;
6. stop, exit, crash, or restart invalidates every authority from the prior generation; and
7. no wallet exposure or signing/submission activation changed as a side effect.

## Trust objects and identity chain

The implementation must keep the following identities separate and bind them explicitly:

### 1. Accepted Core evidence identity

The immutable external evidence package contains the frozen executable, its accepted manifest, the
complete evidence inventory, and the independent verdict. Intake verifies the accepted manifest
hash first, then verifies every inventory entry, then verifies the executable hash and byte size.

The external evidence package remains outside the repository and outside mutable runtime data. It
is never rewritten by Desktop packaging.

### 2. Desktop runtime compatibility identity

Desktop retains a small, reviewed JSON runtime manifest under source control. The complete document,
not merely its wallet subsection, is one security object. It must contain a required top-level
integer `schema_version` with one exact accepted value. A whole-document parser must reject unknown
fields and duplicate member names at the root and recursively in every nested object before typed
construction. Applying `deny_unknown_fields` only to a nested compatibility structure is
insufficient. The manifest must identify, at minimum:

- the exact accepted Core source commit and release identity from the evidence package;
- the exact frozen executable SHA-256 and Windows x86-64 platform;
- the already reviewed consensus and P2P protocol versions;
- wallet contract `vision-wallet-read-v1`;
- bind host `127.0.0.1`;
- peer binding `windows_tcp_owner_pid_v1`;
- status version `3`;
- canonical lowercase 64-hex identifier rules;
- exact balance, nonce, status, transaction lookup, and submission route contract;
- tip raw value `0`, charged base raw value `1`, and fee limit raw value `201`;
- the independently accepted duplicate/rejection semantics, including an explicit definitive
  non-mutating rejection allowlist; and
- the accepted Core evidence-manifest SHA-256 as provenance, not as a substitute for checking the
  executable.

The runtime manifest must not include a URL, DNS host, arbitrary path, proxy, credential, retry
policy, mutable channel, or wildcard compatibility range. Its exact reviewed bytes have a SHA-256
pinned outside the mutable manifest in an ordinary reviewed Rust constant. The expected value must
not come from the manifest itself, an environment variable, runtime configuration, an adjacent
file, or an update channel. Before launch and before authority construction, Desktop hashes the
complete raw document and requires equality with that independently reviewed constant. The pinned
expected digest, verified actual digest, schema version, and manifest file identity are retained in
the owned Core generation and every connection authority.

The manifest is opened through the same non-reparse, share-restricted, handle-bound file policy as
the executable. Parsing and hashing use bytes read from that held handle, not a path reopened after
validation. Authority validation rewinds and rehashes the held manifest handle, compares it again
with the external constant, and confirms the same volume/file identity. Any inability to repeat the
comparison invalidates the generation. A different manifest that is syntactically and semantically
valid but whose complete bytes were not independently pinned must fail closed.

If the accepted evidence package does not prove one of these fields, implementation stops. It must
not fill the gap from documentation memory, the current Core `main` branch, or a nearby build.

### 3. Packaged executable identity

The executable is not committed to ordinary Git history. Release assembly obtains the exact frozen
file from an immutable, access-controlled artifact location using an explicit full path. Before and
after the staging copy, it verifies byte size and the lowercase SHA-256 above. Publication tooling
must refuse an existing destination unless its bytes already match exactly; it must never rebuild,
sign, patch, or silently replace the file.

Tauri packaging may consume only the verified staging copy at
`bundled/core/windows-x64/vision-core.exe`. That ignored path is an assembly input, not the source of
truth. A missing file, changed file, inaccessible evidence manifest, or hash mismatch fails before
the application build.

At runtime, verification is handle-bound rather than path-bound. Desktop opens and retains every
ancestor directory needed to resolve the packaged executable under the trusted resource root. It
rejects an ancestor or final component that is a reparse point and rejects UNC, device, verbatim,
remote, removable, or path-escaping forms. It opens the final executable with a Windows handle that
permits reading but does not share write, delete, or rename authority. A pre-existing conflicting
writer makes admission fail. The handle must identify a normal file on the expected fixed volume,
with one link only; a hard-linked candidate is rejected even when the content hash matches.

File size, SHA-256, volume identity, and 128-bit file identifier are obtained through that same held
handle. The handle and guarded directory chain remain alive across process creation and for the
complete owned Core process generation, preventing path replacement after verification.

### 4. Live Core process identity

After handle-bound runtime verification, the supervisor launches only the guarded packaged path
while the verified executable handle and guarded directory chain remain held. After process
creation, Desktop obtains the running process image path from the held process handle, opens that
resolved image using the same non-reparse and share-restricted policy, and compares its volume and
file identifier with the verified executable handle. Path-string equality, PID equality, and a
later path hash are not substitutes for file-identity equality. If Windows cannot prove that the
running image resolves to the verified file identity, the process is terminated as the exact owned
child and the generation fails closed before any API use.

The supervisor records the verified executable handle and identity, guarded directory chain,
manifest handle and identity, pinned manifest digest, monotonic generation, PID, held process
handle, process creation identity, and literal loopback port. Connection authority remains valid
only while all values continue to match. The executable and manifest handles remain non-cloneable,
non-serializable, and private to the supervisor/authority boundary.

## Permitted implementation scope after design approval

The first implementation commit may change only the narrow Core admission boundary:

- `bundled/core/windows-x64/manifest.json`;
- `src-tauri/src/core_manifest.rs`, a narrowly scoped handle-bound resource-verification module if
  separation is required, and their exact-schema/hash/file-identity tests;
- the existing supervisor launch gate and focused executable/process/generation tests;
- packaging-time artifact retrieval or verification metadata and scripts;
- Core compatibility, API, security, readiness, and release documentation.

An executable may be copied into the ignored local staging path for controlled validation, but it
must not be committed. The implementation commit must remain reviewable without relying on an
untracked executable: the external evidence identity, retrieval rule, expected byte size, and hash
must be explicit.

The implementation may replace the unconditional frozen-RC2 launch refusal only with an exact
policy decision that succeeds for the admitted manifest and artifact. A generic removal of the
guard is prohibited.

## Launch and loopback rules

The supervisor must launch the accepted release with its reviewed loopback-bind mechanism and set
the bind address to literal `127.0.0.1`. The exact environment variable or command-line contract
must come from the accepted Core manifest and source evidence. Desktop must not guess it.

Before reporting the process usable, Desktop must prove:

- the binary and runtime manifest were verified through held non-reparse handles before process
  creation;
- the exact running process image file identity equals the verified executable handle identity;
- the executable and manifest cannot be written, renamed, deleted, or substituted while the owned
  generation exists;
- the requested API port was allocated for loopback use and was not already occupied;
- no wildcard, IPv6 wildcard, LAN, public, DNS-derived, or hostname listener exists for the API;
- the held process is still alive and retains the recorded creation identity;
- the expected listener belongs to the exact held PID; and
- a fresh connection uses literal `127.0.0.1`, no proxy, no redirect, no DNS, no cookies, no ambient
  credentials, no automatic retry, and `Connection: close`.

Port ownership alone is not process identity. Binary verification alone is not connected-peer
identity. Both are required on the same supervised generation.

The supervisor must fail closed and revoke the generation when:

- Core exits, crashes, stops, or restarts;
- the held handle, PID, creation identity, executable volume/file identity, running image identity,
  port, manifest volume/file identity, pinned manifest digest, or schema version differs;
- an executable or manifest path contains or becomes a reparse point, has multiple hard links, or
  cannot remain held against write, rename, and delete replacement;
- a competing or stale listener answers;
- the process stops owning the full connected TCP tuple;
- the status contract or compatibility version changes; or
- pre-operation or post-operation validation cannot be completed.

No connection, authority, response, or signed envelope may cross generations. Connection pooling
across operations remains prohibited.

## Core contract validation

Controlled integration validation must use isolated Core data, log, and test ports. It must verify
the exact typed responses already required by the private wallet client:

- balance and nonce responses echo the exact requested lowercase public address;
- balance and nonce existence values agree;
- a nonexistent account has zero balance and zero nonce;
- status reports version `3` and canonical lowercase 64-hex hashes;
- transaction lookup distinguishes exact observed, pending, mined, reorganized, and not-observed
  states without inventing acceptance;
- submission responses are bounded, typed, and checked against exact local transaction and
  signature identity; and
- every unrecognized status, code, field, shape, identifier, body, or compatibility value fails
  closed with a fixed non-emitting Desktop error.

Tests must preserve exactly one application-level `POST /transactions` per user intent. Elapsed
time, nonce movement, repeated `NotFound`, restart, receipt loss, or reorganization must never cause
automatic retry, replacement, or a new signature. Duplicate transaction or sender/nonce responses
remain ambiguous unless exact signed-envelope lookup proves acceptance. Only rejection codes in the
reviewed, versioned, non-mutating allowlist may become definitive rejection.

## Authority separation

Artifact admission changes only whether the private Core boundary can be constructed. It does not
authorize wallet custody or public IPC.

The integration commit must preserve all of these conditions:

- `INDEPENDENT_LIFECYCLE_SECURITY_REVIEW_APPROVED` remains `false`;
- `INDEPENDENT_SIGNING_SECURITY_REVIEW_APPROVED` remains `false`;
- `INDEPENDENT_SUBMISSION_SECURITY_REVIEW_APPROVED` remains `false`;
- production `duplicate_key_rejection_proven` remains `false`;
- `wallet_contract_gate().signing_enabled` remains `false`;
- no wallet Tauri command or invoke-handler entry exists;
- no wallet AppManifest entry, permission, capability, or frontend invoke exists;
- React receives no password, recovery credential, path token, seed, signature, signed envelope,
  Core port, PID, process generation, or activation proof; and
- recovery export, signing, sending, publication, and wallet activation remain prohibited.

Existing non-wallet node controls may use the admitted Core only after their own exact supervisor
and behavior tests pass. A successful Core launch must not be described as an active or spendable
wallet.

## Implementation sequence

1. Create a branch from the then-current clean `origin/main`.
2. Verify the accepted evidence manifest hash and complete evidence inventory outside the repo.
3. Record the source commit, tree, filename, byte size, platform, toolchain, and acceptance verdict
   from that verified package.
4. Copy the frozen executable to a new immutable staging directory and verify size and hash before
   and after the copy.
5. Define and independently review the complete Desktop runtime-manifest bytes, exact
   `schema_version`, and full-document SHA-256 constant. Add recursive duplicate-key and
   unknown-field rejection.
6. Add handle-bound executable and manifest verification. Keep their guarded directory chains and
   share-restricted handles alive across process creation and the owned generation, and bind the
   process image file identity to the verified executable identity.
7. Replace the RC2-only launch refusal with an exact admitted-artifact policy; retain a fail-closed
   refusal for every other manifest or binary.
8. Add controlled launch, listener ownership, peer tuple, generation, restart, stale-listener, and
   compatibility regressions.
9. Run the isolated Desktop compatibility check without production Core data or wallet custody
   state.
10. Commit only the reviewed source, manifest, scripts, tests, and documentation. Do not force-add the
   executable.
11. Submit the exact commit, tree, pinned and actual Desktop runtime-manifest hash, accepted Core
    hashes, executable and manifest file identities, staging-copy hash, and evidence inventory for
    independent review.

Independent acceptance of that commit authorizes only preparation of the later unpublished atomic
wallet candidate described by `WALLET_ATOMIC_EXPOSURE_DESIGN.md`. It does not authorize that
candidate automatically.

## Validation matrix

The implementation tranche must retain complete command output and hashes for:

```powershell
npm ci
npm run typecheck
npm run test:state
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1
git diff --check
```

It must also run focused tests for:

- runtime-manifest exact parsing and whole-document unknown-field rejection;
- wrong or missing top-level schema version, unknown root and nested fields, duplicate root and
  nested fields, valid-but-unapproved complete manifest bytes, pinned-digest mismatch, manifest
  replacement, and failure to revalidate the held manifest;
- accepted binary, wrong binary, missing binary, changed manifest, and wrong platform;
- executable ancestor and final-component reparse points, hard links, pre-existing write handles,
  path swaps before and during process creation, rename/delete replacement attempts, post-hash
  replacement, process-image/file-identity mismatch, and inability to query the live image identity;
- literal-loopback launch and absence of wildcard listeners;
- exact PID, creation identity, held handle, connected tuple, and generation;
- stopped, crashed, restarted, stale, wrong-PID, occupied-port, and substituted-listener cases;
- all bounded read-only wallet responses;
- submission parsing, exact-envelope identity, one-write/no-retry behavior, ambiguity, and exact
  lookup reconciliation;
- pending, mined, confirmation advancement, mined-to-pending reorganization, canonical-block
  replacement, observation loss, and repeated `NotFound`;
- Tauri ACL and WebView isolation;
- support-package wallet and Core-identity privacy canaries; and
- source scans proving that commands, permissions, frontend custody authority, approval constants,
  and the duplicate-key blocker did not change.

Mock listeners may prove parsers and failure behavior. Only the frozen live artifact may qualify
the process, listener, peer, restart, and contract boundary.

## Rollback boundary

Before wallet exposure, rollback is source-level and complete:

- restore the previously supported Desktop runtime manifest and exact hash constant;
- restore the unconditional launch block if no independently accepted Core remains;
- remove only the untracked frozen staging copy and integration-only retrieval metadata;
- stop only the exact supervised candidate process and verify its listeners are absent; and
- preserve the external accepted evidence package unchanged.

Rollback must not delete Core production data, wallet vaults, recovery artifacts, activity,
reconciliation records, exact-envelope storage, unrelated processes, or the accepted evidence.

After any user-facing wallet qualification, artifact rollback cannot proceed independently because
it could strand custody. It must be part of the complete atomic exposure rollback defined in
`WALLET_ATOMIC_EXPOSURE_DESIGN.md`, including the reviewed offline recovery procedure.

## Stop conditions

Stop the integration without changing the runtime manifest or launch policy if:

- either frozen hash differs, is missing, or cannot be recomputed;
- the complete Desktop runtime-manifest hash is not independently pinned outside the manifest, its
  schema version differs, or whole-document unknown/duplicate-field rejection is unavailable;
- the source commit, tree, artifact size, platform, inventory, or verdict is absent or inconsistent;
- the artifact or accepted manifest was rebuilt, re-signed, repacked into changed bytes, or edited;
- the executable or manifest cannot be opened and retained without reparse traversal or write,
  rename, and delete sharing, has multiple hard links, or its volume/file identity cannot be proven;
- the running process image identity cannot be matched to the verified executable handle;
- the Desktop runtime contract requires an unreviewed Core behavior or field;
- loopback-only binding or same-connection peer ownership cannot be proven;
- a test accesses production data or an unrelated Core process;
- any deterministic no-retry, reorganization, ambiguity, or exact-lookup proof fails;
- any wallet approval flag, duplicate-key policy, command, permission, capability, or frontend invoke
  changes; or
- evidence is Failed, Inconclusive, combined across identities, incomplete, or unhashed.

No exception, temporary bypass, environment override, developer mode, or manual constant change is
permitted.

## Approval requested

Independent review is requested only for this design. If approved, the next action is the isolated
Core artifact-admission implementation and controlled compatibility validation described above.

This document does not authorize copying or executing the frozen artifact, updating the Desktop
manifest, changing the launch guard, registering wallet commands, changing approval constants,
enabling signing or submission, building an enabled wallet, publishing a release, or modifying
Vision-Core.
