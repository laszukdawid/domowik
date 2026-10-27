# Agent guide: drafting a floor plan from photos

For an AI agent (Claude Code or any agent that can run shell commands) asked
to turn photos of a home, plus measurements from its owner, into a Domowik
plan. Everything here goes through the `domowik` command-line tool; never
edit a `.domo` file directly, and never write raw operation batches (`apply`)
for drafting.

## What you can and cannot get from photos

Photos tell you **topology**: which rooms exist, which connect, roughly where
doors and windows sit, and the room order around the home. They do not tell
you **dimensions**. A plan is only as accurate as the measurements the owner
gives you. Ask for them; never present an estimate as a measurement. Typical
widths ("doors are usually 800–900 mm") are draft assumptions, recorded as
such, never measurements.

## Getting the tool

From a checkout: `cargo build --release -p domowik-cli`, then use
`target/release/domowik`. A downloaded release ships the same binary.
`domowik <command> --help` documents every command.

## Conventions

- **Millimetres** everywhere. Plan axes: +X to the right (east), +Y up
  (north); choose the origin at one outer corner.
- **Walls are drawn on their centre line** with a thickness. Owners measure
  between **interior faces**. Convert: centre-line distance = measured clear
  span + half of each bounding wall's thickness. A room measured 4000 mm wide
  between a 200 mm outer wall and a 120 mm partition has centre lines
  4000 + 100 + 60 = 4160 mm apart.
- Walls that meet are joined automatically. A wall ending on another wall's
  middle **splits** that wall in two; this is normal.
- Rooms are **detected**, not drawn: any closed ring of walls is a room.
  Name a room by placing a name at a point inside it (`room add`).
- Defaults: wall thickness 120 mm, wall height from the level (2500 mm),
  door 900 × 2100 mm, window 900 × 1200 mm on a 900 mm sill.
- A door or window is positioned by its **centre**. Owners usually measure
  from an interior corner to the near edge of the frame. Convert to a centre
  POINT on the wall's centre line, then place by that point, which works
  whatever the wall's direction or splits:
  centre = interior face of the corner wall + measured distance + half the
  opening's width, along the wall. Example: the west wall is 200 mm thick
  (interior face at x = 100); the owner measures 700 mm from that face to the
  near jamb of a 900 mm door in the south wall (centre line y = 0):
  centre x = 100 + 700 + 450 = 1250, so `add-opening PLAN 1250,0 door`.
  Ask the owner which reference they measured from (interior face, outside
  corner, frame edge or centre) and record it; never mix them.
- Left and right are seen **walking along a wall from its start to its
  end** (`startMm` → `endMm` in `info --json`). A door's `--hinge
  near-start|near-end` names the hinged end; `--opens-toward left|right` the
  side the leaf swings into. Read both from the photos; when a photo does not
  show it, keep the default and record it as an assumption.
- On a plan with several storeys, a point can lie on walls of more than one
  level and is then refused as ambiguous: use short IDs there, and pass
  `--level NAME` to `room add`, `add-wall`, `check` and `export-png`.

## Naming walls, openings and rooms

Wall and opening **numbers change** after almost every edit, because splits
and removals renumber the plan. Do not carry numbers between commands.
Instead, name things by:

- **a point** `X,Y` on the wall, or inside the door/window: the most robust
  form, because it does not depend on how walls were split. A point exactly
  at a junction touches several walls and is refused: move it into one wall;
- **a short ID**, the 6 hex digits printed by `info`, `check` and every
  command (`W5 #8b72ad` → `8b72ad`). Write it **without `#`** on a command
  line: an unquoted `#` after a space starts a shell comment, silently
  dropping the reference;
- a full UUID from `info --json`.

`info --json` lists every wall (`id`, `shortId`, `startMm`, `endMm`,
`thicknessMm`, `lengthMm`), each opening (`shortId`, `centreMm`,
`centreOffsetMm`, `widthMm`), each room (`boundaryMm`, `displayAreaM2`,
`label`) and each room record (`shortId`, `seedMm`, `name`).

## Workflow

1. **Inventory the photos.** For each photo: which room, which walls and
   openings it shows, what it connects to. Sketch the topology in words: rooms,
   their order, shared walls, doors between them, windows on outer walls.
