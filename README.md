# Vision Desktop

Vision Desktop is the user-facing node manager for Vision Core. It launches and controls a bundled Vision Core executable internally so users do not need Rust, Cargo, Node.js, localhost URLs, TOML editing, manual hash checks, or manual process management.

This repository is intentionally separate from Vision Core. It does not contain Vision Core source code and must not implement or duplicate consensus logic.

Initial version: `0.1.0-alpha.1-dev`

Current desktop scope includes:

- node-manager dashboard
- desktop-managed process controls
- read-only blockchain explorer for address and transaction inspection
- read-only peer manager for current connection and recovery visibility
- read-only mining status from the existing Desktop snapshot
- read-only diagnostics view for process, API, verification, and fixed log-tail visibility
- support package generation
- mock-mode development workflows

The initial Mining page is read-only. It displays confirmed data already present in the Desktop snapshot and Desktop-managed node configuration:

- runtime mining enabled status when reported by Core
- runtime active or inactive state when reported by Core
- mining availability from the existing status snapshot
- paused reason and recovery state when reported by Core
- current height context
- Desktop-managed mining configuration and reward address
- Desktop-side last refresh time

Enabled mining does not necessarily mean that mining is actively producing blocks.

The initial Diagnostics page is read-only. It displays confirmed information already available through the existing Desktop snapshot, process supervisor, and current Tauri command surface:

- Core process state and private API connectivity
- API error details when the dashboard snapshot cannot refresh cleanly
- recovery state and peer summary from the existing snapshot
- bundled Core manifest and binary verification status
- bundled Core executable path plus Desktop-managed data and log directories
- recent stdout and stderr tails from the fixed Desktop-managed log files
- support package availability and Desktop operator message
- mock mode indication and Desktop-side last refresh time

Known Diagnostics limitations:

- no arbitrary log browsing or file-system traversal is exposed in the UI
- active config path is not currently exposed by the Desktop service boundary
- raw log tails are capped and may omit older lines
- the page does not add write controls for mining or Core runtime behavior

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
