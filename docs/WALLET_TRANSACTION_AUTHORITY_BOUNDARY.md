# Wallet Transaction Authority Boundary

## Status

This specification defines the required first-release transaction path. It does not activate
custody or sending. No command, permission, frontend wrapper, approval flag, or Core integration is
added by this document.

The existing Rust amount, transaction, submission, receipt, and journal modules are private,
unregistered primitives. They are not an end-to-end spending authority. Production signing remains
disabled until this complete boundary is implemented, qualified against a supported private-loopback
Vision-Core release, and independently approved.
### Implemented private read-only tranche

The first approved implementation tranche now exists only inside Rust. The supervisor can issue an
opaque `CoreConnectionAuthority` that binds one monotonic process generation, a duplicated live
process handle, the Windows process-creation identity, the exact API port, and the exact approved
compatibility-manifest digest. The authority cannot be cloned, debug-formatted, serialized, or
constructed from caller-supplied connection values.

The private wallet Core client supports bounded `GET` reads for one combined account snapshot and
node status/canonical tip. Balance and nonce wire reads are private implementation details; the
client releases them only after their echoed addresses and existence states agree, and a nonexistent
account must report both values as zero. Status requires the exact supported version and canonical
lowercase 32-byte hexadecimal hash shapes. Production result types do not expose unrestricted
`Debug` formatting. Every operation creates a fresh literal `127.0.0.1` TCP connection,
verifies that the server side of that exact connection belongs to the supervised PID through the
Windows TCP owner table, validates authority before and after the read, applies one total deadline,
requires bounded HTTP/1.1 JSON with an exact typed schema, and returns fixed internal errors.
Redirects, transfer encoding, DNS hosts, proxies, cookies, ambient credentials, retries, pooled
connections, unknown response fields, and network writes are absent or rejected.

Production authority remains unavailable because the bundled RC2 manifest does not declare the
reviewed `vision-wallet-read-v1` contract, literal loopback bind, Windows socket-owner binding, and
exact fee policy. No wallet Tauri command, permission, capability, frontend wrapper, form,
activation-flag change, signing access, transaction submission, or Vision-Core modification is
included. Mock listeners test parsing and failure behavior but do not qualify a production Core
release.

### Implemented private preview tranche approved for native-confirmation integration

The next candidate tranche now exists only inside the private Rust wallet module. Its bounded
request accepts exactly one canonical public recipient address and one plain decimal amount string;
unknown, duplicate, secret-bearing, caller-authoritative, malformed, and oversized fields are
rejected. Rust obtains the unlocked wallet's public identity without exposing or refreshing the
seed session, reads the combined balance/nonce snapshot and status through the reviewed
CoreConnectionAuthority client, and derives the fixed transfer method, exact nonce, zero tip,
charged fee 1, fee limit 201, total debit, canonical tip, and unsigned transaction identifier.

The complete unsigned intent stays in WalletRuntimeState. A random 256-bit opaque handle is bound
to the main window, wallet identity, public account, revocation epoch, and a one-minute monotonic
expiry. The private intent also retains a domain-separated fingerprint of the exact supervised Core
PID, process-creation identity, monotonic generation, loopback port, and compatibility manifest
that produced the observations. The handle is single use. Replacement previews, wallet
creation/restore/unlock, explicit or lifecycle invalidation, wallet mismatch, idle locking,
restart, malformed handles, replay, and expiry all fail closed. Public preview data has no
unrestricted Debug implementation and reports measured monotonic age beginning before the first
authoritative Core read, without exposing an absolute or process-relative timestamp.

This approved preview tranche adds no command, permission, capability, AppManifest entry, frontend
wrapper or form. It performs no signing, seed access, network write, submission, reconciliation,
receipt tracking, recovery export, activation change, or Vision-Core modification. Production
preparation remains unavailable under the current manifest.

### Implemented private native final-confirmation tranche awaiting independent review

The next candidate tranche exists only inside the private Rust wallet module. Preview consumption
retains its runtime operation permit and exact Core-generation authority while a main-window-owned,
owner-drawn Windows dialog displays the complete sender, recipient, amount, charged fee, maximum
fee, total debit, nonce, and transaction identifier. The dialog contains no editable transaction
field, disassociates IME contexts from itself and both focusable buttons, rejects text-service and
clipboard message routes, polls runtime and Core authority while open, and requires one explicit
confirm action. Windows' normal focus-time `WM_IME_SETCONTEXT` notification is suppressed only
after a balanced live check proves that the window remains disassociated; an associated context or
any composition/input-language route fails closed. While the dialog remains open, the same-thread
timer rechecks the live input-context association for the dialog and both focusable controls every
250 milliseconds. IME absence is also revalidated synchronously after focus and before the display
is armed, and again immediately before the exact Confirm command can release authority. Failure at
either transition wipes and closes the ceremony. Its temporary UTF-16 transaction buffers are
zeroized on every exit.