2. **Ask the owner once**, a single bundled list. Ask only for what the
   photos cannot give:
   - outer dimensions of the home, or each room's clear width and depth
     (wall face to wall face), for every room you cannot derive from others;
   - wall thicknesses if known (outer and interior), otherwise say you will
     assume 200 mm outer and 120 mm interior;
   - ceiling height(s);
   - for each door: which wall, and its distance from a corner; width if
     unusual;
   - for each window: which wall, distance from a corner, width, height,
     sill height;
   - anything ambiguous in the photos (a hidden room, a wall that may not be
     straight).
3. **Write down assumptions** in a notes file beside the plan (for example
   `home.assumptions.md`): every value you did not get from the owner, its
   source, and how confident you are.
4. **Draft** in this order: `new`, outer walls, interior walls, openings by
   point, door orientation, room names. Convert measured interior spans and
   opening positions to centre-line coordinates first (see Conventions).
5. **Check**: `domowik check PLAN --strict --json`. `check` detects
   suspicious states; it does not know what the home looks like. For each
   warning, decide against your photo inventory what is meant:
   - a `suggestion` is a command whose geometric effect was verified (for
     example extending a wall end so it meets the wall it nearly touches).
     Apply it only if that is the design; a gap may be intentional;
   - `candidates` are alternatives where the right edit depends on intent,
     such as which room an unplaced name belongs to. Choose by the photos and
     the owner's description, never because one is nearest;
   - some warnings (`no-rooms`, `wall-area-overlap`) have no command: fix the
     walls by hand;
   - `unresolved-wall-crossing` and `unsplit-wall-contact` appear only in
     plans whose history predates the rule that walls meeting mid-span are
     split and joined. `add-wall` and `set-wall` never produce them. The
     message names the repair: remove the named wall and draw it again.
   Apply one change, run `check` again, repeat. `--strict` passing only means
   no warnings remain; notes (`wall-end-free`, `room-unnamed`) still need a
   look.
6. **Verify the result against what you expect**, not just `check`:
   - the number of rooms and their names match your inventory;
   - each room's interior size (its `boundaryMm` polygon) and area agree
     with the owner's measurements, comparing like reference points;
   - each opening's `centreMm`, `hinge` and `opensToward` match the stated
     position and the photos;
   - rooms that connect in the home have a door between them in the plan.
   Visual plausibility is not verification.
7. **Look at it**: `domowik export-png PLAN review.png --review`, then view
   the image. It labels every wall (`W3 854095 · 4000`: number, short ID,
   length), every opening (`D1 1000`, `Win4 1500`: label and width) and draws
   a 1 m scale bar. Compare it with the photos: room order, door sides,
   windows.
8. **Correct** by point or short ID (`set-wall`, `set-opening`,
   `remove-wall`, `room set`), then go back to step 5.
9. **Hand over**: tell the owner which values are measured and which are
   assumed, and that the plan opens in the Domowik editor. Keep the editor
   closed while you are editing the file: there is one writer at a time.

## Command reference (drafting subset)

### Architectural construction

Use dedicated commands, not raw operation batches, for stairs and construction.
`domowik stair add --help` authors a straight flight from explicit lower/upper
level endpoints, width and riser count. `--cut-upper-floor` explicitly requests
an associated floor opening; it is optional for exterior stairs. Rise comes from
the levels. Guard height defaults to zero unless requested. `stair set` changes
parameters, and `stair remove` retains independently editable floor openings.
`stair set PLAN STAIR --name NAME --start-elevation MM --end-elevation MM`
uses the same fields as the editor's Floor panel: start is the upper end and end
is the lower end. Values are absolute plan elevations; the core stores offsets
from the referenced floors. Either endpoint may differ from its floor, but the
start must remain above the end. The editor's **Stairs** tool draws directly
upper-to-lower and automatically chooses the nearest level below.

`domowik stair from-room PLAN ROOM --lower-level NAME --upper-edge east|west|north|south
--going MM --risers N [--guard-height MM]` converts an explicitly selected,
named, axis-aligned rectangular placeholder room. The upper endpoint stays fixed;
the lower end uses `(N-1)*going`, potentially extending beyond the old footprint.
Shared room walls and doors survive. Review both levels and access clearances.
Do not infer real riser counts or dimensions from a room name or image: record
draft assumptions and ask before changing the measured footprint.

`domowik construction --help` exposes reusable bodies, hosted openings and bare
connections. A bare connection creates no physical stairs or ramp. `info --json`
reports these separately and includes stair rise, going, pitch and direction on
each endpoint level. Room areas remain gross enclosure areas; floor cuts do not
change room topology. Construction-bearing plans require schema `0.2` and cannot
be opened by older builds. Keep an untouched backup.

