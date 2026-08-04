# Wallet Cross-Session Ownership

## Status

The private Windows wallet runtime now enforces one wallet-owning Vision Desktop process for the
current Windows user across console, fast-user-switching, and Remote Desktop sessions. Wallet
commands and permissions remain unregistered.

## Ownership object

The runtime retains a non-inheritable Windows kernel mutex handle for its complete lifetime. The
mutex is used as a named process lease rather than a thread-owned synchronization primitive. Its
name is:

`Global\com.vision.desktop.wallet-runtime.v2.<BLAKE3-user-SID>`

The `Global\` namespace makes the object visible across Windows sessions. The current user's SID
is hashed before it enters the name, so the kernel-object name does not disclose the SID. Different
Windows users receive different names and therefore do not block one another's independent local
profiles.

Creation uses a protected DACL that grants generic-all access only to:

- the current Windows user;
- Local System; and
- built-in administrators.

The handle cannot be inherited by a child process. If an object with the same name already exists,
runtime initialization fails closed with the fixed `wallet_process_lock_unavailable` error. The
existing object is never trusted as wallet authority.

## Lifetime and recovery

The mutex is deliberately created without initial thread ownership. Exclusivity comes from the
continued existence of the process-held named object, avoiding thread-affine `ReleaseMutex`
behavior during Tauri teardown. Normal runtime drop closes the handle. Windows closes the handle
after process crash or forced termination, destroying the named object after its final reference
and allowing a later process to acquire ownership.

This lock is independent of the Tauri single-instance plugin. The plugin retains the normal
same-session duplicate-launch experience; the wallet process lease is the fail-closed custody
boundary across every Windows session.

## Automated evidence

Tests verify:

- the name uses the `Global\` namespace;
- two Windows SIDs produce different names;
- the SID itself is absent from the object name;
- the DACL contains exactly current-user, System, and administrators access entries and excludes
  Everyone and Authenticated Users;
- simultaneous acquisition of the same per-user name fails closed;
- independent names do not collide through the normal runtime test path; and
- a child process can own the lease, blocks the parent, and releases ownership after forced
  termination so the parent can acquire it.

## Remaining validation and threat boundary

The automated cross-process test runs within one Windows session. The `Global\` namespace is the
Windows mechanism that extends the same name across sessions, but release qualification must still
exercise fast user switching, RDP reconnect, concurrent console/RDP launches, logoff, and forced
termination on the supported Windows editions.

A process running as the same user, Local System, or an administrator can still pre-create or hold
the object and deny Wallet startup. That is a fail-closed denial of service, not a path to wallet
authority. A fully compromised same-user process or administrator remains outside any credible
local custody isolation guarantee.
