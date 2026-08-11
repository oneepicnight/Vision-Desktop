# Wallet Atomic Exposure Design

## Status and review boundary

This document is a design candidate only. It changes no Rust or frontend behavior and does not
authorize a wallet command, invoke registration, permission, capability, frontend wrapper, security
approval flag, supported-Core manifest entry, signing activation, submission activation, recovery
export, or Vision-Core change.

The current private implementation baseline is commit
`653630cf2c5a60ee1f80343c37fd53e91299527c`, tree
`d80c8519eb3c2601066a40e9f3a226847cdab7b3`. It is the approved, private, unregistered receipt-
refresh correction built on the complete private wallet implementation chain. The accepted Layer B
evidence is commit
`b294b6bfdae28ca7674950c12a159e4d8cdb1e9b`, tree
`61120ad35fee58c756d2aa910ddcc0c767e3aaad`. Its report SHA-256 is
`D83B950A576EABA11B3997577E132C925CA923B392249E75C17AC363F5E4FA02`; its manifest SHA-256 is
`432C41AAEF4059F39235584B08B849DC6923811D15015A7FC36B26D161FA2D8B`. Independent review accepted
all 562 Layer B cases for that exact implementation, dependency set, Windows environment, and
WebView2 runtime.

The independently approved private implementation anchors incorporated by the current baseline are:

- lifecycle command boundary: commit `58e0843d57fec6a7b47004ed868d57df910d61d9`, tree
  `0498bd1f298c6e8648848f282fd6faa22bc6d4c2`;
- native transaction confirmation: commit `a54bdf06be75b762755b96c87294c82af1dd6920`, tree
  `42b5332fb163bdd187c44b658f3626a32c31b843`, with accepted physical evidence commit
  `5e6acbc7c8e7003c11b0520cf42bc4729118d278`, tree
  `ffd79b1e841fb7c0ae6ae4ac28618b41ab207328`;
- private signing: commit `7a3f6f3ceaf8f24d35244a7c103dc98ea600f7fd`, tree
  `6a2a0a5e4ed6120ba7706d0db685ae207008198a`;
- private one-attempt submission and reconciliation: commit
  `31c95a156bc98a3e7b1bb7d549d2b03670864129`, tree
  `32a44de596f446c139a4e2cb0df343da51af6580`;
- private transaction command boundary: commit
  `b108d54a88f658219ec5ab6fce5a07bbf316ce0f`, tree
  `394503ea8d7d0abff9c9cd36f2d58426f4196d74`;
- authenticated envelope storage and journal-v3 association: commit
  `239f64f0c4c8fb483ea1d1be8a2163ea9ee206db`, tree
  `136c742f275538e58b51beed600ea8df75194d73`; and
- private receipt refresh and its final correction: current baseline commit
  `653630cf2c5a60ee1f80343c37fd53e91299527c`, tree
  `d80c8519eb3c2601066a40e9f3a226847cdab7b3`.

These approvals establish private implementation baselines only. They do not authorize production
registration, activation, distribution, or a supported Core contract. Any later source change to an
approved private boundary invalidates that anchor and requires focused re-review before atomic
integration can continue.

That acceptance closes the raw Tauri/WebView transport qualification gate. It does not itself make
the production policy true. `duplicate_key_rejection_proven` and the lifecycle, signing, and
submission security approval constants remain `false`; the current Core manifest still cannot
construct production wallet authority; no wallet command is registered.

Normative ownership is deliberately narrow:

- `WALLET_LIFECYCLE_TAURI_EXPOSURE_DESIGN.md` remains the sole normative lifecycle command,
  lifecycle request, lifecycle response, and lifecycle error contract.
- `WALLET_TRANSACTION_AUTHORITY_BOUNDARY.md` remains normative for transaction derivation, preview,
  native confirmation, signing, receipt confidence, and Core authority.
- `WALLET_PRIVATE_TRANSACTION_SUBMISSION_DESIGN.md` remains normative for one-attempt submission,
  durable ambiguity, reconciliation, and authenticated journal transitions.
- `WALLET_AUTHENTICATED_ENVELOPE_STORAGE_DESIGN.md` remains normative for encrypted exact-envelope
  retention, journal-v3 commitment association, and private receipt refresh.
- This document is the sole normative contract for the combined command inventory, atomic exposure
  sequence, ACL parity, frontend authority, release rollback, and final activation gate.

