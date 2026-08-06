# Wallet Transaction Authority Boundary

## Status

This specification defines the required first-release transaction path. It does not activate
custody or sending. No command, permission, frontend wrapper, approval flag, or Core integration is
added by this document.

The existing Rust amount, transaction, submission, receipt, and journal modules are private,
unregistered primitives. They are not an end-to-end spending authority. Production signing remains
disabled until this complete boundary is implemented, qualified against a supported private-loopback
Vision-Core release, and independently approved.

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
   - The wallet is unlocked inside Rust and the supervised Core process is proven to use the
     approved private-loopback release.
3. `Preparing`
   - Rust obtains current balance, exact next canonical nonce, canonical tip, and fee information
     directly from Core. React cannot supply an authoritative value.
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

Rust derives all other fields. The first policy is fixed to `cash::transfer`, the current canonical
nonce, zero tip, and the approved minimum fee limit. Custom tips, future nonces, replacements,
arbitrary module/method calls, and batches require separate approval.

The preview displays exact, non-floating-point values:

- complete sender and recipient addresses;
- amount in display units and raw units;
- charged fee and maximum authorized fee;
- total debit only after the Core contract proves its calculation;
- nonce and canonical transaction identifier;
- data age and Core compatibility identity; and
- a warning that mined transactions may reorganize.

Rust rejects invalid or zero amounts, self-transfer, overflow, insufficient spendable balance under
the approved contract, stale nonce, unsupported fees, unavailable or recovering Core, mock mode,
and unapproved compatibility state before confirmation.

## Preview authority

The preview handle is random, single-use, short-lived, capability-bearing, and non-secret. Only its
opaque encoding crosses IPC; the transaction remains in private Rust state. Only one transaction
operation may exist. A new preview, lock, wallet change, or restart invalidates the old preview.

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

The transaction path must not reuse the Explorer HTTP helper unchanged. It needs a Rust-only client
that:

- obtains port and process identity from supervised Core state;
- connects only to the approved loopback literal and refuses redirects;
- verifies the running binary and manifest against the approved release;
- uses explicit connect, write, first-byte, and total timeouts;
- bounds every request and response body before parsing;
- requires exact typed responses and content types where specified;
- returns stable wallet error codes rather than raw library or Core text;
- never logs signed transactions, addresses, nonces, identifiers, response bodies, or
  timing-correlated activity; and
- remains inaccessible to React except through later reviewed commands.

Fresh Rust-side Core reads supply balance and nonce. React state, Explorer results, the reducer, and
the journal are never signing inputs.

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
2. Implement the private Core client without commands.
3. Implement private intent and preview authority.
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
