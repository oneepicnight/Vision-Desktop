# Vision Desktop Wallet ? Authenticated Exact-Envelope Storage and Receipt Refresh Design

Date: 2026-08-10
Status: design only; no implementation or exposure approval
Reviewed baseline: `b108d54a88f658219ec5ab6fce5a07bbf316ce0f`
Corrective revision: 2026-08-11
Correction target: M-01 and M-02 from the independent review of
`a20d98154b55727856eed8180544eb559b3d3451`

The correction freezes an immutable commitment independent of retention/container state, binds it to
an authenticated reconciliation parent and reserved `Prepared` generation in both directions, and
adds permanent transaction-identifier uniqueness plus an authenticated journal v3 commitment
association. Finding closure remains subject to independent re-review.


## Purpose

The private transaction command boundary intentionally leaves
`wallet_refresh_transaction_observation` unavailable. Journal v2 authenticates public activity, but
it does not retain the exact signed transaction required by the reviewed receipt parser. A
transaction identifier alone cannot prove that a Core response contains the exact envelope that
Vision Desktop signed.

This design defines the smallest private storage extension that can retain that expectation and use
it for one bounded, read-only receipt refresh. It does not authorize implementation, Tauri command
registration, frontend integration, signing or submission activation, or a Core manifest change.

## Existing facts and constraints

The design preserves these existing boundaries:

- `VisionTransaction` is the exact versioned Core transaction envelope.
- The canonical transaction identifier excludes the signature, so identifier equality alone is not
  exact-envelope equality.
- The private submission coordinator creates one exact JSON body, limits it to 64 KiB, and derives a
  domain-separated BLAKE3 digest over those exact bytes.
- The reconciliation record authenticates that digest and public transfer metadata, but deliberately
  does not retain the body or signature.
- Journal v2 authenticates public display activity and observations, but deliberately contains no
  signed body or signature and grants no transaction authority.
- `parse_exact_signed_receipt_observation` already requires the returned transaction, its canonical
  identifier, its exact serialized-body digest, and its Ed25519 signature to match the expected
  transaction.
- Core `NotFound` is uncertain. It is never proof of rejection and never makes resubmission safe.
- Canonical-depth observations can reorganize. The 50-confirmation label is presentation-only and
  never means finality.
- Production wallet Core authority remains unavailable until an independently supported
  private-loopback Core compatibility manifest defines the reviewed peer-binding mechanism.

## Goals

The extension must:

1. Persist the exact signed envelope before the first network write can occur.
2. Keep the envelope confidential and authenticated at rest.
3. Bind it to one wallet, submission attempt, canonical transaction identifier, exact body digest,
   compatibility contract, and reconciliation generation.
4. Retain accepted envelopes for receipt refresh without granting signing or submission authority.
5. Permit a matching unlocked wallet to perform one fresh, peer-proven, read-only observation.
6. Authenticate and persist only conservative receipt observations in the existing journal.
7. Fail closed without corrupting activity or relaxing the spending interlock.

## Non-goals

This tranche does not design or authorize:

- a transaction retry, rebroadcast, replacement, fee bump, or resubmission path;
- storage or return of a seed, password, recovery credential, activation proof, session token, Core
  endpoint, process identifier, process handle, or write capability;
- a frontend-accessible signed envelope, signature, canonical payload, or exact request body;
- automatic polling, background refresh, chain-wide account history, or pagination;
- receipt finality, balance reconstruction, or nonce authority derived from local history;
- activity deletion, pruning, export, import, cloud synchronization, or portable recovery inclusion;
- production command registration, permission or capability changes, approval-flag changes, or Core
  compatibility-manifest changes.

## Storage separation

The exact-envelope store is a third private custody file set, separate from both:

- the activity journal, whose records remain display-only public metadata; and
- the reconciliation store, whose records remain linear submission-state authority without signed
  bytes.

The separation is deliberate. A signed envelope is public after publication but remains
privacy-sensitive and replayable by any holder. It must therefore never appear in ordinary journal
JSON, reconciliation JSON, logs, diagnostics, errors, support packages, crash messages, frontend
state, browser storage, or IPC.

The implementation must use fixed filenames beneath the canonical directory represented by the
existing `WalletCustodyPathAuthority`. No caller may supply or reconstruct a path.

Proposed fixed names:

