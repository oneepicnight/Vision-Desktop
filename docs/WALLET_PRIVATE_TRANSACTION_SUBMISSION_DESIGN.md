# Vision Desktop Private Transaction Submission Design

## Status and review boundary

This document specifies the next private wallet tranche after independent approval of the private
signing implementation.

Design basis:

- approved signing implementation commit: `7a3f6f3ceaf8f24d35244a7c103dc98ea600f7fd`;
- approved signing implementation tree: `6a2a0a5e4ed6120ba7706d0db685ae207008198a`;
- signing review finding count: zero open High, Medium, or Low findings; and
- approved transaction policy: `WALLET_TRANSACTION_AUTHORITY_BOUNDARY.md`.

This is a documentation-only design. It does not implement or activate submission. The existing
lifecycle and signing approval flags remain false, and the separate submission approval required by
this design does not yet exist. No wallet command, Tauri permission, capability, frontend wrapper,
form, signed-byte transport, network write, recovery export, or Vision-Core change is authorized by
this document.

Independent design approval may authorize only a private, unregistered Rust implementation of the
submission and reconciliation boundary described here. The exact implementation must receive a
separate independent review.

### Correction scope after first design review

This revision corrects the two Medium findings reported against design commit
`5848c5f96a768d735d504551fab0b99b95777d26`:

- M-01: live reconciliation now advances through linear phase-specific typestates, while restart
  recovery uses a separately activated, fixed-store, phase-bound permit that has no signing or Core
  write authority; and
- M-02: definitive rejection now requires an exact independently reviewed non-mutating allowlist.
  The current allowlist is empty, and both duplicate response classes remain ambiguous pending exact
  signed-envelope lookup.

This statement records the claimed correction for re-review; it does not approve the design.

## Existing implementation facts

The reviewed private tree already provides:

- one native-confirmed, session-bound `SignedTransferArtifact` containing the exact signed
  `VisionTransaction`, canonical unsigned identifier, wallet identifier, and Core identity
  fingerprint;
- continuous preview-to-confirmation-to-signing runtime authority with pending-revocation checks;
- exact RC2 transaction construction, bincode identifier calculation, Ed25519 signing, and local
  signature verification;
- a strict parser for the documented `POST /transactions` response;
- exact `GET /transaction/:txid` receipt parsing and signed-transaction identity verification;
- a seed-authenticated, head-protected, crash-safe local activity journal; and
- a generation-bound, peer-proven, read-only loopback Core client.

The existing `wallet/submission.rs` is only a response parser. It performs no network request. The
existing signing coordinator destroys `SignedTransferArtifact` inside `wallet/signing.rs`. No
production code can submit, retain, format, serialize, or expose the artifact.

The current Core manifest cannot construct production wallet Core authority because frozen RC2
cannot bind its HTTP API to loopback only and the approved peer-binding contract is unavailable.

## Security objective

One physical native approval may produce at most one network submission attempt for exactly the
signed transaction that was displayed. Authority must remain continuously bound to:

- the main window and exact `WalletRuntimeState`;
- the unchanged operation generation and revocation epoch;
- the continuously unlocked wallet and public account;
- the single consumed native approval;
- the exact signed transaction, signature, nonce, and unsigned identifier;
- the exact approved Core compatibility contract; and
- one durable reconciliation record committed before the request may be sent.

No replay, retry, replacement, re-signing, nonce adjustment, or alternate transaction body is
permitted. A result that cannot be proven accepted or rejected becomes `OutcomeUnknown`.

## Explicit non-goals

This tranche does not authorize:

- a Tauri command, permission, capability, `AppManifest` entry, frontend wrapper, or React form;
- returning signed bytes, signatures, reconciliation records, or submission authority to React;
- transaction replacement, custom tips, batching, arbitrary modules or methods, or future nonces;
- automatic retry after any submission attempt;
- treating HTTP success, local history, nonce movement, or a missing receipt as transaction success;
- calling any confirmation count final;
- exposing Core ports, PIDs, paths, raw responses, or operating-system errors;
- changing either wallet security approval flag; or
- modifying Vision-Core.

## Separate submission activation

Signing approval must not implicitly grant network-write authority. A conforming private
implementation adds a distinct `WalletActivationScope::Submission`, a non-forgeable
`SubmissionActivationProof`, and a production constant structurally equivalent to
`INDEPENDENT_SUBMISSION_SECURITY_REVIEW_APPROVED: bool = false`.

Submission activation requires every lifecycle and signing requirement plus:

