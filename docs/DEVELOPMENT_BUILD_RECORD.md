# Development Build Record

## Build

- Desktop version: `0.1.0-alpha.1-dev`
- Build type: unsigned local development executable
- Build timestamp: `2026-07-25T15:30:00Z`
- Development executable: `C:\Vision\Vision-Desktop\src-tauri\target\release\vision-desktop.exe`
- Development executable size: `11921920` bytes
- Development executable SHA-256: `44A12057986370BE167D7131934322B70FFDEB8A91B87213F7732FDFCEF44A8E`

## Bundled Core Baseline

- Bundled Core alpha tag: `vision-core-alpha-rc2`
- Bundled Core consensus tag: `vision-core-consensus-v1.0.3`
- Bundled Core commit: `6a065df8206b50874029a27ee2b54dffae5e3cdd`
- Bundled Core SHA-256: `41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01`
- Consensus version: `3`
- P2P protocol version: `4`

## Validation

- Rust backend tests: `9 passed`, `0 failed`
- Frontend typecheck: passed
- Frontend production build: passed
- Cargo release executable build: passed

## Core Integration Result

Blocked before launch.

Frozen RC2 Core binds HTTP API to `0.0.0.0:<VISION_HTTP_PORT>` and does not provide a loopback-only bind setting. Vision Desktop must not launch Core in real mode until the Core runtime exposes a private API bind address option.

The Desktop supervisor enforces this by refusing real Core launch. Mock mode remains available for UI development.

## Known Limitations

- Unsigned development build only.
- Real Core launch is blocked by RC2 API bind behavior.
- Mock dashboard mode works for UI development.
- No wallet.
- No exchange.
- No game launcher.
- No updater.
- No automatic NAT traversal.
- No public release.