If these documents conflict, the more restrictive behavior applies and exposure remains blocked
until the conflict is corrected and reviewed.

## Objective

Expose a complete, usable wallet only after Vision Desktop can create or restore custody, unlock and
lock it safely, prepare and display an exact transfer, obtain trusted native approval, sign inside
Rust, submit once through a proven private Core, preserve ambiguous outcomes, and present
authenticated local activity.

There is no lifecycle-only release. Creating an address invites deposits and mining rewards and
therefore creates a reasonable expectation that those funds can be recovered and spent. The first
production wallet surface must be one reviewed atomic release.

## Exact first-release command inventory

The atomic surface contains exactly twelve commands. No wildcard or default wallet command set is
permitted.

Lifecycle commands retain their already reviewed contracts:

1. `wallet_get_status`
2. `wallet_select_recovery_destination`
3. `wallet_create`
4. `wallet_select_recovery_source`
5. `wallet_restore`
6. `wallet_unlock`
7. `wallet_lock`

Transaction commands are fixed by this design:

8. `wallet_prepare_transfer_preview`
9. `wallet_cancel_transfer_preview`
10. `wallet_confirm_and_submit_transfer`
11. `wallet_list_activity`
12. `wallet_refresh_transaction_observation`

No command exposes a seed, password, recovery credential, selected path, vault path, activity path,
reconciliation path, DPAPI blob, session token, operation permit, activation proof, Core port, PID,
process generation, peer proof, unsigned transaction, signature, signed envelope, canonical payload,
HTTP response, retry control, replacement policy, arbitrary module or method, arbitrary file, shell,
clipboard, dialog, or network authority.

## Whole-envelope transaction contract

Every command uses the already reviewed complete `InvokeBody` extraction mechanism. Parsing begins
only after the fail-closed guard is armed. The declared generated-wrapper name must exactly equal the
framework-provided invoked command. Raw bodies, non-object JSON, unknown fields, duplicate textual
keys, missing fields, wrong-case fields, secret-like fields, and oversized values fail closed.

All transaction requests use one of these exact top-level objects:

- `wallet_prepare_transfer_preview`:
  `{"request":{"recipient":"<64 lowercase hex>","amount":"<bounded plain decimal>"}}`
- `wallet_cancel_transfer_preview`:
  `{"request":{"preview_handle":"<bounded opaque handle>"}}`
- `wallet_confirm_and_submit_transfer`:
  `{"request":{"preview_handle":"<bounded opaque handle>"}}`
- `wallet_list_activity`: `{}`
- `wallet_refresh_transaction_observation`:
  `{"request":{"transaction_id":"<64 lowercase hex>"}}`

The frontend never supplies sender, balance, nonce, tip, fee, transaction identifier for a new
transfer, Core identity, session identity, window identity, retry, or replacement fields. The
refresh command accepts only an identifier already present in the authenticated local journal. It
cannot become an arbitrary Explorer lookup.

All values are bounded before allocation. The exact byte limits must be constants in the private
transaction command boundary and covered by generated-wrapper tests. A future limit change is a
command-contract change requiring review.

## Transaction response contract

Responses are pre-serialized inside the fail-closed boundary. Protocol-sized integers that can
exceed JavaScript's exact integer range cross IPC as canonical decimal strings. No floating-point
amount enters the boundary.

`wallet_prepare_transfer_preview` returns only public display data:

- opaque `preview_handle`;
- complete sender and recipient addresses;
- display amount and raw amount as decimal strings;
- charged fee, maximum fee, total debit, and balance as decimal strings;
- nonce and canonical tip height as decimal strings;
- canonical transaction identifier and tip hash;
- reviewed compatibility-contract and status-version identifiers;
- bounded monotonic `data_age_ms` and `expires_after_ms`; and
- the reorganization warning.

It does not return the unsigned transaction, canonical argument bytes, Core fingerprint, process
identity, signature, or signed body.

`wallet_cancel_transfer_preview` returns exactly `{"state":"cancelled"}` after permanent handle
consumption. Cancellation is idempotent only at the UI layer: replay at the authority layer returns
the fixed invalid/stale-handle error and never recreates a preview.

`wallet_confirm_and_submit_transfer` consumes the handle before native confirmation. It returns one
of these public outcomes:

