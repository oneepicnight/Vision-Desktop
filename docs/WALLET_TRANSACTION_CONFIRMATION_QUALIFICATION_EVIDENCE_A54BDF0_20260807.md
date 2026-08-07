# Wallet Transaction Confirmation Qualification Evidence — `a54bdf0` — 2026-08-07

## Qualification target

- Commit: `a54bdf06be75b762755b96c87294c82af1dd6920`
- Tree: `42b5332fb163bdd187c44b658f3626a32c31b843`
- Parent: `24e9c7c90b6aefd98bcedee5da9cdd78b600aa9d`
- Branch: `fix/wallet-final-security-gates`
- Tracked worktree before the matrix: clean
- Qualification date: `2026-08-07`

The independent reviewer explicitly authorized a complete restart of the six physical scenarios
against this exact commit and tree. Earlier exploratory, diagnostic, and failed runs were not reused.

## Workstation context

- Windows registry product label: Windows 10 Home, display version 25H2
- Windows build: `26200.8655`
- Architecture: 64-bit (`AMD64`)
- Graphics adapter: NVIDIA GeForce RTX 5060 Laptop GPU
- Primary physical display: 1920 x 1200 at 165 Hz
- Confirmation DPI: 120 (125% scaling)
- Required DPI context: Per-Monitor V2
- Qualification input profiles: Microsoft Pinyin and Microsoft Japanese IME

Each run independently reported Per-Monitor V2 awareness for the process, thread, owner window,
confirmation dialog, Confirm button, and Cancel button. Each run reported owner and dialog DPI 120.

## Execution policy

The matrix required every scenario to pass. A failed scenario would have stopped execution. For
keyboard confirmation, the operator was instructed to inspect the complete transaction display,
verify that no composition or candidate UI appeared, and physically press and release Enter once
without using the mouse. For cancellation, the operator inspected the dialog and physically selected
Cancel. For revocation, the operator did not interact while test authority was revoked.

The operator completed all requested actions and reported no visual or IME anomaly during this
matrix. The harness independently enforced the expected input-device result, exact Confirm focus,
keyboard layout, DPI context, IME-context absence at authority transitions, and scenario outcome.

## Results

### Microsoft Japanese

#### Keyboard confirmation — passed

- Evidence label: `final-japanese-keyboard-20260807`
- Input profile: `microsoft-japanese`
- Active keyboard layout: `00000411`
- Scenario: `keyboard`
- Test duration: 2.48 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `1` (hardware keyboard)
- Result: passed

#### Cancellation — passed

- Evidence label: `final-japanese-cancel-20260807`
- Input profile: `microsoft-japanese`
- Active keyboard layout: `00000411`
- Scenario: `cancel`
- Test duration: 14.44 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

#### Authority revocation — passed

- Evidence label: `final-japanese-revoke-20260807`
- Input profile: `microsoft-japanese`
- Active keyboard layout: `00000411`
- Scenario: `revoke`
- Test duration: 2.08 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

### Microsoft Pinyin

#### Keyboard confirmation — passed

- Evidence label: `final-pinyin-keyboard-20260807`
- Input profile: `microsoft-pinyin`
- Active keyboard layout: `00000804`
- Scenario: `keyboard`
- Test duration: 6.97 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `1` (hardware keyboard)
- Result: passed

#### Cancellation — passed

- Evidence label: `final-pinyin-cancel-20260807`
- Input profile: `microsoft-pinyin`
- Active keyboard layout: `00000804`
- Scenario: `cancel`
- Test duration: 6.29 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

#### Authority revocation — passed

- Evidence label: `final-pinyin-revoke-20260807`
- Input profile: `microsoft-pinyin`
- Active keyboard layout: `00000804`
- Scenario: `revoke`
- Test duration: 2.04 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

## Matrix disposition

**Passed — six of six authorized physical scenarios passed on the exact reviewed commit and tree.**

The successful matrix establishes physical qualification evidence for the private, unregistered
native transaction-confirmation ceremony on this workstation and configuration. It does not itself
authorize wallet commands, permissions, activation flags, signing, submission, sending, frontend
wallet authority, recovery export, or Vision-Core changes.

The earlier failed evidence in
`WALLET_TRANSACTION_CONFIRMATION_QUALIFICATION_EVIDENCE_20260807.md` remains preserved and
non-qualifying. This report supersedes it only for the corrected target identified above.

## Next gate

Submit this exact evidence and qualification target for independent acceptance. No wallet exposure,
signing, submission, command registration, permission change, or activation change may proceed until
that review explicitly authorizes the next tranche.
