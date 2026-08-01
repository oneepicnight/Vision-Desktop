# Vision Desktop

Vision Desktop is the user-facing node manager for Vision Core. It launches and controls a bundled Vision Core executable internally so users do not need Rust, Cargo, Node.js, localhost URLs, TOML editing, manual hash checks, or manual process management.

This repository is intentionally separate from Vision Core. It does not contain Vision Core source code and must not implement or duplicate consensus logic.

Initial version: `0.1.0-alpha.1-dev`

Current desktop scope includes:

- node-manager dashboard
- desktop-managed process controls
- read-only wallet account view for configured reward-address visibility and confirmed account lookup data
- read-only marketplace integration view with no market feed or transaction actions
- read-only blockchain explorer for address and transaction inspection
- read-only peer manager for current connection and recovery visibility
- read-only mining status from the existing Desktop snapshot
- read-only diagnostics view for process, API, verification, and fixed log-tail visibility
- read-only node configuration view for Desktop-managed configuration visibility and limited runtime comparison
- support package generation
- mock-mode development workflows

The Desktop shell uses a local Vision World-inspired dark-space theme with namespaced design tokens, glass operator panels, and glowing navigation. The theme is implemented entirely within Vision Desktop: it does not import the legacy wallet runtime, external CDN scripts, wallet custody code, direct browser API calls, or additional frontend dependencies.

The native Desktop accessibility baseline provides one page-level view heading, a keyboard-visible skip link past the persistent sidebar, visible focus treatment, native disabled semantics for unavailable controls, and reduced-motion overrides. Native Tauri development also excludes Rust build output from Vite file watching so locked Windows build artifacts cannot terminate the application launch. These changes affect presentation and development reliability only; service, state, event, lifecycle, and Vision Core boundaries remain unchanged.

The Dashboard includes a dependency-free Vision World network overview adapted from the legacy globe motif. Its globe and orbit artwork are decorative CSS, while the displayed Core state, chain height, peer count, and recovery state come only from the existing Desktop snapshot. It does not claim or infer geographic peer locations.

The Dashboard operations grid adapts the legacy mission-control card hierarchy around the existing process, chain, network, mining/recovery, resource, and support data. It does not import legacy wallet approval, reward linking, guardian status, inferred peer-health grades, hashrate, block timing, block-production claims, or hard-coded dashboard links. Support actions remain limited to the existing redacted support package and fixed Desktop-managed log/data directories.

The Explorer uses a Vision World chain-intelligence presentation around the existing typed address and transaction lookup actions. Chain context is taken from the shared Desktop snapshot, returned address and amount strings are preserved exactly, and no legacy polling, hard-coded endpoint, transaction submission, or protocol interpretation is included.

The Peer Manager uses a Vision World constellation presentation around the existing read-only peer snapshot. Constellation markers reflect only the number of reported directory entries and do not represent geography; Desktop does not invent peer locations, latency, trust, reputation, routing scores, or connectivity probes.

The Mining page uses a Vision World operations presentation adapted from the legacy mining command-center motif. Its reactor artwork and status lighting are decorative; all displayed state comes from the existing Desktop snapshot and Desktop-managed configuration. Legacy localhost calls, miner controls, pools, farms, performance tuning, wallet linking, reward calculations, and unsupported telemetry are not included.

The Diagnostics page uses a Vision World systems-observatory presentation adapted from the legacy command-center and log-console motifs. Its radar artwork is decorative, while process, API, recovery, peer, manifest, verification, operator-message, and log-tail values come only from the existing typed Desktop state and fixed backend commands. Legacy live-stream connections, client-side log export, invented log classification, direct network calls, and unrestricted file access are not included.

The Configuration page uses a Vision World node-blueprint presentation adapted from the legacy settings and command-center hierarchy. Its blueprint artwork is decorative, while every configured/default/runtime comparison comes from the existing filtered configuration view model. Legacy endpoint editing, connectivity tests, browser storage, wallet backup export, mnemonic or private-key display, key downloads, and wallet-wipe behavior are not included.

Desktop lifecycle controls currently include Start, Stop, Restart, and Refresh from the top application bar.

The lifecycle surface uses a Vision World operator-console presentation around the existing tested lifecycle view model. Process state, recovery context, mock-mode locking, action progress, and restart confirmation remain derived through the existing Desktop state/event/request boundaries; no legacy direct process call, shell command, automatic retry, or invented transition state is included.

Lifecycle behavior and safety boundary:

- Start is available only when the observed Core process state is stopped or crashed
- Stop is available only when the observed Core process state is running or crashed
- Restart is available only when the observed Core process state is running or crashed
- lifecycle controls are disabled in mock mode
- restart requires an explicit confirmation step before the command is sent
- command completion and observed process state are treated as separate facts
- while a lifecycle action is in progress, conflicting lifecycle actions and manual refresh are disabled
- recovery state is shown for operator context but does not independently disable lifecycle actions