- `wallet.signed-envelopes.v1.enc`
- `wallet.signed-envelopes.v1.head.json`
- random create-new staging names with a fixed `.wallet-signed-envelopes-stage-` prefix

Exact names are format identifiers. Changing them requires a storage-format review.

## Encrypted store format

The store is one bounded encrypted container plus one independently authenticated head. The
plaintext container has an exact versioned schema with unknown fields denied:

- schema: `vision-desktop-wallet-signed-envelopes`;
- version: `1`;
- wallet identifier;
- store generation;
- previous committed head authentication tag;
- entries sorted lexicographically by decoded 32-byte attempt identifier, subject to the permanent
  uniqueness invariants below.

Each entry contains one immutable identity section:

- random 32-byte submission attempt identifier, encoded as 64 lowercase hexadecimal characters;
- canonical transaction identifier;
- exact signed `VisionTransaction`;
- exact JSON request body bytes encoded as lowercase hexadecimal;
- domain-separated digest of those exact body bytes;
- compatibility-contract digest;
- authenticated reconciliation parent-head generation and tag;
- reserved next `Prepared` generation;
- immutable envelope commitment; and
- non-authoritative creation time.

The entry also contains one mutable retention state: `prepared`, `ambiguous`, or `accepted`.
Retention state and encrypted-container position are not part of entry identity or its commitment.

The exact body must equal `serde_json::to_vec(&transaction)` byte for byte. On every load, Rust must
recompute and compare:

- the canonical transaction identifier;
- the exact body bytes;
- the domain-separated body digest;
- the Ed25519 signature against `sender_pubkey`;
- all public transfer fields against the authenticated reconciliation or journal record being used;
  and
- the wallet public account derived inside the unlocked session.

Semantically equivalent JSON, reordered fields, re-encoded arguments, a different signature with
the same unsigned identifier, or a valid transaction belonging to another wallet must fail closed.

## Immutable envelope commitment

The immutable entry is committed with BLAKE3 derive-key mode using the exact context:

`com.vision.desktop.wallet-signed-envelope-commitment.v1`

The commitment input is a frozen binary encoding in this exact order:

1. four-byte big-endian commitment version `1`;
2. two-byte big-endian wallet-identifier byte length, followed by 1 through 64 validated ASCII bytes
   limited to alphanumeric, hyphen, and underscore;
3. decoded 32-byte attempt identifier;
4. decoded 32-byte canonical transaction identifier;
5. four-byte big-endian exact-body length, followed by the exact JSON body bytes;
6. decoded 32-byte existing signed-body digest;
7. decoded 32-byte compatibility-contract digest;
8. eight-byte big-endian authenticated reconciliation parent-head generation;
9. decoded 32-byte parent-head authentication tag; and
10. eight-byte big-endian reserved next `Prepared` generation.

Every integer is unsigned. Every fixed hexadecimal field is decoded before commitment. The wallet
identifier and body length are validated before allocation. No optional field, platform string,
serializer output other than the already retained exact body, or map iteration order enters this
encoding.

The 32-byte result is stored as exactly 64 lowercase hexadecimal characters. The commitment excludes:

- mutable retention state;
- encrypted-container generation, order, nonce, ciphertext, head, and staging identity;
- creation or observation time; and
- reconciliation phases after the reserved `Prepared` generation.

Changing retention state or republishing the encrypted container therefore cannot change the
commitment. Changing any transaction byte, signature byte, identity, compatibility contract, or
original reconciliation reservation must change it.

## Reconciliation reservation and two-way binding

Before envelope publication, the seed-authenticated reconciliation store must have an authenticated
committed head. The revised reconciliation format creates and verifies an authenticated genesis head
at generation zero before the first attempt; an absent optional head is no longer a valid starting
state for this extension.

While holding the existing transaction-wide exclusion, Rust must:

1. authenticate the current committed reconciliation head and capture its generation and exact tag;
2. reserve `current_generation.checked_add(1)` as the only valid next `Prepared` generation;
3. construct and publish the envelope commitment bound to that parent and reservation;
4. read back, decrypt, and verify the envelope entry and envelope head;
5. publish reconciliation `Prepared` only as the reserved generation, with the same attempt
   identifier, transaction identifier, signed-body digest, and envelope commitment; and
6. read back both stores and prove the cross-reference in both directions before continuing.