- `{"state":"accepted","transaction_id":"<64 lowercase hex>","observation":<public observation>}`
  only after Core acceptance and authenticated local activity recording both complete;
- `{"state":"accepted_recording_pending","transaction_id":"<64 lowercase hex>"}` when exact Core
  acceptance is proven but authenticated local activity recording has not completed;
- `{"state":"outcome_unknown","transaction_id":"<64 lowercase hex>"}` only when the one permitted
  submission attempt may have reached Core but exact acceptance or rejection cannot be proven; or
- `rejected`: only when a later exact compatibility contract has a separately reviewed,
  non-mutating rejection allowlist entry.

`accepted_recording_pending` is known acceptance, not ambiguity. The UI must say that Core accepted
the transaction, local activity recording is pending, and the transaction must not be submitted
again. Only the reviewed authenticated journal-completion transition may resolve it to `accepted`.
It cannot create new signing or Core-write authority.

`outcome_unknown` has exactly the tagged success representation above. It is never also returned as
an IPC error. The UI must say that the outcome is genuinely ambiguous, that no automatic or manual
resubmission should occur, and that read-only reconciliation is required.

Cancellation, native-window failure, runtime revocation, Core replacement, signing failure,
storage failure before a write, and invalid response are fixed errors, not success states. Once the
durable state says the write may have occurred, the command must preserve `outcome_unknown`; it may
not downgrade ambiguity to ordinary failure. Once exact Core acceptance is proven, a later journal
failure must preserve `accepted_recording_pending`; it may not downgrade known acceptance to
`outcome_unknown` or an error. Signed bytes never cross IPC in any outcome.

`wallet_list_activity` returns at most the newest 100 authenticated local public records, newest
first, plus `history_complete: false`. Each record contains only transaction identifier, complete
sender and recipient addresses, raw amount as a decimal string, nonce/tip/fee-limit as decimal
strings, bounded public time values, and the conservative current observation presentation. Current
presentation is limited to `not_observed`, `pending`, `mined_observed`, or
`mined_high_confidence`; none claims irreversible finality. The fixed limit, local scope, and
explicit incompleteness avoid implying chain-wide history.

After a matching wallet is unlocked, this command also performs bounded authenticated discovery of
the one canonical reconciliation store. Its response includes exactly one
`pending_reconciliation` field whose value is `null` or one of:

- `{"state":"accepted_recording_pending","transaction_id":"<64 lowercase hex>"}`; or
- `{"state":"outcome_unknown","transaction_id":"<64 lowercase hex>"}`.

The field is derived from authenticated Rust storage on every call; it is not restored from React,
browser persistence, or the general Desktop reducer. A reload, navigation, main-window replacement,
application restart, or explicit UI-state clearing therefore cannot erase the durable warning. The
Wallet UI must call `wallet_list_activity` after every successful matching-wallet unlock and after a
reload or restart before it enables preview or submission controls.

Discovery uses the existing matching-wallet, canonical-custody, restart-only reconciliation permit.
That permit carries no signing, submission, Core-write, retry, or replacement authority:

- `MayHaveBeenSubmitted` is reported as `outcome_unknown`. When a supported Core authority is
  available, the same call may perform the reviewed exact-envelope read-only lookup. `NotFound`,
  unavailable or recovering Core, timeout, peer/generation failure, or any unproven response leaves
  the durable state and public outcome unchanged. It never submits again.
- `AcceptedRecordingPending` first attempts only the reviewed authenticated journal-completion
  transition. Success resolves the reconciliation record to recorded acceptance and returns the new
  activity record with `pending_reconciliation: null`. Journal unavailability or write failure
  preserves the durable phase and returns `accepted_recording_pending` again. Core connectivity is
  neither required nor consulted for this journal-only completion.
- Prepared or terminal cleanup follows the reviewed restart state machine and cannot manufacture a
  public acceptance, ambiguity, signature, or write permit.

Any authenticated nonterminal reconciliation record blocks `wallet_prepare_transfer_preview` and
`wallet_confirm_and_submit_transfer`. They return the fixed `wallet_reconciliation_pending` error
before creating or consuming new transaction authority. The block remains through reload, restart,
Core unavailability, and repeated activity queries until the reviewed state machine reaches a
terminal phase. No UI dismissal can clear it.

