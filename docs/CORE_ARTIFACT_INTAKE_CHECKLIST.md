# Core Artifact Intake Checklist

This checklist governs admission of a Vision-Core Windows artifact into Vision Desktop. It does
not authorize Vision-Core development, rebuilds, wallet activation, command registration, or
publication.

## Stop conditions

Stop without changing the Desktop manifest or bundled binary if any of the following is true:

- the verdict is Failed, Inconclusive, conditional, or does not explicitly authorize downstream
  Vision Desktop integration;
- the source commit, source tree, artifact hash, artifact size, target architecture, or evidence
  hashes are missing or inconsistent;
- the artifact was rebuilt, copied from a different build, modified, signed, compressed, or
  repackaged after qualification;
- deterministic wallet lookup, reorganization, ambiguity, and no-retry coverage is absent;
- loopback-only binding, live peer identity, or supervised process-generation ownership is not
  proven;
- evidence from separate executions or separate candidate identities was combined;
- a tracked Desktop file, wallet-custody file, Core data directory, or production installation was
  changed during qualification without explicit authorization.

The previously built artifact from Core commit
`223e2f745ebb5f7eb0d48c88397684b9037767bc`, reported as SHA-256
`586a04b311da41adcf5e41ffb390f5d99c6f3732c43083c24602f373bc3721c1`, is not eligible. Its
downstream qualification was inconclusive and must not be reused.

## 1. Required acceptance package

Obtain one immutable evidence package containing:

- final independent verdict and explicit downstream Desktop authorization;
- Core commit, tree, parent, branch or tag, and clean-worktree proof;
- exact CI workflow name, run identifier, head commit, job conclusions, and retained logs;
- locked release-build command and complete exit result;
- artifact filename, byte size, SHA-256, PE architecture, and toolchain identity;
- evidence-manifest filename and SHA-256;
- inventory of every evidence file with byte size and SHA-256;
- Windows version, architecture, account context, and qualification timestamp;
- confirmation that no candidate process or listener remains after the run.

Copy the accepted artifact from the evidence package. Do not rebuild it in Vision Desktop.

## 2. Required Core evidence

The exact source and exact artifact must prove:

- locked release compilation and the complete release test suite;
- formatting, Cargo check, strict Clippy, and repository whitespace checks;
- loopback-only HTTP binding with no wildcard or non-loopback listener;
- exact listener ownership by the supervised Core PID and generation;
- safe startup, shutdown, restart, invalid-bind, and occupied-port behavior;
- fixed, bounded wallet response schemas and hostile identifier rejection;
- balance and nonce identity/existence consistency;
- exact supported status version and canonical lowercase hash shapes;
- canonical transaction serialization, identifier, signature, and submission vectors;
- deterministic pending, mined, confirmation advancement, mined-to-pending reorganization,
  canonical-block replacement, observation-loss, and repeated-NotFound behavior;
- lookup-only reconciliation after ambiguous submission;
- exactly one POST per user intent despite elapsed time, nonce movement, NotFound, restart, or
  reorganization;
- no automatic retry, replacement transaction, or new signature;
- cleanup that preserves unrelated Core processes and production data.

Source-only tests are necessary but do not replace live artifact ownership and loopback evidence.

## 3. Local artifact verification

Before execution, record the artifact from its final evidence location:

```powershell
Get-Item -LiteralPath <accepted-vision-core.exe> |
  Select-Object FullName, Length, LastWriteTimeUtc
Get-FileHash -Algorithm SHA256 -LiteralPath <accepted-vision-core.exe>
```

Require exact equality with the accepted evidence. Record Authenticode status without changing the
file. An unsigned engineering artifact must remain classified as unsigned; do not sign it during
intake.

Verify the evidence manifest and every listed file hash before running the candidate. Copying to a
staging directory requires a second byte-size and SHA-256 comparison.

## 4. Controlled Desktop compatibility check

Use an isolated data directory and unused test ports. Do not access a production Core data
directory. Record before and after process and listener inventories.

Prove that:

- Desktop verifies the exact manifest and artifact hash before launch;
- the launched PID owns the expected loopback listener;
- no wildcard, LAN, public, DNS-derived, proxied, redirected, or ambient-credential path is used;
- a stop or restart invalidates the old process generation and connection authority;
- a stale, competing, or wrong-PID listener fails closed;
- all required read-only wallet routes return the exact versioned typed contract;
- submission compatibility tests use only isolated test state and retain the one-write invariant;
- all processes and listeners are removed at the end while evidence remains intact.

## 5. Desktop integration change

Create a new branch from current `origin/main`. The integration must be isolated from wallet
exposure and include only the files needed to admit the exact Core artifact, such as:

- `bundled/core/windows-x64/manifest.json`;
- the corresponding exact constants and validation tests in `src-tauri/src/core_manifest.rs`;
- packaging or artifact-retrieval metadata required by the approved binary-distribution policy;
- current compatibility and release documentation.

Do not force-add a Core executable to Git. `CORE_COMPATIBILITY_POLICY.md` prefers manifest-only
source control plus a signed, hash-verified release download. Any change to that binary policy
requires separate approval.

The integration change must not:

- set any lifecycle, signing, or submission security-approval constant to `true`;
- set production `duplicate_key_rejection_proven` to `true`;
- register a wallet command or add a permission, capability, AppManifest entry, or frontend invoke;
- relax private-loopback, process-generation, peer-identity, response-shape, or no-retry rules;
- modify Vision-Core source, consensus, persistence, serialization, or protocol behavior.

## 6. Desktop validation

Run and retain complete results for:

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

Also rerun Tauri authority, WebView isolation, Core manifest/hash, peer/process-generation, wallet
Core-client, submission, reconciliation, no-retry, receipt-refresh, and support-package privacy
tests against the exact integration tree.

## 7. Review and promotion boundary

Commit the artifact integration separately and submit its exact commit and tree for independent
review. The reviewer must verify the artifact and evidence hashes directly.

Acceptance of the Core artifact integration authorizes only the next separately reviewed atomic
wallet-exposure candidate. It does not itself authorize wallet activation, signing, sending,
publication, or distribution.

The eventual twelve-command candidate must be frozen before packaged qualification. Any source,
dependency, configuration, permission, capability, signing, packaging, or artifact change after
qualification creates a new candidate and invalidates the prior publication evidence.