- the exact independently approved private submission implementation;
- a supported private-loopback Core manifest whose versioned contract explicitly permits the
  reviewed write route and peer-binding mechanism;
- an independently reviewed non-mutating rejection table covering the exact supported response
  contract;
- exact supervisor generation and live connected-peer proof; and
- a separate independent submission-security approval.

The production flag must remain false throughout the private implementation tranche. Test-only
satisfied policies may exercise the boundary but must not alter production construction. Atomic
promotion consumes one `SubmissionActivationProof` and creates a private, linear activation grant
with two purpose-specific authorities:

- one `LiveReconciliationAuthority` whose typestate can advance only through the exact durable
  submission phases; and
- one single-use `CoreWriteOnce` child for the only permitted network write.

Only the private coordinator can split the grant. The live reconciliation authority is not consumed
by its first store operation. Instead, each authenticated transition consumes the current typestate
capability and returns the one capability valid for the next durable phase. The deepest seed-owning
reconciliation operation and the first write-capable Core operation require their respective
authorities. Command wrappers, booleans, status fields, or caller assertions are not authority.

Restart recovery uses a different `ReconciliationActivationProof`. It requires the approved
reconciliation implementation, lifecycle activation, and the exact supported store schema. It does
not require or imply signing approval, a running Core, the read or write contract, or permission to
begin a new submission. This permits `Prepared` cleanup and accepted journal recovery with Core
offline, including when new sends are later disabled. Its production constructor checks the same false
`INDEPENDENT_SUBMISSION_SECURITY_REVIEW_APPROVED` gate but deliberately has no write-compatibility
requirement. Production construction of either activation proof remains unavailable until the exact
submission implementation and reconciliation boundary are independently approved.

A conforming implementation adds an exact contract requirement such as
`WalletContractRequirement::SubmissionRejectionSemantics`. The existing
`SubmissionResponse` requirement proves response shape only and cannot satisfy this new gate. The
current empty allowlist leaves production submission activation unmet. Restart discovery,
`Prepared` cleanup, and accepted journal-only recovery do not require this write-only contract
gate.

## Threat model

The implementation must fail closed against:

- compromised React or a bundled-WebView injection;
- replayed handles, repeated native approval, duplicated completion, or reused artifacts;
- explicit lock, idle timeout, workstation lock, suspend, window destruction, shutdown, process-lock
  loss, panic, wallet replacement, and concurrent operations;
- Core exit, restart, PID reuse, generation replacement, manifest drift, port replacement,
  peer-owner mismatch, and stale connections;
- connection failure, partial writes, response loss, malformed responses, and process termination at
  every persistence or transport boundary;
- marker deletion, tampering, rollback, reparse points, path replacement, and interrupted file
  publication;
- a returned transaction with the same unsigned identifier but a different signature; and
- leakage through errors, panic output, logs, support packages, command-line state, diagnostics, or
  future IPC.

As elsewhere in the wallet design, a fully compromised administrator or complete rollback of all
wallet files and the Windows profile is residual platform risk. Such rollback must never grant
automatic retry or signing authority.

## Required continuous authority transition

Submission must not begin as a new ordinary wallet operation after signing completes. The existing
`Sign` permit must be atomically promoted to a distinct `Submit` permit while the runtime lock is
held. The active-operation slot must never become empty between native confirmation, signing,
durable intent recording, submission classification, and permit completion.

Promotion must preserve:

- operation generation and revocation epoch;
- main-window owner;
- wallet identifier and public account;
- the exact Core compatibility identity and signed-transaction fingerprint; and
- an armed fail-closed state.

The old signing permit must be explicitly disarmed only after the submission permit owns the same
active slot. Direct creation of a `Submit` operation must be rejected, including in test-satisfied
activation policies.

Dropping an armed submission permit invalidates wallet authority. If invalidation cannot be proven,
the process terminates. Permit loss must not erase a durable ambiguous-outcome record.

## Private module and type boundary

The signed artifact must remain private to the signing module. The implementation should place the
submission coordinator in a private child module such as:

`src-tauri/src/wallet/signing/submission_coordinator.rs`

A child module may consume its parent's private `SignedTransferArtifact` without making that type
visible to ordinary `crate::wallet` siblings. The existing `wallet/submission.rs` may remain the
strict response parser, but it receives only bounded bytes from the private coordinator.

Required private types are structurally equivalent to:

- `WalletSubmissionPermit`: a linear, armed runtime capability produced only by atomic promotion;
- `SubmissionActivationGrant`: a private linear grant split only into one live reconciliation
  state-machine authority and one Core-write child;