The reconciliation record stores the immutable envelope commitment, not a digest of the mutable
entry or encrypted container. The envelope entry stores the authenticated parent-head position and
reserved `Prepared` generation, not a phase observed after publication.
Every reconciliation phase for that attempt retains the same parent-head position, reserved
`Prepared` generation, and envelope commitment as immutable authenticated fields. Phase transitions
may change only the reviewed phase payload; they cannot rewrite the cross-store binding.

Recovery accepts only these cross-store cases:

- If the reconciliation head still equals the entry's authenticated parent and the reserved
  `Prepared` record does not exist, the entry is a proven pre-write orphan. It may be removed only
  through the reviewed no-Core orphan-cleanup authority.
- If an authenticated reconciliation head transition targets the reserved `Prepared` generation,
  recovery must first complete or roll back that existing reconciliation transition under its
  reviewed rules. The envelope may not be removed while the result is uncertain.
- If reconciliation is at the reserved `Prepared` generation or a later phase for that same
  attempt, its stored commitment must equal the envelope commitment and the envelope's parent and
  reservation must match the authenticated predecessor transition.
- A nonterminal reconciliation record with a missing or mismatched envelope blocks spending and
  reconciliation. An envelope claiming a reservation occupied by another attempt also fails closed.

After `ResolvedRecorded` and later attempts advance the reconciliation head, the authenticated
journal commitment described below becomes the durable historical association. The original
parent/reservation fields remain immutable evidence but are not rewritten to the newer head.

## Entry identity, uniqueness, and collision policy

The primary entry identity is the decoded 32-byte attempt identifier. Entries are stored in strictly
increasing bytewise attempt-identifier order. The authenticated plaintext must reject:

- a repeated or out-of-order attempt identifier;
- a repeated immutable envelope commitment; or
- a repeated canonical transaction identifier among any retained entries.

The transaction-identifier rule is permanent for accepted entries. The store permits at most one
`prepared` or `ambiguous` entry because the reconciliation store permits only one nonterminal
attempt. No transition replaces an entry's immutable section. `ambiguous` and `accepted` entries can
never be overwritten or automatically deleted.

A proven `ResolvedNotAttempted` entry may be removed only after both stores authenticate the exact
terminal transition. A future `ResolvedRejected` entry follows the same rule only if the separately
reviewed non-mutating rejection allowlist authorizes that result. Cleanup must finish and read back
before another attempt with the same transaction identifier can begin.

After constructing the unsigned transaction identifier, preview preparation must authenticate the
store and reject a collision before native confirmation. The same check is repeated after signing and
immediately before envelope publication. A collision with an unresolved attempt returns
`wallet_reconciliation_pending`; a collision with accepted history or an inconsistent authenticated
store returns `wallet_activity_unavailable`. Neither path reaches confirmation, seed signing, or a
network write.

This rule covers distinct attempt identifiers and distinct valid signatures that share one unsigned
transaction identifier. They cannot coexist. If an accepted envelope for that identifier exists, no
later signature or attempt may replace it.

## Durable journal association

The storage implementation requires journal schema version 3. Its authenticated `Submitted` event
adds exactly one internal field:

- `envelope_commitment_hex`: the immutable 64-character lowercase envelope commitment.

The journal event authentication chain and independent head cover this field. The internal
`WalletActivityRecord` retains it, but the existing public activity projection omits it. No body,
signature, attempt identifier, reconciliation position, or store path enters the journal or IPC.

`AcceptedSubmissionEvidence` must bind and consume the same commitment carried by the authenticated
reconciliation record and accepted envelope entry. Journal append must verify all three before
publication. Read-back must prove the journal transaction identifier, public fields, and commitment
match the accepted entry before reconciliation may reach `ResolvedRecorded`.

Receipt refresh selects an activity record by transaction identifier, reads its authenticated
commitment, and requires exactly one accepted envelope with both that identifier and commitment.
Identifier-only association is forbidden. A different commitment, missing entry, duplicate entry, or
valid alternative signature fails closed without changing the journal.

Journal version 2 does not contain this association and cannot qualify atomic wallet activation or
receipt refresh. Because custody commands remain unregistered, no automatic v2-to-v3 migration is
authorized. Version 3 activation requires a fresh empty journal or a separately designed,
independently reviewed migration.

## Confidentiality and authentication

The container must use the already pinned XChaCha20-Poly1305 primitive with:

- a dedicated encryption key derived only inside the unlocked wallet runtime from the wallet seed;
- a new domain-separation context used nowhere else;
- a fresh random 24-byte nonce for every publication;
- authenticated associated data binding schema, version, wallet identifier, store generation,
  previous head tag, and ciphertext length; and
- zeroizing ownership for plaintext, derived key material, serialized envelopes, and temporary body
  buffers.

The head must use a different seed-derived authentication subkey and domain. It records:

- schema and version;
- wallet identifier;
- monotonic generation;
- previous head authentication tag;
- ciphertext digest and exact length;
- container nonce;
- entry count; and
- committed or transition state.

Neither key, nonce, plaintext, signature, body, digest inventory, nor head authentication tag may
implement unrestricted `Debug`, `Display`, cloning, serialization across IPC, or logging.

The design does not claim TPM binding, machine binding, resistance to a fully compromised unlocked
Windows account, or detection of a coordinated rollback of both container and head. Coordinated
rollback remains a documented local-storage limitation and cannot create signing, submission, or
retry authority.

## Bounds

The implementation must enforce bounds before allocation:

- exact signed body: 1 through 65,536 bytes;
- transaction identifier and public keys: exactly 64 lowercase hexadecimal characters where the
  versioned contract requires them;
- signature: exactly 128 lowercase hexadecimal characters;
- attempt identifier and every digest: exact reviewed lowercase hexadecimal size;
- maximum entries: 10,000;
- maximum encrypted container: 64 MiB;
- maximum head: 8 KiB;
- exact object/array shapes with unknown and duplicate logical fields rejected before owned
  construction.

No automatic pruning is permitted. If the store reaches a bound, new spending must fail before
native confirmation, signing, or any network write. Existing activity remains readable and existing
receipt refresh remains available where its envelope is present.

## Filesystem publication

Every read and write must reuse the reviewed custody filesystem boundary:

- held, non-reparse directory-chain guards;
- handle-bound regular-file validation;
- restrictive current-user Windows ACL verification;
- random create-new staging files;
- bounded write, flush, read-back, decrypt, authenticate, and semantic verification;
- handle-based non-replacing first publication or atomic replacement;
- directory flush where supported by the existing reviewed primitive;
- independent head transition before container publication and committed head afterward; and
- the cross-session wallet process lease plus non-blocking in-process exclusion.

Path metadata checks followed by an unrelated path reopen are prohibited. Symlinks, junctions,
mount-point reparse data, hard-link count changes, non-regular files, path replacement, sibling-prefix
paths, and selected external paths must fail closed.

Interruption recovery may accept only one completely authenticated old or new container whose digest,
generation, and head transition agree. A partial file, orphan staging file, mismatched transition,
missing committed container, or unprovable generation blocks refresh and new spending.

## Authority model

The private implementation should introduce purpose-specific, non-forgeable authorities. Names are
illustrative but their separation is normative:

- `EnvelopeStoreAuthenticator`: seed-owned encryption/authentication authority for one wallet;
- `PreparedEnvelopeAuthority`: proves one exact envelope was durably stored before the submission
  record can reach `Prepared`;
- `EnvelopeRetentionAuthority`: permits only reviewed retention-state transitions;
- `EnvelopeRefreshAuthority`: permits one authenticated envelope read and one read-only observation;
- `ReceiptJournalAuthority`: permits only append/read-back of the resulting observation for an
  already authenticated journal record.

These types must have private fields and no unrestricted construction, cloning, formatting,
serialization, or ordinary caller closure over the seed. They must bind the wallet identity,
operation generation, revocation epoch, main-window owner, custody authority, transaction identifier,
and exact store generation.

`EnvelopeRefreshAuthority` must structurally carry no signing capability, `CoreWriteOnce`, POST path,
submission request body access outside the storage/receipt coordinator, retry authority, or recovery
export authority.

All storage and refresh work remains under the existing non-blocking transaction-wide exclusion.
Contention returns `wallet_operation_in_progress` immediately; it must not queue behind native UI,
Core I/O, store decryption, or journal publication.

## Submission integration and ordering

The private submission sequence must become:

1. Validate the continuously held wallet operation, Core generation, signed artifact, canonical
   transaction, exact body, signature, wallet account, and compatibility contract.
2. Construct an envelope entry in zeroizing memory.
3. Encrypt, publish, read back, decrypt, and verify the entry and head.
4. Consume `PreparedEnvelopeAuthority` while publishing reconciliation `Prepared`, binding the
   attempt identifier, exact body digest, immutable envelope commitment, authenticated parent-head
   position, and reserved `Prepared` generation.