Cancellation, owner loss, UI failure, panic, Core exit or generation change, wallet/runtime
revocation, and stale authority destroy the consumed intent without producing confirmation
authority. Panic containment invalidates the wallet runtime or terminates if invalidation cannot be
proven. The Confirm control begins disabled and non-default. It is armed only after every required
value has been measured to fit its fixed bounds and every checked Win32 draw operation completes.
Approval then requires a complete, fresh, post-display hardware keyboard or mouse press delivered
by the exact armed control. Mouse and keyboard press state cannot be combined. A keyboard command
generated on key-down is not authority and cannot consume the pending physical press; only the
matching hardware key release drives one exact Confirm-button action. Pre-display and repeated-key
input, ordinary non-UIAccess `SendInput` injection, system-origin input, unavailable-origin input,
wrong-control input, and synthetic command paths fail closed. Windows also classifies input
injected by an application whose manifest has
`uiAccess="true"` as `IMO_HARDWARE`; Vision Desktop therefore treats trusted Windows UIAccess
processes as part of the operating-system trusted-computing boundary and does not claim to
distinguish their injected input from physical hardware. This boundary follows Microsoft's
[`INPUT_MESSAGE_ORIGIN_ID`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ne-winuser-input_message_origin_id)
contract and must be included in activation review and endpoint qualification. Only the
native-confirmation module can construct the private,
non-forgeable approval capability consumed by the preview boundary, so a future sibling wallet
module cannot complete the intent directly. A successful final authority check then releases a
distinct Rust-only confirmed-intent type; that type is not signing authority and has no Clone,
Debug, or serialization implementation.

This candidate registers no command, permission, capability, AppManifest entry, managed Tauri
state, frontend wrapper, or form. It accesses no seed, creates no signature, performs no network
write, submission, reconciliation, receipt update, recovery export, approval change, or Vision-Core
modification. It requires independent review before any signing integration begins.

## Security objective

One explicit user decision may authorize exactly one transaction with one sender, recipient, amount,
nonce, fee policy, and canonical transaction identifier. No stale, replayed, reordered, substituted,
or silently modified request may acquire signing authority.

- React may handle only public transaction input, public previews, opaque authorization handles,
  public identifiers, receipt observations, and safe status/error codes.
- React must never receive a password, signing seed, decrypted vault, password-derived key, recovery
  credential, signed payload before submission, session authority, or activation proof.
- Rust owns the unlocked session, transaction construction, final confirmation authority, signing,
  Core submission, ambiguous-outcome reconciliation, and journal authentication.
- Vision-Core receives public queries and a fully signed transaction only. It never receives wallet
  secrets or Desktop authority.
- The journal is display history only. It never supplies balances, nonces, fees, signing decisions,
  retries, receipt truth, or transaction success.

## Required state machine

1. `Locked`
   - No preview or signing authority exists.
2. `Ready`
   - The wallet is unlocked inside Rust and the client holds a current `CoreConnectionAuthority`
     proving both the approved private-loopback process generation and the identity of the HTTP
     peer serving this operation.
3. `Preparing`
   - Rust reads the current balance, exact next canonical nonce, and canonical chain-tip height
     from that peer. Fee policy comes from the authority's exact compatibility contract rather
     than an unverified Core fee query. React cannot supply an authoritative value.
4. `Reviewable`
   - Rust constructs the complete unsigned transaction and identifier.
   - React may receive a public preview plus one random opaque handle bound to the main window,
     wallet, session generation, transaction fields, Core identity, and a short monotonic expiry.
5. `Confirming`
   - A Rust-owned final confirmation displays complete sender, recipient, amount, charged fee,
     maximum fee, total debit, nonce, and transaction identifier.
6. `Signing`
   - Rust consumes the confirmation exactly once and rechecks window, session, activation scope,
     expiry, revocation epoch, transaction identifier, and Core identity.
7. `Submitting`
   - Rust submits the exact signed transaction once.
   - A timeout or connection loss becomes `OutcomeUnknown`, never an automatic failure or retry.
