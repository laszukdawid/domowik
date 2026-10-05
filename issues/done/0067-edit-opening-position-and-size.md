# 0067: Edit opening position and dimensions

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024, 0066

## Problem

Doors and windows are selectable, but the editor cannot move or resize them.
Selecting an opening without any way to edit it leaves placed openings fixed.

## Scope

Drag an opening along its existing host wall with transient feedback. Expose
width, height, sill height and cut depth in an atomic properties dialog.
Core validation remains responsible for host fit, overlap and vertical limits.

## Acceptance criteria

- [x] Dragging a selected opening along its host previews a new position and
      commits one editable, undoable batch without moving the wall; cancellation
      leaves the document untouched.
- [x] Editing dimensions applies all changed fields in one batch; cancelling,
      unchanged values and rejected fits make no change or undo entry.
- [x] Current values appear in the inspector and survive refresh/undo/redo;
      opening identity and host stay stable.
- [x] Tests cover doors and windows using the real Rust bridge and document
      validation. No new dependencies or persisted format fields are added.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
- **Follow-up issues:** 0068 adds cross-host drag using R4.3.5's required
  delete/create boundary. Opening marquee selection remains a separate decision.

Compatibility: the native edit request gains an optional `setOpening` kind,
and the derived (non-persisted) opening payload gains display properties.
Existing requests and document files retain their shapes.
