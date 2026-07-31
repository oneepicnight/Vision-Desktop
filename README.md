# Vision Desktop

Vision Desktop is the user-facing node manager for Vision Core. It launches and controls a bundled Vision Core executable internally so users do not need Rust, Cargo, Node.js, localhost URLs, TOML editing, manual hash checks, or manual process management.

This repository is intentionally separate from Vision Core. It does not contain Vision Core source code and must not implement or duplicate consensus logic.

Initial version: `0.1.0-alpha.1-dev`

Current desktop scope includes:

- node-manager dashboard
- desktop-managed process controls
- read-only blockchain explorer for address and transaction inspection
- read-only peer manager for current connection and recovery visibility
- support package generation
- mock-mode development workflows

Bundled Core baseline for local development:

- Core alpha tag: `vision-core-alpha-rc2`
- Consensus tag: `vision-core-consensus-v1.0.3`
- Source commit: `6a065df8206b50874029a27ee2b54dffae5e3cdd`
- Consensus version: `3`
- P2P protocol version: `4`
- Windows x64 binary SHA-256: `41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01`

## Development

Prerequisites for development builds:

- Rust stable toolchain
- Node.js and npm
- Tauri platform prerequisites for Windows WebView2

Install dependencies:

```powershell
npm install
```

Run desktop dev mode:

```powershell
npm run tauri:dev
```

Run frontend-only checks:

```powershell
npm run typecheck
npm run build
```

Run Rust backend tests:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

## Repository Boundary

Vision Desktop owns UI, process lifecycle, local configuration, Core binary verification, installer/updater, reports, and diagnostics. Vision Core owns consensus, block validation, P2P protocol, mining, persistence, replay, and state execution.
