# Vision Desktop Architecture Assessment

Date and time: 2026-07-30T11:15:52-04:00
Workstation context: ASUS workstation, Vision Desktop-only audit
Repository path: C:\Vision\Vision-Desktop
Branch: main
Commit: fbee4b5d8e1405b7bb7361d011e8eda6fbfacb83

## Commands Used

- Get-Content package.json
- Get-Content src-tauri/Cargo.toml
- Get-Content src-tauri/tauri.conf.json
- Get-Content bundled/core/windows-x64/manifest.json
- Get-Content src-tauri/src/*.rs selected modules
- Get-Content src/main.tsx and src/styles.css
- Get-Content README.md and selected docs

## Confirmed Architecture

Vision Desktop is currently a Tauri 2 application with:

- Rust backend crate under src-tauri.
- React 19 frontend under src.
- Vite frontend build pipeline.
- TypeScript frontend entry point in src/main.tsx.
- Tauri command bridge in src-tauri/src/lib.rs and src-tauri/src/commands.rs.
- Bundled Core binary and manifest under bundled/core/windows-x64.

Desktop framework:

- Tauri 2.11.5 in Cargo.lock.
- @tauri-apps/api declared as ^2.0.0.
- @tauri-apps/cli declared as ^2.0.0.

Frontend framework:

- React 19.
- react-dom 19.
- Vite 6.
- TypeScript 5.7.
- lucide-react for icons.

Application entry points:

- Frontend: src/main.tsx.
- Backend library: src-tauri/src/lib.rs.
- Backend binary entry: src-tauri/src/main.rs.
- Tauri config: src-tauri/tauri.conf.json.

## Implemented Functional Areas

Confirmed implemented areas:

- Node manager dashboard shell.
- Mock dashboard mode.
- Core binary manifest loading and SHA-256 verification.
- Core process supervisor skeleton.
- Core launch is intentionally blocked because frozen RC2 binds HTTP API to 0.0.0.0.
- Core process status, stop, restart command surfaces.
- Typed API response models for /status, /mining/info, and /peers.
- Dashboard API polling helper using loopback URL http://127.0.0.1:<port>.
- Node configuration model and validation.
- Basic network diagnostics for DNS and seed TCP reachability.
- Support package generation with redaction and ZIP creation.
- Directory path helpers using APPDATA and LOCALAPPDATA.

Not implemented as separate feature modules:

- Wallet interface.
- Blockchain explorer.
- Peer management page beyond dashboard counts.
- Mining control page beyond dashboard state and wizard toggle.
- Exchange UI.
- Land ownership interface.
- Game integration.
- Installer/updater logic beyond Tauri config and docs.
- Persistent frontend route/navigation architecture.

## Separation Of Concerns

Confirmed facts:

- Rust backend owns process lifecycle, API client, config, paths, reports, network diagnostics, and Core manifest verification.
- Frontend owns the dashboard UI, local wizard state, and calls Tauri commands via invoke.
- Desktop does not contain Vision-Core source code.
- Desktop does not implement block validation, PoW, fork choice, state execution, snapshots, replay, or transaction logic.
- Desktop does not read or modify Core database files in the inspected code.

Architectural concerns:

- Frontend is currently a single large src/main.tsx file with local types, state, components, wizard, dashboard, and command calls. This is acceptable for a first milestone but will not scale.
- Frontend types use any for Core status, mining, and peers in several places even though Rust defines typed models.
- No formal state-management layer exists yet.
- Navigation buttons for Networking, Logs, and Settings are visual only; they do not switch views.
- Core artifact path resolution uses CARGO_MANIFEST_DIR and repository-relative paths. This should be reviewed for packaged runtime behavior.

## Error Handling

Confirmed facts:

- Backend commands return Result<T, String>.
- Frontend displays command errors in a status line.
- API client records api_error in DashboardSnapshot.
- Supervisor rejects duplicate owned Core launch.
- Core binary hash mismatch blocks launch.
- Real Core launch is blocked for security until Core can bind API privately.

Concerns:

- Errors are plain strings rather than structured error types.
- User-facing recovery actions are limited.
- API partial failure handling is coarse; peers/mining failures are silently defaulted after /status succeeds.

## Logging And Reports

Confirmed facts:

- Supervisor is designed to redirect Core stdout/stderr to Desktop-managed logs if launch becomes enabled.
- Support package generation writes package-version.json, binary-hash.txt, status samples, peer summary, redacted config, stdout/stderr logs, summary.json, SUMMARY.md, file manifest, and a ZIP.
- Reports are stored under LOCALAPPDATA\Vision\Desktop\reports.

Concerns:

- Support package content is minimal and always assessment INCOMPLETE.
- Redaction is line-based and string-matching; future wallet/secret material will need stricter structured redaction.

## Configuration Management

Confirmed facts:

- NodeConfig is JSON persisted under APPDATA\Vision\Desktop\nodes\default.json.
- Default Core data/log paths are under LOCALAPPDATA\Vision\Core\nodes\default.
- API port 0 means allocate a loopback port.
- Stable non-zero P2P port is required.
- Internet mode requires an advertised host.
- Mining reward address must be 64 lowercase hex characters when mining is enabled.

Concerns:

- Config validation does not fully validate all host/address edge cases.
- The wizard does not expose data/log directory fields even though the config model contains them.
- The frontend default config starts with empty data_dir/log_dir values, so saving wizard config may require follow-up validation behavior review.

## Async, Threading, And Process Management

Confirmed facts:

- Tauri commands are synchronous Rust functions.
- API polling uses reqwest blocking client with 3 second timeout.
- Supervisor state is protected by Mutex<Option<OwnedCoreProcess>>.
- Stop uses child.kill rather than graceful Core shutdown.
- Port closure is checked by binding 127.0.0.1:<port>.

Concerns:

- Blocking network calls can occupy Tauri command threads.
- No background supervisor/event system exists yet.
- No automatic restart loop exists, which is documented.
- Graceful shutdown is not implemented; process kill is used.

## Security Assessment

Confirmed facts:

- Tauri command list is explicit.
- Frontend cannot call arbitrary shell commands through the inspected command set.
- API CSP allows ipc and http://127.0.0.1:* only.
- Core API private bind is required by docs and supervisor blocks RC2 launch because RC2 cannot satisfy it.
- No wallet key or seed phrase storage code exists.
- No private keys were found in source code.

Concerns:

- opener::open is exposed for log/data directories. It uses backend-determined paths, not user-supplied arbitrary paths in the current command surface.
- Future report generation and wallet features need structured redaction, key isolation, and threat modeling before production use.

## Packaging Readiness

Confirmed facts:

- Tauri bundle is enabled with targets all.
- Bundled Core binary and manifest are listed as bundle resources.
- No CI workflow exists.
- Development build record exists.

Concerns:

- Global Tauri CLI is not on PATH; project-local CLI is present through package dependencies.
- MSVC cl.exe is not on PATH in the inspected shell.
- Installer/updater behavior is not implemented beyond Tauri baseline config.
- Core launch is intentionally blocked, so the app is not a functional real-node manager until Core API bind integration lands.

## Coupling To Vision-Core Internals

Confirmed facts:

- Desktop is coupled to documented HTTP API shapes for /status, /mining/info, and /peers.
- Desktop is coupled to the RC2 binary hash and manifest.
- No consensus logic duplication was found.

Recommendation:

- Keep Desktop coupled only to stable Core API contracts and compatibility manifests.
- Add contract tests or fixtures for Core API responses before expanding wallet, transaction, explorer, or mining controls.
