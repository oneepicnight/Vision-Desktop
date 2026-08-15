# Desktop To Core API

Vision Desktop communicates with the exact admitted Vision Core v1.0.4 artifact through a private
loopback HTTP API. The supervisor permits real launch only after strict whole-manifest and executable
admission, and it verifies the running image plus exact loopback listener ownership before releasing
Core authority.

Supported routes verified by the accepted artifact evidence and pinned runtime contract:

- `GET /status`
- `GET /peers`
- `GET /balance/:address`
- `GET /nonce/:address`
- `GET /transaction/:txid`
- `GET /mining/info`
- `POST /transactions`

`/health`, `/height`, and `/block/last` are not part of the admitted contract and must not be assumed
available.

The admitted contract requires literal `127.0.0.1`, the configured `VISION_HTTP_PORT`, and Windows
TCP owner-PID proof for the exact supervised generation. Wildcard, non-loopback, missing, duplicate,
or wrong-owner listeners fail closed. Controlled compatibility evidence for the isolated artifact
admission implementation has been independently accepted. Wallet exposure still requires review and
qualification of the separate unchanged atomic Desktop candidate.

The node-management surface uses:

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

The unpublished wallet candidate uses the same generation-bound authority for bounded account,
status, canonical-tip, exact-envelope submission, lookup, and receipt-observation operations. It
performs exactly one submission attempt per confirmed user intent, never retries or replaces a
transaction automatically, and preserves authenticated ambiguity or accepted-recording-pending
state across reload and restart. Core receives the exact signed public envelope, never the seed,
private key, password, or recovery credential.