8. `Accepted`, `Rejected`, or `OutcomeUnknown`
   - Acceptance requires the exact typed response, matching identifier and nonce, and a
     non-replacing decision.
   - Rejection destroys the transaction authority.
   - Unknown outcome initiates point lookup of the existing identifier without re-signing.
9. `Tracking`
   - Exact acceptance is recorded as public local activity and observed through Core point lookup.
   - States remain `Not observed`, `Pending`, or `Mined - N confirmations`. Reorganizations and
     lost observations remain visible. No state is called final.
10. `Complete`
    - Completion means the current observation and local display record were processed. It does not
      claim irreversible protocol finality.

Explicit lock, session timeout, workstation lock, suspend, main-window reload/destruction, process
teardown, panic containment, process-lease loss, wallet change, or a newer operation invalidates
every preview and confirmation. No stale result may escape after revocation.

## Transaction input and preview

The frontend request must be bounded, deny unknown fields, and contain only:

- a recipient encoded as exactly 64 lowercase hexadecimal characters; and
- an amount encoded as a bounded plain decimal string.

It must not contain sender, nonce, fee, tip, module, method, Core port, window label, session
identifier, transaction identifier, signed bytes, retry instruction, or replacement policy.

Rust derives all other fields. Balance and canonical nonce are fresh reads from the exact peer
bound by `CoreConnectionAuthority`. The first policy fixes `cash::transfer`, tip `0`, charged fee
`1 + tip` raw units, and fee limit `201` raw units from the exact versioned compatibility contract;
Core exposes no separate fee-policy endpoint for this contract. If the running release's fee rules
differ, authority construction fails closed and new compatibility evidence, vectors, and review
are required. Custom tips, future nonces, replacements, arbitrary module/method calls, and batches
require separate approval.

The preview displays exact, non-floating-point values:

- complete sender and recipient addresses;
- amount in display units and raw units;
- charged fee and maximum authorized fee;
- total debit only after the Core contract proves its calculation;
- nonce and canonical transaction identifier;
- monotonic data age measured from before the first authoritative Core read through preview
  publication, without exposing an absolute or process-relative timestamp;
- Core compatibility identity; and
- a warning that mined transactions may reorganize.

Rust rejects invalid or zero amounts, self-transfer, overflow, insufficient spendable balance under
the approved contract, stale nonce, unsupported fees, unavailable or recovering Core, mock mode,
and unapproved compatibility state before confirmation.

## Preview authority

The preview handle is random, single-use, short-lived, capability-bearing, and non-secret. Only its
opaque encoding crosses IPC; the transaction remains in private Rust state. Only one transaction
operation may exist. A new preview, lock, wallet change, or restart invalidates the old preview.

Consuming a preview removes it before any private intent can escape. Rust then constructs a fresh
supervisor-issued `CoreConnectionAuthority`, compares its exact generation-bound identity
fingerprint with the identity retained in the preview, and revalidates that identity immediately
before releasing the private intent to the next native-only stage. Core exit, stop, restart,
generation or manifest replacement, identity mismatch, or inability to obtain current authority
fails closed and permanently consumes that preview handle.

Neither the identifier nor preview handle is sufficient signing authority. Signing also requires a
current runtime permit, matching unlocked session, consumed confirmation, and signing scope.

## Trusted final confirmation

The first release uses a Rust-owned, main-window-parented native final confirmation. React may
provide the form and visual preview, but it is not the final trusted display because compromised
React or WebView code is an explicit threat.

The native confirmation:

- displays complete, non-truncated sender and recipient addresses;
- displays exact amount, fees, total when defined, nonce, and identifier;
- contains no editable transaction field;
- requires an explicit confirmation action;
- fails closed on cancellation, UI failure, unexpected input, revocation, window mismatch, panic,
  or stale completion; and
- clears temporary native text buffers on every exit.

It does not request the wallet password again. Re-authentication for every send would require a
separate security and usability decision.

## Private Core client

The transaction path must not reuse the Explorer HTTP helper unchanged. It requires a private
Rust-only client whose production entry points accept a `CoreConnectionAuthority` rather than a
URL, host, port, PID, process generation, binary hash, or compatibility identifier.

### CoreConnectionAuthority

`CoreConnectionAuthority` is an immutable, non-serializable, non-cloneable, non-debuggable Rust
authority with private fields. Only the Core supervisor may produce it. Ordinary callers cannot
construct, deserialize, reconstruct, or modify one from public process or network values.

The authority binds:

- the supervisor's monotonic process generation;
- a held Windows process handle, PID, and process creation identity so PID reuse cannot satisfy it;
- exact verified binary and manifest identity;
- the approved wallet/Core contract version;
- the literal IPv4 loopback address and supervisor-owned API port;
- the supported peer-binding mechanism and its per-launch state; and
- an invalidation source triggered synchronously by Core exit, restart, or compatibility change.

Proving what Desktop launched is not sufficient. Before any response is trusted or signed envelope
is sent, the client must prove that the process answering the exact connected HTTP socket is that
same supervised process generation. The supported Core release and compatibility manifest must
define one of these reviewed mechanisms:

1. an operating-system-verifiable mapping from the exact connected socket/tuple to the held
   process identity, checked without a path or port ownership race; or
2. a per-connection authenticated challenge/response using a random per-launch secret delivered to
   Core through a supervisor-controlled inherited channel, never through arguments, environment,
   ordinary files, React, logs, or support packages.

PID, configured port, binary hash, listener presence, or a pre-connect listener-owner check alone
does not prove the connected peer. If the supported release cannot supply one complete mechanism,
production `CoreConnectionAuthority` construction remains unavailable.

For every operation the client must:

1. acquire a generation-bound operation lease from the supervisor;
2. prove that the held process is alive and still has the same creation identity;
3. prove the current compatibility manifest and binary identity;
4. open a fresh connection to the literal bound loopback endpoint;
5. prove the peer identity on that same connection before exchanging trusted wallet data;
6. execute exactly one bounded typed request without retry;
7. revalidate supervisor generation, process liveness/identity, peer binding, and compatibility
   after the complete response; and
8. discard the response if any generation, process, port, peer, or compatibility value changed.

Core exit or restart invalidates all leases and connections before a later generation can become
ready. A pooled connection may never cross generations. The initial implementation uses a fresh
connection per operation so no stale pool can survive invalidation.

### Transport restrictions

The client:

- uses only the literal `127.0.0.1` address from the authority and performs no DNS resolution;
- disables environment, system, PAC, and library-default proxies;
- refuses redirects;
- disables cookies, ambient credentials, authentication negotiation, and referrer propagation;
- uses no automatic transport or application retry;
- does not reuse a connection across operations or process generations;
- pins the approved HTTP protocol behavior for the supported Core release;
- applies explicit connect, write, first-byte, and total timeouts;
- caps every request and response body before parsing;
- requires exact typed responses and content types where specified;
- returns stable wallet error codes rather than raw library or Core text;
- never logs signed transactions, addresses, nonces, identifiers, response bodies, peer-binding
  secrets, or timing-correlated activity; and
- remains inaccessible to React except through later reviewed wallet commands.

Fresh Rust-side reads through this authority supply balance, nonce, and chain-tip observations.
React state, Explorer results, the reducer, and the journal are never signing inputs.

### First private client tranche

Design approval may authorize only an unregistered, read-only client tranche for exact bounded
account-balance, account-nonce, status, canonical-chain-tip, and peer-identity operations. It must
not implement or connect:

- `POST /transactions` or any other write request;
- signed-envelope construction or transport;
- reconciliation-marker orchestration;
- wallet-seed, unlocked-session, activation-proof, or signing access;
- Tauri commands, AppManifest entries, permissions, capabilities, frontend wrappers, or forms; or
- changes to lifecycle or signing approval flags.

Production authority construction remains unavailable until a supported private-loopback Core
release and its exact peer-binding mechanism are present in the Desktop compatibility manifest.
Test-only mock authorities and listeners may validate parsing and failure behavior, but they cannot
qualify the production peer-identity boundary.

## Signing

Signing occurs inside one narrow closure over the unlocked `WalletSeed`. Secret-derived storage
remains zeroizing and non-serializable. Rust never returns a seed, private key, activation proof, or
session handle.

Before signing, Rust recomputes the payload and identifier from the bound intent. After signing it
verifies its signature and identifier before submission. The signed transaction never enters logs,
the reducer, support packages, command-line arguments, crash messages, or the journal.

## Submission and ambiguous outcomes

Desktop submits only the exact reviewed `POST /transactions` body. HTTP success alone is not
transaction success.

If the request may have reached Core without a valid response, Desktop retains the public identifier
in bounded private reconciliation state and queries `GET /transaction/:txid`. It must not:

- guess or increment a nonce;
- create a replacement;
- re-sign or automatically submit again;
- label the transfer failed; or
- advise resending until reconciliation proves the prior outcome.

