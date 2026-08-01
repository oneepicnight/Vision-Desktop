# Windows Packaging Baseline

## Baseline Identity

- Recorded: `2026-07-31T22:48:40-04:00` on the ASUS Windows workstation
- Repository: `C:\Vision\Vision-Desktop`
- Branch: `main`
- Source baseline before this packaging commit: `7c96d031414693a881dbb448003cf9473ba6745a`
- Application version: `0.1.0-alpha.1`
- WiX numeric package version: `0.1.0.1`
- WiX language: `en-US`
- Stable WiX upgrade code: `59952895-6664-56c1-8ebc-f94bfe9dca35`
- Windows architecture: `x64`

The separate numeric WiX version is required because MSI package versions do not accept the application's prerelease label. The stable upgrade code must not be regenerated for future versions of this product.

## Build Command

```powershell
npm run tauri:build
```

The command runs the TypeScript/Vite production build, compiles the Tauri Rust application in release mode, and creates MSI and NSIS packages.

## Artifacts

| Artifact | Size | SHA-256 | Authenticode |
| --- | ---: | --- | --- |
| `src-tauri/target/release/vision-desktop.exe` | 12,078,080 bytes | `33521DE6B7D6A55582D949DB347280185422304E5E2DFF42E95A7AE78AB6D3B7` | Not signed |
| `src-tauri/target/release/bundle/msi/Vision Desktop_0.1.0-alpha.1_x64_en-US.msi` | 6,361,088 bytes | `3D9500A6028C21B615009929AB7EC7EC07B8639776941B9D557D73DBF81EC07A` | Not signed |
| `src-tauri/target/release/bundle/nsis/Vision Desktop_0.1.0-alpha.1_x64-setup.exe` | 4,112,002 bytes | `F87C5E8418F8D9AD882CB27E721A172B87E3FDB64223C4DE53331509891AEDBB` | Not signed |

These files are ignored local build outputs. The hashes identify this workstation build and are not release signatures.

## Bundled Resources

Both installers place these resources next to the installed Desktop executable under `bundled/core/windows-x64/`:

- `vision-core.exe` - SHA-256 `41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01`
- `manifest.json` - SHA-256 `5688813388F426EAB344557A934197FFF7241DACCCD42260C9F7479182EDFD16`

Vision Desktop resolves these files from Tauri's runtime resource directory. Repository-relative lookup remains only as the development/test fallback. The packaged application does not depend on the source checkout path.

## Package Verification

The MSI was administratively extracted to a fresh temporary directory. Administrative extraction does not install or register the application. Inspection confirmed:

- the Desktop executable and support DLL were present;
- the manifest and frozen Core executable were present at the intended relative paths;
- the extracted Core executable hash matched the frozen manifest;
- the packaged Desktop executable launched successfully;
- mock mode displayed the current application shell;
- switching to live observation mode left Core stopped and preserved the RC2 safety restriction;
- Diagnostics loaded the packaged manifest and reported the bundled Core binary as `Verified` from the extracted package path.

The MSI and NSIS installers themselves were not executed as installations during this baseline. No registry, Start menu, uninstall, upgrade, repair, or rollback qualification is claimed.

## Confirmed Packaging Fixes

- Added the explicit Windows icon required by the Tauri bundle step.
- Mapped the prerelease application version to the WiX-compatible numeric package version `0.1.0.1`.
- Fixed a stable WiX upgrade code for future product upgrades.
- Preserved the intended nested bundled-resource layout in MSI and NSIS outputs.
- Changed Core manifest/binary discovery from a compile-time repository path to the Tauri runtime resource directory.
- Stabilized the polling callback so packaged diagnostic results are not superseded by effect restarts during state updates.

## Public Release Blockers

- Acquire and protect an appropriate Windows code-signing identity.
- Sign and verify the executable, MSI, and NSIS setup executable in a controlled release workflow.
- Replace the preliminary 16 x 16 ICO with a production multi-resolution Windows icon set.
- Qualify install, uninstall, upgrade, downgrade, repair, and rollback behavior on clean supported Windows systems.
- Define and implement the updater only after signed update metadata and release hosting are approved.
- Retain the real-Core launch block until Vision-Core provides a loopback-only private API bind setting.

Vision-Core was not modified during this work.