5. Revalidate all runtime and Core authorities.
6. Publish reconciliation `MayHaveBeenSubmitted` and only then release the one existing Core-write
   capability for the one network attempt.
7. Move the envelope retention state to `ambiguous` whenever the durable reconciliation phase can no
   longer prove that no write occurred.
8. Move it to `accepted` only after exact Core acceptance is authenticated and durable
   `AcceptedRecordingPending` is committed.
9. Retain the accepted entry after journal append and `ResolvedRecorded` so later receipt refresh can
   prove exact-envelope equality.

The reconciliation phase is authoritative for whether a write may have occurred. Envelope retention
state is a conservative storage label, never submission or retry authority. If a crash leaves
reconciliation at `MayHaveBeenSubmitted` while the envelope still says `prepared`, restart recovery
must treat the effective state as ambiguous and durably advance the envelope label before any other
wallet operation. If reconciliation proves acceptance while the envelope label lags, recovery must
treat it as accepted and may only repair the label or complete the authenticated journal transition.
A lagging envelope label can never downgrade ambiguity or acceptance.

If step 3 fails, no reconciliation `Prepared` record and no network write authority may be produced.

If an interruption leaves a valid envelope entry before reconciliation `Prepared`, it is an orphan
that can be removed only after authenticated discovery proves that no matching reconciliation record
or journal record exists. Orphan cleanup carries no Core access and no signing or write authority.

If a pre-write failure commits `ResolvedNotAttempted`, cleanup may remove that exact envelope only
after the terminal reconciliation head is authenticated. If a future reviewed rejection allowlist
commits `ResolvedRejected`, the same rule applies. The current definitive-rejection allowlist remains
empty.

An `ambiguous`, `accepted`, or journal-associated envelope must never be deleted automatically. Store
unavailability after `MayHaveBeenSubmitted` preserves `outcome_unknown` and the spending interlock.
Store unavailability after acceptance preserves `accepted_recording_pending` or the authenticated
journal record; it never downgrades acceptance or permits resubmission.

The reconciliation schema must be versioned to bind the immutable envelope commitment,
authenticated parent-head position, and reserved `Prepared` generation. The journal schema must be
versioned to bind that same commitment to accepted activity.
Older internal schemas cannot be silently accepted or upgraded. Because wallet commands remain
unregistered and no production custody data exists for this format, the first implementation may
require an empty store, but that assumption must be proven again before activation.

## Receipt-refresh operation

`wallet_refresh_transaction_observation` remains the only planned public shape, but this tranche
implements only its private Rust path. The operation must:

1. Validate the exact request envelope and canonical transaction identifier using the existing
   transaction boundary.
2. Acquire the transaction-wide gate without blocking.
3. Require the matching wallet to be unlocked and issue one purpose-specific refresh operation.
4. Authenticate the journal and require exactly one local activity record for the identifier.
5. Authenticate and decrypt the envelope store and require exactly one accepted entry matching both
   the journal transaction identifier and journal envelope commitment.
6. Revalidate the stored transaction, body, digest, signature, public fields, wallet account,
   journal record, and compatibility contract.
7. Obtain a fresh supervisor-issued `CoreConnectionAuthority` for a supported private-loopback Core.
8. Validate the authority and exact process generation before the first read.
9. On that fresh proxy-free peer-proven connection, perform only bounded status/canonical-tip and
   `GET /transaction/:txid` reads required by the reviewed receipt parser.
10. Require the returned exact signed envelope and body digest to match the stored expectation.
11. Revalidate Core identity, generation, compatibility, runtime revocation, and operation authority
    after reads and before storage.
12. Classify the change with the existing receipt policy.
13. Append the observation through the seed-owned journal authority, then read back and authenticate
    the journal/head.
14. Revalidate runtime authority before projecting one bounded public response.

There is no POST, retry, redirect, proxy, DNS host, cookie, ambient credential, cross-generation
connection reuse, or automatic refresh. A refresh operation performs at most one transaction lookup.
If separate status and lookup reads require separate fresh connections, both must prove the same
supervisor generation and compatibility contract before and after use.

## Observation rules

The existing receipt rules remain normative:

- `NotFound` means `not_observed`; it is not rejection and does not permit another send.
- A found transaction without a canonical block reference is `pending`.
- A found exact transaction with a valid canonical block reference is mined with an observed
  confirmation count.
- Movement to another block, movement back to pending, or disappearance after a prior observation is
  a reorganization or observation loss, not a transaction failure.
- Fifty confirmations may be shown as `mined_high_confidence` but never as final or irreversible.

An unchanged observation does not append another journal event. The response may return the existing
authenticated record and `unchanged`; it must not fabricate a newer stored observation time.

## Public projection and errors

The eventual command response remains limited to the existing public activity-record projection plus
one fixed change classification:

- `first_observation`;
- `unchanged`;
- `pending_to_mined`;
- `confirmations_advanced`;
- `reorganized`; or
- `observation_lost`.

No signed body, signature, exact envelope, body digest, compatibility digest, store generation,
attempt identifier, file path, Core endpoint, PID, process generation, peer proof, raw Core response,
or internal timing enters the response.

The private implementation uses only the existing normative fixed vocabulary:

- malformed request or identifier: `invalid_request`;
- identifier absent from the authenticated journal: `wallet_transaction_unknown`;
- missing, corrupt, unauthenticated, incompatible, or unavailable envelope/journal storage:
  `wallet_activity_unavailable`;
- unsupported Core contract: `wallet_core_compatibility_unavailable`;
- unavailable or recovering Core: `wallet_core_unavailable` or `wallet_core_recovering`;
- malformed, inconsistent, or exact-envelope-mismatched Core result:
  `wallet_core_response_rejected`;
- operation contention: `wallet_operation_in_progress`; and
- lifecycle or runtime revocation: `wallet_runtime_unavailable`.

No new error code is authorized by this design. Error text and diagnostics must not include transaction
values, paths, serialized objects, Core bodies, or cryptographic material.

At minimum, these conditions fail closed without changing journal state:

- missing or unauthenticated envelope;
- wrong wallet, journal, transaction, signature, body, digest, generation, or compatibility binding;
- unsupported storage schema;
- storage/head interruption or rollback mismatch;
- Core unavailable, restarted, replaced, stale, or peer proof failure;
- malformed, oversized, inconsistent, or mismatched Core response;
- runtime revocation, lifecycle transition, contention, panic, or journal publication failure.

Core unavailability is not transaction failure. Journal failure after a valid read does not erase the
previous authenticated observation.

## Reload, restart, and migration behavior

The exact-envelope store is discovered only after matching-wallet unlock. React, reducer state,
browser storage, and caller-provided metadata are never discovery inputs.

On application restart:

- nonterminal reconciliation discovery runs first and retains the existing spending interlock;
- any matching envelope is authenticated before exact lookup;
- accepted-recording-pending remains a journal-only recovery operation;
- an accepted journal record without a valid exact envelope remains visible but refresh-unavailable;
- an envelope without a journal record cannot manufacture activity; and
- no discovery path can submit, re-sign, retry, or advise resubmission.

Portable wallet recovery contains only the already reviewed encrypted wallet recovery material. It
does not contain activity, reconciliation state, or exact envelopes. Restoring a wallet therefore
does not claim to restore local transaction history. Import/export or migration of local history
requires a separate design and review.

## Diagnostics and support packages

The entire envelope container, head, staging files, decrypted buffers, signatures, exact bodies,
digests, attempt identifiers, and store metadata are excluded from diagnostics and support packages.

Tests must place unique canaries in:

- sender and recipient fields;
- signature and exact body;
- ciphertext and associated data;
- attempt and transaction identifiers;
- errors, caught panics, and poisoned-lock paths; and
- every support-package input and generated archive entry.

No canary or fragment may appear in logs, stdout/stderr, crash text, fixed errors, diagnostics,
support manifests, frontend state, or IPC.

## Mandatory adversarial tests

The private storage implementation must deterministically cover:

- first publication, replacement, read-back, restart, and wrong-wallet unlock;
- wrong encryption key, authentication tag, head tag, nonce, associated data, generation, previous
  head, length, entry count, and ciphertext digest;
- truncation, extension, malformed JSON, unknown fields, duplicate logical fields, oversized body,
  excessive entry count, and allocation-before-bound checks;
- mutation of every `VisionTransaction` field, argument byte, signature byte, exact-body byte,
  transaction identifier, compatibility digest, attempt identifier, reconciliation generation, and
  journal field;
