# Known Limitations

- Windows-first.
- MSI and NSIS bundles are unsigned local engineering builds; public distribution requires a trusted code-signing certificate and release-signing procedure.
- No automatic updater yet.
- The user-facing Wallet remains read-only. A reviewed private Rust custody, recovery, signing,
  submission, reconciliation, and receipt-refresh foundation exists, but it is unregistered and
  unreachable from production Tauri and React authority.
- The Marketplace is a read-only integration view; no market feed, exchange, land listing, cash order, checkout, settlement, or transaction action is connected.
- No game launcher yet.
- No automatic NAT traversal yet.
- No relay yet.
- Core API uses loopback HTTP.
- Manual internet router forwarding is still required for public seed operation.
- No production custody. All wallet approval constants and the production duplicate-key transport
  proof remain false.
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
- Real Core launch supports only the exact admitted Vision Core v1.0.4 Windows artifact. Missing,
  changed, rebuilt, repackaged, multi-linked, reparse-reachable, wrong-process, wrong-generation,
  wildcard-bound, or wrong-owner inputs fail closed.
- The admitted Core implementation and controlled launch test still require independent review;
  an enabled wallet build, clean-device qualification, signing, distribution, and publication
  remain separate blocked gates.
- Public endpoint redaction is conservative and may require review before sharing reports.
