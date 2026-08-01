# Known Limitations

- Windows-first.
- MSI and NSIS bundles are unsigned local engineering builds; public distribution requires a trusted code-signing certificate and release-signing procedure.
- The current Windows icon is a preliminary 16 x 16 ICO and needs a production multi-resolution icon set.
- No automatic updater yet.
- The Wallet is read-only and does not provide custody, key generation, signing, or transaction submission.
- No exchange yet.
- No game launcher yet.
- No automatic NAT traversal yet.
- No relay yet.
- Core API uses loopback HTTP.
- Manual internet router forwarding is still required for public seed operation.
- No production custody.
- Test funds only.
- No automatic restart loop yet.
- Windows MSI and NSIS packaging is implemented and locally smoke-tested, but the installers have not completed signed public-release qualification.
- Real Core launch remains blocked because the frozen RC2 Core cannot bind its HTTP API to loopback only.
- Public endpoint redaction is conservative and may require review before sharing reports.
