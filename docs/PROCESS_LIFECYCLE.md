# Process Lifecycle

Vision Desktop owns the Core child process it starts.

## Desktop process ownership

On Windows, Vision Desktop initializes the pinned official Tauri single-instance plugin before
managed state, application setup, command registration, and any future plugin. The app identifier
names the operating-system mutex and rejects normal duplicate launches for that installed
application identity.

A duplicate launch:

1. is rejected before it can become a second wallet-capable process;
2. has its arguments and working directory discarded rather than logged or trusted;
3. asks the primary process to show, restore, and focus the existing `main` window; and
4. exits even if the primary window cannot be activated.

Normal exit releases the plugin mutex and receiver window. Windows also releases process-owned
mutex resources when a process terminates unexpectedly, allowing a later launch to become the new
primary instance. This process boundary is complemented by the private wallet runtime's independent
fail-closed mutex; wallet commands and signing authority remain unavailable.

Engineering validation on 2026-08-01 used the production frontend and a debug, no-bundle Tauri
executable. The primary process remained active with a nonzero main-window handle, a duplicate
launch exited with code 0 and left exactly one process, and a fresh launch became the sole primary
after the first process was forcibly terminated. A 12-process simultaneous-launch burst also left
exactly one process. Static tests enforce first-plugin ordering, the absence of dialog and WebView
plugin permissions, and the separation of duplicate launch data from the activation handler.

The reviewed plugin's Windows implementation creates its named mutex immediately before its hidden
receiver window. Although the simultaneous-launch test did not reproduce a failure, source review
shows a narrow startup interval in which a duplicate can observe the mutex before that window is
discoverable. The plugin is therefore a strong Desktop-level duplicate-launch control, but it is
not the sole future custody lock. `WalletRuntimeState` now acquires an independent, fail-closed
per-user `Global\` Windows mutex during application setup, before Core resource setup, and holds
the non-inheritable process lease until teardown. The name is shared across console, fast-user-
switching, and RDP sessions for the same user, while its DACL excludes other ordinary users. No
wallet command can access the runtime yet.

Wallet custody support is limited to standard Windows Client with one interactive session per
Windows account. The global namespace remains defense in depth; it does not make concurrent
same-account Windows Server/RDS or multi-session virtual desktop operation supported. Native
product classification rejects Server, multi-session, IoT, unknown, and future unreviewed SKUs
before wallet custody state is created.

The private runtime also owns a hidden Rust-only Windows notification window. Windows session lock,
suspend/standby, logoff/shutdown, and native listener teardown synchronously lock the wallet session
and revoke operations and recovery-path authorization. Listener registration must succeed during
application setup; otherwise startup fails closed. Unlock and resume do not automatically restore
wallet authority. The listener adds no WebView command, capability, direct network access, or
polling loop.

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
