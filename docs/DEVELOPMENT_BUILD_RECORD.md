# Development Build Record

## Build

- Desktop application version: `0.1.0-alpha.1`
- Package manifest version: `0.1.0-alpha.1-dev`
- Build type: unsigned local Windows release bundles
- Build timestamp: `2026-07-31T22:43:47-04:00`
- Source baseline before this packaging commit: `7c96d031414693a881dbb448003cf9473ba6745a`
- Release executable: `src-tauri/target/release/vision-desktop.exe`
- Release executable size: `12078080` bytes
- Release executable SHA-256: `33521DE6B7D6A55582D949DB347280185422304E5E2DFF42E95A7AE78AB6D3B7`

## Windows Packages

- MSI: `src-tauri/target/release/bundle/msi/Vision Desktop_0.1.0-alpha.1_x64_en-US.msi`
- MSI size: `6361088` bytes
- MSI SHA-256: `3D9500A6028C21B615009929AB7EC7EC07B8639776941B9D557D73DBF81EC07A`
- NSIS: `src-tauri/target/release/bundle/nsis/Vision Desktop_0.1.0-alpha.1_x64-setup.exe`
- NSIS size: `4112002` bytes
- NSIS SHA-256: `F87C5E8418F8D9AD882CB27E721A172B87E3FDB64223C4DE53331509891AEDBB`
- Authenticode status for the release executable, MSI, and NSIS setup executable: `NotSigned`

## Bundled Core Baseline

- Bundled Core alpha tag: `vision-core-alpha-rc2`
- Bundled Core consensus tag: `vision-core-consensus-v1.0.3`
- Bundled Core commit: `6a065df8206b50874029a27ee2b54dffae5e3cdd`
- Bundled Core SHA-256: `41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01`
- Bundled manifest SHA-256: `5688813388F426EAB344557A934197FFF7241DACCCD42260C9F7479182EDFD16`
- Consensus version: `3`
- P2P protocol version: `4`

## Validation

- Frontend typecheck: passed
- Frontend state tests: passed
- Frontend production build: passed
- Rust formatting check: passed
- Strict Rust Clippy: passed
- Rust backend tests: `12 passed`, `0 failed`
- Tauri release and MSI/NSIS bundle build: passed
- MSI administrative extraction: passed without installing or registering the application
- Extracted bundled Core hash matched the frozen manifest: passed
- Extracted packaged application launch: passed
- Packaged Diagnostics manifest load and bundled Core verification: passed (`Verified`)
- Local non-elevated MSI install: correctly rejected with Windows Installer error `1925`; rollback left no product registration or install directory
- Local elevated MSI install: passed
- Installed-path application launch: passed
- Installed-path Diagnostics manifest load and bundled Core verification: passed (`Verified`)
- Local elevated MSI uninstall: passed; program files, product registration, shortcuts, and process were removed
- Local silent NSIS current-user install: passed
- NSIS installed-path application launch: passed
- NSIS installed-path Diagnostics manifest load and bundled Core verification: passed (`Verified`)
- Local silent NSIS direct uninstall: passed; program files, per-user shortcuts, and process were removed
- Repeated NSIS install/uninstall cleanup: passed
- NSIS Apps & Features registration: failed; no matching uninstall metadata was present in standard HKCU or HKLM uninstall locations

## Core Integration Result

Real Core launch remains blocked.

Frozen RC2 Core binds its HTTP API to `0.0.0.0:<VISION_HTTP_PORT>` and does not provide a loopback-only bind setting. Vision Desktop must not launch Core in real mode until the Core runtime exposes a private API bind address option.

The Desktop supervisor continues to enforce this restriction. Mock mode remains available for UI development. This packaging work changes only Desktop packaging, runtime resource discovery, and polling stability; it does not modify Vision-Core or bypass the launch restriction.

## Release Limitations

- The executable and both installers are unsigned.
- The production signing identity and secure signing procedure are not established.
- The updater is not implemented.
- The current ICO contains only a preliminary 16 x 16 image.
- The MSI lifecycle result is from the development workstation, not an isolated clean Windows system.
- The NSIS lifecycle result is a silent local test, not an interactive-wizard or clean-machine qualification.
- NSIS Windows uninstall discovery is missing even though the installed direct uninstaller succeeds.
- Clean-machine compatibility remains unqualified.
- Upgrade and downgrade require a future package version and remain untested.
- MSI repair is intentionally disabled (`ARPNOREPAIR=yes`, `NoRepair=1`); the release policy for repair remains to be confirmed.
- Real Core launch remains blocked by RC2 API bind behavior.
