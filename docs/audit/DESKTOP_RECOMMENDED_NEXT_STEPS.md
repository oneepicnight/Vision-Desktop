# Vision Desktop Recommended Next Steps

Date and time: 2026-07-30T11:15:52-04:00
Workstation context: ASUS workstation, Vision Desktop-only audit
Repository path: C:\Vision\Vision-Desktop
Branch: main
Commit: fbee4b5d8e1405b7bb7361d011e8eda6fbfacb83

## Commands Used

- Full audit commands listed in DESKTOP_REPOSITORY_INVENTORY.md and DESKTOP_BUILD_AND_TEST_BASELINE.md
- No implementation commands were run.
- No commits, pushes, merges, rebases, resets, branch deletes, tag operations, or dependency installations were performed.

## Recommended Work Units

### 1. Fix strict Rust lint baseline

Scope:

- src-tauri/src/config.rs test initialization style.
- src-tauri/src/paths.rs ensure_parent signature from &PathBuf to &Path.
- src-tauri/src/reports.rs remove unnecessary to_path_buf.

Acceptance checks:

- cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
- cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
- cargo test --manifest-path src-tauri/Cargo.toml
- npm run typecheck

Rationale:

- Small, reviewable, non-functional cleanup.
- Unblocks a clean local quality gate before larger architecture work.

### 2. Split frontend into maintainable modules

Scope:

- Extract dashboard cards, metrics, sidebar, topbar, wizard, and support actions into components.
- Add TypeScript domain types for dashboard, process state, config, diagnostics, and support package results.
- Keep behavior unchanged.

Acceptance checks:

- npm run typecheck
- cargo test --manifest-path src-tauri/Cargo.toml

Rationale:

- src/main.tsx is already too large for the planned product surface.

### 3. Add frontend test harness

Scope:

- Choose a test runner compatible with Vite/React, such as Vitest plus Testing Library, after confirming dependency policy.
- Add dashboard render tests, wizard validation tests, mock-mode banner tests, and command error display tests.

Acceptance checks:

- npm test or equivalent new script.
- npm run typecheck.

Rationale:

- Current frontend has no automated tests.

### 4. Harden Tauri command contracts

Scope:

- Replace stringly typed frontend invoke calls with typed wrappers.
- Add runtime validation or defensive parsing for backend responses.
- Separate UI DTOs from raw Core API DTOs.

Acceptance checks:

- Typecheck catches invalid command payloads.
- Backend unit tests still pass.

Rationale:

- Prevent UI state drift and silent command failures.

### 5. Improve support package schema compliance

Scope:

- Compare current reports.rs output to the documented closed-alpha report schema.
- Add structured redaction tests.
- Add report manifest verification tests.

Acceptance checks:

- Unit tests for redaction and report contents.
- No private or endpoint-sensitive data included by default.

Rationale:

- Support bundles will become important once real operators use the app.

### 6. Prepare Core private-bind integration branch after Core runtime artifact is available

Scope:

- Do not bypass the current supervisor blocker.
- Add a separate development Core manifest for a Core runtime that supports loopback HTTP bind.
- Set VISION_HTTP_BIND=127.0.0.1 and VISION_HTTP_PORT=<allocated> at launch.
- Add integration tests for launch, health/status polling, stop, restart, and port closure.

Acceptance checks:

- Core binary hash verification passes.
- API responds on loopback.
- API does not bind publicly.
- Dashboard shows real Core state.

Rationale:

- This is the first real-node milestone, but it depends on a Core runtime capability handled outside this Desktop-only audit.

### 7. Add CI after local baseline is clean

Scope:

- GitHub Actions or equivalent for npm typecheck, Rust fmt, Rust clippy, Rust tests, and frontend tests once added.
- Do not add release publishing in the first CI step.

Acceptance checks:

- Pull request checks run without requiring secrets.

Rationale:

- Prevent regressions before repository grows.

## Recommended First Implementation Task

Fix the strict Rust lint baseline.

Reason:

- It is small, reviewable, and does not depend on Vision-Core changes.
- It makes the existing backend quality gate clean before larger UI restructuring or Core integration work.