### Walls, openings and rooms

Solid appearance edits use `domowik color list PLAN` to inspect wall IDs and
each placed model's source material keys. `domowik color wall PLAN WALL --color
'#D8CDB7'` colors both wall sides; `--reset` restores the default. Use `domowik
color asset PLAN INSTANCE --material 'gltf:0=#F2F0E8' --material
'gltf:2=#6B5A48'` for per-instance materials, `--material 'gltf:0=original'` to
reset one, or `--reset` for the whole model. Quote hex colors so the shell does
not treat `#` as a comment. Colors sampled visually from photos are approximations,
not measured finishes. These commands do not change geometry or model bytes.

```sh
domowik new PLAN
domowik add-wall PLAN X1 Y1 X2 Y2 --thickness MM [--height MM] [--level NAME]
domowik set-wall PLAN WALL [--start X,Y] [--end X,Y] [--thickness MM] [--height MM]
domowik remove-wall PLAN WALL
domowik add-opening PLAN WALL door|window [--offset MM | --at FRACTION] \
        [--width MM] [--height MM] [--sill MM] \
        [--hinge near-start|near-end] [--opens-toward left|right] [--swing DEG] \
        [--mechanism hinged|sliding|side-hung|double-swing|lift]
domowik set-opening PLAN OPENING [--offset MM | --at FRACTION] [--width MM] [--height MM] \
        [--sill MM] [--hinge ...] [--opens-toward ...] [--swing DEG] [--mechanism ...]
domowik remove-opening PLAN OPENING
domowik room add PLAN X Y --name NAME [--level NAME]
domowik room set PLAN ROOM [--name NAME] [--seed X,Y]
domowik add-level PLAN NAME --elevation MM      # one level per storey
domowik set-level PLAN NAME --wall-height MM --ceiling-height MM
domowik info PLAN [--json]
domowik check PLAN [--strict] [--json] [--level NAME]
domowik export-png PLAN OUT.png [--review] [--scale PX_PER_MM] [--level NAME]
```

`WALL` and `OPENING` take a point, a short ID, a UUID or a number. When
`add-opening` gets a point, the opening is centred there unless `--offset`
(millimetres from the wall's start) or `--at` says otherwise.

## Worked example

A 9 × 7 m flat: outer walls 200 mm; a partition 5000 mm from the west wall;
a hallway wall 2500 mm from the south wall on the east side. Entrance door
centred 2000 mm along the hallway's south wall; doors from the hallway to the
bedroom and to the living room; one window per room on an outer wall.

```sh
domowik new flat.domo
domowik add-wall flat.domo 0 0 9000 0 --thickness 200
domowik add-wall flat.domo 9000 0 9000 7000 --thickness 200
domowik add-wall flat.domo 9000 7000 0 7000 --thickness 200
domowik add-wall flat.domo 0 7000 0 0 --thickness 200
domowik add-wall flat.domo 5000 0 5000 7000 --thickness 120
domowik add-wall flat.domo 5000 2500 9000 2500 --thickness 120
domowik add-opening flat.domo 7000,0 door --width 1000
domowik add-opening flat.domo 6000,2500 door --width 800
domowik add-opening flat.domo 5000,1250 door --width 800
domowik add-opening flat.domo 0,3500 window --width 1500
domowik add-opening flat.domo 7000,7000 window --width 1200
domowik room add flat.domo 2500 3500 --name "Living room"
domowik room add flat.domo 7000 1250 --name Hallway
domowik room add flat.domo 7000 4750 --name Bedroom
domowik check flat.domo --strict
domowik export-png flat.domo flat-review.png --review
```

Correction, after the owner says the hallway is 2000 mm deep, not 2500 mm:
`domowik set-wall flat.domo 6000,2500 --start 5000,2000 --end 9000,2000`.
The walls joined to it follow. Doors keep their position as a fraction of
their wall, so a door on a resized wall moves: re-read `centreMm` from
`info --json` and reset it with `set-opening ... --offset MM` if needed.

## Things not to do

- Do not carry wall or opening numbers from one command to the next.
- Do not guess dimensions silently; record each assumption.
- Do not use `undo` as a rollback stack: each call compensates the latest
  change, so two calls undo and then redo. Correct forward instead.
- Do not edit while the owner has the plan open in the editor.
- Do not treat `validate` passing as "the plan is right": it checks the file
  format. `check` looks for drafting mistakes; numbers and the review image
  confirm the result.
- Do not apply `check` suggestions or candidates without deciding they match
  the home.
