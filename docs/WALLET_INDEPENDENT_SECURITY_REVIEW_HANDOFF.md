# Vision Desktop Wallet - Independent Security Review Handoff

## Review target

Review the exact committed tree at:

`563a8cb8652c99e03968ae336727cd520cb8ab30`

The review must be performed by someone independent of the implementation work. This document is
a handoff inventory, not approval or security certification. Findings must identify the reviewed
commit and must be resolved or explicitly accepted before any wallet command is registered.

Vision-Core is outside this review except for verifying the already-recorded compatibility vectors
and the private-loopback integration gate. No reviewer should modify Vision-Core as part of this
Desktop review.

## Current activation state

- Wallet status, create, restore, unlock, and lock adapters are private Rust functions.
- No wallet lifecycle, signing, submission, recovery-selection, password, path-token, or secret
  command is registered with Tauri.
- React has no wallet custody permission and no access to passwords, secrets, recovery paths, or
  path tokens.
- Signing remains fail-closed while `PrivateLoopbackBinding` is unmet.
- The current review target must not be described as ready to hold real funds.

## Security changes leading to this target

- `5610dfb` - private wallet lifecycle adapters.
- `aea1158` - Windows Known Folder custody path validation.
- `0a9f07f` - handle-bound vault and recovery filesystem operations.
- `09e36fe` - storage-independent explicit wallet lock.
- `8ebbc2f` - accurate current-user DPAPI scope and protection decision.
- `563a8cb` - deterministic create/restore interruption checkpoints.

## Required source review

### Cryptography and public contracts

- `src-tauri/src/wallet/account.rs`
- `src-tauri/src/wallet/amount.rs`
- `src-tauri/src/wallet/contract.rs`
- `src-tauri/src/wallet/device_protection.rs`
- `src-tauri/src/wallet/recovery.rs`
- `src-tauri/src/wallet/transaction.rs`
- `src-tauri/src/wallet/vault.rs`

Review Argon2id parameters, random generation, XChaCha20-Poly1305 nonce and associated-data use,
password/local-factor key combination, DPAPI entropy and scope, format parsing limits, algorithm
agility, cryptographic error equivalence, Ed25519 construction, canonical serialization, and every
fixed Core-compatibility vector.

### Secret lifetime and runtime authority

- `src-tauri/src/wallet/secret_input.rs`
- `src-tauri/src/wallet/secrets.rs`
- `src-tauri/src/wallet/session.rs`
- `src-tauri/src/wallet/runtime.rs`
- `src-tauri/src/wallet/windows_lifecycle.rs`

Review zeroization boundaries, password copies, seed exposure closures, idle locking, failed-unlock
backoff, session replacement, mutex poison behavior, operation generations, process locking,
main-window ownership, token expiry/single use, suspend/session-lock handling, shutdown, panics, and
all unsafe Windows callbacks.

### Onboarding, storage, and recovery

- `src-tauri/src/wallet/lifecycle.rs`
- `src-tauri/src/wallet/onboarding.rs`
- `src-tauri/src/wallet/recovery_selection.rs`
- `src-tauri/src/wallet/secure_filesystem.rs`
- `src-tauri/src/wallet/storage_security.rs`

Review Known Folder resolution, fixed-volume enforcement, component and reparse validation,
directory-handle lifetime, handle access/share modes, handle-bound DACL application, non-replacing
publication, interruption outcomes, portable-recovery source/destination validation, path-token
binding, partial encrypted artifacts, and the absence of unsafe path-based cleanup.

### Submission, observation, and local records

- `src-tauri/src/wallet/submission.rs`
- `src-tauri/src/wallet/receipt.rs`
- `src-tauri/src/wallet/journal.rs`

Review transaction-identifier matching, nonce/replacement policy, ambiguous response rejection,
reorganization handling, conservative confirmation language, journal integrity/sequence limits,
and filesystem behavior. The journal must never become authoritative for balances, nonces,
transaction success, or complete account history.