- multiple retained attempt identifiers, duplicate attempt identifiers, duplicate commitments,
  out-of-order entries, and duplicate transaction identifiers;
- multiple valid signatures sharing one unsigned transaction identifier, including collision
  rejection before confirmation, signing, and network access;
- reparse points, hard links, path swaps, non-regular files, ACL drift, sibling-prefix paths, and
  cross-session contention;
- interruption at every head transition, container write, flush, publication, read-back,
  reconciliation bind, acceptance transition, journal append, and cleanup checkpoint;
- both directions of the envelope-parent/reserved-generation to reconciliation-record binding,
  including mismatched parent tags, occupied reservations, interrupted `Prepared` transitions, and
  phase changes that try to rewrite the commitment;
- journal transaction identifier and commitment association, including right identifier/wrong
  commitment, right commitment/wrong identifier, missing accepted entry, duplicate accepted entries,
  and later unrelated attempts leaving historical associations unchanged;
- orphan-before-Prepared cleanup and refusal to remove ambiguous or accepted entries;
- store-full refusal before confirmation, signing, or network write;
- concurrent and reordered calls returning immediately without queued execution;
- runtime revocation, Core stop/restart/replacement, main-window destruction, lock, sleep, shutdown,
  and caught panic at every refresh stage;
- `NotFound`, pending, mined, advancing confirmations, block replacement, return to pending, and
  observation loss;
- wrong echoed identifier, wrong transaction, wrong signature, malformed block hash, block above tip,
  body-size limit, timeout, and Core generation change between every read;
- unchanged observations causing no journal growth;
- journal failure preserving the previous observation and accepted status;
- activity record without envelope, envelope without activity record, schema-version mismatch, and
  coordinated-store rollback limitation; and
- privacy canaries across all non-emitting boundaries.

Tests must also prove there is no route from the refresh authority to signing, `CoreWriteOnce`, POST,
submission, replacement, recovery export, or raw envelope projection.

## Staged implementation and review sequence

1. Independently review this design at its exact commit and tree.
2. Implement only the encrypted authenticated envelope store, reconciliation binding, and exhaustive
   storage/interruption tests in private Rust modules. Keep refresh fail-closed.
3. Submit that exact storage commit for independent security review.
4. Implement the private read-only refresh coordinator and receipt/journal tests. Keep it unregistered.
5. Submit that exact refresh commit for independent security review.
6. Re-run the full Rust, Tauri-authority, WebView-isolation, frontend, production-build, filesystem,
   lifecycle, privacy, and Windows interruption matrices.
7. Only then may a later atomic-exposure candidate incorporate the private boundary into its frozen,
   unpublished artifact and proceed through the already reviewed publication gate.

No stage may set `duplicate_key_rejection_proven` or any security approval flag, register a command,
grant a permission, add a frontend invoke, relax the Core manifest, or modify Vision-Core.

## Private receipt-refresh implementation status

The staged private receipt-refresh tranche is now implemented for independent review. The
implementation remains unregistered and production-inert. It:

- issues one linear runtime `Refresh` permit only for the unlocked matching wallet and main-window
  owner;
- authenticates exactly one journal-v3 record and its exact accepted encrypted-envelope
  commitment before requesting supervisor-issued Core authority or performing any Core read;
- revalidates the immutable envelope, journal association, compatibility digest, operation
  generation, revocation epoch, owner, wallet identity, and active Core fingerprint;
- uses a read-only Core trait that exposes status and one exact transaction lookup but cannot carry
  `CoreWriteOnce` or call submission;
- parses and verifies the complete signed envelope returned by Core before classifying the receipt;
- records only authenticated public observations, with read-back verification; and
- preserves an identical observation without journal growth or a fabricated observation time.

The implementation has no Tauri registration, permission, capability, frontend invoke, enabled
approval constant, production Core manifest change, signing authority, submission authority, retry,
replacement, recovery export, or Vision-Core change. Independent implementation review remains
mandatory before this tranche may be incorporated into any frozen atomic-exposure candidate.

## Approval requested

Independent review is requested only for the design of:

- encrypted, authenticated, wallet-bound exact-envelope retention;
- its atomic binding to existing reconciliation and journal ordering; and
- one private, bounded, read-only receipt-refresh coordinator.

Implementation, exposure, activation, publication, and Vision-Core changes remain out of scope.
