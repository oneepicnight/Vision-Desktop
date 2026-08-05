# Report Generation

The Desktop support package uses schema `1.1` and a closed, in-memory content allowlist. Package
generation never walks a log, data, report, or wallet directory, and the ZIP is written directly
from the same classified byte buffers that produce the visible report directory.

The WebView receives only the package SHA-256 and fixed assessment. The native report directory and
ZIP path remain Rust-only, and filesystem failures are translated to one fixed pathless error.

Generated content:

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

Excluded rather than redacted:

- raw Core stdout and stderr content
- private keys
- seed phrases
- wallet vaults, recovery material, activity records, and DPAPI data
- wallet passwords, session tokens, path tokens, and activation authority
- public account addresses, transaction activity, and nonces
- operating mode, exact ports, node names, exact peers, advertised hosts, payout addresses, and filesystem paths
- router credentials
- Wi-Fi passwords
- unrelated personal files

`stdout.log` and `stderr.log` remain in the schema but contain only a fixed omission notice. Status
samples and peer summaries are currently fixed omitted markers. `config-redacted.json` contains
only feature booleans, configured/not-configured booleans, and peer count; it never contains exact
mode, port, identifier, host, address, peer, or path values.

Before any file is written, generation verifies the exact ten-file allowlist, UTF-8/JSON shape,
size limits, fixed log notices, binary-hash format, and forbidden custody markers. Unknown,
duplicate, oversized, malformed, or security-sensitive content aborts generation with a fixed
classification error. Tests place distinct secret canaries in every excluded configuration source,
scan every report and ZIP member, contaminate every allowed file in turn, and verify fail-closed
rejection.

Reports are stored under `%LOCALAPPDATA%\Vision\Desktop\reports`.