Before the network write, Rust must atomically publish a bounded, seed-authenticated reconciliation
marker containing the wallet identifier, transaction identifier, a domain-separated digest of the
exact signed envelope, creation time, and `submitting` phase. It stores neither seed nor signed
transaction. If this marker cannot be committed, Desktop must not submit. A definitive rejection
resolves it; exact acceptance transitions it into journal tracking.

After restart, reconciliation requires unlocking the matching wallet, authenticating the marker,
and performing point lookup. The digest of any returned signed transaction must match the stored
digest. Restart never grants permission to resubmit.

A nonce rejection requires a fresh authoritative read and a completely new user review.

## Receipt reconciliation

The receipt observer must compare the returned transaction with the exact submitted signed
transaction, including its expected signature, as well as recomputing the unsigned identifier. This
closes ambiguity because the RC2 identifier excludes `sig`.

Tracking is bounded and restart-safe. Core supplies observation truth; the journal supplies only
authenticated local-display metadata. The UI distinguishes:

- Core accepted but local activity recording failed;
- outcome unknown and reconciliation pending;
- accepted but not currently observed;
- pending;
- mined with current confirmations;
- reorganized or observation lost; and
- Core unavailable with a clearly stale last observation.

No journal failure, missing observation, reorganization, or outage causes automatic resend.

## Journal failure semantics

A valid accepted response remains authoritative if journal persistence fails. Desktop preserves the
public identifier in bounded reconciliation state, reports the recording failure separately, and
retries only the authenticated public journal write. It never resubmits the transaction.

The journal is incomplete local activity. Coordinated rollback of both journal and authenticated
head, or of the complete Windows profile/filesystem, remains a documented limitation and cannot
authorize spending.

## Future Tauri boundary

No transaction command or permission may be added before private implementation, exact Core
qualification, and independent review. Candidate command categories are:

- prepare one transfer preview;
- cancel one preview;
- confirm and submit one exact preview through native confirmation;
- list authenticated local activity as explicitly incomplete; and
- refresh one known transaction observation.

Names and schemas require review. Commands derive the invoking `WebviewWindow`, require `main`, and
never accept a caller-supplied owner label, Core port, path, session token, activation proof, nonce,
fee, signed payload, retry flag, or replacement flag.

Permissions are individually generated and added only to `main-desktop`. They remain absent from
Linux mock and plugin permission sets. React wrappers remain in `src/services/coreApi.ts`. Public
presentation may use the existing event/reducer pipeline; capability and signing state remain Rust
only.

## Required qualification

Before registration, tests cover:

- exact amount, payload, identifier, signature, nonce, fee, submission, and receipt vectors;
- malformed, duplicate, unknown, and oversized IPC fields;
- preview expiry, replay, cancellation, window/session/wallet mismatch, and revocation;
- lifecycle interruption and panic at every state;
- all transaction validation and Core compatibility failures;
- stale or competing listeners, PID reuse, process exit/restart, process-generation changes,
  socket-owner mismatch, peer-authentication failure, and compatibility changes;
- redirects, oversized/truncated bodies, bad content types, timeouts, unknown fields, mismatched
  identifiers/nonces/signatures, and replacement decisions;
- accepted-response loss, restart during ambiguity, journal failure, and no automatic retry;
- pending/mined progress, reorganization, observation loss, and stale Core;
- secret canaries across logs, support packages, reducer, devtools, command line, and crash
  artifacts; and
- packaged Windows native confirmation with international keyboards and IMEs.

The exact packaged Desktop must pass against the exact approved private-loopback Core. Mock
listeners do not replace this gate.

## Activation order

1. Approve this design.
2. Implement only the unregistered, bounded, read-only private Core client tranche.
3. Implement private intent and preview authority (candidate implemented; review pending).
4. Implement and qualify native final confirmation.
5. Connect unlocked-session signing.
6. Add submission, ambiguity reconciliation, receipts, and journal orchestration.
7. Integrate the supported private-loopback Core and update its compatibility manifest.
8. Run adversarial, interruption, packaged-Windows, and clean-device tests.
9. Obtain independent review of the exact commit and tree.
10. Register only approved commands and main-window permissions.
11. Add frontend public forms and state without secrets.
12. Obtain final activation approval.

Until every step passes, wallet creation, receive-capable custody, signing, submission, and sending
remain disabled. Node B must not mine to a newly generated Desktop address merely because private
wallet primitives exist.
