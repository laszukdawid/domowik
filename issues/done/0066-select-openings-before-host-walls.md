# 0066: Select openings before their host walls

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0012, 0024, 0045

## Problem

Clicking the painted aperture of a door or window selects its host wall,
because native hit testing has no opening candidate. The opening cannot be
selected independently.

## Scope

Give active-level aperture clicks an opening candidate ahead of wall regions,
and show selected openings on the canvas and in the selection panel. Keep
existing repeat-click cycling to the host wall. Opening manipulation and
marquee selection are separate work.

## Acceptance criteria

- [x] A click inside a door/window aperture selects that opening rather than
      its wall; another click can cycle to the wall. A click elsewhere still
      selects the wall, and non-active openings are excluded.
- [x] Clicking an opening does not initiate a wall drag or delete its host wall.
      Selection survives refresh and has visible feedback.
- [x] A native hit test and editor gesture tests exercise the behavior. The
      normative selection rule and performance benchmark are updated.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
- **Follow-up issues:** 0067 implements opening editing; marquee selection
  still needs its own specification. This change affects click ordering only;
  existing files, operation logs and FFI request shapes are unchanged.

The opening-hit benchmark at `grid(10)` (220 walls, 110 openings) measured
~149 µs for index build and ~482 ns per indexed query on the local Linux host
(10 samples, 1-second measurement, host-dependent, not a verification gate).
