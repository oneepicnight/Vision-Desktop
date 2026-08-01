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

The NSIS installer was not executed as an installation during this baseline. Upgrade and downgrade behavior remain untested because only one package version exists. MSI repair is intentionally disabled by the package (`ARPNOREPAIR=yes`, `NoRepair=1`) and was not tested.

## Local MSI Lifecycle Qualification

Recorded: `2026-07-31T23:06:11-04:00` on the same ASUS Windows workstation.

This was a local workstation qualification, not a clean-machine test. Windows Sandbox, Hyper-V management tools, VirtualBox, VMware, and an installed WSL environment were unavailable, so no isolated Windows environment was available locally.

The MSI reported this installed-product identity:

- Product name: `Vision Desktop`
- Product version: `0.1.0.1`
- Product code: `{662368E4-A14A-4D80-B509-C068710D197F}`
- Upgrade code: `{59952895-6664-56C1-8EBC-F94BFE9DCA35}`
- Manufacturer: `vision`
- Installation scope: per-machine (`ALLUSERS=1`)

The first non-elevated quiet install returned Windows Installer code `1603` with error `1925`, confirming that the per-machine package requires administrative privileges. Windows Installer rolled back cleanly and did not leave the product directory or registration behind.

The elevated MSI lifecycle then passed:

- elevated quiet installation returned `0`;
- `C:\Program Files\Vision Desktop\vision-desktop.exe` and `vision_desktop_lib.dll` were installed;
- the bundled Core executable and manifest were installed at `bundled\core\windows-x64\`;
- the installed Core and manifest hashes matched the package baseline above;
- the expected all-users Start menu and public Desktop shortcuts were created;
- the expected HKLM uninstall registration was created with modification and repair disabled;
- the installed executable launched from `C:\Program Files\Vision Desktop`;
- mock mode rendered normally;
- live observation mode preserved the RC2 launch safety block;
- Diagnostics loaded the installed manifest and reported the installed Core binary as `Verified`;
- elevated quiet uninstallation returned `0` and Windows Installer logged `Removal completed successfully`;
- the program directory, HKLM product registration, Start menu folder, public Desktop shortcut, and running process were absent after uninstall.

This proves the local MSI install, installed-path launch, and uninstall paths for this unsigned engineering build. It does not qualify clean-machine compatibility, NSIS installation, upgrade, downgrade, code signing, or public distribution.

## Local NSIS Lifecycle Qualification

Recorded: `2026-07-31T23:42:24-04:00` on the same ASUS Windows workstation.

This was a local silent-install qualification, not an interactive-wizard or clean-machine test. The Tauri CLI `2.11.4` schema defaults NSIS to `currentUser`, and the package installed without elevation at:

`C:\Users\bighe\AppData\Local\Vision Desktop`

The local NSIS lifecycle confirmed:

- silent installation returned `0`;
- `vision-desktop.exe` was installed with product/file version `0.1.0-alpha.1`, size `12,078,080` bytes, and SHA-256 `9F4F8597BACDC6C9CCEC948315844AB5A12EDFC466A36391A2D12B46A579AFB9`;
- `uninstall.exe` was installed with SHA-256 `8A00C080F7ED07089A947B9693295BB3E4A5D8FCA3C7E4E76517B35059D5F298`;
- the bundled Core executable and manifest were installed at `bundled\core\windows-x64\` and matched the frozen hashes above;
- per-user Start menu and Desktop shortcuts were created;
- Windows uninstall metadata was registered at `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vision Desktop` with the expected display name, version, publisher, install location, icon, uninstaller, estimated size, and modify/repair policy;
- the installed executable launched from the per-user installation directory;
- mock mode rendered normally;
- live observation mode left Core stopped and preserved the RC2 safety restriction;
- Diagnostics reported the installed Core binary as `Verified`;
- silent uninstallation through the installed `uninstall.exe` returned `0`;
- the program directory, both per-user shortcuts, and running process were absent after uninstall;
- a repeated install/uninstall cycle produced the same cleanup result.

The first uninstall-registration check incorrectly queried the sandbox worker's HKCU hive while the installer had run under the host user. Repeating the check under the same host identity as the installer confirmed the expected registration. After direct uninstallation, that uninstall key was absent along with the program directory, shortcuts, and process.

This proves the local silent NSIS install, Windows uninstall registration, installed-path launch, direct-uninstaller, and cleanup paths for this unsigned engineering build. It does not qualify the interactive installer UI, clean-machine compatibility, upgrade, downgrade, code signing, or public distribution.

## Interactive NSIS Lifecycle Qualification

Recorded: `2026-08-01T00:30:53-04:00` on the same ASUS Windows workstation.

The visible current-user installer and uninstaller flow passed locally:

- opening the installer began the per-user installation immediately; this package does not present a separate pre-install confirmation page;
- the installer displayed `Installation Complete` and `Setup was completed successfully`;
- the details view accurately listed the per-user output directory, Desktop executable, bundled Core directory, manifest, Core executable, generated uninstaller, and Start menu shortcut;
- the finish page offered checked, explicit options to run Vision Desktop and create a Desktop shortcut;
- selecting Finish closed Setup, created the requested Desktop shortcut, and launched the installed executable;
- the host-user Windows uninstall registration, installed payload hashes, and shortcut locations matched the silent-install qualification above;
- the installed app rendered normally in mock mode;
- live observation mode left Core stopped and preserved the RC2 launch restriction;
- Diagnostics reported the installed Core binary as `Verified`;
- the interactive uninstaller displayed the exact installation directory and an unchecked `Delete the application data` option;
- after explicit approval, interactive uninstall removed the Windows uninstall key, program directory, Start menu shortcut, Desktop shortcut, and running process;
- because application-data deletion was not selected, `C:\Users\bighe\AppData\Local\com.vision.desktop` and its WebView data remained.

The interactive NSIS lifecycle is functionally qualified on this workstation. The immediate one-step installation flow and generic NSIS visual presentation remain release UX review items. Clean-machine compatibility, upgrades, downgrades, signing, and public distribution are not qualified by this result.

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
- Repeat install and uninstall qualification on clean supported Windows systems.
- Review and brand the immediate one-step NSIS installer experience for public release.
- Qualify future-version upgrade, downgrade, and rollback behavior.
- Confirm the intentionally disabled MSI repair policy remains appropriate for release.
- Define and implement the updater only after signed update metadata and release hosting are approved.
- Retain the real-Core launch block until Vision-Core provides a loopback-only private API bind setting.

Vision-Core was not modified during this work.
