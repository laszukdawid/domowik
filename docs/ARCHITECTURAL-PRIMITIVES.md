# Architectural primitives

Status: normative for the experimental `0.2` architectural schema. Original
Domowik product design; no external application sources or assets consulted.

## Model

Levels are elevation references, not physical floors. A `constructionBody` is a
rectangular prism swept along a plan segment, with independently specified
bottom offsets at its two ends and constant vertical thickness. Its role is
`slab`, `platform`, `column`, `beam`, `guard` or `generic`. Bodies do not enclose
rooms or participate in wall joining. Existing walls retain their authoritative
wall representation and enclosure semantics; they are not duplicated as bodies.
This first body shape supports rectangular construction and longitudinal slopes,
not arbitrary profiles, curved sweeps or general solid modelling.

A `hostedOpening` subtracts its rectangular footprint through either a named
construction body or the derived floor surfaces of a named level. A level-floor
host is an explicit reference to that level's floor collection, not a room ID
whose derived topology can disappear. Cuts affect floor geometry, not detected
room boundaries or gross room areas. An opening alone implies no traversable
connection. Wall doors/windows retain their existing wall-opening representation.

A `levelConnection` has `from` and `to` endpoints, each with a level ID, plan
position and elevation offset. Elevation is always derived from the current
referenced level plus offset. It carries no physical geometry of its own.

A `staircase` references a connection, a width, an integer riser count and zero
or more explicitly associated hosted openings. Its connection runs from lower
to upper elevation. A straight flight with N risers has N−1 equal goings; the
upper floor/landing is the final tread. Tread bodies and optional rails/posts
are derived, not independently persisted entities. Staircases appear on both
endpoint levels, with UP on the lower and DOWN on the upper. Exterior stairs
need not have an opening. Association does not transfer ownership of an opening:
deleting a staircase never implicitly deletes an independently editable cut.

Landings/platforms, columns and beams can be authored as bodies. Ramps can use
sloped bodies and connections; full ramp, railing, roof and lift assembly tools,
multi-flight stairs and automatic attachment constraints are future work, not
implemented merely by naming these foundations.

## Constraints and edits

### Direct descending drawing

The editor's **Stairs** tool draws in upper-to-lower order. The first point
is the upper endpoint on the active level, the second the lower endpoint. Both
click-click and primary-pointer drag are supported. Rust chooses the nearest
strictly lower elevation, breaking duplicate-elevation ties by ascending UUID;
level-list display order has no effect. No lower level refuses the gesture and
disables the tool. Preview is read-only Rust-derived geometry, with no history
or document changes. Commit creates the connection, straight stair and aligned
upper-floor opening atomically. Escape, tool/level/file changes, focus loss and
interruption by navigation cancel uncommitted drawing. Pan/zoom only reprojects
the preview and never creates a staircase.

Holding **Ctrl** while drawing stairs constrains the direction to the nearest
absolute 15° increment in plan coordinates, for both dragging and click-click
drawing. The endpoint is the raw pointer's projection onto the constrained ray;
the upper/start endpoint stays fixed. Pressing/releasing Ctrl updates the preview
without requiring pointer movement; release restores the raw direction without
drift. Preview and commit use the same current modifier and endpoint calculation.
This is explicit pointer-input quantization using approximate binary64 headings
and trigonometry, not the exact-real, radius-based wall angle-target inventory.
Cardinal and 45° rays use exact component directions; other increments remain
approximate. Existing Rust bounds, validation and stair derivation still apply.
Wall snapping, stair movement and furniture rotation are unchanged.

Drawing defaults are width 900 mm, guard height 1000 mm and
`ceil(rise/175)` risers clamped to [2,256], named `Stairs N`. They are draft
choices, not code-compliance guarantees. The drawn plan endpoints determine the
run; the editor does not infer a measured going from them. For example, drawing
from `(3780,0)` on a 2600 mm level to `(0,0)` selects a 0 mm lower level, produces
15 risers and 14 goings of 270 mm. The stored connection still runs lower→upper.

