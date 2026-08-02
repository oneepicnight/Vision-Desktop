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

The first lifecycle supports one local, device-bound vault at:

`%LOCALAPPDATA%\Vision\Desktop\wallet\wallet.vault.json`

The path is derived inside Rust and is never supplied by React. Vault storage retains the existing
create-new, no-overwrite, bounded-file, DPAPI, and verified Windows DACL protections. A missing
vault is reported as a locked, uninitialized lifecycle. An existing invalid or insecure vault
fails closed as unavailable.

The portable recovery file is different. Its destination or source must come from the existing
main-window-parented native dialog flow and be redeemed by a matching two-minute, single-use,
purpose-bound token. The lifecycle never receives a raw frontend path.

## Create ordering

Create performs these steps inside one main-window-owned exclusive runtime operation:

1. Prove that no local vault already exists.
2. Consume the recovery-destination authorization before secret processing.
3. Convert the bounded zeroizing secret inputs into Rust-only passwords.
4. Generate the seed and separately encrypt the device vault and portable recovery artifact.
5. Store the recovery artifact with create-new semantics.
6. Read it back through the bounded parser, decrypt it, and prove the same Vision identity.
7. Store the local vault with create-new semantics.
8. Retain only public metadata and return a locked status.

The adapter checks its operation generation around every sensitive stage and every filesystem
write. Session lock, suspend, main-window loss, explicit invalidation, or stale work prevents later
stages from being accepted. A narrow race may allow an already-entered atomic file write to finish,
but create-new storage prevents replacement and the result remains locked and inaccessible.

## Restore ordering

Restore consumes a source token, loads and decrypts the original bounded recovery artifact, derives
the same Vision account identity, encrypts a new device-bound vault under a new local password, and
stores it with create-new semantics. The source recovery file is read only and is never changed or
deleted. The local and recovery passwords must differ. Restore completes locked.

## Unlock and lock

Unlock loads and validates the fixed local vault, applies the existing escalating password-failure
backoff, decrypts the seed inside Rust, derives its public identity, and retains the seed only in
the zeroizing `WalletSession`. If cached public identity conflicts with the decrypted vault, the
session locks and the operation fails closed. Backoff duration and filesystem details are never
returned.

Lock is idempotent. It synchronously drops the unlocked seed and invalidates active operations,
pending native selections, and path authorizations before returning current public status. The
existing five-minute idle lock and Windows lifecycle invalidation continue to apply.

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
