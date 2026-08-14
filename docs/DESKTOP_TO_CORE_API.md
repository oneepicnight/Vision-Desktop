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
Vision-Core source commit `223e2f745ebb5f7eb0d48c88397684b9037767bc` contains a reviewed
loopback-capable wallet API candidate, but no release artifact from that source has completed the
Desktop intake gate. Desktop must not change its manifest, binary, or production authority on the
strength of source review alone.

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

