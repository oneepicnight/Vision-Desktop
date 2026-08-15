# Known Limitations

- Windows-first.
- MSI and NSIS bundles are unsigned local engineering builds; public distribution requires a trusted code-signing certificate and release-signing procedure.
- No automatic updater yet.
- The complete twelve-command wallet candidate exists only on an unpublished review branch. It is
  not approved for distribution or real-funds use until exact-source review, packaged
  qualification, clean-device recovery testing, evidence acceptance, and an explicit publication
  decision are complete.
- The Marketplace is a read-only integration view; no market feed, exchange, land listing, cash order, checkout, settlement, or transaction action is connected.
- No game launcher yet.
- No automatic NAT traversal yet.
- No relay yet.
- Core API uses loopback HTTP.
- Manual internet router forwarding is still required for public seed operation.
- Wallet custody remains unreleased. The candidate sets its reviewed activation and transport gates
  only as part of the all-or-nothing source diff; no partial or lifecycle-only artifact may ship.
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
- The admitted Core implementation and controlled compatibility evidence are accepted. The exact
  enabled Desktop candidate, clean-device qualification, distribution, and publication remain
  separate blocked gates.
- Public endpoint redaction is conservative and may require review before sharing reports.
