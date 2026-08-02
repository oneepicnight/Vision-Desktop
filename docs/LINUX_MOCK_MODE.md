# Linux Mock Mode

Vision Desktop's Linux AppImage shell supports a least-privilege read-only mock mode.

The Linux main-window capability permits only:

- reading the deterministic mock dashboard snapshot;
- reading the non-secret Desktop-managed node configuration snapshot;
- reading Desktop default path metadata.

Linux mock mode cannot:

- start, stop, or restart Vision Core;
- invoke live Vision Core APIs;
- write node configuration;
- open directories, run network diagnostics, or generate support packages;
- provide wallet custody.