`wallet_refresh_transaction_observation` operates only on one authenticated local record for which
Rust retains the reviewed exact-envelope lookup expectation. It returns the updated public activity
record and a change classification that may include `reorganized` or `observation_lost`. If exact
signed-envelope equivalence cannot be proven from authenticated local state, the command fails
closed; transaction identifier, nonce movement, or a Core `NotFound` result is insufficient.

The approved private implementation authenticates the journal-v3 record and immutable envelope
commitment before requesting Core authority. It then uses only a fresh peer-proven read-only Core
connection, validates the complete signed envelope and Core generation at every required
checkpoint, and authenticates and reads back the resulting public journal observation before it
escapes. Local eligibility failure requests zero Core authority. Unchanged observations do not grow
the journal or fabricate a new observation time. No refresh authority can sign, submit, retry,
replace, export recovery material, or project raw envelope bytes.

This approval closes the former private storage and refresh implementation blocker. Production use
remains blocked by the atomic registration, supported-Core, frozen-artifact, and publication gates
in this document. Identifier-only journal association remains prohibited.

## Fixed transaction errors

The private transaction boundary must define one closed mapping from existing reviewed Rust errors
to fixed IPC codes. It must include, at minimum:

- `invalid_request`
- `invalid_window`
- `wallet_activation_unavailable`
- `wallet_runtime_unavailable`
- `wallet_unavailable`
- `wallet_operation_in_progress`
- `wallet_core_compatibility_unavailable`
- `wallet_core_unavailable`
- `wallet_core_response_rejected`
- `wallet_core_recovering`
- `wallet_account_unavailable`
- `insufficient_balance`
- `wallet_amount_arithmetic_rejected`
- `wallet_preview_unavailable`
- `wallet_confirmation_cancelled`
- `wallet_reconciliation_pending`
- `wallet_activity_unavailable`
- `wallet_transaction_unknown`

Errors contain exactly `{"code":"<fixed-code>"}`. They contain no free-form dependency message,
path, configuration, endpoint, PID, handle, account payload, transaction body, signature, timing,
secret, or ciphertext. The final exact mapping requires independent review with the private
transaction command boundary; ordinary callers cannot construct a new error string.

## Single Rust authority boundary

The eventual managed state owns one wallet runtime, lifecycle adapters, supervisor reference, and
exact main-window native identity. Lifecycle and transaction command adapters must share the same
non-forgeable `WalletExposureAuthority`; there is no second store or parallel exposure authority.

Runtime authority issuance in the exact unpublished release candidate requires all of the following
simultaneously:

1. The accepted whole-envelope transport mechanism and duplicate-key proof are compiled for the
   exact pinned Tauri/Wry/WebView2 release, and the accepted Layer B identity matches the release
   manifest. The production `duplicate_key_rejection_proven` policy becomes `true` only in the
   complete unpublished atomic candidate; it is never changed independently.
2. The exact supported private-loopback Core release and peer-binding contract are present in the
   Desktop compatibility manifest.
3. The held Core process, generation, manifest, literal loopback endpoint, and connected socket peer
   satisfy `CoreConnectionAuthority` where required.
4. The independent lifecycle, signing, and submission approval constants are true for the exact
   reviewed release tree.
5. The single-process wallet lease, supported single-interactive-session policy, main-window
   identity, lifecycle hooks, panic policy, storage roots, and support-package exclusions are valid.
6. The complete twelve-command inventory, AppManifest, generated permissions, capability, frontend
   wrappers, routes, documentation, and packaged binary pass exact parity checks.

`WalletExposureAuthority` has private fields and no `Clone`, `Copy`, `Debug`, formatting,
serialization, deserialization, or ordinary constructor. A boolean, feature flag, command name,
window label, frontend state, or successful status call is never authority. Each command validates
the exposure, live window generation, revocation epoch, and operation-specific authority at entry
and at the deepest irreversible transition.

Production construction remains impossible until all conditions are implemented and reviewed. The
accepted Layer B result must not be represented by manually changing
`WholeEnvelopeTransportPolicy::production()` in isolation.

The current private baseline intentionally satisfies none of the production-construction shortcut
conditions: all three approval constants and the production duplicate-key policy remain `false`, no
wallet wrapper is registered, and the current Core manifest cannot issue wallet production
authority. Documentation approval cannot change those facts.

## Release-candidate authority and publication gate

Final evidence acceptance is a distribution gate, not an input to runtime authority construction.
This distinction prevents a circular qualification requirement without introducing a test bypass.

