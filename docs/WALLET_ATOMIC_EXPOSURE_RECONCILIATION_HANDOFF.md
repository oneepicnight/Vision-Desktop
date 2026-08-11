# Wallet Atomic Exposure Reconciliation Handoff

Date: 2026-08-11

Branch: `fix/wallet-signing-adversarial-matrix`

Approved private baseline commit: `653630cf2c5a60ee1f80343c37fd53e91299527c`

Approved private baseline tree: `d80c8519eb3c2601066a40e9f3a226847cdab7b3`

Reconciled design SHA-256:
`8BF5AAF0A5EC28D8799AB1158A37BA2F8F63E29283363BE6394DE40933316DB0`

## Verdict requested

Independent review is requested only for the revised documentation-only atomic exposure design.
The candidate is the commit containing this handoff. The final report must provide its exact commit,
tree, parent, and this document's SHA-256.

Approval does not authorize a current wallet source or runtime exposure. It may authorize
preparation of the complete unpublished atomic candidate only after the separate Core workflow
supplies an exact private-loopback release and its Desktop compatibility contract receives
independent acceptance.

## Reconciled private implementation anchors

The design now records the exact approved private chain through the current baseline:

- lifecycle command boundary;
- native transaction confirmation and accepted physical evidence;
- private signing;
- one-attempt submission and restart reconciliation;
- transaction command boundary;
- encrypted authenticated envelope storage and journal-v3 association; and
- private local-first receipt refresh.

These anchors remain unregistered and unreachable from production Tauri IPC. A later source change
inside any approved private boundary invalidates that anchor and requires focused re-review.

## Former storage and refresh blocker closed

The design no longer describes exact-envelope storage or receipt refresh as proposed future work.
It now requires the approved implementation behavior:

- matching wallet, journal-v3 record, encrypted envelope, immutable commitment, transaction, and
  compatibility expectation authenticate before Core authority is requested;
- local failure requests zero Core authority;
- one fresh peer-proven read-only Core path validates the complete signed envelope and generation;
- observation recording is authenticated and read back before it escapes; and
- refresh carries no signing, submission, retry, replacement, recovery-export, or raw-envelope
  projection authority.

This closes only the private implementation blocker. It does not close production registration,
supported-Core, artifact qualification, or publication gates.

## Atomic integration remains all-or-nothing

The design continues to require exactly twelve production wallet commands. The eventual unpublished
candidate must change every required exposure surface together:

- generated wrappers and production invoke registration;
- `tauri_build::AppManifest` and twelve narrow generated permissions;
- only the existing main-window capability;
- shared managed runtime and window authority;
- frontend service wrappers and public-only Wallet UI;
- all three independently reviewed approval constants;
- the accepted production duplicate-key policy;
- the exact supported Core compatibility contract;
- packaging, parity, qualification, rollback, and documentation.

No lifecycle-only or transaction-only exposure is permitted. The approved private boundaries must
be composed directly; wrappers, frontend code, and managed state cannot reproduce their security
logic or create a parallel authority path.

## Supported Core remains the next hard prerequisite

The current Core manifest still cannot construct production wallet authority. Before an atomic
candidate may be prepared, the separate Core workflow must supply an exact release with:

- private-loopback HTTP binding;
- the reviewed connected-peer binding mechanism;
- exact status, balance, nonce, fee, transaction, submission, lookup, and rejection semantics; and
- binary, manifest, process-generation, endpoint, and compatibility identities suitable for
  independent Desktop review.

Vision-Core is not modified from this Desktop workflow.

## Non-circular frozen-artifact gate

After supported-Core acceptance, the complete atomic delta may be implemented only in one
unpublished candidate. That candidate uses its ordinary production authority path and contains no
test authority, environment override, hidden permission, alternate manifest, debugger mutation, or
manual flag bypass.

Independent source review must reach zero open findings before one final artifact is built and
signed. The artifact, source tree, dependencies, configuration, signature, and complete hashes are
then frozen. Qualification runs against those exact bytes. Evidence acceptance is an external
publication gate: only the byte-identical qualified artifact may later be distributed. Any source,
binary, configuration, dependency, permission, capability, manifest, frontend, or packaging change
creates a new candidate and reopens affected review and qualification.

## Authority surface remains closed

- No Tauri wallet command or production invoke registration changed.
- No AppManifest entry, permission, capability, managed state, frontend invoke, form, or route
  changed.
- All three wallet approval constants remain `false`.
- Production `duplicate_key_rejection_proven` remains `false`.
- Production Core wallet authority remains unavailable.
- No signing, submission, sending, recovery export, enabled artifact, or publication was added.
- No dependency, lockfile, configuration, or Vision-Core file changed.

## Required independent decision

The reviewer must decide whether the revised design accurately composes the approved private
boundaries and defines a non-circular, all-or-nothing, frozen-artifact exposure and publication
sequence. Approval must not be interpreted as permission to change any current production authority
surface before the supported Core compatibility prerequisite is independently accepted.
