# Backend validation

Use [acceptance traceability](../../../docs/traceability.md) and the task's own observable criteria. A passing document check or fixture test does not establish a provider integration or native OS behavior.

Once the workspace exists, task P1-01 must establish reproducible formatting, Clippy, test, build, and contract-check commands. Until then, do not report those commands as passing.

Prioritize meaningful tests for overflow, decimal precision, DST boundaries, missing versus zero, transaction rollback, candidate publication, stale generations, crash recovery, and session-only restart. Use injected clocks, synthetic provider identities, temporary application directories, and failure-capable fake stores. Never read a contributor's real credentials in tests.

Native Keychain/Secret Service and window/tray/notification checks require separate evidence with OS version, architecture, Linux session type, exact procedure, and result. Keep G01/G02/G04 open until their stated evidence exists. Record benchmark measurements rather than estimates.