After independent pre-implementation approval, the complete atomic registration diff is committed
to an unpublished release-candidate tree. That tree contains the final twelve commands, production
transport policy, three reviewed approval constants, exact Core compatibility entry, permissions,
capability, frontend, packaging, and runtime authority logic. The final distributable Windows
artifact is built and signed once from that exact tree. Its commit, tree, lockfile, source,
configuration, signature, and full-file hash manifest are frozen before any wallet qualification
begins.

The candidate uses its ordinary production authority path during qualification. There is no test
authority, environment override, alternate command inventory, hidden permission, manual flag
change, debugger mutation, or developer bypass. Its commands work only because the exact candidate
already contains the reviewed production gates and supported Core contract that would ship.

Independent reviewers then qualify that exact frozen artifact and accept or reject its evidence. A
Failed or Inconclusive result permanently rejects that artifact for distribution. Corrections create
a new commit, tree, artifact, and complete affected evidence set.

Successful evidence acceptance authorizes publication of only the byte-identical artifact whose
hash was qualified. Publication metadata records the accepted artifact hash but does not rebuild,
re-sign, patch, reconfigure, or otherwise mutate it. Any byte, configuration, dependency,
permission, capability, manifest, frontend, flag, or packaging change creates a new artifact and
reopens qualification. Thus the publication gate is externally enforced release authorization,
while the runtime security boundary remains identical before, during, and after qualification.

## Atomic registration and ACL transaction

The final release tranche changes every exposure surface together or none of them:

- register exactly twelve generated wrappers in the production invoke handler;
- add exactly twelve command names to `tauri_build::AppManifest`;
- generate one narrow permission for each command;
- add only those permissions to the existing Windows `main-desktop` capability;
- keep the capability target exactly `main` with no `remote.urls`;
- add exact wrappers only in `src/services/coreApi.ts`;
- add public-intent wallet UI and feature-local public state;
- set the three approval constants only in the independently reviewed release tree;
- add the exact supported Core compatibility entry without relaxing any older contract;
- add release, package, ACL, source-scan, and end-to-end evidence; and
- update user and security documentation to match the exact behavior.

No second wallet capability is created. No wildcard, default command set, dialog, filesystem, HTTP,
shell, clipboard, updater, process, or broad plugin permission is granted to the WebView. Native
recovery selection and native secret/confirmation windows remain Rust-only.

Static parity tests compare the invoke handler, AppManifest, generated permissions,
`main-desktop`, command-boundary vocabulary, `coreApi.ts`, frontend routes, and this inventory.
Missing, extra, differently cased, duplicated, or stale entries fail the build. Runtime startup also
fails closed if the packaged qualification identity or compatibility identity differs.

Tauri registration is compile-time, so runtime revocation cannot remove a wrapper. Every registered
wrapper must therefore remain inert without live Rust authority and return only a fixed failure.
There is no hidden developer bypass, environment-variable override, mock production authority, or
partial lifecycle mode.

## Frontend boundary

React may hold only public intent and public presentation:

- wallet label and public wallet identifier;
- public address and locked/unlocked availability state;
- recovery-selection availability, never the selected path;
- recipient and amount draft;
- public preview fields and opaque preview handle;
- public transaction identifier, conservative outcome, and authenticated activity presentation;
- one bounded pending reconciliation presentation restored from authenticated Rust storage; and
- bounded loading, cancellation, and fixed error code.

Passwords and portable recovery credentials are collected only in Rust-owned native controls.
Selected paths remain behind purpose-bound Rust tokens. React never receives, stores, logs,
serializes, reduces, persists, or renders secret material, native handles, signed bytes, or Core
authority.

All Tauri calls remain in `src/services/coreApi.ts`. Components and hooks do not import Tauri APIs.
Wallet capability state remains outside the general Desktop reducer and event stream. Public
feature-local presentation state is cleared on lock, reload, navigation, main-window destruction,
session lock, sleep, process lease loss, and application exit. No password input is added to the
WebView, and no browser storage is used for wallet state.

Clearing frontend state does not clear reconciliation state. After reload, restart, or a new
matching-wallet unlock, the frontend must keep preview and submission controls disabled until
`wallet_list_activity` completes and supplies the current authenticated
`pending_reconciliation` value. Failure or cancellation of that query leaves spending controls
disabled; it never means that no pending record exists.

