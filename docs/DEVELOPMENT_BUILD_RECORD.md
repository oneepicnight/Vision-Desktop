# Development Build Record

## Build

- Desktop application version: `0.1.0-alpha.1`
- Package manifest version: `0.1.0-alpha.1-dev`
- Build type: unsigned local Windows release bundles
- Build timestamp: `2026-08-01T00:45:10-04:00`
- Source baseline before this production artwork commit: `e4013558ae827e32331c84bc557a76106c09b575`
- Release executable: `src-tauri/target/release/vision-desktop.exe`
- Release executable size: `12158976` bytes
- Release executable SHA-256: `2E752C3138178A23F19DD743BC32EB9C8313E34B6F5063B80BBF384D74E2A4CA`

## Windows Packages

- MSI: `src-tauri/target/release/bundle/msi/Vision Desktop_0.1.0-alpha.1_x64_en-US.msi`
- MSI size: `6520832` bytes
- MSI SHA-256: `7AFBE2BD99480CA9E4F9516BA5A1171C7D7C5A60E2E64ECA84590FC2B3A3D938`
- NSIS: `src-tauri/target/release/bundle/nsis/Vision Desktop_0.1.0-alpha.1_x64-setup.exe`
- NSIS size: `4283643` bytes
- NSIS SHA-256: `953F6F3DB2ED3021C854B64586C7119C79912D6CB86BE09EA142508CAF8F4C08`
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
- Production Windows artwork generation and dimension/frame validation: passed
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
- NSIS Windows uninstall registration: passed at `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vision Desktop`; the key was removed during uninstall
- Interactive NSIS installation-completion and details pages: passed
- NSIS finish-page run and Desktop-shortcut options: passed
- Interactive NSIS installed-app launch and Diagnostics Core verification: passed (`Verified`)
- Interactive NSIS uninstall with application-data deletion unchecked: passed; application binaries, shortcuts, registration, and process were removed while the retained WebView data directory remained
- Branded NSIS installer cyan header and completion-page Vision sidebar visual verification: passed
- Branded NSIS uninstaller coral header visual verification: passed
- Production multi-resolution icon in the packaged application window: passed

## Core Integration Result

Real Core launch remains blocked.

Frozen RC2 Core binds its HTTP API to `0.0.0.0:<VISION_HTTP_PORT>` and does not provide a loopback-only bind setting. Vision Desktop must not launch Core in real mode until the Core runtime exposes a private API bind address option.

The Desktop supervisor continues to enforce this restriction. Mock mode remains available for UI development. This packaging work changes only Desktop packaging, production artwork, runtime resource discovery, and polling stability; it does not modify Vision-Core or bypass the launch restriction.

## Release Limitations

- The executable and both installers are unsigned.
- The production signing identity and secure signing procedure are not established.
- The updater is not implemented.
- The MSI lifecycle result is from the development workstation, not an isolated clean Windows system.
- The silent and interactive NSIS lifecycle results are from the development workstation, not an isolated clean Windows system.
- The branded NSIS lifecycle has not been qualified across every supported Windows display scale or accessibility configuration.
- Clean-machine compatibility remains unqualified.
- Upgrade and downgrade require a future package version and remain untested.
- MSI repair is intentionally disabled (`ARPNOREPAIR=yes`, `NoRepair=1`); the release policy for repair remains to be confirmed.
- Real Core launch remains blocked by RC2 API bind behavior.
