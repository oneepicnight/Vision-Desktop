# Known Limitations

- Windows-first.
- MSI and NSIS bundles are unsigned local engineering builds; public distribution requires a trusted code-signing certificate and release-signing procedure.
- No automatic updater yet.
- The Wallet is read-only and does not provide custody, key generation, signing, transaction submission, denomination metadata, or transaction history. Live observation requires a running compatible Core private API.
- The Marketplace is a read-only integration view; no market feed, exchange, land listing, cash order, checkout, settlement, or transaction action is connected.
- No game launcher yet.
- No automatic NAT traversal yet.
- No relay yet.
- Core API uses loopback HTTP.
- Manual internet router forwarding is still required for public seed operation.
- No production custody.
- Test funds only.
- Future wallet custody is supported only on the exact Windows 11 Client build/edition matrix in
  `WALLET_RUNTIME_SECURITY.md`, with one interactive session per Windows account. Windows 10,
  evaluation editions, unlisted Client editions, Server/RDS, Azure Virtual Desktop multi-session,
  IoT, Cloud, unknown/future builds, and concurrent same-account multi-session configurations are
  unsupported and fail closed.
- No automatic restart loop yet.
- Windows MSI and NSIS packaging is implemented. The unsigned MSI passed a local elevated lifecycle, and the unsigned NSIS package passed a local silent per-user install, launch, direct-uninstall, and cleanup cycle.
- The NSIS package creates and removes the expected current-user Windows uninstall registration.
- The branded interactive NSIS install, packaged-app launch, and retained-data uninstall lifecycle passed locally; clean-machine and cross-display-scale presentation remain unqualified.
- Clean-machine, upgrade/downgrade, signing, and public-release qualification remain incomplete.
- Real Core launch remains blocked because the frozen RC2 Core cannot bind its HTTP API to loopback only.
- Public endpoint redaction is conservative and may require review before sharing reports.
