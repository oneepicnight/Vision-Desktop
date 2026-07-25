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

Mock mode is enabled from the UI and does not require Vision Core. Real Core mode uses the bundled RC2 binary and loopback API polling.

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
