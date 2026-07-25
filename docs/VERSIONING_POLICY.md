# Versioning Policy

Vision Desktop and Vision Core version independently.

## Vision Core

Vision Core owns consensus and node-runtime compatibility. Core consensus releases use tags such as `vision-core-consensus-v1.0.3`. Alpha/runtime packages use tags such as `vision-core-alpha-rc2`.

The frozen bundled baseline for this desktop milestone is:

- Consensus tag: `vision-core-consensus-v1.0.3`
- Alpha tag: `vision-core-alpha-rc2`
- Commit: `6a065df8206b50874029a27ee2b54dffae5e3cdd`
- Consensus version: `3`
- P2P protocol version: `4`

## Vision Desktop

Vision Desktop releases are application releases. They may bundle, download, or manage a specific compatible Vision Core binary, but they do not define consensus.

Initial development version:

- `0.1.0-alpha.1-dev`

Future public Desktop tags should use `vision-desktop-vX.Y.Z` or a similar Desktop-only namespace.

## Compatibility Manifest

Every Desktop build must carry a compatibility manifest:

```json
{
  "desktop_version": "0.1.0-alpha.1-dev",
  "supported_core_tags": ["vision-core-alpha-rc2"],
  "bundled_core_commit": "6a065df8206b50874029a27ee2b54dffae5e3cdd",
  "bundled_core_sha256": "41F61A18B48D1FB28604910D27D4AADD8368D35CEF27B4E6EB385ADA0BA02C01",
  "consensus_version": 3,
  "p2p_protocol_version": 4,
  "minimum_data_format_version": 1,
  "maximum_data_format_version": 1
}
```

Desktop must refuse incompatible Core binaries unless the user is in an explicit developer override mode.
