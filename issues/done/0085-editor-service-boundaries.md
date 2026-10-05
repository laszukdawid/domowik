# 0085: Separate editor file/session and render-cache responsibilities

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0040, 0051, 0078, 0084

## Acceptance criteria

- [x] Native plan handle ownership and New/Open/Save allocations live outside
  the widget, with failed replacement preserving the current plan.
- [x] File picker and unsaved-change workflows have injectable boundaries;
  duplicate actions, cancellation, exceptions and disposal remain safe.
- [x] Session reset code is shared between New and Open.
- [x] Per-level geometry cache lifecycle/invalidation moves out of the widget,
  preserving reuse, visibility eviction and retry of failed reads.
- [x] Unit and native-backed integration tests cover the new boundaries;
  existing rendering, file, gesture and history regressions continue passing.
- [x] Architecture/provenance and remaining extraction work are documented;
  full repository verification is run without adding dependencies.

## Provenance

Original owner approval of the application-boundary review on 2026-10-04.
Refactoring existing Domowik behavior only; no third-party artifacts consulted.

## Closure — 2026-10-04

Extracted PlanSession, FileDialogs/NativeFileDialogs, FileWorkflowController
and PlanGeometryCache. The widget borrows the session handle, owns UI decisions,
shares one replacement reset and delegates cache lifetime/invalidation.
Seventeen new tests plus existing native-backed lifecycle regressions pass.
All 305 Flutter tests, analysis and the Linux release build pass.

Full repository verification passed all executed checks through tracker
consistency, then printed `=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`,
exit 1 because the workspace has uncommitted changes. macOS execution was
explicitly skipped on Linux. An approval-service timeout was retried successfully;
it was not a verification result.

Remaining work: preview-image resource ownership, other editor FFI queries,
render-layer assembly and the 3D surface/upload controller are not extracted
by this issue. File operations still use the synchronous ABI.

See [service-boundary evidence](../../docs/evidence/0085-editor-service-boundaries.md).