- `PreparedReconciliationAuthority`, `MayHaveBeenSubmittedAuthority`, and
  `AcceptedRecordingAuthority`: private typestate capabilities produced only by consuming the
  preceding authenticated phase;
- `WriteReadySubmission`: a private aggregate produced only after
  `MayHaveBeenSubmitted` is durably authenticated; it owns the one Core-write child and the
  matching may-have typestate authority;
- `ReconciliationDiscoveryPermit`: a restart-only permit for bounded fixed-store discovery and
  authentication after matching-wallet unlock;
- `RestartReconciliationPermit`: a phase-bound restart capability that contains no signing or
  network-write authority;
- `PendingSubmission`: the permit, exact single-owner signed artifact, and retained Core authority;
- `SubmissionAttemptId`: random 256-bit internal identifier that never crosses IPC;
- `AuthenticatedReconciliationRecord`: bounded durable public metadata authenticated by a
  wallet-seed-derived subkey; and
- `AcceptedSubmissionEvidence`: a private, non-forgeable capability created only from an exact
  accepted response or exact matching read-only reconciliation result; and
- `PrivateSubmissionDisposition`: fixed internal categories for `NotAttempted`, `Accepted`,
  `Rejected`, `OutcomeUnknown`, and `AcceptedRecordingPending`.

Capability-bearing types implement neither `Clone`, `Copy`, `Debug`, `Display`, nor serialization.
Privacy-sensitive response types expose `Debug` only in tests. No general-purpose method may return
the signed transaction or request body to an ordinary wallet caller.

## Exact signed body

The coordinator consumes the artifact once and validates again that:

- the signature is exactly 64 bytes encoded as 128 lowercase hexadecimal characters;
- Ed25519 verification succeeds against the exact retained canonical payload and sender key;
- the unsigned BLAKE3 identifier is unchanged and matches the retained identifier;
- sender, recipient, amount, nonce, tip, fee limit, module, method, and canonical arguments still
  match the reviewed intent;
- the retained wallet and public account still match the unlocked session; and
- the retained Core compatibility identity remains approved.

The request body is exactly one reviewed `serde_json::to_vec` encoding of that signed
`VisionTransaction`. That byte vector is created once, bounded to at most 64 KiB, and retained only
through the single attempt. The digest is calculated over those exact bytes using a domain-separated
BLAKE3 construction. The body is never reconstructed between durable recording and transport.

The body and signature never enter the journal, reconciliation record, logs, errors, support
packages, crash text, command line, reducer, WebView, or developer tools.

## Durable pre-write reconciliation record

Before any network write is possible, Rust must durably publish one bounded reconciliation record.
The record contains only:

- schema version and domain identifier;
- wallet identifier;
- random attempt identifier;
- canonical transaction identifier;
- canonical sender and recipient public addresses;
- exact decimal amount in raw units, nonce, tip in raw units, and fee limit in raw units;
- digest of the exact signed request body;
- original Core compatibility and generation fingerprint;
- non-authoritative creation time for operator display; and
- one exact phase.

These public transfer fields are retained because the existing authenticated journal needs them to
finish recording after an accepted response and a crash. They must exactly match the native-approved
intent, signed artifact, canonical transaction identifier, and envelope digest before publication.
The record contains no signed body, signature, seed, password, recovery credential, activation
proof, session handle, Core port, PID, filesystem path, or retry instruction.

The phases are:

1. `Prepared`
   - The record is durable and authenticated, but no operation capable of writing to the socket has
     begun.
2. `MayHaveBeenSubmitted`
   - This phase is durably committed before the first call that may write request bytes. Every crash,
     cancellation, timeout, or transport failure after this transition is ambiguous.
3. `AcceptedRecordingPending`
   - An exact accepted response or exact matching point lookup was proven, but authenticated local
     activity recording has not yet completed. The authenticated phase binds the matching
     transaction identifier and nonce, the exact `accept` decision with no replacement, the signed
     body digest, and the compatibility-contract digest; it stores no raw response.

The protected head also records one terminal state: `ResolvedNotAttempted`, `ResolvedRejected`, or
`ResolvedRecorded`. A terminal transition increments a monotonic store generation, authenticates the
attempt and preceding head, and is committed before live record cleanup. Definitive rejection uses
`ResolvedRejected` and binds the exact status/code plus the reviewed non-mutating-allowlist digest;
a proven pre-write failure uses `ResolvedNotAttempted`; and accepted journal
completion uses `ResolvedRecorded` only after the exact local activity record is verified. Deletion
without a committed terminal head is not a valid transition. Starting a later attempt increments the
generation again and binds the new head to the preceding terminal state.

