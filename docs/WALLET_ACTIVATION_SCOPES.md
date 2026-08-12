# Wallet Activation Scopes

## Status

Vision Desktop separates wallet lifecycle authority from transaction-signing authority inside the
private Rust runtime. Both production scopes remain disabled. No wallet command, permission,
password form, or frontend service is registered by this change.

The private signing and submission implementations have since passed their exact-commit reviews,
but that does not activate either production scope. The approval flags remain false and the current
Core manifest still lacks the supported private-loopback contract. The future lifecycle Tauri
boundary and its atomic full-wallet release sequence are specified in
`WALLET_LIFECYCLE_TAURI_EXPOSURE_DESIGN.md`.

This separation permits lifecycle qualification to proceed without weakening the private-Core and
transaction gates. It does not approve custody or sending.

## Lifecycle scope

The lifecycle scope covers only:

- selecting a recovery destination or source through the Rust-owned native flow;
- creating a local encrypted vault and independently recoverable backup;
- restoring a local encrypted vault from an existing recovery artifact;
- unlocking the local Rust session; and
- deriving and returning secret-free account identity and lock status.

The private lifecycle implementation passed its Windows qualification and independent security
review at commit `b027d18`. Production lifecycle authority nevertheless remains `false`: native
secret ceremonies, fail-closed unwind guards, a complete spending path, and their separate reviews
are still required before user-facing custody.

Explicit lock remains storage-independent and always revokes runtime authority; it does not require
an activation proof.

## Signing scope

The signing scope is a strict superset. It requires the complete lifecycle scope and all transaction,
submission, receipt, private-loopback, and independent signing-review gates. It remains disabled.

The runtime issues a scope-bearing, non-constructible activation proof for each operation. A
lifecycle proof cannot satisfy the signing primitive: the signer validates signing scope again at
the deepest authority point before it reads a seed or builds a signature. This prevents a future
command-wiring mistake from turning lifecycle approval into signing authority.

## Full-wallet activation gate

Vision Desktop will not expose create, restore, unlock, or receive-capable custody merely because
the private lifecycle foundation is approved. Wallet creation creates a reasonable expectation that
funds and mining rewards are spendable. User-facing custody therefore waits for independently
reviewed private-loopback Core access, native secret ceremonies, signing, submission, receipt
tracking, recovery, and an end-to-end spending qualification.

Node B must not mine to a newly generated Desktop address before that full-wallet gate. It must
remain stopped or use an already controlled and independently spendable reward address.

## Tests and invariants

Automated tests prove:

- each missing lifecycle requirement blocks lifecycle, recovery selection, and signing;
- each signing-only requirement blocks signing without blocking lifecycle qualification;
- production lifecycle and signing policies both remain closed;
- a lifecycle activation proof is rejected by the transaction signer; and
- the Tauri authority inventory still contains no wallet command or wallet permission.

The revised native secret and unwind design is specified in
`WALLET_NATIVE_SECRET_CEREMONY_DESIGN.md`. Signing and sending remain separate gates. Other controls
are documented in `WALLET_LIFECYCLE_REVOCATION.md` and
`WALLET_INDEPENDENT_SECURITY_REVIEW_HANDOFF.md`.
