# Wallet Transaction Confirmation Operator Qualification

## Purpose and boundary

This procedure qualifies the private Rust-owned native transaction-confirmation window on real
Windows. It uses one ignored `cfg(test)` harness. The harness is absent from production builds,
registers no Tauri command or permission, cannot create or unlock a wallet, cannot access a seed,
cannot sign or submit, and displays only fixed public test values.

Passing automated tests is necessary but does not replace this operator evidence. Signing remains
blocked until an independent reviewer accepts the exact candidate commit and the complete evidence.

## Preconditions

- Use the exact candidate commit and a clean worktree on a supported Windows 11 Client host.
- Use the documented standard single-interactive-session Windows policy.
- Close real wallet software. Never enter a password, recovery credential, private key, or seed.
- Disconnect or disable unneeded remote-control and automation software for physical-input runs.
- Record the commit, tree, Windows edition/build, display resolution and scaling, active keyboard
  layout, timestamp, and operator identity.
- Preserve the complete console transcript. Screenshots may contain only the fixed public values.

## Command template

Run each scenario separately from the repository root:

```powershell
$env:VISION_WALLET_CONFIRMATION_SCENARIO='<scenario>'
$env:VISION_WALLET_CONFIRMATION_EVIDENCE_LABEL='<short-label>'
$env:VISION_WALLET_CONFIRMATION_INPUT_PROFILE='<input-profile>'
cargo test --manifest-path src-tauri/Cargo.toml wallet::transaction_confirmation::tests::real_windows_transaction_confirmation_operator_harness -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:VISION_WALLET_CONFIRMATION_SCENARIO
Remove-Item Env:VISION_WALLET_CONFIRMATION_EVIDENCE_LABEL
Remove-Item Env:VISION_WALLET_CONFIRMATION_INPUT_PROFILE
```

Allowed scenarios are `mouse`, `keyboard`, `held-enter`, `injected-enter`, `cancel`, and `revoke`.
Allowed input profiles are `us`, `microsoft-pinyin`, and `microsoft-japanese`; each is bound to its
expected Windows keyboard-layout identifier. Every successful run prints
`VISION_WALLET_CONFIRMATION_QUALIFICATION_PASS` with its scenario, evidence label, declared input
profile, active keyboard-layout identifier, DPI-awareness context, and actual confirmation-window
DPI. It also records the accepted physical input device. A `keyboard` or `held-enter` run fails if
a mouse completed confirmation, and a `mouse` run fails if a keyboard completed it.

## Production-equivalent DPI boundary

The pinned production window runtime is TAO 0.35.3. On supported Windows 11, TAO first requests
`DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` before creating its event-loop windows. The ignored
harness must establish that same Per-Monitor V2 process context before creating any HWND; it does
not accept DPI virtualization or a 96-DPI default as equivalent.