## Reconciliation storage security

The reconciliation store is independent from the display-only activity journal because the journal
must never supply retry or submission authority. It must use:

- a dedicated domain-separated subkey derived inside the unlocked wallet session;
- authenticated content and an independently authenticated head/phase record;
- fixed filenames beneath the validated wallet custody directory;
- held, non-reparse directory chains and handle-bound file operations;
- restrictive per-user Windows permissions;
- bounded reads and exact schemas with unknown fields denied;
- random create-new staging files, flush, read-back authentication, and handle-based atomic replace;
- interruption recovery that accepts only a matching complete old or new phase; and
- the existing cross-session wallet process lease plus in-process exclusion.

The store permits at most one unresolved submission for the wallet. Its independently protected head
retains the latest attempt identifier, monotonic generation, record digest, phase, and preceding-head
authentication link, including after terminal cleanup. Corruption, unexpected deletion, rollback
mismatch, authentication failure, or ambiguous transition prevents new submission and returns one
fixed reconciliation-unavailable error.

A coordinated rollback of the record and its authenticated head remains a documented local-storage
limitation. It cannot authorize automatic submission or retry; any later transaction still requires
fresh Core reads, a new preview, and new physical confirmation.

## Reconciliation authority and seed access

The implementation must not add a generic seed closure. During the live attempt, runtime code
exposes only purpose-specific typestate transitions:

1. The initial live authority authenticates and durably publishes `Prepared`, then is consumed to
   produce `PreparedReconciliationAuthority`.
2. That authority authenticates and durably publishes `MayHaveBeenSubmitted`, then is consumed to
   produce `MayHaveBeenSubmittedAuthority`. Only then may the coordinator combine it with the
   single-use Core-write child to create `WriteReadySubmission`.
3. Exact acceptance consumes the may-have authority while publishing
   `AcceptedRecordingPending`, producing `AcceptedRecordingAuthority`.
4. An exact contractually definitive rejection consumes the may-have authority while publishing
   `ResolvedRejected`.
5. A proven pre-write stop consumes the prepared authority while publishing
   `ResolvedNotAttempted`.
6. Verified accepted journal completion consumes the accepted-recording authority while publishing
   `ResolvedRecorded`.

No transition API accepts an enum supplied by an ordinary caller. Every operation validates the
current authenticated record and head, expected store generation, attempt, wallet, operation
generation, revocation epoch, and exact preceding phase before publishing the next state. An
`OutcomeUnknown` leaves `MayHaveBeenSubmitted` durable; completing the runtime operation does not
resolve or delete it.

Every transition:

- requires the exact phase-bound authority;
- checks pending runtime revocation before and after publication;
- derives only the dedicated reconciliation authentication subkey;
- authenticates or verifies one bounded transition;
- zeroizes the derived key and workspace on success, error, panic, and drop; and
- returns no seed, subkey, signing key, activation proof, or generic authenticator.

### Restart-only discovery and recovery

Restart reconciliation begins only after bounded discovery of the fixed reconciliation filename
beneath the validated wallet custody directory. Discovery treats every byte as unauthenticated,
reveals no record fields to the caller, follows no caller path, and grants no authority.

After the user unlocks the matching wallet, the runtime may create one
`ReconciliationDiscoveryPermit` only when:

- the main-window owner, wallet identifier, public account, runtime generation, revocation epoch,
  cross-session process lease, and fixed custody directory all match;
- no other wallet operation is active;
- a non-forgeable `ReconciliationActivationProof` satisfies the exact store-schema requirement; and
- the permit is bound to the current unlocked session but carries neither signing nor write
  activation.

The permit authorizes one purpose-specific seed operation that authenticates the complete record and
protected head. It is consumed on success and returns a `RestartReconciliationPermit` bound to the
authenticated attempt, store generation, record digest, wallet, account, owner, runtime generation,
revocation epoch, and exact phase.

The restart permit is a private enum whose variants expose only these operations:

- `Prepared`: publish `ResolvedNotAttempted`; no Core operation is allowed.
- `MayHaveBeenSubmitted`: obtain a fresh supervisor-issued `CoreConnectionAuthority` satisfying the
  approved read-only contract, perform bounded point lookup, and either advance to
  `AcceptedRecordingPending` on an exact signed-envelope match or remain unresolved. Core authority
  unavailability leaves the record unresolved.
