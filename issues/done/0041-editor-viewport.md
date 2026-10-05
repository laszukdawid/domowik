# 0041: Make the editor viewport stable and specification-correct

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0020, 0024

## Problem

The Flutter prototype refits content on every paint, making edits move the
camera. Its plan-to-screen Y axis is inverted relative to BEHAVIOR-SPEC 1.2,
wheel zoom uses a different input contract and incorrect anchor offsets,
and pointer conversion depends on mutable global painter state. There is
no explicit fit control, grid, or reliable pan cancellation.

## Scope

Implement the existing normative desktop viewport rules in sections 1.2–1.5:
one explicit invertible camera, stable across edits; explicit fit; anchored
zoom and wheel/keyboard navigation; origin-aligned adaptive grid and rulers;
cancel-safe middle-button/space panning. Consume issue 0024's macOS editor.
Touch arbitration and snapping/selection behavior remain separate work.

## Acceptance criteria

- [x] Camera transforms match the normative coordinate, fit and zoom examples;
      non-finite input is discarded without partial updates.
- [x] Editing and ordinary repainting never silently refit or move the camera;
      opening/new and explicit Fit apply the specified defaults.
- [x] Painter and input share per-editor camera state, without global transform
      storage; resizing preserves the plan point at the canvas centre.
- [x] Wheel, modifier-wheel, keyboard zoom/pan and cancellable desktop drag-pan
      follow section 1.4 and produce no document edits or dirty changes.
- [x] Adaptive grid/rulers follow section 1.5 and draw only visible lines.
- [x] Correctness tests exercise actual editor gestures and camera invariants;
      a separate benchmark measures transform/grid work without timing assertions.
- [x] Full verification passes, including the actual built-app raster probe.

## Closure

- **Implementation/spec commits:** `fix(editor): stabilize viewport navigation`
  (the commit containing this closure).
- **Follow-up issues:** 0033 (whole-editor frame/input/memory budgets).

Delivered 2026-09-05. Six pure viewport tests cover the normative coordinate,
fit, zoom, finite-input, wheel and grid examples. Four additional real-library
editor tests cover camera stability while drawing, navigation's dirty-state
firewall, cancelled middle-button panning and fit/resize behavior. The full
Flutter correctness suite has 21 passing tests; the built-app raster probe
reports 4 walls, 57,788 nontransparent raster pixels and no outstanding buffer.

The isolated CPU benchmark and its limits are recorded in
`docs/evidence/0041-viewport/MEASURED.md`: on this M4 Max host, the initial
grid/ruler display-list baseline at 1200×800 has p50 271 µs / p99 501 µs.
These are JIT CPU numbers, not presented frame times or issue 0033 closure.
The camera work removes per-paint content-bounds scans; geometry traversal and
whole-editor frame budgeting remain separate work. All implementation and
fixtures derive independently from Domowik's normative behavior specification.
