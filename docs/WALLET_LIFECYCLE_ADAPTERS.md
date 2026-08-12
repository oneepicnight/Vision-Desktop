# Private Wallet Lifecycle Adapters

## Status

Vision Desktop now constructs private Rust lifecycle adapters for wallet status, creation,
restoration, unlock, and lock. They are managed by Tauri only as inaccessible Rust state. None is
registered as a Tauri command, no wallet permission exists, `coreApi.ts` has no lifecycle wrapper,
and React still cannot submit a password, select a recovery file, create a vault, or unlock a
session.

This implementation connects the previously reviewed cryptographic, recovery, storage, session,
runtime, and native-selection primitives. Commit `b027d18` received independent approval for this
private lifecycle-only foundation. It did not approve any IPC or user-facing secret boundary.

## Local storage contract

The first lifecycle supports one local, current-user DPAPI-protected vault at:

`%LOCALAPPDATA%\Vision\Desktop\wallet\wallet.vault.json`

The path is resolved through Tauri's Windows Known Folder implementation for
`FOLDERID_LocalAppData`; it does not trust the `LOCALAPPDATA` process environment variable and is
never supplied by React. Startup rejects relative, UNC, verbatim/device, removable, remote, and
non-fixed-volume roots, as well as any existing root component that is not a real directory or is
a Windows reparse point. Vault storage retains the existing create-new, no-overwrite, bounded-file,
DPAPI, and verified Windows DACL protections. A missing vault is reported as a locked,
uninitialized lifecycle. An existing invalid or insecure vault fails closed as unavailable.

The portable recovery file is different. Its destination or source must come from the existing
main-window-parented native dialog flow and be redeemed by a matching two-minute, single-use,
purpose-bound capability handle. The non-secret handle may eventually cross React; the lifecycle
never receives a raw frontend path.

On Windows, vault and recovery operations now hold every ancestor directory open without delete
sharing for the duration of the operation. Each final file is opened with reparse traversal
disabled, validated from its handle, and read or written through that same handle. Vault
publication uses a random encrypted temporary file whose handle denies delete and write sharing.
The restrictive DACL, atomic non-replacing rename, and exact encrypted-byte read-back all operate
through that same handle. A pathname cannot redirect which staging file is published, and an
existing vault destination cannot be replaced.

If a vault write fails before publication, a randomly named encrypted staging file may remain in
the protected wallet directory. The writer deliberately does not delete it by pathname after
releasing its validated handle. It is never treated as the canonical vault and contains only the
already-encrypted vault envelope.

If a recovery write fails or the process stops during it, a partial encrypted destination may
remain. The writer deliberately does not delete that file by path after releasing its validated
handle because doing so would create a substitution/deletion race. Existing destinations are never
reused; the operator must choose a different new destination or deliberately remove the failed
artifact outside the wallet flow.

## Create ordering

Create performs these steps inside one main-window-owned exclusive runtime operation:

1. Prove that no local vault already exists.
2. Consume the recovery-destination authorization before prompting or secret processing.
3. Capture and confirm the local password through the Rust-owned native design in
   `WALLET_NATIVE_SECRET_CEREMONY_DESIGN.md`, then transfer it directly into the private adapter.
4. Generate the seed and an independent 256-bit portable recovery credential directly from the operating-system random source.
5. Separately encrypt the current-user-protected local vault and portable recovery artifact.
6. Display the generated credential in a Rust-owned, main-window-parented native Windows ceremony and require an exact user re-entry.
7. Clear the native input and encoded Rust presentation buffer after acknowledgement.
8. Store the recovery artifact with create-new semantics only after acknowledgement.
9. Read it back through the bounded parser, decrypt it, and prove the same Vision identity.
10. Store the local vault with create-new semantics.
11. Retain only public metadata and return a locked status; the credential is never part of the lifecycle result.

Cancellation, native-UI failure, or runtime invalidation during the acknowledgement ceremony occurs before either filesystem write. The operation returns a fixed failure, retains no public account metadata, and leaves both the selected recovery destination and canonical vault absent. The ceremony uses no WebView, Tauri command, frontend state, clipboard API, command-line output, or support-package path. The credential is intentionally not auto-copied; the operator must record it offline and prove possession by re-entering it.

The adapter checks its operation generation around every sensitive stage and every filesystem
write. Session lock, suspend, main-window loss, explicit invalidation, or stale work prevents later
stages from being accepted. A narrow race may allow an already-entered handle-bound write to
finish, but create-new storage prevents replacement and the result remains locked and inaccessible.