- `AcceptedRecordingPending`: construct accepted evidence from the authenticated phase and finish
  the journal only; no Core request is required.

No restart variant is accepted by signing, preview, confirmation, live promotion, or Core-write
APIs. It contains no `SubmissionActivationProof`, `CoreWriteOnce`, signed artifact, native approval,
seed accessor, or request body and is structurally incapable of calling `POST /transactions`.

Pending revocation is checked before and after authentication, every read-only lookup, journal
publication, and terminal transition. Cancellation or an unresolved `NotFound` may consume the
permit and release the active runtime slot while leaving the authenticated record unchanged for a
later read-only recovery attempt. Dropping an armed discovery or restart permit invalidates the
wallet runtime; failure to prove invalidation terminates the process. Panic containment returns only
fixed errors, preserves the last authenticated durable phase, zeroizes derived material, and never
converts restart authority into signing or write authority.

## Core write authority

The private submission client must obtain its authority from the supervisor. Production URL, port,
PID, process creation identity, generation, and compatibility fingerprint remain impossible for an
ordinary caller to construct.

For the one attempt it must:

- use a fresh connection to literal `127.0.0.1`;
- disable proxies, redirects, DNS, cookies, ambient credentials, authentication, compression,
  automatic retries, and cross-generation pooling;
- retain the supervised process handle and validate the exact process generation before connecting;
- prove the connected server endpoint's owning PID from the live four-tuple before writing;
- reject a stale listener, PID/creation mismatch, port mismatch, peer mismatch, or manifest change;
- use `POST /transactions`, `Content-Type: application/json`, exact `Content-Length`, and
  `Connection: close`;
- reject chunked request or response semantics;
- use the established 2-second connect timeout, 3-second whole-operation deadline, 8-KiB header
  bound, and 64-KiB body bound unless a newly reviewed compatibility contract is stricter;
- perform no retry at the HTTP, socket, application, or coordinator layer; and
- revalidate peer ownership, supervisor generation, and compatibility after the response.

The write client accepts only `WriteReadySubmission`, proving both that
`MayHaveBeenSubmitted` is durable and that the one Core-write child remains unused. It consumes the
Core-write child at the deepest write-capable boundary and returns the phase-bound may-have
authority with the fixed response classification; there is no second write child. It accepts no
caller-built URL, port, process identity, compatibility value, HTTP method, path, request body, or
retry policy. Exact JSON responses require an approved `application/json` content type; missing,
ambiguous, or conflicting content types fail closed after the request and therefore produce
`OutcomeUnknown` unless the exact response was already proven.

The first write-capable call must occur only after `MayHaveBeenSubmitted` is durably committed.
Partial header or body writes are not retried. Connection reuse is forbidden.

Production construction remains impossible until a supported private-loopback Core release defines
the approved peer-binding mechanism in the compatibility manifest.

## Submission outcome classification

The coordinator executes at most one write attempt and classifies conservatively:

- `NotAttempted`
  - Authority or connection establishment failed before the record entered
    `MayHaveBeenSubmitted`. The artifact is consumed and destroyed. A future attempt requires a new
    preview and physical confirmation.
- `Accepted`
  - Only HTTP 200 with the exact typed `accepted` response, matching canonical identifier and nonce,
    `accept` decision, no replacement, and successful post-response Core validation.
- `Rejected`
  - Only an exact typed response whose status/code tuple appears in the independently reviewed,
    versioned compatibility contract's non-mutating rejection allowlist, followed by successful
    post-response Core validation. The current verified RC2 response-shape documentation contains no
    such no-insertion guarantee, so its definitive-rejection allowlist is empty.
- `OutcomeUnknown`
  - Any failure after `MayHaveBeenSubmitted` that is not an exact accepted or rejected result,
    including timeout, partial response, connection loss, malformed or oversized response,
    unexpected status, identifier/nonce mismatch, replacement decision, peer-validation failure,
    Core exit/restart, panic, cancellation, lifecycle revocation, or a typed 400/422 response not in
    the exact non-mutating allowlist.
- `AcceptedRecordingPending`
  - Acceptance is proven, but the authenticated journal update has not been verified.

HTTP success alone is never acceptance. A malformed response after a request was sent is not
rejection. Error text from Core is never logged or returned through a future boundary.

No classification permits automatic resubmission.

### Definitive-rejection allowlist

