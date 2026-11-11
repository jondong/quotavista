# Design notes

This is a documentation and status-honesty slice, not a new adapter.

OpenRouter Management Keys are administrative credentials (create/edit/delete API keys) per official management-key docs. Credits requires a management key. QuotaVista may only issue the required query calls; that is client discipline, not a vendor read-only mode.

The live-smoke log line "key-budget returned metrics" must be interpreted against the test helper that already requires a key-budget row to carry metrics even when the dataset is `Unsupported`.

Kimi membership quota has no documented official consumer session-token issuance path in the sources reviewed for this task. Update the blocker accordingly; leave scope revision to the product owner.
