# 0046: Bound grid painting under extreme finite camera origins

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0041

## Problem

Grid loops increment absolute lattice indices. A finite negative camera
origin around -1e30 at zoom 0.1 produces positive indices beyond int64.
Dart ceil/floor saturate both loop bounds to 9223372036854775807; the first
increment wraps to -9223372036854775808 and still satisfies the upper bound.
Thus the claimed viewport-pixel work bound does not hold for all admitted
finite camera states. More extreme inverse coordinates can overflow too.

Confirmed on Dart 3.11.4 macOS/arm64 using a bounded three-iteration probe,
not by hanging the GUI: ceil(1e31/100) and floor((1e31+12000)/100) both
produce int64 max, and the subsequent two indices remain <= the upper bound.
This is a synthetic-input robustness defect, not claimed normal mouse input.

## Acceptance criteria

- [x] Grid/ruler traversal has an explicit viewport-pixel work bound that
      cannot depend on incrementing saturated absolute integer indices.
- [x] Nonfinite derived bounds and unrepresentable lattice locations cannot
      throw or hang painting; normal-origin alignment and major lines remain.
- [x] Tests cover both signs of huge finite origins and inverse overflow;
      a bounded measurement covers normal and adversarial cases.
- [x] Full repository verification passes without new dependencies.

## Closure

- **Implementation/spec commits:** `fix(editor): bound grid traversal in screen space`
  (the commit containing this closure).
- **Follow-up issues:** None.

Delivered 2026-09-05. Screen-space phase replaces absolute int64 indices;
unrepresentable ruler labels are omitted and unsupported synthetic extents
are refused before unbounded work. Four new correctness tests and normal/
extreme paint measurements are recorded in `docs/evidence/0046-grid/MEASURED.md`.
Full verification is run against the closure commit, including native probes.
