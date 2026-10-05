# 0001: Behavior specification for walls and joins

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** None

## Problem

Define `../../docs/BEHAVIOR-SPEC.md` section 3 through original decisions and
black-box observation. This issue owns wall creation, authoritative wall
parameters, derived wall polygons, joins, editing, crossing/splitting, and
deletion. It does not own levels or openings.

## Acceptance criteria

- [x] Every section 3 TODO has testable rules and provenance.
- [x] Coordinate examples cover isolated, L, unequal-thickness T, X, acute,
      obtuse, reversed, and degenerate walls.
- [x] Crossing, overlap, auto-split, wall reversal, join membership, and
      deletion behavior are explicit.
- [x] Each gesture identifies its atomic operation batch and undo boundary.
- [x] Ambiguities are listed as follow-up decisions rather than hidden behind
      subjective reviewer approval.

## Closure

- **Main repository commit:** `a3f59a4`
- **Review:** Ox Alpha worker and GPT-5.6 Luna adversarial reviewer agreed the
  issue was complete on 2026-08-22.
