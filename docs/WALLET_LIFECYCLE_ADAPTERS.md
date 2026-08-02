# Private Wallet Lifecycle Adapters

## Status

Vision Desktop now constructs private Rust lifecycle adapters for wallet status, creation,
restoration, unlock, and lock. They are managed by Tauri only as inaccessible Rust state. None is
registered as a Tauri command, no wallet permission exists, `coreApi.ts` has no lifecycle wrapper,
and React still cannot submit a password, select a recovery file, create a vault, or unlock a
session.

This implementation connects the previously reviewed cryptographic, recovery, storage, session,
runtime, and native-selection primitives before an IPC boundary is permitted to reach them.

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
purpose-bound token. The lifecycle never receives a raw frontend path.

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
2. Consume the recovery-destination authorization before secret processing.
3. Convert the bounded zeroizing secret inputs into Rust-only passwords.
4. Generate the seed and separately encrypt the current-user-protected local vault and portable recovery artifact.
5. Store the recovery artifact with create-new semantics.
6. Read it back through the bounded parser, decrypt it, and prove the same Vision identity.
7. Store the local vault with create-new semantics.
8. Retain only public metadata and return a locked status.

The adapter checks its operation generation around every sensitive stage and every filesystem
write. Session lock, suspend, main-window loss, explicit invalidation, or stale work prevents later
stages from being accepted. A narrow race may allow an already-entered handle-bound write to
finish, but create-new storage prevents replacement and the result remains locked and inaccessible.

Deterministic interruption tests invalidate runtime authority after destination/source consumption,
encrypted preparation, recovery storage and verification, and local-vault storage. They prove that
no later stage runs, the runtime remains locked with no accepted account metadata, existing files
are never replaced, restore never changes its source backup, and only files whose handle-bound
write already completed may remain. Retained recovery or vault artifacts are encrypted and are not
accepted as an unlocked session.

## Restore ordering

Restore consumes a source token, loads and decrypts the original bounded recovery artifact, derives
the same Vision account identity, encrypts a new current-user-protected vault under a new local password, and
stores it with create-new semantics. The source recovery file is read only and is never changed or
deleted. The local and recovery passwords must differ. Restore completes locked.

## Unlock and lock

Unlock loads and validates the fixed local vault, applies the existing escalating password-failure
backoff, decrypts the seed inside Rust, derives its public identity, and retains the seed only in
the zeroizing `WalletSession`. If cached public identity conflicts with the decrypted vault, the
session locks and the operation fails closed. Backoff duration and filesystem details are never
returned.

Lock is idempotent. It synchronously drops the unlocked seed and invalidates active operations,
pending native selections, and path authorizations before returning only `{ locked: true }`. It
does not inspect the vault or depend on storage status, so damaged or unavailable storage cannot
turn a completed lock into a reported failure. Current account and storage information must be
requested separately through status. The existing five-minute idle lock and Windows lifecycle
invalidation continue to apply.

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

## Activation gates

This is not user-facing custody. Activation still requires:

- an independent security review of cryptography, secret lifetime, IPC, storage, recovery, and
  lifecycle handling;
- a supported Vision Core release with verified private-loopback binding;
- explicit main-window-only wallet permissions and reviewed command registration;
- isolated frontend forms that clear password fields immediately and never enter shared state;
- packaging, recovery, crash, leak, and end-to-end compatibility tests.

Until those gates pass, the existing read-only Wallet page remains the only wallet UI.