The **Floor** panel exposes name, width, guard height, integer riser count, and
absolute starting/ending elevations. Start means upper/drawing start, end means
lower/drawing end, regardless of which endpoint floor is active. Rust converts
absolute elevation edits to endpoint offsets from the referenced datums. A
positive rise remains required, but neither endpoint must coincide with its
floor; level edits retain those offsets. Name edits update assembly/connection
names without replacing identities. Valid inspector edits apply automatically:
typing bursts are coalesced for 120 ms; Enter, focus loss, saving, leaving the
level and history actions flush pending valid changes synchronously. One flush
is one atomic undoable batch; rejected and unchanged requests do not add history.
Invalid input remains local with inline errors; valid unrelated fields can still
apply. Undo/redo, context changes and disposal must not replay delayed edits.
Acknowledging a live edit preserves focus and the caret. Dimensions use the
editor's display units without changing the stored millimetre contract. No new persistence fields or
schema label are needed; these are existing connection endpoint offsets.
Elevation acknowledgements reconcile only binary64 datum-round-trip noise
(less than `10^-8` mm); unchanged accepted text retains Rust's precise value
without generating another edit. History and external edits still reset local
pending work.

### Selecting and moving stairs

In the 2D **Select** tool, clicking a stair footprint selects its assembly ID
and highlights it. It is eligible on either endpoint floor, never an unrelated
or merely visible passive floor. Point hits rank after wall regions and before
asset placeholders, using the existing exact rectangle distance/ring/ID order;
marquee selection uses the same footprint and existing min-corner/class/ID key.
Invalid, omitted stair drawings are not selection candidates.

Dragging translates both connection plan endpoints and aligned associated
upper-floor openings by the same millimetre displacement. Levels, elevation
offsets, width, riser count, guards and identities remain unchanged. Custom or
shared associated openings refuse the entire move rather than guessing an
attachment. Stairs can accompany a group whose members share an eligible floor.
The drag preview translates Rust-supplied drawing geometry only; it does not
change the document. Release commits one atomic undoable batch; Escape, focus
loss and pointer cancellation restore the pre-gesture selection without edits.
Zero displacement creates no history entry. Stair properties remain in **Floor**.
The 3D scene
updates after commit, not during the stair drag.

No persistence fields or format-version changes are required. Bridge ABI 25
adds the `staircase` hit class and stair targets to `translateSelection`, with
unchanged ownership, threading and error transport.

With canvas focus, **Delete** or **Backspace** removes the primary selected
stair through the same atomic command as **Remove stairs** in the Floor panel.
The Selection panel also offers **Delete stairs**. The private connection is
removed and independent floor openings are retained. Undo restores the assembly
and connection together, without reviving transient selection. Text-field Delete
and Backspace continue to edit text, not plan entities. Multi-selection follows
the existing primary-object deletion rule, not a new bulk-delete operation.

### Resizing stairs with handles

Selecting a single stair exposes four midpoint handles in the 2D **Select** tool,
on either connected floor. Side handles change width while keeping the opposite
side fixed. End handles freely change both run length and angle while keeping
the opposite endpoint fixed. Left/right mean
the sides of the stored lower-to-upper axis, regardless of the displayed UP/DOWN
symbol. Handle hits take precedence over nearby wall bodies; body dragging still
moves the stair. Handles are not offered for a multi-selection or a drawing tool.

Rust projects a side handle's raw plan-millimetre pointer displacement from the
initial grab onto the side normal. A side resize shifts both connection endpoints
by half the perpendicular displacement and updates width. An end resize changes
only the grabbed plan endpoint by the full two-dimensional displacement, rotating
the flight around the opposite endpoint. Hold **Ctrl** for nearest absolute 15°
direction constraints relative to that fixed endpoint, using projected length as
when drawing stairs. Pressing/releasing Ctrl updates a stationary preview and
releasing restores the raw direction without drift. Side handles ignore Ctrl.
Zero displacement preserves the original stair even if its angle is not snapped.
Levels, elevation offsets, riser count, name,
guard height, IDs and unknown optional fields remain unchanged. Goings and pitch
are rederived; resizing does not certify building-code compliance.

Aligned associated upper-floor openings follow in the same atomic batch.
Custom/shared cuts, crossing the opposite side edge, collapsed endpoint pairs,
zero/negative dimensions,
non-finite input and existing coordinate/dimension-bound violations refuse the
entire edit rather than clamping, flipping or overwriting unrelated geometry.
Unchanged requests add no history. Exterior stairs without a cut can resize.

Preview is read-only Rust-derived construction graphics, sharing the commit
builder. Release commits one undoable edit. Escape, capture/focus loss, tool or
floor changes and navigation interruption discard preview and restore selection.
Pan/zoom reprojects input and preview consistently. 3D updates after commit.
The handle affordance uses midpoints of the supplied rectangle's corners, ordered
lower-right, upper-right, upper-left, lower-left; Dart does not derive physical
resize consequences. Projection and offsets are deterministic approximate
binary64 operations with no extra tolerance or minimum size. Angle constraints
use approximate atan2/trigonometry, rounded half steps away from zero; cardinal
and diagonal rays use exact component directions. A free end may rotate through
any angle; lower/upper endpoint identities and elevations are not swapped.

