# 0044: Support continuous trackpad and two-touch navigation

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0041, 0043

## Problem

The canvas handles discrete mouse-wheel events only. It does not consume
Flutter's continuous pan/zoom events or arbitrate two-touch navigation, so
the normative simultaneous translate/zoom interaction in section 1.4 is absent.

## Acceptance criteria

- [x] A tested immutable camera primitive composes centroid translation and
      scale, clamps before anchoring, and discards invalid steps whole.
- [x] Continuous trackpad events pan and zoom around the reported centroid;
      event scale is interpreted cumulatively, not multiplied repeatedly.
- [x] A second touch cancels the first touch's edit before navigation starts;
      two-touch gestures emit no operations and cancel restores the camera.
- [x] Stale events and extra touches do not corrupt active navigation state.
- [x] Widget tests cover event sequences, history/dirty invariants and cancel;
      automated event injection is not described as physical-device validation.
- [x] Full repository verification passes without new dependencies.

## Coordination

Claude Opus 5 in the Herdr review pane implements only the pure camera
primitive and its unit tests. Codex owns integration, event arbitration,
review and all verification. Shared worktree; no concurrent native builds.

## Closure

- **Implementation/spec commits:** `fix(editor): preserve gesture intent across input devices`
  (the commit containing this closure).
- **Follow-up issues:** Physical Linux/Android and reference-device interaction
  validation remain issues 0024 and 0033.

Delivered 2026-09-05. Four pure camera tests and five continuous-navigation
widget scenarios cover simultaneous translation/scale, saturated anchoring,
invalid steps, cumulative trackpad scales, the nonzero canvas layout offset,
second-touch editing cancellation, extra touches, release and Escape. The
whole Flutter suite has 39 passing tests; the two-step camera primitive is
benchmarked separately without frame-time claims. Evidence and limitations:
`docs/evidence/0043-0044-input/MEASURED.md`. Full verification is run against
the closure commit, including native bundle probes.
