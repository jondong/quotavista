# Phase 2 parent design

## Approach and boundaries

Reuse Phase 1 provider ports, typed errors, refresh coordinator, and secret stores. Live adapters sit behind the same ports with non-synthetic provenance. Capability research produces sanitized fixtures before or beside live calls. Credentials never enter git, CI, Issues, or orchestration prompts.

## Compatibility and rollback

Synthetic demo adapters remain available and clearly marked. Live adapters must fail closed when permission or network is missing — never invent balances. Keep each child on its own branch; revert as a unit if validation fails.

## Deferred work

History pagination/backfill, alerts, retention, and Linux release validation stay in later phases.