The UI must preserve the current distinction between a configured mining reward address and a
Desktop-custodied wallet. Creation or unlock does not automatically edit mining configuration,
copy an address, fund an account, start Core, start mining, or submit a transaction.

## Startup, revocation, panic, and rollback

The panic policy, single-instance enforcement, process wallet lease, wallet runtime, supported
Windows lifecycle handlers, storage authority, and main-window identity initialize before wallet
exposure state. Any failure leaves every wrapper inert. If authority invalidation cannot be proven
after panic or security-boundary failure, the process terminates.

Explicit lock, idle timeout, console lock, sleep, main-window reload/destruction, process shutdown,
wallet-lease loss, runtime poisoning, Core stop/restart/replacement, or compatibility change revokes
all previews, confirmations, signing permits, submission permits, path tokens, and unlocked session
authority. Durable reconciliation data is preserved where a write may have occurred; revocation
never triggers automatic resubmission.

Rollback is the complete reversal of the atomic exposure release. It removes all twelve production
wrappers, AppManifest entries, permissions, capability entries, frontend invokes/routes, and true
approval constants together. It does not delete a user's vault, recovery artifact, authenticated
activity, or reconciliation evidence. A release must retain a reviewed offline recovery procedure
before rollback can strand custody.

No operator may repair a failed gate by changing only one constant, permission, manifest entry, or
frontend route. A correction receives a new exact-commit review and repeats affected qualification.

## Approved private baseline and composition constraints

The private `WalletLifecycleCommandBoundary`, `WalletTransactionCommandBoundary`, authenticated
envelope store, journal-v3 association, receipt-refresh engine, native confirmation, signing, and
one-attempt submission/reconciliation implementations have each completed their authorized private
review tranches. They remain unregistered and unreachable from production Tauri IPC.

Atomic integration must compose those exact approved boundaries; it may not recreate their logic in
wrappers, managed state, frontend code, or an alternate service. In particular:

- all twelve wrappers share one `WalletExposureAuthority`, one `MainWalletWindowAuthority`, one
  `WalletRuntimeState`, one custody-path authority, and the existing supervisor;
- lifecycle and transaction envelopes use the same complete-message parser, exact command-name
  binding, duplicate-key policy, fail-closed guard, fixed-response construction, and epoch checks;
- transaction refresh uses the approved local-first receipt path and cannot request Core authority
  until the matching wallet, journal-v3 record, exact envelope, commitment, and compatibility
  expectation authenticate;
- no wrapper may accept a secret, selected path, sender, nonce, fee, Core identity, retry,
  replacement, signed body, or authority object from React;
- no wrapper, service, component, or test helper may create a second signing, submission,
  reconciliation, recovery-selection, or Core-write path; and
- any change inside an approved private boundary receives focused independent re-review before it
  can enter the frozen atomic candidate.

The next implementation is not a gradual exposure tranche. It is the separately reviewed atomic
candidate described below, and it remains blocked until the supported private-loopback Core release
and exact Desktop compatibility contract are supplied by the separate Core workflow.

## Required integrated qualification

Before publication of the frozen atomic release candidate, independent evidence must prove at least:

- exact parity of all twelve command names across every production surface;
- the accepted Layer B malformed, duplicate, window, origin, reload, destruction, concurrency,
  panic, and transport-route families for the complete twelve-command inventory;
- each individual unmet activation prerequisite makes all twelve commands inert;
- unknown, missing, extra, malformed, oversized, duplicated, and secret-like fields fail closed;
- replay, expiry, cancellation, stale window/session/wallet/Core generation, and reordered operation
  failures at every preview and transaction transition;
- native confirmation only, with the accepted DPI, focus, IME, input-origin, and revocation matrix;
- exact transaction, signature, Core peer, submission, ambiguity, receipt, reorganization, journal,
  and restart-reconciliation vectors;
- reload, restart, Core-offline, journal-failure, and repeated-query cases proving that one durable
  `outcome_unknown` or `accepted_recording_pending` state remains visible and blocks new spending;
- successful journal-only completion of `accepted_recording_pending` without Core access, followed
  by terminal reconciliation and exactly one authenticated activity record;
- no automatic write retry or replacement after any ambiguous network outcome;
- secret and telemetry canaries across fixed errors, panic containment, logs, diagnostics, support
  packages, crash output, command line, frontend state, browser storage, and packaged assets;