Each run queries and requires Per-Monitor V2 equivalence for the process, current thread, owner
window, actual confirmation window, Confirm button, and Cancel button. It prints every context and
records `GetDpiForWindow` for both the owner and actual confirmation HWND. The two windows must have
the same nonzero monitor DPI. Context establishment, inheritance, or window-DPI disagreement fails
the harness. This follows Microsoft's
[`SetProcessDpiAwarenessContext`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext)
and [`GetDpiForWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow)
contracts.

## Required baseline runs

At the workstation's normal resolution and scaling, set the US keyboard layout, use the `us` input
profile, and run all six scenarios:

1. `mouse`: inspect every value and both buttons for clipping, overlap, truncation, or substitution;
   click Confirm exactly once with a physical mouse.
2. `keyboard`: inspect the same content; press and release Enter once on the focused Confirm control.
3. `held-enter`: keep the Enter used to launch the command held while the dialog appears. Repeated
   input and the eventual release must not confirm. Only a fresh press-and-release may confirm.
4. `injected-enter`: do not touch input until the harness prints
   `VISION_WALLET_INJECTED_INPUT_REJECTED`. The injected Enter must not close the dialog. Confirm
   physically only after that marker appears.
5. `cancel`: cancel or close the dialog and verify no confirmed result is produced.
6. `revoke`: provide no input. The dialog must close after test authority is revoked and must not
   produce confirmation.

## DPI and layout matrix

Repeat at least the `mouse`, `keyboard`, and `cancel` scenarios at every supported Windows display
scaling value that can be configured on the qualification workstation, including 100%, 125%, 150%,
and 200% when the display supports them. Test the smallest supported display resolution separately.
Every sender, recipient, amount, fee, total, nonce, transaction identifier, warning, and button must
remain completely visible and distinguishable.

Repeat `keyboard`, `held-enter`, and `cancel` with every other supported non-IME international
keyboard layout installed for qualification where an explicit harness input profile is available.

## Transaction-confirmation IME matrix

The transaction window has its own owner-drawn procedure and its own guarded
`WM_IME_SETCONTEXT` path. Secret-entry IME evidence from another window does not qualify it.

Enable Microsoft Pinyin, declare `microsoft-pinyin`, verify that the emitted layout identifier is
`00000804`, and run `keyboard`, `cancel`, and `revoke`. Then enable Microsoft Japanese IME, declare
`microsoft-japanese`, verify `00000411`, and run the same three scenarios.

For each of the six IME runs, verify and record that:

1. Normal focus does not close the confirmation window.
2. The dialog displays every exact value without composition, candidate, prediction, conversion,
   guide, or soft-keyboard UI.
3. The emitted context record covers the dialog and both focusable controls under Per-Monitor V2.
4. A physical keyboard confirmation succeeds only after a fresh complete key press and release.
5. Cancellation produces no confirmation.
6. Authority revocation closes the window without confirmation.

While open, the production confirmation loop rechecks the dialog and both button HWNDs every 250
milliseconds. Any newly associated IME context fails closed. The normal focus-time
`WM_IME_SETCONTEXT` notification is suppressed only while a balanced `ImmGetContext` and
`ImmReleaseContext` check proves the actual dialog remains disassociated. This behavior follows
Microsoft's [`WM_IME_SETCONTEXT`](https://learn.microsoft.com/en-us/windows/win32/intl/wm-ime-setcontext)
and [`ImmGetContext`](https://learn.microsoft.com/en-us/windows/win32/api/imm/nf-imm-immgetcontext)
contracts. The same balanced absence check gates both security transitions: after focus and before
the display is armed, and synchronously immediately before an exact Confirm command is accepted.
Either failure wipes and closes the ceremony without confirmation.

The keyboard test preserves a physical key-down across the standard dialog's early,
non-authoritative command and drives the exact Confirm action only after the matching hardware
key-up. This permits Microsoft IME layouts to retain keyboard operation without accepting a
key-down-only, repeated, injected, or mixed-device ceremony.

## Trust-boundary note

The injected-input scenario exercises ordinary non-UIAccess `SendInput`, which Windows labels
`IMO_INJECTED`. Windows labels input injected by a trusted `uiAccess="true"` process as
`IMO_HARDWARE`; trusted UIAccess processes are explicitly part of the documented Windows
trusted-computing boundary and this harness does not claim to distinguish them from physical input.

## Acceptance evidence

For every run, preserve:

- exact commit and tree;
- clean-worktree proof;
- scenario and evidence label;
- Windows edition/build, display resolution, scaling, input profile, and keyboard layout;
- process, thread, owner, confirmation, Confirm-button, and Cancel-button DPI contexts;
- owner-window and actual confirmation-window DPI reported by `GetDpiForWindow`;
- full console output and exit code;
- operator observation that all exact values were visible before approval;
- confirmation that no wallet command, permission, activation flag, signing path, or Vision-Core
  change was introduced;
- SHA-256 hashes for transcripts and any screenshots.

Any unexpected close, confirmation before verified display, injected-input acceptance, clipping,
missing value, authority-revocation failure, crash, or ambiguous operator observation fails the
qualification. Stop and preserve evidence without enabling signing.

The harness smoke test must also prove that ordinary Windows focus establishment does not close the
dialog. The implementation suppresses `WM_IME_SETCONTEXT` only while a balanced OS context check
continues to report no associated IME context; every actual composition or input-language route
remains fail-closed.

## Current status

This document and harness define the qualification procedure only. No scenario is represented as
passed until the real Windows matrix is executed and independently reviewed at the exact candidate
commit. Wallet exposure, signing, submission, sending, and approval flags remain prohibited.
