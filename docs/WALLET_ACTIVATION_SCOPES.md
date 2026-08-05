# Wallet Activation Scopes

## Status

Vision Desktop separates wallet lifecycle authority from transaction-signing authority inside the
private Rust runtime. Both production scopes remain disabled. No wallet command, permission,
password form, or frontend service is registered by this change.

This separation permits lifecycle qualification to proceed without weakening the private-Core and
transaction gates. It does not approve custody or sending.

## Lifecycle scope

The lifecycle scope covers only:

- selecting a recovery destination or source through the Rust-owned native flow;
- creating a local encrypted vault and independently recoverable backup;
- restoring a local encrypted vault from an existing recovery artifact;
- unlocking the local Rust session; and
- deriving and returning secret-free account identity and lock status.

Production lifecycle authority requires the independently reviewed key-derivation and address
contracts plus explicit lifecycle-security approval. That approval remains `false` until the real
Windows qualification matrix and an independent review of its evidence pass.

Explicit lock remains storage-independent and always revokes runtime authority; it does not require
an activation proof.

## Signing scope

The signing scope is a strict superset. It requires the complete lifecycle scope and all transaction,
submission, receipt, private-loopback, and independent signing-review gates. It remains disabled.

The runtime issues a scope-bearing, non-constructible activation proof for each operation. A
lifecycle proof cannot satisfy the signing primitive: the signer validates signing scope again at
the deepest authority point before it reads a seed or builds a signature. This prevents a future
command-wiring mistake from turning lifecycle approval into signing authority.

## Node B consequence

After packaged Windows lifecycle qualification and independent lifecycle-only approval, the Desktop
may expose create, restore, status, unlock, and lock through explicit main-window-only permissions.
That limited boundary can safely create a recoverable public reward address for Node B without
enabling transaction signing or submission.

Node B must not mine to a newly generated address until the recovery artifact has been written,
read back, decrypted, and matched to the account identity by the existing Rust onboarding flow.

## Tests and invariants

Automated tests prove:

- each missing lifecycle requirement blocks lifecycle, recovery selection, and signing;
- each signing-only requirement blocks signing without blocking lifecycle qualification;
- production lifecycle and signing policies both remain closed;
- a lifecycle activation proof is rejected by the transaction signer; and
- the Tauri authority inventory still contains no wallet command or wallet permission.

The journal-head correction in `WALLET_JOURNAL_AUTHENTICATION.md` remains subject to independent
re-review before signing or sending. Other gates are documented in
`WALLET_LIFECYCLE_REVOCATION.md` and `WALLET_INDEPENDENT_SECURITY_REVIEW_HANDOFF.md`.
