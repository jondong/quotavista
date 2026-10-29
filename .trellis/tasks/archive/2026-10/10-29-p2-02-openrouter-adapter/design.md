# P2-02 design

## Approach

Add `crates/providers/src/openrouter/` (or `openrouter.rs`) implementing the Phase 1 `Provider` port. HTTP client stays inside the providers crate; core remains free of URLs and credentials. Inject transport for tests.

## Contracts

Map Credits JSON to `CurrentMetric` with money/credits units already defined in domain. History pages respect coordinator bounds. Provenance must not be `Synthetic` for live results.

## Safety

Management Key retrieved from native secret store / session memory by higher layers; adapter receives an opaque auth handle or header provider callback — never reads env files from disk ad hoc in library code paths used by tests that could leak. Document required local env name for optional live tests only.
