# Wallet Transaction Confirmation Qualification Evidence — `a54bdf0` — 2026-08-07

## Qualification target

- Commit: `a54bdf06be75b762755b96c87294c82af1dd6920`
- Tree: `42b5332fb163bdd187c44b658f3626a32c31b843`
- Parent: `24e9c7c90b6aefd98bcedee5da9cdd78b600aa9d`
- Branch: `fix/wallet-final-security-gates`
- Detached exact-target worktree before every primary run: clean
- Qualification date: `2026-08-07`

The independent reviewer explicitly authorized a complete restart of the six physical scenarios
against this exact commit and tree. Earlier exploratory, diagnostic, and failed runs were not reused.

## Workstation context

- Supported operating system: Windows 11 Home, version 25H2
- Stale Windows registry `ProductName` value reported by the host: Windows 10 Home
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
Each primary transcript includes the Windows operator identity, exact start and end timestamps,
exact commit and tree, clean-worktree proof, complete console output, exit code, operator observation,
and a statement that no screenshots were captured.

## Results

### Microsoft Japanese

#### Keyboard confirmation — passed

- Evidence label: `primary-japanese-keyboard-a54bdf0-20260807-r1`
- Input profile: `microsoft-japanese`
- Active keyboard layout: `00000411`
- Scenario: `keyboard`
- Test duration: 3.51 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `1` (hardware keyboard)
- Result: passed

#### Cancellation — passed

- Evidence label: `primary-japanese-cancel-a54bdf0-20260807-r1`
- Input profile: `microsoft-japanese`
- Active keyboard layout: `00000411`
- Scenario: `cancel`
- Test duration: 2.86 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

#### Authority revocation — passed

- Evidence label: `primary-japanese-revoke-a54bdf0-20260807-r1`
- Input profile: `microsoft-japanese`
- Active keyboard layout: `00000411`
- Scenario: `revoke`
- Test duration: 2.07 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

### Microsoft Pinyin

#### Keyboard confirmation — passed

- Evidence label: `primary-pinyin-keyboard-a54bdf0-20260807-r2`
- Input profile: `microsoft-pinyin`
- Active keyboard layout: `00000804`
- Scenario: `keyboard`
- Test duration: 2.01 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `1` (hardware keyboard)
- Result: passed

#### Cancellation — passed

- Evidence label: `primary-pinyin-cancel-a54bdf0-20260807-r1`
- Input profile: `microsoft-pinyin`
- Active keyboard layout: `00000804`
- Scenario: `cancel`
- Test duration: 5.39 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

#### Authority revocation — passed

- Evidence label: `primary-pinyin-revoke-a54bdf0-20260807-r1`
- Input profile: `microsoft-pinyin`
- Active keyboard layout: `00000804`
- Scenario: `revoke`
- Test duration: 2.05 seconds
- Confirmation DPI: 120
- Confirm focus verified: `true`
- Accepted input device: `0` (no confirmation input accepted)
- Result: passed

## Primary evidence manifest

No screenshots were captured. The following six transcript files are the complete primary records:

| Scenario | Transcript | SHA-256 |
| --- | --- | --- |
| Japanese keyboard | `wallet-qualification/a54bdf0/20260807-japanese-keyboard-r1.txt` | `79E3E1E557BD609C8C4679AA8DFC2E924AD01F18F8A0EE32CD6F8586F72945E4` |
| Japanese cancellation | `wallet-qualification/a54bdf0/20260807-japanese-cancel-r1.txt` | `D57FC7CD7B8239A555F2EE77A77FAE7B76C7D4C9A5145EE1C28808F2B61A1D7A` |
| Japanese revocation | `wallet-qualification/a54bdf0/20260807-japanese-revoke-r1.txt` | `DD6A8EA3B10418342BCD73F006D60B977B3C13E8BC2596E361C936EA78E5F122` |
| Pinyin keyboard | `wallet-qualification/a54bdf0/20260807-pinyin-keyboard-r2.txt` | `CACCF573999BD4FB2A67B274ABE943DC0FB9F3909DDC61B649D78AC45F715360` |
| Pinyin cancellation | `wallet-qualification/a54bdf0/20260807-pinyin-cancel-r1.txt` | `C26D5FBB764EA6C2F85A7157BB57F163A1AA0F7E175916546C776E00F0535FE5` |
| Pinyin revocation | `wallet-qualification/a54bdf0/20260807-pinyin-revoke-r1.txt` | `D838B11133A98ABC2FEF0257BD0FD2576BAEF363089772BEDBBB7D904EEFC341` |

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
