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
- Lifecycle and signing use separate runtime activation scopes. Lifecycle approval covers only local
  create, restore, unlock, lock, recovery selection, and public account identity. Signing is a
  strict superset and validates its scope again inside the signer. Both production approvals remain
  false; see `WALLET_ACTIVATION_SCOPES.md`.
- Wallet creation must not become available until recovery produces the identical public key and address in an independent vector test.
- Amounts are strings or bounded integers in confirmed smallest units; floating-point blockchain arithmetic is forbidden.
- Transaction success requires an explicit accepted response and later observed receipt/finality state. Missing errors are not success.
- A nonce conflict causes a fresh read and a new user review; Desktop does not silently re-sign a changed transaction.
- No automatic signing, submission, retry, password capture, recovery export, clipboard copy, or unlock is allowed.
- Support packages must never include wallet directories, vault contents, public-to-secret mappings, or secret-bearing errors.

Support-package generation now enforces this boundary structurally. It does not read Core log tails,
does not traverse report or data directories, and does not serialize complete Desktop state or node
configuration. An exact file allowlist is built in memory, scanned for forbidden custody markers,
and written to both the report directory and ZIP from the same classified buffers. Public wallet
addresses, transaction activity, nonces, local wallet paths, vaults, recovery files, activity
journals, DPAPI material, passwords, session/path tokens, and activation authority are absent by
construction. See `docs/WALLET_SUPPORT_PACKAGE_SECURITY.md`.

## Encrypted vault status

The first encrypted vault foundation is implemented inside the Rust wallet module but is not exposed through Tauri commands. It includes:

- Argon2id password-based key derivation with fixed, validated cost parameters;
- XChaCha20-Poly1305 authenticated encryption with a unique random salt and nonce for every vault;
- a random local protection key wrapped by current-user DPAPI with authenticated optional entropy, combined with the Argon2id password key so neither factor is sufficient alone;
- authenticated vault metadata so identifiers and cryptographic parameters cannot be altered silently;
- encrypted-only, create-new file storage that never overwrites an existing vault, bounded file sizes, and corruption detection;
- handle-bound Windows vault and recovery I/O that holds ancestor directories against rename, rejects reparse traversal, applies the vault DACL and performs non-replacing atomic publication through the staging handle, and avoids path-based deletion after failed writes;
- fail-closed storage permissions: Unix `0700` wallet directories and `0600` vault files; protected Windows DACLs granting full control only to the owner, SYSTEM, and local Administrators;
- identical non-revealing errors for an incorrect password and damaged ciphertext;
- Rust-only unlocked sessions with explicit lock and five-minute idle auto-lock enforcement before every secret operation;
- escalating unlock backoff after repeated incorrect-password or damaged-vault results;
- an internal versioned portable recovery artifact that encrypts the same opaque seed with an independent Argon2id password key and XChaCha20-Poly1305, without DPAPI or machine binding;
- a Rust-only onboarding coordinator that generates an independent 256-bit portable recovery credential, requires an explicit create-new backup destination, presents and verifies that credential in a main-window-parented native Windows ceremony before any file publication, reads the subsequently saved artifact back, restores it in memory, and proves equality of the Vision account identity before the local vault can be stored;
- no plaintext temporary files or crash-report inclusion.

Argon2 memory is now caller-owned rather than allocated by the convenience API. The dependency's
`zeroize` feature clears its internal initial/final hashes, while a dedicated RAII workspace wipes
all memory-hard blocks before the allocation is released on normal return, error, or unwinding.
Vault and portable-recovery derivation share this implementation. AEAD initialization borrows the
already-zeroizing combined key instead of creating a separate caller-owned cipher-key buffer. See
`docs/WALLET_KDF_MEMORY_SECURITY.md`.

The Rust wallet module now also contains an internal RC2-compatible `cash::transfer` builder, exact 9-decimal amount conversion, conservative current-nonce and zero-tip fee policy, canonical bincode serializer, BLAKE3 transaction-identifier function, Ed25519 signer, strict submission-response parser, canonical receipt observer, and public-metadata-only local activity journal backed by exact Core and independent fixed vectors. These primitives are deliberately not registered as Tauri commands, are not connected to the unlocked-session manager, and cannot be invoked by React. They accept only exact integer units and a narrowly typed transfer draft; arbitrary modules or methods are not exposed. Unknown response shapes, mismatched transaction identifiers, unexpected accepted nonces, unapproved replacements, inconsistent returned transactions, invalid block references, damaged activity records, and unknown journal schemas fail closed.

