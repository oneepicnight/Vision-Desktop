# Desktop To Core API

Vision Desktop is designed to communicate with Vision Core through a private loopback HTTP API.
The currently bundled frozen RC2 binary cannot satisfy that bind requirement, so the production
supervisor refuses to launch it in real mode. Route support documented below describes the frozen
RC2 source contract; it does not mean that the bundled binary is safe to launch.

Supported RC2 routes verified from source:

- `GET /status`
- `GET /peers`
- `GET /balance/:address`
- `GET /nonce/:address`
- `GET /transaction/:txid`
- `GET /mining/info`
- `POST /transactions`

`/health`, `/height`, and `/block/last` are not registered in the RC2 API router and must not be assumed available.

Important blocker: frozen RC2 Core binds the API to `0.0.0.0:<VISION_HTTP_PORT>` and has no
loopback-only bind override. Real Core launch remains blocked for the current bundled manifest.
An independently accepted loopback-capable Windows artifact is frozen by the hashes recorded in
`FROZEN_CORE_ARTIFACT_INTEGRATION_DESIGN.md`, but it has not completed Desktop intake or runtime
manifest integration. Desktop must not change its manifest, binary, launch policy, or production
authority until that separately reviewed integration is complete.

The first desktop milestone uses these routes only after an exact accepted artifact is integrated:

- `/status`
- `/peers`
- `/mining/info`

Errors handled:

- Core not running
- API starting
- timeout
- invalid JSON
- recovery state
- mining unavailable
- mining paused
- stale status

The user never needs to open localhost manually.

The current integration state and artifact requirements are recorded in
`WALLET_READINESS_STATUS.md` and `CORE_ARTIFACT_INTAKE_CHECKLIST.md`.

