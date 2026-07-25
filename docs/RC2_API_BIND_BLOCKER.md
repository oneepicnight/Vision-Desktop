# RC2 API Bind Blocker

## Summary

The first Desktop milestone discovered a security blocker in the frozen RC2 Core runtime interface: RC2 builds the HTTP API listen address as `0.0.0.0:<VISION_HTTP_PORT>` and does not expose an environment variable or configuration option for binding the API to `127.0.0.1`.

Source evidence from frozen Core commit `6a065df8206b50874029a27ee2b54dffae5e3cdd`:

- `src/config/settings.rs` constructs `http_addr` from `VISION_HTTP_PORT` as `0.0.0.0:<port>`.
- `src/main.rs` parses `settings.http_addr` and binds Axum to that socket address.
- No `VISION_HTTP_ADDR` or equivalent loopback bind override exists in RC2.

## Impact

Vision Desktop's security baseline requires the administrative Core API to remain private by default. Launching frozen RC2 Core from Desktop would expose the HTTP API on all local interfaces unless the operating system or network blocks it externally. That is not an acceptable default for a production-quality desktop application.

## Decision

The Desktop supervisor currently refuses to launch the frozen RC2 Core binary in real mode. Mock mode and all non-launch planning/UI/backend functionality can continue.

No Vision Core source was modified. No consensus behavior was changed. No protocol version was changed.

## Required Core Runtime Follow-Up

A future Core runtime-only change, with consensus behavior unchanged, should add an explicit HTTP bind setting such as:

- `VISION_HTTP_ADDR=127.0.0.1:<port>`

or equivalent config support. The Desktop can then launch Core safely with API bound to loopback.

This must not change:

- PoW
- VisionX
- difficulty
- target calculation
- fork choice
- transaction execution
- snapshots
- replay
- persistence
- state root
- block validation
- consensus version 3
- P2P protocol version 4