### Tauri and WebView boundary

- `src-tauri/src/lib.rs`
- `src-tauri/tauri.conf.json`
- `src-tauri/capabilities/main-desktop.json`
- `src-tauri/tests/tauri_acl.rs`
- `src-tauri/tests/webview_security.rs`
- `src/services/coreApi.ts`

Review the exact command registration list, main-window-only permissions, plugin ordering,
production CSP, IPC-only frontend networking, support-package exclusions, and the absence of direct
wallet invocation from React. Reviewers should compare these files from the target commit, not from
an unrelated dirty working tree.

## DPAPI decision to review

Vault version 2 records `windows_dpapi_current_user`. It uses default current-user DPAPI plus an
independent Argon2id password-derived factor. It does not use machine-wide DPAPI because that scope
allows any local user to unwrap the DPAPI layer. It does not promise TPM or strict hardware binding;
Windows roaming profiles are a documented exception to usual same-computer behavior.

The reviewer must assess whether current-user DPAPI plus the independent password factor is
adequate for the intended first release. Any TPM, Windows Hello, CNG, or DPAPI-NG change requires a
new vault version, migration and recovery policy, hardware-availability behavior, tests, and a new
review. It must not be introduced as an unversioned implementation detail.

## Deterministic evidence

At the target commit, the Windows validation baseline is:

- 121 Rust unit tests pass.
- 5 Tauri authority tests pass.
- 2 production WebView security tests pass.
- strict Clippy passes with warnings denied.
- Rust formatting check passes.
- frontend typecheck and state-transition tests pass.

The lifecycle tests inject invalidation after create destination consumption, encrypted
preparation, recovery storage, recovery verification, and vault storage, and after restore source
consumption, encrypted preparation, and vault storage. They assert that later stages do not run,
the runtime remains locked without accepted account metadata, source recovery bytes never change,
destinations are never overwritten, and only completed encrypted writes may remain.

Run at minimum:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
npm run typecheck
npm run test:state
npm run build
git diff --check
```

Reviewers should also perform process-kill, power-loss, Windows session-lock, suspend/resume,
second-instance, corrupted-vault, wrong-password, reparse-point, destination-race, and recovery on a
separate clean test account. Unit tests do not prove physical storage durability or resistance to a
fully compromised Windows user session.

## Known blockers and limitations

1. Independent review is not yet complete.
2. Wallet commands and wallet permissions remain deliberately unregistered.
3. The supported Core release still must prove private loopback-only binding end to end.
4. Signing primitives are not connected to the unlocked session or a reviewed command boundary.
5. Vault-format migration and backup-before-upgrade behavior are not implemented.
6. Strict hardware binding is not provided or claimed by vault version 2.
7. Portable recovery is an offline password-guessing target if stolen; UI guidance and real-device
   recovery drills remain required.
8. Deterministic checkpoints model lifecycle invalidation between sensitive stages; sudden process
   termination during a Windows system call still requires external fault testing.
9. The local activity journal is incomplete by design. Version 2 adds wallet-seed authentication and an authenticated event chain, but complete-prefix rollback and storage/race behavior still require review
   before send activation.

## Required reviewer output

The independent report should include:

- reviewer identity and independence statement;
- exact reviewed commit;
- threat model and assumptions;
- findings with severity, file, line, exploit scenario, and recommended correction;
- cryptographic construction verdict;
- secret-lifetime and panic-boundary verdict;
- Windows storage, DPAPI, ACL, reparse, and process-lock verdict;
- IPC input, permission, CSP, logging, crash, and support-package verdict;
- recovery and transaction-signing verdict;
- residual risks and explicit release blockers;
- signed conclusion stating either approval for command-boundary work or required changes.

No wallet command may be registered merely because this handoff exists. Command-boundary work can
begin only after the independent report is received, its blocking findings are resolved, and the
reviewer confirms the final corrected commit.