Deterministic interruption tests invalidate runtime authority after destination/source consumption,
encrypted preparation, native recovery acknowledgement, recovery storage and verification, and local-vault storage. They prove that
no later stage runs, the runtime remains locked with no accepted account metadata, existing files
are never replaced, restore never changes its source backup, and only files whose handle-bound
write already completed may remain. Retained recovery or vault artifacts are encrypted and are not
accepted as an unlocked session.

## Restore ordering

Restore consumes a source capability handle before prompting, captures the exact recovery
credential plus a new local password and confirmation through a Rust-owned native ceremony, loads
and decrypts the original bounded recovery artifact, derives the same Vision account identity,
encrypts a new current-user-protected vault, and stores it with create-new semantics. The source
recovery file is read only and is never changed or deleted. Restore accepts only the exact
versioned, checksummed credential format generated by Rust; legacy arbitrary recovery passwords are
rejected before the KDF. Restore completes locked.

## Unlock and lock

Unlock captures the local password through a Rust-owned native ceremony, loads and validates the
fixed local vault, applies the existing escalating password-failure backoff, decrypts the seed
inside Rust, derives its public identity, and retains the seed only in the zeroizing
`WalletSession`. If cached public identity conflicts with the decrypted vault, the session locks
and the operation fails closed. Backoff duration and filesystem details are never returned.

Lock is idempotent. It synchronously drops the unlocked seed and invalidates active operations,
pending native selections, and path authorizations before returning only `{ locked: true }`. It
does not inspect the vault or depend on storage status, so damaged or unavailable storage cannot
turn a completed lock into a reported failure. Current account and storage information must be
requested separately through status. The existing five-minute idle lock and Windows lifecycle
invalidation continue to apply.

Create, restore, and unlock now execute each cryptographic, recovery, vault, metadata, and unlock
stage through the runtime's generation- and epoch-bound authority closure. Invalidation advances
the epoch before it waits for the runtime mutex. Every stage checks authority before execution and
again before returning its result, and each lifecycle result passes a final atomic completion check.
This prevents a lock, suspend, session change, teardown, or concurrent invalidation from being
followed by an accepted stale credential, unlocked status, metadata update result, or success
response. Irreversible create-new or atomic publication that completed before cancellation may
remain on disk, but the operation fails closed and must be reconciled explicitly on the next run.

Every private lifecycle entry point is enclosed by the fail-closed unwind guard in
`WALLET_NATIVE_SECRET_CEREMONY_DESIGN.md`. The guard invalidates the complete runtime on every
uncommitted return or panic, including a panic immediately after session unlock. A non-emitting
panic policy is installed before Wallet initialization, and the boundary returns one fixed error.
Recovery-selection initiation, callback completion, and uncommitted permit drop apply the same
fail-closed rule.

The production create and restore entry points consume the bounded public request schemas directly.
Public validation completes before vault inspection, runtime mutation, capability consumption,
native prompting, filesystem access, or cryptographic work. Raw-string helpers are test-only.

## Public metadata and restart behavior

Lifecycle responses contain only:

- whether the local vault exists;
- whether the session is locked;
- optionally, wallet identifier, public key, public address, creation time, label, and backup state.

The vault intentionally stores neither label nor backup-verification metadata. During the process
that creates or restores a wallet, those two values are known and may be reported. After an
application restart, status reports no account metadata until a successful unlock derives the
public identity. The derived identity then reports label and backup state as unknown. Desktop does
not invent either value or claim ownership merely because a public address is present.

No public metadata store was added in this slice. Designing a separately integrity-bound metadata
record is a future review decision.

## Fixed failures

The lifecycle translates lower-level failures into fixed codes and operator-safe messages. It does
not return paths, operating-system errors, passwords, ciphertext, wallet material, raw retry
durations, or recovery contents. Incorrect wallet passwords and damaged encrypted data remain
indistinguishable at the boundary.

The lifecycle adapters cannot obtain a reusable activation-proof reference. The proof is exposed
only inside the runtime-controlled stage closure, preventing future internal wiring from separating
authority validation from the sensitive operation it authorizes.

## Activation gates

This is not user-facing custody. Activation still requires:

- independent approval and exact-implementation review of the native secret ceremonies, bounded
  public requests, unwind guard, IPC, storage, and recovery handling;
- a supported Vision Core release with verified private-loopback binding;
- explicit main-window-only wallet permissions and reviewed command registration;
- no frontend secret forms or secret-bearing Tauri arguments;
- independently reviewed signing, submission, receipt tracking, and a complete spending path; and
- packaging, clean-device recovery, crash, leak, and end-to-end compatibility tests.

Until those gates pass, the existing read-only Wallet page remains the only wallet UI.
