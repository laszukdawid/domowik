# 0070: Improve selection, warnings and opening affordances

- **Phase:** 2
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0041, 0043, 0054, 0066, 0068

## Problem

The resize boundary has no cursor cue, the Select / draw control does not
provide a selection-only escape from wall creation, Delete does not remove
selected openings, Fit omits door leaves, and warning dialogs do not identify
their affected plan items.

## Scope

Provide the cursor, selection-only switch, native opening deletion, complete
Fit bounds, and transient, reversible warning highlighting on the canvas and
level list.

## Acceptance criteria

- [x] Hover over either opening end uses a resize cursor, returning to the
      normal cursor away from the boundary.
- [x] Explicitly selecting Select / draw leaves the Walls chain and an empty
      drag creates no wall; clicking existing items still selects them.
- [x] Deleting an opening affects only that opening and is undoable.
- [x] Fit includes derived door leaf tips and other rendered opening glyphs.
- [x] Warning rows can highlight their referenced walls/levels without
      mutating selection or document history; closing the dialog clears it.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
- **Follow-up issues:** 0033 owns frame-time/performance checks.

All highlights are view-local; no new persisted fields or dependencies.
