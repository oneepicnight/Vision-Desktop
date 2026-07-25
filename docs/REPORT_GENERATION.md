# Report Generation

The desktop support package follows the closed-alpha report schema.

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

Redacted by default:

- private keys
- seed phrases
- wallet recovery material
- router credentials
- Wi-Fi passwords
- public advertised host values
- unrelated personal files

Reports are stored under `%LOCALAPPDATA%\Vision\Desktop\reports`.
