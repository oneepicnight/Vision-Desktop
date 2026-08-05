# Wallet Windows Lifecycle Qualification

## Purpose

This matrix qualifies the private lifecycle boundary before any wallet command or WebView
permission is registered. It exercises real Windows session and power notifications and the real
cross-session kernel wallet lease. The probes are ignored Rust tests compiled only by the test
harness; they are absent from the production application and cannot be invoked by React.

Passing these probes is necessary but not sufficient for custody activation. An independent reviewer
must approve the exact evidence and then approve a narrowly permissioned lifecycle boundary for a
packaged clean-account drill. Signing and sending remain a separate later decision.

## Safety boundary

- Run only on a clean committed wallet candidate with Vision Desktop closed.
- Do not use real funds or an existing wallet.
- Store evidence outside the repository.
- Do not put passwords, recovery credentials, vaults, or recovery artifacts in evidence logs.
- Do not set either production approval constant during qualification.
- Stop on any unexpected process, listener, path, permission, or test result.

The qualification script refuses a dirty worktree and records the exact commit, tree, UTC time,
machine, Windows user, action, exit code, and log SHA-256. Its output contains no wallet secret.

## Automated baseline

Before the manual matrix, run the exact candidate in release mode:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --release -- --test-threads=1
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npm run typecheck
npm run test:state
npm run build
git diff --check
git status --short
```

Required result: every command passes and the worktree remains clean. The ignored real-Windows tests
do not run as part of the ordinary suite.

## Real Windows event probes

Run each action separately from an interactive Windows console. Replace the evidence path with a
trusted operator-controlled directory outside the repository.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/run-wallet-windows-qualification.ps1 -Action SessionLock -EvidenceDirectory C:\Vision\wallet-qualification-evidence
powershell -ExecutionPolicy Bypass -File scripts/run-wallet-windows-qualification.ps1 -Action Suspend -EvidenceDirectory C:\Vision\wallet-qualification-evidence
powershell -ExecutionPolicy Bypass -File scripts/run-wallet-windows-qualification.ps1 -Action Hibernate -EvidenceDirectory C:\Vision\wallet-qualification-evidence
```

For each action:

1. Wait for `VISION_WALLET_QUALIFICATION_READY`.
2. Perform exactly the requested real Windows action.
3. Sign back in or resume normally.
4. Require `VISION_WALLET_QUALIFICATION_PASS` and exit code `0`.

Unlock or resume must never restore old authority. The probe proves that the actual hidden listener
revoked an existing operation and that the runtime can issue only fresh authority afterward.

Record `powercfg /a` with suspend evidence. On an S0 Low Power Idle system, selecting Sleep must
produce a real Modern Standby entry in the Windows System event log; merely turning off the display
does not qualify. The production hidden window explicitly opts into Desktop Activity Moderator
suspend/resume notifications, and failure to receive the notification is a failed gate even when
Windows later resumes normally.

## Cross-session ownership

Use the same Windows account in two genuine Windows sessions, such as console plus RDP or Fast User
Switching where supported.

In session one:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/run-wallet-windows-qualification.ps1 -Action CrossSessionOwner -EvidenceDirectory C:\Vision\wallet-qualification-evidence
```

After `VISION_WALLET_QUALIFICATION_OWNER_READY`, run in session two:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/run-wallet-windows-qualification.ps1 -Action CrossSessionContender -EvidenceDirectory C:\Vision\wallet-qualification-evidence
```

The contender must report `VISION_WALLET_QUALIFICATION_CONTENDER_DENIED`. Terminate the owner
normally, repeat with forced process termination, and then run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/run-wallet-windows-qualification.ps1 -Action CrossSessionRecovery -EvidenceDirectory C:\Vision\wallet-qualification-evidence
```

Recovery must report `VISION_WALLET_QUALIFICATION_OWNERSHIP_RECOVERED`. Repeat for console/RDP,
RDP reconnect, and Fast User Switching configurations supported for the release.

## Packaged lifecycle gate

The following tests cannot be completed until an independent reviewer permits a qualification-only
main-window lifecycle boundary. They must run on an installed release build under a disposable clean
Windows account before production lifecycle approval becomes `true`:

- create with native recovery acknowledgement, followed by locked completion;
- close, reload, workstation lock, suspend, hibernate, logoff, and process-kill at every ceremony and
  recovery/vault publication phase;
- no success response or restored session after revocation;
- no plaintext or secret-bearing crash, dump, pagefile sample, log, support package, or UI remnant;
- recovery artifact transfer to a second clean Windows device/account;
- exact public address equality after restore;
- wrong-password, damaged-vault, reparse, destination-race, and existing-file refusal;
- uninstall/reinstall with deliberate retained-data and delete-data choices;
- abnormal termination releases the global wallet lease; and
- signing and submission remain unavailable in every lifecycle-qualified build.

Power-loss tests must use a disposable VM or sacrificial test machine. Never deliberately power-cut
the development workstation or a machine holding real wallet material.

## Evidence verdict

A qualifying report must list every matrix row as passed, failed, blocked, or not applicable; include
the exact commit/tree and SHA-256 for every log and packaged binary; preserve interruptions and
warnings; and state that no wallet command, permission, signing path, or approval flag was enabled
during the private-runtime probe stage.

Only an independent reviewer may convert this evidence into lifecycle-only integration approval.
The implemented journal-head correction still requires independent re-review, and private-loopback
Core compatibility remains mandatory before signing or sending.
