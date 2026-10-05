# 0088: Support the editor in a web browser

- **Phase:** 2
- **Priority:** High
- **Status:** Open
- **Depends on:** 0085, 0086

## Scope

Web is a required future target, explicitly deferred by the owner on 2026-10-04.
The current native FFI/filesystem application is not a working browser editor.
Choose and review a browser core boundary (for example WASM), file/storage and
rendering adapters without duplicating Rust document/edit semantics. Technology
choices remain open; generating Flutter web scaffolding alone is insufficient.

## Acceptance criteria

- [ ] An ADR documents the browser core, ownership/errors/cancellation,
      persistence, worker/rendering boundaries and dependency/license review.
- [ ] The editor builds and runs in a browser without native-only imports.
- [ ] Create/edit/undo, opening/saving plans, asset import/resize and preferences
      have browser-backed tests and documented portability limitations.
- [ ] 2D/3D rendering and input behavior have explicit browser support boundaries.
- [ ] CI requires a web build and browser workflow tests; setup documentation
      and the web task describe a demonstrated workflow.

## Provenance

Original owner requirement, 2026-10-04. No implementation or external code.
