# Vision Desktop Repository Inventory

Date and time: 2026-07-30T11:15:52-04:00
Workstation context: ASUS workstation, Vision Desktop-only audit
Repository path: C:\Vision\Vision-Desktop
Branch: main
Commit: fbee4b5d8e1405b7bb7361d011e8eda6fbfacb83

## Commands Used

- git -C C:\Vision\Vision-Desktop status --short --branch
- git -C C:\Vision\Vision-Desktop rev-parse HEAD
- git -C C:\Vision\Vision-Desktop remote -v
- git -C C:\Vision\Vision-Desktop branch --all --verbose
- git -C C:\Vision\Vision-Desktop log --oneline --decorate -20
- git -C C:\Vision\Vision-Desktop tag --list
- git -C C:\Vision\Vision-Desktop diff --name-only
- git -C C:\Vision\Vision-Desktop diff --cached --name-only
- git -C C:\Vision\Vision-Desktop ls-files --others --exclude-standard
- git -C C:\Vision\Vision-Desktop rev-list --left-right --count "@{u}...HEAD"
- git -C C:\Vision\Vision-Desktop ls-files
- Get-ChildItem and Get-Content on repository files
- Get-FileHash on bundled Core files

## Git State

Confirmed findings:

- Absolute repository path: C:\Vision\Vision-Desktop
- Current branch: main
- Current commit: fbee4b5d8e1405b7bb7361d011e8eda6fbfacb83
- Upstream: origin/main
- Ahead/behind upstream: 0 ahead, 0 behind
- Working tree before audit documents: clean
- Modified files before audit documents: none
- Staged files before audit documents: none
- Untracked files before audit documents: none
- Existing tags: none reported by git tag --list

Configured remotes:

```text
origin  https://github.com/oneepicnight/Vision-Desktop.git (fetch)
origin  https://github.com/oneepicnight/Vision-Desktop.git (push)
```

Local branches:

```text
* main fbee4b5 Add first node-manager dashboard
```

Remote-tracking branches:

```text
origin/main fbee4b5 Add first node-manager dashboard
```

Recent commits:

```text
fbee4b5 Add first node-manager dashboard
4cc9d05 Add Core supervisor and diagnostics backend
0545f33 Add frozen Core artifact manifest
3ae5cee Initialize Vision Desktop Tauri application
972e38e Initialize Vision Desktop repository policy
```

## Tracked Project Files

```text
.gitignore
NOTICE.md
README.md
bundled/core/windows-x64/manifest.json
docs/CONFIGURATION_MODEL.md
docs/CORE_COMPATIBILITY_POLICY.md
docs/DESKTOP_TO_CORE_API.md
docs/DEVELOPMENT_BUILD_RECORD.md
docs/DEVELOPMENT_SETUP.md
docs/KNOWN_LIMITATIONS.md
docs/PROCESS_LIFECYCLE.md
docs/RC2_API_BIND_BLOCKER.md
docs/REPORT_GENERATION.md
docs/REPOSITORY_BOUNDARIES.md
docs/SECURITY_BASELINE.md
docs/VERSIONING_POLICY.md
index.html
package-lock.json
package.json
release-notes/0.1.0-alpha.1-dev.md
src-tauri/Cargo.lock
src-tauri/Cargo.toml
src-tauri/build.rs
src-tauri/icons/icon.ico
src-tauri/src/api.rs
src-tauri/src/commands.rs
src-tauri/src/config.rs
src-tauri/src/core_manifest.rs
src-tauri/src/lib.rs
src-tauri/src/main.rs
src-tauri/src/network.rs
src-tauri/src/paths.rs
src-tauri/src/reports.rs
src-tauri/src/supervisor.rs
src-tauri/tauri.conf.json
src/main.tsx
src/styles.css
tsconfig.json
vite.config.ts
```

## Bundled Core Artifact

Confirmed findings:

- Manifest path: bundled/core/windows-x64/manifest.json
- Binary path: bundled/core/windows-x64/vision-core.exe
- Binary SHA-256: 41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01
- Manifest SHA-256: 5688813388F426EAB344557A934197FFF7241DACCCD42260C9F7479182EDFD16
- Manifest identifies Core alpha tag vision-core-alpha-rc2, consensus tag vision-core-consensus-v1.0.3, source commit 6a065df8206b50874029a27ee2b54dffae5e3cdd, consensus version 3, P2P protocol version 4.

## Repository Structure Summary

Confirmed findings:

- Frontend source: src/main.tsx, src/styles.css
- Tauri/Rust backend source: src-tauri/src/*.rs
- Public assets directory exists but no tracked files were identified under public.
- scripts directory exists but is empty.
- tests directory exists but is empty.
- src-tauri/tests exists but no tracked test files were identified.
- Documentation exists under docs.
- Release notes exist under release-notes.
- CI workflows: .github directory not present.

## Notes

This document records repository state before audit documents were added. The audit itself intentionally creates files under docs/audit only.
