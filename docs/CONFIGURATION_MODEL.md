# Configuration Model

Users do not edit TOML. Vision Desktop stores structured JSON config and renders Core-compatible runtime environment variables internally.

Recommended Windows paths:

- Desktop app config: `%APPDATA%\Vision\Desktop\config.json`
- Node config: `%APPDATA%\Vision\Desktop\nodes\default.json`
- Core data: `%LOCALAPPDATA%\Vision\Core\nodes\default\data`
- Core logs: `%LOCALAPPDATA%\Vision\Core\nodes\default\logs`
- Desktop logs: `%LOCALAPPDATA%\Vision\Desktop\logs`
- Reports: `%LOCALAPPDATA%\Vision\Desktop\reports`
- Updates: `%LOCALAPPDATA%\Vision\Desktop\updates`

Rules:

- API is loopback-only.
- P2P port is stable and non-zero.
- Internet mode requires a reachable advertised host/DNS name.
- RC2 internet mode requires manual router forwarding.
- Mining requires a valid 64-character lowercase hex reward address.