A typed parser enum is not proof that Core performed no state mutation. The supported compatibility
manifest must enumerate every exact HTTP status and rejection code that Core guarantees is returned
before mempool or chain insertion of the submitted envelope. That guarantee, including ordering
relative to durable Core state, requires separate Core-compatible evidence and independent review.
Absence, drift, or an unknown code makes the result `OutcomeUnknown`.

The currently parsed response codes are classified as follows:

- `DuplicateCanonicalTxId`: always `OutcomeUnknown` from the POST response. The identifier excludes
  the signature, so only point lookup followed by exact body-digest equality and signature
  verification can prove acceptance of this envelope.
- `DuplicateSenderNonce`: `OutcomeUnknown` unless the exact envelope is proven by point lookup. A
  sender/nonce collision does not identify which signed envelope Core retains.
- `StaleNonce`, `NonceGap`, `TxTooLarge`, `MissingSenderPubkey`, `MissingSignature`,
  `UnsupportedModuleMethod`, `BadTransferArgs`, `InvalidTransferDestination`,
  `TransferAmountZero`, `TransferToSelf`, `FeeLimitTooLow`, `SenderPubkeyWrongLength`,
  `SenderPubkeyNotLowercaseHex`, `SignatureWrongLength`, `SignatureNotLowercaseHex`,
  `MalformedPublicKey`, and `InvalidSignature`: definitive only if each exact code is separately
  present in the reviewed non-mutating allowlist; otherwise `OutcomeUnknown`.
- HTTP 400 `MalformedRequest`: definitive only if the exact versioned contract proves that this
  response is emitted before any insertion; otherwise `OutcomeUnknown`.
- every future, unknown, mismatched, or structurally invalid code: `OutcomeUnknown`.

Neither duplicate code may transition to `ResolvedRejected` merely because its response was typed,
the nonce moved, or the transaction identifier matched. An exact point lookup that returns the same
unsigned identifier but a different signature also leaves the outcome unknown.

## Ambiguous-outcome reconciliation

An `OutcomeUnknown` record has no expiry. It is resolved only after the matching wallet is unlocked
and the record authenticates successfully.

Reconciliation uses a newly supervisor-issued, read-only `CoreConnectionAuthority` and performs only
bounded `GET /transaction/:txid` and status reads. A newer approved Core generation may answer the
read-only lookup, but it never receives another submission.

The observer must:

- require the echoed identifier to match;
- recompute the unsigned identifier from the returned transaction;
- canonically serialize the complete returned signed transaction;
- require its domain-separated digest to match the stored exact-envelope digest; and
- require the returned signature to verify against the sender key.

An exact matching pending or mined transaction proves acceptance and advances the record to
`AcceptedRecordingPending`. `NotFound` leaves the outcome unknown. A different signature sharing the
same unsigned identifier, malformed transaction, unavailable Core, stale observation, moved nonce,
or changed balance does not prove acceptance or rejection.

Reconciliation must never:

- re-sign, resubmit, retry, or create a replacement;
- guess whether Core received the original request;
- infer rejection from `NotFound`, current nonce, balance, elapsed time, or local journal state; or
- advise the user that resending is safe.

## Accepted activity and journal ordering

After exact acceptance:

1. Commit `AcceptedRecordingPending` to the reconciliation store.
2. Construct one private `AcceptedSubmissionEvidence` capability from the exact accepted response
   and the authenticated record. On restart from `MayHaveBeenSubmitted`, construct it only after an
   exact matching signed transaction is returned by the read-only point lookup. On restart from an
   already authenticated `AcceptedRecordingPending` phase, reconstruct it directly from that
   phase's protected public metadata and acceptance binding so journal-only recovery does not require
   Core availability.
3. Derive the journal authenticator through the existing purpose-specific seed boundary.
4. Append the exact accepted public metadata carried by the evidence capability to the authenticated
   journal. Ordinary callers cannot construct accepted metadata or call a transaction-shaped
   journal append path.
5. Read back and verify the journal and protected head.
6. Commit `ResolvedRecorded` to the reconciliation head and remove only safely replaceable staging
   material.

If journal persistence fails, acceptance remains authoritative and the reconciliation record stays
`AcceptedRecordingPending`. A later operation may retry only the authenticated journal write. It
must never contact `POST /transactions`.

If an identical journal record already exists after interruption, Rust treats recording as complete
only after verifying every public field and authenticated chain/head position. A duplicate identifier
with different metadata fails closed.

