# Wallet Transaction Confirmation Qualification Evidence — 2026-08-07

## Qualification target

- Commit: `203c6a72731b572d42246e28274797ee4ea6c872`
- Tree: `9d63c2d41ec9ce0aeb1c1602d60dedf459a54a0a`
- Parent: `eaa3c7058ff5f39461652e115139fdcd09e15252`
- Branch: `fix/wallet-final-security-gates`
- Evidence start: `2026-08-07T12:59:30-04:00`
- Tracked worktree before the run: clean

## Workstation context

- Windows registry product label: Windows 10 Home, display version 25H2
- Windows build: `26200.8655`
- Architecture: 64-bit
- Primary physical display: 1920 x 1200 at 165 Hz
- Confirmation DPI: 120 (125% scaling)
- DPI context: Per-Monitor V2 for the process, thread, owner, confirmation dialog, Confirm button,
  and Cancel button
- Installed qualification input profiles: Microsoft Pinyin and Microsoft Japanese IME

## Authorized matrix

The independent reviewer authorized six physical runs against the exact target:

1. Microsoft Pinyin keyboard confirmation
2. Microsoft Pinyin cancellation
3. Microsoft Pinyin authority revocation
4. Microsoft Japanese keyboard confirmation
5. Microsoft Japanese cancellation
6. Microsoft Japanese authority revocation

The gate requires every run to pass. Execution stops on the first failure.

## Result

### Microsoft Japanese keyboard confirmation — failed

- Scenario: `keyboard`
- Evidence label: `official-japanese-keyboard-20260807`
- Input profile: `microsoft-japanese`
- Active keyboard layout: `00000411`
- Operator action: one physical Enter press and release on the displayed confirmation ceremony
- Observed result: the dialog did not confirm
- Safe exit: the operator clicked Cancel only after the failure was declared
- Harness result: `Err(Cancelled)` where `Ok(())` was required
- Test result: failed, exit code 101
- Test duration: 101.85 seconds
- Accepted confirmation device: none
- No wallet command, signature, submission, or activation authority existed

The dialog remained fail-closed: the failed keyboard action produced no confirmation, and
cancellation produced no confirmation authority.

## Matrix disposition

**Failed — physical qualification is not complete.**

The remaining five runs were not executed after the failed first gate. Earlier exploratory or
diagnostic runs occurred on different source states and are not qualification evidence for this
exact commit and tree.

## Corrective investigation boundary

Source inspection after preserving the failed result identified a focus-order race candidate:
startup calls `SetFocus(dialog)` after showing and foregrounding the window, while verified painting
separately focuses the armed Confirm control. Paint timing can therefore determine whether the final
focus remains on Confirm or is overwritten by the non-button dialog. This is an engineering
hypothesis, not a passed qualification result.

Any correction requires a new isolated commit, independent review, and a complete rerun of all six
physical scenarios on the newly approved exact commit and tree.
