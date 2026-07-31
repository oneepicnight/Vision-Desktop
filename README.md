# Vision Desktop

Vision Desktop is the user-facing node manager for Vision Core. It launches and controls a bundled Vision Core executable internally so users do not need Rust, Cargo, Node.js, localhost URLs, TOML editing, manual hash checks, or manual process management.

This repository is intentionally separate from Vision Core. It does not contain Vision Core source code and must not implement or duplicate consensus logic.

Initial version: `0.1.0-alpha.1-dev`

Current desktop scope includes:

- node-manager dashboard
- desktop-managed process controls
- read-only wallet account view for configured reward-address visibility and confirmed account lookup data
- read-only blockchain explorer for address and transaction inspection
- read-only peer manager for current connection and recovery visibility
- read-only mining status from the existing Desktop snapshot
- read-only diagnostics view for process, API, verification, and fixed log-tail visibility
- read-only node configuration view for Desktop-managed configuration visibility and limited runtime comparison
- support package generation
- mock-mode development workflows

The Desktop shell uses a local Vision World-inspired dark-space theme with namespaced design tokens, glass operator panels, and glowing navigation. The theme is implemented entirely within Vision Desktop: it does not import the legacy wallet runtime, external CDN scripts, wallet custody code, direct browser API calls, or additional frontend dependencies.

The Dashboard includes a dependency-free Vision World network overview adapted from the legacy globe motif. Its globe and orbit artwork are decorative CSS, while the displayed Core state, chain height, peer count, and recovery state come only from the existing Desktop snapshot. It does not claim or infer geographic peer locations.

Desktop lifecycle controls currently include Start, Stop, Restart, and Refresh from the top application bar.

Lifecycle behavior and safety boundary:

- Start is available only when the observed Core process state is stopped or crashed
- Stop is available only when the observed Core process state is running or crashed
- Restart is available only when the observed Core process state is running or crashed
- lifecycle controls are disabled in mock mode
- restart requires an explicit confirmation step before the command is sent
- command completion and observed process state are treated as separate facts
- while a lifecycle action is in progress, conflicting lifecycle actions and manual refresh are disabled
- recovery state is shown for operator context but does not independently disable lifecycle actions

The initial Wallet page is read-only. It displays only confirmed non-secret data already available through the existing Desktop configuration and read-only backend surface:

- configured mining reward address from the Desktop-managed node configuration
- live address, balance, and nonce only when the existing read-only address lookup path returns them
- explicit address source labels
- Core, recovery, mock-mode, and freshness context
- exact balance strings as returned by the backend

Wallet limitations and security boundary:

- a configured reward address does not prove Desktop custody or ownership
- no private keys, seed phrases, mnemonics, keystores, signing, imports, exports, or transaction submission are implemented
- balance denomination and precision metadata are not currently exposed, so values are displayed exactly as returned
- transaction or receipt history is not currently exposed by the Desktop service boundary

The initial Mining page is read-only. It displays confirmed data already present in the Desktop snapshot and Desktop-managed node configuration:

- runtime mining enabled status when reported by Core
- runtime active or inactive state when reported by Core
- mining availability from the existing status snapshot
- paused reason and recovery state when reported by Core
- current height context
- Desktop-managed mining configuration and reward address
- Desktop-side last refresh time

Enabled mining does not necessarily mean that mining is actively producing blocks.

The initial Configuration page is read-only. It displays confirmed Desktop-managed configuration data plus limited runtime observations already exposed through the existing Desktop state and backend surface:

- persisted or Desktop-default node configuration source when Desktop can load it
- Desktop-managed node name, mode, ports, paths, seed peers, mining status, and reward address
- runtime-observed process, API, port, and directory values only where the existing process and snapshot models expose them
- explicit configured versus runtime-observed labeling
- mock-mode context, configuration-source context, and last refresh time

Configuration limitations and safety boundary:

- this page does not edit, save, import, export, reset, or apply configuration
- enabled or configured values do not prove they are active in the running Core process unless a matching runtime observation is exposed
- API bind host and private-peer policy are not currently exposed by the Desktop configuration model
- secret-bearing values are deliberately excluded from the page
- the active persisted node-config source path is shown only through the Desktop-managed config location, not through arbitrary file browsing

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
