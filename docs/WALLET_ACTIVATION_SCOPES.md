# Wallet Activation Scopes

## Atomic candidate status

Lifecycle, signing, submission, and reconciliation remain distinct, non-forgeable Rust authority
scopes. The unpublished atomic candidate sets the three independently reviewed approval constants
to `true` only after the exact v1.0.4 Core contract, private transport, lifecycle, signing,
submission, and receipt foundations passed their separate reviews.

Production activation is all-or-nothing at the release boundary. A build with lifecycle authority
but missing transaction authority, commands, permissions, frontend behavior, or the admitted Core
contract is invalid and must not ship.

## Authority hierarchy

- Lifecycle covers native recovery selection, create, restore, unlock, lock, and public status.
- Signing requires lifecycle plus the exact Core compatibility, preview, native confirmation, and
  signing proof.
- Submission requires signing plus durable pre-write ambiguity, one-write authority, exact response
  handling, and authenticated reconciliation.
- Reconciliation permits only its phase-specific read or journal transition and cannot sign or
  submit.

The deepest Rust authority point rechecks each scope. A lifecycle proof cannot sign; a signing proof
cannot perform the network write; restart reconciliation has no signing or write child authority.

## Release gate

The candidate remains unpublished until exact-source review, unchanged packaged qualification,
clean-device recovery testing, evidence acceptance, and explicit publication authorization.
Changing any command, permission, capability, source, dependency, configuration, Core identity, or
packaged byte invalidates qualification.

Automated tests retain per-requirement negative policies, prove scope separation, require the
production policy to satisfy all four reviewed scopes, and verify exact twelve-command Tauri parity.
