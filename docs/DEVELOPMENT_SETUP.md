# Development Setup

## Prerequisites

- Windows x64
- Rust stable toolchain
- Node.js and npm
- Microsoft Edge WebView2 runtime
- Tauri Windows prerequisites

## Install Dependencies

```powershell
npm install
```

## Run In Development

```powershell
npm run tauri:dev
```

Mock mode is enabled from the UI and does not require Vision Core. Real Core mode requires the
exact admitted Vision Core v1.0.4 Windows artifact. Stage it from the independently accepted
evidence package before launch:

```powershell
.\scripts\stage-frozen-core.ps1 -EvidenceRoot <accepted-evidence-directory>
```

The staging script first verifies the separate exact Desktop-integration acceptance record, then the
original complete evidence inventory and candidate hash and size. The supervisor performs
handle-bound acceptance, manifest, executable, running-image, process-generation, IPv4/IPv6 listener,
and loopback-owner checks. A kill-on-close Windows Job Object prevents Core from escaping Desktop
ownership. The executable is ignored and must not be committed.

## Backend Tests

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

## Frontend Checks

```powershell
npm run typecheck
npm run build
```

## Repository Boundary

Do not copy Vision Core source into this repository. The only Core artifact currently allowed for local development is the frozen Windows x64 binary plus manifest under `bundled/core/windows-x64`.