The activity journal schema is now version 2. Every event is authenticated with a domain-separated BLAKE3 keyed hash using a dedicated subkey derived from the wallet seed, and every event authenticates the preceding event tag. The journal verifies exact wallet identity, sequence, chain order, and event content before exposing local records; an accepted transaction must also have the sender address derived from the authenticating seed. Windows reads are handle-bound beneath a held non-reparse directory chain. Updates use a protected, flushed, random create-new staging file and atomic handle-based replacement rather than modifying the live journal in place, so interruption before publication preserves the prior complete journal. Alternate data streams and reparse-point journal files fail closed. A restrictive per-user `Global\` Windows process lease now excludes a second wallet runtime across console, fast-user-switching, and RDP sessions before it can read or replace journal state. The journal remains size-bounded, synchronized within the Desktop process, and stored with restrictive filesystem permissions. It records only public transfer metadata after an exact accepted submission and later validated receipt observations. It never stores signed bytes, signatures, passwords, seeds, recovery material, or vault contents, and it never supplies balances, nonces, or signing decisions. Complete rollback to an older authentic journal prefix is not yet detectable because no external authenticated head anchor exists. Its `High confidence` presentation at 50 canonical confirmations is a Desktop diagnostic policy, never a claim of deterministic finality.

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
revoke authority. An atomic pending-revocation counter stays nonzero until all overlapping
invalidations finish, and the revocation epoch is advanced before each invalidation waits for the runtime
mutex; sensitive lifecycle stages check it both before execution and before releasing results, and
final completion is linearized against revocation. Unlock and resume never restore authority
automatically. `SecretInput` provides a bounded, zeroizing future Rust request representation.
`docs/WALLET_LIFECYCLE_REVOCATION.md` and `docs/WALLET_RUNTIME_SECURITY.md` record the exact boundary.

The internal vault schema is version 2 and records the existing `windows_dpapi_current_user` protection algorithm. Copying the local vault file alone is insufficient for recovery: decryption also requires the wallet password and the DPAPI-protected local factor. The internal portable recovery schema is version 1 and deliberately excludes that factor. Its tests prove that the encrypted artifact restores the exact original opaque seed and Vision account identity using a Rust-generated 256-bit recovery credential. Arbitrary user-chosen recovery passwords are rejected. The credential has an exact lowercase, versioned format and a 32-bit BLAKE3-derived checksum for transcription-error detection; the checksum adds no entropy. The onboarding flow never overwrites the selected destination, never creates arbitrary parent directories, and now performs credential display and exact re-entry entirely in native Rust-owned Windows UI before any file publication. It then reloads the stored artifact through a bounded regular-file parser and withholds local-vault storage until the restored identity matches. Cancellation or revocation before acknowledgement leaves no backup and no vault. The artifact and credential have no Tauri command, frontend state, automatic export, or clipboard path. Portable backups rely on encryption rather than local filesystem permissions so that offline recovery remains possible on another machine. See `docs/WALLET_RECOVERY_ACKNOWLEDGEMENT.md`.

### Windows DPAPI scope decision

Vision Desktop calls `CryptProtectData` with its default current-user scope and does not request
machine-wide scope. Microsoft documents that default DPAPI normally requires matching user logon
credentials and usually the same computer, but explicitly identifies roaming profiles as an
exception. Therefore version 2 is described as current-user DPAPI-protected, not guaranteed
hardware- or device-bound. See Microsoft's
[`CryptProtectData` documentation](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata).

Machine-wide DPAPI is intentionally rejected: Microsoft documents that any user on that computer
could then unwrap the DPAPI layer, which would weaken per-user isolation. Strict TPM-backed binding
is not a version 2 activation requirement. Adding it later would require a new versioned vault
format, hardware-availability and recovery policy, migration behavior, deterministic tests, and an
independent review. No current code or documentation may claim that version 2 is hardware-bound.

## Recovery requirements

The current Desktop recovery contract is the versioned encrypted portable artifact, not a mnemonic. The exact supported RC2 source confirms that the restored 32-byte seed is used directly as an Ed25519 signing seed and that its address is the 64-character lowercase hexadecimal public key. Fixed tests now verify seed-to-public-key-to-address derivation and require portable recovery to reproduce the identical address.

The repository contains conflicting historical phrase descriptions, so no mnemonic is selected or implied. If a mnemonic is added later, its word list, normalization, checksum, and phrase-to-seed algorithm require a separately approved contract and cross-platform vectors. The legacy browser wallet derivation remains unapproved.

The verified identity, amount, nonce/fee, serialization, transaction-identifier, signature, submission-response, receipt-observation, local-activity, recovery-gated onboarding, and private lifecycle contracts do not unlock user-facing transaction signing. The conservative confirmation presentation, limited local-history model, recovery-backup ordering, generated-credential rule, and requirement to wait for loopback-only Core binding are current policy. Wallet creation and restore remain inaccessible to the frontend until the implemented native credential ceremony and narrow Tauri lifecycle are independently reviewed; signing and submission additionally remain disabled until private-loopback compatibility and the independent security review pass.

## Review gates

Before any release can hold real funds:

1. Core compatibility vectors pass in Rust and against the supported Core binary.
2. Vault format and key lifecycle receive an independent security review.
3. Create, restore, lock, unlock, send, rejection, nonce race, corruption, and recovery tests pass.
4. Logs, diagnostics, reducer state, frontend storage, support packages, and crash paths are scanned for secret leakage.
5. The release is code-signed and built through the controlled release process.

This architecture reduces risk but does not claim that unaudited software is equivalent to certified hardware. Hardware-wallet support remains a recommended future defense-in-depth option.

The future WebView-to-Rust custody boundary is specified in `docs/WALLET_TAURI_COMMAND_THREAT_MODEL.md`. It defines explicit command permissions, a main-window-only capability, Rust-side native file selection, opaque path tokens, secret request handling, lifecycle locking, production CSP separation, and the conditions that keep every wallet command unregistered.
