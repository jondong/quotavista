# Frontend guidelines

Status: Design constraints for the first prototype. A minimal React frontend now exists in `apps/desktop/src` with `npm run lint`, `npm run typecheck`, and `npm run build` scripts (see CONTRIBUTING.md); bilingual catalogs arrive with task P1-08.

- [Structure and IPC](directory-structure.md)
- [Components and accessibility](component-guidelines.md)
- [Asynchronous reads](hook-guidelines.md)
- [State ownership](state-management.md)
- [Types and numeric values](type-safety.md)
- [Bilingual validation](quality-guidelines.md)

Read [Architecture](../../../docs/architecture.md#interface-states) and the active task artifacts before development. Review every visible state in both locales and ensure synthetic data remains unmistakable.
