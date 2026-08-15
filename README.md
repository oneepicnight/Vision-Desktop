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

The Dashboard operations grid adapts the legacy mission-control card hierarchy around the existing process, chain, network, mining/recovery, resource, and support data. It does not import legacy wallet approval, reward linking, guardian status, inferred peer-health grades, hashrate, block timing, block-production claims, or hard-coded dashboard links. Support actions remain limited to the existing privacy-hardened support package and fixed Desktop-managed log/data directories.

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

The Wallet uses a Vision World command-center presentation adapted from the legacy wallet's visual hierarchy. The unpublished atomic candidate connects that presentation to twelve reviewed Tauri commands for native create, restore, unlock, lock, transfer preview, trusted confirmation, submission, authenticated local activity, and receipt observation.

All secret-bearing work remains native Rust. React receives only public account identifiers, public previews, public transaction identifiers, fixed error codes, and authenticated public activity. It never receives passwords, recovery credentials, selected filesystem paths, seeds, private keys, signatures, or signed envelopes. Native final confirmation displays the exact recipient, amount, fee, and transaction identifier before signing. Ambiguous or accepted-but-not-recorded outcomes are durably rediscovered and block new spending; Desktop never retries or replaces a transaction automatically.

This candidate is unpublished and not approved for real-funds use. Exact-source review, packaged Windows qualification, clean-device recovery testing, evidence acceptance, and a separate publication authorization remain mandatory.

## Wallet readiness

The reviewed private wallet foundation is merged into `main`. Branch `feat/atomic-wallet-exposure`
now prepares the complete unpublished twelve-command source candidate as one all-or-nothing review
unit. Its command registration, permissions, capability, typed frontend boundary, public-only UI,
activation scopes, and exact admitted Core contract must be reviewed and qualified together; no
partial wallet artifact may ship.

Vision-Core commit `890c98a02c7147e166805fe52002d22d1fcd81f9` produced the independently
accepted Windows artifact used by the isolated Desktop admission implementation. The runtime
manifest pins its exact source tree, executable hash and size, evidence-manifest hash, loopback
policy, peer-binding contract, routes, and fee/submission semantics. Real Core launch is permitted
only after handle-bound manifest and executable admission, exact running-image verification, and
same-generation loopback-listener ownership verification. Controlled compatibility evidence for
this admission boundary has been independently accepted. See
`docs/FROZEN_CORE_ARTIFACT_ADMISSION_HANDOFF.md` and `docs/CORE_ARTIFACT_INTAKE_CHECKLIST.md`.

The supported wallet host boundary is one interactive session per Windows account on Windows 11
build families 26100 (24H2), 26200 (25H2), or 28000 (26H1), limited to the reviewed non-evaluation
Home, Pro, Enterprise, Education, LTSC, Workstations, and Pro Education variants listed in
`docs/WALLET_RUNTIME_SECURITY.md`. Evaluation, Server/RDS, multi-session, IoT, Cloud, unlisted,
unknown, and future Windows editions/build families fail closed. The per-user `Global\` wallet
lease still spans Windows sessions as defense in depth and denies a second runtime, but that
mechanism does not expand the supported platform boundary.

The approved target is an embedded user-controlled wallet whose secret-bearing operations remain inside Rust. Its encrypted vault combines password hardening, authenticated encryption, a Windows current-user DPAPI-protected local factor, operating-system randomness, restrictive verified storage, idle locking, unlock backoff, and a restrictive per-user global process lease. Portable recovery uses an independently encrypted artifact without DPAPI machine binding. Creation verifies the native recovery ceremony and portable backup before publishing the local vault; restore never alters its source. The admitted v1.0.4 contract fixes address derivation, denomination, nonce and fee rules, serialization, identifiers, Ed25519 signatures, submission responses, and receipt observations. Authenticated local activity is intentionally not complete chain history and never controls balances, nonces, signing, or success. The protocol has no deterministic finality, so Desktop reports observed confirmations and never claims irreversibility.

The wallet Tauri threat model is documented in `docs/WALLET_TAURI_COMMAND_THREAT_MODEL.md`. The atomic candidate grants each of its twelve exact commands only to the labelled Windows `main` WebView; the Linux mock capability remains read-only and has no wallet authority. Production WebView connectivity is IPC-only. Duplicate launches are rejected before managed wallet state initializes. A hidden Rust-only listener clears authority on session lock, suspend/standby, logoff/shutdown, window teardown, page load, and Core stop or restart; unlock and resume never restore a prior session. The pinned dialog plugin is initialized for private Rust use, but React receives no dialog permission. Local vault and portable-recovery passwords are collected only by native zeroizing controls.

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
- raw stdout and stderr are deliberately excluded from generated support packages even though their bounded tails remain visible locally in Diagnostics
- support packages use an exact file allowlist and include only non-identifying configuration counts and booleans; node names, operating mode, exact ports, peer endpoints, advertised hosts, payout addresses, filesystem paths, private activity, and all custody material are excluded
- support-package IPC returns only the package SHA-256 and fixed assessment; native destination paths and filesystem errors never cross into the WebView
- the page does not add write controls for mining or Core runtime behavior

Admitted Core baseline for controlled local development:

- Core release tag: `vision-core-v1.0.4`
- Consensus tag: `vision-core-consensus-v1.0.3`
- Source commit: `890c98a02c7147e166805fe52002d22d1fcd81f9`
- Source tree: `2ae583bbfc887490b8af1398aead7b916796700c`
- Consensus version: `3`
- P2P protocol version: `4`
- Windows x64 binary SHA-256: `8082d57c0f4a5cb82af9696fe4d53aeb65fcb280c062afe81abdcfe78e12ed28`
- Accepted evidence manifest SHA-256: `35f3233003a0b0c39d9331e0d3771b6d472aef3e556a516557f2b62d0aacb64a`

The executable remains an ignored local/release input and is never committed to this repository.

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
