# Vision Desktop Private Transaction Submission Implementation Handoff

## Review status

This document hands off the private, unregistered transaction-submission implementation for an
independent exact-commit security review. It does not approve or activate submission.

Design authority:

- approved design commit: `3037b09e08c2f3e416c504aab732d73d8922ae06`;
- approved design tree: `8bd851cb2d19d6a6e804103c451679617e8eee03`;
- approved design SHA-256: `73527EA088797D403F68C20AB5A96740E017403A2AB2629308295438414346D2`;
- design finding count: zero open High, Medium, or Low findings.

The implementation remains private Rust code. It adds no Tauri command, permission, capability,
`AppManifest` entry, frontend wrapper, React form, recovery export, dependency, lockfile, Core
manifest relaxation, or Vision-Core change. Lifecycle, signing, and submission security-approval
constants remain `false`. The current Core manifest still cannot construct production wallet Core
authority.

The first implementation review of commit `9e47e2c26018622af9a36bb4608436f7758390ba`
reported zero High, two Medium, and two Low findings. This corrective candidate addresses all four:

- lifecycle now issues one non-constructible `WalletCustodyPathAuthority`; submission and restart
  reconciliation accept that authority rather than caller-selected vault or journal paths;
- the canonical journal and both reconciliation files are derived from the same lifecycle-owned
  custody directory;
- restart lookup retains the original Core fingerprint as authenticated provenance, but permits a
  newer independently validated Core generation and requires that one fresh generation to remain
  unchanged before and after the read;
- live and restart acceptance call one canonical compatibility-digest implementation; and
- exact restart lookup independently verifies the Ed25519 signature against the sender key after
  exact field validation and before accepting the signed-body digest.

## Authority path

The reviewed native approval and private signing path now promotes the continuously occupied
`WalletSigningPermit` into one `WalletSubmissionPermit` while the runtime mutex is held. Promotion
preserves the operation generation, revocation epoch, main-window owner, wallet identifier, public
account, and exact Core identity fingerprint. Direct ordinary creation of `Sign`, `Submit`, or
`Reconcile` operations is rejected.

Submission activation is separate from signing activation. It requires the signing gates, the
separate `SubmissionRejectionSemantics` compatibility requirement, and the independent submission
review gate. Restart reconciliation has its own scope and does not receive signing or network-write
authority.

The promoted permit receives one non-forgeable `SubmissionActivationGrant`, split only inside the
private coordinator into:

- a linear live reconciliation authority; and
- one private `CoreWriteOnce` child.

The token types have private proof fields and no cloning, formatting, serialization, or ordinary
construction path. The Core-write child is combined with the authenticated
`MayHaveBeenSubmitted` typestate only after that phase is durably published. The deepest Core client
consumes it and cannot return a replacement.

## Durable reconciliation state machine

The fixed store uses:

- `wallet.submission-reconciliation.json`;
- `wallet.submission-reconciliation.head.json`;
- distinct seed-derived BLAKE3 authentication keys for record and head data;
- bounded, exact Serde schemas with unknown fields denied;
- held non-reparse directory chains and fixed custody-directory paths;
- restrictive Windows file protections;
- create-new staging files, flush, handle verification, and handle-bound atomic publication; and
- authenticated transition-head recovery that accepts only the complete old or new state.

Its location is not accepted as a raw submission argument. The lifecycle boundary derives the
canonical vault and activity-journal names and issues a single-owner path authority. The store and
journal paths are derived internally from that authority, so an accepted journal write cannot be
redirected away from the marker or unlocked wallet directory.

Live authority advances linearly through `Prepared`, `MayHaveBeenSubmitted`,
`AcceptedRecordingPending`, and one terminal phase. No ordinary caller selects an arbitrary next
phase. A later attempt can begin only after an authenticated terminal record and must use a new
attempt identifier and increasing store generation. An unresolved attempt excludes another send.

The terminal phases are:

- `ResolvedNotAttempted` only before write authority can be used;
- `ResolvedRejected` only for an exact reviewed non-mutating status/code pair; and
- `ResolvedRecorded` only after exact accepted evidence is present in the authenticated activity
  journal.

The production non-mutating rejection allowlist is deliberately empty. Duplicate canonical-ID and
sender/nonce responses always remain ambiguous. A coordinated rollback of both authenticated store
files remains the documented local-storage limitation from the approved design; it does not grant
automatic signing, retry, or submission authority.

## One-attempt Core transport

The private Core client opens one fresh literal `127.0.0.1` TCP connection. It retains the existing
supervisor generation, process-creation identity, manifest fingerprint, and four-tuple server-owner
verification before and after I/O. It uses no proxy, DNS host, redirect, cookie, ambient credential,
compression, retry, or cross-generation pool.

The write path:

