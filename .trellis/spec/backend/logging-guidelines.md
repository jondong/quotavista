# Safe diagnostics

The [credentials and IPC boundary](../../../docs/architecture.md#credentials-and-ipc-boundaries) defines the diagnostic allowlist: dataset, elapsed time, anonymous local connection ID, status code, and safe error category.

Do not record credentials, cookies, headers, proxy authentication, raw requests or responses, prompts, model replies, or identifying account data. Redact at the boundary; frontend messages use stable error codes and localized explanations.

Use only synthetic credentials and identities in public fixtures, tests, screenshots, and CI. Test representative failure paths for accidental secret disclosure before claiming credential isolation. No telemetry service is part of Phase 1.
