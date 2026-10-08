# 0088: Support the editor in a web browser

- **Phase:** 2
- **Priority:** High
- **Status:** Open
- **Depends on:** 0085, 0086

## Current disposition — 2026-10-05

Keep open and deferred by owner: web is required, not optional or implemented.

Review a browser core boundary and file/storage/render adapters, preserving
Rust semantics. Native dart:ffi is not a browser implementation. Deliver
actual browser workflows and mandatory web CI tests; scaffolding alone is
insufficient. Frontend promotion and Linux CI do not close this requirement.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

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