- Windows console lock, sleep, main-window teardown, clean shutdown, forced termination, power loss,
  process lease loss, and supported single-interactive-session behavior;
- clean-device create, portable recovery acknowledgement, restart, unlock, funding, preview, native
  confirmation, signing, one-attempt submission, receipt tracking, lock, clean-device restore, and
  spend-after-restore against the exact supported private-loopback Core; and
- binary, lockfile, source, capability, permission, Core manifest, WebView2 runtime, repository,
  custody storage, and evidence hashes before and after qualification.

Any Failed or Inconclusive case keeps every production blocker closed. Mock Core listeners prove
parsing and failure behavior only; they cannot qualify the supported peer-identity or real spending
boundary.

## Staged implementation and review sequence

1. Independently review this exact documentation-only design.
2. Preserve the approved private baseline at commit
   `653630cf2c5a60ee1f80343c37fd53e91299527c`. Any private-boundary correction receives focused
   independent review before this sequence continues.
3. Through the separate Core workflow, obtain an exact supported private-loopback release with the
   reviewed peer-binding mechanism. In Desktop, add only the exact compatibility contract and
   independently review its binary, manifest, process-generation, endpoint, fee, status, and
   rejection semantics. Do not modify Vision-Core from this Desktop workflow.
4. Prepare the complete atomic exposure delta against the approved private baseline. The delta must
   contain all twelve wrappers, production handler entries, AppManifest entries, generated narrow
   permissions, the existing main-window capability entries, shared managed state, frontend
   services and public UI, documentation, three reviewed approval constants, the production
   duplicate-key policy, exact Core compatibility entry, packaging, parity tests, and rollback
   controls together. No partial subset may be committed as an activatable state.
5. Independently review that unpublished source tree before any wallet qualification. Correct every
   finding in a new exact tree and repeat source review until there are zero open findings. The tree
   remains undistributed.
6. Build and sign the final distributable artifact once from the accepted source tree; freeze its
   commit, tree, dependency lockfiles, compiler inputs, configuration, signature, complete file-hash
   manifest, and expected WebView2/runtime identity. Do not publish or install it outside the
   controlled qualification environment.
7. Run static ACL, twelve-wrapper transport, authority-surface, secret-canary, interruption,
   packaged Windows, and clean-device end-to-end custody, recovery, spending, ambiguity, receipt,
   and spend-after-restore qualification against that exact artifact using its ordinary production
   authority. No test authority, flag override, alternate manifest, debugger mutation, or bypass is
   permitted.
8. Independently verify and accept both the complete primary evidence and the byte-exact artifact.
   Any Failed, Inconclusive, missing, changed, unhashed, or non-reproducibly classified result rejects
   the candidate and prohibits distribution.
9. Only after final written publication authorization, publish the byte-identical qualified
   artifact and enable the wallet for users. Do not rebuild, re-sign, patch, reconfigure, or change
   release metadata embedded in the artifact after qualification.
10. Preserve the approved private baseline and every rejected candidate/evidence set. A later
    correction begins with a new commit, tree, artifact identity, and all affected reviews and
    qualifications; it never reuses a prior Passed classification across changed bytes.

Failure at any step returns to the private, unreachable state. No earlier private implementation or
transport qualification is permission to skip later gates.

## Approval requested by this revision

Independent review is requested only for this reconciled atomic-exposure design. Approval may
authorize preparation of the complete unpublished atomic candidate only after the separately
supplied private-loopback Core compatibility contract is available and independently accepted.

This revision does not authorize any current source or runtime exposure. In particular, it does not
authorize changing `duplicate_key_rejection_proven`, changing an approval constant, adding a Tauri
command or invoke-handler entry, adding an AppManifest entry, generating or granting a permission,
changing a capability, adding a frontend invoke or custody form, enabling production Core wallet
authority, building an enabled artifact, publishing a release, exporting recovery material, or
modifying Vision-Core.

## Deliberately deferred features

This first surface does not authorize multiple wallets, wallet deletion, password change, recovery
export or rotation, hardware wallets, external signers, address books, clipboard copy, QR receive
requests, arbitrary account lookup, remote Core, custom fees or tips, future nonces, batch sends,
contract calls, transaction replacement, automatic retry, browser secret entry, mining-address
editing, or marketplace payments. Each requires its own threat model, compatibility contract, and
independent review.
