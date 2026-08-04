# Native Wallet Recovery Acknowledgement

## Status

Vision Desktop now contains a private Rust-owned Windows recovery-credential ceremony for new-wallet creation. It remains unreachable from React because no wallet Tauri command or permission is registered and independent security review approval remains false.

## Required sequence

Creation is one exclusive, main-window-owned runtime operation:

1. Confirm the canonical vault does not exist.
2. Consume a short-lived native recovery-destination token.
3. Generate the seed, local encrypted vault, 256-bit recovery credential, and encrypted portable artifact in Rust memory.
4. Display the encoded credential in a native Windows modal owned by the real main-window handle.
5. Require exact re-entry into a masked native edit control.
6. Recheck runtime authority and clear the native input and zeroizing Rust presentation buffer.
7. Publish the encrypted recovery artifact with create-new semantics.
8. Read it back through the bounded, handle-protected parser, decrypt it, and verify the exact Vision address.
9. Publish the canonical local vault with create-new semantics.
10. Retain only public metadata and complete in the locked state.

The lifecycle result contains no recovery credential.

## Failure and restart behavior

Cancellation, inability to create trusted native UI, session lock, suspend, main-window teardown, explicit lock, or any other runtime revocation before acknowledgement returns a fixed failure before either filesystem write. The selected destination remains absent, the canonical vault remains absent, and no public account metadata is accepted.

After acknowledgement, an interruption can leave only a completed encrypted recovery artifact or completed encrypted vault according to the already documented handle-bound create-new publication rules. A backup can precede the vault, but a vault can never precede both acknowledgement and successful backup read-back verification. Restart never treats a noncanonical or partial artifact as a wallet.

## Secret boundary

- The credential is rendered only in Rust-owned Win32 controls.
- The full value never enters React state, events, reducers, Tauri payloads, logs, support packages, command-line arguments, or clipboard APIs.
- The input control is cleared before every normal, cancellation, mismatch, revocation, and close path.
- Rust UTF-16 and UTF-8 presentation buffers use zeroizing ownership.
- Re-entry comparison examines the exact full credential and does not accept prefixes, normalization, or shortened values.
- Mismatch text contains no credential content.

Windows control internals and process memory remain within the operating-system trust boundary. Release qualification still requires optimized-build memory characterization and real Windows interruption tests.

## Automated evidence

Tests prove:

- exact versus changed credential comparison;
- no clipboard API in the production ceremony implementation;
- cancellation produces no recovery artifact or vault;
- native-UI failure produces no recovery artifact or vault;
- authority revocation during the ceremony produces no recovery artifact or vault;
- interruption immediately after acknowledgement still precedes both publications;
- the successful flow publishes and verifies recovery before the vault;
- lifecycle creation returns public locked status only;
- static Tauri authority checks enforce ceremony → recovery → vault ordering and keep wallet IPC closed.

## Remaining gates

This correction is submitted for independent re-review of H-RR-02. It does not approve lifecycle integration, set the independent-review flag, register wallet commands, grant permissions, expose passwords, enable signing, or enable sending. Real Windows session-lock, suspend, termination, power-loss, recovery-drill, and installer qualification remain mandatory before user custody.
