# Desktop editor user guide

This guide describes the experimental editor, not a finished product. Follow
[Getting started](GETTING-STARTED.md) to run it. Keep backups of important
plans and read [compatibility and limitations](COMPATIBILITY.md).

## Your first plan

1. The editor starts with an untitled plan. Choose **Walls**, then drag on
   empty canvas to draw a wall. Draw a closed outline to obtain a derived room.
2. Select a wall or an endpoint to move it. The inspector exposes wall
   thickness, height and centreline length editing.
3. Choose **Door** or **Window**, then click a host wall. Select an opening to
   edit its position, size and mechanism. Its handles support movement,
   resizing and rehosting; door controls depend on the selected mechanism.
4. Use the level controls to switch the active level and adjust its properties.
   Visible and ghosted non-active levels provide context; edits target the
   active level.
5. Save a `.domo` plan. The CLI can read, validate and export the same file.

The sidebar holds plan details and object properties. At narrow window widths,
use **Plan details** to open it as a drawer. Status messages appear below the
canvas; hover for the full message or use **Copy status message**.
Warnings are separate from operation errors; use their item controls to locate
the affected objects. A warning is not automatically a rejected edit.

Room geometry and floor areas are derived from walls. Areas use the inner wall
faces rather than centreline rectangles. Room naming uses a record with a seed
point, not a separately editable room polygon. See the
[CLI room example](../README.md#try-it) and the behavior specification for
association rules when a room is split or opened to the exterior.

## Navigate and select in 2D

The editor's in-app hints detect the platform. Use these primary shortcuts:

| Action | Linux | macOS |
| --- | --- | --- |
| Pan | Scroll, middle-drag, or Space-drag | Scroll, middle-drag, or Space-drag |
| Zoom about the cursor | Ctrl + scroll | Command (⌘) + scroll |
| Toggle an object in the selection | Shift-click or Ctrl-click | Shift-click or Command-click |
| Add walls with a marquee | Shift-drag or Ctrl-drag from empty canvas | Shift-drag or Command-drag from empty canvas |
| Undo | Ctrl+Z | Command+Z |
| Redo | Ctrl+Shift+Z | Command+Shift+Z |
| Cancel a gesture | Escape | Escape |

Click repeatedly at the same spot to cycle overlapping objects. Selection can
contain several objects, but edits and Delete act on the **primary object**,
not the entire group. Removing the primary does not silently promote another
object. The marquee adds intersecting active-level walls; it is not a general
group-edit command.

Drag a wall body to move it, or an endpoint to move its joined corner. Escape
during a gesture restores the pre-gesture state; Escape while idle clears the
selection. Escape also cancels a pan.

**Fit plan** explicitly frames the plan. Editing does not automatically refit
the camera. The **+ / −** controls zoom; arrow keys pan by a grid step rather
than nudging selected objects. Grid display and snapping are separate controls.
Plan +Y points upward. Current dimension fields use millimetres.

## Save, open and leave a plan

**Save** currently opens a destination picker every time; there is no separate
silent Save / Save As workflow. Cancelling the picker makes no save.
Dirty plans are marked beside the filename.

New, Open, window close and application quit ask before leaving unsaved work:

- **Save changes** continues only after a successful save.
- **Discard** permits leaving without saving the current edits.
- **Cancel** keeps the current plan.

Cancelled pickers and failed saves/opens preserve the current session. Pending
file/property actions exclude overlapping actions. Undo/Redo belongs to the
core, but the editor's undo stack is session-scoped; saving a file is not a
promise that reopening restores the same interactive undo stack.

## Import furniture

Open **Assets** in the header. Use **Add directory** for a native folder chooser,
or **Enter path** to attach a filesystem directory manually. Search the library
for models discovered in its subdirectories. The current workflow supports
GLB, OBJ and ZIP containing OBJ; this is not an importer for another floor-plan
application's document or catalog format.

Reusable library locations are remembered on this device. Plan-attached paths
can be edited or replaced. Merely attaching a directory does not embed all its
contents: placing a model embeds the chosen content lazily. Already embedded
models remain available if the source directory is removed. Directory paths
are local machine locations, not portable download links.

Insertion size follows what the decoder can establish:

- GLB geometry uses defined metre units, converted to millimetres, including
  static scene transforms.
- Unitless OBJ/ZIP-OBJ preserves model proportions with a 1 m longest axis;
  that is an estimate, not a recovered physical measurement.
- Missing usable size/proportion information uses a diagnosed 1 m cube
  fallback. Very flat models have a minimum stored height; this does not
  extrude their mesh.

The 2D view displays a cached top-down model preview where decoding succeeds.
A footprint/diagnostic fallback does not mean the source is fully rendered.
Current preview rendering is approximate, not photorealistic.

Only import content you have rights to use. Imported models retain their own
rights; importing them does not license them under Apache-2.0. Sharing a plan
with embedded models also shares their bytes.

## Resize an object

Right-click an imported object and choose **Resize…**, or use the inspector's
Resize action. **Select** is also available in the context menu. During wall
chaining, right-click retains its wall-chain finish behavior.

The dialog has Width, Depth and Height fields in millimetres:

- **Keep proportions** is enabled initially. Changing one dimension adjusts
  the others; disable it to resize axes independently.
- Displayed values are rounded to at most 0.1 mm. Untouched dimensions retain
  their underlying precision.
- Valid edits update the dialog's footprint/height preview immediately, with
  the original size shown for comparison. This is a local dialog preview,
  not a live mutation of the plan or the 3D viewport.
- Invalid input keeps the last valid preview and disables **Apply**.
- **Apply** commits one undoable edit; **Cancel** changes nothing.

Long object names are truncated in the title; hover to see the full name.
Size changes do not rewrite the embedded source model.

## Explore in 3D (Linux experiment)

Use **Choose 2D / 3D view** in the header to select **2D plan**, **3D view** or
**2D + 3D split view**. The current native renderer requires Linux and a working
OpenGL context. It renders walls, openings, room slabs and imported meshes,
with diagnostic boxes where needed. It is not yet a portable or photorealistic
renderer; use 2D on platforms without this backend.

In aerial/orbit mode, drag to orbit, right-drag or Shift-drag to pan, and scroll
to zoom. **Fit** resets the aerial framing. In **Walk through**, drag for mouse
look, use W/A/S/D to move, Q/E to change height and Shift for faster movement.
These are camera operations, not edits to the plan.

## Preferences and local configuration

Open **Preferences** in the header to change **2D / 3D split**:

- **Vertical (left and right)** is the default: 2D left, 3D right.
- **Horizontal (top and bottom)** stacks the views.

Choose **Save** to persist the setting for all plans on this device, or
**Cancel** to leave it unchanged. Preferences are separate from plan history.

The local configuration also stores reusable asset-directory locations:

| Platform | Config file |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/domowik/config.json` when XDG_CONFIG_HOME is absolute; otherwise `$HOME/.config/domowik/config.json` |
| macOS | `$HOME/Library/Application Support/domowik/config.json` |

Corrupt, oversized or unsupported configuration is reported and not silently
overwritten. Back it up before repairing it. The config contains local paths;
inspect it before sharing diagnostics.

## Current limits

There is no general copy/paste or group-edit workflow, autosave UI, recent-file
list, supported web editor, synchronization or collaborative editing. Display
unit preferences and full behavior-spec conformance are not complete. Model
rendering and snapping have documented approximation/conformance limits.
See [Compatibility](COMPATIBILITY.md) and the
[issue index](../issues/README.md) rather than assuming every planned feature
is available.
