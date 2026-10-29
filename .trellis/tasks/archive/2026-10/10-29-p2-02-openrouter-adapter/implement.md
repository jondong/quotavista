# P2-02 execution plan

## Entry checks

- [x] P2-00 artifacts on disk.
- [x] Read `crates/core/src/provider.rs` and `crates/providers/src/simulated.rs`.
- [x] Confirm no Management Key in repo or prompts.

## Work sequence

- [x] Skeleton module + fixture stubs + trait wiring.
- [x] Credits mapping and permission failure paths.
- [x] Activity/Analytics stubs or partial implementation per capability notes.
- [x] Offline unit tests; optional `#[ignore]` live test.
- [ ] Local-first review (different provider than implementer); push/PR only after APPROVE.

## Validation and review

- [x] `cargo test -p quotavista-providers` (and workspace checks per repo norms).
- [x] Secret grep on diff.
