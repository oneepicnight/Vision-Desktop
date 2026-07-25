# Repository Boundaries

Vision Desktop is a separate repository from Vision Core. It must not copy Vision Core source, history, database files, harnesses, or consensus tests.

## Vision Core Owns

- consensus
- block validation
- P2P protocol
- mining
- state
- persistence
- snapshots and replay
- node API
- transaction execution

## Vision Desktop Owns

- user interface
- Core process lifecycle
- local configuration
- Core binary verification
- installer
- updater
- support reports
- network diagnostics
- future wallet UI
- future exchange UI
- future game launcher

## Vision Desktop Must Never

- modify Core databases directly
- decide whether blocks are valid
- bypass Core validation
- duplicate consensus logic
- ship private keys in config
- silently expose the Core API publicly
- change consensus version or P2P protocol behavior

## Remote Repository

Preferred GitHub repository name: `Vision-Desktop` under the same intended account/organization as Vision Core. If the remote does not already exist, create it explicitly with the correct owner, then add it locally:

```powershell
git remote add origin https://github.com/oneepicnight/Vision-Desktop.git
git push -u origin main
```

Do not create the remote through an unrelated account or organization.
