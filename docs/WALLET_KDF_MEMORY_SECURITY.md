# Wallet KDF Memory Security

## Status

Vision Desktop's private Rust vault and portable-recovery KDF paths now own and wipe the complete
Argon2 workspace. This correction does not register wallet commands, grant permissions, change
Vision-Core, or enable custody.

## Construction

Both KDF paths use Argon2id version 1.3 with their existing reviewed parameters. The `argon2` crate
is pinned at `0.5.3` with its `zeroize` feature enabled. That feature wipes Argon2's internal initial
hash, final block hash, and final hash bytes.

Vision Desktop no longer calls `hash_password_into`, whose convenience allocation is outside the
caller's wiping control. The shared Rust KDF module:

1. validates the requested Argon2 parameters;
2. allocates the required block count through fallible reservation;
3. owns the blocks in `Argon2Workspace`;
4. calls `hash_password_into_with_memory` with that workspace;
5. stores the 32-byte result in `Zeroizing<[u8; 32]>`;
6. wipes every `argon2::Block` in `Drop` before the vector releases its allocation.

The same destructor runs on successful derivation, returned errors, and Rust panic unwinding. If the
process is terminated or aborts without unwinding, Windows process teardown remains the final memory
reclamation boundary; that is not represented as an application-level wipe guarantee.

## Adjacent temporaries

Passwords, generated recovery credentials, derived keys, device factors, combined keys, decrypted
seeds, and plaintext buffers remain held by the existing zeroizing/secret wrappers. Vault and
recovery AEAD setup converts the combined zeroizing byte array to a borrowed key view, avoiding a
separate caller-owned cipher-key copy that would require an independent lifetime.

Non-secret salts, nonces, authenticated metadata, encrypted artifacts, and public identity values do
not require secret-memory treatment.

## Automated evidence

Tests provide:

- a compile-time proof that `argon2::Block` implements `Zeroize`, which fails if the dependency
  feature is removed;
- deterministic output from caller-owned memory;
- an instrumented workspace destructor that verifies every initialized block is zero before
  allocator release, while the vector still retains its original length;
- full-size 64 MiB coverage with both the first and final blocks dirtied before drop;
- explicit derivation-error cleanup coverage;
- panic-unwind cleanup coverage;
- the complete existing create, restore, unlock, recovery, vault, and lifecycle suite.

Static Tauri authority tests require the pinned Argon2 feature, caller-memory API, workspace drop
implementation, and absence of the convenience KDF call in both custody paths.

## Remaining qualification

Independent re-review must confirm this correction. Release qualification must also assess Windows
crash-dump policy and perform memory-dump/allocator-reuse characterization against optimized release
builds. Support packages do not collect process dumps or raw logs, and wallet commands remain
unregistered while review approval is false.
