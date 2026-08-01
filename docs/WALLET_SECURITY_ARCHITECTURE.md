# Wallet Security Architecture

## Decision

Vision Desktop will provide an embedded, non-custodial wallet. The user controls the wallet, while secret-bearing operations remain inside the Rust/Tauri backend.

React is a presentation process. It may receive public wallet metadata, balances, nonces, fee quotes, transaction previews, transaction identifiers, receipts, and explicit success or failure states. It must never receive a private key, password-derived key, decrypted vault, signing seed, or recovery phrase returned by Rust. User-entered recovery words may exist only transiently in an isolated import form and must never enter shared state, persistence, logs, diagnostics, or a response from the Rust backend.

## Trust boundaries

### Rust custody boundary

Only the Rust wallet module may:

- generate or restore secret material;
- derive public keys and addresses after the Core contract is approved;
- encrypt or decrypt a wallet vault;
- hold an unlocked signing seed;
- produce a signature;
- construct the final signed transaction after compatibility vectors pass.

Secret values must use types that restrict accidental access and zeroize memory on drop. They must not implement Serde serialization, `Display`, unrestricted `Debug`, or cloning by default.

### Tauri command boundary

Wallet commands may return only purpose-built public response types. Commands must not accept a raw private key from React. Recovery input is a special one-time flow and must not be stored in Desktop state, reducer events, logs, error strings, diagnostics, or support packages.

### React boundary

The existing `coreApi.ts` module remains the only frontend Tauri invocation boundary. Public wallet state must continue through typed Desktop actions and events with request-ordering protection. Secret input must be isolated from the general Desktop state model and cleared immediately after the Rust command completes.

### Core boundary

Vision Core receives public queries and fully signed transactions only. Desktop must never call a Core signing endpoint or send Core a private key, even over loopback.

## Fail-closed rules

- Signing is disabled until every item in `wallet_contract_gate()` has an approved contract and deterministic test vectors.
- Wallet creation must not become available until recovery produces the identical public key and address in an independent vector test.
- Amounts are strings or bounded integers in confirmed smallest units; floating-point blockchain arithmetic is forbidden.
- Transaction success requires an explicit accepted response and later observed receipt/finality state. Missing errors are not success.
- A nonce conflict causes a fresh read and a new user review; Desktop does not silently re-sign a changed transaction.
- No automatic signing, submission, retry, password capture, recovery export, clipboard copy, or unlock is allowed.
- Support packages must never include wallet directories, vault contents, public-to-secret mappings, or secret-bearing errors.

## Encrypted vault status

The first encrypted vault foundation is implemented inside the Rust wallet module but is not exposed through Tauri commands. It includes:

- Argon2id password-based key derivation with fixed, validated cost parameters;
- XChaCha20-Poly1305 authenticated encryption with a unique random salt and nonce for every vault;
- authenticated vault metadata so identifiers and cryptographic parameters cannot be altered silently;
- encrypted-only, create-new file storage that never overwrites an existing vault, Unix `0600` creation permissions, bounded file sizes, and corruption detection;
- identical non-revealing errors for an incorrect password and damaged ciphertext;
- Rust-only unlocked sessions with explicit lock and five-minute idle auto-lock enforcement before every secret operation;
- escalating unlock backoff after repeated incorrect-password or damaged-vault results;
- no plaintext temporary files or crash-report inclusion.

No wallet creation, recovery, unlock, signing, or send command is registered with Tauri yet. Before custody is enabled for users, the remaining vault work includes:

- OS-backed protection for an additional wrapping key where available;
- restrictive Windows ACL creation and verification;
- integration of the Rust-only session with future Tauri commands and process/window lifecycle events;
- versioned migration with backup-before-upgrade behavior;
- secure backup/recovery UX and an independent cryptographic review.

## Recovery requirements

The repository contains conflicting historical recovery descriptions. No mnemonic or derivation algorithm is selected by this document. The final choice must be specified as a Vision wallet contract and backed by fixed vectors for:

- phrase to seed;
- seed to private key;
- private key to public key;
- public key to address;
- restoration to the identical address across platforms.

The legacy browser wallet derivation is not approved for reuse.

## Review gates

Before any release can hold real funds:

1. Core compatibility vectors pass in Rust and against the supported Core binary.
2. Vault format and key lifecycle receive an independent security review.
3. Create, restore, lock, unlock, send, rejection, nonce race, corruption, and recovery tests pass.
4. Logs, diagnostics, reducer state, frontend storage, support packages, and crash paths are scanned for secret leakage.
5. The release is code-signed and built through the controlled release process.

This architecture reduces risk but does not claim that unaudited software is equivalent to certified hardware. Hardware-wallet support remains a recommended future defense-in-depth option.
