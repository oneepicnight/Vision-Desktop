# Security Baseline

Security rules implemented from the first milestone:

- Core API must bind to loopback only. Frozen RC2 cannot currently satisfy this, so real Core launch is blocked by the Desktop supervisor.
- Desktop frontend cannot execute arbitrary shell commands.
- Tauri command list is explicit and narrow.
- Backend commands validate inputs.
- Core binary hash is verified before launch.
- Desktop records the exact child PID it owns.
- Desktop never kills a process solely because it occupies a port.
- Desktop does not read or modify Core database files.
- Logs and reports redact public endpoints and secrets by default.
- Desktop runs as a normal user.
- No automatic firewall or router changes are performed.
- Rust dependencies are audited against RustSec on relevant changes, weekly, and on manual request.
- Frontend production and development dependencies are audited from `package-lock.json` on relevant changes, weekly, and on manual request. Moderate, high, and critical npm advisories fail the job; low-severity findings remain visible.
- The production WebView's script connection policy has no general network source. Its `connect-src` is limited to Tauri IPC, while local Vite and loopback development sources exist only in `devCsp`.
- Frontend TypeScript cannot call Core directly; automated tests keep Tauri core access centralized in `src/services/coreApi.ts`.
- Windows rejects tested duplicate Vision Desktop launches per application identity. Duplicate-launch arguments and working directories are discarded, while the existing main window is restored and focused on a best-effort basis. Source review identified a narrow mutex/receiver startup interval in the official plugin, so the independent runtime lock below closes the custody exclusion boundary.
- The private Rust wallet runtime now holds an independent Windows kernel mutex, one main-window-owned operation slot, a locked-by-default session, and short-lived recovery authorization storage. A Rust-only native listener synchronously revokes this authority on Windows session lock, suspend/standby, logoff/shutdown, and teardown; unlock and resume do not restore it. Listener setup, mutex poison, and lifecycle invalidation fail closed. No wallet command or WebView permission exposes this state.
- The pinned native dialog plugin is initialized only after single-instance enforcement. Private Rust adapters accept only validated local, non-reparse Windows recovery paths and retain them inside the wallet runtime behind generation-bound, two-minute, single-use tokens. React has no dialog package, permission, wallet command, raw path, or token access.
- Private Rust lifecycle adapters now connect status, create, restore, unlock, and lock to the existing custody primitives without registering a Tauri command. They use one fixed device-bound local vault path, require token-authorized recovery files, verify recovery before local vault storage, never overwrite wallet files, complete create and restore locked, retain unlocked seeds only inside the zeroizing session, and return fixed non-disclosing failures. React still has no custody access.
- The custody vault root is resolved through Windows `FOLDERID_LocalAppData`, not the process environment. Startup accepts only a normal absolute path on a fixed local volume and rejects UNC, device/verbatim, removable/remote, malformed, inaccessible, and existing reparse-point directory chains.
- Windows vault and recovery I/O holds every ancestor directory against rename, disables final-file reparse traversal, validates the opened file handle, and performs I/O through that same handle. Vault DACL application, non-replacing publication, and exact encrypted-byte read-back remain bound to the staging handle. Failure cleanup never deletes an untrusted path, and existing destinations remain non-overwritable.

The frontend audit uses the committed npm lockfile without installing packages or running dependency lifecycle scripts. Its checkout and Node setup actions are pinned to reviewed full commit SHAs, the job has read-only repository permission, and package-manager caching is disabled. The admission-time npm audit on 2026-08-01 reported zero vulnerabilities across the locked frontend graph.

Future changes must preserve these rules unless a formal security review replaces them with a stricter design.

