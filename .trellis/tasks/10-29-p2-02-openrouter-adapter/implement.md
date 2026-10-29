# P2-02 execution plan

## Entry checks

- [ ] P2-00 artifacts on disk.
- [ ] Read `crates/core/src/provider.rs` and `crates/providers/src/simulated.rs`.
- [ ] Confirm no Management Key in repo or prompts.

## Work sequence

- [ ] Skeleton module + fixture stubs + trait wiring.
- [ ] Credits mapping and permission failure paths.
- [ ] Activity/Analytics stubs or partial implementation per capability notes.
- [ ] Offline unit tests; optional `#[ignore]` live test.
- [ ] Local-first review (different provider than implementer); push/PR only after APPROVE.

## Validation and review

- [ ] `cargo test -p quotavista-providers` (and workspace checks per repo norms).
- [ ] Secret grep on diff.
