# Behavior Specification

Status: **Sections 1–9 are normative. Section 10 defines normative experimental-
editor behavior where marked and retains explicitly identified open questions;
it is not a complete format contract. Section 11 defines normative
experimental-editor behavior.** This document defines what the editor does
without prescribing source structure or copying another application's
implementation. Draft TODOs are questions, not implementation instructions;
conformance remains bounded by the current implementation and documented limits.

Behavior may come from original product decisions, black-box observation of
public applications, usability testing, and independently published
references. Record necessary external-source and test-data provenance in
[Provenance](PROVENANCE.md), not internal review-session logs.

## Required specification style

Each normative rule must state:

- inputs, units, and preconditions;
- observable result and document operations produced;
- edge and invalid cases;
- at least one coordinate-level example where geometry is involved;
- whether the behavior is authoritative document state, local view state, or
  derived data;
- implications for undo, asset use, and synchronization where applicable.

## 1. Coordinates and viewport

Status: **Normative** (issue 0020).

State classification for everything this section touches:

| State | Class |
|---|---|
| View transform (zoom, pan offsets), grid/ruler visibility, display unit | Local per-device view state |
| Current 2-D viewport camera | Local per-device view state |
| Selection and hover | Local transient state; never persisted |
| Current 3-D camera | Local transient state; no v0.1 persistence |
| Named or saved viewpoints | Not specified in v0.1; deferred |

View-state changes produce no document operations, appear in no operation
log, move no causal frontier, and are never undoable steps.

### 1.1 Plan space and units

- The plan plane is right-handed X/Y in millimetres. Angles measure from 0°
  along +X, increase counter-clockwise, and normalize to the range
  (−180°, +180°].
- Every stored or entered coordinate must be finite. Non-finite values are
  rejected wherever they occur. Coordinate magnitude bounds are set by the
  ratified numerical model (issues 0022 and 0030) and are intentionally not
  fixed here.
- Display-unit preferences (mm/cm/m, and the editor's metric/imperial choice
  in §11.1) affect labels and numeric parsing only;
  they never alter document values.

### 1.2 View transform

The 2-D view uses no rotation. One explicit invertible affine map sends plan
millimetres to logical pixels (device-independent pixels):

```text
│ lx │   │ z   0   ox │   │ x │
│ ly │ = │ 0  −z   oy │ · │ y │
│ 1  │   │ 0   0    1 │   │ 1 │
```

The inverse, valid because z ≠ 0, is x = (lx − ox)/z and y = (oy − ly)/z.

Worked example: with z = 0.1 and offsets (ox, oy) = (512, 384), the plan point
(1000 mm, 2000 mm) maps to logical pixel (612.0, 184.0); applying the inverse
returns the same plan point. Numeric examples in this specification are
compared under the numerical tolerance later ratified by issues 0022 and
0030, not bit-exactly.

### 1.3 Zoom and fit

- Zoom z is expressed in logical pixels per millimetre and clamped to
  [0.0001, 100]. Requests outside the range saturate at the nearer limit.
- Empty-document default view: z = 0.1 with the viewport centre mapped to the
  plan origin.
- Fit command: given viewport width W and height H in logical pixels and
  content bounds B with extents Δx and Δy, the target zoom is
  min(W·(1−2m)/Δx, H·(1−2m)/Δy) with margin m = 0.05, clamped to the zoom
  range and centred on B.
- Edge cases: fitting an empty document yields the empty-document default
  view; a bound with zero extent on either axis uses the default zoom centred
  on the midpoint of the bound; a computed zoom outside the clamp recentres so
  that B stays inside the margin where the clamp allows.
- A fit request or any view configuration carrying non-finite zoom, viewport
  dimensions, or bounds is rejected whole; no part of it applies.
- Fit vector example: W = 800, H = 600 logical pixels, bounds extents
  Δx = 4000 mm, Δy = 3000 mm, m = 0.05 gives z_x = 800·0.9/4000 = 0.18 and
  z_y = 600·0.9/3000 = 0.18, so z_fit = min(0.18, 0.18) = 0.18 centred on B,
  within the zoom clamp.

### 1.4 Input semantics

- Wheel deltas are normalized to signed logical-pixel deltas (dx, dy) and
  must be finite; positive dy means the user scrolls downward. Plain wheel
  pans by (0, dy); shift+wheel pans by (dx, 0) when dx is nonzero,
  otherwise by (dy, 0) so a vertical-only mouse wheel can pan horizontally.
  Panning applies
  ox += dx, oy += dy so content follows the input direction.
- Ctrl/cmd+wheel zooms: the normalized dy maps to zoom factor 1.25^(−dy/100);
  dy = 0 is a no-op; a finite dx is ignored for zooming. The zoom applies
  about the cursor and saturates at the clamp of section 1.3.
- Anchored zoom keeps the plan point beneath the anchor fixed: for an anchor
  at logical point (ax, ay) and new zoom z′, the offsets become
  ox′ = ax − z′·x and oy′ = ay + z′·y for the anchored plan point (x, y).
- Two-touch gestures translate and zoom simultaneously: the view translates
  by the centroid movement and multiplies zoom by the positive finite ratio
  of current to previous finger distance, keeping the plan point beneath the
  centroid invariant under the scale component. A non-finite, zero, or
  negative distance ratio discards that gesture step without state change;
  an accepted ratio saturates at the zoom clamp before anchoring applies.
- Touch cancel ends the gesture and restores the view transform exactly as
  it was at gesture start.
- Palm contact and input from more than two simultaneous touches are
  unsupported and ignored entirely in v0.1.
- Drag pan: starting a middle-button or held-space drag acquires pointer
  capture; each captured move applies its logical-pixel delta to the offsets;
  release ends the capture. Cancelling the drag (Escape or losing capture,
  for example through focus loss) restores the offsets as they were at drag
  start.
- Keyboard: `+` and `−` zoom by a factor of 1.25 about the viewport centre;
  arrow keys pan by one current grid spacing step.
- Any input event carrying non-finite coordinates is discarded whole; no part
  of it applies.
- Numeric fields reject unparsable and non-finite input and leave the stored
  value unchanged. Switching display units while a field is being edited
  reverts that field to the stored value rendered in the new unit.
- Platform-specific arbitration outside the two-touch contract above — such
  as operating-system gesture recognition, platform palm-rejection
  heuristics, and touch/stylus coexistence — is deferred to Phase 2 GUI work.
  The two-touch translate/zoom behavior specified above is itself fully
  normative here, not deferred.
- Selection tolerance is defined with snapping and selection behavior
  (sections 6–7, issue 0003), not here.

### 1.5 Grid and rulers

- Minor grid spacing comes from the series {1, 2, 5} × 10ⁿ millimetres,
  choosing the smallest spacing s such that z·s ≥ 8 logical pixels.
- Grid vector example: at z = 0.1 px/mm the requirement is s ≥ 80 mm, so
  s = 100 mm; at z = 0.05 px/mm it is s ≥ 160 mm, so s = 200 mm.
- A major line occurs every five minor lines. Lines are origin-aligned at
  multiples of s and drawn only across the visible region.
- Rulers read plan coordinates through the current display-unit preference.
  Major labels sit on major lines with a label increment of 5·s, rendered in
  the current display unit; values round half away from zero to the label
  increment.
- Renderer robustness (issue 0046, original decision): grid traversal must not
  depend on incrementing an unbounded absolute world-coordinate index. If
  numerical precision cannot distinguish adjacent lattice coordinates, omit
  their ruler labels rather than show invented precision; the grid may still
  be drawn from its screen-space phase. An unsupported synthetic layout may
  refuse grid rendering before unbounded work or allocation. Such rendering
  refusal changes neither the camera nor the document and is not a zoom limit.

### 1.6 Document edits versus view-state adjustments

Numeric input is classified by its target, never by its modality:

- Editing an authoritative document value (for example a level elevation or
  thickness through a properties field) is a **document edit**: it emits an
  operation batch, is undoable under section 8 semantics, participates in
  synchronization metadata, and changes saved state.
- Adjusting a view control (zoom, pan, grid/unit preference) is **local view
  state**: it emits nothing and is not undoable.
- The same physical gesture may drive either class depending on the active
  tool; classification follows the target of the edit, and implementations
  must not let a view adjustment silently become a document edit or vice
  versa.

### 1.7 Cross-cutting performance scopes

These qualitative scopes are normative for every feature in this
specification. Numeric targets, reference devices, corpora, resolutions,
query mixes, percentile thresholds (frame time p50/p95/p99), input latency,
memory and allocation limits, and culling/grid/fit thresholds belong to
whole-editor performance validation; correctness tests remain separate from benchmarks and must not
use timing-dependent assertions.

- Work is scoped to visible levels (the active level plus visible/ghosted
  levels).
- Spatial work is scoped to the visible region.
- View-state changes cause zero document operations.
- View-state changes cause no document-wide recomputation.

## 2. Levels

Status: **Normative** (issue 0020).

### 2.1 Authoritative level entity

A level is an authoritative document entity with:

- a stable UUID and a display name; names need not be unique;
- `elevationMm`: the height above the plan datum of the finished-floor top
  surface;
- `floorThicknessMm`: the structural floor depth extending downward from the
  elevation;
- `defaultWallHeightMm` and `defaultCeilingHeightMm`;
- `displayOrder` (optional): the level's stored place in the level-list
  presentation, a whole number between 0 and 2^53−1 inclusive. Absence means
  "no stored ordinal"; it serializes as absent, never as `null`.

Geometry convention: walls rise upward from the level elevation, the ceiling
plane sits at elevation plus ceiling height, and the floor slab extends
downward by the floor thickness.

Document defaults: floor thickness 200 mm, wall height 2500 mm, ceiling
height 2500 mm. These three values are authoritative source values held on
the level. Authoritative handoff: a consuming entity copies a default only
when its own normative creation rule explicitly says so; changing a level
default never mutates entities that already exist. For an interactive wall
creation or CLI wall creation with no explicit height override, the interaction
resolves the target level's current `defaultWallHeightMm` at commit time and
carries that explicit value into the section 3 wall command. An explicit
height override wins. Existing walls, including split children, retain their
own resolved height; changing a default never rewrites them. This is the
level-owned resolution rule referenced by R3.0.6, not implicit inheritance in
the wall entity. Normative section 4 openings copy NO defaults by explicit rule
(R4.0.4); no other entity kind infers a copy from these defaults.

Field constraints:

- `floorThicknessMm` ≥ 0;
- `defaultWallHeightMm` > 0;
- `defaultCeilingHeightMm` > 0;
- `0 ≤ displayOrder ≤ 2^53−1` (the JSON/JavaScript-exact integer range; the
  bound keeps every ordinal exactly representable in the browser editor).
  Ordinals need not be distinct or contiguous on any document state: ties
  resolve by stable UUID, so every mixed history remains well-ordered.
- wall height and ceiling height are independent fields; a validation warning
  (not an error) fires when ceiling height is below wall height;
- a validation warning (not an error) fires when the upper level's slab
  intrudes below the immediately preceding level's finished-floor plane:
  `elevation_n - floorThickness_n < elevation_(n−1)`, with levels ordered as
  in section 2.2. It is attributed to the upper level and identifies the
  preceding level. The comparison is strict: merely touching does not warn.
  This tests intrusion below a plane, not positive-volume intersection of
  two slabs; a zero-thickness lower slab does not suppress it. A
  zero-thickness upper slab cannot intrude. Equal elevations with positive
  upper thickness can produce both this and the duplicate-elevation warning.

Warnings are a separate read-only diagnostic channel, never validation
errors or reasons to reject a batch, save or valid-file exit status. Enumerate
levels in ascending elevation/UUID order; within each level report ceiling
below wall height, then duplicate elevation against its immediate predecessor,
then slab intrusion against that predecessor. A run of N equal elevations
therefore emits N−1 duplicate-elevation diagnostics, not every pair. Evaluate
each condition only when its operands satisfy their field constraints;
invalid operands are reported by validation errors instead. For diagnostics
on invalid documents only, levels with invalid elevations follow all valid
elevations, ordered by UUID; they do not interrupt valid-level adjacency.
Their own valid ceiling/wall operands may still produce that advisory. The strict
intrusion relation is over the exact real values of the stored binary64
operands, without a warning-specific epsilon.

Finite-value constraints apply everywhere; exact magnitude bounds follow the
ratified numerical model (issues 0022 and 0030).

Copy scope: no entity copies any default implicitly. Each consumer names the
value it copies in its own normative creation rule — wall height resolution
is specified above and handed explicitly to section 3; openings never copy
(normative section 4, R4.0.4). Rooms are specified separately in normative
section 5 and copy no level defaults. Entity kinds whose sections remain
draft outlines (labels, dimensions, asset instances) define their behavior
when those sections become normative and must not be assumed here.

### 2.2 Document invariants

- A document always contains at least one level.
- A new document contains exactly one level at elevation 0 named "Level 1".
- Duplicate elevations are permitted and produce a validation warning, not an
  error. When two levels share an elevation, render order between them breaks
  ties deterministically by ascending stable UUID.
- The level-list presentation (the editor's level panel and any equivalent
  listing a user rearranges) follows the document-carried order when one is
  stored: levels holding a `displayOrder` ordinal come first, in ascending
  ordinal with the stable-ID tie-break; levels without one follow in the
  derived order. With nothing stored, the order is derived: ascending
  elevation with the stable-ID tie-break.
- The derived order remains authoritative for everything that is NOT the
  level-list presentation: paint tiers, the 3-D scene, diagnostics and warning
  ordering, deletion's active fallback, Add Storey's next elevation, and
  default (unsuccessful-targeted) level addressing, each of which derives
  from ascending elevation with the stable-ID tie-break regardless of any
  stored ordinal.
- Storing the list is a WHOLE-LIST authoring device: the editor's level-panel
  drag and the CLI `order-levels` command assign ascending ordinals 0..n−1 to
  every level of the document and refuse any list that does not name every
  level exactly once. There is no partial-list authoring command; a document
  arrives at an arbitrary mixed state only because levels are added,
  deleted or duplicated after a reorder. Re-storing the order by editing an
  elevation is FORBIDDEN — changing `elevationMm` is an ordinary document
  edit (geometry, diagnostics and derived order) and never writes ordinals.

### 2.3 Level deletion

Deletion rejections produce actionable errors and never cascade in v0.1:

- deleting the last remaining level is rejected;
- deleting a level that contains any entity is rejected, listing the blocking
  entity kinds and counts;
- deleting a level otherwise referenced by surviving data (for example an
  attached background-image descriptor) is rejected;
- deleting the active, empty, unreferenced level succeeds. The newly active
  level is the remaining level with the lowest elevation; ties break by the
  lowest stable UUID.

Admission evaluates these rules immediately before each effective level
deletion in batch operation order (FORMAT-SPEC B4). Explicit operations may
remove contained entities or detach references earlier in the same batch;
deleting the level never performs those removals implicitly. Deleting the
last level and creating a replacement later in that batch is rejected; create
the replacement first. Rejection leaves the entire batch, document, dirty
state and history unchanged. The same rule applies to local commands,
undo/redo compensation and newly admitted external batches, including held
batches when they become applicable at their canonical positions.
Admission also rejects a nonempty materialized plan becoming empty after the
complete canonical replay: reordering a new batch before accepted history
must not evade the last-level invariant.

These are admission rules, not a silent rewrite of historical replay. Empty
internal builders and replay snapshots are permitted intermediates, but a
materialized plan presented as valid or saved through an editing session must
have at least one level. New-plan entry points create the section 2.2 level
as a clean non-undoable baseline. Existing names, snapshots and logs are not
silently migrated; an old initial level-create batch cannot be compensated
if that would delete the last level. A refused undo retains its stack entry.

### 2.4 Per-device level states

| Aspect | Class |
|---|---|
| Existence, name, elevation, thicknesses, height defaults, background anchor, `displayOrder` | Authoritative document data |
| Active level | Local per-device view state; exactly one per open document |
| Visible/ghosted/hidden flags per non-active level | Local per-device view state |

Total state matrix. Each level on a device is in exactly one of four states:

| State | Rendered | Render emphasis and order | Hit-tested, selectable, snappable, mutable |
|---|---|---|---|
| Active | Yes | Full emphasis, above all other levels | Yes — the only level that is |
| Visible (non-active) | Yes | Normal rendering, below the active level | No |
| Ghosted (non-active) | Yes | Reduced emphasis, below visible levels | No |
| Hidden (non-active) | No | Not rendered at all | No |

Within one tier, order follows ascending elevation with stable-ID tie-break
(section 2.2). Paint tiers never consult a stored `displayOrder`: the stored
order is presentation of the level LIST only (section 2.2), not render order.

Transition table:

| Event | Resulting level states |
|---|---|
| Activate level L through level controls | L becomes active: forced visible, normal emphasis. The formerly active level becomes visible-ghosted. |
| Activate the already-active level | No state change; selection and other level flags are preserved. |
| A new level appears in an accepted batch, including undo/redo restoration or an external batch | The new level starts ghosted. Surviving flags and the active level are preserved; if the active level was removed, apply the deletion fallback below. No previous view flag is restored from history. |
| Create a level using the Add Storey level control | After accepted creation, explicitly activate the newly created level through that control. A refused creation changes no view state. |
| Hide or ghost request targeting the active level | Rejected with no state change; the corresponding controls are disabled for the active level. |
| Set visible/ghosted/hidden on a non-active level | Applied exactly as requested. |
| Delete the active, empty, unreferenced level | Level removed; the remaining level with lowest elevation (ties: lowest stable UUID, section 2.3) becomes active, forced visible and normal. Other levels keep their flags. |
| Delete last/nonempty/referenced level | Rejected per section 2.3; no state change. |
| Any canvas click, drag, or marquee | Never changes any level state, including which level is active. |

Additional rules:

- Exactly one active level exists per open document per device.
- Only the active level is hit-tested, selected, snapped against, or mutated
  on canvas. Non-active levels are visual only regardless of their flag:
  never picked, never marquee-selected, never mutated.
- Level activation happens exclusively through level controls such as the
  level panel.
- No separate editable flag exists in any persisted form; editability is
  derived as "this level is the active level".
- Changing authoritative level fields emits ordinary operation batches subject
  to undo (section 8 semantics). Changing any view-state flag emits nothing.

The appearance and Add Storey rows are original Domowik decisions clarified
under issue 0051, not deductions from the absence of view history. For
example, with active A, visible B and hidden C, adding D via Add Storey makes
D active, A ghosted, B visible and C hidden. Undo removes D and activates the
lowest surviving level (A in this example); redo restores D as ghosted and
does not reactivate it. B and C keep their flags throughout. Activating A
while it is already active is a no-op. Reopening instead applies section 2.6
and discards all these local preferences.

### 2.5 Background images

- Each level may reference background images through asset descriptors whose
  requirement level is optional.
- The image-to-plan anchor is authoritative data: a 2-D similarity transform
  plus a stacking hint. Units and form: translation t in plan millimetres,
  dimensionless uniform scale s > 0 (plan millimetres per image pixel),
  rotation θ in degrees counter-clockwise, mapping an image point q to plan
  coordinates p = t + s·R(θ)·q where R(θ) is the standard rotation matrix
  [[cosθ, −sinθ], [sinθ, cosθ]]. A vector example: t = (0, 0), s = 10,
  θ = 90° maps the image point (100 px, 0 px) to the plan point
  (0 mm, 1000 mm).
- Stacking hint: a non-negative integer ordinal ordering backgrounds on the
  same level; ties break by ascending stable UUID.
- Scale ≤ 0 or non-finite anchor values are rejected at edit time.
- Image visibility and opacity are local per-device view state.
- A missing or corrupt blob renders as a placeholder under section 9.6.

### 2.6 Reopen defaults

Every open applies:

- active level: the device's remembered level for this document identity
  (issue 0117), when the document carries a `documentId` (issue 0116), the
  device has recorded an activation for it, and that level still exists in
  the document. Otherwise, lowest elevation among all levels, ties broken by
  lowest stable UUID. A document without a `documentId` is never keyed, and
  only a new-plan entry point mints identity (section 2.2), so a
  first-opened old plan always uses the fallback;
- all other levels: visible and ghosted;
- view: fit with margin if the document has drawable content, otherwise the
  empty-document default view of section 1.3;
- display unit millimetres; grid and rulers visible.

The remembered level is the document's `documentId` paired with the id of
the level last activated through the level controls of section 2.4 while
that document was open, stored per device alongside the other local
preferences and refreshed on every activation. It is device-local view
state and never observable in the document, its log, any CLI output or any
undo record: activating a level still emits nothing (section 2.4), the
content of the storage is not part of the format, and no CLI command reads
or writes it. Deleting the remembered level leaves a stale, harmless entry
that no longer matches a live level; the next open applies the fallback
rows above, and the deletion-fallback rule of section 2.4 governs the
session, not the stored value. The store is the editor's existing platform
config store and holds at most the most-recently touched limit of
documents' remembered levels; identity of other devices' storage is not
this state's business (no sync), and every other view preference in this
section always resets as stated.

### 2.7 Duplicating a level

A duplicate-level command takes a supplied new stable UUID, new name, and
new elevation. It copies exactly the three default values (floor thickness,
wall height, ceiling height) and the source's `displayOrder` ordinal when the
source holds one. It copies background references and their anchors only when
the user explicitly selects that option. A copied ordinal means the duplicate
ties with its source in the level-list presentation (section 2.2) and, with
the stable-ID tie-break, sits adjacent to it. Contained entities are never
copied. The duplicate's render tiers and every other elevation-derived
behaviour remain derived from its elevation; nothing about duplication
changes any other stored order or the derived order itself.

If the supplied UUID collides with any existing entity identifier, the
command is rejected deterministically as invalid input: it produces an empty
operation batch, no state change, and no undo entry.

## 3. Walls

Status: **Normative**. Rules are identified as `R3.*`. All coordinates,
thicknesses, and heights are millimetres. Fixture outputs are exact; R3.0.8
states the numerical-policy boundary.

### 3.0 Conventions

- **R3.0.1 Straight walls.** A wall axis is a single straight segment.
  Curved walls do not exist in v0.1; conforming validators MUST reject any
  attempt to define one (`unsupported-wall-shape`).
- **R3.0.2 Centered axis.** The wall axis is the segment from `startMm` to
  `endMm`. The wall's occupied rectangle offsets `thicknessMm/2` to each side,
  perpendicular to the axis.
- **R3.0.3 Butt caps.** An endpoint that belongs to no join terminates in a
  butt cap: the occupied rectangle ends exactly at that endpoint's
  cross-section (R3.2.2).
- **R3.0.4 Dimensions and values.** All coordinates and dimensions MUST be
  finite; `thicknessMm` and `heightMm` MUST be strictly positive. Violations
  reject (`nonfinite-value`, `nonpositive-thickness`, `nonpositive-height`,
  `nonfinite-coordinate`).
- **R3.0.5 Zero length.** A wall with `startMm == endMm` exactly is invalid
  (`zero-length-wall`). This specification defines NO minimum length, epsilon,
  tolerance, or detach threshold; near-degenerate classification is deferred
  (R3.0.8).
- **R3.0.6 Explicitly resolved parameters.** `thicknessMm`, `heightMm`, and
  the wall's style reference(s) are authoritative per wall: `thicknessMm` and
  `heightMm` MUST be finite positive. Wall-CREATION commands, and any command
  that SETS `thicknessMm`, `heightMm`, or the style reference(s), MUST carry
  the relevant explicitly resolved values; this specification defines NO
  default sourcing or inheritance. Geometry-only edit commands (endpoint or
  joint drag, body drag, reversal, numeric coordinate edits) MAY reference the
  existing unchanged wall parameters without redundantly restating them.
  Resolution against level or document defaults is an interaction concern
  owned by issue 0020 (levels). Height affects vertical extrusion only: it
  never influences plan contact, splitting, joining, or derived occupied
  geometry, so walls of unequal heights join in plan. Walls on different
  levels never interact: same-XY walls on different levels have no contact,
  split, or join (F24).
- **R3.0.7 Data classes.** Authoritative: wall id, levelId, `startMm`,
  `endMm`, `thicknessMm`, `heightMm`, style references, and join entities
  (membership lists only). Derived: occupied regions, join patches,
  intersections, areas, render meshes — rebuildable, never authoritative
  fields, persistable only in a non-authoritative cache namespace per the
  format specification. View-local: previews, tool/chain state, hover; never
  persisted as document state.
- **R3.0.8 Numerical-policy boundary.** All rules and fixtures are defined in
  exact real arithmetic. Fixtures are SEMANTIC ORACLES: they state the exact
  real-valued result a conforming implementation must satisfy under the
  conformance relation of the future numerical model (issues 0022/0030) —
  representable bounds, precision, quantization, overflow, tolerated error,
  and near-degenerate classification live there. They do NOT mandate that an
  implementation emit bit-identical coordinates.

### 3.1 Creation

- **R3.1.1 Gestures and batch boundaries.** A creation gesture places one or
  more chained segments by click/drag; chaining continues from the last
  accepted endpoint. Pointer-event mapping, chain finish/cancel lifecycle,
  angle constraints, and snapping are input/tool concerns owned by issue 0003
  (sections 6–8); this section fixes only the geometric effect of committed
  input and requires that a finished chain segment commit exactly one batch.
  Each accepted chain segment is exactly ONE atomic batch and one undo
  boundary, including any incidental splits/joins it causes. Previews and
  cancelled gestures emit zero operations.
