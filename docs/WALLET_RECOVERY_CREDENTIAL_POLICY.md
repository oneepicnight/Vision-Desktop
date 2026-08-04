# Wallet Portable Recovery Credential Policy

## Security decision

Vision Desktop does not accept a user-chosen password to protect a new portable recovery artifact.
Rust generates an independent 32-byte credential directly inside zeroizing ownership using the
operating-system cryptographic random source. Creation fails closed if that source is unavailable.

The encoded credential is:

`vision-recovery-v1-<64 lowercase hexadecimal characters>-<8 lowercase checksum characters>`

The 64 hexadecimal characters encode the complete 256-bit secret. The final eight characters are
the first four bytes of a domain-separated BLAKE3 digest and exist only to detect transcription
errors. The checksum adds no entropy and is not an authentication substitute.

## Cryptographic use

The credential is processed by Argon2id version 0x13 with 65,536 KiB of memory, three iterations,
one lane, a unique random 16-byte salt, and a 32-byte output. The resulting key protects the seed
with XChaCha20-Poly1305 and a unique random 24-byte nonce. Artifact metadata is authenticated as
associated data.

The security of an application-generated credential is bounded primarily by its 256 random bits,
not by human password quality. Argon2id remains defense in depth and preserves the versioned
artifact contract. This document does not claim a measured guesses-per-second value: production
hardware timing and resource analysis must be recorded during the independent activation review.

## Boundary and lifetime

- The credential type has no `Clone`, Serde, `Display`, or unrestricted `Debug` implementation.
- Its binary form lives in `SecretBox`; encoded native presentation uses `Zeroizing<String>`.
- Wallet creation accepts only the local wallet password. It generates the recovery credential in
  Rust and returns it only inside a private, non-serializable creation result.
- Restore accepts only the exact versioned/checksummed credential. Arbitrary legacy text, uppercase,
  truncation, extra characters, and checksum changes are rejected before Argon2id.
- The credential must never enter React, Desktop shared state, reducer events, logs, diagnostics,
  support packages, clipboard automation, or browser storage.
- Incorrect but well-formed credentials and damaged ciphertext produce the same decryption error.

No wallet Tauri command or permission is registered. A future Rust-native presentation and
verification flow must be reviewed before creation can be exposed.

## Activation tests still required

Before real funds are permitted, release engineering must complete and record:

1. Recovery to a fresh supported Windows user profile or separate supported Windows device.
2. Recovery from the intended offline/removable-media workflow.
3. Operator transcription and checksum-failure drills.
4. Measured Argon2id latency and resource use on minimum, typical, and high-end supported hardware.
5. Crash, suspend, workstation-lock, process-termination, and memory-leak testing during credential
   generation, presentation, artifact verification, and restore.
6. Independent review of the final native presentation, Tauri command signatures, panic boundary,
   and zeroization behavior.

Until those tests and the broader independent review pass, wallet custody remains inactive.
