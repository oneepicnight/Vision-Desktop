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
- a random device key protected for the current Windows user and machine by DPAPI, combined with the Argon2id password key so neither factor is sufficient alone;
- authenticated vault metadata so identifiers and cryptographic parameters cannot be altered silently;
- encrypted-only, create-new file storage that never overwrites an existing vault, bounded file sizes, and corruption detection;
- handle-bound Windows vault and recovery I/O that holds ancestor directories against rename, rejects reparse traversal, applies the vault DACL and performs non-replacing atomic publication through the staging handle, and avoids path-based deletion after failed writes;
- fail-closed storage permissions: Unix `0700` wallet directories and `0600` vault files; protected Windows DACLs granting full control only to the owner, SYSTEM, and local Administrators;
- identical non-revealing errors for an incorrect password and damaged ciphertext;
- Rust-only unlocked sessions with explicit lock and five-minute idle auto-lock enforcement before every secret operation;
- escalating unlock backoff after repeated incorrect-password or damaged-vault results;
- an internal versioned portable recovery artifact that encrypts the same opaque seed with an independent Argon2id password key and XChaCha20-Poly1305, without DPAPI or machine binding;
- a Rust-only onboarding coordinator that requires distinct local-vault and recovery passwords, an explicit create-new backup destination, read-back of the saved artifact, successful in-memory restoration, and equality of the restored Vision account identity before the local vault can be stored;
- no plaintext temporary files or crash-report inclusion.

The Rust wallet module now also contains an internal RC2-compatible `cash::transfer` builder, exact 9-decimal amount conversion, conservative current-nonce and zero-tip fee policy, canonical bincode serializer, BLAKE3 transaction-identifier function, Ed25519 signer, strict submission-response parser, canonical receipt observer, and public-metadata-only local activity journal backed by exact Core and independent fixed vectors. These primitives are deliberately not registered as Tauri commands, are not connected to the unlocked-session manager, and cannot be invoked by React. They accept only exact integer units and a narrowly typed transfer draft; arbitrary modules or methods are not exposed. Unknown response shapes, mismatched transaction identifiers, unexpected accepted nonces, unapproved replacements, inconsistent returned transactions, invalid block references, damaged activity records, and unknown journal schemas fail closed.

The activity journal is append-only, versioned, wallet-bound, sequence-checked, size-bounded, synchronized within the Desktop process, and stored with the same restrictive filesystem permissions as wallet storage. It records only public transfer metadata after an exact accepted submission and later validated receipt observations. It never stores signed bytes, signatures, passwords, seeds, recovery material, or vault contents, and it never supplies balances, nonces, or signing decisions. Its `High confidence` presentation at 50 canonical confirmations is a Desktop diagnostic policy, never a claim of deterministic finality.

No wallet creation, recovery, unlock, user-facing signing, or send command is registered with Tauri yet. Private Rust lifecycle adapters now connect status, create, restore, unlock, and lock to the existing vault, recovery, session, runtime, and native-selection primitives. They use a Windows Known Folder-resolved, fixed-local-volume vault root whose existing chain is rejected if it contains a reparse point; complete create/restore locked; never overwrite either vault or recovery data; and remain inaccessible to React. Before custody is enabled for users, the remaining vault work includes:

- versioned migration with backup-before-upgrade behavior;
- user-facing recovery selection and explicit offline-storage guidance without clipboard or automatic cloud behavior;
- an independent cryptographic review.

The private Rust runtime now exists without registered commands. It owns the existing session, an
independent Windows kernel wallet mutex, main-window operation exclusion, generation-safe permits,
one generation-bound pending native selection, and one short-lived recovery authorization. The
Rust-only native save/open adapters validate local non-reparse Windows paths and never return paths
to React; no wallet command exposes them. Page load/reload, main-window close/destruction,
Windows session lock, suspend/standby, logoff/shutdown, teardown, and mutex poison synchronously
revoke authority. Unlock and resume never restore authority automatically. `SecretInput` provides a
bounded, zeroizing future Rust request representation. `docs/WALLET_RUNTIME_SECURITY.md` records
the exact boundary.

The internal vault schema is version 2 and is device-bound. Copying the local vault file to another Windows user or computer is intentionally insufficient for recovery. The internal portable recovery schema is version 1 and deliberately excludes the DPAPI device factor. Its tests prove that the encrypted artifact restores the exact original opaque seed and Vision account identity using only its recovery password. The approved onboarding policy requires a different recovery password of at least 16 bytes, never overwrites the selected destination, never creates arbitrary parent directories, reloads the stored artifact through a bounded regular-file parser, and withholds local-vault storage until the restored identity matches. The artifact has no Tauri command, frontend state, automatic export, clipboard path, or file-picker workflow. Portable backups rely on encryption rather than device-bound filesystem permissions so that offline recovery remains possible on another machine. Because a password-only artifact becomes an offline guessing target if stolen, the future UI must direct the user to an explicitly selected offline destination.

## Recovery requirements

The current Desktop recovery contract is the versioned encrypted portable artifact, not a mnemonic. The exact supported RC2 source confirms that the restored 32-byte seed is used directly as an Ed25519 signing seed and that its address is the 64-character lowercase hexadecimal public key. Fixed tests now verify seed-to-public-key-to-address derivation and require portable recovery to reproduce the identical address.

The repository contains conflicting historical phrase descriptions, so no mnemonic is selected or implied. If a mnemonic is added later, its word list, normalization, checksum, and phrase-to-seed algorithm require a separately approved contract and cross-platform vectors. The legacy browser wallet derivation remains unapproved.

The verified identity, amount, nonce/fee, serialization, transaction-identifier, signature, submission-response, receipt-observation, local-activity, recovery-gated onboarding, and private lifecycle contracts do not unlock user-facing transaction signing. The conservative confirmation presentation, limited local-history model, recovery-backup ordering, separate-password rule, and requirement to wait for loopback-only Core binding were approved on 2026-08-01. Wallet creation and restore remain inaccessible to the frontend until the narrow Tauri lifecycle is reviewed; signing and submission additionally remain disabled until private-loopback compatibility and the independent security review pass.

## Review gates

Before any release can hold real funds:

1. Core compatibility vectors pass in Rust and against the supported Core binary.
2. Vault format and key lifecycle receive an independent security review.
3. Create, restore, lock, unlock, send, rejection, nonce race, corruption, and recovery tests pass.
4. Logs, diagnostics, reducer state, frontend storage, support packages, and crash paths are scanned for secret leakage.
5. The release is code-signed and built through the controlled release process.

This architecture reduces risk but does not claim that unaudited software is equivalent to certified hardware. Hardware-wallet support remains a recommended future defense-in-depth option.

The future WebView-to-Rust custody boundary is specified in `docs/WALLET_TAURI_COMMAND_THREAT_MODEL.md`. It defines explicit command permissions, a main-window-only capability, Rust-side native file selection, opaque path tokens, secret request handling, lifecycle locking, production CSP separation, and the conditions that keep every wallet command unregistered.
