# Wallet IME Operator Qualification

## Purpose and boundary

This procedure qualifies the Rust-owned native secret windows against Microsoft Pinyin and the
Microsoft Japanese IME. It uses an ignored `cfg(test)` harness only. The harness is absent from
production builds, registers no Tauri command or permission, cannot create or unlock a Wallet, and
uses only the public test value `VisionImeTestOnly-1234`.

The automated Windows test first proves that `ImmAssociateContextEx(window, NULL, 0)` is applied to
the exact top-level window and that each focusable child is disassociated immediately after it is
created. It then uses balanced `ImmGetContext`/`ImmReleaseContext` handling to require a null input
context for both windows. Existing IME and input-language message rejection remains defense in
depth.

Real IME evidence is still required because message injection and context inspection cannot prove
the behavior of each installed Microsoft IME user interface.

## Preconditions

- Use the exact candidate commit and a clean worktree.
- Use a supported Windows 11 Client host under the documented single-interactive-session policy.
- Enable the Microsoft IME being tested through approved Windows administration. Do not alter
  Vision Desktop source, activation flags, permissions, or capabilities.
- Close real wallet software and never enter an actual password or recovery credential.
- Confirm the active input selector shows the intended Microsoft IME before each run.

## Automated association check

```powershell
cargo test --manifest-path src-tauri/Cargo.toml wallet::recovery_ceremony::tests::operating_system_reports_no_context_for_secret_window_or_focusable_children -- --exact --test-threads=1
```

The test must pass. A false result from association, a non-null context, or a child that inherits a
context fails the test.

## Microsoft Pinyin run

Select Microsoft Pinyin, then run:

```powershell
$env:VISION_WALLET_IME_QUALIFICATION_LABEL='microsoft-pinyin'
cargo test --manifest-path src-tauri/Cargo.toml wallet::recovery_ceremony::tests::real_windows_ime_operator_qualification_harness -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:VISION_WALLET_IME_QUALIFICATION_LABEL
```

## Microsoft Japanese IME run

Select Microsoft Japanese IME, then run:

```powershell
$env:VISION_WALLET_IME_QUALIFICATION_LABEL='microsoft-japanese'
cargo test --manifest-path src-tauri/Cargo.toml wallet::recovery_ceremony::tests::real_windows_ime_operator_qualification_harness -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:VISION_WALLET_IME_QUALIFICATION_LABEL
```

## Required operator observations

For both runs:

1. Record the exact commit, tree, Windows edition/build, test timestamp, and emitted keyboard-layout
   identifier.
2. In the password-capture dialog, verify that no IME composition, candidate, prediction, or
   conversion window appears. Enter `VisionImeTestOnly-1234` and continue.
3. In the recovery-acknowledgement dialog, verify the same absence of IME UI, enter the same public
   test value exactly, and complete the ceremony.
4. Confirm the test prints `VISION_WALLET_IME_QUALIFICATION_PASSED` for the intended label and exits
   successfully.
5. Repeat after cancelling each dialog once and confirm cancellation closes the dialog without a
   fallback text control. Cancellation is recorded separately from the successful evidence run.
6. Preserve the complete console transcript and hashes of any evidence files. Screenshots must
   contain only the public test value and no real wallet material.

## Evidence status

At the time this procedure was added, the development workstation exposed only the US keyboard
preload. No Microsoft Pinyin or Japanese IME was installed, so real operator qualification remains
pending and must not be represented as passed. Lifecycle command registration remains prohibited
until this evidence and another independent exact-commit review are complete.
