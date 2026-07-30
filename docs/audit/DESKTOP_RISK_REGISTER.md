# Vision Desktop Risk Register

Date and time: 2026-07-30T11:15:52-04:00
Workstation context: ASUS workstation, Vision Desktop-only audit
Repository path: C:\Vision\Vision-Desktop
Branch: main
Commit: fbee4b5d8e1405b7bb7361d011e8eda6fbfacb83

## Commands Used

- Code inspection of src/main.tsx and src-tauri/src/*.rs
- Manifest inspection of package.json, Cargo.toml, tauri.conf.json, and lockfiles
- npm run typecheck
- cargo test --manifest-path src-tauri/Cargo.toml
- cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
- cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

## Critical Risks

None confirmed in the current codebase.

Rationale: no wallet key storage, seed phrase handling, transaction signing, exchange, land ownership, game-launcher, direct database access, or consensus logic was found in the inspected code.

## High Risks

### HIGH-1: Real Core launch is blocked by RC2 API bind behavior

Confirmed facts:

- docs/RC2_API_BIND_BLOCKER.md states frozen RC2 binds HTTP API to 0.0.0.0:<VISION_HTTP_PORT>.
- src-tauri/src/supervisor.rs intentionally returns an error before spawning Core.
- Security baseline requires loopback-only administrative API.

Impact:

- Desktop cannot currently operate a real node from this repository state.
- Attempting to bypass the blocker would risk exposing the administrative API.

Recommendation:

- Integrate only with a Core runtime that supports explicit loopback HTTP bind, then update Desktop manifest separately.

### HIGH-2: No wallet/security boundary exists yet for future financial actions

Confirmed facts:

- No wallet module exists.
- No key storage code exists.
- No transaction submission UI exists.

Impact:

- This is safe for current code because financial actions are absent, but it is a high design risk for upcoming wallet/exchange work.

Recommendation:

- Design wallet storage, encryption, redaction, signing, and recovery flows before any wallet implementation.

## Medium Risks

### MEDIUM-1: Frontend architecture is a single large entry file

Confirmed facts:

- src/main.tsx contains types, command calls, layout components, dashboard, wizard, state, and rendering.

Impact:

- Maintainability will degrade quickly as explorer, wallet, peer management, exchange, and game features are added.

Recommendation:

- Split into feature modules, typed API client hooks, shared UI primitives, and app state boundaries.

### MEDIUM-2: Frontend uses any for Core response surfaces

Confirmed facts:

- DashboardSnapshot in src/main.tsx uses status: any, mining: any, peers: any[].
- Rust backend has typed models.

Impact:

- UI can silently drift from backend/Core response contracts.

Recommendation:

- Add generated or manually mirrored TypeScript types and runtime guards for Tauri responses.

### MEDIUM-3: Support package redaction is line-based

Confirmed facts:

- reports.rs redacts lines containing private, seed_phrase, secret, or password, and masks p2p_advertised_host.

Impact:

- Future wallet or account data could leak if field names differ.

Recommendation:

- Move to structured redaction before wallet support or external alpha support-package collection.

### MEDIUM-4: Clippy fails with warnings as errors

Confirmed facts:

- cargo clippy --all-targets -- -D warnings fails on four lints.

Impact:

- CI readiness is blocked if strict linting is required.

Recommendation:

- Fix lint issues in a small non-functional cleanup commit.

### MEDIUM-5: No CI workflows

Confirmed facts:

- .github directory is not present.

Impact:

- Typecheck, tests, linting, and packaging checks are not enforced remotely.

Recommendation:

- Add CI once the local baseline is clean.

## Low Risks

### LOW-1: Global Tauri CLI and cl.exe are not on PATH

Confirmed facts:

- Global tauri command not found.
- cl.exe not found on PATH in inspected shell.
- Project has @tauri-apps/cli as dev dependency.

Impact:

- Local development can still use npm scripts, but packaging may require Visual Studio build tools environment validation.

### LOW-2: Mock mode defaults to enabled in UI state

Confirmed facts:

- const [mockMode, setMockMode] = React.useState(true).

Impact:

- Good for development but must be explicit and disabled or gated in production builds.

### LOW-3: Stop uses process kill instead of graceful shutdown

Confirmed facts:

- SupervisorState::stop calls child.kill() if process is still running.

Impact:

- Acceptable while launch is blocked, but real node lifecycle should prefer graceful shutdown if Core supports it.

## Informational Findings

- No private keys, seed phrases, hardcoded credentials, or direct wallet material were found.
- No direct Core database mutation was found.
- No consensus logic duplication was found.
- No blockchain explorer, exchange, land, or game code exists yet.
- Desktop is currently Windows-first and unsigned, matching docs.
