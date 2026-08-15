# Tauri Command Access Control

## Atomic candidate boundary

The unpublished Windows candidate registers 31 application commands with
`tauri_build::AppManifest`: 19 existing Desktop commands plus exactly 12 wallet commands. Every
command has its own generated allow/deny permission. The single `main-desktop` capability:

- applies only to the explicitly labelled `main` window on Windows;
- has no remote-origin grant;
- contains no wildcard, shell, filesystem, HTTP, clipboard, dialog, or single-instance permission;
- grants each application command individually.

The Linux mock capability remains limited to its three read-only commands and has no wallet
permission. Frontend Tauri access remains centralized in `src/services/coreApi.ts`.

## Wallet commands

The atomic inventory is:

- lifecycle: `wallet_get_status`, `wallet_select_recovery_destination`, `wallet_create`,
  `wallet_select_recovery_source`, `wallet_restore`, `wallet_unlock`, and `wallet_lock`;
- transactions: `wallet_prepare_transfer_preview`, `wallet_cancel_transfer_preview`,
  `wallet_confirm_and_submit_transfer`, `wallet_list_activity`, and
  `wallet_refresh_transaction_observation`.

Every wrapper accepts the reviewed whole-invoke `WalletInvokeRequest`, then forwards it to the
private lifecycle or transaction boundary. Wrappers perform no custody, secret parsing, signing,
submission, or response projection. The native dialog plugin remains unpermissioned to React.

## Drift enforcement

`src-tauri/tests/tauri_acl.rs` fails when command attributes, invoke registration, AppManifest,
generated permissions, capability grants, or `coreApi.ts` wrappers diverge. It also enforces the
main-window/Windows restriction, absence of remote or broad plugin grants, twelve-command wallet
count, reviewed activation constants, accepted duplicate-key policy, and the private native custody
boundary.

This is a source candidate, not publication authority. No partial command inventory or
lifecycle-only wallet artifact may be packaged or distributed.

## Plugin and network boundary

Single-instance enforcement remains the first Windows plugin. The exact native dialog plugin is
initialized second for Rust-only recovery selection and has no JavaScript package or WebView
permission. Production `connect-src` remains Tauri IPC only; Vite and loopback development sources
remain confined to `devCsp`.