Bridge ABI 25 adds `resizeStaircase` and a resize request to the existing read-only
stair preview call, with unchanged symbols, buffer ownership, threading and
panic/error transport. No persistence field or format-version change is needed.
End drags permit free rotation with optional
`snapAngle` (default false) in the shared preview/edit request. Older libraries
must not silently project an updated client's rotation back onto the old axis.

### Validation and consequences

All distances are millimetres and use the existing coordinate/dimension bounds.
Segment endpoints must be distinct; widths/thicknesses are positive and capped.
Offsets and derived elevations must be finite and within the coordinate bound.
Riser counts are integers in [2, 256]; names use the existing display-string cap.
Referenced hosts, levels, connections and associated openings must exist.
One connection has at most one staircase assembly. Stair width is positive;
the rise must be positive and at most the dimension cap. Rail height is zero
(no guards) or positive and capped. Derived body corners must remain within the
derived-coordinate bound. Invalid edits reject atomically, including deletions
which leave dangling references. Level elevation edits recompute the rise,
risers and guard elevations without changing the specified count or footprint;
an inverted/collapsed stair is refused. Undo/replay use ordinary entity operations.

Changing stair width updates its aligned associated upper-floor cuts in the same
batch. If an associated cut has custom geometry, the command refuses rather
than overwriting it by inference. An unchanged parameter edit creates no batch.
Entity-level unknown optional fields are preserved; nested footprint/endpoint
descriptors are closed and reject unknown fields rather than silently stripping
them. Role and host-kind tokens are likewise closed.

Original coordinate example: lower endpoint `(0, 0)` at elevation 0 and upper
endpoint `(3780, 0)` at elevation 2600, width 924, and 15 risers produce 14 goings
of 270 and a riser height of `2600/15` mm. The first tread top is at `2600/15`;
the last generated tread is at `14*2600/15`; the upper landing is at 2600.
Treads are represented as open 50 mm bodies (or one riser thick when smaller).
Optional rails are 40 mm wide/thick with 30 mm square posts at every third tread
boundary and both ends. These are bounded display assumptions, not engineering
specifications. Incline is the tread pitch `atan(riser/going)`.

The stair builder can create an explicitly requested upper-level floor cut in
the same atomic batch. A room-to-stair conversion must use the existing room
footprint, retain its deck-side endpoint, explicitly remove the placeholder room
and its outline walls, and create the staircase and cut atomically. It is never
an automatic inference that every room named "steps" is a staircase.
Room conversion remains a CLI operation, not the editor's stairs-creation UI.

Stair checks report rise, going and incline; these are geometric diagnostics,
not building-code approval. Example configurations and generated guard/tread
dimensions are draft assumptions, not measured construction specifications.

## Persistence and compatibility

The new document collections are `constructionBodies`, `hostedOpenings`,
`levelConnections` and `staircases`; entity tags are their singular camelCase
names. Each entity preserves unrecognized optional fields. New collections are
omitted when empty. A package whose baseline or history uses these kinds MUST
declare `formatVersion: "0.2"`, including after their deletion. Existing `0.1`
plans remain readable and retain `0.1` on ordinary saves. Older readers reject
`0.2` before editing rather than treating essential construction data as opaque
optional fields. This is a defined core-schema addition, not an implementation
of the namespaced-extension proposal in ADR 0011. No public compatibility or
released-schema migration promise is introduced.

## Numerical and rendering contract

Rectangle offsets and stair interpolation use approximate binary64 arithmetic.
Existing filtered/exact polygon predicates and arrangement operations perform
cut topology; no new approximate orientation predicate is introduced. Output
order follows document order, with generated treads in ascending step order.
Inputs are validated before edits; geometry functions return errors for invalid
references or parameters and impose the fixed riser budget before allocation.
Drawing an already-invalid file is best-effort: 2D/3D views and SVG/PNG exports
omit only stair assemblies that cannot be derived, report their entity IDs and
reasons, and retain the valid drawing. Strict derivation and edit admission do
not become permissive. A missing connection is reported even when its level
cannot be determined.
2D clients receive Rust-derived outlines, tread/arrow segments and direction
labels. They do not reconstruct stair geometry in Dart. 3D construction derives
from the same bodies and cuts; presenter resource ownership is unchanged.
