# 0065: Preview wall movement and pan with Shift+wheel

- **Phase:** 2
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0024, 0041, 0043, 0058

## Problem

An existing wall stays visually stationary while being dragged, making the
result of moving a room edge difficult to anticipate. A vertical-only mouse
wheel reports no horizontal delta, so Shift+wheel cannot pan horizontally.

## Scope

Provide a transient visual guide for endpoint and body drags, including their
immediately attached joined walls, and allow a vertical-only wheel to pan
horizontally while Shift is held.

## Acceptance criteria

- [x] Endpoint and body drags display their in-flight snapped location and
      directly attached wall axes before release, without changing the document.
- [x] Cancellation clears the preview; release commits through the existing
      native gesture, including its existing validation and undo behavior.
- [x] Shift+wheel with vertical-only deltas pans horizontally; horizontal
      deltas remain authoritative when present; plain wheel remains vertical.
- [x] Widget tests exercise in-flight and cancelled previews and actual wheel
      events, and the input specification records the fallback rule.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
- **Follow-up issues:** 0033 owns editor frame-time measurements.
