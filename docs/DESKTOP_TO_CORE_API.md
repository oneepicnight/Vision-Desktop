# Desktop To Core API

Vision Desktop communicates with Vision Core through the private loopback HTTP API exposed by RC2.

Supported RC2 routes verified from source:

- `GET /status`
- `GET /peers`
- `GET /balance/:address`
- `GET /nonce/:address`
- `GET /transaction/:txid`
- `GET /mining/info`
- `POST /transactions`

`/health`, `/height`, and `/block/last` are not registered in the RC2 API router and must not be assumed available.

Important blocker: frozen RC2 Core binds the API to `0.0.0.0:<VISION_HTTP_PORT>` and has no loopback-only bind override. Real Core launch is blocked until a runtime-only Core setting exists. The first desktop milestone uses these routes once safe private binding is available:

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