The Dashboard Create Node workflow uses the same Vision World operator presentation while retaining the existing typed Desktop configuration save action. It edits only the current public node configuration fields: node name, supported mode, P2P port, seed peers, advertised host, mining-enabled status, and public reward address.

Create Node workflow boundaries:

- saving configuration does not automatically start or restart Core
- Desktop does not open firewall or router ports
- no arbitrary endpoint probe or connectivity test is performed
- no secret, private key, seed phrase, mnemonic, keystore, or signing field is accepted
- the configured reward address remains a public identifier and does not prove custody
- lifecycle controls continue to rely on observed process state rather than configuration intent

The Wallet uses a Vision World command-center presentation adapted from the legacy wallet's visual hierarchy. Its account hero, context strip, address provenance, account observation, and security-boundary cards use only the existing read-only Desktop view model; legacy wallet runtime behavior is not included.

The initial Wallet page is read-only. It displays only confirmed non-secret data already available through the existing Desktop configuration and read-only backend surface:

- configured mining reward address loaded from the persisted Desktop-managed node configuration (never from an unsaved setup form)
- live address, balance, and nonce only when the existing read-only address lookup path returns them
- explicit address source labels
- Core, recovery, mock-mode, and freshness context
- exact balance strings as returned by the backend

Wallet limitations and security boundary:

- a configured reward address does not prove Desktop custody or ownership
- no private keys, seed phrases, mnemonics, keystores, signing, imports, exports, or transaction submission are implemented
- balance denomination and precision metadata are not currently exposed, so values are displayed exactly as returned
- transaction or receipt history is not currently exposed by the Desktop service boundary

The approved target is an embedded, non-custodial wallet whose secret-bearing operations remain inside the Rust backend. An internal encrypted-vault foundation now uses password hardening, authenticated encryption, operating-system randomness, redacted errors, and encrypted-only create-new storage, but it is not exposed through Tauri commands. The current release still has custody, creation, recovery, unlock, signing, and sends disabled. `docs/WALLET_SECURITY_ARCHITECTURE.md` and `docs/WALLET_CORE_CONTRACT_GAPS.md` define the fail-closed gates that must pass before real keys or funds are handled.

The Marketplace uses the legacy wallet-marketplace market-terminal hierarchy as a visual reference only. Its read-only observatory shows Core, recovery, mock-mode, and refresh context from the existing Desktop state while making the missing marketplace service boundary explicit. It does not display fallback prices, fabricated order books, balances, listings, trades, or transaction history.

Marketplace limitations and security boundary:

- no marketplace, exchange, land-listing, cash-order, checkout, or settlement API is connected
- no direct browser fetch, hard-coded localhost endpoint, WebSocket, or additional polling loop is present
- no buy, sell, checkout, order placement, mint, replay, or payment action is exposed
- no floating-point price or amount arithmetic is performed
- no wallet keys, custody, signing, or ownership claims are introduced
- live integration requires an explicitly approved typed Desktop service boundary; Vision Desktop does not invent the external API

The initial Mining page is read-only. It displays confirmed data already present in the Desktop snapshot and Desktop-managed node configuration:

- runtime mining enabled status when reported by Core
- runtime active or inactive state when reported by Core
- mining availability from the existing status snapshot
- paused reason and recovery state when reported by Core
- current height context
- Desktop-managed mining configuration and reward address
- Desktop-side last refresh time

Enabled mining does not necessarily mean that mining is actively producing blocks.

The Mining command center deliberately does not display or derive hashrate, rewards, profitability, worker counts, or block-production claims when those facts are not available through the current Desktop boundary. It also contains no start, stop, pause, resume, pool, farm, or performance controls.

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
- no endpoint editor, save/test action, browser storage, key export, mnemonic display, private-key display, or destructive wallet action is imported from the legacy settings page

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
- stdout and stderr are fixed snapshot tails, not a live streaming console
- Desktop does not infer severity, category, or node health from arbitrary log text
- support-package generation and directory opening remain the only support actions; they use existing Desktop-managed paths
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

Build the Windows desktop bundles:

```powershell
npm run tauri:build
```

The Windows build produces both an x64 MSI and an x64 NSIS setup executable under `src-tauri/target/release/bundle/`. The current alpha packages are unsigned local engineering artifacts and are not suitable for public distribution until the code-signing and release process is established. The MSI uses the stable upgrade code documented in `docs/WINDOWS_PACKAGING_BASELINE.md`, while its numeric package version maps the application prerelease `0.1.0-alpha.1` to `0.1.0.1` for WiX compatibility.

Run Rust backend tests:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

## Repository Boundary

Vision Desktop owns UI, process lifecycle, local configuration, Core binary verification, installer/updater, reports, and diagnostics. Vision Core owns consensus, block validation, P2P protocol, mining, persistence, replay, and state execution.