The `AcceptedSubmissionEvidence` capability is linear, has no unrestricted formatting, cloning, or
serialization, and is consumed by journal recording. It binds the wallet, attempt, exact accepted
response, public transfer metadata, unsigned identifier, and signed-envelope digest. Neither a raw
`VisionTransaction` nor a caller-constructed `WalletSubmissionOutcome` is sufficient journal-write
authority after this tranche.

Rejected and malformed requests are not added to accepted activity. The journal remains incomplete
local display history and never supplies balances, nonces, fees, signing authority, submission
authority, retry decisions, or receipt truth.

## Receipt tracking

Once acceptance and local recording are complete, read-only tracking may use the existing exact
receipt parser. Every observation must validate the complete returned signed transaction and the
stored envelope digest, not only the unsigned identifier.

Presentation states remain:

- `Accepted - recording pending`;
- `Outcome unknown - reconciliation required`;
- `Accepted - not currently observed`;
- `Pending`;
- `Mined - N confirmations`;
- `Reorganized` or `Observation lost`; and
- `Core unavailable - last observation stale`.

No state is called final. Fifty confirmations remain a presentation-only `High confidence` threshold.
Missing observations, reorganizations, journal failures, and outages never trigger resubmission.

## Runtime revocation and panic ordering

The active submission permit must check runtime generation, revocation epoch, pending revocation,
wallet identity, and Core authority:

- before promotion;
- before marker authentication;
- after `Prepared` publication;
- before `MayHaveBeenSubmitted` publication;
- immediately before the first possible socket write;
- after response receipt and parsing; and
- before final permit completion.

Revocation before `MayHaveBeenSubmitted` prevents the request. Revocation at or after that phase
leaves a durable ambiguous record and suppresses every success result until reconciliation.

One panic boundary encloses artifact consumption, promotion, marker operations, body construction,
Core validation, transport, response parsing, journal transition, and completion. A panic before the
ambiguous phase resolves as not attempted only when the durable phase proves no write-capable action
began. A panic afterward is `OutcomeUnknown`. Panic text remains non-emitting and contains no
transaction data.

## Fixed internal errors

The private coordinator may expose only fixed categories such as:

- submission activation unavailable;
- runtime authority revoked;
- signed artifact invalid;
- reconciliation storage unavailable;
- Core identity unavailable;
- not attempted;
- rejected with a reviewed enum code;
- outcome unknown; and
- accepted but local recording pending.

Raw Core messages, JSON, operating-system errors, paths, PIDs, ports, addresses, nonces, identifiers,
signatures, body digests, timing values, and retry hints must not enter formatted errors, logs,
diagnostics, or future IPC.

## Required private implementation scope

An independently approved implementation is limited to:

- `wallet/runtime.rs`
  - atomic `Sign`-to-`Submit` permit promotion and purpose-specific reconciliation authentication;
- `wallet/activation.rs` and `wallet/contract.rs`
  - the separate, fail-closed submission scope and false production approval gate;
- `wallet/signing.rs` and a private child coordinator
  - single-use artifact consumption, exact body creation, orchestration, and local destruction;
- `wallet/core_client.rs` or a private write sibling
  - one bounded generation-bound POST and read-only reconciliation lookup;
- `wallet/submission.rs`
  - strict response parsing and fixed internal classification;
- a new private reconciliation-store module
  - authenticated crash-safe phase storage;
- `wallet/journal.rs` and `wallet/receipt.rs`
  - exact accepted-record idempotence and complete signed-envelope verification; and
- focused Rust tests and security documentation.

No command module, capability, permission, frontend service, component, shared Desktop state, event,
reducer, dependency, lockfile, Core manifest approval, production flag, or Vision-Core file belongs in
that implementation tranche.

## Required adversarial tests

Implementation review must include deterministic tests for:

### Authority and single use

- atomic `Sign`-to-`Submit` promotion with no empty active slot;
- direct `Submit` operation rejection;
- artifact, approval, permit, request-body, and completion replay;
- duplicate coordinator invocation and concurrent wallet operation exclusion; and
- wallet, account, window, generation, and revocation mismatch;
- consumption of every live reconciliation typestate with no skipped, repeated, reversed, or
  caller-selected phase;
- inability to construct `WriteReadySubmission` before durable `MayHaveBeenSubmitted`, plus
  proof that the write client consumes the only Core-write child and cannot return another;
- restart discovery before unlock, wrong-wallet unlock, unauthenticated record, mismatched head,
  unsupported schema, and concurrent active operation; and
- compile-time or authority-surface proof that discovery and restart permits cannot reach preview,
  confirmation, signing, `CoreWriteOnce`, or `POST /transactions`.

