# 0019: Behavior specification for doors and windows

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0001

## Problem

Define `../../docs/BEHAVIOR-SPEC.md` section 4 before opening entities or geometry
are finalized.

## Acceptance criteria

- [x] Coordinate examples define host parameterization independent of wall
      endpoint direction, dimensions, sill/elevation, depth, and side.
- [x] Host reversal, shortening/deletion, wall split, join proximity,
      overlap, out-of-range dimensions, and multiple openings are specified.
- [x] Door swing and visual symbolism are classified as authoritative or
      derived/display data.
- [x] Creation/edit/delete gestures identify atomic operations and undo scope.
- [x] Normative rules have study-log entries and a provenance review.

## Closure

- **Main repository commit:** `a6468f8`
- **Review:** Ox Alpha worker and GPT-5.6 Luna adversarial reviewer agreed the
  issue was complete on 2026-08-23 (Luna round 6 issued PRE-MERGE WORK
  APPROVAL after six review rounds, rev B–G).
