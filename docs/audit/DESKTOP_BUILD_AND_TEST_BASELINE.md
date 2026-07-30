# Vision Desktop Build And Test Baseline

Date and time: 2026-07-30T11:15:52-04:00
Workstation context: ASUS workstation, Vision Desktop-only audit
Repository path: C:\Vision\Vision-Desktop
Branch: main
Commit: fbee4b5d8e1405b7bb7361d011e8eda6fbfacb83

## Commands Used

- rustc --version
- cargo --version
- rustup --version
- node --version
- npm --version
- git --version
- tauri --version check through Get-Command
- cl.exe discovery through Get-Command
- npm run typecheck
- cargo test --manifest-path C:\Vision\Vision-Desktop\src-tauri\Cargo.toml
- cargo fmt --manifest-path C:\Vision\Vision-Desktop\src-tauri\Cargo.toml -- --check
- cargo clippy --manifest-path C:\Vision\Vision-Desktop\src-tauri\Cargo.toml --all-targets -- -D warnings
- git status --short before and after executed checks

## Toolchain Inventory

Confirmed versions on this workstation:

- rustc: rustc 1.96.1 (31fca3adb 2026-06-26)
- cargo: cargo 1.96.1 (356927216 2026-06-26)
- rustup: rustup 1.29.0 (28d1352db 2026-03-05)
- Node.js: v24.18.0
- npm: 11.16.0
- Git: git version 2.55.0.windows.2
- Global Tauri CLI: not found on PATH
- MSVC cl.exe: not found on PATH in this shell

## Dependency Manifests

JavaScript/TypeScript dependencies from package.json:

- @tauri-apps/api ^2.0.0
- @vitejs/plugin-react ^4.3.4
- vite ^6.0.0
- typescript ^5.7.2
- react ^19.0.0
- react-dom ^19.0.0
- lucide-react ^0.468.0
- @tauri-apps/cli ^2.0.0 as dev dependency
- @types/react ^19.0.0
- @types/react-dom ^19.0.0

Rust dependencies from src-tauri/Cargo.toml:

- tauri 2
- serde 1 with derive
- serde_json 1
- sha2 0.10
- hex 0.4
- once_cell 1
- reqwest 0.12 with rustls-tls, json, blocking
- sysinfo 0.33
- time 0.3 with formatting
- zip 2 with deflate
- walkdir 2
- opener 0.7
- tempfile 3 as dev dependency

Lockfile observations:

- Tauri resolved to 2.11.5.
- reqwest appears as both 0.12.28 and 0.13.4 in Cargo.lock due dependency graph.
- sysinfo resolved to 0.33.1.
- zip resolved to 2.4.2.
- opener resolved to 0.7.2.
- No git-based or local path dependencies were identified in the inspected lockfile patterns.
- Desktop is tied to Vision-Core by bundled binary plus manifest, not by source dependency.

## Documented Commands

From README.md and docs/DEVELOPMENT_SETUP.md:

- Install dependencies: npm install
- Development startup: npm run tauri:dev
- Frontend typecheck: npm run typecheck
- Frontend build: npm run build
- Rust backend tests: cargo test --manifest-path src-tauri/Cargo.toml

From package.json:

- dev: vite
- build: tsc && vite build
- preview: vite preview
- typecheck: tsc --noEmit
- tauri: tauri
- tauri:dev: tauri dev
- tauri:build: tauri build

No npm lint, npm test, or npm format scripts are defined.

## Commands Executed

### npm run typecheck

- Executed: yes
- Exit code: 0
- Duration: 1.196 seconds
- Result: PASS
- Output summary: tsc --noEmit completed successfully.
- Files changed: none reported by git status.

### cargo test --manifest-path src-tauri/Cargo.toml

- Executed: yes
- Exit code: 0
- Duration: 23.923 seconds
- Result: PASS
- Output summary: 9 unit tests passed; main binary tests 0; doc tests 0.
- Files changed: none reported by git status.

Passed tests:

```text
config::tests::config_rejects_public_loopback_advertised_host
config::tests::config_requires_valid_miner_address_when_mining
api::tests::parses_status_json
paths::tests::paths_are_user_scoped_not_repo_relative
supervisor::tests::stopped_state_has_no_pid
reports::tests::redaction_removes_secret_like_fields
core_manifest::tests::manifest_parses
core_manifest::tests::hash_verification_detects_mismatch
supervisor::tests::tail_file_limits_output
```

### cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

- Executed: yes
- Exit code: 0
- Duration: 0.263 seconds
- Result: PASS
- Files changed: none reported by git status.

### cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

- Executed: yes
- Exit code: 101
- Duration: 30.958 seconds
- Result: FAIL
- Files changed: none reported by git status.

Important errors:

- src/config.rs: field_reassign_with_default in config_rejects_public_loopback_advertised_host test.
- src/config.rs: field_reassign_with_default in config_requires_valid_miner_address_when_mining test.
- src/paths.rs: ptr_arg warning promoted to error for ensure_parent(path: &PathBuf).
- src/reports.rs: unnecessary_to_owned for entry.path().to_path_buf().

These are lint/style failures, not functional test failures.

## Commands Not Executed

- npm install: skipped because dependency installation is outside audit scope.
- npm run build: skipped because it runs vite build and writes dist generated assets.
- npm run dev: skipped because it starts a dev server.
- npm run preview: skipped because it starts a preview server.
- npm run tauri:dev: skipped because it starts the desktop app and dev server.
- npm run tauri:build: skipped because it creates desktop build artifacts/installers.
- npm test: skipped because no npm test script exists.
- npm run lint: skipped because no npm lint script exists.
- Automatic format/fix commands: skipped by instruction.

## Baseline Conclusion

Confirmed facts:

- TypeScript typecheck passes.
- Rust unit tests pass.
- Rust formatting check passes.
- Strict Clippy fails on four style/lint issues.
- Real Core integration is intentionally blocked by the RC2 API bind blocker, so no end-to-end real Core launch test is currently possible from Desktop main.

Recommendation:

- First implementation task should be a small lint-cleanup and test-hardening commit only after this audit is reviewed, unless Core private-bind integration is prioritized first.
