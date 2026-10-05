# 0043: Preserve gesture intent across navigation and cancellation

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0040, 0041, 0042

## Problem

The editor commits the last move's plan coordinate rather than the release
position. A wheel or keyboard view change during a drag leaves the preview
using stale coordinates. Cancellation clears the drag but does not restore
its pre-gesture selection, and application lifecycle interruption is not
observed. Plan-space drag thresholds turn click jitter into edits at ordinary
zoom levels. A transient zero-sized layout can consume a requested Fit.

## Scope

Implement the existing desktop interaction lifecycle in BEHAVIOR-SPEC 1.4,
R6.5.2–4, R6.5.6–9 and R8.7. This does not claim the full snapping/selection
state machine, wall chaining, touch arbitration or platform performance gate.

## Acceptance criteria

- [x] Release uses its finite cursor position under the current camera,
      including outside the viewport; stale pointer IDs do nothing.
- [x] Mouse/touch click-versus-drag classification uses displacement greater
      than five logical pixels, independent of zoom and duration.
- [x] View navigation reprojects active previews from the latest cursor
      position without committing, and release agrees with the preview.
- [x] Escape, pointer cancellation, application interruption and tool changes
      discard previews and restore pre-gesture selection without a batch.
- [x] A zero-sized layout retains a pending Fit until a usable size arrives.
- [x] Regression tests use the real Rust library, include history/dirty-state
      assertions, and distinguish unsupported touch behavior.
- [x] Full repository verification passes without new dependencies.

## Closure

- **Implementation/spec commits:** `fix(editor): preserve gesture intent across input devices`
  (the commit containing this closure).
- **Follow-up issues:** 0044 supplies continuous/two-touch navigation; 0045
  owns targeting disagreement, 0046 extreme grid bounds, 0033 whole-editor
  performance. Full selection/snapping remains Phase 2 work.

Delivered 2026-09-05. Six regression scenarios fail against the prior committed
editor and pass with these fixes. New tests also cover stale/nonfinite release
semantics. Evidence, provenance, review triage and measurement limitations:
`docs/evidence/0043-0044-input/MEASURED.md`. Full verification is run against
the closure commit, including the actual macOS bundle probes.
