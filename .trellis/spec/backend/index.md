# Backend guidelines

Status: Design constraints for the first prototype. A Cargo workspace now exists: `crates/core` (serialization-independent domain crate with calendar dependencies, boundary enforced by `scripts/check-core-boundaries.sh`), `crates/providers` (synthetic adapters only), `crates/infrastructure` (scaffold), and `apps/desktop/src-tauri` (composition root). Requirements 0.9 and Architecture 0.4 remain authoritative; extend these examples as implementation lands.

- [Boundaries and contracts](directory-structure.md)
- [Persistence and recovery](database-guidelines.md)
- [Errors and bounded work](error-handling.md)
- [Safe diagnostics](logging-guidelines.md)
- [Validation](quality-guidelines.md)

Before development, read the linked guides and the task's PRD, design, and execution plan. During review, check dependency direction, numeric precision, credential isolation, failure recovery, and the evidence claimed.
