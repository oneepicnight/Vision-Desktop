# Wallet Dependency Provenance

## Status

This record began as dependency admission. The single-instance crate is now initialized on Windows as the first plugin, while the dialog crate remains uninitialized. No JavaScript package, Tauri capability, wallet command, file dialog, or frontend wallet behavior is enabled.

## Admission record

Reviewed on 2026-08-01 against the official Tauri documentation, the crates.io registry metadata, and the generated Cargo lockfile.

| Crate | Exact version | Registry | License | Minimum Rust | Repository | Selected features |
| --- | --- | --- | --- | --- | --- | --- |
| `tauri-plugin-dialog` | `2.7.2` | crates.io | Apache-2.0 OR MIT | 1.77.2 | `tauri-apps/plugins-workspace` | Default features disabled |
| `tauri-plugin-single-instance` | `2.4.3` | crates.io | Apache-2.0 OR MIT | 1.77.2 | `tauri-apps/plugins-workspace` | No optional features |

Vision Desktop currently builds with Rust 1.96.1 and Tauri 2.11.5, so both plugins meet the active toolchain and Tauri-major compatibility boundary.

Locked registry checksums:

- `tauri-plugin-dialog 2.7.2`: `b2d3c1dbe38037e7f590cdf2492594d5ceebe031e7bc7e827509b22a999d2940`
- `tauri-plugin-single-instance 2.4.3`: `b3214becf9ef5783c0ae99a3bb25adf5353a7a16ebf53e74b909e29205735c6c`

Official references:

- [Tauri dialog plugin](https://v2.tauri.app/plugin/dialog/)
- [Tauri single-instance plugin](https://v2.tauri.app/plugin/single-instance/)
- [tauri-plugin-dialog 2.7.2](https://crates.io/crates/tauri-plugin-dialog/2.7.2)
- [tauri-plugin-single-instance 2.4.3](https://crates.io/crates/tauri-plugin-single-instance/2.4.3)

## Scope controls

- Both dependencies are declared only under `cfg(windows)`.
- Versions use exact `=` requirements so normal dependency resolution cannot silently select a later plugin release.
- The dialog crate disables its default Linux GTK feature because the admitted target is Windows.
- The dialog JavaScript package is absent from `package.json` and `package-lock.json`.
- The single-instance plugin is initialized first in `src-tauri/src/lib.rs`; duplicate-launch arguments and working directories are discarded, and only the existing `main` window is restored and focused.
- The dialog plugin remains uninitialized.
- No dialog or single-instance permission is granted to a WebView.
- No application capability file, wallet Tauri command, or frontend service wrapper is added.
- Cargo registry checksums are committed in `src-tauri/Cargo.lock` and must remain part of release verification.

Cargo added 47 lockfile entries for the plugins' complete cross-platform resolution graph. The Windows build uses a smaller target-specific subset, but all locked packages remain part of the reviewed supply-chain surface.

The `Rust dependency audit` GitHub Actions workflow installs exact `cargo-audit 0.22.2` from its locked release graph and scans `src-tauri/Cargo.lock` on dependency changes, weekly, and on manual request. The workflow has read-only repository permission, pins `actions/checkout` to its reviewed full commit SHA, persists no checkout credentials, and has no advisory ignore list. Known vulnerabilities and yanked crates fail the job; informational and unsoundness warnings remain visible in the log for explicit follow-up.

The admission-time scan on 2026-08-01 loaded 1,178 RustSec advisories and reported no known vulnerability or yanked crate. It reported 18 warnings: 16 unmaintained GTK/Unicode/macro dependencies, the direct unmaintained `bincode 1.3.3`, and `RUSTSEC-2024-0429` for Linux GTK `glib 0.18.5`. The GTK warning applies to cross-platform packages retained in the lockfile and not built into the Windows target. `bincode 1.3.3` is currently pinned to the verified Vision-Core RC2 transaction serialization contract; replacing it requires a separate Core compatibility decision and exact-vector validation. No warning is hidden or placed on an ignore list.

## Deferred review

Single-instance initialization completed with source-contract, lifecycle, ordinary duplicate-launch,
forced-termination recovery, and 12-process burst tests. Source review also identified a narrow
Windows mutex/receiver startup interval, so the plugin is not accepted as the sole future custody
lock. Before native dialog initialization occurs, review must still prove:

1. The native dialog plugin is invoked only from Rust application commands; its JavaScript package and WebView permissions remain absent.
2. Selected recovery paths remain in Rust and are represented to the WebView only by short-lived, single-use, window-bound opaque tokens.
3. Application shutdown, window destruction, cancellation, and dialog-plugin failure preserve fail-closed wallet locking.
4. The complete dependency tree and registry checksums receive release-time vulnerability and provenance review in addition to the automated CI gate.
