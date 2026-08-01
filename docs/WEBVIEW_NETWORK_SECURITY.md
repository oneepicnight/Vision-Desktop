# WebView Network Security

## Active production connection boundary

Vision Desktop's production WebView `connect-src` permits only Tauri's IPC transports:

- `ipc:`
- `http://ipc.localhost`, Tauri's internal IPC bridge

The production `connect-src` contains no general loopback HTTP, WebSocket, remote API, or arbitrary network source. React cannot contact Vision Core directly. All frontend requests continue through `src/services/coreApi.ts`, the only `@tauri-apps/api` import, and then through the explicitly permissioned Rust commands.

The Rust backend's bounded loopback client is unaffected by WebView CSP. It obtains the supervised Core process's observed API port and performs the existing typed requests from Rust.

## Development boundary

Development uses a separate `devCsp`. It permits:

- the same Tauri IPC transports;
- the local Vite origin;
- Vite hot-reload WebSockets on port `1420`;
- the pre-existing `http://127.0.0.1:*` development allowance.

Development connectivity is never copied into the production CSP.

## Automated enforcement

`src-tauri/tests/webview_security.rs` verifies that:

- production and development policies remain separate and exact;
- production contains no broad loopback or WebSocket source;
- frontend TypeScript contains no direct Fetch, XMLHttpRequest, WebSocket, EventSource, or hard-coded loopback access;
- `src/services/coreApi.ts` remains the only frontend import of the Tauri core API.

Any future frontend network requirement must be reviewed explicitly. It must not expand the production CSP merely to bypass the Rust service and permission boundaries.

## Security limits

CSP does not replace input validation, command permissions, secret-lifetime controls, explicit navigation policy, or protection against malicious Rust code. A WebView compromise could still attempt commands granted to its window, so the main-window ACL and Rust-side fail-closed validation remain required. No remote application page is configured, but top-level navigation is a separate control from `connect-src`.
