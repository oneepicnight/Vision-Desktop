# Process Lifecycle

Vision Desktop owns the Core child process it starts.

Lifecycle:

1. Verify bundled Core manifest.
2. Verify Core binary SHA-256.
3. Load or create Desktop-managed node config.
4. Allocate a loopback API port when configured as `0`.
5. Use a stable configured P2P port.
6. Start Core as a child process.
7. Redirect stdout/stderr into Desktop-managed logs.
8. Record PID and start time.
9. Poll process/API state.
10. Stop only the owned child process.
11. Wait for exit.
12. Confirm ports close where practical.

No automatic restart loop exists in this milestone.