1. serializes the exact signed transaction once into a zeroizing bounded buffer;
2. authenticates `Prepared`;
3. revalidates runtime and Core authority;
4. authenticates `MayHaveBeenSubmitted` before the first possible socket byte;
5. revalidates authority immediately before the write;
6. consumes `CoreWriteOnce` in one bounded `POST /transactions` operation;
7. parses the response into a fixed internal classification;
8. revalidates runtime and Core authority after parsing and before result completion; and
9. returns `OutcomeUnknown` whenever exact acceptance or an allowlisted non-mutating rejection
   cannot be proven.

Partial transport, response loss, malformed data, unknown statuses, replacement decisions,
duplicate responses, Core replacement, or authority revocation never trigger retry. Response and
exact-lookup bodies are held in zeroizing buffers and expose no production `Debug` implementation.

## Restart-only recovery

After matching-wallet unlock, a separate runtime permit can authenticate the fixed store. It has no
native approval, signing artifact, submission activation, request body, or Core-write child.

Its phase-specific operations are limited to:

- `Prepared`: resolve as not attempted without Core;
- `MayHaveBeenSubmitted`: perform one bounded, read-only exact transaction lookup; and
- `AcceptedRecordingPending`: complete idempotent journal recording without Core.

`NotFound` remains unresolved. Acceptance requires exact signed-envelope identity, including the
signature, independent Ed25519 verification against the sender key, and the zeroizing signed-body
digest. The same unsigned identifier with another or invalid signature is rejected. The historical
Core fingerprint remains authenticated provenance; restart recovery uses a fresh supervisor-issued
read authority and rejects any generation change during the lookup. Restart lookup panics are
caught, revoke runtime authority, and preserve the last authenticated phase.

## Journal and receipt boundary

Production activity insertion now consumes only non-forgeable accepted evidence produced by the
reconciliation state machine. Exact duplicate records are idempotent; mismatched metadata fails
closed. The journal stores public activity metadata only and does not store the signature, signed
request body, authentication keys, or authority tokens.

Receipt reconciliation verifies the complete signed envelope and signed-body digest before it can
produce an exact accepted-lookup proof. Journal and receipt observations never create retry or
submission authority.

## Panic, revocation, and interruption evidence

One private panic boundary encloses native-approval consumption, signing, permit promotion,
reconciliation publication, body construction, Core validation, transport, parsing, journal
recording, and completion. Armed permit drop revokes the wallet session; inability to prove
invalidation terminates the process.

Deterministic tests cover:

- lifecycle revocation at every submission transition before and after the possible write;
- Core replacement immediately before and after the write;
- a write panic after durable ambiguity;
- a restart lookup panic;
- acceptance under a newer stable, independently validated Core generation;
- rejection when the fresh Core generation changes during restart lookup;
- canonical custody-directory binding for the vault, journal, and reconciliation files;
- independent Ed25519 rejection even when forged envelope fields and body digest agree;
- accepted response loss, malformed response, transport failure, and duplicate response;
- exactly one write and no retry;
- restart cleanup, journal-only completion, exact accepted lookup, and persistent `NotFound`;
- explicit allowlisted rejection versus the empty production policy;
- every reconciliation publication checkpoint;
- all three terminal-head interruption paths;
- missing, mismatched, oversized, unknown-version, unknown-field, tampered, and wrong-wallet data;
- exact UTF-16 publication filenames at the Windows allocation boundary; and
- direct sensitive-operation rejection and production activation closure.

The secure-filesystem filename test accompanies a correction that reserves the required trailing
UTF-16 NUL in the Windows `FILE_RENAME_INFO` buffer while keeping `FileNameLength` exact. Without
that correction, an allocation-boundary filename could acquire trailing garbage.

## Validation baseline

The unreviewed implementation candidate passed:

- Rust formatting check;
- strict Clippy with warnings denied;
- full serialized Rust suite (`--test-threads=1`): 277 passed, 0 failed, 4 operator-only tests
  ignored;
- Tauri authority suite: 7 passed;
- WebView isolation suite: 2 passed;
- frontend TypeScript typecheck;
- frontend state tests;
- production frontend build; and
- Git whitespace validation.

## Independent review request

The independent reviewer should verify the exact implementation commit and tree against
`WALLET_PRIVATE_TRANSACTION_SUBMISSION_DESIGN.md`, with particular attention to:

- continuous `Sign`-to-`Submit` authority and the non-forgeable one-write child;
- durable ordering before any socket byte;
- authenticated linear and restart-only reconciliation typestates;
- empty production rejection semantics and conservative duplicate handling;
- generation-bound peer-proven transport and pre/post checks;
- exact-envelope lookup and idempotent accepted-journal recording;
- panic, revocation, interruption, and privacy behavior;
- false production approval flags and unavailable current-manifest construction; and
- the absence of commands, permissions, frontend authority, dependencies, manifest relaxation, and
  Vision-Core changes.

Even independent approval of this private implementation must not by itself enable wallet commands,
signing, submission, sending, frontend custody authority, recovery export, or production activation.
