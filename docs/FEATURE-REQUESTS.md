# Feature requests and missing basics

Status: **Working notes, not a specification.** This is an original review of
the Phase 2 editor's observable behavior against what a person editing a floor
plan would reasonably expect. Nothing here is normative, and a listed item is
not a commitment until it has a tracker issue with acceptance criteria. Issue
numbers name work that already owns the gap; unnumbered items are unowned.

Source: repository review of `spikes/0024-flutter-rust-bridge/flutter_app` and
its bridge on 2026-10-01. No comparable-application source was consulted.

Historical snapshot: subsequent increments implemented several items below,
including wall properties, asset libraries/model previews, object resizing,
context actions and split preferences. The original observations are retained,
not a current missing-feature checklist. Use the [user guide](USER-GUIDE.md),
[compatibility guide](COMPATIBILITY.md) and [tracker](../issues/README.md) for
current capabilities and outstanding work.

## What already works

Wall drawing and editing, openings with real drag handles (move, resize,
rehost, swing), the opening mechanism set, levels with visibility and
properties, room naming, undo/redo, save/open with unsaved-changes protection,
viewport navigation, snapping toggles, warnings with item highlighting, and a
240+ test suite against the real Rust library.

## Missing basics, in rough priority order

### 1. Walls cannot be given real dimensions

`_createWall` and the chain path hardcode `thicknessMm: 200`
(`main.dart:3111`, `main.dart:3203`); the inspector shows Length and Thickness
as read-only counters. There is no field for wall height, thickness, exact
length, or endpoint coordinates, even though the bridge already carries
`setWall` (`thicknessMm`/`heightMm`) and numeric `moveWall`. Openings got a
size dialog; walls never did. There is also no UI for `reverseWall`, which the
bridge exposes.

Owned by issue 0071 for thickness, height, and centerline length. Curved walls
are excluded from v0.1 by R3.0.1 and owned by the decision issue 0072.

### 2. Save always asks where

`_saveFile` calls the native `savePlan` dialog on every save
(`main.dart:1178`). There is no silent Save versus Save As, no recent-files
list, and no surfaced autosave or crash-recovery path even though the
container core implements recovery behavior.

### 3. The selection idioms people expect do not exist

No select-all, copy/paste, duplicate, multi-delete, or right-click context
menu; arrow keys pan the camera instead of nudging the selection
(`main.dart:1919-1922`). Some of this is deliberate: v0.1 declares no group
commands (BEHAVIOR-SPEC R7.1.1/R7.4), so multi-selection is visual only.
"Move the two walls I selected together" is nonetheless about as basic as it
gets, and it is a specification decision rather than a missing button.

### 4. No live measurement feedback

There is no cursor coordinate readout, no length or angle shown while drawing
or dragging, and no measure tool. Drawing a wall without seeing
"3 200 mm, 90°" is a notable hole for a plan tool.

### 5. The display-unit preference does not exist in the UI

The specification defines mm/cm/m display preferences; the editor is
hardcoded to millimetres everywhere.

### 6. Labels and dimensions are entirely absent

The format defines `label` and `dimension` entities (FORMAT-SPEC §2.3), but
nothing creates, edits, or draws them. Dimension lines are a staple of any
plan tool.

### 7. Assets and underlays cannot be used

`assetInstance` and level `backgroundImages` exist in the core and format,
but the editor cannot place furniture or import/display a tracing image. The
roadmap lists asset placement as Phase 2 work, so the placement gap is known,
but a plan editor that cannot drop a sofa or trace a photo is visibly
incomplete.

### 8. There is no way to get a picture out

The CLI exports SVG and PNG; the GUI has no export, print, or share. "Give me
a PNG of my plan" has no GUI path.

### 9. Smaller gaps

- No zoom-to-selection and no zoom percentage display.
- No room-record delete or restyle affordance; only naming.
- No duplicate-storey action.
- No manual wall split, trim, or extend.
- No alignment guide beyond the snap point during edits.
- Single window; no tabs or multiple open plans.
- No dark mode.
- No keyboard tool shortcuts.

## Already owned by tracker work

These are known gaps with existing owners; they are listed so this document
does not read as a discovery of the tracker:

| Gap | Owner |
|---|---|
| Full snapping/magnetism | 0058 |
| Marquee placeholder participation | 0059 |
| Editor frame-time and input budgets | 0033 |
| Linux and Android editor validation | 0024 |
| Asset-instance placement | Roadmap Phase 2 |

## The pattern behind the list

The bridge and the format are ahead of the UI. `setWall`, `reverseWall`,
`deleteRoom`, styles, labels, dimensions, and assets all exist behind the ABI,
but the editor exposes roughly half of them. The highest-value next increment
is probably not new core work: a wall-properties dialog, a real Save/Save As
split, select-all plus arrow-key nudge, and a live draw readout would turn the
existing engine into something that feels like a finished editor.