- **R3.1.2 Contact classification.** On every committed creation or edit, the
  final positions of all affected walls are classified pairwise at the same
  level:

  | Contact class | Effect |
  |---|---|
  | Endpoint–endpoint (shared point) | Auto-join; no split |
  | Endpoint-on-interior (axes non-collinear) | Split the interior wall at the point; one join among the touching endpoint and both children's adjacent endpoints |
  | Proper crossing (interior × interior) | Split EVERY participating wall at the point; one explicit multi-member endpoint join |
  | Contact point already occupied by an existing join | Merge into that join; cardinality grows |
  | Positive-length collinear overlap | Reject whole batch (`collinear-overlap`) |
  | Full axis coincidence | Reject whole batch (`wall-coincidence`) |
  | Split plan producing a zero-length child | Reject whole batch (`zero-length-split-child`) |
  | Occupied rectangles overlap with positive area but NO axis contact (parallel or nonparallel disjoint axes) | No join, no split; semantic warning `wall-area-overlap` after final-state validation; batch commits |

  Precedence is total and ordered: (1) full axis coincidence rejects; (2)
  positive-length collinear overlap rejects; (3) contact classes resolve
  (endpoint-endpoint joins, endpoint-on-interior and proper crossings split
  and join, occupied points merge into existing joins); (4) remaining
  positive-area rectangle overlap without any axis contact warns
  `wall-area-overlap` and commits — covering parallel strips AND nonparallel
  walls whose segments do not meet although their swept bands overlap
  (example: `W1` (0,0)→(2000,0) and `W2` (850,900)→(3250,-900), t=200 each:
  the axes cross only at (2050,0), outside `W1`'s segment, while the bands
  overlap near `W1`'s right cap). At a given contact
  point, a wall whose ENDPOINT lies there is never split, while a wall whose
  INTERIOR contains it is always split (F14b). A collinear endpoint-to-interior
  touch necessarily implies positive-length overlap or coincidence, hence
  rejects; endpoint-on-interior splits therefore only occur for non-collinear
  axes. v0.1 has no unconnected overpasses: same-level crossing walls always
  resolve into shared joins.

  Classification is exact. A contact POINT, however, is identified on the
  topology grid (ED-0103 N-2, ED-0119): a point sharing a 2⁻¹⁰ mm cell with an
  endpoint of a wall IS that endpoint, so that wall is not split there. An
  endpoint on another wall's interior, in the cell of one of that wall's
  endpoints, is endpoint-endpoint contact. A proper crossing in one wall's
  end cell splits only the other wall, at the crossing, and that end joins
  the new ends by cell. A proper crossing in an end cell of each wall is a
  corner. A wall splits at most once per cell, and every split point lies on
  its wall's axis. A constructed crossing is the exact intersection,
  committed as the nearest binary64 (NUMERICAL-MODEL class C1).

  Splitting is closed under its consequences: a wall split by an edit has a
  changed axis, so its contacts are resolved in the same batch, including any
  that rounding a split point exposes. Each wall splits once, at all its
  points, so an opening remaps in one R4.3.3 step.
  Two joins brought into one cell merge into one. A wall whose ends share a
  cell is a point at topology resolution and joins nothing there.
- **R3.1.3 Splitting and identity.** When a wall splits at an ordered set of
  points along its original Start→End axis, it becomes a chain of children.
  The child containing the ORIGINAL start endpoint keeps the original wall id.
  Every other child receives a command-supplied unused entity id, assigned in
  chain order from Start. Created joins receive command-supplied unused ids.
  Children inherit all parent properties (levelId, thicknessMm, heightMm,
  style references).
- **R3.1.4 Join remapping.** Existing endpoint joins remap deterministically:
  a membership on the original Start role stays on the original-id child's
  Start; a membership on the original End role follows the original end
  endpoint into the final child's End. The new join at each split point
  absorbs both flanking children endpoints (and merges with any pre-existing
  join at that point).
- **R3.1.5 Batch model (abstract effects and dependency order).** An
  accepted batch is a SET of effects plus a DEPENDENCY PARTIAL ORDER over
  them: each split child depends on its parent's split; each join depends on
  the walls and memberships it relates; an update depends on its target
  entity's existence; a deletion depends on the membership removals it
  implies. Join membership is an UNORDERED set. Textual fixture listings show
  one valid topological presentation of this order (creations before the
  updates/deletions that depend on them; children of one parent written in
  chain order for readability); effects not connected by the dependency
  order are independent and their relative presentation is UNOBSERVABLE in
  the projection. Undo applies the inverse effects respecting the REVERSE
  dependency edges. No serialization order, identifier scheme, or tie-breaking
  beyond the stated dependencies is defined here: physical framing, operation
  identity, counters, and encoding are owned by the format specification.
- **R3.1.6 Atomic rejection.** Any rejecting class aborts the entire batch:
  zero operations apply, document state is unchanged, and the gesture reports
  the diagnostic. Undo state does not change for rejected batches.
- **R3.1.7 Warning ordering.** `wall-area-overlap` is emitted after ALL
  validation and resolution succeed, against final positions; it does not
  abort and the batch commits. Warnings are diagnostics attached to the batch
  result, not operations.
- **R3.1.8 Opening-host coordination.** A committed command that SPLITS a wall
  hosting live openings resolves every hosted opening inside the SAME batch by
  the remap-or-reject rule of section 4 (R4.3.3): each opening whose aperture
  lies strictly within exactly one child remaps to that child; otherwise the
  whole batch rejects atomically with `opening-split-ambiguous`. Reversal of a
  hosting wall applies the R4.3.1 opening transform in the same batch and is
  NOT a keep-parameters edit. All other
  host-affecting mutations (shortening or lengthening, endpoint or joint
  drag, body drag, numeric field edits) keep stored opening parameters
  and MUST satisfy R4.4 validation at final positions, rejecting atomically
  otherwise. The former total rejection gate `opening-host-remap-undefined` is
  retired now that section 4 is normative.

#### Wall-contact admission and existing documents

The refusing axis classes in R3.1.2 apply to newly admitted local commands,
undo/redo/compensation and newly applicable external batches, including held
batches when their prerequisites arrive.

So do two UNRESOLVED classes (ED-0119), ranked after them:

- `unresolved-wall-crossing`: two axes cross at interior points of both,
  unsplit;
- `unsplit-wall-contact`: an endpoint lies on another wall's interior,
  unsplit.

A gesture never produces either, because the builder splits. A raw, external
or history batch that would produce one is refused rather than rewritten. The
point-identity rule of R3.1.2 applies, so a crossing in an end cell of each
wall, or an endpoint in the cell of the other wall's endpoint, is not
unresolved. Coincident endpoints without a join are legal, since an explicit
detach (R3.3.7) produces them. Each effective batch is judged at
its final positions, not between its ordered operations: a temporary overlap
which that same batch removes is not a refusal. Stale or malformed batches
discarded under FORMAT-SPEC I5 retain their existing discard behavior.

Stored history replays unchanged. A legacy coincidence, collinear overlap or
unresolved contact does not prevent unrelated edits, a reversal, or changes
to height, thickness or metadata. For this compatibility rule an affected axis is one newly present,
on a different level, or changed as an undirected segment. Every affected final
axis must be free of refusing contacts with ALL same-level walls, not merely
its previous contact partners. Deleting a wall or atomically moving it clear
can repair old overlap. Undoing that repair is a new admission and refuses if
it would recreate the overlap; the failed undo leaves both stacks unchanged.
This explicit legacy-repair exception qualifies R3.4.7, not ordinary undo of
an accepted edit between conforming states.

Admission also checks the complete candidate projection against the current
projection. Canonical reordering can change whether historical batches take
effect, so only checking entity ids written by the incoming batch is
insufficient. No old batch is rewritten, discarded for a new contact rule or
silently repaired. These are admission checks, not an alteration to the file
schema or historical replay. Thickness exemption concerns AXIS refusals only;
occupied-area warnings must reflect thickness changes and final geometry.

### 3.2 Derived occupied geometry

- **R3.2.1 Definition.** A wall cluster's occupied region is the union of:
  (a) each wall's centered butt-capped rectangle; and (b) at each join, one
  patch defined in R3.2.3. The region may be disconnected (a multipolygon),
  e.g., separate wall clusters.
- **R3.2.2 Cross-section.** At a member endpoint of a join, the wall
  contributes its cross-section: the CLOSED segment of length equal to that
  wall's `thicknessMm`, centered on the endpoint, perpendicular to that
  wall's own axis.
- **R3.2.3 Join patch.** The patch is the convex hull of all member
  cross-sections. The hull is two-dimensional EXCEPT when all contributed
  cross-section points are collinear (e.g., collinear continuation, F3), in
  which case it degenerates to a lower-dimensional closed set and is unioned
  as a closed set. Patches need no miter construction and carry no
  miter-ratio constant.
- **R3.2.4 Canonical output normalization.** Derived occupied regions are
  canonically normalized as follows. Each ring is reduced to a simple cycle:
  duplicate vertices removed, collinear vertices eliminated, and winding
  fixed (exterior rings counter-clockwise, hole rings clockwise). Rings are
  OPEN: the closing vertex is not repeated. After cleanup, each simple ring
  is ROTATED to begin at its lexicographically LEAST vertex. Holes of one
  component are sorted by their canonical vertex-tuple sequences.
  Disconnected components are sorted by their canonical outer-ring tuple,
  then by their hole tuples in that order. Tuple comparison is standard
  lexicographic comparison over exact `(x, y)` pairs (x first, then y); a
  ring tuple is the lexicographic comparison of its vertex sequences starting
  at the least vertex; prefix-shorter sequences compare less. The result is
  invariant under wall reversal. Binding precision, rounding, and
  near-degenerate handling defer to issues 0022/0030; fixture outputs remain
  exact real values interpreted per R3.0.8.
- **R3.2.5 Non-authority.** Occupied geometry is derived data (R3.0.7): it is
  recomputed, never edited directly, and never persisted as authoritative
  document state.

### 3.3 Joins

- **R3.3.1 Join entity.** A persisted join references AT LEAST TWO unique
  wall-endpoint members `(wallId, role ∈ {Start, End})`; membership is an
  UNORDERED set. Joins store no coordinates of their own.
- **R3.3.2 Single membership.** Each wall endpoint belongs to at most one
  join. Multi-way joins (three or more members) are single join entities
  (F14, F14b, F15).
- **R3.3.3 Same-wall rejection.** Both endpoints of one nonzero straight wall
  in one join is invalid and rejects (`degenerate-join-membership`); a wall
  cannot join to itself.
- **R3.3.4 Coordinate agreement.** Wall endpoint coordinates are
  authoritative. All members of a join MUST have equal coordinates under the
  ratified numerical policy; a mutating command that would violate agreement
  either moves the whole node (R3.4.2) or is rejected.
- **R3.3.5 Merge rule.** A new contact landing on an already-occupied join
  point merges into the existing join entity; cardinality grows; no second
  join may exist at one point.
- **R3.3.6 Reversal neutrality.** Reversing a wall swaps `startMm`/`endMm`
  and rewrites affected join member roles in the SAME batch; net membership
  geometry and derived occupied geometry are unchanged (F8).
- **R3.3.7 Explicit detach only.** Detachment is an explicit user command
  removing one member from a join. It is the ONLY way a membership ends short
  of deletion; dragging distance never detaches implicitly (contrast: implicit
  detach-by-distance is forbidden). A join dropping below two members is
  deleted.

### 3.4 Editing, detachment, deletion, performance

- **R3.4.1 Preview versus commit.** All drag previews are transient view
  state (R3.0.7); zero operations occur before release/confirm. Cancelled
  gestures emit nothing.
- **R3.4.2 Joined endpoint and joint drag.** Dragging an endpoint that
  belongs to a join moves the JOIN NODE and ALL member endpoints rigidly
  together (one batch updating every moved wall; joins store no coordinates,
  so no join operation is needed). Numeric edits of a joined endpoint act on
  the join node identically (F23a).
- **R3.4.3 Body drag.** Translating a wall's body translates ONLY that wall
  plus its two attached join nodes; directly incident opposite endpoints move
  with their nodes so immediate neighbors stretch or rotate, but propagation
  does NOT recurse through those neighbors' opposite joins (F17).
- **R3.4.4 Incidental resolution.** Every committed edit reclassifies final
  positions per R3.1.2; incidental splits/joins/warnings join the same single
  batch as the edit.
- **R3.4.5 Deletion effects.** Deleting a wall removes its memberships;
  joins dropping below two members are deleted. Survivors are NEVER
  auto-merged (F19).
- **R3.4.6 Deletion and edit gates.** Deleting a wall hosting live openings
  CASCADES: the same batch deletes every hosted opening per R4.3.4 (F20);
  there is no separate rejection gate for hosted walls. Every other
  host-affecting mutation follows R3.1.8 and section 4 validation (R4.4).
  Wall changes and deletions NEVER gate on room metadata: room
  reconciliation is fully specified and purely derived (section 5), so such
  commands commit and emit zero room operations (R5.2.5). Rejected gates
  leave state unchanged.
- **R3.4.7 Undo equivalence.** Each accepted batch is one undo boundary; undo
  restores a projection equivalent in entity and field values to the prior
  state, not byte-identical history. Redo semantics, typing coalescing, and
  session persistence are owned by section 8.
- **R3.4.8 Performance locality.** Recomputation after any accepted batch
  MUST be confined to the union of the pre-edit and post-edit join-connected
  components of the touched walls, together with the faces reached through
  their transitive component closure under section 5: DISTINCT
  join-connected components are linked by spatial containment or by
  positive-area intersection/overlap of their occupied regions, evaluated
  over both states (R5.1.9). Whole-level
  scope is allowed only when that proven closure reaches every face of the
  level; unjustified whole-level or document-wide invalidation is forbidden.
  Named benchmark corpora, percentiles, memory bounds, and thresholds are
  owned by tracker issue 0032; none are specified here.

### 3.5 Normative fixtures

All coordinates millimetres, exact real values (R3.0.8): fixtures are
semantic oracles for the future numerical-model conformance relation, NOT
mandated implementation output. Unless a fixture concerns styles, every wall
uses the single fixture style token `S0`; every CREATED wall names its style
explicitly - independent creations never inherit it. `supplied:N` marks fresh command-supplied
unused identifiers; ordering language is abstract (R3.1.5). Outlines are
canonical CCW exterior rings of the occupied region (R3.2.4).

#### F1 — Isolated wall
- Before: empty document, level L1.
- Command: create wall `W1`: (0,0)→(4000,0), thickness 200, height 2500, style S0.
- Effects: `[create Wall W1 {levelId:L1, start:(0,0), end:(4000,0), t:200, h:2500, style:S0}]`.
- After: one wall, no joins. Outline: `(0,-100) (4000,-100) (4000,100) (0,100)`.
- Undo: restores the empty projection.

#### F2 — L join (endpoint–endpoint)
- Before: F1 state (`W1` t=200 h=2500).
- Command: create wall `W2`: (4000,0)→(4000,3000), thickness 200,
  height 2500, style S0.
- Classification: endpoint-endpoint at (4000,0): auto-join, no split.
- Effects: `[create Wall W2 supplied:id2 {start:(4000,0), end:(4000,3000),
  t:200, h:2500, style:S0}; create Join J1 {(W1,End),(W2,Start)}]`.
- After: outline `(0,-100) (4000,-100) (4100,0) (4100,3000) (3900,3000) (3900,100) (0,100)`;
  patch hull quad `(3900,0) (4000,-100) (4100,0) (4000,100)` (edges hidden by
  rectangles except where they form the outer boundary).
- Undo: deletes J1 and W2, restoring F1.

#### F3 — Collinear continuation (degenerate patch)
- Before: wall `W1` (0,0)→(4000,0), t=200, h=2500, isolated.
- Command: create wall `W2`: (4000,0)→(8000,0), t=200, h=2500, style S0.
- Classification: endpoint-endpoint → auto-join J1. Cross-sections coincide;
  patch hull degenerates to the closed segment `{(4000,-100),(4000,100)}`,
  unioned as a closed set.
- Effects: `[create Wall W2 supplied:id2 {start:(4000,0), end:(8000,0),
  t:200, h:2500, style:S0}; create Join J1 {(W1,End),(W2,Start)}]`.
- After: outline `(0,-100) (8000,-100) (8000,100) (0,100)` (collinear vertices
  eliminated).
- Undo: deletes J1, W2.

#### F4 — Unequal-thickness T (endpoint-on-interior)
- Before: wall `Wa` (0,0)→(6000,0), t=200, h=2500, isolated.
- Command: create wall `Wb`: (3000,0)→(3000,2500), t=100, h=2100, style S0 (unequal
  height joins in plan, R3.0.6).
- Classification: `Wb.Start` lies on `Wa`'s interior, axes non-collinear →
  split `Wa` at (3000,0). Child containing original start keeps `Wa`
  ((0,0)→(3000,0)); new child `B` = (3000,0)→(6000,0), inheriting t=200,
  h=2500.
- Effects: `[create Wall B supplied:idB {start:(3000,0), end:(6000,0),
  t:200, h:2500, style:S0}; create Wall Wb supplied:idWb {start:(3000,0),
  end:(3000,2500), t:100, h:2100, style:S0}; create Join J1 {(Wa,End),
  (B,Start),(Wb,Start)}; update Wa {end:(3000,0)}]` - all created entities
  carry complete post-resolution field values.
- After: outline `(0,-100) (6000,-100) (6000,100) (3050,100) (3050,2500)
  (2950,2500) (2950,100) (0,100)`; patch hull diamond
  `(3000,-100) (3050,0) (3000,100) (2950,0)` fully covered by the rectangles.
- Undo: inverse batch restores full-length `Wa`; deletes B, Wb, J1.

#### F5 — X crossing (proper crossing during creation)
- Before: wall `W1` (0,0)→(4000,0), t=200, h=2500, isolated.
- Command: create wall `W2`: (2000,-2000)→(2000,2000), t=200, h=2500, style S0.
- Classification: proper crossing at (2000,0), interior × interior → split
  BOTH walls. `W1` → `W1`=(0,0)→(2000,0) + `idB`=(2000,0)→(4000,0);
  `W2` → `W2`=(2000,-2000)→(2000,0) + `idC`=(2000,0)→(2000,2000); created with
  post-resolution fields (R3.1.5).
- Effects: `[create Wall idB {start:(2000,0), end:(4000,0), t:200, h:2500, style:S0};
  create Wall W2 {start:(2000,-2000), end:(2000,0), t:200, h:2500, style:S0};
  create Wall idC {start:(2000,0), end:(2000,2000), t:200, h:2500, style:S0};
  create Join J1 {(W1,End),(idB,Start),(W2,End),(idC,Start)}; update W1
  {end:(2000,0)}]` — all creations carry post-resolution values; only the
  pre-existing `W1` is updated.
- After: outline `(0,-100) (1900,-100) (1900,-2000) (2100,-2000) (2100,-100)
  (4000,-100) (4000,100) (2100,100) (2100,2000) (1900,2000) (1900,100)
  (0,100)`; patch hull diamond `(2000,-100) (2100,0) (2000,100) (1900,0)`
  covered by rectangles.
- Undo: inverse batch restores full `W1`, removes all created entities.

#### F6 — Acute join (36.87°)
- Before: wall `W1` (0,0)→(3000,0), t=200, h=2500, isolated.
- Command: create wall `W2` supplied:id2: (3000,0)→(1400,1200), t=200,
  h=2500, style S0 (axis direction (-4/5, 3/5)).
- Classification: endpoint-endpoint at (3000,0) → auto-join, no split.
- Effects: `[create Wall W2 supplied:id2 {start:(3000,0), end:(1400,1200),
  t:200, h:2500, style:S0}; create Join J1 {(W1,End),(W2,Start)}]`.
- After state: `W1` unchanged; `W2` as commanded; `J1` with two members.
- Patch: convex hull of cross-sections `{(3000,-100),(3000,100)}` ∪
  `{(2940,-80),(3060,80)}` = quad `(3000,-100) (3060,80) (3000,100)
  (2940,-80)`; only edge `(3000,-100)→(3060,80)` is exterior.
- After: outline `(0,-100) (3000,-100) (3060,80) (1460,1280) (1340,1120)
  (2700,100) (0,100)` (the acute corner is bevel-filled by the patch; the two
  rectangles overlap slightly near the joint and the union absorbs them).
- Undo: deletes J1, W2.

#### F7 — Obtuse join (≈106.26°)
- Before: wall `W1` (0,0)→(3000,0), t=200, h=2500, isolated.
- Command: create wall `W2` supplied:id2: (3000,0)→(3700,2400), t=200,
  h=2500, style S0 (axis direction (7/25, 24/25)).
- Classification: endpoint-endpoint at (3000,0) → auto-join, no split.
- Effects: `[create Wall W2 supplied:id2 {start:(3000,0), end:(3700,2400),
  t:200, h:2500, style:S0}; create Join J1 {(W1,End),(W2,Start)}]`.
- After state: `W1` unchanged; `W2` as commanded; `J1` with two members.
- Patch: convex hull of `{(3000,-100),(3000,100)}` ∪ `{(2904,28),(3096,-28)}`
  = quad `(3000,-100) (3096,-28) (3000,100) (2904,28)`; only edge
  `(3000,-100)→(3096,-28)` is exterior.
- After: outline `(0,-100) (3000,-100) (3096,-28) (3796,2372) (3604,2428)
  (2925,100) (0,100)`.
- Undo: deletes J1, W2.

#### F8 — Wall reversal
- Before: F1 state.
- Command: reverse `W1`.
- Effects: `[update W1 {start:(4000,0), end:(0,0)}]`. If reversed wall had
  memberships, the same batch rewrites affected join member roles
  (`(W1,End)` ↔ `(W1,Start)`).
- After: identical canonical outline `(0,-100) (4000,-100) (4000,100)
  (0,100)`; reversal is observationally neutral in plan (R3.2.4, R3.3.6).
- Undo: swaps fields back.

#### F9 — Zero-length create rejects
- Command: create wall (1000,1000)→(1000,1000), t=200, h=2500, style S0.
- Effect: reject `zero-length-wall`; empty batch; state unchanged.

#### F10 — Invalid value rejects
- Commands (each independently): `endMm` non-finite → `nonfinite-coordinate`;
  thickness 0 → `nonpositive-thickness`; height -5 → `nonpositive-height`.
  Each rejects atomically.

#### F11 — Collinear overlap rejects
- Before: wall `W1` (0,0)→(4000,0), t=200, h=2500.
- Command: create wall `W2`: (2000,0)→(6000,0), t=200, h=2500, style S0.
- Effect: positive-length collinear overlap [2000,4000] → reject
  `collinear-overlap`; empty batch.

#### F12 — Coincidence rejects
- Before: F1 state.
- Command: create wall (0,0)→(4000,0), t=150, h=2500, style S0.
- Effect: full axis coincidence regardless of thickness → reject
  `wall-coincidence`; empty batch.

#### F13 — Parallel area overlap warns and commits
- Before: F1 state (`W1` t=200 h=2500).
- Command: create wall `W2`: (0,50)→(4000,50), t=200, h=2500, style S0.
- Classification: parallel axes, no intersection → no contact, no join, no
  split. Final-state check: occupied areas intersect with positive area →
  warning `wall-area-overlap` after validation; batch COMMITS (R3.1.7).
- Effects: `[create Wall W2 supplied:id2 {start:(0,50), end:(4000,50),
  t:200, h:2500, style:S0}]`; the warning is a diagnostic on the batch result, not an
  operation.
- After state: two independent walls, no joins.
- After: union outline `(0,-100) (4000,-100) (4000,150) (0,150)`.
- Undo: deletes W2.

#### F14 — Contact lands on an existing join point (merge)
- Before: F2 state (`W1`, `W2` t=200 h=2500; J1 at (4000,0) with members
  (W1,End),(W2,Start)).
- Command: create wall `W3`: (4000,0)→(5200,900), t=200, h=2500, style S0 (axis
  (4/5,3/5)).
- Classification: `W3.Start` occupies J1's point → merge; J1 grows to three
  members; no new join entity.
- Effects: `[create Wall W3 supplied:id3 {start:(4000,0), end:(5200,900),
  t:200, h:2500, style:S0}; update Join J1 {members += (W3,Start)}]`.
- After state: `W1`, `W2` unchanged; `W3` as commanded; `J1` three members.
- After: patch hull hexagon `(4000,-100) (4060,-80) (4100,0) (4000,100)
  (3940,80) (3900,0)` (only edge `(4000,-100)→(4060,-80)` exterior); outline
  `(0,-100) (4000,-100) (4060,-80) (5260,820) (5140,980) (4100,200)
  (4100,3000) (3900,3000) (3900,100) (0,100)`.
- Undo: removes (W3,Start) from J1 (leaving F2 membership) and deletes W3.

#### F14b — Interior crossing through an existing joint point
- Before: F2 state (`W1`, `W2` t=200 h=2500).
- Command: create wall `W3`: (2800,-1600)→(4600,800), t=200, h=2500, style S0 (axis
  (3/5,4/5)); its INTERIOR passes through (4000,0).
- Classification: precedence (R3.1.2) — `W1.End`/`W2.Start` are endpoints at
  the point (never split); `W3`'s interior contains it → `W3` splits into
  `W3`=(2800,-1600)→(4000,0) + `idD`=(4000,0)→(4600,800); J1 merges to four
  members. No zero-length children arise because endpoint walls are not split.
- Effects: `[create Wall W3 {start:(2800,-1600), end:(4000,0), t:200,
  h:2500, style:S0}; create Wall idD {start:(4000,0), end:(4600,800), t:200, h:2500, style:S0};
  update Join J1 {members = (W1,End),(W2,Start),(W3,End),(idD,Start)}]` —
  per R3.1.5 the newcomer is created ONCE with post-resolution fields (the
  command's requested extent to (4600,800) never exists as state).
- After: patch hull hexagon `(4000,-100) (4080,-60) (4100,0) (4000,100)
  (3920,60) (3900,0)`; outline `(0,-100) (3800,-100) (2720,-1540)
  (2880,-1660) (4680,740) (4520,860) (4100,300) (4100,3000) (3900,3000)
  (3900,100) (0,100)` (the free start cap of `W3` protrudes below `W1`; the
  children's colinear flanking faces merge across the split point, eliminating
  the vertex at `(4080,-60)`).
- Undo: inverse batch restores F2.

#### F15 — Double crossing (chain and deterministic remap)
- Step 1 — Before: wall `W1` (0,0)→(6000,0), t=200, h=2500, style S0.
  Command: create wall `W2`: (2000,-1000)→(2000,1000), t=200, h=2500, style S0.
  Classification: proper crossing at (2000,0). Effects:
  `[create Wall idB {start:(2000,0), end:(6000,0), t:200, h:2500, style:S0};
  create Wall W2 {start:(2000,-1000), end:(2000,0), t:200, h:2500, style:S0};
  create Wall idC {start:(2000,0), end:(2000,1000), t:200, h:2500, style:S0};
  create Join J1 {(W1,End),(idB,Start),(W2,End),(idC,Start)}; update W1
  {end:(2000,0)}]`.
  Step-1 outline: `(0,-100) (1900,-100) (1900,-1000) (2100,-1000)
  (2100,-100) (6000,-100) (6000,100) (2100,100) (2100,1000) (1900,1000)
  (1900,100) (0,100)`.
- Step 2 — Command: create wall `W3`: (4000,-1000)→(4000,1000), t=200,
  h=2500, style S0. It crosses `idB`'s interior at (4000,0): `idB` →
  `idB`[2000→4000] + `idE`[4000→6000]; newcomer splits into
  `W3`[-1000→0] + `idF`[0→1000]. J1's membership `(idB,Start)` is untouched by
  the second split (remap stability, R3.1.4); new J2 at (4000,0).
  Effects: `[create Wall W3 {start:(4000,-1000), end:(4000,0), t:200,
  h:2500, style:S0}; create Wall idE {start:(4000,0), end:(6000,0), t:200, h:2500, style:S0};
  create Wall idF {start:(4000,0), end:(4000,1000), t:200, h:2500, style:S0};
  create Join J2 {(idB,End),(idE,Start),(W3,End),(idF,Start)}; update idB
  {end:(4000,0)}]`.
- Final state: chain `W1`[0→2000], `idB`[2000→4000], `idE`[4000→6000];
  crossing walls `W2`/`idC` joined at (2000,0) and `W3`/`idF` joined at
  (4000,0); J1 {(W1,End),(idB,Start),(W2,End),(idC,Start)}, J2
  {(idB,End),(idE,Start),(W3,End),(idF,Start)}; all thicknesses 200, all
  heights 2500, every created wall names style:S0 explicitly.
- Final outline: `(0,-100) (1900,-100) (1900,-1000) (2100,-1000) (2100,-100)
  (3900,-100) (3900,-1000) (4100,-1000) (4100,-100) (6000,-100) (6000,100)
  (4100,100) (4100,1000) (3900,1000) (3900,100) (2100,100) (2100,1000)
  (1900,1000) (1900,100) (0,100)`.
- Undo boundaries: step 1 and step 2 are independent batches; undoing step 2
  applies inverse effects along reverse dependencies and restores the step-1
  projection exactly (entities and memberships).

#### F16 — Joint drag (rigid co-move)
- Before: F2 state (`W1` t=200 h=2500; `W2` t=200 h=2500; J1 coordinate-free).
- Command: drag join node J1 from (4000,0) to P=(4000,900). Endpoint ROLES
  are unchanged: `W1.End` and `W2.Start` both move to the node.
- Classification at final positions: endpoint-endpoint at P already joined;
  `W1`'s axis meets `W2`'s axis only at P (no crossing); no other contacts;
  no warnings.
- Effects: `[update W1 {end:(4000,900)}; update W2 {start:(4000,900)}]`
  (`W1` becomes (0,0)→(4000,900), axis direction (4000/4100, 900/4100);
  `W2` shortens to (4000,900)→(4000,3000)).
- After state: two walls as above, J1 members unchanged
  {(W1,End),(W2,Start)}.
- Patch hull quad `(3900,900) (164900/41,32900/41) (4100,900)
  (163100/41,40900/41)`.
- After: outline `(-900/41,4000/41) (900/41,-4000/41) (164900/41,32900/41)
  (4100,900) (4100,3000) (3900,3000) (3900,980)` (exact rationals,
  denominator 41; decimal equivalents ≈ −21.951, 97.561, −97.561, 4021.951,
  802.439, 3978.049, 997.561).
- Undo: restores both endpoints to (4000,0).

#### F17 — Body drag (stretch neighbors, no recursion)
- Before: F2 state.
- Command: drag `W1`'s body by (0,+300).
- Effects: `[update W1 {start:(0,300), end:(4000,300)}; update W2
  {start:(4000,300)}]`. Only `W2`'s dragged-node endpoint moves; `W2`'s
  opposite endpoint stays fixed at (4000,3000), stretching `W2`. If `W2`'s
  far end had its own join, that node would NOT move.
- After: outline `(0,200) (4000,200) (4100,300) (4100,3000) (3900,3000)
  (3900,400) (0,400)`.
- Undo: restores both walls.

#### F18 — Explicit detach
- Before: F2 state.
- Command: detach member (W2,Start) from J1.
- Effects: `[delete Join J1]` (drops below two members). Authoritative
  endpoints are unchanged, but the join PATCH disappears from derived
  geometry: the occupied region becomes the union of two plain butt-capped
  rectangles.
- After state: `W1`, `W2` unchanged; no joins; subsequent drags are
  independent.
- After: outline `(0,-100) (4000,-100) (4000,0) (4100,0) (4100,3000)
  (3900,3000) (3900,100) (0,100)` — note the extra vertex `(4000,0)` versus
  the joined F2 outline.
- Undo: recreates J1.

#### F19 — Deletion succeeds
- Before: F2 state.
- Command: delete `W2`.
- Effects: `[delete Wall W2; delete Join J1]` (J1 falls below two members).
  Survivors are never merged; `W1` is untouched.
- Post-delete state: single wall `W1` (0,0)→(4000,0), t=200, h=2500, no
  joins.
- Post-delete outline: `(0,-100) (4000,-100) (4000,100) (0,100)`.
- Undo: recreates equivalent `W2` and J1.

#### F20 — Host deletion cascades (multiple openings, dependency order)
- Before: level L1 exists; wall `Wh` {levelId:L1, start:(0,0), end:(3000,0),
  thicknessMm:200, heightMm:2500, style:S0} hosting two
  live openings: `Ow` supplied:idOw {kind:door, hostWallId:Wh, u:1/2,
  widthMm:800, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0} and `Ow2`
  supplied:idOw2 {kind:window, hostWallId:Wh, u:1/2, widthMm:600,
  heightMm:400, sillMm:2100, depthMm:100, side:Right, hinge:nearEnd,
  opensToward:Left, swingDeg:30, style:S0} (legal: vertical touch at 2100
  with the door; top check 2500 ≤ 2500 passes inclusively). Both openings
  resolve their effective level to L1 through the host (R4.0.2).
- Command: delete `Wh`.
- Effects (abstract order per R3.1.5): `[delete Opening Ow; delete Opening
  Ow2; delete Wall Wh]`. The opening deletions come FIRST and the WALL
  deletion DEPENDS on them; joins dropping below two members are deleted
  per R3.4.5.
- After state: no walls, no openings.
- Undo: applies inverse effects along REVERSE dependencies — `[create Wall
  Wh {levelId:L1, start:(0,0), end:(3000,0), thicknessMm:200,
  heightMm:2500, style:S0}]` FIRST, then `[create Opening Ow supplied:idOw
  {kind:door, hostWallId:Wh, u:1/2, widthMm:800, heightMm:2100, sillMm:0,
  depthMm:200, side:Left, hinge:nearStart, opensToward:Right, swingDeg:90,
  style:S0}; create Opening Ow2 supplied:idOw2 {kind:window,
  hostWallId:Wh, u:1/2, widthMm:600, heightMm:400, sillMm:2100,
  depthMm:100, side:Right, hinge:nearEnd, opensToward:Left, swingDeg:30,
  style:S0}]`.

#### F21 — Host edits keep u and revalidate completely
All variants below independently reset to the SAME complete before-state:
wall `Wh` {levelId:L1, start:(0,0), end:(4000,0), thicknessMm:200,
heightMm:2500, style:S0} hosting opening `Ow` {kind:window, hostWallId:Wh,
u:3/10, widthMm:1200, heightMm:1300, sillMm:900, depthMm:100, side:Left,
hinge:nearStart, opensToward:Right, swingDeg:60, style:S0}. At L = 4000 the
aperture bounds are [uL − w/2, uL + w/2] = [600,1800]; derived outline
`(0,-100) (4000,-100) (4000,100) (1800,100) (1800,0) (600,0) (600,100)
(0,100)`.

- **(a) Accepted numeric edit.** Command: set `Wh.end := (3600,0)`.
  Revalidation (complete-field, R4.3.2): new aperture center u·L = 1080,
  bounds [480,1680]; strict fit holds (0 < 480, 1680 < 3600); top and
  depth unaffected; no overlap introduced.
  Effects: `[update Wh {end:(3600,0)}]`.
  Authoritative post-state: Wh {levelId:L1, start:(0,0), end:(3600,0),
  thicknessMm:200, heightMm:2500, style:S0}; every Ow field unchanged
  (u stays 3/10).
  Derived geometry: outline `(0,-100) (3600,-100) (3600,100) (1680,100)
  (1680,0) (480,0) (480,100) (0,100)`.
  Undo: inverse effects `[update Wh {end:(4000,0)}]` restoring the
  before-state projection exactly.
- **(b) Accepted reversal.** Command: reverse `Wh`.
  Effects: `[update Wh {start:(4000,0), end:(0,0)}; update Ow {u:7/10,
  side:Right, hinge:nearEnd, opensToward:Left}]`.
  Authoritative post-state: Wh {levelId:L1, start:(4000,0), end:(0,0),
  thicknessMm:200, heightMm:2500, style:S0}; Ow {kind:window,
  hostWallId:Wh, u:7/10, widthMm:1200, heightMm:1300, sillMm:900,
  depthMm:100, side:Right, hinge:nearEnd, opensToward:Left, swingDeg:60,
  style:S0}.
  Derived geometry: world facts invariant — center P(7/10) still (1200,0);
  Right face on flipped n̂ = (0,−1) still the world +y face; nearEnd hinge
  at c + 600·â still (600,0); canonical outline IDENTICAL to the
  before-state outline above.
  Undo: inverse effects `[update Wh {start:(0,0), end:(4000,0)}; update Ow
  {u:3/10, side:Left, hinge:nearStart, opensToward:Right}]`.
- **(c) Accepted body drag.** Command: drag `Wh`'s body by (0,+300).
  Effects: `[update Wh {start:(0,300), end:(4000,300)}]` (ONE update
  operation).
  Authoritative post-state: Wh {levelId:L1, start:(0,300), end:(4000,300),
  thicknessMm:200, heightMm:2500, style:S0}; every Ow field unchanged.
  Derived geometry: the outline translates rigidly — every vertex gains
  (0,+300): `(0,200) (4000,200) (4000,400) (1800,400) (1800,300) (600,300)
  (600,400) (0,400)`; all checks pass at final positions.
  Undo: inverse effects `[update Wh {start:(0,0), end:(4000,0)}]`.
- **(d) Rejected numeric edit.** Command: set `Wh.end := (1700,0)`.
  Classification: new aperture center u·L = 510, bounds [−90,1110]:
  lo < 0 crosses the Start endpoint → reject `aperture-not-strict-fit`;
  empty batch; state unchanged; no undo entry.
- Revalidation scope: every accepted batch above revalidates `Ow` against
  ALL final geometric wall fields (`startMm`, `endMm`, `thicknessMm`,
  `heightMm`), not only the edited ones (R4.3.2).

#### F22 — Wall edit inside a room-bearing component commits
- Before: closed rectangle on level L1: walls `W1` (0,0)→(4000,0),
  `W2` (4000,0)→(4000,3000), `W3` (4000,3000)→(0,3000), `W4` (0,3000)→(0,0),
  all t=200 h=2500 S0; corner joins `J1`–`J4`; one room record `R1`
  {levelId:L1, seed:(2000,1500), name:"Living"} associated with the single
  interior face.
- Command: drag `W1`'s body by (0,-500).
- Classification per R3.4.3: `W1` translates; its two attached join nodes
  move; `W2` and `W4` stretch from their nodes with opposite endpoints fixed.
- Effects: `[update W1 {start:(0,-500), end:(4000,-500)}; update W2
  {start:(4000,-500)}; update W4 {end:(0,-500)}]`.
- After state: `W2` = (4000,-500)→(4000,3000); `W4` = (0,-500)→(0,3000);
  joins unchanged (they store no coordinates). ZERO room operations: the
  batch contains no Room effects (R5.2.5).
- Derived room delta (projection only): the interior face becomes the ring
  `(100,-400) (3900,-400) (3900,2900) (100,2900)`; exact area
  3,800 × 3,300 = 12,540,000 mm²; display area 12.54 m² (R5.3.2); `R1`
  remains associated (seed strictly contained); `R1` holds its exclusive
  status `associated` throughout.
- Undo: restores the three wall updates; face and derived room values
  restore without any room operation.

#### F23 — Numeric edits
- (a) Joined endpoint acts on the node. Before F2: command sets
  `W1.end := (4000,-900)`. Because `W1.End` is joined, the NODE moves:
  `[update W1 {end:(4000,-900)}; update W2 {start:(4000,-900)}]` (R3.4.2).
  Post-state: `W1`=(0,0)→(4000,-900), axis (40/41,-9/41), leg 4100;
  `W2`=(4000,-900)→(4000,3000), leg 3900; J1 members unchanged at the node.
  Final-position classification: shared endpoint already joined; axes meet
  only at the node; no other contacts.
  Derived outline: `(-900/41,-4000/41) (163100/41,-40900/41) (4100,-900)
  (4100,3000) (3900,3000) (3900,-775) (900/41,4000/41)` (exact rationals,
  denominator 41).
- (b) Free endpoint landing on an interior, exact identities. Before F18
  aftermath: `W1`=(0,0)→(4000,0) t=200 h=2500; `W2`=(4000,0)→(4000,3000)
  t=200 h=2500; NO joins (J1 was deleted by detachment). Command sets
  `W1.end := (4000,900)` (4000²+900²=4100², keeping the oracle
  exact-rational), landing on `W2`'s interior → classification
  endpoint-on-interior: split `W2` at (4000,900). The child containing
  `W2`'s ORIGINAL start keeps id `W2`, so `W2.start` stays `(4000,0)` and
  `W2.end` moves to the split point; the child containing the original end
  receives supplied:idG. The new join is a NEW entity supplied:idJ2 (the old
  J1 stays deleted; identity is never recycled):
  `[create Wall idG {start:(4000,900), end:(4000,3000), t:200, h:2500, style:S0};
  create Join J2 supplied:idJ2 {(W1,End),(W2,End),(idG,Start)}; update W1
  {end:(4000,900)}; update W2 {end:(4000,900)}]`.
  Post-state: `W1`=(0,0)→(4000,900); `W2`=(4000,0)→(4000,900);
  `idG`=(4000,900)→(4000,3000); J2 three members at (4000,900).
  Derived outline: `(-900/41,4000/41) (900/41,-4000/41) (3900,775)
  (3900,0) (4100,0) (4100,3000) (3900,3000) (3900,980)` (exact rationals,
  denominator 41 where fractional).
- (c) Reject variants: setting `end == start` rejects `zero-length-wall`;
  setting thickness 0 rejects `nonpositive-thickness`; setting height -5
  rejects `nonpositive-height`; all leave state unchanged.
- Undo boundary: one batch per confirmed edit; undo applies inverse effects
  along reverse dependencies (R3.1.5).

#### F24 — Cross-level isolation
- Before: wall `W1` (0,0)→(4000,0), t=200, h=2500, style S0, level L1.
- Command: create wall `W2`: identical XY endpoints (0,0)→(4000,0), t=200,
  h=2700, style S0 (explicitly resolved; no default sourcing in this section), level L2.
- Classification: levels differ → NO contact evaluation (R3.0.6). Heights may
  differ freely.
- Effects: `[create Wall W2 {levelId:L2, start:(0,0), end:(4000,0), t:200,
  h:2700, style:S0}]`.
- After state: two stacked walls on L1/L2, no joins, no warnings; each
  level's occupied region is the F1 rectangle.
- Undo: deletes W2.

### 3.6 Boundaries and exclusions

Owned elsewhere: pointer-event mapping, chain finish/cancel lifecycle,
snapping/angle-constraint input, selection, typing coalescing, redo and
session persistence (sections 6–8, issue 0003); level/document default
resolution (height, thickness, style) and visibility (section 2, issue 0020);
opening lifecycle, host parameterization, host remap (section 4, issue 0019);
room topology, metadata predicates, reconciliation (section 5, issue 0002);
operation framing, counters, conflict rules (format specification, issue
0004); representable bounds, precision, near-degeneracy (issues 0022/0030);
benchmark corpus, percentiles, thresholds (issue 0032). Excluded from v0.1:
curved walls (R3.0.1), unconnected overpasses, implicit detach-by-distance,
implicit minimum lengths or tolerances, miter constructions, normative
default sourcing for wall parameters, and any numerical performance claim in
this specification.

## 4. Openings

Status: **Normative**. Rules are identified as `R4.*`. All coordinates,
dimensions, and sills are millimetres; angles are degrees. Fixture outputs are
exact; R4.0.7 states the numerical-policy boundary.

### 4.0 Conventions

- **R4.0.1 Generic kind.** There is exactly ONE entity kind `Opening`,
  covering doors and windows through the authoritative field
  `kind ∈ {door, window}`. No separate door or window entity kinds exist in
  v0.1; validators MUST reject any other value (`invalid-opening-kind`). Kind
  influences no geometric rule in this section — only derived symbolism
  (R4.0.5) — and MAY be changed by an ordinary update command, which is then
  revalidated exactly like any semantic opening command (R4.4).
- **R4.0.2 Direct host.** Every opening references its host wall directly via
  `hostWallId`, which MUST name an existing wall (`unknown-host`); there is
  no indirection through other entities, and v0.1 creation commands cannot
  create the host inline. Openings attach to wall bodies only: an opening
  hosted by a join, by another opening, or spanning multiple walls does not
  exist. Openings store NO level of their own: the effective level is always
  the host's `levelId`, and openings on different levels never interact
  (extends the F24 isolation to apertures). The following-host rule is
  OBSERVABLE, not merely definitional: any projection query reporting an
  opening's level resolves it as the current `hostWallId`'s level at read
  time. Because `hostWallId` changes ONLY through the split remap of
  R4.3.3 — whose children inherit the host's level (R3.1.3) — and a wall's
  `levelId` is itself immutable (R4.3.2), an opening's effective level
  NEVER changes during the opening's lifetime. An update attempting to set
  a `levelId` on an opening rejects (`opening-level-immutable`; vector in
  F34).
- **R4.0.3 Center fraction from stored Start.** Let L be the host's axis
  length and â = (end − start)/L its unit axis, with left normal
  n̂ = (−ây, âx) relative to travel direction Start→End. An opening's center
  fraction `u ∈ [0,1]` places the aperture center at
  P(u) = start + u·(end − start), measured along the STORED Start→End axis.
  This parameterization is independent of endpoint direction, dimensions,
  sill, elevation, depth, and side: identical u values locate corresponding
  points on any host. Non-finite values reject (`nonfinite-value`); u outside
  [0,1] rejects (`invalid-opening-center`).
- **R4.0.4 Dimensions, sill, swing, style.** Authoritative per opening:
  `widthMm > 0`, `heightMm > 0`, `depthMm > 0`, all finite
  (`nonpositive-width`, `nonpositive-height`, `nonpositive-depth`);
  `depthMm ≤ host.thicknessMm` (`depth-exceeds-host-thickness`); `sillMm`
  finite and ≥ 0 (negative sills reject with `negative-sill`), measured from
  the HOST LEVEL's finished-floor elevation (`elevationMm`, section 2.1);
  relative side `∈ {Left, Right}` where Left names the +n̂ host face and
  Right the −n̂ face — this mapping is used consistently everywhere,
  including the cut spans of R4.2.1; hinge `∈ {nearStart, nearEnd}` naming
  the hinged end in axis terms; `opensToward ∈ {Left, Right}` naming, by the
  same
  +n̂/−n̂ convention, the half-plane toward which the leaf extends when open;
  and `swingDeg`: an UNSIGNED finite angle in `[0°, 180°]` whose zero means
  the leaf closed, coplanar with the wall plane (making `opensToward`
  geometrically inert though still stored authoritative data) and whose
  positive magnitude is the leaf's excursion about the hinge into the
  `opensToward` half-plane. Values outside the range reject
  (`invalid-swing-angle`); a value outside {Left, Right} rejects
  (`invalid-opens-toward`); a side outside {Left, Right} rejects
  (`invalid-side`); a hinge outside {nearStart, nearEnd} rejects
  (`invalid-hinge`); non-finite values reject (`nonfinite-value`).
  Style references are optional opaque tokens (FORMAT-SPEC §2.3; ED-0025:
  no document-level style registry exists to resolve them against, ED-0114).
  An opening may carry none; an explicitly supplied EMPTY token rejects
  (`missing-style`). Resolution of tokens against declared styles is not
  defined and no "stale" diagnostic exists. No opening parameter is ever sourced from level or document
  defaults, and changing a level default never mutates an existing opening
  (section 2.1 handoff rule).
- **R4.0.4a Operable mechanism (original GUI decision, issue 0069).** Door
  mechanisms are `doorHinged` and `doorSliding`; window mechanisms are
  `windowSideHung`, `windowDoubleSwing`, and `windowLift`. A mechanism of the
  wrong kind rejects. Absence means `doorHinged` for a door and
  `windowSideHung` for a window, without rewriting pre-existing files.
  The 2D editor displays `doorHinged` with the R4.0.4 swing, `doorSliding`
  with a parallel track, `windowSideHung` with one hinged leaf,
  `windowDoubleSwing` with two symmetric leaves, and `windowLift` with a
  parallel track and lift marker. Glyphs are derived; no persisted per-leaf
  opening fraction or actuator animation is inferred. Existing aperture fit
  and overlap rules apply equally to all mechanisms.
- **R4.0.5 Data classes.** Authoritative: id, `kind`, optional `mechanism`, `hostWallId`, `u`,
  `widthMm`, `heightMm`, `sillMm`, `depthMm`, `side`, `hinge`,
  `opensToward`, `swingDeg`, style references. Derived: aperture footprint,
  cut/subtracted occupied geometry, door/window glyphs, meshes, and swing
  arcs — rebuildable, never authoritative, persistable only in a
  non-authoritative cache namespace per the format specification.
  View-local: previews, hover, tool state.
- **R4.0.6 Set semantics.** The openings hosted by one wall form an
  UNORDERED SET. Every validation predicate below is pairwise and symmetric,
  and every derived result is invariant under creation, edit, and listing
  order.
- **R4.0.7 Numerical-policy boundary.** Identical in kind to R3.0.8: all
  rules and fixtures are defined in exact real arithmetic; fixtures are
  SEMANTIC ORACLES under the conformance relation of the future numerical
  model (issues 0022/0030); implementations are not required to emit
  bit-identical coordinates.

### 4.1 Creation and validation

- **R4.1.1 Batch boundaries.** A committed opening creation is exactly ONE
  atomic batch containing one create-Opening operation; a committed property
  edit is one atomic batch containing its update operations. Previews and
  cancelled gestures emit zero operations. Any rejecting class aborts the
  whole batch with zero effects, unchanged state, and no undo entry
  (R3.1.6 semantics).
- **R4.1.2 Strict horizontal fit.** With aperture half-width w/2, the OPEN
  interval (u·L − w/2, u·L + w/2) must lie strictly inside (0, L): positive
  clearance from BOTH host endpoints is mandatory. Zero clearance or crossing
  rejects (`aperture-not-strict-fit`). Example: L = 4000, w = 900, u = 1/2 →
  aperture [1550, 2450], clearances 1550 and 1550 — accepted; L = 900 with
  u = 1/2 gives clearance exactly 0 at both ends — rejected.
- **R4.1.3 Top check.** sillMm + heightMm ≤ host.heightMm, INCLUSIVELY
  (`aperture-exceeds-host-height`). Only values exceeding the host height
  reject; equality is legal. Example: sill 2100 + height 400 = 2500 against a
  2500 host is accepted; sill 2100 + height 401 rejects.
- **R4.1.4 Overlap matrix.** For two openings on the SAME host, define the
  along-axis interval [u·L − w/2, u·L + w/2] and the vertical interval
  [sill, sill + height]. Reject with `opening-overlap` IFF both intervals
  intersect with POSITIVE length. Touching (zero-length intersection) in
  either axis is legal regardless of the other axis; disjointness in either
  axis is legal. Openings on different hosts are never compared. The
  predicate is symmetric; results never depend on order (R4.0.6). Examples:
  side-by-side openings sharing one endpoint commit; a transom window whose
  vertical interval touches the door's at sill = 2100 commits even with
  positive horizontal overlap; shifting that transom down to sill = 2099
  makes both intersections positive and rejects.
- **R4.1.5 Multiple openings.** A host may carry any number of openings
  subject to R4.1.2–R4.1.4 pairwise. Each creation gesture is its own batch
  and undo boundary.

### 4.2 Derived aperture geometry

- **R4.2.1 Cut rectangle.** The aperture footprint is the closed plan
  rectangle with corners P(u) ± (w/2)·â + η·n̂ where η spans the across-
  thickness span: for `side: Left`, η ∈ [t/2 − d, t/2]; for `side: Right`,
  η ∈ [−t/2, −t/2 + d] (d = `depthMm`, t = host thickness). The cut always
  touches its named face and extends inward by d; d = t yields a
  full-thickness cut. Vertical extent [sill, sill + height] above the level
  datum never appears in plan.
- **R4.2.2 Subtraction scope and order.** The occupied region of a wall
  cluster (R3.2.1) is amended as follows: each wall contributes its CLOSED
  centered butt-capped rectangle MINUS the union of the OPEN interiors of its
  OWN apertures' footprints — the cut happens BEFORE the union — and join
  patches (R3.2.3) are unioned afterwards and are NEVER subtracted, however
  close an aperture lies to a join. Because subtraction is formulated over
  the union of footprints, the result is independent of opening order
  (R4.0.6). A full-thickness cut may disconnect the cluster region into
  additional components; a partial-depth cut produces a notch.
- **R4.2.3 Canonical normalization.** R3.2.4 applies unchanged to the amended
  region (simple rings, CCW exteriors, CW holes, least-vertex rotation,
  component ordering). The result is invariant under the reversal map of
  R4.3.1: reversing a host leaves every canonical outline unchanged.
- **R4.2.4 Non-authority.** Aperture geometry is derived data (R4.0.5):
  recomputed, never edited directly, never persisted as authoritative state.
- **R4.2.5 Rooms unaffected.** Openings NEVER alter room enclosure, room
  detection, room boundary polygons, or room metadata in any sense of
  section 5: room machinery consumes walls and joins only. This allocation is
  binding on issue 0002.

### 4.3 Host lifecycle

- **R4.3.1 Reversal transform.** Reversing a host (R3.3.6) swaps
  `startMm`/`endMm` and, in the SAME batch, maps EVERY hosted opening by
  `u := 1 − u`, `side := opposite(side)`,
  `hinge := opposite(hinge)` (nearStart ↔ nearEnd),
  `opensToward := opposite(opensToward)`; `swingDeg`, all dimensional
  fields, `kind`, and style references are untouched. World intent is
  preserved exactly: P(1−u) on the reversed axis equals P(u) on the original,
  the flipped side names the same physical face, the flipped hinge names the
  same physical end, and the flipped `opensToward` names the same physical
  open half-plane (F29 verifies this on an axis-aligned host with hinge
  nearStart → nearEnd; the identity
  holds for arbitrary straight hosts since it depends only on â/n̂ antipodal
  symmetry).
- **R4.3.2 Geometry edits keep u.** Endpoint drag, joint drag, body drag,
  numeric coordinate edits, and shortening or lengthening leave EVERY stored
  opening field untouched — u is kept by design, never re-floated.
  COMPLETE-FIELD REVALIDATION (binding): every committed batch that affects
  ANY geometric field of a hosting wall (`startMm`, `endMm`,
  `thicknessMm`, `heightMm`) revalidates EVERY hosted opening against ALL
  FOUR final fields plus the top check (R4.1.3) and pairwise overlap
  (R4.1.4) — not merely the edited fields — and any failure rejects the
  whole batch atomically. Commands changing ONLY a wall's style references
  or other non-geometric metadata change no geometric field: hosted openings
  keep every field, cut geometry is unaffected, and revalidation trivially
  passes; opening style references are independent of wall styles and are
  never altered by a host style change. DIRECT WALL LEVEL MUTATION IS
  REJECTED, and this section owns that rule because the host-supplies-level
  contract (R4.0.2) depends on it: any command attempting to set `levelId`
  on an existing wall — hosting openings or not — rejects deterministically
  with `wall-level-immutable`. Re-leveling therefore requires deleting and
  recreating the wall, which is an ORDINARY cascade with NO opening
  carry-over: the deletion removes its hosted openings (R4.3.4), the
  recreated wall starts with NO openings, and equivalent openings on the
  new wall exist only as fresh explicit create commands validated in full
  against the new geometry. Consequently an opening's effective level never
  changes during its lifetime (R4.0.2).
- **R4.3.3 Split remap-or-reject.** When a hosting wall splits into ordered
  children with spans [s_k, e_k] along the original axis (R3.1.3), each
  hosted opening remaps IFF exactly ONE child span strictly contains its
  CLOSED aperture interval [u·L − w/2, u·L + w/2]:
  s_k < lo AND hi < e_k. The remapping batch updates `hostWallId` to that
  child's id (the original id when the child contains the original Start,
  else the new child's id) and recomputes
  `u := (u·L − s_k)/(e_k − s_k)`; all other fields stay. If any hosted
  opening straddles a split point, touches one within zero clearance, or has
  no strictly containing child, the ENTIRE batch — including the incoming
  wall creation or endpoint move that caused the split — rejects atomically
  with `opening-split-ambiguous`. Opening identity never changes.
- **R4.3.4 Deletion.** Two forms exist, each one atomic batch and one undo
  boundary. STANDALONE: deleting an opening removes exactly that entity;
  undo recreates an equivalent opening (F36). CASCADE: deleting a hosting
  wall deletes all of its openings in the SAME batch; the opening deletions
  come FIRST and the wall deletion DEPENDS on them in the R3.1.5 dependency
  model (F20). Undo applies inverse effects along REVERSE dependencies:
  the wall is restored first, then its openings.
- **R4.3.5 Host identity is immutable per opening.** Update commands MUST
  NOT change `hostWallId` (`opening-direct-rehost`); moving an opening to
  another host is done by deleting it and creating a new one. The only
  operation that changes a stored host is the split remap of R4.3.3. An
  update command that ADDRESSES an opening through a wall other than its
  stored host rejects with `wrong-host`. An update attempting to set a
  `levelId` on an opening rejects with `opening-level-immutable` — the
  level follows the host observably (R4.0.2). A supplied opening id
  colliding with ANY existing entity identifier of ANY kind — wall, join,
  level, opening, or otherwise — rejects deterministically as invalid input
  with an empty batch and no undo entry (`entity-id-collision`), mirroring
  the section 2.7 policy.
- **R4.3.6 Undo equivalence.** Each accepted batch is one undo boundary;
  undo restores a projection equivalent in entity and field values to the
  prior state (R3.4.7 semantics), including remapped `hostWallId`/`u` values.
- **R4.3.7 Performance locality.** Recomputation after any accepted batch
  MUST be confined to the union of the pre-edit and post-edit join-connected
  components of the touched walls, mirroring R3.4.8; aperture recomputation
  adds no document-wide invalidation. Named benchmark corpora, percentiles,
  memory bounds, and thresholds are owned by issue 0032; render budgets for
  glyphs and meshes are owned by issue 0033. None are specified here.

### 4.4 Diagnostics

All rejections are atomic (R4.1.1). Complete code matrix:

| Code | Condition |
|---|---|
| `nonfinite-value` | Any non-finite stored value (u, widths, heights, sill, depth, swingDeg, or coordinate-bearing input) |
| `invalid-opening-kind` | `kind` not in {door, window} |
| `invalid-opening-center` | u < 0 or u > 1 |
| `unknown-host` | `hostWallId` does not name an existing wall |
| `wrong-host` | A command addresses an opening through a wall other than its stored host (R4.3.5) |
| `opening-direct-rehost` | An update attempts to set `hostWallId` (R4.3.5) |
| `entity-id-collision` | A supplied opening id equals ANY existing entity identifier of ANY kind (R4.3.5) |
| `wall-level-immutable` | A command attempts to set levelId on an existing wall (R4.3.2; owned by this section) |
| `nonpositive-width` | widthMm ≤ 0 |
| `nonpositive-height` | heightMm ≤ 0 |
| `nonpositive-depth` | depthMm ≤ 0 |
| `negative-sill` | sillMm < 0 |
| `depth-exceeds-host-thickness` | depthMm > final host thicknessMm |
| `invalid-swing-angle` | Finite swingDeg outside [0°, 180°] |
| `invalid-opens-toward` | `opensToward` not in {Left, Right} |
| `invalid-side` | side not in {Left, Right} |
| `invalid-hinge` | hinge not in {nearStart, nearEnd} |
| `opening-level-immutable` | An update attempts to set a levelId on an opening (R4.0.2/R4.3.5) |
| `missing-style` | An explicitly supplied empty style token (ED-0114) |
| `aperture-not-strict-fit` | Horizontal strict-fit failure against final host length (zero clearance counts as failure) |
| `aperture-exceeds-host-height` | sillMm + heightMm > final host heightMm (equality is legal) |
| `opening-overlap` | Positive-length intersection in BOTH horizontal and vertical axes against any same-host opening |
| `opening-split-ambiguous` | Split leaves an aperture without exactly one strictly containing child |

Accepted boundaries: depth == host thickness is legal; sill == 0 is legal;
sillMm + heightMm exactly equal to the host height is legal (R4.1.3);
horizontal touching and vertical touching are legal (R4.1.4).

**Deterministic precedence for multiple invalid fields.** When a command
violates more than one condition, exactly ONE primary diagnostic is
reported, chosen by the FIRST match in this total order; the batch is still
rejected atomically and the other violations change nothing:

1. `nonfinite-value`
2. `entity-id-collision`
3. `unknown-host`
4. `wrong-host`
5. `opening-direct-rehost`
6. `opening-level-immutable`
7. `wall-level-immutable`
8. `invalid-opening-kind`
9. `missing-style`
10. `invalid-opening-center`
11. `invalid-side`
12. `invalid-hinge`
13. `invalid-opens-toward`
14. `invalid-swing-angle`
15. `nonpositive-width`
16. `nonpositive-height`
17. `nonpositive-depth`
18. `negative-sill`
19. `depth-exceeds-host-thickness`
20. `aperture-not-strict-fit`
21. `aperture-exceeds-host-height`
22. `opening-overlap`
23. `opening-split-ambiguous`

The ordering principle is identity and addressing first, then declaration
syntax (kind, styles), then per-field enums, then dimensional constraints —
TOTAL within the dimensional group as well: width before height before
depth — then cross-entity geometry. A combined-invalid vector is normative
at F34, including one proving the width-before-height-before-depth order.

### 4.5 Normative fixtures

Conventions as in section 3.5: exact rational values (R4.0.7), fixture style
token `S0` explicitly named by every created entity, `supplied:N` marks fresh
command-supplied unused identifiers, outlines are canonical CCW rings of the
amended occupied region (R4.2.2/R4.2.3).

#### F25 — Full-thickness door (accepted)
- Before: F1 state (wall W1 (0,0)→(4000,0), t=200, h=2500, isolated).
- Command: create Opening Od supplied:idOd {kind:door, hostWallId:W1, u:1/2,
  widthMm:900, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}.
- Classification: strict fit [1550,2450] ⊂ (0,4000); top 2100 ≤ 2500;
  depth 200 ≤ thickness 200; no other openings.
- Effects: `[create Opening Od supplied:idOd {kind:door, hostWallId:W1,
  u:1/2, widthMm:900, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}]`.
- Post-state: W1 {levelId:L1, start:(0,0), end:(4000,0), thicknessMm:200,
  heightMm:2500, style:S0}; Od exists with exactly the commanded fields and
  resolves its effective level to L1 through W1 (R4.0.2).
- After: outline two components (full-thickness cut disconnects the band):
  `(0,-100) (1550,-100) (1550,100) (0,100)` and
  `(2450,-100) (4000,-100) (4000,100) (2450,100)`.
- Undo: inverse effects `[delete Opening Od]`, restoring F1 exactly.

#### F26 — Partial-depth window (accepted)
- Before: F1 state.
- Command: create Opening Ow supplied:idOw {kind:window, hostWallId:W1,
  u:1/2, widthMm:1200, heightMm:1300, sillMm:900, depthMm:100, side:Left,
  hinge:nearEnd, opensToward:Left, swingDeg:45, style:S0}.
- Classification: strict fit [1400,2600]; top 2200 ≤ 2500; depth 100 ≤ 200;
  Left = +n̂ face, so the cut span is η ∈ [t/2 − d, t/2] = [0,100] along
  n̂ = (0,1) → plan notch y ∈ [0,100].
- Effects: `[create Opening Ow supplied:idOw {kind:window, hostWallId:W1,
  u:1/2, widthMm:1200, heightMm:1300, sillMm:900, depthMm:100, side:Left,
  hinge:nearEnd, opensToward:Left, swingDeg:45, style:S0}]`.
- Post-state: W1 unchanged; Ow exists with exactly the commanded fields.
- After: outline single ring `(0,-100) (4000,-100) (4000,100) (2600,100)
  (2600,0) (1400,0) (1400,100) (0,100)` — the partial-depth material below
  the notch (y ∈ [−100,0] across x ∈ [1400,2600]) remains. The vertical
  extent [900,2200] appears nowhere in plan.
- Undo: inverse effects `[delete Opening Ow]`.

#### F27 — Touching openings commit (accepted, order-invariant)
- Before: F1 state.
- Step 1 command: create Opening Od supplied:idOd {kind:door, hostWallId:W1,
  u:1/4, widthMm:800, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0} → aperture
  [600,1400], full thickness.
- Step 1 effects: `[create Opening Od supplied:idOd {kind:door,
  hostWallId:W1, u:1/4, widthMm:800, heightMm:2100, sillMm:0, depthMm:200,
  side:Left, hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}]`.
  Post-state: W1 unchanged; Od as commanded. Outline two components
  `(0,-100) (600,-100) (600,100) (0,100)` and `(1400,-100) (4000,-100)
  (4000,100) (1400,100)`. Undo: `[delete Opening Od]`.
- Step 2 command: create Opening On supplied:idOn {kind:window,
  hostWallId:W1, u:9/20, widthMm:800, heightMm:1300, sillMm:900,
  depthMm:100, side:Left, hinge:nearEnd, opensToward:Left, swingDeg:45,
  style:S0} → aperture [1400,2200] with partial-depth notch y ∈ [0,100],
  vertical interval [900,2200].
- Step 2 classification: horizontal intervals share exactly the point 1400
  (zero-length intersection) while vertical overlap is positive → LEGAL
  (R4.1.4); commits.
- Step 2 effects: `[create Opening On supplied:idOn {kind:window,
  hostWallId:W1, u:9/20, widthMm:800, heightMm:1300, sillMm:900,
  depthMm:100, side:Left, hinge:nearEnd, opensToward:Left, swingDeg:45,
  style:S0}]`. Post-state: W1 unchanged; Od, On as commanded.
- Final outline TWO components (the window's surviving lower strip
  [1400,2200] × [−100,0] shares the edge x = 2200 with the plain right
  band, so the union MERGES them into one ring):
  `(0,-100) (600,-100) (600,100) (0,100)` and
  `(1400,-100) (4000,-100) (4000,100) (2200,100) (2200,0) (1400,0)`.
- Order invariance: performing the steps in reverse order yields the same
  classifications and the identical canonical final state (R4.0.6).
- Step 3 (transom continuation): create Opening Ot supplied:idOt
  {kind:window, hostWallId:W1, u:1/4, widthMm:800, heightMm:380,
  sillMm:2100, depthMm:100, side:Left, hinge:nearEnd, opensToward:Left,
  swingDeg:45, style:S0}.
  Classification: horizontal interval [600,1400] overlaps Od positively BUT
  the vertical interval [2100,2480] touches Od's [0,2100] at exactly 2100 →
  legal by R4.1.4; top check 2480 ≤ 2500 passes; its plan footprint lies
  inside the door's removed footprint.
  Effects: `[create Opening Ot supplied:idOt {kind:window, hostWallId:W1,
  u:1/4, widthMm:800, heightMm:380, sillMm:2100, depthMm:100, side:Left,
  hinge:nearEnd, opensToward:Left, swingDeg:45, style:S0}]`. Post-state: W1,
  Od, On, Ot as commanded. Outline IDENTICAL to the step-2 final outline
  (no plan change). Undo: `[delete Opening Ot]` restoring the step-2
  projection exactly.
- Undo boundaries: each step is an independent batch.

#### F28 — Overlap matrix (complete vectors)
All three commands run independently against the same before-state: F26
aftermath (wall W1 with window aperture [1400,2600], vertical [900,2200]).

- (a) Command: create Opening Oa supplied:idOa {kind:door, hostWallId:W1,
  u:3/8, widthMm:800, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}.
  Classification: aperture [1100,1900]; horizontal intersection
  (1400,1900) positive AND vertical intersection (900,2100) positive →
  reject `opening-overlap`; empty batch; state unchanged; no undo entry.
- (b) Command: create Opening Ob supplied:idOb {kind:door, hostWallId:W1,
  u:3/8, widthMm:800, heightMm:150, sillMm:2300, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}.
  Classification: vertical interval [2300,2450] disjoint from [900,2200] →
  legal; top check 2450 ≤ 2500 passes; commits.
  Effects: `[create Opening Ob supplied:idOb {kind:door, hostWallId:W1,
  u:3/8, widthMm:800, heightMm:150, sillMm:2300, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}]`.
  Post-state: W1 unchanged; Ow per F26; Ob as commanded. Outline two
  components `(0,-100) (1100,-100) (1100,100) (0,100)` and `(1900,-100)
  (4000,-100) (4000,100) (2600,100) (2600,0) (1900,0)` (Ob's full-thickness
  cut [1100,1900] removes the band left of the window notch).
  Undo: `[delete Opening Ob]`, restoring the F26 outline exactly.
- (c) Command: create Opening Oc supplied:idOc {kind:door, hostWallId:W1,
  u:3/4, widthMm:800, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0} — run against
  the plain F26 aftermath (without Ob).
  Classification: aperture [2600,3400] touches the window at 2600 only →
  horizontal intersection zero-length → legal despite positive vertical
  overlap; commits.
  Effects: `[create Opening Oc supplied:idOc {kind:door, hostWallId:W1,
  u:3/4, widthMm:800, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}]`.
  Post-state: W1 unchanged; Ow per F26; Oc as commanded. Outline two
  components `(0,-100) (2600,-100) (2600,0) (1400,0) (1400,100) (0,100)`
  and `(3400,-100) (4000,-100) (4000,100) (3400,100)`.
  Undo: `[delete Opening Oc]`, restoring the F26 outline exactly.

#### F29 — Reversal maps openings, world intent invariant
- Before: wall W1 {levelId:L1, start:(0,0)→(4000,0), thicknessMm:200,
  heightMm:2500, style:S0} hosting Ow {kind:window,
  hostWallId:W1, u:3/10, widthMm:1200, heightMm:1300, sillMm:900,
  depthMm:100, side:Left, hinge:nearStart, opensToward:Right, swingDeg:60,
  style:S0}.
- Command: reverse W1.
- Effects: `[update W1 {start:(4000,0), end:(0,0)}; update Ow {u:7/10,
  side:Right, hinge:nearEnd, opensToward:Left}]`. SwingDeg and all
  dimensions untouched.
- World check: center P(7/10) on the reversed axis is still (1200,0); the
  Right face on the flipped n̂ = (0,−1) is still the world +y face; the
  nearEnd hinge at c + 600·â is still (600,0); opensToward Left on the
  flipped n̂ is still the world −y open half-plane. Canonical outline
  identical to pre-reversal: `(0,-100) (4000,-100) (4000,100) (1800,100)
  (1800,0) (600,0) (600,100) (0,100)`.
- Post-state: W1 {start:(4000,0), end:(0,0)}; Ow {u:7/10, side:Right,
  hinge:nearEnd, opensToward:Left}, all other fields unchanged.
- Undo: inverse effects `[update W1 {start:(0,0), end:(4000,0)}; update Ow
  {u:3/10, side:Left, hinge:nearStart, opensToward:Right}]`.

#### F30 — Shorten keeping u (accepted)
- Before: F29 pre-reversal state (W1 (0,0)→(4000,0), Ow u = 3/10,
  widthMm 1200).
- Command: numeric edit sets W1.end := (3600,0).
- Revalidation (complete-field, R4.3.2): new aperture center u·L =
  3/10 · 3600 = 1080, bounds [480,1680]; strict fit holds since
  0 < 480 and 1680 < 3600; top and depth unaffected; no overlap
  introduced. NOTE the aperture moves with u: it is NOT pinned at
  [600,1800].
- Effects: `[update W1 {end:(3600,0)}]`; Ow fields untouched (u stays
  3/10).
- Post-state: W1 (0,0)→(3600,0); every Ow field unchanged.
- After: outline `(0,-100) (3600,-100) (3600,100) (1680,100) (1680,0)
  (480,0) (480,100) (0,100)`.
- Undo: inverse effects `[update W1 {end:(4000,0)}]`.

#### F31 — Shorten violating strict fit rejects
- Before: F30 before-state.
- Commands (each independently): set W1.end := (1700,0) → new aperture
  bounds [uL − w/2, uL + w/2] = [510 − 600, 510 + 600] = [−90,1110]: lo < 0
  crosses the Start endpoint → reject `aperture-not-strict-fit`; set
  W1.end := (2000,0) → new bounds [600 − 600, 600 + 600] = [0,1200]:
  zero clearance at the Start endpoint → reject `aperture-not-strict-fit`.
- Effect each: empty batch, state unchanged, no undo entry.

#### F32 — Split remaps to the strictly containing child
- Before: F29 pre-reversal state (aperture [600,1800]).
- Command: create Wall W2: (3000,-1500)→(3000,1500), t=300, h=2500,
  style S0. Unequal thickness makes the perpendicular join patch
  NONDEGENERATE (diamond below), exercising patch preservation during a
  split.
- Classification: proper crossing at (3000,0), interior × interior → W1
  splits into W1[0→3000] (keeps id) + idB[3000→4000]; W2 splits into
  W2[(3000,-1500)→(3000,0)] + idC[(3000,0)→(3000,1500)], both inheriting
  t=300; the crossing walls' strips occupy x ∈ [2850,3150]; the join patch
  is the NONDEGENERATE hull of cross-sections {(3000,−100),(3000,100)} ∪
  {(2850,0),(3150,0)} = diamond `(3000,-100) (3150,0) (3000,100)
  (2850,0)`. Remap: closed aperture [600,1800] satisfies 0 < 600 and
  1800 < 3000 → strictly inside child A alone → hostWallId stays W1 and
  u := (3/10 · 4000)/3000 = 2/5.
- Effects: `[create Wall idB supplied:idB {levelId:L1, start:(3000,0),
  end:(4000,0), thicknessMm:200, heightMm:2500, style:S0}; create Wall W2
  {levelId:L1, start:(3000,-1500), end:(3000,0), thicknessMm:300,
  heightMm:2500, style:S0}; create Wall idC supplied:idC {levelId:L1,
  start:(3000,0), end:(3000,1500),
  thicknessMm:300, heightMm:2500, style:S0}; create Join J1 supplied:idJ1
  {(W1,End),(idB,Start),(W2,End),(idC,Start)}; update W1 {end:(3000,0)};
  update Ow {u:2/5}]` — created walls carry complete post-resolution field
  values per R3.1.5; every created child explicitly carries levelId:L1
  (idB inheriting it from W1 like all split children per R3.1.3).
- Post-state: W1[0→3000] with Ow {hostWallId:W1, u:2/5, all other fields
  unchanged}; idB[3000→4000] t=200; W2[(3000,-1500)→(3000,0)] t=300;
  idC[(3000,0)→(3000,1500)] t=300; J1 four members as above.
- After: outline ONE merged component (the t=300 strips overlap the host
  band and absorb the diamond entirely):
  `(0,-100) (2850,-100) (2850,-1500) (3150,-1500) (3150,-100) (4000,-100)
  (4000,100) (3150,100) (3150,1500) (2850,1500) (2850,100) (1800,100)
  (1800,0) (600,0) (600,100) (0,100)`.
- Undo: inverse effects along reverse dependencies delete idB, W2, idC, J1,
  restore W1 {end:(4000,0)} and Ow {u:3/10}.

#### F32b — Remap to a NEW child (complete vectors)
- Before: F29 pre-reversal state.
- Command: create Wall W3 supplied:idW3: (500,-1000)→(500,1000), t=200,
  h=2500, style S0.
- Classification: proper crossing at (500,0), interior × interior → W1
  splits into W1[0→500] (keeps id) + idD[500→4000]; W3 splits into
  W3[(500,-1000)→(500,0)] + idE[(500,0)→(500,1000)]; new join J1 with four
  members {(W1,End),(idD,Start),(W3,End),(idE,Start)}. Remap: closed
  aperture [600,1800] satisfies 500 < 600 and 1800 < 4000 → strictly inside
  child B alone → `hostWallId := idD` and u := (uL − s_B)/(e_B − s_B) =
  (1200 − 500)/3500 = 1/5. All other fields unchanged.
- Effects: `[create Wall idD supplied:idD {levelId:L1, start:(500,0),
  end:(4000,0), thicknessMm:200, heightMm:2500, style:S0}; create Wall W3
  {levelId:L1, start:(500,-1000), end:(500,0), thicknessMm:200,
  heightMm:2500,
  style:S0}; create Wall idE supplied:idE {levelId:L1, start:(500,0),
  end:(500,1000), thicknessMm:200, heightMm:2500, style:S0}; create Join J1
  supplied:idJ1
  {(W1,End),(idD,Start),(W3,End),(idE,Start)}; update W1 {end:(500,0)};
  update Ow {hostWallId:idD, u:1/5}]` — every created child explicitly
  carries levelId:L1 (idD inheriting it from W1 per R3.1.3).
- Post-state: W1[0→500]; idD[500→4000]; W3, idE as resolved; J1 four
  members; Ow {hostWallId:idD, u:1/5}, all other fields unchanged (its
  effective level still resolves through idD to the same level).
- After: outline ONE merged component:
  `(0,-100) (400,-100) (400,-1000) (600,-1000) (600,-100) (4000,-100)
  (4000,100) (1800,100) (1800,0) (600,0) (600,1000) (400,1000) (400,100)
  (0,100)`.
- Undo: inverse effects restore Ow {hostWallId:W1, u:3/10} and W1's full
  extent; delete idD, W3, idE, J1.

#### F33 — Straddling split rejects whole batch
- Before: F29 pre-reversal state.
- Command: create Wall W4: (1200,-1000)→(1200,1000), t=200, h=2500, style S0.
- Classification: crossing at (1200,0) lies strictly inside the aperture
  (600,1800), so the aperture intersects BOTH prospective children → reject
  `opening-split-ambiguous`; the entire batch aborts: no wall, no join,
  Ow unchanged.
- Variant: crossing exactly at (600,0), ON the aperture edge → containment
  not strict → same rejection.

#### F34 — Validation matrix (compact rejects)
- Before: levels L1 and L2 exist (L1 active); bare wall W1 {levelId:L1,
  start:(0,0), end:(4000,0), thicknessMm:200, heightMm:2500, style:S0}.
- Creation commands, each independently rejected atomically with empty batch
  and no undo entry: widthMm 0 → `nonpositive-width`; heightMm 0 →
  `nonpositive-height`; depthMm 0 → `nonpositive-depth`; depthMm 201 →
  `depth-exceeds-host-thickness`; u 5/4 → `invalid-opening-center`; u
  non-finite → `nonfinite-value`; sillMm −100 → `negative-sill`; swingDeg
  −5 and swingDeg 181 → `invalid-swing-angle`; swingDeg non-finite →
  `nonfinite-value`; opensToward:Inward → `invalid-opens-toward`;
  kind:sliding → `invalid-opening-kind`; side:Inward → `invalid-side`;
  hinge:middle → `invalid-hinge`; empty
  style token → `missing-style`; hostWallId naming a nonexistent wall → `unknown-host`;
  supplied id equal to W1's id (or any other existing entity id of any
  kind) → `entity-id-collision`; {sillMm:2101, heightMm:400} against
  h=2500 → top sum 2501 exceeds → reject `aperture-exceeds-host-height`.
- Edit-command rejects against the F26 aftermath (existing Ow): update Ow
  with `hostWallId` set to another existing wall → `opening-direct-rehost`;
  an update addressing Ow via a different wall's identity → `wrong-host`;
  update Ow {levelId:L2} with L2 the other declared level →
  `opening-level-immutable` (the level follows the host observably,
  R4.0.2); update W1 {levelId:L2} — direct wall level mutation, hosted
  openings or not → reject `wall-level-immutable` (owned by R4.3.2).
- Combined-invalid vector (deterministic precedence, R4.4): creation
  command `create Opening supplied:idX {kind:sliding, hostWallId:W1,
  u:5/4, widthMm:0, heightMm:1300, sillMm:−5, depthMm:100, side:Left,
  hinge:nearStart, opensToward:Left, swingDeg:45, style:S0}` is otherwise
  valid — supplied id unused against every entity of every kind, host W1
  exists on L1, and every remaining authoritative field is in range — with
  ONLY kind, u, widthMm, and sillMm invalid. The FIRST match in the
  precedence order is `invalid-opening-kind`, reported as the primary
  diagnostic while the u/width/sill violations are ignored by the atomic
  rejection. Empty batch; no undo entry.
- Combined multi-dimensional vector (width primary): creation command
  `create Opening supplied:idY {kind:door, hostWallId:W1, u:1/2,
  widthMm:0, heightMm:0, sillMm:0, depthMm:0, side:Left, hinge:nearStart,
  opensToward:Left, swingDeg:45, style:S0}` is otherwise valid — supplied
  id unused, host exists, kind/enums/sill in range — with ALL THREE
  dimensions nonpositive simultaneously. The precedence order is total
  within the dimensional group: `nonpositive-width` (rank 16) precedes
  height (17) and depth (18), so the primary diagnostic is
  `nonpositive-width`. Empty batch; no undo entry.
- Accepted boundary notes: depthMm exactly 200 commits; {sillMm:2100,
  heightMm:400} against h=2500 COMMITS (equality is legal under the
  inclusive top check); {sillMm:0, heightMm:2100} commits.

#### F35 — Join proximity leaves patches intact (accepted)
- Before: level L1; wall W1 {levelId:L1, start:(0,0), end:(3000,0),
  thicknessMm:200, heightMm:2500, style:S0} joined with W2
  {levelId:L1, start:(3000,0)→(3000,2500),
  thicknessMm:300, heightMm:2500, style:S0} via J1 {(W1,End),(W2,Start)}.
  Unequal thicknesses make the perpendicular join patch NONDEGENERATE: hull
  of cross-sections {(3000,-100),(3000,100)} ∪ {(2850,0),(3150,0)} = diamond
  `(3000,-100) (3150,0) (3000,100) (2850,0)`; W2 occupies x ∈ [2850,3150],
  y ∈ [0,2500].
- Command: create Opening Oj supplied:idOj {kind:door, hostWallId:W1, u:5/6,
  widthMm:600, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearEnd, opensToward:Right, swingDeg:90, style:S0}.
- Classification: strict fit [2200,2800] ⊂ (0,3000) with clearance 200 to
  the joined endpoint; top and depth pass; patch is never subtracted
  (R4.2.2).
- Effects: `[create Opening Oj supplied:idOj {kind:door, hostWallId:W1,
  u:5/6, widthMm:600, heightMm:2100, sillMm:0, depthMm:200, side:Left,
  hinge:nearEnd, opensToward:Right, swingDeg:90, style:S0}]`.
  Post-state: W1, W2, J1 unchanged; Oj as commanded.
- After: outline two components:
  `(0,-100) (2200,-100) (2200,100) (0,100)` and
  `(2800,-100) (3000,-100) (3150,0) (3150,2500) (2850,2500) (2850,100)
  (2800,100)` — the join-side door piece merges with W2 and the diamond;
  only the diamond's lower-right edge `(3000,-100)→(3150,0)` is exterior,
  its other three edges being absorbed by the rectangles.
- Undo: `[delete Opening Oj]`, restoring the pre-opening occupied region.

#### F36 — Standalone opening deletion
- Before: F26 aftermath (wall W1 (0,0)→(4000,0), t=200, h=2500, hosting Ow
  per F26).
- Command: delete Opening Ow.
- Classification: ordinary single-entity deletion; the host is unaffected.
- Effects: `[delete Opening Ow]`.
- Post-state: W1 unchanged; no openings.
- After: bare rectangle outline `(0,-100) (4000,-100) (4000,100) (0,100)`
  (the notch is gone).
- Undo: inverse effects recreate Ow {kind:window, hostWallId:W1, u:1/2,
  widthMm:1200, heightMm:1300, sillMm:900, depthMm:100, side:Left,
  hinge:nearEnd, opensToward:Left, swingDeg:45, style:S0}; the F26 outline
  reappears.

#### F37 — Property updates with complete vectors (accepted)
Each command runs against the F26 aftermath and is its own atomic batch and
undo boundary; base Ow = {kind:window, hostWallId:W1, u:1/2, widthMm:1200,
heightMm:1300, sillMm:900, depthMm:100, side:Left, hinge:nearEnd,
opensToward:Left, swingDeg:45, style:S0}. Every update revalidates the
whole entity (R4.4).

- (a) kind: command `update Ow {kind:door}`. Classification: kind carries
  no geometric rule (R4.0.1); all fit checks pass unchanged. Effects:
  `[update Ow {kind:door}]`. Post-state: Ow as base except kind:door.
  Outline unchanged (glyph symbolism is derived and may differ). Undo:
  `[update Ow {kind:window}]`.
- (b) side: command `update Ow {side:Right}`. Classification: cut span
  flips to η ∈ [−t/2, −t/2 + d] = [−100,0] along n̂ = (0,1) → notch moves
  to y ∈ [−100,0]; strict fit, top, depth, overlap all pass. Effects:
  `[update Ow {side:Right}]`. Post-state: Ow as base except side:Right.
  Outline single ring `(0,-100) (1400,-100) (1400,0) (2600,0) (2600,-100)
  (4000,-100) (4000,100) (0,100)`. Undo: `[update Ow {side:Left}]`
  restoring the F26 outline.
- (c) hinge: command `update Ow {hinge:nearStart}`. Classification: valid
  enum; hinge affects derived leaf geometry only; all checks pass.
  Effects: `[update Ow {hinge:nearStart}]`. Post-state: Ow as base except
  hinge:nearStart. Outline unchanged. Undo: `[update Ow {hinge:nearEnd}]`.
- (d) opensToward: command `update Ow {opensToward:Right}`. Classification:
  valid enum; display-relevant only; all checks pass. Effects:
  `[update Ow {opensToward:Right}]`. Post-state: Ow as base except
  opensToward:Right. Outline unchanged. Undo: `[update Ow
  {opensToward:Left}]`.
- (e) sill: command `update Ow {sillMm:600}`. Classification: sill ≥ 0;
  new vertical interval [600,1900]; top check 1900 ≤ 2500 passes; no other
  opening to overlap. Effects: `[update Ow {sillMm:600}]`. Post-state: Ow
  as base except sillMm:600. Outline unchanged (plan shows the footprint,
  not the elevation). Undo: `[update Ow {sillMm:900}]`.
- (f) style: command `update Ow {style:S1}` with S1 a non-empty token.
  Classification: explicitly resolved set command (R4.0.4); styles have no
  geometric rule. Effects: `[update Ow {style:S1}]`. Post-state: Ow as
  base except style:S1. Outline unchanged. Undo: `[update Ow {style:S0}]`.
- (g) u: command `update Ow {u:5/8}`. Classification: u ∈ [0,1]; new
  aperture center u·L = 2500, bounds [1900,3100] ⊂ (0,4000); all other
  checks pass. Effects: `[update Ow {u:5/8}]`. Post-state: Ow as base
  except u:5/8. Outline single ring `(0,-100) (4000,-100) (4000,100)
  (3100,100) (3100,0) (1900,0) (1900,100) (0,100)`. Undo:
  `[update Ow {u:1/2}]` restoring the F26 outline.
- (h) widthMm: command `update Ow {widthMm:800}`. Classification: new
  bounds [1600,2400] ⊂ (0,4000); strict fit holds; all other checks pass.
  Effects: `[update Ow {widthMm:800}]`. Post-state: Ow as base except
  widthMm:800. Outline single ring `(0,-100) (4000,-100) (4000,100)
  (2400,100) (2400,0) (1600,0) (1600,100) (0,100)`. Undo:
  `[update Ow {widthMm:1200}]`.
- (i) heightMm: command `update Ow {heightMm:1500}`. Classification: top
  check 900 + 1500 = 2400 ≤ 2500 passes; no overlap consequence. Effects:
  `[update Ow {heightMm:1500}]`. Post-state: Ow as base except
  heightMm:1500. Outline unchanged (vertical extent is invisible in plan).
  Undo: `[update Ow {heightMm:1300}]`.
- (j) depthMm: command `update Ow {depthMm:200}`. Classification: depth ==
  host thickness is legal; the cut becomes full-thickness and DISCONNECTS
  the band; strict fit unchanged ([1400,2600]); all checks pass. Effects:
  `[update Ow {depthMm:200}]`. Post-state: Ow as base except depthMm:200.
  Outline TWO components `(0,-100) (1400,-100) (1400,100) (0,100)` and
  `(2600,-100) (4000,-100) (4000,100) (2600,100)`. Undo:
  `[update Ow {depthMm:100}]` restoring the single-ring F26 outline.
- (k) swingDeg: command `update Ow {swingDeg:90}`. Classification: within
  [0°,180°]; swing is display-relevant only; all checks pass. Effects:
  `[update Ow {swingDeg:90}]`. Post-state: Ow as base except swingDeg:90.
  Outline unchanged. Undo: `[update Ow {swingDeg:45}]`.
- Immutability note: NO update vector exists for `hostWallId` or levelId —
  both are rejected per R4.3.5/R4.0.2 (vectors in F34). Every accepted
  update above is one atomic batch whose undo is exactly the inverse
  update restoring the prior authoritative field value.

#### F38 — Cross-level opening isolation (complete vectors)
- Before: levels L1 and L2 exist. Wall W1 {levelId:L1, start:(0,0),
  end:(4000,0), thicknessMm:200, heightMm:2500, style:S0}; wall W2
  {levelId:L2, start:(0,0), end:(4000,0), thicknessMm:200, heightMm:2700,
  style:S0} — IDENTICAL plan geometry stacked across levels (F24
  arrangement); no joins; no openings.
- Step 1 command: create Opening Oa supplied:idOa {kind:door,
  hostWallId:W1, u:1/2, widthMm:900, heightMm:2100, sillMm:0, depthMm:200,
  side:Left, hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}.
- Step 1 classification: strict fit [1550,2450] ⊂ (0,4000) on W1; top
  2100 ≤ 2500; depth ≤ thickness; NO cross-level comparison takes place
  (R4.0.2). Commits.
- Step 1 effects: `[create Opening Oa supplied:idOa {kind:door,
  hostWallId:W1, u:1/2, widthMm:900, heightMm:2100, sillMm:0, depthMm:200,
  side:Left, hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}]`.
- Step 1 authoritative post-state: W1 {levelId:L1, start:(0,0),
  end:(4000,0), thicknessMm:200, heightMm:2500, style:S0}; W2 {levelId:L2,
  start:(0,0), end:(4000,0), thicknessMm:200, heightMm:2700, style:S0};
  Oa as commanded, effective level L1 through W1; no other openings.
- Step 1 derived geometry (exact, PER LEVEL): L1's occupied region is W1
  minus Oa — two components `(0,-100) (1550,-100) (1550,100) (0,100)` and
  `(2450,-100) (4000,-100) (4000,100) (2450,100)`; L2's occupied region is
  the untouched bare rectangle `(0,-100) (4000,-100) (4000,100) (0,100)`.
- Step 1 undo: inverse effects `[delete Opening Oa]` restoring both levels'
  regions to bare rectangles.
- Step 2 command: create Opening Ob supplied:idOb {kind:door,
  hostWallId:W2, u:1/2, widthMm:900, heightMm:2100, sillMm:0, depthMm:200,
  side:Left, hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}.
- Step 2 classification: the apertures of Oa and Ob have IDENTICAL plan
  footprints and vertical intervals, yet they live on DIFFERENT hosts on
  different levels → openings are never compared across hosts or levels
  (R4.0.2/R4.1.4): `opening-overlap` is not triggered. All of Ob's own
  checks pass against W2 (top 2100 ≤ 2700). Commits.
- Step 2 effects: `[create Opening Ob supplied:idOb {kind:door,
  hostWallId:W2, u:1/2, widthMm:900, heightMm:2100, sillMm:0, depthMm:200,
  side:Left, hinge:nearStart, opensToward:Right, swingDeg:90, style:S0}]`.
- Step 2 authoritative post-state: W1 hosting Oa (effective level L1), W2
  hosting Ob (effective level L2); every field as commanded; W1/W2
  otherwise unchanged.
- Step 2 derived geometry (exact, PER LEVEL): each level's occupied region
  is computed SEPARATELY and both now show the SAME full-thickness cut
  outline — L1: `(0,-100) (1550,-100) (1550,100) (0,100)` and `(2450,-100)
  (4000,-100) (4000,100) (2450,100)` (W1 minus Oa); L2: the identical two
  components (W2 minus Ob). The two cuts never merge into one
  document-wide region.
- Order invariance: performing the steps in reverse order yields the same
  classifications and identical canonical per-level regions.
- Undo boundaries: each step is an independent batch with inverse-effect
  undo (`[delete Opening Oa]`, `[delete Opening Ob]` respectively);
  undoing step 2 leaves Oa and W1 untouched, and vice versa.

### 4.6 Boundaries and exclusions

Owned elsewhere: pointer-event mapping, gesture lifecycle, snapping, and
selection (sections 6–7, issue 0003); room topology and reconciliation, which
openings never influence (section 5, issue 0002, bound by R4.2.5); operation
framing, counters, encoding (format specification, issue 0004); representable
bounds, precision, near-degeneracy (issues 0022/0030); benchmark corpora and
thresholds for opening workloads (issue 0032); viewport/render/input budgets
including glyph and mesh cost (issue 0033); style catalogs, asset content,
and license display (issues 0021/0026, section 10). Excluded from v0.1:
curved or non-straight hosts; openings attached to joins rather than wall
bodies; nested openings; openings spanning multiple walls; automatic
relocation of apertures during host edits (u is kept by design, R4.3.2);
default sourcing of any opening parameter (R4.0.4); and any numerical
performance claim in this specification.

## 5. Rooms

Status: **Normative**. Rules are identified as
`R5.*`. All coordinates are millimetres; areas are square millimetres. All
topology is defined in exact real arithmetic (R3.0.8); fixture outputs are
exact semantic oracles interpreted under the numerical model's
conformance relation and do not mandate bit-identical
implementation output. This section depends only on sections 1–3; it does not
depend on section 4 (openings), whose R4.2.5 independently states the same
enclosure exclusion.

### 5.1 Topology and detection

- **R5.1.1 Per-level free space.** For each level L, let O_L be the occupied
  region over all walls with levelId L, defined exactly as in R3.2.1–R3.2.3.
  The free space of L is F_L = ℝ² ∖ O_L, an open set. Levels never interact:
  detection runs independently per level (R3.0.6).
- **R5.1.2 Rooms, exterior, and simple-ring decomposition.** A room face is
  a bounded connected component of F_L. The unique unbounded component of
  F_L is EXTERIOR and is never a room. Detection decomposes every arrangement
  so that each face is bounded by ONE simple closed exterior ring (plus
  separate hole rings per R5.1.4): a point-pinch — free regions meeting only
  at an occupied boundary point — is SPLIT into separate simple-ring faces
  at that point rather than forming a figure-eight face (F35). Connectivity
  is judged within the open set F_L: two regions touching only at boundary
  points (points of O_L) are distinct faces.
- **R5.1.3 Face normalization and total ordering.** A face boundary consists
  of straight segments inherited from wall occupied geometry. Each face
  canonically normalizes exactly as in R3.2.4: exterior ring
  counter-clockwise, hole rings clockwise, duplicate and collinear vertices
  removed, open rings rotated to begin at their lexicographically least
  vertex (x first, then y), holes sorted by canonical vertex tuple. The
  result is invariant under wall reversal. Faces are TOTALLY ordered by the
  full canonical component ordering: comparison of canonical outer-ring
  vertex sequences, then, on equality, hole-ring tuples in their canonical
  order — the same comparison R3.2.4 defines for disconnected components.
  This order is deterministic and is the iteration order for all derived
  room output.
- **R5.1.4 Holes and nesting.** A face may enclose other faces. An enclosed
  face is its own component of F_L and never belongs to another room; the
  enclosing face's canonical form carries each enclosed loop as a hole ring.
  Nesting depth is derived data with no v0.1 limit beyond what the wall
  arrangement produces (F27).
- **R5.1.5 Openings never affect enclosure.** Enclosure derives SOLELY from
  O_L per R3.2. Section 4 entities MUST NOT cut, merge, split, or otherwise
  alter faces in v0.1 (R4.2.5, F36). No rule or fixture in this section reads
  any section 4 state.
- **R5.1.6 Degenerate cases.** A level with no walls has exactly one
  (unbounded) component and no faces. Components of the OPEN set F_L always
  have strictly positive exact area, so zero-area pseudo-faces cannot exist
  and need no rejection path. Faces may be non-convex; pinched shapes are
  split per R5.1.2 before any further processing.
- **R5.1.7 No manual room geometry.** v0.1 has no manually drawn, edited, or
  overridden room polygon. Face geometry and areas are derived (R3.0.7);
  any attempt to persist them as authoritative document state MUST be
  rejected by conforming validators (`manual-room-geometry-unsupported`).
  Detected faces EXIST whether or not any metadata record associates with
  them (R5.1.8); records are optional persistent metadata layered on top of
  detection (5.2), never a precondition for a face to exist.
- **R5.1.8 No persisted or automatic face identities.** Faces carry NO
  identifiers of any kind: none are minted by detection, none appear in
  operations, and none are persistable in authoritative or cached document
  state. Implementations MAY key faces ephemerally within a single
  reconciliation pass for internal bookkeeping; such keys are ephemeral,
  implementation-private, non-authoritative, and observable only through the
  derived projection of that pass. Faces exist whether or not any metadata
  record associates with them.
- **R5.1.9 Recomputation locality (transitive containment-and-overlap
  component closure).** Partition each level's walls into join-connected
  components (R3.4.8), each owning its R3.2 occupied region. DISTINCT
  components are LINKED when their occupied regions stand in either
  relation, evaluated over the union of the pre-edit and post-edit states:
  - spatial CONTAINMENT — one region lies within another's rings, tying
    ancestor faces to descendant faces (for example a courtyard face
    containing an island loop's walls); or
  - INTERSECTION WITH POSITIVE AREA — the regions overlap positively even
    though NO join connects their walls (the `wall-area-overlap` class of
    R3.1.2), so unjoined overlapping clusters are linked.
  Starting from the touched components, take the transitive closure under
  both relations; EXACTLY the faces whose closures meet an occupied region
  of the closed set recompute — this is the closure referenced by R3.4.8
  and exercised through issue 0032. Whole-level recomputation is permitted
  ONLY when this proven closure actually reaches every face of the level;
  unjustified whole-level scope violates this rule, and document-wide
  (cross-level) invalidation is forbidden because levels never interact
  (R5.1.1) — except that an R5.2.2 `levelId` move projects exactly its
  source and target levels and no others. Benchmark corpora, percentiles,
  memory bounds, and thresholds are owned by tracker issue 0032; none are
  specified here.
  **Numerical routing dependencies (ED-0111).** The permitted set also
  includes every face whose boundary is routed through a topology cell
  (ED-0110) that the edit makes hot or cold, and every face bounded by an
  edge whose routing changed. Such a cell can be produced by geometry that
  contributes no occupied region of its own — a wall smaller than one cell
  — so it is not reachable through the component relation above. Faces
  that are rebuilt need not belong to the closed set's components: an
  unchanged component may be READ as boundary context (a room enclosed by
  corner-touching, unlinked walls) without its own faces recomputing.

### 5.2 Identity and metadata reconciliation

- **R5.2.1 Record entity and data classes.** A room record is an
  authoritative document entity whose ENTIRE authoritative field set is:
  its command-supplied stable UUID; `levelId`; `seedMm`, a finite editable
  coordinate pair; an optional display name (a string; absent or present,
  and present includes the empty string); an optional floor-style reference;
  an optional ceiling-style reference. Nothing else about a room is
  authoritative. Face polygons, nesting, candidacy, effective association,
  record status, areas, label anchors, and surface footprints are DERIVED
  data (R3.0.7): rebuildable, never directly edited, persistable only in a
  non-authoritative cache namespace.
- **R5.2.2 Record commands and validation.** Exactly three command kinds
  touch records — create, update, delete — and each is exactly one atomic
  batch and one undo boundary with no incidental effects:
  - CREATE requires an existing `levelId`, finite `seedMm`, and a
    command-SUPPLIED identifier unused anywhere in the document (no
    automatic id assignment ever occurs for rooms); a collision rejects
    deterministically as invalid input with an empty batch, no state change,
    and no undo entry (the §2.7 pattern). Name and style references are
    optional; NO default sourcing exists (R3.0.6 discipline).
  - UPDATE targets an existing record id; targeting an unknown id rejects
    deterministically as invalid input with an empty batch. Settable fields:
    `seedMm`, name, either style reference, `levelId`. A non-finite seed
    rejects (`nonfinite-coordinate`); an empty-string style reference
    rejects (`invalid-style-reference`); setting a field to null CLEARS it
    to absent. An `levelId` update moving a record between levels
    recomputes and reprojects BOTH the source and the target level's faces
    in one batch, and undo restores both projections; no other level is
    touched.
  - DELETE targets an existing record id under the same unknown-target rule
    and removes it authoritatively.
  Seed location NEVER rejects or warns (R5.2.4).
- **R5.2.3 Candidate containment.** Candidacy is pure geometry: a record on
  level L is a CANDIDATE of face f if and only if its seed lies strictly
  inside f (an exact predicate: the seed is an interior point of that
  component). A record's seed yields at most one candidate face. Candidate
  sets drive nothing by themselves; only R5.2.4 promotes candidacy to
  effective association.
- **R5.2.4 Effective association, tie-break, and exclusive status.** For
  each face, among its candidates EXACTLY ONE record effectively associates:
  the one with the LOWEST stable UUID under the same stable-UUID total
  order this specification already uses for §2.2–§2.6 tie-breaks. No textual,
  case, or serialization form is specified here; the order is the existing
  semantic total order over stable UUIDs, and its physical realization is
  owned by the format specification. Every record holds EXACTLY ONE derived
  status at any time, mutually exclusive by construction:
  - `associated` — it is its face's effective winner;
  - `superseded` — it is a candidate of some face but lost that face's
    tie-break;
  - `seed-on-boundary` / `seed-in-wall` / `seed-in-exterior` — its seed lies
    on ∂O_L, inside O_L, or in the unbounded component, so it has no face.
  Status changes are pure projection when seeds or faces move; they are
  never authored by operations, and no record can hold two statuses at once.
- **R5.2.5 No automatic operations.** Detection and reconciliation emit ZERO
  operations. Every wall mutation batch contains no Room effects regardless
  of how candidacy, status, faces, or areas change; those changes are pure
  projection. Room operations occur ONLY through the R5.2.2 commands.
- **R5.2.6 Lifecycle.** With candidacy and effective association fixed by
  R5.2.3/R5.2.4, all transitions follow deterministically and emit nothing
  (R5.2.5):
  - Split: one face becomes several; each record candidates for the child
    face strictly containing its seed; a record with no containing child
    takes the matching non-candidate status per R5.2.4 (F28).
  - Merge: several faces become one; multiple seeds in the merged face
    resolve by the R5.2.4 tie-break, losers becoming `superseded` (F29).
  - Disappear: a face vanishes into another face or the exterior; records
    reclassify per R5.2.4, possibly `seed-in-exterior`, and persist (F30).
  - Reappear: a face returns beneath a seed; the record re-associates
    automatically as projection (F30 undo, F31).
- **R5.2.7 Deletion prevents resurrection.** Deleting a record removes it
  authoritatively; no later wall edit, reconciliation pass, undo of OTHER
  batches, or save/reload cycle regenerates it (F34). Only an explicit
  create — or undo of the deletion batch itself, restoring an equivalent
  record per R3.4.7 (F37j) — brings a record into existence.
- **R5.2.8 Level interaction.** Records live on levels; §2.3 rejects
  deleting a level containing any entity, room records count among the
  blocking kinds reported (F38), and moving a record to another level is the
  R5.2.2 `levelId` update, not a deletion.

### 5.3 Measurement, labels, styles, and surfaces

- **R5.3.1 Exact area.** A face's area is its exact REAL measure in mm²,
  determined by the canonical rings; a holed face subtracts hole areas
  (F27). Area is derived, deterministic, invariant under wall reversal
  (R3.2.4), and never authoritative (R3.2.5). The general guarantee is
  exact-real: no representation, precision, or rationality claim is made for
  arbitrary inputs — binding precision and the conformance relation belong
  to issues 0022/0030. Exact-rational (in these fixtures, exact-integer)
  values are asserted ONLY for the listed section 5.4 fixtures, whose
  coordinates make the measures rational.
- **R5.3.2 Deterministic display rounding; fixed unit.** Displayed area is a
  pure function of the exact area A mm²: convert to m² by exact division by
  1,000,000, then round to nearest, resolving EXACT halves to the EVEN last
  digit, at exactly two decimal places. The room-area label is ALWAYS
  rendered by the core in m² with two decimals regardless of the §1.1
  display-unit preference. The experimental editor's imperial presentation
  in §11.1 is an exception; it does not change this core string or CLI exports.
  Exact areas are measures of open set components and
  are strictly positive (R5.1.6): display input can never be signed, no
  negative-area display case exists, and the output MUST never render as
  negative zero (`-0.00` is forbidden; any value below 0.005 m² displays as
  `0.00`). Examples: A = 10,640,000 → `10.64 m²`;
  A = 38,200,000 → `38.20 m²`; tie rounding down: A = 30,245,000 →
  30.245 m² → `30.24 m²` (4 is even); tie rounding up: A = 10,655,000 →
  10.655 m² → `10.66 m²` (6 is even). Display rounding and unit choice are
  presentation only and never feed back into geometry, validation, or
  persisted state.
- **R5.3.3 Labels.** Label behavior is deterministic on both sides:
  - Absence: a face with NO candidates shows NO label; a level with no faces
    shows no room labels anywhere.
  - Placement: each face with at least one candidate shows exactly one
    label, anchored at its effective record's `seedMm` (strictly inside the
    face by R5.2.3, so the anchor never needs clamping).
  - Text: the effective record's name when present AND non-empty; otherwise
    the fallback token `Room`. Absent and empty-string names are distinct
    authoritatively but identical in display; core preserves the distinction
    (F37d).
  - The displayed area per R5.3.2 always accompanies the label.
    Non-candidate records (`superseded` is impossible without candidacy, so
    this means `seed-on-boundary`, `seed-in-wall`, `seed-in-exterior`) have
    NO deterministic room label of any kind; any visualization of them is
    OPTIONAL view-local diagnostics outside this specification's normative
    surface — core exposes only ids, authoritative fields, and derived
    status. Label visual style is local view state (R3.0.7).
- **R5.3.4 Floor and ceiling surfaces.** Each ASSOCIATED face derives one
  floor surface and one ceiling surface whose footprint is the face MINUS
  its hole rings (an annulus footprint when holes exist): horizontal at the
  owning level's elevation and at elevation plus ceiling height
  respectively (§2.1 geometry convention). Floor styling comes solely from
  the effective record's floor-style reference and ceiling styling solely
  from its ceiling-style reference. Style references follow wall-reference
  discipline: optional, non-empty opaque tokens with NO registry resolution
  in v0.1 — an empty-string reference rejects (`invalid-style-reference`,
  R5.2.2), an absent one means unspecified style, and neither is ever
  inherited from walls, levels, or defaults (explicit-resolution discipline,
  R3.0.6). Faces without an associated record derive no styled surfaces in
  v0.1.
- **R5.3.5 Non-authority.** Everything in 5.3 is derived or view-local; no
  rule here creates or mutates authoritative state.

### 5.4 Normative fixtures

Conventions follow section 3.5: all coordinates millimetres, exact real
values (R3.0.8); every wall uses style token `S0` with explicitly named
style on creation; `supplied:` marks fresh command-supplied unused
identifiers; rings are canonical CCW exteriors and CW holes, open, least
vertex first (R3.2.4, R5.1.3); ordering language is abstract (R3.1.5).
Room identifiers appear as tokens `R1`, `R2`, … with the asserted stable-UUID
total order `R1 < R2 < …` (R5.2.4); this asserts semantic order only, not
representation. Fixture walls are t=200 h=2500 unless stated. Every fixture
below also asserts ZERO room operations unless the command itself creates,
updates, or deletes a record.

Base arrangement B4 (reused by F25, F28–F31, F33, F34, F36–F38): level L1;
walls
`W1` (0,0)→(4000,0), `W2` (4000,0)→(4000,3000), `W3` (4000,3000)→(0,3000),
`W4` (0,3000)→(0,0); corner joins `J1` {(W4,End),(W1,Start)},
`J2` {(W1,End),(W2,Start)}, `J3` {(W2,End),(W3,Start)}, `J4`
{(W3,End),(W4,Start)}. Its single interior face has canonical ring
`(100,100) (3900,100) (3900,2900) (100,2900)` and exact area
3,800 × 2,800 = 10,640,000 mm² (display `10.64 m²`).

#### F25 — Closed room: record creation and strict association
- Before: base arrangement B4; no room records.
- Command: create room record `R1` supplied:idR1 {levelId:L1,
  seed:(2000,1500), name:"Living"}.
- Classification: seed strictly inside the single face → associated; no
  flags.
- Effects: `[create Room R1 {levelId:L1, seed:(2000,1500), name:"Living"}]`.
- After: one face, ring and area as in B4; label anchored at (2000,1500),
  text "Living", displayed area `10.64 m²` (R5.3.3).
- Undo: deletes R1; the face is unchanged.

#### F26 — Non-convex L-shaped room
- Before: level L1; six-wall L loop: `W1` (0,0)→(8000,0),
  `W2` (8000,0)→(8000,3000), `W3` (8000,3000)→(3000,3000),
  `W4` (3000,3000)→(3000,6000), `W5` (3000,6000)→(0,6000),
  `W6` (0,6000)→(0,0); corner joins J1–J6.
- Command: create room record `R1` supplied:idR1 {levelId:L1,
  seed:(1500,1500), name:"L-room"}.
- After: single non-convex face, canonical ring `(100,100) (7900,100)
  (7900,2900) (3000,2900) (2900,3000) (2900,5900) (100,5900)`. The diagonal
  edge `(3000,2900)→(2900,3000)` is the room-facing edge of the J3 patch
  hull diamond `(3000,2900) (3100,3000) (3000,3100) (2900,3000)` — both
  walls terminate at (3000,3000), so the hull alone seals the inner corner;
  face extraction MUST account for join patches (R5.1.1, R3.2.3). Exact
  area 21,840,000 + 8,400,000 + 5,000 = 30,245,000 mm²; display
  `30.24 m²` (30.245 is an exact tie; ties round to even, R5.3.2);
  associated, no flags.
- Undo: deletes R1.

#### F27 — Holed courtyard and nested island room
- Before: level L1; outer loop `W1`–`W4`: rectangle axes x∈{0,8000},
  y∈{0,6000} (corners (0,0), (8000,0), (8000,6000), (0,6000)); island loop
  `I1` (2500,2000)→(5500,2000), `I2` (5500,2000)→(5500,4000),
  `I3` (5500,4000)→(2500,4000), `I4` (2500,4000)→(2500,2000); corner joins
  for each loop. Two faces: the courtyard annulus between the loops and the
  island interior.
- Commands (two independent batches): create `Rc` supplied:idRc
  {levelId:L1, seed:(1200,3000), name:"Courtyard"}; create `Ri` supplied:idRi
  {levelId:L1, seed:(4000,3000), name:"Island"}.
- After: courtyard face exterior ring `(100,100) (7900,100) (7900,5900)
  (100,5900)` with clockwise EIGHT-vertex hole `(2400,2000) (2400,4000)
  (2500,4100) (5500,4100) (5600,4000) (5600,2000) (5500,1900) (2500,1900)`
  (least vertex first). The hole's corners are CHAMFERED for the same reason
  F26's inner corner is: each island corner is two walls terminating at a
  shared point, so the R3.2.3 patch is the convex hull of their perpendicular
  cross-sections — at (2500,2000) the diamond `(2500,1900) (2600,2000)
  (2500,2100) (2400,2000)` — which seals half of the 100 × 100 corner square
  and leaves the other half, a 5,000 mm² triangle, in the free region. Patches
  need no miter construction (R3.2.3), so the square corner is never restored.
  Hole area 3,200 × 2,200 − 4 × 5,000 = 7,020,000 mm²; courtyard exact area
  45,240,000 − 7,020,000 = 38,220,000 mm²; display `38.22 m²`.
  Island face ring `(2600,2100) (5400,2100) (5400,3900) (2600,3900)`;
  exact area 2,800 × 1,800 = 5,040,000 mm²; display `5.04 m²` — the loop's
  INNER corners are unaffected, because a diamond reaches only 100 mm from its
  node along each axis and the interior corner stands 100 mm from it in BOTH.
  Each record associates with its own face; the island lies inside the
  courtyard's hole yet they are distinct rooms (R5.1.2, R5.1.4).
- Undo: deletes Ri, then Rc independently.

#### F28 — Partition splits a recorded room (zero room operations)
- Before: F25 post-state (`R1` seed (2000,1500)).
- Command: create wall `P` supplied:idP: (1000,0)→(1000,3000), t=200 h=2500
  S0.
- Classification: `P.Start` endpoint-on-interior of `W1` (axes
  non-collinear) → split `W1` at (1000,0); `P.End` endpoint-on-interior of
  `W3` → split `W3` at (1000,3000); patch diamonds fully covered by
  rectangles; no other contacts.
- Effects: `[create Wall B supplied:idB {start:(1000,0), end:(4000,0),
  t:200, h:2500, style:S0}; create Wall C supplied:idC {start:(1000,3000),
  end:(0,3000), t:200, h:2500, style:S0}; create Wall P supplied:idP
  {start:(1000,0), end:(1000,3000), t:200, h:2500, style:S0}; create Join
  Ja {(W1,End),(B,Start),(P,Start)}; create Join Jb {(W3,End),(C,Start),
  (P,End)}; update W1 {end:(1000,0)}; update W3 {end:(1000,3000)}]`.
- After: left face ring `(100,100) (900,100) (900,2900) (100,2900)`, exact
  area 800 × 2,800 = 2,240,000 mm², unrecorded; right face ring
  `(1100,100) (3900,100) (3900,2900) (1100,2900)`, exact area
  2,800 × 2,800 = 7,840,000 mm²; `R1` follows the right child (seed strictly
  contained); display `7.84 m²`. ZERO room operations (R5.2.5, R5.2.6
  split).
- Undo: inverse batch restores the F25 post-state projection exactly; `R1`
  re-spans the single face as projection.

#### F29 — Merge with lowest-stable-UUID tie-break
- Before: F28 post-state plus record `R2` supplied earlier {levelId:L1,
  seed:(500,1500), name:"Bath"} associated with the left face; stable-UUID
  total order `R1 < R2` (R5.2.4).
- Command: delete wall `P`.
- Effects: `[delete Wall P; update Join Ja {members = (W1,End),(B,Start)};
  update Join Jb {members = (W3,End),(C,Start)}]` — both joins keep two
  members and survive; survivors are never auto-merged (R3.4.5).
- After: one face again, ring `(100,100) (3900,100) (3900,2900) (100,2900)`,
  area 10,640,000 mm², display `10.64 m²`. Both seeds lie strictly inside:
  `R1` (lowest stable UUID) effectively associates; `R2` persists unchanged
  with exclusive status `superseded`. ZERO room operations (R5.2.6 merge).
- Undo: recreates `P` and restores memberships; the faces split again and
  `R2` returns to `associated`, losing `superseded` — pure projection.

#### F30 — Room disappears into the exterior (seed-in-exterior)
- Before: F25 post-state (`R1` seed (2000,1500) associated).
- Command: delete wall `W2`.
- Effects: `[delete Wall W2; delete Join J2; delete Join J3]` (both fall
  below two members; F19 pattern).
- After: the interior connects to the exterior through the gap between
  `W1`'s and `W3`'s bands; NO bounded face remains on L1. `R1` persists with
  exclusive status `seed-in-exterior` (R5.2.4); ZERO room operations
  (R5.2.6 disappear).
- Undo: recreates `W2`, `J2`, `J3`; the face reappears and `R1`
  re-associates automatically (R5.2.6 reappear).

#### F31 — Room reappears after rebuilding the closure
- Before: F30 post-state (no bounded faces; `R1` flagged
  `seed-in-exterior`).
- Command: create wall `W2'` supplied:idW2: (4000,0)→(4000,3000), t=200
  h=2500 S0.
- Classification: endpoint-endpoint contacts at (4000,0) with `W1`.End and
  at (4000,3000) with `W3`.Start → auto-joins; the deleted `J2`/`J3` are
  gone, so these are NEW join entities.
- Effects: `[create Wall W2' supplied:idW2 {start:(4000,0), end:(4000,3000),
  t:200, h:2500, style:S0}; create Join J2' {(W1,End),(W2',Start)}; create
  Join J3' {(W2',End),(W3,Start)}]`.
- After: the original face reappears with identical canonical ring and area
  10,640,000 mm²; `R1` re-associates as pure projection; ZERO room
  operations. New join identities differ from the deleted ones; nothing was
  resurrected (R5.2.7 discipline applied to structural entities too).
- Undo: deletes `W2'`, `J2'`, `J3'`, restoring the F30 post-state.

#### F32 — Non-candidate seed statuses (three compact cases)
Before for each: F25 walls without records; each command is an independent
batch creating `R1` supplied:idR1 {levelId:L1, seed:S, name:"X"}.
- (a) S = (2000,100): the seed lies on ∂O_L (the inner edge of `W1`'s band)
  → accepted; exclusive status `seed-on-boundary`.
- (b) S = (2000,0): the seed lies in the interior of O_L → accepted;
  exclusive status `seed-in-wall`.
- (c) S = (10000,1500): the seed lies in the unbounded component →
  accepted; exclusive status `seed-in-exterior`.
Each effect is `[create Room R1 {levelId:L1, seed:S, name:"X"}]`; faces and
areas are unchanged; undo deletes R1. No case rejects or warns (R5.2.2,
R5.2.4).

#### F33 — Two seeds, one face: tie-break at creation time
- Before: F25 post-state (`R1` seed (2000,1500), associated).
- Command: create room record `R2` supplied:idR2 {levelId:L1,
  seed:(3000,1500), name:"Study"}.
- After: both seeds strictly inside the same face — two candidates, one
  effective winner; `R1` (lower stable UUID) keeps association, `R2`
  persists with exclusive status `superseded`; the label shows `R1`'s
  anchor/text. Effect: `[create Room R2 {levelId:L1,
  seed:(3000,1500), name:"Study"}]`; no other effects (R5.2.4).
- Undo: deletes R2; `R1` unaffected.

#### F34 — Deletion prevents resurrection
- Before: F25 post-state.
- Step 1: delete `R1` — effects `[delete Room R1]`; no faces change; no
  record exists afterward.
- Step 2: drag `W1`'s body by (0,-500) (F22 command against the now
  record-less state) — commits with `[update W1 …; update W2 …; update
  W4 …]` and zero room effects; NO record reappears.
- Step 3: undo step 2 — walls restore; still no record.
- Step 4: undo step 1 — restores an equivalent `R1` (undo OF THE DELETION
  BATCH itself, R5.2.7/R3.4.7); association resumes as projection.
- Invariant: across steps 2–3 every reconciliation pass leaves the record
  count at zero; resurrection happens ONLY via explicit create or the
  deletion's own undo.

#### F35 — Point-pinch: lobes touching at one occupied point stay distinct
- Before: level L1; loop A: `A1` (0,0)→(2000,0), `A2` (2000,0)→(2000,2000),
  `A3` (2000,2000)→(0,2000), `A4` (0,2000)→(0,0); loop B: `B1`
  (2000,2000)→(4000,2000), `B2` (4000,2000)→(4000,4000), `B3`
  (4000,4000)→(2000,4000), `B4'` (2000,4000)→(2000,2000); all t=200 h=2500
  S0. At the shared point (2000,2000) the contact merges into ONE four-member
  join `K` {(A2,End),(A3,Start),(B1,Start),(B4',End)} (R3.3.5, F14
  pattern); other corners have their own joins.
- Pinch occupancy: despite mere axis ENDPOINT contact at (2000,2000), the
  occupied bands overlap in ALL FOUR 100 × 100 quadrants around that point —
  `A2 ∩ A3` occupy `(1900,1900)–(2000,2000)` (SW), `A2 ∩ B1` occupy
  `(2000,1900)–(2100,2000)` (SE), `A3 ∩ B4'` occupy `(1900,2000)–(2000,2100)`
  (NW), and `B1 ∩ B4'` occupy `(2000,2000)–(2100,2100)` (NE) — because each
  wall's band extends a full thickness on both sides of its axis while its
  along-axis extent reaches the shared endpoint. The K patch hull diamond
  `(1900,2000) (2000,2100) (2100,2000) (2000,1900)` spans the diagonals
  between these squares; the occupied union near the pinch is therefore the
  four bands' union, which already covers every quadrant, with the diamond
  adding no uncovered area. Sample points (1950,1950), (2050,1950),
  (1950,2050), (2050,2050) all lie in O_L.
- Resulting free-space components: EXACTLY TWO bounded faces — A interior
  ring `(100,100) (1900,100) (1900,1900) (100,1900)`, exact area
  1,800 × 1,800 = 3,240,000 mm²; B interior ring `(2100,2100) (3900,2100)
  (3900,3900) (2100,3900)`, exact area 3,240,000 mm². No sliver or
  figure-eight face arises: the pinch is resolved by R5.1.2's simple-ring
  decomposition into these two faces, which share no free-space point.
  Display `3.24 m²` each.
- Records: creating `Ra` {seed:(1000,1000)} and `Rb` {seed:(3000,3000)}
  associates each with its own lobe; neither affects the other.
- Undo: independent batches restore prior states.

#### F39 — Disconnected enclosing/nested loops mutated in both directions (locality proof)
- Before: level L1 carries the F27 arrangement — outer loop `W1`–`W4`
  (rectangle corners (0,0), (8000,0), (8000,6000), (0,6000)) and island loop
  `I1`–`I4` ((2500,2000)–(5500,4000)) with records `Rc` {seed:(1200,3000)}
  and `Ri` {seed:(4000,3000)} — PLUS a remote third loop `C1`
  (12000,0)→(15000,0), `C2` (15000,0)→(15000,2000), `C3`
  (15000,2000)→(12000,2000), `C4` (12000,2000)→(12000,0) with record `Rd`
  {seed:(13500,1000)}. Three faces: courtyard annulus 38,220,000 mm²,
  island 5,040,000 mm², remote room `(12100,100) (14900,100) (14900,1900)
  (12100,1900)` = 2,800 × 1,800 = 5,040,000 mm².
- Step 1 (nested → enclosing): drag `I1`'s body by (0,-300).
  Effects: `[update I1 {start:(2500,1700), end:(5500,1700)}; update I2
  {start:(5500,1700)}; update I4 {end:(2500,1700)}]`.
  Derived delta: island face ring becomes `(2600,1800) (5400,1800)
  (5400,3900) (2600,3900)`, area 2,800 × 2,100 = 5,880,000 mm²; courtyard
  hole grows to the block `(2400,1600)–(5600,4100)` less its four
  R3.2.3 corner chamfers, area 8,000,000 − 4 × 5,000 = 7,980,000 mm², so the
  courtyard is 45,240,000 − 7,980,000 = 37,260,000 mm²; both seeds remain
  candidates; displays `5.88 m²` / `37.26 m²`. ZERO room operations.
- Step 2 (enclosing → nested): undo step 1; then drag `W1`'s body by
  (0,-500). Effects: `[update W1 {start:(0,-500), end:(8000,-500)}; update
  W2 {start:(8000,-500)}; update W4 {end:(0,-500)}]`.
  Derived delta: courtyard exterior region extends to y ∈ (−400, 5900):
  annulus = 7,800 × 6,300 − 7,020,000 = 42,120,000 mm² (`42.12 m²`);
  island face UNCHANGED at 5,040,000 mm²; ZERO room operations.
- Locality assertions (normative under R5.1.9): each step's proven closure is
  exactly `{courtyard, island}` — step 1 reaches the island directly and the
  courtyard as containment ancestor; step 2 reaches the courtyard directly
  and the island as transitive descendant through the courtyard's hole. The
  remote room is in NEITHER step's closure: its walls neither join, overlap,
  intersect, nor stand in any containment relation to the touched
  components, so recomputing it — or invoking whole-level scope when a
  disjoint same-level face exists outside the closure — violates R5.1.9.
  No document-wide work occurs; levels other than L1 are untouched.
- Undo: each step is one batch/undo boundary; undo restores wall positions
  and both projections without room operations.

#### F36 — Opening presence leaves topology unchanged (abstract vector)
- Purpose: pin R5.1.5 independently of the normative section 4 schema and
  alongside its matching R4.2.5 exclusion.
- Before: base arrangement B4 with record `R1` associated (F25 post-state),
  plus any valid live opening hosted on wall `W2` under section 4.
- Command: any accepted section 4 create, edit, or delete gesture for that
  opening within its host.
- Effect: faces, rings, areas, candidacy, statuses, labels, and surfaces are
  UNCHANGED in every case: the derived room projection after the command is
  SEMANTICALLY EQUIVALENT to the projection before it under the future
  numerical model's conformance relation (R3.0.8, issues 0022/0030) — no
  byte-level identity claim is made. ZERO room operations. If section 4
  rejects the gesture, that rejection applies independently and still
  produces no room effect.
- Undo: per section 8/section 4 semantics; the room projection remains
  unchanged across the undo boundary.

#### F37 — Room metadata edit vectors (seed/name/styles/validation/undo)
Before for every step: F25 post-state (`R1` seed (2000,1500) name "Living",
no style references). Each step is one independent batch/undo boundary;
faces never change unless stated.
- (a) Supplied-id collision: create `Rx` supplied:idR1 {levelId:L1,
  seed:(2000,1500)} → rejected as invalid input; empty batch; state
  unchanged; no undo entry (§2.7 pattern).
- (b) Seed move within face: `[update Room R1 {seed:(3500,2500)}]` →
  accepted; `R1` stays `associated` (still strictly inside the same face);
  label anchor moves to (3500,2500). Undo restores seed (and anchor as
  projection).
- (c) Seed move out: `[update Room R1 {seed:(10000,1500)}]` → accepted;
  status flips to `seed-in-exterior`; label disappears from the face
  (R5.3.3 absence). Undo restores `associated`.
- (d) Name edit, empty, and clear: `[update Room R1 {name:"Den"}]` → label
  text "Den"; `[update Room R1 {name:""}]` → authoritative name is the
  empty string, display text falls back to `Room` (absent vs empty distinct
  authoritatively, identical on screen, R5.3.3); `[update Room R1
  {name:null}]` → name absent, display unchanged (`Room`). Each undo
  restores the prior field exactly.
- (e) Floor style set/change/clear: `[update Room R1 {floorStyleRef:"F-wood"}]`
  then `{floorStyleRef:"F-tile"}` then `{floorStyleRef:null}` → accepted
  each time; floor surface styling follows, ending unspecified after clear;
  each undo restores the prior reference exactly.
- (f) Ceiling style set/change/clear and undo: `[update Room R1
  {ceilingStyleRef:"C-white"}]` → ceiling surface styled `C-white`; `[update
  Room R1 {ceilingStyleRef:"C-black"}]` → styled `C-black`; `[update Room
  R1 {ceilingStyleRef:null}]` → cleared, ceiling style unspecified; undoing
  the three batches in order restores `C-black`, then `C-white`, then
  absent. Faces, rings, areas, and candidacy never change.
- (g) Invalid style reference: `[update Room R1 {ceilingStyleRef:""}]` →
  reject `invalid-style-reference`; empty batch; state unchanged; no undo
  entry.
- (h) Non-finite seed: `[update Room R1 {seed:(nonfinite, 0)}]` → reject
  `nonfinite-coordinate`; empty batch; state unchanged; no undo entry.
- (i) Level move and undo: before this step only, an additional empty level
  L2 exists. `[update Room R1 {levelId:L2}]` → accepted as ONE batch; `R1`
  now lives on L2 with status per L2's faces there (exterior if none); BOTH
  projections recompute: L1's face loses its label (no candidates) while
  L1's walls, face ring, and area are unchanged, and no third level or
  document-wide work occurs (R5.1.9). Undo of THIS batch restores R1 to L1
  and BOTH projections in one boundary.
- (j) Delete and undo: `[delete Room R1]` → record gone, face persists
  (faces exist without records, R5.1.8); undo of THIS batch recreates an
  equivalent `R1` per R3.4.7 with association resuming as projection
  (R5.2.7).

#### F38 — Level deletion blocked by a room record
- Before: two levels L1, L2 (L2 empty); B4 walls and record `R1` on L1.
- Command: delete level L1.
- Effect: rejected per §2.3 — L1 contains entities — with blocking kinds
  and counts explicitly including `Room: 1` alongside `Wall: 4`,
  `Join: 4`; empty batch; state unchanged; no undo entry.
- Control: deleting L2 instead succeeds (empty, unreferenced); no room data
  is touched.

## 6. Magnetism and snapping

Status: **Normative**. Rules are identified as
`R6.*`. Screen
quantities are logical pixels as defined by section 1.2; plan quantities are
millimetres. Fixture vectors are exact real oracles under the R3.0.8
numerical-policy boundary.

### 6.0 Conventions and state classes

- **R6.0.1 Device-pixel normalization.** Every pointer coordinate arrives in
  device pixels and MUST first be converted to logical pixels through the
  platform device pixel ratio (DPR), then mapped through the inverse view
  transform of section 1.2 into plan coordinates, before any threshold,
  radius, distance, or equality comparison in this section applies. All
  screen-space constants here are logical-pixel values; no rule of this
  section ever compares raw device pixels.
- **R6.0.2 Snap radii.** The snap radius T is 12 logical pixels for mouse and
  stylus pointers and 24 logical pixels for touch pointers, identical on every
  platform. Stylus input is never touch-classified. The plan-space radius is
  T/z for current zoom z; because z is clamped to [0.0001, 100] (section 1.3),
  the plan radius is always finite and positive.
- **R6.0.3 Inclusive boundary; exact-real semantics.** A candidate qualifies
  when its distance from the pointer is less than OR EQUAL TO the applicable
  radius. All distances and equalities in this section are EXACT REAL
  comparisons over exact geometric values; this specification defines NO
  epsilon, fuzzy band, or implementation tolerance. Implementations compare
  SQUARED distances wherever possible so exact rational arithmetic stays
  closed under comparison. How an implementation approximates exact-real
  comparison on floating-point inputs is owned by the numerical model
  (issues 0022/0030). Fixtures below state squared or exactly-representable
  values only and never rely on approximate irrational claims.
- **R6.0.4 Purity; no hysteresis.** Snapping is a pure function of the current
  pointer position, the current view transform, the current document
  projection, and the active snap preferences (R6.4.3). Equal inputs yield
  equal winners. v0.1 defines NO hysteresis, stickiness, or winner memory
  across frames.
- **R6.0.5 Sources and classes.** DOCUMENT-DERIVED snap sources are exactly
  the active level's wall features (extends the section 2.4 matrix to
  magnetism): non-active levels are never snappable. `grid` and `angle`
  targets are LOCAL constructions computed from view state and gesture state,
  not document entities. Placeholder instances are NEVER snap sources; their
  interaction role is selection-only (R7.1.3).
- **R6.0.6 State classes.** Committed post-snap command coordinates are
  authoritative operation inputs. Live previews, unsnapped cursor positions,
  winning-candidate computations, guides, hover, and per-gesture state are
  transient view state: they emit zero operations, appear in no operation log,
  move no causal frontier, and are never persisted.

### 6.1 Candidate inventory and exclusions

- **R6.1.1 Exhaustive inventory.** For each feature below, evaluation computes
  the CLOSEST point on the feature to the pointer's plan position under the
  Euclidean metric:

  | Class | Feature | Closest point |
  |---|---|---|
  | `join-node` | Each join's node point — the centre of its members' topology cell (ED-0112); all member endpoints deduplicate to ONE candidate per join (R6.3.2) | The join point itself |
  | `free-endpoint` | A wall endpoint belonging to no join | The endpoint itself |
  | `edge` | Occupied-boundary segments of active-level walls (derived rings, R3.2) | The closest point of the closed segment |
  | `axis` | Wall axis LINES (the infinite line through the wall's axis; extensions beyond the endpoints included) | The perpendicular projection onto the line, whether on or off the segment |
  | `grid` | Grid points at the active minor spacing s (section 1.5 series) | The nearest lattice point |
  | `angle` | Chain angle targets (R6.2) | Direction constraint only; length by projection onto the constrained ray |

  This inventory is exhaustive: no other target class exists in v0.1.
  Unlike the clamped geometric projection, the `axis` candidate extends to
  the full axis LINE, making centerline alignment reachable beyond a wall's
  endpoints. Because the line is infinite, candidate queries are BOUNDED by the
  VISIBLE REGION: a qualifying projection lies within T/z of an on-viewport
  pointer (its perpendicular distance), so every eligible projection is
  contained in the visible plan rectangle expanded by T/z on all sides —
  a finite interval per axis line that indexed implementations MUST search
  in full. No smaller longitudinal bound (such as extending the segment by
  T/z) is sound: S1 and S4 qualify extensions hundreds of millimetres past
  their walls' endpoints (ownership per the R8.9 matrix).
- **R6.1.2 No centers.** Wall-body centroids, edge or segment midpoints, join
  patch centers, and every other derived center are NOT snap targets in v0.1.
- **R6.1.3 Self-snap exclusions.** Exclusions are by identity and role, NEVER
  by distance. During a gesture, the dragged element excludes its own
  features; the moving endpoint of a chain segment excludes its own anchor
  point and every candidate whose acceptance would produce a zero-length
  segment (the R3.0.5 rejection stands regardless). Anchor coincidence
  filters EVERY class: any candidate whose target point coincides exactly
  with the anchor — free-endpoint, join-node point, edge closest point,
  axis projection, or grid lattice point — is removed from the allowed
  set, never merely deprioritized; a body-dragged wall excludes its own
  edges and axis from snapping candidacy.

### 6.2 Angle targets

- **R6.2.1 Reference directions and normalization.** Angles measure from 0°
  along +X and increase counter-clockwise (section 1.1). For TARGET
  ENUMERATION this section normalizes to the half-open range [−180°, 180°);
  the +180°/−180° pair deduplicates to the single representative −180°. Two
  families exist during wall-chain creation ONLY:
  - **Global family:** absolute direction multiples of 45° measured
    against the plan +X axis: {−180°, −135°, −90°, −45°, 0°, 45°, 90°,
    135°} (with −180°/180° deduplicated per R6.2.2);
  - **Relative family:** the offsets {0°, 45°, 90°, 135°} added to the
    direction of the previous ACCEPTED segment of the current chain, each sum
    normalized into [−180°, 180°).

  AMENDMENT NOTE (round 11): earlier drafts enumerated all 15° multiples.
  Targets without an exactly representable direction (the remaining 15°
  multiples, e.g. 15°, 30°, 60°, 75°) admit no sound inclusion test under
  the exact-real semantics of R6.0.3 — their boundary lines contain no
  second rational point, so perpendicular-distance comparison cannot be
  evaluated exactly. They are DEFERRED to the ratified numerical model
  (issues 0022/0030) together with all implementation tolerances.
- **R6.2.2 Deduplication and anchor exclusion.** A relative value exactly
  equal (exact-real equality, R6.0.3) to a global-family value yields ONE
  target. When the chain has no previous accepted segment, only the global
  family applies. An angle candidate qualifies ONLY when the projection of
  the pointer onto the target RAY lies strictly ahead of the chain anchor:
  a projection at or behind the anchor (non-positive parameter along the ray)
  excludes the candidate entirely.
- **R6.2.3 Effect and inclusion.** An angle snap constrains DIRECTION only;
  segment length is the projection of the pointer position onto the
  constrained ray. Inclusion uses the same radius discipline as every other
  class: an angle candidate qualifies when the perpendicular plan distance
  from the pointer position to the target RAY (through the chain anchor) is
  less than or equal to T/z — an exact-real comparison; a pointer exactly on
  the ray qualifies at zero. Angle candidates rank below every point class
  (R6.3.1), and their comparator key (level 3) is the normalized angle value.

### 6.3 Winner comparator

- **R6.3.1 Total order.** Candidates compare by:
  1. CLASS, in inventory order: `join-node` > `free-endpoint` > `edge` >
     `axis` > `grid` > `angle`;
  2. EXACT squared screen distance from the pointer to the candidate's closest
     point, in logical pixels after R6.0.1 normalization, compared as exact
     real values — equality is exact-real equality only;
  3. canonical geometry key, lexicographic, TOTAL per class:
     - `join-node`, `free-endpoint`: the exact `(x, y)` plan pair;
     - `edge`: the exact `(x, y)` of the closest point; then the canonical
       boundary-segment key `(component ordinal, ring ordinal, vertex-run
       index)` — components ordered by their R3.2.4 canonical outer-ring
       tuples, outer ring first followed by holes in R3.2.4 hole order,
       segments indexed by vertex-run from each ring's canonical least
       vertex; then discriminator `edge-start < edge-closest < edge-end`;
     - `axis`: the exact projection point `(x, y)`, then the position
       attribute `on-segment < extension`, then ascending stable wall ID.
       One straight axis line (R3.0.1) yields exactly ONE candidate per
       wall — its unique perpendicular projection; the position attribute
       orders candidates BEFORE identity is consulted and never splits one
       wall into two;
     - `grid`: integer lattice indices `(i, j)` ascending;
     - `angle`: normalized angle value ascending in [−180°, 180°);
  4. stable entity ID ascending where level 3 did not already bind it (grid
     and angle candidates carry none).
  No two distinct candidates compare equal at every level: the keys are
  TOTAL.
- **R6.3.1a Reported point (ED-0112).** Ranking and admission compare the
  EXACT closest point and squared distance. The point a candidate reports
  (and a commit carries) is a binary64 point within the certified enclosure
  of the exact one — exact for stored points, provably exact computations,
  and a pointer lying on the feature; otherwise within an ulp or so. A candidate whose exact or
  reported point equals the anchor is excluded under R6.1.3. The pick
  radius may be supplied as `T` logical pixels over the view scale `z`, in
  which case admission is exactly `distance ≤ T/z`.
- **R6.3.2 Dedupe preserves class semantics.** Two candidates of the SAME
  class with equal keys through the final level are ONE candidate and keep
  that class and its subfeature discriminators. Candidates of different
  classes never merge: class differences always resolve at level 1. Joined
  endpoints always deduplicate to their join node (`join-node`), never to
  per-member endpoint candidates.
- **R6.3.3 Determinism.** Delivery order, map iteration order, insertion
  order, and frame history are never observable in the winner; the comparator
  depends only on the R6.0.4 inputs.

### 6.4 Guides, temporary disable, and preference transitions

- **R6.4.1 Guide lifetime.** An alignment guide renders while its candidate
  wins consecutive evaluations and vanishes immediately when the winner
  changes or the gesture ends (commit, finish, cancel, capture loss, second
  touch). Guides are transient derived visuals: zero operations, never
  persisted, never undoable.
- **R6.4.2 Temporary disable binding.** The hold-modifier Alt disables ALL
  snapping. The disable decision is SAMPLED AT GESTURE START and held
  constant for the whole gesture:

  | Transition | Effect |
  |---|---|
  | Alt pressed before gesture start | snapping fully disabled for that gesture |
  | Alt released mid-gesture | gesture remains disabled until it ends |
  | Alt pressed mid-gesture (not held at its start) | no effect on the in-flight gesture; affects gestures begun later |
  | Session start / new document open | disable state inactive; sampling begins fresh |

- **R6.4.3 Local snap preferences and defaults.** Per-mechanism toggles
  (grid, object, angle) exist as local per-device view state. They DEFAULT TO
  ENABLED at every session start and every document open, are not persisted
  in v0.1, never enter the operation log, and are never undoable. A toggle
  change takes effect on subsequent evaluations immediately, including within
  an active gesture. Alt-disable overrides all toggles for gestures begun
  while it is held.

### 6.5 Pointer, capture, click-drag, and touch lifecycle

- **R6.5.1 Drag threshold; duration irrelevance.** A potential gesture becomes
  a drag when its cumulative displacement from gesture start exceeds 5 logical
  pixels measured as an exact EUCLIDEAN distance (never per-axis) after
  R6.0.1 normalization. The threshold is identical for mouse, stylus, and
  touch, applies in screen space only, and never enters document values.
  Displacement of exactly 5 pixels does not exceed the threshold. HOLD
  DURATION participates in NO classification: any release below the threshold
  is a click/tap regardless of elapsed time.
- **R6.5.2 Capture.** A classified drag acquires pointer capture. Captured
  moves feed preview updates only (zero operations, R6.0.6). Loss of capture
  — focus loss, platform steal, or pointer/touch-cancel events — cancels the
  gesture exactly like Escape. Failure to ACQUIRE capture cancels the
  gesture: preview discarded, pre-gesture state restored exactly, zero
  operations, no undo entry.
- **R6.5.3 Release semantics.** Release with capture COMMITS wherever the
  cursor sits, inside or outside the viewport: capture ownership governs, not
  cursor location. Release before the threshold is a click (or tap) governed
  by section 7.
- **R6.5.4 Stale events.** A pointer-up whose pointer ID does not match the
  gesture's owning pointer, or any up without a matching active down, is
  discarded ENTIRELY: no commit and no cancel side effect.
- **R6.5.5 Second touch.** On a touch device, a second finger landing while a
  one-finger editing OR selection/marquee gesture is active cancels that
  gesture FIRST — restoring pre-gesture state INCLUDING pre-gesture selection
  exactly and emitting nothing — and only then does the two-finger
  translate/zoom of section 1.4 arbitrate, proceeding from the restored
  transform. Palm contact and more-than-two-touch input remain unsupported
  and ignored entirely (section 1.4).
- **R6.5.6 Suspension and cancellation.** Application suspension (lifecycle
  hide/background or system-initiated interruption) cancels any active
  document gesture IMMEDIATELY with the same restore-exactly, zero-operation
  outcome as Escape; suspension itself never commits state. Platform
  pointer-cancel/touch-cancel delivery is treated identically to capture loss
  for EVERY gesture type, including selection and marquee.
- **R6.5.7 Tool switch and document close.** Switching tools discards a
  pending preview and cancels an active drag; previously committed batches are
  unaffected. Closing or reopening a document cancels any gesture and clears
  undo/redo stacks (R8.4.2).
- **R6.5.8 Non-finite input.** Any event carrying non-finite coordinates is
  discarded whole (section 1.4).
- **R6.5.9 Escape semantics (total).** While ANY gesture is active — drag,
  marquee, chain preview, or selection gesture — Escape cancels it and
  restores pre-gesture state including PRE-GESTURE SELECTION exactly. While
  idle (no active gesture), Escape clears the selection entirely and ends its
  click cycle. Restoring pre-gesture selection does not undo cycle resets
  already caused by observed input/document transitions (R7.2.6).
- **R6.5.10 Chain lifecycle.** Under the wall tool, chaining continues from
  the last accepted endpoint; each accepted segment commits exactly ONE batch
  (R3.1.1, preserved verbatim). Finish events — double-click, Enter,
  right-click, tool switch — end the chain after any already-committed
  segments. Cancel events discard ONLY the uncommitted preview segment;
  previously committed segments remain as independent batches. Cancelled
  previews emit zero operations and no undo entries.

Transition table (total; every cancellation restores previews AND selection
exactly, emitting zero operations):

| Event | Precondition | Resulting state |
|---|---|---|
| Pointer down | creation/edit tool armed | potential gesture |
| Pointer move, cumulative Euclidean displacement ≤ 5 px, any duration | potential gesture | still click-classified; duration irrelevant |
| Pointer move, displacement > 5 px | potential gesture | drag classified; capture acquired; live preview begins |
| Capture acquisition fails | drag classified | gesture cancelled; pre-gesture state and selection restored; zero ops; no entry |
| Release with capture, inside viewport | active drag | commit: one batch per section 8 boundaries |
| Release with capture, outside viewport | active drag | commits identically; cursor location irrelevant |
| Release without capture (stale/unmatched ID) | any | event discarded entirely |
| Escape while gesture active | any gesture | cancelled; pre-gesture state AND selection restored; zero ops |
| Escape while idle | no gesture | selection cleared |
| Focus loss, capture loss, pointer-cancel, touch-cancel | any gesture incl. selection/marquee | cancelled like Escape; restore-exactly incl. selection |
| App suspend / lifecycle hide-background | active gesture | gesture cancelled IMMEDIATELY; restore-exactly incl. selection |
| Tool switch | pending preview or active drag | preview discarded / drag cancelled; committed batches unaffected |
| Document close or reopen | any gesture | gesture cancelled; stacks cleared (R8.4.2) |
| Second touch lands | one-finger editing or selection/marquee gesture | gesture cancelled first; pre-gesture selection restored; §1.4 pinch then arbitrates |
| Accepted external/replayed batch lands | any local interaction active | interaction cancelled with full restore first; batch then applies; stale preview never commits |
| Event carries non-finite coordinates | any | event discarded whole |

### 6.6 Topology boundary

- **R6.6.1 Final-state classification.** Contact classification runs on final
  COMMITTED positions only, pairwise at the same level, under the R3.1.2 total
  precedence. Snapping is input quantization only: it changes which
  coordinates a command CARRIES, never how the classifier treats them. A
  near-miss that did not snap follows the normal reject/warn classes
  (`collinear-overlap`, `wall-area-overlap`). This section introduces no
  geometric tolerance of any kind.
- **R6.6.2 Whole-batch effects.** Incidental splits/joins join the same single
  batch and undo entry as their triggering edit (R3.4.4); gates evaluate
  atomically (R3.1.8, R3.4.6); warnings attach to the batch result after
  validation and never abort it (R3.1.7). Rejected batches leave undo state
  unchanged.
- **R6.6.3 No implicit detach.** Detach-by-distance remains forbidden
  (R3.3.7); dragging a joined endpoint moves the node rigidly (R3.4.2);
  explicit detach is the only membership-ending path short of deletion.
  Sections 6–8 introduce no exception.

### 6.7 Normative fixtures (S-series)

Every fixture states `view {z, ox, oy, DPR d}` and converts each input through
the full chain RAW DEVICE PIXELS (CANVAS-LOCAL, relative to the canvas
element origin) → LOGICAL PIXELS (divide by d) → PLAN MM (inverse transform
x = (lx − ox)/z, y = (oy − ly)/z). Canvas-local raws make the mapping total;
no additional window-level offset is modeled. Unless stated, fixtures
use z = 0.1, offsets (ox, oy) = (512, 384), DPR d = 2. Winners name class and
exact candidate with comparator traces using SQUARED screen distances; all
asserted quantities are exact rationals — approximate irrational claims appear
nowhere.

#### S1 — Inclusive boundary hit; full two-endpoint segment commit
- View: defaults; mouse (T = 12 px ⇒ plan radius 120 mm). Before: F1 state
  (`W1` isolated); wall tool armed; chain started.
- Input 1: raw device (1800.0, 768.0) px → logical (900.0, 384.0) → plan
  (3880, 0): squared screen distance to `W1.End` exactly T² = 144 px².
- Winner 1: `free-endpoint` (4000,0) — INCLUSIVE at exact equality
  (distance ≤ radius holds); class order outranks grid regardless.
- Input 2: raw device (2000.0, 768.0) px → logical (1000.0, 384.0) → plan
  (4880, 0). Winner 2: `axis` — W1's axis line projects to (4880,0) at
  squared distance 0 (grid (4900,0)/(4800,0) at 4/64 px² lose by class).
- Effects: `[create Wall Wc supplied:idC {start:(4000,0), end:(4880,0),
  t:200, h:2500, style:S0}; create Join J supplied:idJ {(W1,End),(Wc,Start)}]`
  — endpoint-endpoint auto-join at the snapped start (F2 pattern), created
  fields explicitly resolved (R3.0.6); ONE batch, ONE undo entry.
- Undo trace: undo deletes J and Wc restoring F1.

#### S2 — Equal-distance tie broken by tuple; UUID tie needs full key match
- View: defaults; mouse. Before: `Wa`(idA) (−600,0)→(940,0) and `Wb`(idB)
  (1060,0)→(2600,0), t=200, h=2500. Axes share line y=0 but segments are
  disjoint ([−600,940] vs [1060,2600]); the butt-capped rectangles span
  exactly x∈[−600,940] and x∈[1060,2600] — caps terminate at the endpoints —
  leaving a positive 120 mm gap, so NO contact,
  NO area overlap, and NO warnings arise — both walls exist cleanly
  (R3.1.2 precedence leaves them independent).
- Input: click at plan (1000, 0) — raw device (1224.0, 768.0) px → logical
  (612.0, 384.0).
- Winner: both free endpoints sit at squared screen distance 60²·z² = 36 px²
  — exact tie → tuple level: (940,0) < (1060,0) → winner `free-endpoint`
  (940,0) on `Wa`.
- Contrast case: in the F18 aftermath, `W1.End` and `W2.Start` coincide at
  (4000,0) as free endpoints; keys match through level 3 but IDs differ →
  ascending stable UUID picks `W1`. Dedupe (R6.3.2) does NOT apply because
  the final key differs.

#### S3 — Joined endpoints deduplicate to one node candidate
- View: defaults; mouse. Before: F2 state.
- Input: joint-drag begins at plan (4010, −10) — raw device (1826.0, 770.0)
  px → logical (913.0, 385.0).
- Winner: exactly ONE `join-node` candidate at (4000,0); member endpoints
  produce NO separate `free-endpoint` candidates.
- Effects: rigid co-move per R3.4.2; one batch on release.

#### S4 — Self-snap exclusion; snapped winners resolve legally
- View: defaults; mouse; grid spacing s = 100 mm.
- Part (a): accepted chain segment seg1 (0,0)→(3000,0), t=200, h=2500,
  style S0 (ONE batch); second-segment preview from anchor (3000,0);
  pointer raw device (1654.0,766.0) px → logical (827.0,383.0) → plan
  (3150,10).
  - Candidate scan at exact squared distances: own anchor (3000,0) at
    (150²+10²)·z² = 226 px² > 144 — OUTSIDE the radius and excluded
    regardless (R6.1.3); every acceptance yielding a zero-length segment
    is likewise removed (R3.0.5); edge candidates of seg1 all miss — the
    NEAREST edge point is on the right cap face x=3000 at (3000,10),
    squared distance (150²+0²)·z² = 225 px² > 144 (the top-face point
    (3000,100) is farther at 306 px²); AXIS-line projection of seg1:
    (3150,0) at (0²+10²)·z² = 1 px² — within, an EXTENSION 150 mm beyond
    seg1's end; grid (3100,0) and (3200,0) tie at (50²+10²)·z² = 26 px².
  - Winner: `axis` (3150,0) — highest PRESENT class (no node/endpoint/
    edge within radius), exact 1 px². Two probes confirm
    filtering: at (3005,3) the allowed set keeps cap-face edge (3000,3)
    and axis extension (3005,0) while the anchor-coincident endpoint and
    any zero-length-producing candidate are dropped; at plan
    (3025,0) — raw device (1629.0,768.0) px → logical (814.5,384.0) — the
    two cap-face EDGE points tie at 425/4 px² and win by CLASS over the
    axis extension (3025,0) at exactly 0 px² and the surviving grid point;
    the ANCHOR-COINCIDENT free-endpoint and grid targets at (3000,0) were
    removed across classes before evaluation.
  - Effects: commit (3000,0)→(3150,0): positive length sharing ONLY the
    endpoint with seg1 → endpoint-endpoint contact, axes collinear →
    degenerate-patch continuation auto-join, legal per F3:
    `[create Wall seg2 supplied:idS2 {start:(3000,0), end:(3150,0),
    t:200, h:2500, style:S0}; create Join J supplied:idJ1 {(seg1,End),
    (seg2,Start)}]` — idJ1 a UNIQUE fresh identity. ONE batch, ONE undo
    entry.
- Part (b): additionally `Wv` (3600,−500)→(3600,1500), t=200, h=2500,
  isolated; chain anchor is the ACTUAL PRIOR ENDPOINT seg2.End = (3150,0);
  third-segment preview; pointer raw device (1744.0,428.0) px → logical
  (872.0,214.0) → plan (3600,1700) — exactly on `Wv`'s axis LINE, 200 mm
  beyond its top endpoint.
  - Winner: `axis` extension (3600,1700) at squared distance 0; the
    free-endpoint (3600,1500) and cap-edge face sit at (0²+200²)·z² =
    400 px² — OUTSIDE the radius; grid (3600,1700) ties at 0 px² and
    loses by class.
  - Effects: commit (3150,0)→(3600,1700), starting at the true prior
    endpoint. Final-state classification: NO axis contact (seg3's axis
    meets x=3600 only at its own endpoint (3600,1700), beyond `Wv`'s
    span y∈[−500,1500]; its centerline crosses x=3500 at
    y = (350/450)·1700 = 11900/9 mm, inside `Wv`'s span); occupied
    rectangles overlap with positive area
    near `Wv`'s left face → warning `wall-area-overlap` after
    validation; batch COMMITS (R3.1.7). Whole-batch effects per R6.6.2:
    `[create Wall seg3 supplied:idS3 {start:(3150,0), end:(3600,1700),
    t:200, h:2500, style:S0}; create Join J2 supplied:idJ2 {(seg2,End),
    (seg3,Start)}]` with diagnostic warning attached to the batch
    result. Join J2's membership sits at the exact coordinate (3150,0).
    idJ2 is a NEW identity (part (a)'s idJ1 never reused). ONE batch,
    ONE undo entry.

#### S5 — Angle families normalize and dedupe; ahead-of-anchor check
- View: defaults; mouse; grid toggle OFF (local preference), object snap ON.
- Before: accepted chain segment (0,0)→(3000,0) (direction 0°); second-
  segment preview.
- Input: pointer raw device (1824.0, 568.0) px → logical (912.0, 284.0) →
  plan (4000, 1000): exactly on the 45° ray from the anchor (3000,0);
  ray-perpendicular distance exactly 0; projection parameter strictly
  positive — ahead of the anchor (R6.2.2 satisfied).
- Winner: relative sums {0°,45°,90°,135°} all coincide with global 45°-family
  multiples → deduplicated single targets; winner `angle` 45°. Committed point:
  the pointer's own plan position (4000,1000) — the projection of an on-ray
  pointer is itself.

#### S6 — Disable modifier held across a full two-endpoint commit
- View: defaults; mouse; Alt HELD from before gesture start. Before: empty
  document, level L1; wall tool armed.
- Input 1: raw device (1271.4, 650.2) px → logical (635.7, 325.1) → plan
  (1237, 589). Input 2: raw device (1471.4, 450.2) px → logical (735.7,
  225.1) → plan (2237, 1589).
- Winners: NONE — Alt sampled at gesture start disables ALL snapping despite
  default-enabled toggles (R6.4.2/R6.4.3 transitions).
- Effects: `[create Wall Wn {start:(1237,589), end:(2237,1589), t:200,
  h:2500, style:S0}]` — explicitly resolved parameters (R3.0.6); raw
  coordinates commit unsnapped; empty scene ⇒ no contacts; ONE batch, ONE
  undo entry.

#### S7 — Touch versus mouse radii
- View: defaults EXCEPT the grid snap toggle is OFF for this fixture
  (stated exactly: local preference `grid=disabled`, `object=enabled`;
  with grid enabled the always-present ≤ 70.7 mm lattice distance would
  make a true mouse MISS impossible). Before: F1 state (`W1.End` at
  (4000,0)).
- Input: tap/click at plan (4150,150) — raw device (1854.0,738.0) px →
  logical (927.0,369.0); squared screen distance to the endpoint
  (150²+150²)·z² = 450 px².
- Winner: mouse/stylus — NO candidate within radius (endpoint 450,
  axis-line projection (4150,0) at 225, nearest boundary (4000,100) at
  250 px² — all > 144): a TRUE MISS. Touch — endpoint 450 ≤ 576 (T =
  24): HIT, and class order picks `free-endpoint` (4000,0) over the
  farther axis/edge candidates that also qualify. Identical coordinates
  and preferences; the device decides the radius.

#### S8 — Stale pointer-up ignored
- View: defaults; touch. Joint drag owned by pointer t1, begun at S3's start
  coordinates.
- Input: up arrives with pointer id t2 while t1 remains down.
- Result: entire event ignored — no commit, no cancel side effect. The later
  matching t1 up commits normally.

#### S9 — Release outside viewport commits
- View: defaults; mouse. Before: F2 state; joint-drag J1 captured, preview
  moving toward (4000,900) — logical (912.0, 294.0): lx = 512 + 0.1·4000,
  ly = 384 − 0.1·900.
- Input: release occurs beyond the right viewport edge while captured.
- Result: COMMITS `[update W1 {end:(4000,900)}; update W2 {start:(4000,900)}]`
  identically to an interior release (F16 vectors apply).

#### S10 — Second-touch cancellation precedes viewport gesture (executable)
- View: defaults (z = 0.1, offsets (512,384), DPR 2); touch. Before: F2
  state. One-finger joint drag active with finger f1 held at canvas-local
  raw device (1800.0, 768.0) px → logical (900.0, 384.0) → plan (3880, 0).
- Input 1: second finger f2 lands at raw (1400.0, 768.0) px → logical
  (700.0, 384.0) → plan (1880, 0).
- Result 1: the editing gesture is cancelled FIRST — F2 state restored
  exactly including selection; zero operations; no entry. Both fingers
  remain down, so the §1.4 two-finger gesture arbitrates from the RESTORED
  transform over the current finger positions: initial centroid =
  ((900+700)/2, 384) = (800.0, 384.0) logical → plan (2880, 0); initial
  finger distance = |900−700| = 200 logical px exactly.
- Input 2: fingers move to raw (1870.0, 768.0)/(1370.0, 768.0) px →
  logical (935.0, 384.0)/(685.0, 384.0): current centroid (810.0, 384.0)
  (translation +10 logical px along +lx); current distance 250 px; ratio
  250/200 = 5/4 exactly.
- Derivation (section 1.4): translation applies first, content following
  input: ox 512 → 522. The scale keeps the plan point beneath the centroid
  invariant: anchored plan point x = (810−522)/0.1 = 2880, y = 0;
  z′ = 0.1·(5/4) = 0.125; ox′ = 810 − 0.125·2880 = 450; oy′ = 384 + 0.
- Result 2: final view {z = 0.125, offsets (450, 384)} — every value an
  exact rational; view-only changes emit zero operations.

#### S11 — Threshold boundary exact; duration irrelevant
- View: defaults; mouse. Empty canvas region, select tool.
- Input: down at raw device (200.0, 200.0) px → logical (100.0, 100.0); move
  to raw device (206.0, 208.0) px → logical (103.0, 104.0) — cumulative
  Euclidean displacement exactly 5 px (squared 25 px²); hold 3 s; release.
- Result: 5 px does not exceed the threshold AND duration is irrelevant:
  CLICK semantics apply (selects top-ranked hit, or clears on empty space);
  never marquee/drag. A later (6,8)-logical-px move (squared 100 px²)
  classifies drag.

#### G1 — Guide lifetime follows consecutive wins and gesture end
- View: defaults; mouse. Scene: ONE isolated wall `Wa` (0,0)→(1000,0),
  t=200, h=2500 — occupied rectangle exactly x∈[0,1000], y∈[−100,100].
  Wall tool armed; chain started grid-snapped at (2000,−400); first
  accepted segment (2000,−400)→(2400,−400), t=200, h=2500, style S0 (ONE
  batch) — its rectangle x∈[2000,2400], y∈[−500,−300] never touches
  `Wa`'s rectangle, so topology stays valid. The CONTINUATION ANCHOR for
  the second-segment preview is the PRIOR ACCEPTED ENDPOINT (2400,−400)
  (R6.5.10); only that anchor is excluded (R6.1.3); `Wa`'s edges/axis and
  seg1's edges/axis are legal candidates.
- Frame 1: pointer raw device (1128.0,744.0) px → logical (564.0,372.0)
  → plan (520,120). Winner: `edge` — closest point (520,100) on `Wa`'s
  top face, squared distance (0²+20²)·z² = 4 px²; best grid (500,100) at
  8 px² and `Wa`'s axis-line projection (520,0) at 144 px² are moot:
  under the R6.3.1 class-major comparator the HIGHEST PRESENT CLASS wins
  outright, and `edge` is the highest class within radius (no node or
  free-endpoint is within radius). The edge alignment guide renders.
- Frame 2: pointer raw device (1130.0,742.0) px → logical (565.0,371.0)
  → plan (530,130). Winner: `edge` again — closest point (530,100),
  squared (0²+30²)·z² = 9 px², with best grid at 18 px² reported for
  completeness. Same-class consecutive win ⇒ the edge guide PERSISTS
  unchanged.
- Frame 3: pointer raw device (1364.0,708.0) px → logical (682.0,354.0)
  → plan (1700,300): OUTSIDE EVERY OBJECT RADIUS — nearest object points:
  `Wa` boundary (1000,100) at ((700)²+(200)²)·z² = 5300 px²; seg1 boundary
  (2000,−300) at ((300)²+(600)²)·z² = 4500 px²; axis lines project to
  900 px² (`Wa`) and 4900 px² (seg1); the anchor is excluded — while the
  pointer lies EXACTLY on grid point (1700,300) at squared distance 0.
  Winner flips to `grid` ⇒ the edge guide VANISHES and the grid alignment
  guide renders immediately (guide change verified).
- Frame 4: Escape cancels the chain preview ⇒ ALL guides clear instantly.
- Assertions: guides render exactly while their candidate wins
  consecutive evaluations (frames 1–2), vanish on winner change (frame 3)
  and at gesture end (frame 4); every frame emits ZERO operations;
  nothing is persisted; no stack changes (R6.4.1).

#### NF1 — Non-finite events are discarded whole
- View: defaults; mouse; F2 state; select tool.
- Input: a pointer-move event whose y coordinate is NaN, followed by a valid
  click at plan (4050,1500) — raw (1834.0,468.0) px.
- Result: the non-finite event is discarded ENTIRELY (section 6.5 table):
  no partial application, no snap-evaluation disturbance, no diagnostic
  storm, no stack or selection change. The subsequent valid click behaves
  normally (selects top hit). Document, op log, and stacks unchanged by the
  non-finite event itself.

#### S12 — Capture loss cancels like Escape
- View: defaults; mouse. Active joint drag as in S9, mid-preview.
- Input: window loses focus mid-preview.
- Result: gesture cancelled; pre-gesture state restored exactly; zero ops; no
  entry. Identical observable outcome to Escape (F16 state unchanged).

## 7. Selection

Status: **Normative**. Rules are identified as
`R7.*`.

### 7.1 Selection model

**Owner amendment (2026-10-15, issue 0107).** The default tool is **Select**,
not Select / draw. In Select, a plain drag from empty canvas replaces the
selection with the intersecting marquee result; modifier-drag adds. Only the
Walls tool creates walls. A plain drag starting on any selected member of a
multi-selection translates the selected walls and model instances together
by the raw plan-space displacement, without snapping or endpoint resizing.
The set and primary remain unchanged. The translation is one atomic undoable
batch, with each shared wall node moved once, ordinary attached-node co-moves
and contact resolution on the final geometry. Hosted openings follow their
walls once. A selected opening without its selected host, join handle or
degraded placeholder is not a supported group translation target: refuse the
whole command rather than partially moving the group. Cancellation restores
pre-gesture selection without editing. Single-object edits and modifier-click
toggles retain their existing behavior. This explicit multi-entity command
supersedes the primary-only drag rule below only for that gesture.
Provenance: original product direction in the owner's request, not external
application source or behavior. No document-format change.

- **R7.1.1 Set plus primary; explicit group commands.** Selection is an
  UNORDERED set of entity IDs on the active level plus one designated PRIMARY
  item: the most recent addition. Removing the primary leaves primary UNSET
  until the next addition; unset is an observable, specified state and
  implementations must never silently reassign it. Commands act on their
  commanded target unless a normative rule explicitly defines a multi-entity
  command. The owner amendment above defines plain selected-group translation;
  inspector edits and deletions still target the PRIMARY item.
- **R7.1.2 State class.** Selection, hover, and cycle index are local
  transient state: never persisted, never in the operation log, never
  restored by undo/redo, cleared on level switch (section 7.4 table).
- **R7.1.3 Hit classes, ordering, and total keys.** Hit testing reuses
  section 6 machinery: radii 12/24 logical px (mouse+stylus/touch),
  inclusive boundaries, T/z plan conversion, exact-real squared comparisons,
  R6.0.1 normalization. Hit CLASSES and precedence:
  1. `join-node` handles — key as R6.3.1;
   2. `free-endpoint` — key as R6.3.1;
   3. `opening` — a door/window's aperture rectangle over its host wall's
      occupied band, from its stored width and centre position; nearest point,
      canonical ring, then stable opening ID break ties. A click on the
      aperture selects the opening before its host wall; dragging the opening
      does not move the host wall. Repeated clicks may cycle to the host wall;
      opening marquee selection is deferred.
   4. `region` — each wall's OWN occupied rectangle (centered butt-capped
     rectangle, R3.0.2), NOT the cluster union: the closest point is the
     pointer itself when inside (exact distance zero); total key = exact
     `(x, y)` of the closest point, then the wall's canonical outer-ring
     tuple sequence (R3.2.4 normalization: least-vertex rotation, open
     rings), then hole tuples, then stable wall ID;
   5. `placeholder-box` — degraded placeholder instances only, bounded by the
     §9.6 deterministic placeholder geometry (descriptor-declared dimensions,
     else the fixed 1000 mm box centred on the instance origin); total key =
     exact min-corner `(x, y)`, then stable instance ID.
   Only active-level entities are eligible; non-active levels are never hit,
   cycled, or marquee-selected (section 2.4).

- **R7.1.4 Opening drag handles (original GUI decision, issue 0068).** A drag
  starting inside an aperture moves it along its host. When it ends inside a
  different active-level wall's own occupied region, that wall becomes the
  target: R4.3.5's delete-old/create-new-with-fresh-ID operations share ONE
  atomic undo batch and selection follows the newly created opening. A drag
  starting within eight logical pixels of either aperture END instead resizes
  width and centre while the opposite end remains fixed; it never changes
  hosts. A selected door's leaf is a swing handle outside the wall band:
  dragging its tip across the host changes `opensToward` to the final physical
  side; it never changes `side`, `hinge`, or wall geometry. Invalid results
  are refused as a whole by the core. Previews remain view-local and emit
  zero operations; cancellation restores the original state.
- **R7.1.5 Selection-only and warning focus (GUI decisions, issues 0070/0107).**
  Select is the default tool. Explicit Select activation after using the Walls
  chain leaves the creation tool: a plain drag starting in empty space selects
  by marquee and cannot create a wall. A warning's referenced wall pair or
  level may be highlighted while its details dialog is open. Highlighting is
  transient, does not change selection or the operation log, and clears when
  the dialog closes. An opening's end-resize boundary exposes a resize cursor.
- **R7.1.6 Wall measurement editing (original GUI decision, issue 0071).** The
  editor MAY present a wall's thickness, height, and centreline length as one
  numeric form submitted as ONE atomic batch. A length edit is expressed
  about the fixed Start endpoint: End moves along the wall's existing
  Start→End direction to the requested distance, and a joined End moves its
  R3.4.2 node members with it. Every value MUST be finite and strictly
  positive (R3.0.4); the core revalidates the resulting state, so a refused
  batch (contact, zero length, opening constraints) leaves the document, both
  history stacks, and selection unchanged. The form seeds from the current
  authoritative snapshot and emits zero operations when nothing changed.

### 7.2 Click, tap, cycling, modifiers

- **R7.2.1 Tap/click classification.** Any release below the 5 px Euclidean
  threshold is a tap/click REGARDLESS of hold duration (R6.5.1); duration
  creates no additional behavior.
- **R7.2.2 Click and cycle precedence.** For plain clicks (no Ctrl/Meta/Shift;
  Alt alone remains plain), the FIRST click selects the top-ranked entity in
  the selection list defined below, replacing selection and setting it
  primary. Each REPEATED UNCHANGED plain click — same pointer position within the
  applicable radius, unchanged view transform, unchanged modifier state,
  unchanged pointer type, unchanged ordered selection list — advances to the NEXT
  entity in that selection list, wrapping after the last. Clicking empty space
  clears the selection. Plain click on an already-selected non-primary item
  reselects it as sole selection.
- **R7.2.2a Entity cycle projection (original decision, issue 0049).** Raw
  hit candidates keep their feature identity and R6.3.2 class semantics.
  Click/tap selection derives a separate ordered list of distinct entity IDs
  by retaining each ID's FIRST occurrence in the R7.1.3 native order. One
  entity occupies one cycle stop even when its two endpoints and region all
  qualify. This projection does not merge native features, change their
  ranking or choose a different drag handle. An empty click ends the cycle
  as well as clearing selection. This rule also applies to modifier clicks.
- **R7.2.3 Exhaustive cycle reset.** The cycle index resets to "first" when
  ANY of: the pointer position moves beyond the applicable radius; the view
  transform changes (zoom or pan); the active level changes; any level
  visibility/ghost flag changes; the modifier state changes; the pointer type
  changes (mouse ↔ touch ↔ stylus); the candidate set changes (any accepted
  document change or external batch).
- **R7.2.3a Reset observations (issue 0049 clarification).** The position
  anchor is the FIRST click of the current cycle, in logical pixels;
  subsequent in-radius clicks do not move it. The boundary at T is inclusive.
  Observing a finite pointer position beyond T resets immediately, even if it
  returns before another click. View, level, visibility/ghost, modifier and
  pointer-type changes likewise reset when observed; changing away and back
  does not revive the prior cycle. Modifier state is the set of held Ctrl,
  Meta/Cmd, Shift and Alt families; Ctrl/Meta/Shift enable selection toggling,
  Alt alone does not. Accepted document batches always invalidate the cycle,
  even when the selection list at the pointer is unchanged. A refused edit,
  no-op history call, read or save does not. At a click, a changed ordered
  selection list also restarts at its first entity.
- **R7.2.4 Modifiers.** Modifier-click (ctrl/cmd and shift equivalent in
  v0.1) toggles the clicked candidate's membership without clearing others:
  adding sets it primary; removing the primary unsets primary.
  Modifier-click on empty space clears the selection. Modifier-drag from an
  empty start performs additive marquee.
- **R7.2.4a Modifier target (original decision, issue 0049).** Each
  Ctrl/Meta/Shift-click targets the FIRST entity in the projected selection
  list and ends any active cycle. Repeated held-modifier clicks toggle that
  same top entity on/off; they do not cycle overlapping entities. Returning
  to plain clicks starts a fresh cycle. This explicitly limits R7.2.2's
  cycling precedence rather than inferring a target from existing selection.
- **R7.2.5 No long-press.** Duration NEVER changes classification nor arms
  extra behavior: long-press triggers no marquee, context menu, or drag
  override; its release is an ordinary tap (R7.2.1). Recorded as a deliberate
  exclusion.
- **R7.2.6 Potential drag selection (original decision, issue 0049).** A
  plain press on the current primary leaves the entire selection intact for
  a potential drag. A plain press on another hit entity tentatively replaces
  selection with that entity alone as the potential drag primary. If release
  classifies as a click, evaluate R7.2.2 against the PRE-PRESS selection,
  not that tentative selection. If it becomes a drag, commands target the
  resulting primary and keep that post-press selection (R7.4). Cancellation
  restores pre-press selection exactly, without reviving cycle state already
  invalidated by observed input or document transitions. Modifier press does
  not change selection. A modifier click still toggles the clicked entity
  whether or not it was primary; a modifier drag over an entity may edit only
  when its hit target is the existing primary. Otherwise it emits no edit.
  Empty modifier drags remain additive marquee under R7.2.4, never wall
  creation. These rules introduce no group commands.

### 7.3 Marquee

- **R7.3.1 Single intersecting mode.** One mode exists, independent of drag
  direction: entities whose interaction shape INTERSECTS the rectangle are
  selected — walls via their own occupied rectangles (`region` shape) and
  placeholders via their §9.6 bounds. **Owner amendment (2026-10-15,
  issue 0111):** resolved asset instances also participate, using the
  axis-aligned envelope of their displayed editor footprint (including the
  conservative projected footprint for tilted models). This may select a
  rotated object when the rectangle touches its envelope rather than its
  exact silhouette. Asset candidacy comes from the same resolution snapshot
  as clicking; a degraded instance participates only as a placeholder, never
  twice. Resolved assets rank after geometry classes on a min-corner tie;
  otherwise all classes share the positional key below. This is original
  owner-requested multi-selection behavior, with no external source consulted
  and no document-format change. Intersection is boundary-inclusive
  under exact-real semantics: tangential or boundary-only contact selects.
- **R7.3.2 Validity and inputs.** Marquee corners arrive as LOGICAL pixel
  positions and convert to plan through the §1.2 inverse transform; corners
  must be finite — a non-finite corner discards the whole event (section 1.4
  rule). A rectangle whose two corners COINCIDE degenerates to point-hit
  semantics at that corner (R7.3.2a). Marquee replaces the selection on plain
  drag and adds on modifier drag. For rectangle-compatible total ordering
  across shapes, every candidate carries a MARQUEE KEY: (min-corner x,
  min-corner y of its interaction shape's axis-aligned bounding box, R7.1.3
  class rank, stable entity ID); additive marquee processes newly
  intersecting entities in ascending marquee-key order, and each ENTITY is
  added at most once even when several of its features intersect. PRIMARY becomes deterministically
  the LAST entity of that order among the ADDED entities; when an additive
  marquee adds NO new entity (every intersecting entity is already
  selected), the selection AND primary remain UNCHANGED. "Most recent
  addition" is thereby total and delivery-order-independent. Marquee
  gestures follow the
  FULL transition table of section 6.5: second touch, capture loss,
  pointer/touch-cancel, suspension, and Escape cancel the marquee and restore
  PRE-GESTURE SELECTION exactly.
- **R7.3.2a Degenerate rectangles (original decision, issue 0059).** The
  degeneracy R7.3.2 sends to point-hit semantics is COINCIDENT CORNERS, and
  nothing wider. A LINE rectangle — zero in one extent, and so zero in area
  too — keeps R7.3.1 intersection semantics and selects every candidate whose
  interaction shape it meets, boundary contact included. Earlier wording said
  "zero-area", which is ambiguous: a line has zero area, but "at its corner"
  names one point only when the two corners are the same point, and point-hit
  semantics have nowhere to be applied otherwise. The two readings are
  observably different rather than a matter of phrasing. At a join node,
  point-hit semantics return the join alone, because R7.1.3 ranks `join-node`
  above `region`; a line through that same node returns every wall region it
  crosses. Fixture L13 states the case.

  **Provenance and status.** Original Domowik behavior, aligned with the core's
  implementation. This clarification remains subject to owner review; no
  independent owner ratification is claimed.

### 7.4 Primary after every mutation

Total rules. Unless a normative section explicitly defines a multi-entity
command (including §7.1's selected-group translation), drags, edits, and deletion target
the PRIMARY item only; other selected members are unaffected witnesses.
For a drag, the "unchanged" command baseline below is the selection AFTER
R7.2.6's tentative press retargeting, not the pre-press selection. Cancellation
instead restores the pre-press selection, as stated explicitly in that rule.

| Mutation | Observable result |
|---|---|
| Primary-targeted command commits (drag/edit) | Batch covers the command plus ordinary topology locality (e.g., R3.4.2 co-moves); selection unchanged; primary kept |
| Deletion of the primary | Deleted ID leaves the set; remaining members stay selected; primary UNSET |
| Deletion targeting a non-selected entity via controls | Set unchanged unless it contains the deleted ID |
| Undo/redo execution | Selection untouched by stack execution itself; selected IDs that cease to exist are removed; primary UNSET if removed |
| Reopen/recovery open | Selection EMPTY, primary unset (selection was never persisted) |
| Active-level switch | Selection clears entirely (section 2.4) |
| Escape while idle | Selection clears entirely |
| Escape during gesture | Pre-gesture selection restored exactly (R6.5.9) |
| Accepted external/replayed batch | Selected IDs that cease to exist are removed; primary UNSET if removed |

No implementation may invent a primary; UNSET propagates exactly as tabulated.

### 7.5 Hidden, locked, placeholders, cross-level

- **R7.5.1 Hidden.** Hiddenness is exactly the section 2.4 level-state model;
  there is no per-entity visibility or lock flag in v0.1. Non-active levels
  are never picked, cycled, or marquee-selected regardless of flag.
- **R7.5.2 Locked entities.** Excluded from v0.1 entirely: no authoritative
  lock field exists (consistent with section 2.4 having no stored editable
  flag). Deferred indefinitely.
- **R7.5.3 Placeholders.** Degraded placeholder instances participate in hit
  testing, cycling, marquee, selection, and deletion exactly per §9.6, via
  the `placeholder-box` class (R7.1.3); they are never snap sources
  (R6.0.5).
- **R7.5.4 Cross-level selection.** Forbidden: selection spans one level (the
  active level) only.

### 7.6 Normative fixtures (L-series)

View conventions per section 6.7 (defaults unless stated). All states use
level L1 unless stated.

#### L1 — Deterministic cycle order among coincident regions
- Before: three parallel horizontal walls, `Wa`(idA) (0,1400)→(4000,1400),
  `Wb`(idB) (0,1450)→(4000,1450), `Wc`(idC) (0,1500)→(4000,1500), each
  t=200, h=2500. Bands share y ∈ [1400,1500]; parallel axes never touch, so
  each pair commits with a `wall-area-overlap` warning and no joins.
- Input: three consecutive clicks at plan (2000,1450) — raw device
  (1424.0, 478.0) px → logical (712.0, 239.0).
- Winner trace: every `region` candidate contains the pointer (closest point
  = pointer itself, exact distance zero; canonical ring tuples then wall IDs
  order them) → cycle order `Wa`, `Wb`, `Wc`, wrap. First click selects the
  top-ranked (`Wa`); each repeated unchanged click advances one step. Order
  is identical across frames.

#### L2 — Boundary-inclusive marquee tangent contact
- Before: F1 state (`W1` rectangle y ∈ [−100,100]).
- Input: plain marquee given in LOGICAL pixels first: corners
  logical (412.0, 374.0) and (1012.0, 324.0) — raw device (824.0, 748.0)
  and (2024.0, 648.0) px — converting to plan corners (−1000, 100) and
  (5000, 600). The rectangle's lower edge lies exactly on y = 100, tangent
  to W1's top face over x∈[−1000,4000].
- Result: intersection is the closed segment y=100 (zero area) — W1
  SELECTED (boundary-inclusive, R7.3.1).

#### L3 — Zero-area marquee degenerates to point hit
- Input: marquee with both corners at logical (712.0, 239.0) — raw device
  (1424.0, 478.0) px — plan (2000,1450), over L1's scene.
- Result: point-hit semantics; selects the top-ranked candidate at that
  point (`Wa` by L1 ordering), replacing any prior selection.

#### L4 — Cross-level exclusion
- Before: F24 state (identical XY walls W1@L1, W2@L2), active level L1.
- Input: marquee corners logical (462.0, 434.0) → raw device (924.0, 868.0)
  px, plan (−500, −500); and logical (962.0, 34.0) → raw device
  (1924.0, 68.0) px, plan (4500, 3500): fully containing both rectangles'
  XY extents.
- Result: only `W1` selected. `W2` is invisible to selection despite XY
  identity; no level state changed (section 2.4).

#### L5 — Primary-unset propagation through primary deletion
- Before: L1 scene; selection {Wa,Wb,Wc}, primary `Wb`.
- Command: delete the PRIMARY `Wb` (legal; no gates apply). v0.1 defines no
  multi-entity delete command (R7.4).
- Result: set {Wa,Wc}, primary UNSET. The next plain click adds its target
  with primary. Undo recreates equivalent `Wb` per F19, but selection is
  NOT restored by undo: the set remains {Wa,Wc}, primary UNSET (R7.4).

#### L6 — Multi-selection does NOT group commands; drag targets primary only
- Before: F2 state; modifier-click built selection {W1,W2} with primary
  `W2`; J1 joins (W1,End),(W2,Start).
- Input: body-drag begins on primary `W2` at plan (4050,1500) — raw device
  (1834.0, 468.0) px → logical (917.0, 234.0); moves by (0,+300) plan;
  releases.
- Effects: the body drag translates BOTH of `W2`'s endpoints by (0,+300)
  (start (4000,0)→(4000,300), end (4000,3000)→(4000,3300)); ONLY the
  topology-attached neighbor endpoints co-move with their join nodes
  (R3.4.3/F17): `J1`'s node carries `W1.End` along:
  `[update W1 {end:(4000,300)}; update W2 {start:(4000,300)},
  end:(4000,3300)}]`. No further propagation: `W1`'s far end stays at
  (0,0), and nothing beyond the directly incident nodes moves. The co-move
  arises from TOPOLOGY, never from selection grouping. Selection remains
  {W1,W2} with primary W2 after commit and after undo; ONE undo entry.

#### L7 — Level switch clears selection
- Before: selection non-empty on L1. Activate L2 via level controls.
- Result: selection EMPTY, primary unset; canvas interaction never changes
  level state (section 2.4 transition table stands).

#### L8 — Duration never creates long-press behavior
- View: defaults; touch. Before: L1 scene.
- Input: press on `Wb`'s region at plan (2000,1450) — raw device
  (1424.0, 478.0) px → logical (712.0, 239.0); hold 2 s WITHOUT movement;
  release (displacement 0 < 5 px).
- Result: ordinary TAP semantics (R7.2.1): selects top-ranked candidate
  `Wa`, replacing any prior selection. The hold duration produced NO
  marquee arming, context menu, or drag override.

#### L9 — One entity, multiple native features, one cycle stop

- Before: unjoined wall A runs from (−100,0) to (100,0), thickness 20;
  unjoined wall B runs from (0,−1000) to (0,1000), thickness 20, on the same
  active level. Query (0,0), z=0.1, mouse T=12 gives raw ordered features:
  A's left endpoint, A's right endpoint, A's region, B's region. The endpoint
  keys order by x; A's canonical ring precedes B's at distance zero.
- Selection list: [A,B], not [A,A,A,B]. Three unchanged plain clicks select
  {A}/primary A, then {B}/primary B, then {A}/primary A. Native feature
  inventory and order are unchanged by this selection projection.

#### L10 — Fixed anchor and round-trip reset observations

- Begin with an unchanged ordered selection list [A,B,C], mouse T=12 and
  first logical click (400,300): {A}/primary A. A click at (412,300) advances
  to {B}/primary B at the inclusive boundary. The anchor remains (400,300).
  Observe (412.25,300), then return to (400,300), then click: {A}/primary A,
  not C. This trace specifies selection-state inputs; it assumes queries
  continue to return the stated list and does not override geometric queries.
- Independently restart from a first click selecting A for EACH of: pan by
  (+10,0) logical pixels then (−10,0); zoom from 0.1 to 0.2 then 0.1 with the
  original offsets restored; hide then show an inactive level; hold then
  release Shift; observe touch then mouse pointer type. Each transition is
  observed before the original-position click; every final click selects A.
- Independently accept a batch changing a remote entity while the local list
  remains [A,B,C]. The next click selects A. A read/save/refused edit/no-op
  undo instead leaves the next unchanged click selecting B.

#### L11 — Potential drag preserves or restores the correct selection

- Before: selection {A,B}, primary B. Plain press on B retains {A,B}/B;
  a classified body drag edits B only (plus specified topology consequences),
  and selection remains {A,B}/B after commit.
- Restart from {A,B}/B. Plain press on A tentatively shows {A}/A. Release
  without crossing the drag threshold applies a plain click to the pre-press
  state and leaves {A}/A. Cancel instead restores {A,B}/B exactly. A real
  drag after that press edits A only and leaves {A}/A after commit.
- Restart from {A,B}/B. Modifier press on A changes nothing; click release
  toggles A out, leaving {B}/B. Independently restart from {A,B}/B: modifier
  press on B followed by click release removes B, leaving {A}/primary UNSET.
  Independently restart again: a modifier drag starting on A while B is
  primary emits no edit and leaves {A,B}/B unchanged.

#### L12 — Held-modifier clicks toggle the top entity, without cycling

- Ordered native IDs `[A, A, B]` project to `[A, B]`; start with empty
  selection and no primary at one unchanged logical position.
- Three Shift-clicks yield `{A}` / primary A, then empty / primary UNSET,
  then `{A}` / primary A. Each click ends any cycle.
- Release Shift without moving. The next plain click selects A alone;
  the following unchanged plain click selects B alone.
- Alt-only clicks use the same plain-click sequence, not toggling.
- This is original Domowik behavior; no comparable application's behavior or
  source is claimed as its authority.

#### L13 — A line rectangle is not a point (R7.3.2a)

- Before: F2 state (`W1` (0,0)→(4000,0) and `W2` (4000,0)→(4000,3000), both
  t=200, joined by `J1` at (4000,0)), active level L1.
- Input A: marquee with BOTH corners at logical (912.0, 384.0) — raw device
  (1824.0, 768.0) px — plan (4000,0), the join node.
- Result A: coincident corners, so point-hit semantics: `J1` alone, because
  R7.1.3 ranks the join node above the two regions that meet there.
- Input B: marquee corners logical (912.0, 384.0) and (912.0, 334.0) — raw
  device (1824.0, 768.0) and (1824.0, 668.0) px — plan (4000,0) and
  (4000,500). The rectangle has zero width and 500 mm of height.
- Result B: R7.3.1 intersection semantics, NOT point-hit. Over y ∈ [0,100] the
  segment lies on `W1`'s right face, which selects `W1` because contact is
  boundary-inclusive; the whole segment lies inside `W2`'s rectangle. Both
  walls are selected, in ascending marquee-key order `W1` (box min corner
  (0,−100)) then `W2` (box min corner (3900,0)). Primary is `W2`. The join is
  NOT selected: `join-node` is not a marquee candidate class (R7.3.1).
- The two inputs share a corner and answer differently, which is what makes
  R7.3.2a a rule rather than a restatement.
- **Provenance.** Stated with R7.3.2a on ISSUE-OWNER authority (issue 0059) and
  independently reviewed, NOT by owner direction. The owner has not ratified
  this fixture and it is subject to their review.

#### CR1–CR7 — Every R7.2.3 cycle reset trigger, directly exercised
- Common setup: L1 scene (parallel walls Wa/Wb/Wc of L1). FIRST click:
  raw device (1424.0,478.0) px → logical (712.0,239.0) → plan (2000,1450)
  → selects {Wa}, primary Wa. Each case applies ONE reset trigger, states
  its exact control delta, then clicks again; the per-case EXPECTED result
  is stated explicitly.
- CR1 pointer-beyond-radius: move pointer to raw (1696.0,478.0) px →
  logical (848.0,239.0) → plan (2600,1450) (displacement 136 px > radius);
  move back to (1424.0,478.0); click → selection {Wa} primary Wa (cycle
  restarted at top).
- CR2 view-transform change: shift+wheel with dx = −100 logical px → ox
  512 → 412 (content follows input); the pointer stays at logical
  (712.0,239.0), now over plan (3000,1450) — still inside all three bands;
  click → top-ranked `Wa` afresh (a persisted cycle would have yielded
  `Wb`). Zero operations from the pan.
- CR3 active-level change: activate L2 in the level panel (no canvas
  input); §7.4 clears selection entirely; reactivate L1; click at the
  original coordinates → {Wa} primary Wa. Both flag changes emitted
  nothing.
- CR4 visibility-flag change: set non-active L2 hidden via the level panel
  (control delta: visibility menu, L2 → hidden); click on L1 at the
  original coordinates → {Wa}; the flag change emitted nothing.
- CR5 modifier-state change — OWN expected result: hold Shift; SHIFT-CLICK
  at the same position toggles the ALREADY-SELECTED primary `Wa` OUT:
  selection becomes EMPTY and primary UNSET (R7.2.4 removal-of-primary).
  Release Shift; a subsequent plain click at that position selects the
  TOP-RANKED candidate `Wa` ({Wa}, primary Wa) — proving the cycle index
  was reset, since a persisted cycle would have advanced to `Wb`.
- CR6 pointer-type change: TAP with a touch finger at raw (1424.0,478.0)
  px (touch DPR identical; radius 24 px) → {Wa}; then CLICK with the mouse
  at the same position → restarts from top: {Wa}. The mouse click did not
  advance to `Wb` (reset proven); each step replaced the selection.
- CR7 candidate-set change: delete `Wc` via the level/panel command
  targeting the NON-selected entity (R7.4 row: set unchanged unless it
  contains the deleted ID — selection stays {Wa}); candidates reduce to
  Wa,Wb; click at the original coordinates → top-ranked `Wa` afresh, not
  the would-be successor. The deletion emitted exactly one batch.

#### PH1 — Placeholder-box selection; placeholders never snap sources
- Setup: degraded open where an optional asset instance with NO descriptor
  dimensions renders as the fixed 1000 mm box (§9.6) centred at instance
  origin (2000,0) on active level L1. The active level has **NO wall
  features**: every document-derived snap/hit candidate in this fixture
  would have to come from the placeholder, which is forbidden as a snap
  source (R6.0.5); only LOCAL grid/angle constructions may appear.
- Snap probe: wall-tool chain start clicked at raw device (1514.0,768.0)
  px → logical (757.0,384.0) → plan (2450,0) — within T/z = 120 mm of the
  box's right face x=2500. Winner: `grid` — grid points (2400,0)/(2500,0)
  tie at squared 25 px² and resolve by tuple to (2400,0); the box face
  contributes NO candidate (a violation would select it here). Committed
  start: grid-snapped (2400,0), never box-attracted.
- Hit: click at raw (1424.0,768.0) px → logical (712.0,239.0) → plan
  (2000,0), inside the box → selects the instance via the
  `placeholder-box` class ({instance}, primary instance).
- Marquee: corners raw (1304.0,888.0)/(1544.0,648.0) px → logical
  (652.0,444.0)/(772.0,324.0) → plan (1400,−600)/(2600,600), containing
  the whole box → selects {instance}; boundary-inclusive rules apply
  identically to its §9.6 bounds.
- Deletion of the primary placeholder removes the instance per §9.6
  degraded interaction rules with R7.4 primary-unset propagation
  (selection EMPTY, primary UNSET).

#### PH2 — A non-identity transform leaves the fallback box alone

- Before: PH1's scene, with the instance's transform changed to
  `scaleUniform` 3 and `rotationDeg` 45. Everything else is PH1's.
- Result: the placeholder box is UNCHANGED — still the fixed 1000 mm square
  x ∈ [1500,2500], y ∈ [−500,500], axis-aligned in plan space and centred at
  `translationMm` (2000,0). PH1's marquee, click and deletion results all hold
  unaltered.
- Discriminating values, stated so a wrong implementation fails visibly
  rather than subtly: `scaleUniform` would give a 3000 mm span, and rotating
  a 1000 mm square by 45 degrees would give a bounding box about 1414 mm
  across. Either would move the min corner and so change the R7.1.3 key.
- This fixture exists because PH1 CANNOT discriminate the two readings: its
  instance has an identity transform, under which they agree. See ED-0106,
  which also scopes the rule to the no-dimensions fallback branch — the
  descriptor-dimensions branch of §9.6 remains owed to asset resolution.

## 8. Undo and redo

Status: **Normative**. Rules are identified as
`R8.*`. Undo
restores an equivalent document projection, not byte-identical history
(FORMAT-SPEC section 4; ADR 0004, Proposed).

### 8.1 Architecture

- **R8.1.1 Session-local stacks.** Each open document on each device owns a
  local undo stack and redo stack whose entries reference authoritative
  batches. Executing undo appends a NEW compensating batch built from
  inverse effects along reverse dependency edges (R3.1.5); replay and
  synchronization metadata remain correct because history grows
  monotonically.
- **R8.1.2 Fresh identities.** EVERY executed undo emits a compensating
  batch with freshly minted operation and batch identities (new IDs, actor
  counter advance). Entries are never popped-and-reused. EVERY executed redo
  likewise appends new effects under fresh batch/op identities.
- **R8.1.3 Authorized entity-ID restoration.** Redo restores the SAME entity
  IDs its undone command created or modified — including authorized
  restoration of tombstoned identities, where the tombstone originated from
  THIS device's own undo. Sync-merge-created tombstones are never
  resurrected by redo. Identity reuse applies to entity IDs only;
  operation/batch IDs are always fresh (R8.1.2).

### 8.2 Boundaries

- **R8.2.1 One entry per accepted batch.** Exactly one undo entry per
  accepted operation batch: each accepted chain segment (per-segment
  batching preserved verbatim from R3.1.1), each completed drag, each
  confirmed numeric edit, and every other command defined by a normative
  section, including its incidental-resolution effects (R3.4.4). No group
  commands exist in v0.1 (R7.1.1); should a later normative section define
  one, it is ONE entry like any accepted batch. EXEMPTIONS: (a)
  compensation/restoring batches emitted by executed undos and redos
  (R8.1.2) are stack-mechanics operations — they create NO new undo-stack
  entries themselves; (b) accepted external/replayed batches create NO
  local entries either — they CLEAR both stacks instead (R8.3.1/R8.3.2).
  Only local command batches enter the stacks.
- **R8.2.2 No-entry cases.** Rejected commands and gate hits (R3.1.6,
  F20–F22), cancelled gestures (any source in the section 6.5 table), and
  confirmed-but-unchanged edits emit nothing and leave both stacks
  unchanged.

### 8.3 External-batch fence

- **R8.3.1 Fence.** Any ACCEPTED external or replayed batch —
  sync-delivered, recovery-restored, or CLI-applied — accepted AFTER an
  open completes clears BOTH the local undo stack AND the local redo
  stack. Local undo therefore never crosses an external batch. Batches
  replayed WHILE CONSTRUCTING the projection during recovery open are
  part of that open itself (R8.4.2): they establish the baseline and are
  not post-open deliveries, so they never fire the fence.
- **R8.3.2 External batches never enter local undo.** They appear only in
  the durable operation history and synchronization metadata, never as
  local stack entries.
- **R8.3.3 External batches make the document dirty.** An accepted external
  batch advances the frontier per sync metadata AND marks the document dirty
  relative to the saved target: the next explicit save must write them.
  Dirty clears only per section 9.1's save contract.
- **R8.3.4 External batches during active interactions.** An accepted
  external/replayed batch arriving while ANY local interaction is active —
  drag, marquee, chain preview, snap evaluation, or an open text-field draft
  — MUST first CANCEL that interaction with full restore-exactly semantics
  (preview discarded, pre-gesture selection restored per section 6.5) and
  only then apply. A stale preview may never commit afterwards; the
  interaction's stacks effects follow R8.3.1/R8.3.2 as for any other
  trigger. The text-field draft reverts without commit (uncommitted focus
  loss, R8.6.2).

### 8.4 Invalidation, persistence, compaction, capacity

- **R8.4.1 Redo invalidation.** The redo stack clears when an accepted LOCAL
  document command commits outside redo execution, or when an accepted
  external/replayed batch applies (R8.3.1). Compensation batches generated
  by executed undos do NOT clear redo. Explicit saves, autosaves, view-state
  changes, selection/hover/guide changes, snap-preference changes,
  rejections, and cancellations never clear redo and never push entries.
- **R8.4.2 Session scope.** Undo/redo stacks are session-local and never
  persisted. Reopening a document, opening from recovery artifacts, or
  degraded-mode transitions clear both stacks completely (also R6.5.7).
  Recovery open additionally establishes the §9.2-style saved-source
  baseline over the OPENED package's exact bytes — including every batch
  replayed while constructing that open's projection — and sets dirty
  FALSE relative to that opened source. Replayed-open batches never fire
  the R8.3.1 fence (the fence governs only post-open deliveries); the
  first subsequent edit marks the document dirty exactly once.
- **R8.4.3 Compaction interaction (non-silent).** Where the retention policy
  cannot preserve the operations a live stack needs to resolve, affected
  stacks truncate to the retained prefix and a NON-BLOCKING diagnostic names
  the truncation. Silent breakage of undo or redo after compaction is
  forbidden. Retention thresholds remain format/implementation policy
  (issues 0004/0005/0027); the observable contract here is normative.
- **R8.4.4 Finite capacity on BOTH stacks (explicit, independent).** The
  undo stack and the redo stack EACH have a finite capacity; the two bounds
  are independently settable implementation policy, but their existence and
  exhaustion behavior are normative: exceeding a capacity drops the OLDEST
  entry of THAT stack — for undo its bottom (least recent) entry, for redo
  its bottom entry (farthest from the current position) — and never blocks
  or corrupts any command or stack operation; exhausting ONE stack leaves
  the OTHER stack unchanged. Because the bounds are independent, redo
  overflow IS reachable through undos alone (U11).
  Exhaustion MAY surface a non-blocking drop diagnostic; correctness is
  unaffected either way.

### 8.5 Exact undo matrix

| Trigger | Undo stack | Redo stack | Saved frontier / dirty | Entries |
|---|---|---|---|---|
| Accepted local command | push 1 | cleared | dirty advances beyond saved frontier | 1 new batch |
| Executed undo | pops; source recorded for redo | gains redo entry | frontier advances honestly via compensation batch; dirty reflects compensated state | 1 new compensating batch, fresh op/batch IDs |
| Executed redo | gains undo entry back via new compensation | pops | same honesty rule | 1 new batch restoring original entity IDs (fresh op/batch IDs) |
| Rejected command / gate hit | unchanged | unchanged | unchanged | none |
| Cancelled gesture (any source) | unchanged | unchanged | unchanged | none |
| Field confirm with unchanged value | unchanged | unchanged | unchanged | none (no-op) |
| Explicit save success | unchanged | unchanged | saved frontier = current; dirty clears | none |
| Explicit save failing BEFORE replacement (section 9.1 steps 1-3) | unchanged | unchanged | dirty retained; saved baseline/frontier unchanged; target is the PREVIOUS package | none |
| Replacement INTERRUPTED (section 9.1 step 4): target is old OR new, both complete | unchanged stacks pending re-read | unchanged | saved baseline AMBIGUOUS between previous-baseline bytes and just-written candidate bytes; the interface MUST resolve by comparing the target's exact bytes against BOTH (reader-equivalent validation of whichever matches) before further saves and then restore truthful dirty/frontier state | none |
| Replacement completed; directory sync fails/unsupported (section 9.1 step 5) | unchanged | unchanged | saved baseline/frontier advance to current; dirty CLEARS; result acknowledges reduced durability per section 9.1 step 5 | none |
| Autosave artifact write | unchanged | unchanged | dirty retained (never cleared, section 9.3) | none |
| Accepted external/replayed batch | CLEARED (fence) | CLEARED (fence) | frontier advances; DIRTY becomes TRUE vs saved target | none locally |
| Reopen / recovery open / degraded transition | cleared | cleared | recovery open: baseline over opened package bytes; dirty FALSE vs opened source | none |
| Compaction truncating reachable history | truncated to retained prefix | truncated likewise | unchanged | non-blocking truncation diagnostic |
| Capacity exhaustion of the UNDO stack | oldest entry dropped; REDO unchanged | unchanged | unchanged | optional non-blocking drop diagnostic |
| Capacity exhaustion of the REDO stack | unchanged | oldest (bottom) entry dropped; UNDO unchanged | unchanged | optional non-blocking drop diagnostic |

Diagnostics are never operations. Dirty/saved-frontier bookkeeping follows
section 9.1's explicit-save contract; undo compensation after a save makes
the document dirty again exactly as any accepted batch would. Per the R8.2.1
exemption, executed undo/redo push NO new entries from their own
compensation/restoring batches — those rows' stack changes are exactly the
stated pop/push of the moved entry, never an extra push.

On one-stack capacity exhaustion the OTHER stack remains unchanged.

### 8.6 Text fields: focus, IME, scrubbing, coalescing

- **R8.6.1 Total lifecycle.** Focus shows the stored value rendered in the
  current display unit. All input between focus and confirm — keystrokes,
  IME composition, and SCRUBBING (dragging steppers/sliders through
  intermediate values) — coalesces into ONE local draft; nothing intermediate
  reaches the document. Confirm emits at most ONE batch covering the entire
  draft if the value changed, else NOTHING.
- **R8.6.2 Committed versus uncommitted focus loss (total).** Focus loss
  COMMITS the draft ONLY when transfer is user-initiated navigation into
  another VALUE-ACCEPTING control: Tab/shift-Tab, or clicking into another
  field or value editor. Every other focus-loss source REVERTS the draft
  without commit, emitting nothing: clicks on the CANVAS or tool activation
  (they begin unrelated gestures); transfers to NON-VALUE controls such as
  toggles, panels, and menus; window-manager/system losses (alt-tab,
  notifications, suspension); and DOCUMENT CLOSE, where the draft is
  inherently unpersistable. Reverting leaves the stored value intact and
  both stacks unchanged.
- **R8.6.3 IME discipline.** IME composition text composes locally and is
  NEVER a partial document edit; composition must resolve into the field's
  draft before confirm applies.
- **R8.6.4 Units and rejection.** Switching units mid-edit reverts the field
  to the stored value rendered in the new unit (section 1.4). An unparsable
  or non-finite confirm leaves the field in editing state with the stored
  value intact and emits nothing. A rejected mutation behaves per R3.1.6:
  empty batch, no entry. Escape always reverts and emits nothing,
  regardless of draft state.

### 8.7 View-state firewall

Nothing classified view or transient in sections 1, 6, or 7 ever enters
either stack (reinforces sections 1.6 and 2.4).

### 8.8 State ownership matrix

| State | Class |
|---|---|
| Committed post-snap command coordinates | Authoritative operation inputs |
| Live drag/chain preview, unsnapped cursor | Transient view state |
| Winning snap/hit candidate computation | Derived, recomputable, never stored |
| Alignment guides | Transient visuals |
| Selection set, primary, hover, cycle index | Local transient, never persisted |
| Snap preferences, bindings | Local per-device view state; toggles default enabled per session |
| Undo/redo stacks | Local session state, never persisted |
| Compensating/restoring batches emitted by undo/redo | Durable authoritative operations |
| Saved frontier, dirty flag, saved-source baseline | Local session state derived from durable history |

### 8.9 Performance ownership matrix

Qualitative scopes only, per section 1.7; numeric targets are owned
elsewhere and no timing-dependent assertion appears in any correctness test.

| Concern | Owner | Normative here |
|---|---|---|
| Candidate-query architecture (snap/hit/marquee) | Issue 0012 (spatial index) | Indexed visible-region queries MUST suffice; per-move whole-document scans are architecturally forbidden |
| Benchmark corpora, percentiles, memory limits | Issues 0032/0033 | None specified here |
| Preview update cost | Issue 0033 | Previews emit ZERO document operations; no document-wide recomputation |
| Candidate scope | This section | Active level × visible region only |
| Axis-extension query bounds | Issue 0012 (implementation) | Bound is normative here: each axis line is searched only over its intersection with the visible plan rectangle expanded by T/z — every qualifying projection of an in-viewport pointer lies inside; nothing outside qualifies |

### 8.10 Normative fixtures (U-series)

View conventions per section 6.7. Canvas clicks below use the default view;
panel interactions state N/A coordinates.

#### U1 — Chain of three segments; independent boundaries; redo ID reuse
- Before: empty document, level L1.
- Inputs (wall tool): clicks at plan (1000,0)/(2000,0)/(2000,1000)/
  (1000,1000) — raw device respectively (1224.0,768.0), (1424.0,768.0),
  (1424.0,568.0), (1224.0,568.0) px.
- Effects (three batches, one per accepted segment):
  `[create Wall c1 supplied:idC1 {start:(1000,0), end:(2000,0), t:200,
  h:2500, style:S0}]`; then
  `[create Wall c2 supplied:idC2 {start:(2000,0), end:(2000,1000),
  t:200, h:2500, style:S0}; create Join Ja {(c1,End),(c2,Start)}]`; then
  `[create Wall c3 supplied:idC3 {start:(2000,1000), end:(1000,1000),
  t:200, h:2500, style:S0}; create Join Jb {(c2,End),(c3,Start)}]` — all
  created entities carry explicitly resolved fields (R3.0.6); Ja/Jb are
  command-supplied fresh identities.
- Trace: stack depth 3. Three undos apply three compensating batches
  (fresh IDs each) restoring the empty projection; three redos append
  three NEW batches RESTORING entity ids c1,c2,c3 with their joins
  (R8.1.3). Matrix rows: accepted command ×3; undo ×3; redo ×3.

#### U2 — Drag cancel emits nothing
- Before: F2 state. Input: begin joint drag at S3's start coordinates; press
  Escape mid-preview.
- Trace: gesture cancelled, pre-gesture SELECTION restored (R6.5.9); state
  identical to F2; both stacks unchanged; no entry. Row: cancelled gesture.

#### U3 — Field lifecycle totals
- Before: `W1` t=200; undo-stack depth 0. (a) Edit thickness to 250, Enter:
  one batch `[update W1 {thicknessMm:250}]`; undo-stack depth 0 → 1; durable
  history gains exactly one batch. (b) Re-open showing 250,
  confirm unchanged: NO-OP, no entry. (c) Open, type garbage, Escape:
  nothing. (d) Switch unit mm→cm mid-edit: reverts to stored value in cm.
  (e) Scrub stepper 200→180→250 in one focus period then Tab into another
  field (COMMITTED focus loss): one batch setting 250. (f) Same scrub then
  alt-tab away (UNCOMMITTED focus loss): revert, no entry.

#### U4 — Honest edge-class start snap; identity under R3.1.3 explicit
- Before: F4-style setup — isolated `Wa` (0,0)→(6000,0), t=200, h=2500,
  style S0; empty stacks.
- Inputs (wall tool): FIRST click raw device (1624.0,780.0) px → logical
  (812.0,390.0) → plan (3000,−60): clicked near `Wa`'s BOTTOM face.
  COMPLETE candidate scan — every qualifying candidate, no omissions:

  | Class | Target | Squared distance |
  |---|---|---|
  | edge (bottom face) | (3000,−100) | (0²+40²)·z² = 16 px² |
  | grid | (3000,−100) | 16 px² — IDENTICAL target to the bottom-face edge |
  | axis ON-segment | (3000,0) | (0²+60²)·z² = 36 px² |
  | grid | (3000,0) | 36 px² |

  The TOP-face edge (3000,100) does NOT qualify: its squared distance
  (0²+160²)·z² = 256 px² exceeds the 144 px² radius.

  Winner resolution under R6.3.1: the two lowest-distance candidates are
  the bottom-face EDGE and a GRID point at the IDENTICAL target
  (3000,−100); class-major promotes the edge, which also beats the other
  edge (top face, 256 px²) on distance within its own class. The winner
  is `edge` (3000,−100) for BOTH reasons — no rescue either way.
- SECOND click raw device (1624.0,268.0) px → logical (812.0,134.0) →
  plan (3000,2500): winner `grid` (3000,2500) at 0 px² (no higher class
  within radius).
- Effects — classification runs on FINAL COMMITTED positions (R6.6.1):
  Wb's committed axis (3000,−100)→(3000,2500) properly crosses `Wa`'s
  axis at (3000,0), interior × interior → EVERY participating wall splits
  there. ONE batch containing:
  `[create Wall WaChild supplied:idD {start:(3000,0), end:(6000,0),
  t:200, h:2500, style:S0};
  create Wall WbBot supplied:idB2 {start:(3000,0), end:(3000,2500),
  t:200, h:2500, style:S0};
  create Wall Wb {start:(3000,−100), end:(3000,0), t:200, h:2500,
  style:S0};
  create Join J supplied:idJ {(Wa,End),(idD,Start),(Wb,End),(WbBot,Start)};
  update Wa {end:(3000,0)}]`.
  IDENTITY per R3.1.3, explicit: the newcomer commanded as
  (3000,−100)→(3000,2500) splits at (3000,0); the child containing its
  ORIGINAL START endpoint (3000,−100) — here presented as `Wb` — RETAINS
  the original commanded identity; the other child takes supplied:idB2.
  Likewise `Wa` retains its id because the ORIGINAL-start child
  (0,0)→(3000,0) contains (0,0). Join J's four memberships all sit at
  the exact coordinate (3000,0). ONE undo entry; undo applies inverse
  effects along reverse dependencies restoring full-length `Wa`; redo
  restores every entity ID under a fresh batch. Rows: accepted command;
  undo; redo.

#### U5 — Tombstone restoration authorized
- Before: empty document, level L1. Command: create wall with supplied id
  e9 — clicks as U1's first two — effects
  `[create Wall We9 supplied:e9 {start:(1000,0), end:(2000,0), t:200,
  h:2500, style:S0}]`; explicit save succeeds (baseline covers We9); undo
  (compensating batch B_k with fresh IDs tombstones We9); redo.
- Trace: redo appends NEW batch/op IDs whose effects recreate entity id e9
  (authorized restoration of this device's own undo tombstone, R8.1.3).
  After undo the document is dirty again; after the earlier save the
  matrix's save row had cleared dirty.

#### U6 — Intervening local command clears entire redo stack
- Sequence: create A — `[create Wall A supplied:idA {start:(1000,0),
  end:(2000,0), t:200, h:2500, style:S0}]`; undo A; create B —
  `[create Wall B supplied:idB {start:(3000,0), end:(4000,0), t:200,
  h:2500, style:S0}]` (disjoint from A's span; no contacts).
- Trace: redo stack cleared at B (row: accepted command). Subsequent redo
  attempt: no-op, no state change, no error.

#### U7 — External-batch fence clears BOTH stacks and dirties
- Sequence: local create A; explicit save (dirty clears); an accepted
  replayed/external batch X applies.
- Trace: BOTH stacks cleared (R8.3.1); undo attempt is a no-op; local undo
  can never revert X or anything beneath it; X is not a local entry
  (R8.3.2); document becomes DIRTY vs the saved target though X itself made
  no local entry (R8.3.3). The next save writes X's state.

#### U8 — Save failure retains dirty state and stacks
- Before: non-empty stacks, dirty document. Explicit save fails at content
  sync (section 9.1 step 3).
- Trace: stacks unchanged, dirty retained, saved frontier unchanged (matrix
  row). A later successful save then clears dirty without touching stacks.

#### U9 — Reopen clears stacks; recovery establishes baseline
- Sequence: build stack depth n; close; reopen the target normally.
- Trace: both stacks EMPTY; first undo attempt no-op; dirty FALSE against
  the target baseline (rows: reopen; R8.4.2). Variant: target invalid,
  recovery artifact opened — same clearing PLUS baseline over the opened
  artifact's bytes; first local edit sets dirty exactly once.

#### U10 — Compaction truncation diagnostic is observable (executable)
- Setup: empty document; commands K1,K2,K3 accepted in order → undo stack
  depth 3 with entries e1↦K1, e2↦K2, e3↦K3; explicit save succeeds,
  frontier F_s covers batches B1,B2,B3.
- Compaction runs under a retention policy that writes snapshot S covering
  exactly F(K2) — the effects of K1 and K2 — retains batch B3 as a live
  operation, and DROPS the batch records of B1,B2 into the snapshot.
- Trace: compaction completes; entries e1,e2 cannot resolve against
  retained metadata → the undo stack truncates to [e3]; the redo stack was
  already empty. A NON-BLOCKING DIAGNOSTIC names the boundary: "snapshot
  frontier covers K1–K2; 2 undo entries truncated". Subsequent behavior:
  one undo SUCCEEDS (reverts K3 via retained B3); a second undo attempt is
  a no-op. Silent breakage would violate the contract (R8.4.3).

#### U11 — Capacity exhaustion drops oldest; redo overflow via undos ONLY
- Setup: independent capacities k_undo = 5, k_redo = 3 (illustrative
  policy). SIX accepted commands C1..C6 in order → pushing C6 EXHAUSTS
  k_undo and drops C1's entry; the stack holds [C2,C3,C4,C5,C6].
- Phase 1 (five UNDOS, no intervening commands): each undo pops the top and
  pushes a redo entry r_i restoring the undone command. Undos pop
  C6,C5,C4,C3,C2 pushing r1..r5; r1..r3 fill the redo stack, r4's push
  EXHAUSTS k_redo and drops bottom entry r1, r5's push drops r2. Redo
  stack: [r3,r4,r5]. NO new commands occur, so nothing clears redo
  (R8.4.1).
- Phase 2: three redos succeed — r5 restores C2, r4 restores C3, r3
  restores C4 (each as a fresh batch reusing entity IDs, R8.1.3); the redo
  stack is then EMPTY even though C5,C6 restorations were dropped: a fourth
  redo attempt is a no-op, optionally preceded by the non-blocking drop
  diagnostic from phase 1 (R8.4.4).

#### U12 — IME composition never a partial edit (ASCII)
- View: panel field focus; N/A canvas coordinates. IME active with ASCII
  output mode.
- Input: type "2", "5", "0" through the composition buffer (underlined
  "250" pending), commit the composition, then Enter.
- Trace: composition keystrokes emitted NOTHING (no intermediate batches);
  the single confirm batch updates thicknessMm to 250 (R8.6.1/R8.6.3).

#### EB1 — External batch during an active drag cancels before applying
- Setup history (all accepted local batches B1..B3): chain creates W1,W2
  with join J1 (F2 state; B1,B2); create isolated W3 (6000,−2000)→
  (8000,−2000), t=200, h=2500, style S0 (B3).
- EXPLICIT SAVED-TARGET BASELINE #1: an explicit save succeeds — baseline
  over the saved bytes covers B1..B3; dirty FALSE; frontier F_s = cover
  (B1,B2,B3).
- Undo of B3: compensating batch B3′ (fresh IDs) tombstones W3 → undo
  stack [e1↦B1, e2↦B2] (e3 was popped), redo stack [r1 restoring W3's
  identity]; dirty TRUE (compensation batch).
- EXPLICIT SAVED-TARGET BASELINE #2: a second explicit save succeeds —
  baseline now covers B1..B3′; dirty FALSE again. Both stacks remain
  nonempty ([e1,e2] / [r1]); frontier F_s′ = cover (B1,B2,B3′).
- Pre-gesture SELECTION: click-select `W1` at its BODY-ONLY point
  raw device (1324.0,758.0) px → logical (662.0,379.0) → plan (1500,50):
  inside W1's rectangle; the NEAREST endpoint/join node (0,0) sits at
  exact squared distance (1500²+50²)·z² = 22525 px² ≫ 144 — no handle
  or node within any radius → unambiguous region hit → selection {W1},
  primary W1.
- Active gesture: joint-drag J1 captured, preview moving toward (4000,900)
  (S9 coordinates).
- Input: an accepted external/replayed batch X arrives NOW with effect
  `[create Wall X1 supplied:idX {start:(9000,−2000), end:(11000,−2000),
  t:200, h:2500, style:S0}]`, advancing the frontier past F_s′ per sync
  metadata.
- Resulting trace (R8.3.4 → R8.3.1/R8.3.2/R8.3.3):
  1. interaction cancelled FIRST — preview discarded; pre-gesture state
     AND selection restored exactly ({W1}, primary W1); zero operations;
     no entry from the drag;
  2. X applies under its own external actor key: the frontier gains an
     entry beyond F_s′ for that actor (e.g., A:4 local, EXT:5 external);
  3. BOTH stacks clear: [e1,e2] gone, forfeiting r1's pending
     restoration of W3 (R8.3.1);
  4. the subsequent pointer-up has no owning gesture → discarded entirely
     as stale (R6.5.4): the stale preview NEVER commits.
- Post-state assertions: document = F2 walls + tombstoned W3 + X1;
  selection {W1} primary W1; both stacks EMPTY; dirty transitioned
  FALSE → TRUE against saved-target baseline #2 — attributable solely to
  X (no local batch intervened).

#### VF1 — View-state firewall holds idle and mid-gesture (executable)
- Idle phase on the L1 scene, starting view {z=0.1, offsets (512,384)},
  empty stacks, clean document:
  1. plain wheel dy = +100 logical px → pan FIRST: oy 384 → 484;
  2. THEN ctrl+wheel dy = +100 at cursor raw (1400.0,600.0) px → logical
     (700.0,300.0): factor 1.25^(−100/100) = 0.8 anchored there. Plan
     under the cursor AFTER the pan: x = (700−512)/0.1 = 1880,
     y = (484−300)/0.1 = 1840. z′ = 0.08;
     ox′ = 700 − 0.08·1880 = 549.6 (exact);
     oy′ = 300 + 0.08·1840 = 447.2 (exact).
     Final idle view {z=0.08, offsets (549.6,447.2)}.
  3. grid preference toggled OFF then ON via the snap menu;
  4. hover moves across raw (1000.0,400.0)/(1200.0,500.0) px;
  5. CORRECTED POST-TRANSFORM SELECTION INPUT: click at raw
     (1419.2,662.4) px → logical (709.6,331.2) → plan
     ((709.6−549.6)/0.08, (447.2−331.2)/0.08) = (2000,1450) — inside all
     three bands → selects top-ranked `Wa`; then idle Escape clears.
  6. Alt pressed and released with no gesture active.
- Idle assertions: operation log EMPTY after every step; undo/redo stacks
  unchanged (empty); frontier unmoved; dirty FALSE throughout.
- Mid-gesture phase: restore view {z=0.1, offsets (512,384)} on a fresh
  F2 state. Alt is HELD FROM BEFORE THE GESTURE START, explicitly
  disabling snapping for this gesture (R6.4.2). Joint-drag J1 begins at
  raw (1826.0,770.0) px → logical (913.0,385.0) → plan (4010,−10).
  DURING the preview, plain wheel dy = +100 pans oy 384 → 484. Release
  with capture WITHOUT further pointer movement: the release position
  logical (913.0,385.0) now maps to plan
  ((913−512)/0.1, (484−385)/0.1) = (4010,990). Snapping was disabled at
  gesture start, so the RAW coordinates commit unsnapped:
  `[update W1 {end:(4010,990)}; update W2 {start:(4010,990)}]` — full
  mapping stated, ONE batch, geometry-only fields.
- Mid-gesture assertions: preview tracked the new transform with ZERO
  operations during panning; undo-stack depth 0 → 1; one undo restores
  F2; no view field ever appears in any operation (R8.7).

## 9. Save, recovery, and missing assets

Status: **Normative** (issue 0020).

Cross-reference (issue 0023): format-level mechanics that implement this
section — consolidated resource ceilings, rejection taxonomy with
machine-readable categories, container-neutral save/replacement state
machine and platform durability profiles, recovery-state observability
categories, and the fault-injection cross-reference — are ratified in
[FORMAT-SPEC.md](FORMAT-SPEC.md) §9 and constrain implementations without
modifying any rule in this section. Where this section and FORMAT-SPEC §9
could appear to differ, this section owns observable behavior; FORMAT-SPEC
§9 owns format-level enforcement mechanics.

### 9.1 Explicit save contract

The experimental desktop editor uses native file choosers for Open and Save.
Open selects one local file. Save rewrites the plan's own file — the path it
was opened from or last successfully saved to — without asking again; Save As
asks for a target, suggesting the current plan filename or `plan.domo`. A plan
that has no file yet saves through the same chooser. Where the host cannot
rewrite a file in place — a browser download — every save goes through the
chooser; in a browser that can confirm writes, the confirmed file plays the
same role as a path and plain Save rewrites it. Whichever chooser opens offers
a `.domo` filter. Cancelling returns no target and does not change the
document or report a file-action error. These rules apply on Linux as well as
macOS (issue 0081).

Only an explicit save replaces the target file. The writer performs, in
order:

1. build a complete candidate package beside the target, in the same
   directory and filesystem;
2. validate the candidate with reader-equivalent checks;
3. sync the candidate's contents to stable storage;
4. replace the target atomically within the same filesystem;
5. sync the parent directory where the platform supports it.

Guarantee: if an interruption occurs before step 4 completes, the target
remains the previous complete package; if step 4 completes, the target is the
new complete package. Old-or-new readability holds under these stated
assumptions: a local filesystem with atomic same-filesystem rename semantics
and functioning file syncing.

First save: when the target does not yet exist, the steps are identical. On
success the target exists and is valid; on failure before step 4 no target
file exists, and any leftover candidate is ignorable and cleanable.

Error acknowledgement at every step:

- Step 2 failure (candidate does not validate): abort with an actionable
  error; target untouched.
- Step 3 failure (content sync fails): abort with an error; target untouched.
- Step 4 failure (replace fails): report the error; target remains whatever
  the filesystem left, which under the rename assumption is old or new.
- Step 5 failure or unsupportability (directory sync fails or the platform
  has no equivalent): the completed replace is not rolled back; the save
  result acknowledges the caveat so users know durability is reduced until
  the platform flushes.

Declared non-guarantees: there is no universal power-loss guarantee
(filesystems that ignore sync requests may revert a completed replace);
concurrent writers are unsupported (the last completed replace wins; each
outcome remains individually valid); network and cloud-synced filesystems are
outside the guarantee.

### 9.1.1 Leaving an unsaved editor session

Status: **Normative** (2026-09-05; issue 0040, original product decision).

- **R9.1.1a Dirty state.** An opened file starts clean. A new empty plan's
  default level is its clean starting state, with empty undo/redo stacks;
  creating that baseline is not a user command. Every subsequently accepted
  document batch, including an executed undo/redo, marks the session dirty
  until an explicit save succeeds under section 9.1. A no-op, rejected edit,
  selection, view change or cancelled gesture does not make it dirty.
- **R9.1.1b Replacement guard.** New, Open, window close and application quit
  must offer Save, Discard and Cancel before abandoning a dirty session.
  Cancel is the default and dismissing the prompt is Cancel. Save permits
  the pending action only after a successful save. Cancelling a save dialog
  or a failed save retains the current session and aborts the pending action.
- **R9.1.1c Atomic adoption.** Choosing Discard permits abandoning the old
  session only once its replacement has been successfully created or opened.
  A cancelled Open dialog or invalid file retains the current session and
  its dirty state. Merely choosing Discard does not clear dirty.
- **R9.1.1d Exclusive lifecycle action.** While a file/confirmation dialog or
  a pending replacement is active, the editor accepts no new document edits
  and starts no second lifecycle action. This prevents a save response from
  approving the loss of edits made while the prompt was open. View and
  selection state are not persisted by this mechanism.
- **R9.1.1e Feedback.** The document title indicates unsaved changes. Save
  errors remain visible with the current document available for retry.

Fixtures: create a wall, cancel New → same wall and dirty state; create a
wall, choose Save for Open, cancel the save dialog → same wall and no Open
dialog; choose Discard for Open, cancel file selection → same wall and dirty
state; choose Save for close and fail the write → window stays open and dirty;
successfully save then execute undo → dirty even if the wall count equals an
earlier saved projection (the compensation batch is still unsaved history).

Compatibility/provenance review: these are original GUI lifecycle choices,
consistent with the saved-frontier/dirty matrix in section 8.5 and atomic
save rules in section 9.1. No external application implementation or assets
were consulted; no stored entity, operation or container format changes.

### 9.2 External modification detection

At load time the writer records a baseline over the target's exact bytes. At
explicit save time it refuses to overwrite silently when the target's current
bytes differ from that baseline; it requires either explicit user
confirmation or a save-as. This check assumes an exclusive writer between
check and replace. Race boundary, declared explicitly: a modification landing
after the baseline comparison but before the replacement is outside detection
capability and falls under the concurrent-writer non-guarantee of section
9.1 — the last completed replace wins and each outcome remains individually
valid.

### 9.3 Autosave and recovery artifacts

Autosave is optional and disabled by default in v0.1; enabling it is a
per-device setting that also sets the interval. Exact trigger behavior:

- An autosave fires when the document has unsaved changes and the configured
  interval has elapsed since the last autosave or explicit save, and
  additionally on application lifecycle hide/background events where the
  platform permits.
- Autosave never runs while an explicit save is in progress; it skips and
  waits for the next trigger.
- A failed autosave retries at the next trigger; failures surface as
  non-blocking notifications and never interrupt editing.
- Autosave writes only a separate complete recovery artifact; it never writes
  the target file and never clears the unsaved-changes state.

This specification does not mandate artifact names, physical container form,
or write mechanisms. A recovery artifact must itself be a fully valid package
carrying its own save timestamp and a monotonic per-device sequence number.

Ordering and identity of artifacts:

- Same-device candidates order solely by greatest monotonic per-device
  sequence number; within one device it strictly increases, so ties cannot
  occur locally.
- Timestamps are display-only metadata and never affect ordering.
- Cross-device candidates, or any same-sequence collision, have no automatic
  order: no artifact is auto-selected and the user chooses explicitly from
  the validated candidates.

Discovery and cleanup:

- Discovery enumerates candidate artifacts associated with the document by an
  implementation-defined mechanism and admits only artifacts that pass full
  semantic validation, not merely structural parsing. Invalid candidates are
  excluded and reported.
- Cleanup of superseded artifacts happens only through explicit user action.
  Automatic cleanup is limited to leftovers that are provably not packages,
  such as truncated write-candidates from failed saves.

### 9.4 Recovery precedence

| Case | Observable behavior |
|---|---|
| Target passes complete reader-equivalent semantic validation (never parse-only) | Open the target. Artifacts stay untouched; the interface may report stale artifacts. |
| Target missing or invalid; at least one artifact validates | Same-device candidates order solely by greatest sequence; cross-device or same-sequence ambiguity requires explicit user choice per section 9.3. Opening a candidate requires user consent; replacing the target happens only explicitly, otherwise via save-as. |
| Target missing or invalid; no artifact validates | Opening fails with diagnostics naming the target and every artifact attempted. |
| Individual artifact corrupt, partial, or semantically invalid | Skipped by precedence; never auto-promoted. |

### 9.5 Fault-injection outcomes

The complete normative fault matrix:

| # | Injection point | Guaranteed observable outcome |
|---|---|---|
| F1 | First save, no prior target; failure before replace | No target file exists; leftover candidate ignorable and cleanable |
| F2 | Candidate write interrupted | Target untouched: previous package readable; leftover candidates ignorable and cleanable |
| F3 | Candidate validation fails | Save aborts with an actionable error; target untouched |
| F4 | File-content sync fails | Save aborts with an error; target untouched |
| F5 | Atomic replacement interrupted | Old-or-new readable, per the rename-atomicity assumption |
| F6 | Replacement completed; directory sync fails or unsupported | New package readable but durability uncertain until the platform flushes; the result acknowledges the caveat; no rollback |
| F7 | Recovery-artifact write or sync fails | Artifact discarded or marked invalid and excluded from precedence; target unaffected |
| F8 | Cleanup interrupted | Artifacts remain valid or excluded per discovery rules; never partially promoted |
| F9 | Autosave failure | Non-blocking notification; retry at next trigger; unsaved-changes state unchanged; target untouched |

Validation profiles. Old-or-new readability including power-loss behavior is
claimed only for these profiles and only after their fault-injection suites
pass:

- Linux with ext4 on local storage: atomic rename plus file and directory
  fsync;
- Windows with NTFS on local storage: `ReplaceFile` plus `FlushFileBuffers`.

All other platforms are process-crash-only under explicitly declared
capabilities; no power-loss claim is made for them.

### 9.6 Missing assets and degraded inspection

Asset identity is the content digest. Digest mismatch is treated exactly like
a missing blob. Invalid bytes are never decoded and never rendered.

Read-only degraded inspection means: pan, zoom, viewing, and diagnostics
work, and CLI `info` and `validate` operate, while interactive editing and
normal saving are unavailable.

Complete condition matrix (requirement level × package profile × blob
condition):

| Requirement | Profile | Blob condition | Observable behavior |
|---|---|---|---|
| Required | Thin | Missing or digest mismatch | Editable open blocked; read-only degraded inspection available; placeholder rendering; blocking diagnostic |
| Required | Thin | Digest valid but undecodable or unsupported encoding/media type | Same as missing |
| Required | Thin | Offline; retrieval disabled or unreachable | Same as missing; counted in the offline summary |
| Optional | Thin | Missing, digest mismatch, undecodable, unsupported, or offline | Document opens editable; placeholder rendering; non-blocking diagnostic; offline cases counted in the summary |
| Any | Portable pack run | Reference cannot be embedded and digest-verified | Packing fails completely with an omission report naming every such asset |
| Any | Portable pack run | Blob embeds and verifies but is undecodable locally | Packing succeeds; bytes are preserved opaquely; a warning reports that the content cannot be decoded on this device |
| Required | Portable read | Embedded blob missing, digest-mismatched, or corrupt | Normal open refused; read-only degraded inspection available; the package's portability claim is never presented as holding |
| Required | Portable read | Embedded blob present, digest valid, unsupported encoding/media type | Read-only degraded inspection available; editable open blocked |
| Optional | Portable read | Embedded blob missing, digest-mismatched, corrupt, or unsupported | Opens editable with placeholder rendering; non-blocking diagnostic |

Actionable diagnostics name, per affected asset: identity (descriptor name
plus digest), purpose (requirement level and which entities or levels use
it), and remediation (restore the blob into the cache, re-link the file,
repack portable once available, or remove the referencing instance).

Deterministic placeholders:

- Geometry: when dimensions are declared, use width × depth (height is
  vertical), centered on the instance origin. Apply `scaleUniform` and
  `rotationDeg`, then use the axis-aligned bounds of that rectangle in plan
  space. When none are declared, use a fixed 1000 mm axis-aligned square
  centered on the instance origin, ignoring scale and rotation (ED-0106).
  The declared-branch rule was approved by the owner on 2026-10-08 for issue
  0059. Construction uses binary64 (ED-0106, retained by ED-0112); issues
  0012 and 0030 are closed.
- Interaction: the placeholder participates in hit testing exactly as its
  entity normally would, so instances stay selectable and deletable while
  degraded.
- Export: headless output draws the box outline with an unresolved-reference
  annotation, never asset content.

Reference preservation and reopen: semantic reference preservation applies —
descriptors and every reference survive save cycles with unchanged meaning
and resolvable identity, regardless of blob availability. Reopening applies
this matrix deterministically: identical conditions produce identical open
modes, diagnostics, and placeholder geometry.

Thin packages offline: no network retrieval happens unless retrieval policy
enables it. When assets are unavailable offline, the interface shows one
summary of how many assets are unavailable rather than per-asset noise.
Enabled retrieval inherits every security requirement of FORMAT-SPEC
section 9.

## 10. Furniture and asset instances

### 10.1 Plan-local directory library (issue 0074)

**Normative for the experimental 2D editor.** Original owner-requested behavior:

- Plan-level directory locations belong to one plan, never the application's bundled
  catalog. Browsing recursively enumerates regular GLB, OBJ and ZIP files,
  case-insensitively, without following symlinks. ZIP files are opaque model
  bundles; neither their contents nor another application's catalogs are parsed.
- Browsing copies no model bytes. Placement reads the chosen file with the
  per-blob limit, hashes its exact bytes, embeds it and creates one instance in
  one undoable batch. Identical bytes share a blob. Saved placed instances work
  without their source directory. Library locations persist as optional
  `editorAssetDirectories` manifest metadata; unavailable locations are reported
  in the browser while embedded models remain reusable.
- **Directory chooser (issue 0075):** Add directory opens the system folder
  picker on supported desktop platforms. Cancel is a no-op. Manual path entry
  remains an explicit alternative. Existing library paths can be replaced by
  typing or browsing. Replacement validates the new directory before changing
  the plan, replaces the old location instead of appending it, and deduplicates
  an already attached target. Placed instances and embedded bytes are retained.
- Models have explicitly labelled 2D footprints; §10.2 adds decoded model art
  within those bounds. New placement derives size from the model (§10.4).
  The optional `editorFootprintMm: [width,depth]` instance extension records
  positive finite unscaled dimensions, each at most 1,000,000 mm. The pivot is
  the footprint center, mapping local coordinates with
  `p = translationMm + scaleUniform * R(rotationDeg) * q`, counter-clockwise.
  Missing or malformed legacy footprint metadata falls back to 1000 by 1000 mm for
  this editor view. This footprint is distinct from §9.6 degraded placeholders.
- **Library search (issue 0121, owner request 2026-10-17):** filter by a literal,
  case-insensitive substring of the display name, available full file path,
  source directory or source/provenance description. This includes native
  absolute and browser-relative import paths. Slash and backslash separators
  are equivalent for matching; stored/displayed paths are unchanged. An empty
  query shows all entries, including models without source metadata. Filtering
  is local view state, never a file read, rescan, plan operation or source-file
  mutation. No matches is distinct from an empty library.
- Native code supplies transformed corners and click interaction geometry.
  Footprints rank after existing wall/opening candidates and among themselves
  by stable UUID. Only active-level instances may be selected or edited;
  passive levels follow the existing visibility rules. Individual instance
  placement, transform/size edits and deletion are undoable; library attachment
  and embedded bytes are package metadata and are retained across undo.
- A failed placement changes neither instances nor embedded content. Unknown
  attribution/license fields are preserved; importing user content does not
  assign Domowik's license to it. No decoder or network retrieval is implied.

### 10.4 Model dimensions and resizing (issue 0082)

**Normative experimental-editor behavior.** Placement has no manual fixed-size
inputs. Decode the selected model's scene geometry with static node transforms
before measuring its X/Y/Z bounds (+Y up). GLB/glTF defines metres; convert
extents to millimetres. OBJ/ZIP-OBJ has no supported physical-unit declaration:
preserve its XYZ proportions and assign 1000 mm to its longest extent. Require
positive horizontal extents; flat height receives a 1 mm minimum. If supported
bounds cannot be decoded, use 1000 × 1000 × 1000 mm and show the sizing failure.
Do not silently substitute a cube for a defined physical size exceeding the
editor's supported 1,000,000 mm per-axis bound; refuse placement instead.

Store the initial width/depth as `editorFootprintMm` and height as
`editorHeightMm` in the instance, outside the unchanged source asset bytes.
Reusing embedded models follows the same sizing rules. Existing instance sizes
are retained; without explicit height, 3D height retains the legacy model
height/width ratio applied to its stored width (or width for a missing model).

**Three-axis placement (owner direction, 2026-10-15, issue 0109).** The owner
chose live placement controls rather than direct 3D handles. The shared inspector
uses a fixed Your plan heading followed by Selection and Floor tabs (owner
direction, 2026-10-15, issue 0110). Selection is initially active and shows
selected-object controls first; Floor owns levels, room details and totals.
Selecting an object or opening placement activates Selection at its top. Tabs
retain independent scrolling and preserve a pending placement draft when switched.
These are transient UI states, not document or history data. The inspector
offers plan X/Y, elevation offset above the active level, and rotations X/Y/Z.
Plan axes are right-handed: X right, Y up the plan, Z upward in space. Rotations
are applied X then Y then Z about the object's center; Z is the existing
`transform.rotationDeg`. Dimensions are the object's unrotated local dimensions.
The centered cuboid's unrotated bottom is at level elevation plus the offset;
rotation preserves that cuboid center, so a tilt may extend below that bottom.
Optional editor metadata `editorElevationMm`, `editorRotationXDeg`, and
`editorRotationYDeg` stores the additional pose, defaulting to zero. Elevation
must be finite and within the coordinate bound; angles must be finite. These
follow the existing optional editor-dimension metadata convention, not a new
required format capability. Older editors preserve them as unknown fields but
render without tilt/elevation; accurate pose display requires this increment.
The 2D interaction shape conservatively encloses the projected local cuboid in
its yaw-aligned rectangle; it is not the exact projected model silhouette.
Live preview is session-local, never saved or logged. Apply commits one batch;
Cancel removes the preview. No external application source was consulted.

**Live-preview responsiveness (issue 0112, 2026-10-15).** Continuous controls
coalesce input to the latest valid pose once per frame, rather than building an
input queue. An in-flight preview may display while a newer pose is pending;
document/session changes still discard stale results. Warm updates retain the
scene's framing and geometry and update transforms only. Apply/Cancel discard
queued input, preserving the existing one-batch/no-edit semantics. Presenter
baseline tokens are transient transport state, not document fields.

Right-click on an active-level object offers Select, Resize, Rotate, Copy and
Cut, plus Paste while the clipboard is non-empty; wall-chain
right-click keeps its existing finish behavior. Rotate selects the object and
opens the three-axis placement controls above, superseding issue 0108's
single-angle dialog. Apply is one undoable edit; Cancel and unchanged values
emit no edit. Resize selects the object and
opens width/depth/height in millimetres. Proportions are locked by default:
editing any dimension scales the other two by the same factor. Unlocking permits
independent changes. Apply commits all three dimensions in one undoable batch;
Cancel changes none. Dimensions must be finite, positive and at most 1,000,000 mm.
Keep the footprint center, position, rotation, source bytes and other instance
metadata. Saved dimensions update both views and survive reopening. Object
properties also expose height; resizing never re-decodes models during camera
navigation.

The resize dialog is capped at 420 logical pixels wide. Its object-name title
has at most 50 characters including an ellipsis; hover reveals the full name.
Initial/computed dimension text uses one decimal place in millimetres, while
untouched underlying values retain their precision. A live preview shows cached
top-down object art at the proposed footprint (or a labelled footprint when art
is unavailable), the original footprint outline, and a height comparison.
Invalid input retains the last valid preview and disables Apply. This display
preview never changes the document or adds undo history (issue 0083).

**Asset copy, cut and paste (owner direction, 2026-10-15, issue 0113).** The
owner asked for copy / cut / paste of objects, especially cutting a model on one
level and pasting it onto another. The clipboard holds asset instances only
(furniture/models); walls, openings and room records keep their current
behavior. Copy (Ctrl/⌘+C) and Cut (Ctrl/⌘+X) capture the selected model
instances with digest, name, transform, resized dimensions, elevation and pose.
Cut deletes exactly the captured instances in ONE undoable batch; a refusal
leaves the document and the clipboard unchanged. Paste (Ctrl/⌘+V) creates
fresh-identity copies of every captured instance on the ACTIVE level, anchored
so the reference instance's origin lands at the pointer, or at the viewport
centre when no canvas pointer position is known. It preserves each instance's
transform, dimensions, elevation, pose and the instances' relative arrangement,
is one atomic undoable batch, and refuses whole when the embedded bytes of any
captured model are unavailable or a target is invalid. Selection follows the
pasted anchor. The clipboard is plan-session-local view state, like a live
preview: never persisted, never in the operation log, not restored or cleared by
undo/redo, and discarded when the plan session is replaced. Provenance:
original product direction in the owner's request, not external application
source or behavior. No document-format change.

### Furniture preview and configuration behavior

### 10.2 Imported-model plan previews (issue 0076)

**Normative for the experimental editor.** Imported meshes render as a cached
orthographic view from above (+Y toward the X/Z plane), with diffuse material
colors and simple directional shading. Transparent background preserves gaps
and non-rectangular outlines. The preview fits the instance's configured width
and depth and follows its rotation/translation and level visibility. Footprint
boxes remain interaction/selection bounds, not a replacement for decoded art.
Existing plans regenerate previews from embedded content without source files.
Unreadable or unsupported models retain a labelled fallback and a diagnostic.
Preview generation is derived, off the UI thread and never enters plan history.
Furniture-only plans are not empty for the introductory canvas overlay.

Supported content is polygonal OBJ with diffuse MTL colors inside ZIP bundles,
standalone OBJ, and glTF 2.0 binary meshes with static node transforms and Draco
compression. No external resource fetching, animation, skin deformation or
texture/PBR fidelity is implied by this initial orthographic preview.

**Embedded base-colour textures (issue 0120, owner request 2026-10-17).**
The experimental editor samples embedded GLB PNG, JPEG and WebP base-colour
images in both plan previews and 3D, multiplied by the material base-colour
factor in linear colour space. UVs (including Draco attributes), repeat,
mirrored-repeat and clamp wrapping, and `KHR_texture_transform` are retained.
Images are bounded before allocation; malformed, external or unsupported
referenced textures produce the existing diagnostic fallback, never a network
or filesystem request. Interactive LOD may reduce geometry/image resolution,
but does not replace textures with uniform tints. Decoded images and GPU
resources are cached with the model, not rebuilt during camera navigation.
Normal/metalness/roughness maps, animation and OBJ image maps remain outside
this increment. One imported asset file represents one finish; there is no
multi-finish selector. Original imported bytes, plan persistence and history
remain unchanged.

### 10.3 User configuration (issue 0077)

Reusable asset library locations are local user/device preferences, available
across restarts and across plans. The browser merges them with legacy
plan-level locations. Reading preferences or adding a global library does not
create a plan operation or dirty the plan. Placing a model from a global
library validates its selected source location and embeds its bytes as before.

Version-1 JSON config has `version: 1` and `assetDirectories: [absolute path,...]`.
Unknown preferences survive updates. Reads/writes are limited to 1 MiB and 64
unique locations; invalid/future files are reported and never silently replaced.
Updates write and flush a same-directory candidate, then rename it over the
previous config. This is local preference persistence, not a cross-store plan
transaction or a power-loss guarantee. Paths remain recorded when offline so
users can repair them using the existing path editor or folder picker.

Remaining specification questions include content-license display and the
boundary between catalog metadata and plan-specific metadata. The placement
transform, model dimensions, elevation, and missing-asset behavior are defined
above and in §9.6.

**Resolved-instance interaction geometry (issue 0059, owner-approved
2026-10-08).** Resolved furniture uses the declared footprint and transform in
§10.1 for point-hit and marquee queries. This is separate from the degraded
`placeholder-box` behavior in §9.6; resolved instances must not be routed
through the placeholder query. Whole-app interaction budgets remain with issue
0033.

## 11. Live 3D editor view (issue 0078)

**Normative experimental-editor behavior.** The 3D view is derived from the
same plan as the 2D editor and updates after accepted edits/history and file
replacement. Camera state is local, independent of selection and history.
Orbit navigation revolves around a target; pan translates it; zoom changes
distance; Fit frames visible content. Walk-through uses an eye-level camera
with yaw/pitch and forward/strafe/vertical movement. Switching modes or views
does not edit the plan. Visible levels render at finished-floor elevations,
with slabs extending downward, walls/openings upward, and furniture standing
on its level. Imported mesh pivots are footprint-centered and bottom-aligned.
Surface/unsupported-model failures are surfaced rather than mistaken for an
empty scene. Experimental texture backend evidence does not close ADR 0003's
platform or measured-performance gates.

Performance-first owner direction: camera navigation never decodes model bytes,
triangulates floors, rebuilds scene snapshots or uploads geometry. Imported
meshes are cached by digest and shared between instances; bounded display LOD
and diffuse/approximate transparency are permitted. Camera state is coalesced
per frame, with at most one update in flight, and the camera handler must not
wait on the GPU render lock. These choices never alter plan/model source data.

### 11.1 Editor preferences (issue 0080)

**Normative experimental-editor behavior.** Preferences is accessible from
the editor. The 2D/3D split defaults to vertical: 2D on the left, 3D on the
right. Horizontal places 2D above 3D. Both views receive equal space separated
by a divider. Existing user configurations without an orientation use vertical.
Save persists the choice as user/device configuration and applies it to the
current split view; Cancel discards edits. Failed writes are shown in the
preferences window and leave the applied choice unchanged. Preferences never
dirty a plan or enter undo history. View/camera state survives layout changes.
No plan format or dependency changes are required.

**Unit-system preference (issue 0118, owner request 2026-10-16).** Metric
(the default for existing configurations) uses millimetres and square metres.
Imperial uses decimal inches for length fields/inspector values, feet for
rulers, and square feet for area labels. Save persists units and split layout
together; Cancel or a failed write applies neither. This device-wide preference
survives opening/new plans, overriding the session display-unit default in §2.6.
Only display and numeric parsing change: geometry, snapping/grid spacing, bridge
values, saved documents, CLI exports and undo history remain unchanged.
Decimal-inch fields display up to four decimals; confirming the unchanged
display retains the original stored value without a rounding edit. Metric room
areas retain the core's §5.3.2 string; imperial labels convert the unrounded
core area using 1 ft = 304.8 mm and display two decimals (nearest, half away
from zero), an editor-only exception to §5.3.2's fixed display unit.
Angles remain degrees. Switching units discards any active placement preview;
subsequently opened fields use the new units.

### 11.2 Platform-aware workflow guidance (issue 0084)

**Normative experimental-editor behavior.** Navigation, selection and history
guidance uses the runtime platform, not a user preference or visual theme:
macOS displays Command (⌘); Windows and Linux display Ctrl. Other current
targets use Ctrl as the keyboard fallback. Hints show only the platform's
primary modifier, plus Shift where independently supported. Existing Control
and Meta aliases remain accepted for compatibility. Undo is primary-modifier+Z;
Redo is primary-modifier+Shift+Z in both 2D and 3D. Do not advertise shortcuts
that are not implemented. Shared guidance and shortcut recognition must not
change document semantics, focus routing or gesture cancellation.