### Durable ordering and interruption

- every checkpoint before and after `Prepared`, `MayHaveBeenSubmitted`, acceptance, journal write,
  and resolution;
- process termination and panic at every publication phase;
- partial staging writes, flush failure, rename failure, read-back failure, and cleanup failure;
- tampered, deleted, rolled-back, duplicated, oversized, unknown-version, and unknown-phase records;
- reparse points, alternate data streams, path replacement, and held-handle races; and
- proof that no socket write occurs unless the ambiguous phase is durably authenticated; and
- terminal-head interruption tests for `ResolvedNotAttempted`, `ResolvedRejected`, and
  `ResolvedRecorded`, including cleanup followed by rollback or deletion.

### Transport and Core identity

- zero, partial, and complete request writes with failure at every boundary;
- one request maximum and no retry under every error;
- connect timeout, whole-operation timeout, truncated/oversized headers and bodies, chunking,
  redirect, invalid content type, malformed JSON, unknown fields, and unexpected status;
- Core stop, restart, PID/creation change, generation change, manifest drift, port replacement,
  four-tuple owner mismatch, and peer-binding failure before and after the write; and
- fresh literal-loopback connections with no proxy, DNS, credentials, cookies, compression, or
  pooling.

### Response and ambiguity

- exact accepted, rejected, malformed-request, mismatch, replacement, and future response vectors;
- accepted-response loss and response-parse failure after Core accepts;
- restart with every unresolved phase;
- restart from `Prepared` resolving only as not attempted, restart from
  `MayHaveBeenSubmitted` permitting only read-only lookup, and restart from
  `AcceptedRecordingPending` completing journal recording with Core offline;
- every parsed rejection code independently exercised as absent from, present in, and mismatched
  with the versioned non-mutating allowlist where eligibility applies;
- `DuplicateCanonicalTxId` and `DuplicateSenderNonce` always remaining unknown until exact-envelope
  lookup, including matching unsigned identifier with a different signature;
- HTTP 400 `MalformedRequest`, every current non-duplicate 422 code, every future code, and every
  malformed status/code combination failing to resolve unless the exact reviewed contract permits
  it;
- `NotFound` remaining unknown indefinitely;
- matching pending/mined lookup proving acceptance;
- same unsigned identifier with a different signature being rejected; and
- no nonce, balance, time, or journal inference and no automatic resend.

### Runtime lifecycle

- explicit lock, idle timeout, same-wallet lock/unlock, wallet replacement, public-account mismatch,
  window reload/destruction, workstation lock, suspend, process-lock loss, shutdown, and panic at
  every submission transition;
- pending revocation waiting on the runtime mutex before marker authentication and before result
  escape; and
- durable ambiguity surviving runtime and process teardown without preserving secret authority; and
- cancellation, panic, armed-drop, runtime revocation, and failed invalidation for discovery and
  every restart-permit phase, with the last authenticated record preserved.

### Journal and receipts

- acceptance followed by journal failure and journal-only retry;
- interruption before and after an idempotent accepted record;
- duplicate exact record versus duplicate mismatched metadata;
- pending, mined, increasing confidence, reorganization, observation loss, and stale Core; and
- proof that journal or receipt state never grants submission or retry authority.

### Privacy and surface closure

- seed, key, password, signature, signed body, digest, address, nonce, identifier, and Core-identity
  canaries across errors, panic handling, logs, diagnostics, support packages, command line,
  serialization, frontend assets, and generated permissions;
- capability types lacking unrestricted formatting, cloning, and serialization;
- no wallet Tauri command, permission, capability, frontend wrapper, or form;
- no production authority under the current manifest and false approval flags; and
- no path from signing approval alone to marker authentication or a network write; and
- no dependency, lockfile, broad filesystem, arbitrary network, shell, or Vision-Core change.

## Independent review gate

Before implementation, an independent reviewer must approve this exact design or identify required
corrections. Design approval may authorize only the private, unregistered implementation scope above.

After implementation, a separate reviewer must inspect the exact commit and tree, reproduce the
focused and full gates, verify the authority surface remains closed, and decide whether a later
private command-boundary design may begin.

Even an approved private submission implementation does not authorize:

- production submission or signing activation;
- wallet commands, permissions, capabilities, frontend sending, or signed-byte IPC;
- integrating or relaxing the current Core manifest;
- transaction retry or replacement;
- recovery export or hardware-wallet support; or
- beginning the three-node internet mining test with a Desktop-generated address.
