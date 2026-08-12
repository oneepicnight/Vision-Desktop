# Wallet Support-Package Security

## Status

Vision Desktop support packages now exclude wallet custody material and private operational data by
construction. This hardening does not register a wallet command, grant a WebView permission, change
Vision-Core, or activate custody.

## Closed input boundary

The generator accepts only the typed Desktop node configuration plus the bundled, typed Core
manifest and verified binary hash. It does not accept:

- Core stdout or stderr;
- arbitrary JSON status or peer payloads;
- Desktop reducer or wallet runtime state;
- filesystem paths selected by React;
- a directory to recursively collect;
- wallet vault, recovery, journal, or session data.

Diagnostics may still display bounded local log tails to the operator. Those tails are never read by
the support-package command. The package's `stdout.log` and `stderr.log` entries contain only a fixed
omission notice.

## Exact package allowlist

Exactly ten files are permitted:

- `SUMMARY.md`
- `summary.json`
- `package-version.json`
- `config-redacted.json`
- `status-samples.jsonl`
- `peer-summary.json`
- `binary-hash.txt`
- `stdout.log`
- `stderr.log`
- `file-manifest-sha256.txt`

The generator constructs every file in memory, computes the manifest from those buffers, validates
the complete set, writes the report directory, and then writes the ZIP directly from the same
buffers. It never uses directory walking, so an unrelated file, wallet directory, reparse target, or
same-user file injection cannot be swept into the ZIP by discovery.

## Configuration minimization

The configuration document includes only:

- availability;
- configured peer count;
- whether an advertised endpoint is configured;
- whether mining is enabled;
- whether a mining payout is configured;
- whether data and log directories are configured.

It excludes node names, exact peers, advertised hosts, payout addresses, data/log paths, public
account addresses, operating mode, exact ports, and wallet ownership or activity.

The public Tauri result is also minimized. React receives only the ZIP SHA-256 and fixed assessment;
the native report-directory and ZIP paths never cross IPC. Native filesystem failures are mapped to
one fixed pathless error.

## Fail-closed classification

Before any output is created, classification requires:

- the exact allowlist with no missing, duplicate, or additional files;
- a per-file size ceiling;
- valid UTF-8 for every file;
- valid JSON for every JSON/JSONL member;
- the exact fixed content for both log members;
- an exact uppercase SHA-256 binary-hash shape;
- absence of wallet, vault, recovery, password, seed, mnemonic, DPAPI, device-key, session-token,
  activation-proof, and ciphertext markers.

Failure produces the fixed message `support package content failed security classification` and
occurs before the report directory or ZIP is created.

## Automated evidence

Tests use distinct canaries for private-key, recovery, DPAPI, session, activation, and activity data.
They place those canaries in every excluded configuration field and verify their absence from every
directory and ZIP member. Each allowed file is then contaminated individually and must fail
classification. Additional tests reject unexpected and duplicate files, verify that both logs
contain only the omission notice, enforce the exact minimized configuration key set, and prove the
serialized IPC result and public failures are pathless.

Static Tauri authority tests prevent the support command from calling the log-tail reader and
prevent the report generator from reintroducing directory walking or arbitrary log-tail inputs.

## Remaining review boundary

This correction requires independent re-review before custody commands are registered. Generated
packages contain no wallet material, but they still expose the bundled Core version, source commit,
binary hash, feature booleans, peer count, and generation timestamp. Operators should review
packages before sharing them, and release qualification must include real-package inspection on a
clean Windows account.
