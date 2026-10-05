# 0068: Rehost and directly manipulate openings

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0066, 0067

## Problem

Moving an opening was restricted to its first host. Its boundary and door
leaf had no drag affordances, so widening a window and reversing a door swing
required indirect edits or were impossible in the canvas.

## Scope

Use native hit results to choose another active-level wall on release; move
the opening there as an atomic delete/create with a fresh ID, as required by
R4.3.5. The selection follows that new ID. Give both aperture ends resize
handles that preserve the opposite end and use one `setOpening` edit. Dragging
the visible leaf of a selected door across the host changes `opensToward`.

## Acceptance criteria

- [x] Dragging onto a different wall transfers an opening with its properties
      in one undoable batch, retaining the source if validation refuses.
- [x] Dragging either aperture end changes width and centre with the opposite
      end fixed; core fit/overlap validation remains authoritative.
- [x] Dragging a selected door leaf across its wall changes inward/outward
      swing, with an in-flight visual guide and undo/redo support.
- [x] Mouse tests exercise the native bridge and real editor gestures; no new
      persisted fields or third-party dependencies are introduced.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
- **Follow-up issues:** 0033 owns whole-editor performance measurements.

Compatibility: direct host updates remain forbidden. Moving to another host
deletes the old identity and creates a fresh one in the same batch; undo
restores the former ID. The bridge adds a gesture kind, and `setOpening`
accepts `u` and `opensToward` as optional fields. Existing requests and files
retain their meaning.
