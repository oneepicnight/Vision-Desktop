# Wallet Wiring and Security Boundary

## Verified read-only flow

The Vision Desktop Wallet is a public-account observation surface. Its live lookup path is:

1. The Wallet view becomes active and uses the existing shared five-second Desktop refresh cycle.
2. Desktop loads the persisted node configuration through `get_node_config_snapshot`.
3. The persisted `miner_reward_address` is selected as the lookup address. Unsaved setup-form values are not used.
4. Desktop calls `lookup_explorer_address` through `src/services/coreApi.ts`, the only frontend Tauri boundary.
5. The Tauri command obtains the private loopback API port from the supervised Core process.
6. The Desktop backend requests `/balance/<address>` and `/nonce/<address>` from `127.0.0.1` with a three-second timeout.
7. Balance and nonce strings are returned without floating-point conversion and applied through the existing Desktop event/reducer pipeline.
8. Existing request tokens reject stale refresh completions.

The loopback request paths and exact balance/nonce preservation are covered by a Rust integration-style unit test using a local test listener.

## Correctness rules

- The persisted Desktop node configuration is authoritative for the reward address.
- An unsaved create-node form cannot redirect Wallet lookups.
- If the persisted configuration cannot be loaded, Wallet clears prior account data, reports configuration unavailability, and performs no guessed lookup.
- If Core is stopped or its API is unavailable, Wallet does not claim a live balance.
- Mock data remains visibly identified as mock data.
- A configured reward address is a public identifier and does not prove custody or ownership.

## Security boundary

Wallet does not contain or accept:

- private keys, seeds, mnemonics, or keystores;
- wallet creation, import, export, backup, or recovery material;
- signing or transaction submission;
- automatic clipboard access;
- arbitrary frontend network requests or direct Tauri calls;
- floating-point balance calculations;
- custody or ownership claims.

The account address is encoded as a URL path segment by the Desktop backend. Requests are limited to the supervised Core process's loopback API port.

## Remaining limitations

- Live Wallet observation depends on a running compatible Core private API.
- Real Core launch remains blocked by the separately documented frozen-RC2 loopback-bind limitation.
- Balance denomination and precision metadata are not exposed.
- Transaction and receipt history are not exposed through the current Desktop service boundary.
- The embedded Rust-managed custody model is now selected and documented in `WALLET_SECURITY_ARCHITECTURE.md`, but custody remains unimplemented and disabled until its Core compatibility and vault security gates pass.
