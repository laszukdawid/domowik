# 0058: Implement magnetism and snapping

- **Phase:** 2
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0003, 0012, 0024, 0049

## Current disposition — 2026-10-05

Keep partial: snapping is implemented; remaining construction/conformance
evidence is not complete.

`crates/domowik-geometry/src/snap.rs` and frontend native snap calls replace
the old Dart-only helper. S-series/lifecycle coverage and named-host query
timings exist. Certified intersection construction is also present in
`arrangement.rs`; its remaining review and fast/fallback measurements must be
reconciled with 0030's exact topology/closure requirements. Do not implement
that construction again or revive historical branch/ABI scheduling notes.
Earlier ticked boxes do not waive the later residuals.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Remaining acceptance work — 2026-10-08

The original checked criteria below describe delivered scope and dated
evidence. These later residuals explain the current nonterminal status.

- [ ] Reconcile certified construction and exact topology/distance guarantees
      with 0012/0030, including reviewed fast/fallback benchmark evidence.

## Original problem

BEHAVIOR-SPEC section 6 is normative and 539 lines long, and almost none of it
is built. A review on 2026-09-06 found that the only implemented part is
R6.3's winner comparator, and only because hit testing reuses its total
ordering and join-node deduplication — `domowik-geometry::hit` cites `R6.3.1`,
`R6.3.2` and `R6.3.3` and nothing else in section 6.

Unimplemented: section 6.1 candidate inventory and exclusions, 6.2 angle
targets, 6.4 guides, temporary disable and preference transitions, 6.5
pointer, capture, click-drag and touch lifecycle, 6.6 the topology boundary,
and the entire 6.7 S-series fixture set.

The Flutter editor carries a six-line ad-hoc axis snap
(`flutter_app/lib/main.dart`, `_snap`) that pulls a drag toward horizontal or
vertical. It is not the specified comparator, it is not shared with the CLI,
and per ED-0101 editing semantics belong to the core rather than to a front
end.

There is a further recorded conformance gap to close here: R6.3.1 requires an
EXACT squared-distance comparison, and the bridge documents its own
implementation as binary64 (`spikes/0024-flutter-rust-bridge/src/lib.rs`,
"Conformance caveat"). Issue 0012 owns native distance exactness; this issue
must not silently inherit an inexact comparator into the snapping path.

## Why it matters

Drawing a floor plan without magnets is not a workflow anyone completes. This
is the largest single item between the current prototype and Phase 2's exit
criterion, and no issue owned it — issue 0049 deferred "full snapping" to
separate work without filing one.

## Scope

Belongs here: the section 6 candidate inventory, angle targets, winner
selection over the full candidate set, guides, temporary disable, preference
transitions, and the pointer/capture/click-drag/touch lifecycle, implemented
in the shared core so the CLI and GUI cannot diverge; the S-series fixtures;
removal or replacement of the Flutter-side ad-hoc snap.

Explicitly excluded: marquee selection (issue 0059); the measured interaction
budgets that section 6 work will need to respect (issue 0033); native
exact-distance predicates (issue 0012), consumed here as a dependency rather
than re-solved.

## Acceptance criteria

- [x] Every S-series fixture in BEHAVIOR-SPEC section 6.7 passes against the
      core, with the exact real oracles the section specifies rather than a
      tolerance chosen to make them pass.
- [x] Candidate inventory, exclusions and angle targets are implemented in the
      shared core; the Flutter editor supplies gestures and consumes results,
      holding no snapping rules of its own (ED-0101). The ad-hoc `_snap`
      helper is removed rather than left beside the real path.
- [x] Guides, temporary disable and preference transitions behave as 6.4
      specifies across the 6.5 pointer, capture, click-drag and touch
      lifecycle, with cancel-safe previews.
- [x] The winner comparator used for snapping is exact per R6.3.1, or the
      residual inexactness is named, bounded and recorded against issue 0012
      rather than left implicit.
- [x] New predicates try a floating-point filter with a proven error bound
      before exact fallback, and land with adversarial tests and benchmarks
      per AGENTS.md.
- [x] Snap evaluation cost per pointer move is measured on the named
      benchmark host and does not perform unbounded document-wide work.
- [x] Full repository verification passes, with remaining scope stated
      honestly.

## Core checkpoint: 2026-09-07

`domowik_geometry::snap` supplies a reusable six-class query with canonical
occupied-boundary segment keys, stored-point exact distance ordering, native
guide segments, toggles and anchor filtering. Point and edge indices are built
once; the axis index is cached by the expanded view, including an outside
captured pointer. Views are not restricted to document coordinate bounds.

The 21 focused tests include S1-S5/S7 query cases, Alt-disabled evaluation,
ordering, grid refusal, outside-domain views and ray equality under underflow.
They do not prove the fixtures' command batches, undo traces or editor lifecycle.
Geometry all-target clippy passed. This checkpoint is not exposed through the
FFI or Flutter yet. Full repository verification at `987ce83` exited 0 with
`=== VERIFICATION PASSED === all stages` and no skipped stages, including
217 geometry tests, the native/Flutter suites and macOS packaging. Local log:
`/private/tmp/domowik-0058-core-final-verify.log`.

On Apple M4 Max (16 cores, 48 GiB, macOS 26.6.2, rustc 1.98.0), repeated
840-wall query median confidence intervals were 9.36-9.56 us at a corner,
3.57-3.62 us at a mid-span, and 9.68-9.81 us with angle targets. These measure
cached core queries, not first-query/index-build cost, p95 latency or Flutter
frame time. Command: `cargo bench --locked -p domowik-geometry --bench snap
-- 'snap/query' --sample-size 20 --warm-up-time 0.5 --measurement-time 1
--verbose`. Local raw output: `/private/tmp/domowik-0058-core-query-bench.log`.
The benchmark checks that each query succeeds and returns candidates before
timing; it does not benchmark an error path as a successful query.

Remaining scope is substantial: wall self-exclusion returns the explicit
`EdgeOwnershipUnavailable` error because merged boundary provenance is missing;
it does not guess ownership from coordinate coincidence. Projection, derived
boundary and rotated-vector arithmetic remain rounded with no proved error
bound under issue 0012. Exact stored-ray comparison does not fix those
constructions. Editor chaining, guide lifetime, Alt sampling, live preferences
and all end-to-end S-series effects remain unfinished. No acceptance criterion
is closed by this checkpoint.

## Native bridge checkpoint: 2026-09-08

ABI 12 adds the read-only `domowik_plan_snap` adapter and shared Dart binding.
The document index is cached by level and revision; read/save/refused-edit
retention and accepted edit/undo/redo/level-switch invalidation are tested.
Core refusal reasons remain distinguishable in the native diagnostic and leave
the output buffer untouched. Focused validation passed 76 native tests,
131 C harness checks and 97 Dart harness checks. The bundled-app self-test now
calls the snap export as well. Full verification at `0e7a3ab` exited 0 with
`=== VERIFICATION PASSED === all stages` and no skipped stages, including the
171 Flutter tests and packaged macOS self-test. Local log:
`/private/tmp/domowik-0058-abi12-final-verify.log`.
The Flutter gesture path still does not use this query; the core checkpoint's
ownership and numerical limitations remain unchanged.

## Provenance

Implementation began on 2026-09-07 after `4dd59ff` supplied exact point-distance
comparison and radius admission. The shared query, editor integration and
S-series coverage are in progress; no acceptance criterion is yet closed.

Original Domowik specification (BEHAVIOR-SPEC section 6) and a repository
review on 2026-09-06. No comparable-application source, fixtures or assets
consulted.

## Editor creation checkpoint: 2026-09-08

New-wall drags now consume native snap points and guides, with default-enabled
grid/object/angle controls scoped to new walls. Alt is latched at pointer-down.
Classified drags query on input and release; camera changes schedule a preview
reevaluation outside layout. Preferences remain local and reset on new/open.
Native query errors suppress the draft and commit instead of becoming misses.

Eight focused widget tests cover default grid and endpoint targets, preferences,
Alt transitions, release without a move, live preference changes, camera
changes, guide cancellation and the actual release cursor. Existing raw-camera
tests explicitly hold Alt; other creation tests retain enabled defaults.

This is not the full wall-tool lifecycle. Chaining and edit self-exclusions
remain unfinished, and endpoint editing still uses the legacy `_snap` helper.
The native numerical gaps above remain open. No acceptance box is closed.

Full verification at `caab412` exited 0 with
`=== VERIFICATION PASSED === all stages` and no skipped stages, including
179 Flutter tests and macOS packaging. Local log:
`/private/tmp/domowik-0058-editor-final-verify.log`.

Claude's read-only review identified an anchor-lifetime defect: toggles
requeried the already-established start. The regression reproduced the shift
from `(-3600, 1500)` to `(-3565, 1545)` before correction. Preferences now
reevaluate only the moving cursor; query failures also retain an established
anchor. Local red evidence: `/private/tmp/domowik-0058-anchor-red.log`.
The eight focused tests pass after correction. Full verification at `914fead`
also exited 0 with `=== VERIFICATION PASSED === all stages`, no skipped
stages, 179 Flutter tests and macOS packaging. Final local log:
`/private/tmp/domowik-0058-editor-anchor-final-verify.log`.

An attempted refusal-path widget test also exposed a remaining native gap:
a finite cursor at `1e25` mm does not produce `GridRange` despite exceeding
the grid's i64 index range. The attempted test's assumption was invalid;
it is not counted as refusal coverage. Saturating lattice-index conversion
needs a core regression and correction before numerical conformance is claimed.

## Grid-index refusal correction: 2026-09-08

All four floored/ceiled lattice bounds are checked before conversion to i64.
The lower bound `-2^63` is inclusive and `2^63` is exclusive; non-finite
quotients are refused. This fixes the saturation gap identified above without
adding a document-coordinate restriction. Alt and grid-disabled queries still
bypass grid enumeration. Derived lattice coordinate rounding remains unfinished.

Nine new core tests cover overflow, representable boundaries, quotient infinity,
bypasses and ordinary grid results. All 30 snapping-core tests and geometry
clippy pass. The native regression covers both signs and axes, unchanged output
buffers and unchanged plan state; disabling the guard made it fail with OK
instead of MALFORMED_REQUEST. Restoring the guard passes. Local mutation log:
`/private/tmp/domowik-0058-grid-guard-mutation.log`.

Ten snapping widget tests pass against the rebuilt native library, including
refusal on release and recovery with the established anchor preserved. Flutter
analysis passes. No issue-level acceptance criterion is closed by this fix.

On the same M4 Max host, the 840-wall cached corner query measured a median
confidence interval of 9.74-9.89 us (20 samples, 0.5 s warmup, 1 s measurement).
Criterion reported a 4.15% slope increase against its stored baseline; this is
not an isolated causal measurement of the new checks. No frame-time or p95
claim follows. Command filter: `snap/query-corner/grid-20x20`; local output:
`/private/tmp/domowik-0058-grid-query-bench.log`.

Full verification at `0a29c4a` exited 0 with
`=== VERIFICATION PASSED === all stages` and no skipped stages, including
226 geometry tests, 181 Flutter tests and macOS packaging. Local log:
`/private/tmp/domowik-0058-grid-final-verify.log`.

## Wall-chain editor checkpoint: 2026-09-09

An explicit Walls tool now creates click-click or dragged chains. The existing
hybrid interaction remains the default, labeled Select / draw. Accepted native
`addWall` gestures alone advance the anchor and previous direction, with one
batch per segment. Clicks, drags and hover share native snap requests; the last
accepted vector supplies the relative-angle family. Refused queries do not
fall back to committing raw coordinates. Point markers remain distinct from
line guides.

Finish and cancellation use the existing cleanup path. Enter, double-click,
right-click and tool switching discard previews without adding trailing walls;
history and document/level transitions clear stale chains. Alt is latched for
the whole chain, treating R3.1.1's multi-segment creation gesture as the gesture
in R6.4.2. Live preference and camera changes reevaluate idle hover too.

Ten focused chain tests pass, covering accepted continuity, independent undo,
relative-angle hover/commit after a first drag, finish transitions, rejection,
Alt lifetime, panning, snap refusal and inverse-coordinate overflow. The combined
chain/snapping/editor-lifecycle run passes 87 tests; Flutter analysis is clean.
This is not full S-series conformance: edit self-exclusion and the native
numerical gaps remain open. No issue-level acceptance criterion is closed.

Full verification at `595a627` exited 0 with
`=== VERIFICATION PASSED === all stages` and no skipped stages, including
191 Flutter tests and macOS packaging. Local log:
`/private/tmp/domowik-0058-chain-final-verify.log`.

## Boundary-source foundation checkpoint: 2026-09-11

The additive `union_with_sources` geometry API preserves caller-supplied ring
tags on canonical occupied-boundary edges, including merged collinear edges,
duplicate contributors, holes, winding changes and component ordering. The
ordinary union path is unchanged. No dependency, document format or ABI changes.

Six independent integration tests pass, including 64 mixed-polygon cases that
check ordinary-union parity and input-order/winding invariance. A focused unit
regression covers metadata alignment when canonicalization removes vertex zero.
Geometry all-target clippy passes.

The 22-wall, 100-room fixture measured median confidence intervals of
392.88-398.56 us for ordinary union and 311.46-317.82 us for tagged union on the
M4 Max host. This compares whole APIs, not isolated metadata overhead: ordinary
union also computes complement rings. Command:
`cargo bench --locked -p domowik-geometry --bench regions -- occupied-region-sources --sample-size 20 --warm-up-time 0.5 --measurement-time 1 --verbose`.
The run exited 0; local log:
`/private/tmp/domowik-0058-boundary-sources-bench.log`.

This is infrastructure only. SnapIndex does not yet consume these tags. Shared
join-patch ownership and collapsed-edge exclusion policy still need resolution;
edit self-exclusion continues to fail explicitly rather than guess ownership.
Existing numerical limitations remain. No acceptance criterion is closed.

Full verification at `aada781` exited 0 with
`=== VERIFICATION PASSED === all stages`, including macOS packaging. Local log:
`/private/tmp/domowik-0058-boundary-sources-final-verify.log`.

## Edit self-exclusion checkpoint: 2026-09-12

Occupied-boundary provenance is now kept per constant-owner SUB-SPAN rather
than per canonical edge. `union_with_sources` returns a `BoundaryRun` for each
canonical edge: the interior vertices collinear canonicalization deleted, and
one tag list per piece between them. Canonical geometry is untouched, so the
R3.2.4 rings and the R6.3.1 `(component, ring, vertex-run index)` key are the
same values the ordinary union produces. `walls::occupied_region_with_sources`
tags wall rectangles and join patches and decodes a tag back to the solid that
supplied it.

`SnapIndex` consumes that record, and `EdgeOwnershipUnavailable` is gone: wall
exclusion is answered by identity instead of refused. A contributing ring is
excluded when its geometry depends on an excluded wall. A wall rectangle
depends on its own wall; a join patch depends on ALL of its members, because
the patch is the convex hull of every member cross-section and its surviving
boundary fragments run between members rather than along any one of them. A
sub-span is retained when at least one non-excluded ring contributes it, so a
neighbour that pins a span keeps it. Where exclusion breaks a canonical edge
into disjoint allowed parts, the candidate is the closest point of the
retained parts, and equidistant parts resolve by R6.3.1's own level-3 key
rather than by ring traversal order.

`excludedWallIds` and `excludedJoinIds` are separate semantics and neither
implies the other. Patch exclusion is derived from the wall list inside the
core, because dragging a wall's far end deforms the patch at its near join
while leaving that join's node where it was, and a caller cannot be relied on
to say so. Node exclusion stays the caller's, because a node excluded for
being the gesture anchor sits on a patch that nothing is moving.

This policy was proposed here, independently reviewed by the issue-0030
numerics owner, who reproduced two executed counterexamples against the rule as
first proposed, and corrected by root afterwards on the separation of the two
exclusion sets. Both counterexamples are now regressions: a merged collinear
face excludes only the dragged wall's own span, and a join patch is excluded by
any moving member without the join being named.

No ABI or wire change. `excludedWallIds` and `excludedJoinIds` already existed
on ABI 12, the response shape is unchanged, and no field was added to the
`SnapFeature` JSON. The bridge no longer has an edge-ownership refusal reason,
because no request produces one.

Focused results: 36 snapping core tests, 9 boundary-source integration tests,
235 geometry library tests and 9 integration tests overall, and 77 native
bridge tests, all passing. Workspace `cargo test`, `cargo clippy --workspace
--all-targets -- -D warnings` and `cargo fmt --check` pass, as do the bridge
crate's own clippy and fmt. Five mutations were each shown to break exactly the
check that guards them and to pass again on restore: collapsing span ownership
into the canonical-edge tag union, dropping the join patch member dependency,
folding `excludedJoinIds` back into patch exclusion, resolving equidistant
retained spans by iteration order, and discarding the sub-span break during
collinear collapse. Local log:
`/private/tmp/claude-501/-private-tmp-domowik-finalize-NbT7jY-snap0058/5db58164-ac8b-4beb-8a22-c74557eaed4f/scratchpad/mutation.log`.

Full repository verification has NOT been run for this checkpoint; it needs the
shared verification slot and is requested from root. No benchmark was re-run,
so no performance claim is made here.

No acceptance criterion is closed. The Flutter editor still commits endpoint
edits through the legacy `_snap` helper and does not yet send exclusions; the
6.5 lifecycle fixtures S8-S12, G1 and NF1 are unfinished; and the derived
projection, rotation and lattice arithmetic remain rounded under issue 0012.

## Editor edit-drag checkpoint: 2026-09-12

The ad-hoc `_snap` helper is deleted. Endpoint and body drags now go through
the same native query the creation path uses, so the editor holds no snapping
rule of its own (ED-0101). Each query names the wall the gesture is moving and
the joins whose node travels with it, and the core decides the rest. Angle
targets are switched off for edit drags, because R6.2.1 puts both families in
wall-chain creation only; the anchor is still supplied for an endpoint drag, so
R6.1.3 removes every candidate sitting on the fixed end and with it every
acceptance that would produce a zero-length wall.

The grab point is snapped as well as the cursor. R6.0.4 makes snapping a pure
function of the pointer, and measuring a body translation from a raw grab to a
snapped release would invent a displacement the user never made and would turn
a drag that returned to where it started into an edit. A refused query
suppresses the commit instead of falling back to raw coordinates, matching the
creation path.

Two existing lifecycle tests changed behaviour, and both changes are the
specification rather than a regression. A classified drag returning to its own
start is still not an edit, now because both ends snap identically rather than
because neither snapped. The two overlapping-wall refusals now hold Alt: with
snapping on, R6.3.1's class-major order sends a pointer inside a wall to that
wall's FACE rather than its centre line, so an exactly coincident axis is no
longer reachable by dragging, and R6.4.2's disable is the way to commit raw
coordinates. Those tests are about refusal handling and keep their full
assertions.

Three focused tests were added: an endpoint drag landing exactly on another
wall's endpoint, a dragged wall not being offered its own axis extension, and
Alt held from before an endpoint drag committing raw coordinates. Four
mutations were each shown to break only the check guarding them and to pass
again on restore: dropping the wall exclusion, bypassing the native query for
edit drags, dropping the Alt latch, and measuring the grab point raw while the
release is snapped. Local log:
`/private/tmp/claude-501/-private-tmp-domowik-finalize-NbT7jY-snap0058/5db58164-ac8b-4beb-8a22-c74557eaed4f/scratchpad/mutation-dart.log`.

`flutter analyze` is clean and all 194 Flutter tests pass against the release
bridge library. No ABI or wire change: the request already carried
`excludedWallIds` and `excludedJoinIds` on ABI 12.

This does not close the second acceptance box. The candidate inventory,
exclusions and angle targets are in the core and the `_snap` helper is gone,
but the 6.5 lifecycle fixtures S8-S12, G1 and NF1 are still unproven, so the
third box stays open and the first does too. Full repository verification has
not been run for this checkpoint.

## S-series fixture checkpoint: 2026-09-12

`crates/domowik-geometry/tests/snap_fixtures.rs` executes the section 6.7
fixtures against the core query. The R6.7 conversion chain is implemented once
and every input is driven through it from the RAW DEVICE PIXELS the fixture
states, so the plan millimetres quoted beside them are derived and then checked
against the quote rather than typed in as a second literal that would agree
with a transcription error (ED-0089). Squared screen distances are derived the
same way, from the squared plan distance scaled by z squared.

Twelve fixture tests pass: S1 both inputs, S2 both the tuple tie and the
identity-level contrast, S3, S4 part (a) with both exclusion probes and part
(b), S5, S6, S7 at both radii, and G1 frames 1 to 3. Every derived plan
coordinate matched the specification's quoted value exactly, and so did every
squared-pixel figure in the comparator traces, including S4's 425/4 at the
cap corners and S1's inclusive 144.

Six mutations were each shown to break only the fixture that guards them and to
pass again on restore: making the radius boundary exclusive, dropping class
precedence so distance wins outright, ranking anchor-coincident candidates
instead of removing them, skipping join-member deduplication, disarming Alt, and
collapsing the edge feature identity so a persisting guide cannot be told from a
changed one. The first attempt at the radius mutation was VACUOUS — it changed
the interval filter, which cannot separate an exact tie, and the exact path
still admitted the candidate. Retargeting it at the exact comparison made it
bite, which also located where inclusivity is actually decided. Local log:
`/private/tmp/claude-501/-private-tmp-domowik-finalize-NbT7jY-snap0058/5db58164-ac8b-4beb-8a22-c74557eaed4f/scratchpad/mutation-fixtures.log`.

What these do NOT cover, and what keeps the first acceptance box open: the
command batches and undo traces S1, S4 and S9 specify, and the lifecycle
fixtures S8 to S12, NF1 and G1 frame 4, which are editor behaviour rather than
core queries. Existing widget tests cover that lifecycle generically — stale and
non-finite releases, second touch before pinch, capture loss, tool switch, the
five-pixel threshold — but not under the fixtures' own names and vectors.

## Lifecycle fixtures and exclusion benchmark: 2026-09-12

Two named section 6.7 fixtures whose substance is event handling are now
executed under their own names. S11 holds a five-logical-pixel displacement for
three seconds and gets click semantics, then classifies a drag on the same
fixture's (6,8) move; the displacement comes from the fixture's raw device
pixels at DPR 2. S1's effects are covered with the wall tool armed: a press
near an existing endpoint commits the endpoint itself, and ONE undo takes both
the wall and the join it induced, which is what "one batch, one undo entry"
means observationally.

The rest of the 6.5 lifecycle was already covered and is not re-listed under
fixture names it does not carry: stale and non-finite releases (S8, NF1),
release outside the canvas (S9), second touch before pinch (S10), capture loss
and tool switch (S12), and Escape clearing guides (G1 frame 4). What remains
genuinely uncovered is S4's and S9's exact command batches and undo traces, and
S10's pinch derivation to `{z = 0.125, offsets (450, 384)}`.

`benches/snap.rs` gains a `query-with-exclusion` case at the same three scales,
so the sub-span ownership path is measured rather than assumed to be free. It
compiles; it has NOT been timed, because timing runs need the shared
verification slot and that has been requested from root. No performance claim
is made for the new path, and the sixth acceptance box stays open until the
measurement exists.

`flutter analyze` is clean and all 196 Flutter tests pass.

## Acceptance review: 2026-09-12

Two boxes are ticked and the rest are not. What supports each is recorded here
so the ticks can be argued with.

**Candidate inventory, exclusions and angle targets (ticked).** All six R6.1.1
classes, R6.1.3's exclusions including sub-span ownership, R6.2's two angle
families and R6.3's comparator are in `domowik_geometry::snap`. The Flutter
editor now sends gestures and consumes winners for creation, chaining, endpoint
edits and body drags; `_snap` is deleted rather than parked beside the real
path, and the only snapping decision left in Dart is which wall and which nodes
the gesture moves, which is gesture knowledge rather than a snapping rule.

**Guides, temporary disable and preference transitions (ticked).** R6.4.1 guide
lifetime is proven by G1 frames 1 to 3 in the core and by the editor clearing
guides on cancel, Escape, tool switch and finish. Every row of R6.4.2's
transition table is now tested on both a creation gesture and an editing one,
including Alt pressed mid-drag having no effect on the gesture in flight.
R6.4.3's defaults, session-only lifetime, absence from undo and immediate
mid-gesture effect are tested. The 6.5 lifecycle is covered: the five-pixel
threshold with duration irrelevance, stale and non-finite releases, release
outside the canvas, capture loss, tool switch, second touch before pinch, and
cancel-safe previews throughout.

**Not ticked, and why.**

- S-series completeness. Every core query fixture passes with derived oracles,
  and S9's rigid co-move and one-undo effect, S10's pinch derivation and S1's
  batch are covered. S4's exact command batch is not asserted verbatim.
- Comparator exactness. Ordering over STORED candidate points is exact, and
  under a uniform positive zoom the plan and screen orders agree, so R6.3.1's
  level 2 is met for those. Derived points — axis projections, rotated angle
  rays, lattice products — remain rounded. They are named here but NOT bounded,
  and the criterion asks for a bound. That bound is issue 0012/0030 territory
  and has been routed through root rather than invented here.
- Predicate filter-plus-exact with adversarial tests. No new predicate was
  added; the work reuses `compare_squared` and `within_radius`, which already
  filter before escalating. The independent adversarial round required by
  INSTRUCTIONS.md could not run: Codex `gpt-5.6-sol` reports the workspace is
  out of credits, which needs an owner refill. Self-attack was done instead, in
  the form of fifteen mutations across the core, the fixtures and the editor,
  each shown to break only the check that guards it.
- Snap cost per pointer move, and full repository verification. Both need the
  shared verification slot. It has been requested from root twice; root is
  itself stalled on the same credit exhaustion. No timing or verification
  result is claimed, and none will be without a banner.

## Sub-span key and command-batch fixtures: 2026-09-12

The ownership policy this issue implements is recorded by the issue-0030
numerics owner as **ED-0103 Part 2**, which also assigns implementation
ownership of `boolean.rs`, `walls.rs` and `SnapIndex` here. The earlier
checkpoints above describe the same rule; ED-0103 is the record, and no second
decision was written for it.

ED-0103 states that the edge feature key gains a sub-span index. It does:
`SnapFeature::Edge` now carries `span` between the R3.2.4 vertex-run index and
the start/closest/end discriminator. This matters for IDENTITY rather than
ordering. Canonicalization can merge several walls' faces into one vertex run,
and two frames whose winner sits on different owners of that run are not the
same feature even though the run is; a caller tracking R6.4.1 guide lifetime
could not otherwise tell them apart. A regression covers exactly that: the two
halves of a shared bottom face report the same `(component, ring, segment)` and
different `span`.

**The ordering half of that change is currently unreachable, and is kept
anyway.** `span` sits in the comparator so the level-3 key is total by
construction, but R6.3.1 compares the closest point first and the query emits
one candidate per canonical edge, so no input reaches the comparison. Removing
it from the comparator broke nothing — that mutation was VACUOUS and is
reported as such rather than counted. The identity use does bite: forcing every
edge to report sub-span zero fails the regression.

Three command-batch fixtures now run in `domowik-edit`, which is where a
gesture's MEANING is decided. S4 part (a) commits one wall and one join with no
split, because the contact is endpoint to endpoint. S4 part (b) still commits
one wall and one join although the rectangles overlap with positive area, since
R3.1.7 makes that a warning attached to the result rather than a rejection. S6
commits a single wall with no join on an empty scene. Moving part (a)'s
continuation off seg1's endpoint drops the join, so the assertion measures the
join rather than a constant.

No wire change. The core carries `span`; the bridge deliberately does NOT
serialize it, because adding a field to the snap response is an externally
observable change and ABI 13 is allocated elsewhere. Nothing reads the feature
key across the boundary today, so withholding it costs nothing. The addition is
ready and reported to root for ABI coordination.

Focused results: 37 snapping core tests, 12 S-series fixtures, 9
boundary-source tests, 3 command-batch fixtures, 77 native bridge tests, 197
Flutter tests. Workspace `cargo test`, `clippy --workspace --all-targets -D
warnings` and `fmt --check` pass.

## S-series coverage map: 2026-09-12

Where each section 6.7 fixture is executed, so the first acceptance criterion
can be checked rather than taken on trust. "Derived oracle" means the fixture's
raw device pixels are converted through R6.7's chain in the test and the quoted
plan and squared-pixel values are checked against that derivation.

| Fixture | Query winner | Effects / lifecycle |
|---|---|---|
| S1 | `snap_fixtures.rs`, both inputs, derived oracle | `editor_snapping_test.dart` S1 batch and single undo |
| S2 | `snap_fixtures.rs`, tuple tie and identity-level contrast | n/a |
| S3 | `snap_fixtures.rs`, one node candidate, no member endpoints | `editor_lifecycle_test.dart` joined-corner rigid co-move and undo |
| S4 | `snap_fixtures.rs`, part (a) with both probes and part (b) | `snap_effects.rs` S4(a) and S4(b) batches |
| S5 | `snap_fixtures.rs`, one deduplicated 45-degree target | n/a |
| S6 | `snap_fixtures.rs`, every class suppressed | `snap_effects.rs` S6 batch |
| S7 | `snap_fixtures.rs`, miss at mouse radius, hit at touch | n/a |
| S8 | n/a | `editor_lifecycle_test.dart` stale release ignored, later matching release commits |
| S9 | n/a | `editor_lifecycle_test.dart` release outside the canvas commits at its own cursor |
| S10 | n/a | `editor_lifecycle_test.dart` second touch cancels first; `plan_viewport_test.dart` pinch derivation to z = 0.125, offsets (450, 384) |
| S11 | n/a | `editor_snapping_test.dart` S11, five pixels held three seconds is a click, (6,8) classifies |
| S12 | n/a | `editor_lifecycle_test.dart` capture loss and tool switch cancel like Escape |
| G1 | `snap_fixtures.rs` frames 1 to 3, including the persisting feature | `editor_chain_test.dart` frame 4, Escape clears guides |
| NF1 | n/a | `editor_lifecycle_test.dart` non-finite event discarded whole |

The query winners and G1's frames use derived oracles. The lifecycle rows are
behavioural: they assert the commit, cancel or no-op the fixture requires, not
its coordinates, because those fixtures turn on event handling rather than on
geometry. S8, S9, S10, S12 and NF1 are covered by tests that do not carry the
fixture names, which is why this map exists rather than a rename of other
people's tests in a shared file.

## Adversarial review and its outcome: 2026-09-12

Codex `gpt-5.6-sol` at high reasoning effort, banner verified in the output,
pointed at `snap.rs` and three functions of `boolean.rs` and asked to attack
four named claims. It produced findings, not a plan. It could not break
sub-span provenance through collinear removal, winding reversal and rotation,
and could not break ring exclusion. It broke the other two, and both
reproductions were executed here before anything was changed.

**Finding 1 — a real defect, with the wrong cause.** It reported that a
sub-span's closest point was reconstructed as `a + (b - a) * 1` rather than
returned as the stored `b`, and gave a two-wall configuration whose retained
endpoint then fell outside an admission radius it sat exactly on.

The reconstruction defect is real and is fixed: a clamped projection parameter
now returns the stored endpoint. Sampling segments inside the ED-0100
coordinate bound, the reconstruction differs from the stored endpoint for
roughly one in six, so this was not a corner case. A regression queries at each
boundary vertex and requires the edge ENDING there to answer with that vertex;
rebuilding it instead fails the test. The first version of that regression was
VACUOUS, because every vertex is also some edge's START and a parameter clamped
low already returned `a` unchanged — it had to name the segment to bite.

But the cause Codex gave is NOT the cause of the symptom it demonstrated, and
its suggested fix does not remove it. Dumping the ring shows the stored break
vertex is already `0.2999999999999545` before any projection runs. The input
rectangle corners are exactly `0.3`; the arrangement's welded representative is
the interpolated cut point, and `union` never exposed that because collinear
canonicalization deletes the vertex outright. Recording sub-spans is what made
the choice observable.

**Finding 2 — same underlying cause, and it stands.** Three collinear walls
with the middle excluded: the bottom face resolves an equidistant pair to one
side and the top face to the other, because the two faces store different
representatives of the same welded vertex (`1.9` against
`1.8999999999999773`). The coordinate tie-break never runs, since the stored
distances genuinely differ. This is NOT fixed here and no test asserts the
agreement, because asserting it would be asserting an exactness the
arrangement does not provide.

**What that residual is, and its bound.** A sub-span break inherits the
arrangement's welded vertex representative. Under ED-0103's N-2 the welded
candidates are ONE vertex, so this is a representative choice inside the
topology quantum rather than a topology error, and it is bounded by `WELD_MM`,
ratified at 10⁻³ mm. Within that bound it can decide admission exactly at the
radius and can decide between equidistant candidates, so two faces of one
rectangle may disagree about which retained span is nearer. Routed to root for
issues 0012/0030; it is not repaired here because the representative choice
belongs to the arrangement, which this issue does not own.

This is the first bound this issue has been able to state rather than merely
name, and it covers only the sub-span break. Projections, rotated angle rays
and lattice products remain named and unbounded, so the comparator-exactness
criterion stays open.

## Derived-construction error bounds: 2026-09-12

The comparator itself is exact: class, then squared distance over STORED
candidate coordinates through the filter-then-`Dyadic` path, then the R6.3.1
keys. Because the view transform is a uniform positive scale, ordering by plan
distance and by screen distance agree, so level 2 needs no view. What was never
bounded is the DERIVED coordinates some candidates carry. Those bounds now
exist.

`project_onto_ray` serves both the `axis` projection and the `angle` ray,
because they are one construction. For unit roundoff `u = 2^-53`, exact
direction `D`, its rounded form `d`, and returned point `q`, the perpendicular
deviation of `q` from the exact line is at most

```text
u · ( |t|·|D|·(1 + sqrt(2)(1+u)) + sqrt(2)·|q| )
```

from two sources: the direction carries at most one rounding per component,
contributing `|t|·u·|D|` because `2|Dx·Dy| <= |D|^2`; and the reconstruction
rounds twice per coordinate, contributing `u·sqrt(2)(|t|·|d| + |q|)`. An error
in `t` contributes nothing, because it slides the point ALONG the ray. The
global angle family is exact, being integer pairs, so it only improves the
bound.

At ED-0100's `derivedCoordBoundMm` of 1.1e7 mm this is under **8e-9 mm**, about
five orders of magnitude below the ratified topology quantum `WELD_MM` of
1e-3 mm. The construction therefore cannot move a point across a topology-grid
cell, which is what ED-0103's N-2 identity relation depends on. The two
remaining derived classes are bounded the same way and by inspection: a `grid`
lattice point is at most two roundings, so at most `2u·|i·s|`; a sub-span break
inherits the arrangement's welded representative, bounded by `WELD_MM` as
recorded above.

The bound is verified EXACTLY rather than in the arithmetic it bounds. Over
20,000 deterministic in-domain configurations — axis-aligned, diagonal, and
short relative to their offset from the origin — the perpendicular deviation is
compared as `cross^2 <= bound^2 · |D|^2` in `Dyadic`, which removes the division
and the square root so nothing in the check rounds. It is also TIGHT: halving
the bound makes the check fail, so the derivation is within a factor of two of
the worst case the sample reaches. Local log:
`/private/tmp/claude-501/-private-tmp-domowik-finalize-NbT7jY-snap0058/5db58164-ac8b-4beb-8a22-c74557eaed4f/scratchpad/bound-tightness.log`.

**Three more boxes are ticked.** The S-series box, supported by the coverage map
above: the core query fixtures pass with oracles derived from the fixtures' own
raw device pixels and no tolerance anywhere, and the five lifecycle fixtures are
covered behaviourally under other test names, which the map states rather than
hides. The comparator box, supported by this section. The predicate box: no new
predicate was added, the work reuses `compare_squared` and `within_radius`,
which already filter before escalating to exact; the adversarial round required
by INSTRUCTIONS.md ran against Codex `gpt-5.6-sol` and found a real defect that
is fixed; and the exclusion benchmark has landed, though it is not yet timed.

Only the two slot-bound boxes remain: the measured per-move cost, and full
repository verification.

## Constructed-vertex defect fixed at its source: 2026-09-12

The issue-0030 numerics owner handed over a minimal witness for a defect in
`boolean.rs`, which this issue owns exclusively: `union` could emit a vertex one
ulp from an input vertex the ring passes through. Reproduced here before
changing anything. Two merely TOUCHING rectangles,
`(0, 0, 2864.5859375, 13658.0546875)` and
`(2864.5859375, 741.6640625, 3364.5859375, 3591.8203125)`, produced a ring of
eight vertices of which seven were exact input corners and one carried
`y = 3591.8203124999995` where the input corner is `3591.8203125`.

The cause is `split_all`: a cut parameter that lands on another ring's CORNER
has an exact answer — that corner — but `lerp` rebuilds it as `a + (b - a)·t`
and can miss by an ulp. Fixed by giving every welded input vertex one canonical
representative and reproducing it whenever a cut lands there. Points sharing a
`VertexKey` are ONE vertex under ED-0103's N-2, so choosing between them has no
geometric consequence, and the input coordinate is the only choice that returns
the caller's own numbers. Ties between several go to the least `(x, y)`, so the
representative never depends on iteration order (R6.3.3).

**This closes the adversarial finding that was recorded above as unfixed.**
Codex's second finding — the bottom and top faces of one rectangle resolving an
equidistant pair to opposite sides — was diagnosed here as a constructed vertex
rather than a comparator fault, and deliberately left with no test, because
asserting agreement would have asserted an exactness the arrangement did not
provide. It provides it now. Both faces store `1.9`, the pair is a genuine tie,
and R6.3.1's key decides it the same way in either direction. The admission case
is covered too: a retained span's endpoint sitting exactly at the radius is
admitted, which needs the stored break to BE the input corner.

Removing the reproduction fails all three regressions and restoring it passes
them. Local log:
`/private/tmp/claude-501/-private-tmp-domowik-finalize-NbT7jY-snap0058/5db58164-ac8b-4beb-8a22-c74557eaed4f/scratchpad/mutation-vertex.log`.

**Correction to the WELD_MM residual recorded above.** That entry said a
sub-span break inherits the arrangement's welded representative and is bounded
by `WELD_MM`. That now applies ONLY to a cut at a genuine crossing, which
coincides with no input vertex. A cut landing on an input corner is exact, and
those are the cases the earlier reproductions used.

Numerics reports this defect is also what left a residual around 1e-9 mm² in
areas that must cancel exactly, and what forced `positive_area`'s `Uncertain`
band and its recorded over-inclusion against R5.1.9. Those measurements are on
the numerics branch and are not re-run here; they are owed a re-measurement
against this fix once the branches meet.

## Full verification: 2026-09-12

`./scripts/verify.sh` at `4fae1ef` exited **0** with
`=== VERIFICATION PASSED === all stages` and NO skipped stages. Exit status was
captured to a file rather than read through a pipe. 36 test suites reported ok,
including 240 geometry library tests, 77 native bridge tests and 197 Flutter
tests, and the run reached macOS packaging and the Flutter benchmark harnesses
at the end. Local log:
`/private/tmp/claude-501/-private-tmp-domowik-finalize-NbT7jY-snap0058/5db58164-ac8b-4beb-8a22-c74557eaed4f/scratchpad/verify3.log`.

**It took three runs, and the first two failed on a defect of mine.** The
`spike 0024 Dart FFI harness` stage asserted that a snap request carrying
`excludedWallIds` must THROW — the contract before wall exclusion existed, when
the core refused with `EdgeOwnershipUnavailable`. Exclusion works now, so
nothing throws. The Rust bridge tests were updated for that change and the Dart
harness, which keeps its own copy of the expectation, was not: it is a third
surface, and the checkpoints above record that it had not been exercised.

The stage runs `dart run ... 2>/dev/null | grep -q`, so it discards stderr and
pipes to `grep`, and neither failing run recorded any cause. Running the stage's
own command by hand with stderr visible produced it immediately:
`FAIL: unsupported wall exclusion throws`, 97 checks, 1 failure. The check now
asserts the current contract instead — an id naming no wall on this level
excludes nothing and is not an error, so the response must equal the unexcluded
query exactly, which also catches the opposite regression of exclusion wrongly
dropping local grid candidates.

The first two runs shared the host with four other agents. That was ruled out
as the cause rather than assumed: the failure reproduced identically on a
verified-exclusive host before the command was run by hand.

That closes the verification criterion. **One box remains open**: the snap
evaluation cost per pointer move. `verify.sh` compiles benchmarks but
deliberately does not time them, because timings are host-dependent and are not
a pass/fail gate, so no measurement follows from this run and none is recorded
here.

## Measured snap cost: 2026-09-12

Recorded in [docs/evidence/0058-snapping/MEASURED.md](../docs/evidence/0058-snapping/MEASURED.md),
following the format of the 0047 evidence document.

A 21-fold larger document costs between 1.56 and 1.95 times as much per query.
Work proportional to document size would have cost about 21 times as much, so
snap evaluation does no document-wide work per pointer move, which is the
second half of the criterion. On a 20x20 grid of 840 walls a cached corner
query measures 9.7869-9.9089 us, a mid-span query 3.6894-3.7528 us, and a
corner query with angle targets 10.191-10.279 us.

The sub-span ownership path added for R6.1.3 self-exclusion is free within
measurement noise: 9.7941-9.9941 us against 9.7869-9.9089 us for the same query
without exclusions, intervals overlapping across almost their whole width.

**Host load is recorded with the numbers**, which the 0047 document establishes
as the convention and which two evidence documents shipped earlier today
omitted. The host was NOT idle: 1-minute load average 1.54 rising to 2.37,
alongside a Kandji endpoint-security extension at 18.9% CPU, a
Virtualization.framework VM at 8.6%, Docker Desktop at 8.5%, and a leftover
headless Android emulator from issue 0024 at 4.1%. No other Domowik agent was
building or benchmarking. The absolute figures are therefore an upper bound
rather than a best case. The scaling conclusion is unaffected, because a ratio
between three cases measured under the same conditions does not depend on the
absolute level.

This closes the sixth criterion, and with it every acceptance box. The issue is
NOT Done: two owner decisions remain open, listed in the closure section below.

## ABI 13 integration: 2026-09-12

Merged `main` at `06ce1f9`, which carries ABI 13 and issue 0024's Android and
Linux work. No conflicts. Nothing belonging to another issue was altered: the
diff from `main` to this branch is the same fifteen snapping-owned files as
before, and none of them is a rooms, Android or Linux file. Room naming, the
Android probe and the Linux helpers arrived intact and untouched.

Focused results on the merged tree: workspace `cargo test` green, 79 native
bridge tests, 218 Flutter tests, `flutter analyze` clean, bridge clippy at
`-D warnings` clean, and the Dart FFI harness at 97 checks with 0 failures.

**The `span` wire addition stays off the wire.** `SnapFeature::Edge` carries the
constant-owner sub-span index in the core, and `snap_feature_json` continues to
drop it. Root has sequenced it into a single coordinated ABI 14 alongside exact
level display and asset deletion, so it waits for that rather than taking a
bump of its own. Nothing reads the feature key across the boundary, so holding
it costs nothing.

Issue 0058 is NOT on `main`. A fast-forward of `main` to this branch was
attempted under the owner's integration authorisation and was refused by the
permission layer; the fast-forward remains available and unblocked otherwise.

## Near-parallel construction bound: 2026-09-12

Numerics asked whether `WELD_MM` is the real position bound for a cut at a
genuine crossing, or a placeholder over ulp-scale rounding. It is neither, and
the `WELD_MM` figure recorded in the earlier checkpoint above is **withdrawn**.
That was the second wrong number this issue gave for the same quantity; it is
corrected here against a measurement rather than replaced with another
assertion.

The cut is `lerp(a, b, t)` with `t = num/denom` and
`denom = r1·s2 - r2·s1 = |r|·|s|·sin(theta)`. Two products and a subtraction on
each side carry relative error about `3u`, so the relative error in `t` is about
`6u/|sin(theta)|` and the displacement is bounded by

```text
|r| · 6u / |sin(theta)|
```

**Conditioned on the crossing angle, and unbounded as that angle goes to zero.**
At a healthy angle it is ulp-scale: `|r| = 8.4e6` at `sin(theta) = 1` gives
5.6e-9 mm.

Witness, with the true intersection exactly representable so the error is
unambiguous. `a = (-3145728, 2097152)`, `b = (5242880, 2097152)`,
`c = (-3145728, 2097151.95805696)`, `d = (5242880, 2097152.04194304)`. The true
crossing is exactly `(1048576, 2097152)` and the exact `t` is `0.5`. Computed
`t = 0.4999999986122212`; computed cut x = `1048575.9883584678`
(`0x412ffffffa0a1f00`). Displacement **0.011641532182693481 mm, which is 11.64
times WELD_MM**, at `sin(theta)` about 1e-8. **WITHDRAWN — this witness is
invalid; see the correction below. Its displacement was input representation,
not construction error.** The derived bound gives 5.59e-1 mm
and held on this case and on 160,000 randomized samples across four angle
decades, so it is a true upper bound, conservative by about 50x here.

Consequence for issue 0030's `Uncertain` band: at that witness the induced area
error for eight vertices at the coordinate bound is about 1.4e6 mm² against a
`64·n·u·M²` band of about 6.9 mm². Five orders, not the four numerics
estimated.

Their sign-safety argument is weaker than it looks, and that is recorded here
so nobody leans on it: the regime where construction error is largest is
near-parallel, which is exactly the regime where the true overlap is a thin
sliver. The two effects are correlated, so the comfort is thinnest where the
error is biggest.

Two things deliberately NOT done. No refusal threshold on a near-parallel
`denom`, because a threshold is an epsilon and epsilon concealment is
forbidden; that is a policy question for root. No band sizing, because the band
is issue 0030's. Offered to numerics instead: the arrangement can expose
per-constructed-vertex conditioning, `|r|` and `|sin(theta)|` or the bound
itself, so the band is sized from the geometry rather than from a constant.

This does not affect the vertex-preservation fix or any acceptance criterion.
Cuts that land on an input vertex remain exact, which is what that fix
established and what numerics re-measured at 3000 pairs with zero failures.

## Plan: certified intersection construction — 2026-09-12

Root's route, stated for review before any code moves: filter, then escalate to
exact rational evaluation from the stored dyadic coordinates, with correctly
rounded output and exact endpoint reuse. This is a construction algorithm under
N-3, not a product threshold, and no angle epsilon is introduced. Finite valid
crossings are never rejected.

### Why exact evaluation is available at all

Every input coordinate is a binary64, hence an exact dyadic rational. With
`r = b - a` and `s = d - c` taken as EXACT rational differences rather than the
rounded f64 subtractions used today:

```text
N  = (c-a) x s          D  = r x s
Px = (a.x·D + r.x·N) / D        Py = (a.y·D + r.y·N) / D
```

Both numerators and `D` are exact dyadics, being products and sums of dyadics.
So the exact intersection is a ratio of two exact dyadics, and the correctly
rounded result is reachable **without ever dividing exactly**: for a candidate
double `q`, comparing `Nx` against `q·D` is one exact multiply and one exact
compare. `Dyadic` already has `mul`, `sub`, `cmp` and `abs`, so no new numeric
primitive is needed. Magnitudes reach about 1e21 at the coordinate bound, which
its limb representation carries.

### API, bounded

- `intersection_param` keeps its signature and its exact-orientation admission
  test, and the `t`/`u` in-range acceptance moves to exact sign comparison so a
  crossing is never admitted or dropped on a rounded parameter.
- New `intersection_point(a, b, c, d) -> Option<Point>` returns the correctly
  rounded exact intersection. This is the real fix: rounding a PARAMETER and
  then interpolating reintroduces the error the parameter had.
- `cut_params` carries each cut's point beside its sort key, so `split_all`
  consumes a certified point instead of calling `lerp`. Parameters still sort
  and dedupe positions along the segment; they stop being the source of
  coordinates.
- Endpoint reuse is unchanged: `t = 0` and `t = 1` carry `a` and `b`, collinear
  overlap bounds carry input vertices, and the welded-representative
  reproduction from `522ba12` stays.

Blast radius is `arrangement.rs` and `boolean.rs`, both owned here.

### Proof obligations

1. The fast path never accepts a wrong result. Forcing escalation off must
   reproduce the exact answer or be caught, checked on adversarial inputs.
2. Escalation returns the correctly rounded exact intersection, checked against
   an independent exact oracle over randomized cases rather than against
   itself.
3. Invariance: the same point under argument order `(ab, cd)` versus
   `(cd, ab)`, under reversal of either segment, and under translation and
   scaling by powers of two, which are exact operations. This is the property
   the topology contract needs, and exact evaluation makes it stronger than it
   is today rather than weaker: the exact intersection does not depend on which
   segment it is approached from, so both approaches round to the same double.
4. The ill-conditioned witness above returns exactly `(1048576, 2097152)`.
5. The three `522ba12` vertex-preservation regressions still pass.
6. Fast versus fallback path cost, measured under a later exclusive slot.

### What would make this fail

If correct rounding per coordinate cannot preserve the topology contract, the
concrete remaining case gets reported rather than papered over. No conformance
claim will rest on `WELD_MM` alone.

## Correction: the near-parallel witness was invalid — 2026-09-12

The witness in the checkpoint above is **withdrawn**. Its 0.0116 mm was not
construction error. It was the gap between the geometry I intended and the
geometry actually stored: `P.y - L·e` and `P.y + L·e` round differently, so the
two segments were not symmetric about the point I was measuring against. The
exact crossing of the STORED coordinates is `1048575.9883584678`, and the
ordinary floating-point path returned exactly that. **Construction error in
that case was zero.** I found this while building the fix, by checking the
oracle's answer against the value I had called wrong.

The lesson is the reference, not the arithmetic: construction error must be
measured against the exact crossing OF THE STORED COORDINATES, never against an
intended ideal. Measuring against an ideal conflates input representation with
construction.

**Re-measured correctly**, against exact rational crossings of the stored
coordinates, over 60,000 randomized segment pairs with the crossing angle
controlled:

| sin(theta) | worst construction error |
|---:|---:|
| 1e-1 | 1.05e-9 mm |
| 1e-3 | 1.41e-7 mm |
| 1e-5 | 1.18e-5 mm |
| 1e-7 | 1.10e-3 mm |
| 1e-9 | 1.14e-1 mm |

So the conclusion survives and the derived `|r|·6u/|sin(theta)|` shape is
confirmed: error grows as the reciprocal of the crossing angle, reaches the
topology quantum near `sin(theta) = 1e-7`, and passes a hundred times it by
1e-9. Only the specific number and witness were wrong. A valid witness is now
a regression: `a`, `b`, `c`, `d` at bit patterns recorded in
`tests/intersection_oracle.rs`, where the ordinary path misses the exact
crossing by 0.136 mm, 136 times the quantum.

That is the third correction this issue has made to the same quantity. The
first two were wrong because they were asserted; this one is measured against
an independent oracle, and the earlier two were caught the same way rather than
by being re-derived.

## Exact crossing construction — 2026-09-12

`arrangement::intersection_point` returns the correctly rounded exact crossing.
Every coordinate is a binary64 and therefore an exact dyadic, so with `r` and
`s` taken as exact differences the crossing is a ratio of two exact dyadics per
coordinate. Correct rounding needs no exact division: deciding whether a
candidate `q` is below `num/den` is one exact multiply and one exact compare,
`q·den < num`, once the sign of `den` is handled. The ordinary floating-point
answer seeds the search, so a case that is already right costs two comparisons,
and a badly conditioned one costs a gallop and a bisection over at most 64 ULP
steps in a monotone integer view of the bit pattern.

Membership in both segments is decided on exact signs rather than on a rounded
parameter, so no finite valid crossing is rejected and no angle threshold
exists anywhere in the path.

Verified against an independent oracle: 44 cases computed by Python's
`fractions.Fraction` at arbitrary precision and correctly rounded, matched
**bit for bit** in both coordinates, including a near-parallel family at
sin(theta) from 1e-1 down to 1e-9.

The construction step and ED-0103's N-2 canonical representative remain
**separate steps**. This function rounds; the caller then replaces the result
by the representative of its weld class. Neither is folded into the other.

## Exact construction wired into the arrangement — 2026-09-12

`cut_params` is replaced by `cuts_along`, which returns the cut POINTS in order
rather than parameters to be interpolated back into points.

**Root's ordering obligation is dissolved rather than satisfied.** The concern
was that rounded `t`/`u` sort keys could misorder or collapse distinct cuts
when the keys tie, and that the carried point and the parameter order could
disagree. There is now no parameter to disagree with: ordering compares the
exact projection `(p - a)·r` in `Dyadic`, a total order along the segment
needing no division, with exact coordinate comparison as the tie-break so the
order is total even for coincident points. Two approximations of the same
quantity became one exact one.

The collapse rule moved with it. It was `|t_x - t_y| < WELD_MM/len`, a
parameter-space epsilon that a long edge diluted; it is now
`|p - q|² < WELD_MM²` evaluated exactly. Same ratified quantum, no division,
no length dependence. Both existing contracts survive unchanged: cuts half a
weld apart still collapse to one, and two cuts 1 mm apart on a
domain-spanning edge both survive.

Collinear overlap bounds are the other segment's own vertices, reused exactly
instead of reprojected, since the exact orientation predicate has already
established that all four points are collinear.

Construction and ED-0103's N-2 representative stay **two steps** in
`split_all`: `cuts_along` supplies correctly rounded or exactly reused points,
then `snap` replaces each by the canonical representative of its weld class.
Neither is folded into the other.

Regressions, all passing: the independent rational oracle at 44 cases matched
bit for bit; invariance under argument order and under reversal of either
segment, which is the property the topology contract needs; exact scaling by
powers of two, chosen because any other factor would round the INPUTS and the
test would then be measuring that instead; non-finite input yielding no
crossing rather than a non-finite point; parallel, collinear and
lines-cross-but-segments-do-not cases; and endpoint touching, which reuses the
stored endpoint exactly.

Two existing tests now assert cut POINTS where they asserted parameters, which
is the stronger statement.

Local results: 252 geometry library tests, 9 boundary-source, 12 S-series, 10
arrangement, 5 intersection oracle, 79 native bridge, 218 Flutter, Dart harness
97 checks 0 failures, workspace clippy at `-D warnings` and `fmt --check`
clean.

**Not yet done**: the fast-versus-fallback cost of the new path is unmeasured,
and root requires the crossing work reviewed and benchmarked before completion.
No timing claim is made, and the verification slot was declined and released
while this was mid-flight.

## Integration attempt held: 2026-09-12

Merged `main` at `f819ae6` into this branch with no conflicts (`0410bf2`). The
diff from `main` remains the seventeen files this issue owns.
`./scripts/verify.sh` on the merged result exited **0** with
`=== VERIFICATION PASSED === all stages` and no skipped stages. Local log:
`/private/tmp/claude-501/-private-tmp-domowik-finalize-NbT7jY-snap0058/5db58164-ac8b-4beb-8a22-c74557eaed4f/scratchpad/verify5.log`.

**The branch was NOT merged into `main`**, under the integration
instruction's own stop condition that the work is not complete. Root gated the
exact crossing construction explicitly: reviewed AND benchmarked before final
completion. Neither has happened. The construction has had no adversarial
round, and its fast-versus-fallback cost is unmeasured.

That gate is not a formality here. `intersection_point` runs exact `Dyadic`
arithmetic on every crossing in every union, and the arrangement is shared by
rooms, areas, CLI rendering and the snap index. `verify.sh` compiles benchmarks
and never times them, so a passing banner says nothing about whether this puts
a performance regression into the shared core. Landing that while four other
branches build on `main` is what the stop condition is for.

The benchmark could not be run either. The slot was described as exclusive, but
the host carried two `rustc` processes at 100% CPU each and a 1-minute load
average of 6.06 throughout the window. Root's standing rule is timings only on
an actually quiet host, so no measurement was taken and none is recorded.

A ten-minute watch, polling every fifteen seconds for a 1-minute load below 2.5
with no build process above 20% CPU, never once saw a quiet host. It ended at
load 3.79 and the machine settled at 3.34 rather than going idle. The residue is
not other agents: a Virtualization.framework VM at 112% CPU, Spotlight's `mds`
at 39%, and the Kandji MDM agent at 32%, which together are roughly 180% of
background load independent of anything this project runs. **Pausing agents
cannot make this host quiet.** A benchmark host that meets the standing rule has
to be either a different machine or one with those three services stopped, and
that is an owner decision rather than a scheduling one.

The owner's uncommitted `INSTRUCTIONS.md` in the main checkout is present and
unmodified, md5 `f9c379c7756063ad165fa41cddde275e`, unchanged from before the
attempt.

Merging is one clean fast-forward whenever the gate is cleared or waived.

## R11: the fast path is missing, and the plan to add it — 2026-09-12

Root's R11 review is accepted on both counts.

**The filter genuinely is absent.** `intersection_point` builds `Dyadic` values
for `den`, `tn`, `un`, `nx` and `ny` unconditionally, before anything has been
decided. The ordinary float computation in it is a SEED for the rounding
search, not a filter: nothing it produces can accept a result and return early.
AGENTS.md requires a floating-point filter with a proven error bound before
exact escalation, and this has none. The earlier plan said "filter, then
escalate" and the implementation only did the second half.

**The stale witness is fixed.** The `intersection_point` doc quoted the
withdrawn 0.0116 mm figure. It now carries the correctly measured table and,
more usefully, states the methodological trap that produced the bad number:
construction error must be measured against the exact crossing of the STORED
coordinates, never against an intended ideal. The historical checkpoint above
keeps its original text with a withdrawal marker rather than being rewritten.

### The filter

Enclose rather than estimate. Compute `den`, `nx` and `ny` in floating point
with running error bounds, giving intervals guaranteed to contain the exact
values. The quotient interval follows, with a zero-containing denominator
interval forcing escalation immediately.

A candidate `q` is CERTIFIED when the whole quotient interval lies strictly
between the two midpoints adjacent to `q`, because rounding is monotone and an
interval containing no midpoint maps entirely to one double. That is a proof,
not a heuristic: it does not ask whether the float answer looks close, it asks
whether any real number the exact value could be would round elsewhere.

Anything not certified escalates to the existing exact path, unchanged.

**WITHDRAWN, and it was a dangerous thing to have written:** an earlier version
of this section said a filter bug could lose speed but never correctness. That
is false. A FALSE CERTIFICATION accepts a wrong candidate and the exact path
never runs, so the error ships. Only conservative UNCERTAINTY is free. The
safety property is not structural; it rests entirely on the enclosure being
OUTWARD, which makes that a proof obligation rather than an implementation
detail. Root caught this in review R17; it is the first of this issue's
corrections that was not self-caught.

`distance.rs` already has an `Enclosure` type built for exactly this and
currently private to that module. Reusing it beats writing a second one, and
which module should own it is worth one exchange with issue 0030 rather than a
unilateral move.

### Cost, in operations rather than wall clock

No timing is claimed; the slot is not granted and this host cannot be made
quiet. Operation counts need neither.

Today every crossing pays, at minimum, ten `Dyadic` multiplications and six
add/subtracts with their allocations, plus per coordinate a bracket check of
two multiplications. A badly conditioned one adds a gallop and a bisection of
up to 64 iterations per coordinate, each a multiplication and a comparison, so
the worst case approaches 130 arbitrary-precision multiplications per crossing.

With the filter, a well-conditioned crossing costs roughly twenty floating
point operations and **zero** allocations. Escalation is then rare and paid
only where it is needed. The predicates benchmark in this crate already
measures a comparable filter-versus-exact gap at 511x, which is the order of
magnitude at stake here.

What the filter cannot do is bound how OFTEN escalation happens on real plans:
that depends on how many near-parallel crossings a document contains, and
answering it needs the interaction corpus and a quiet host.

## R17: the filter, and what it turned out it cannot do — 2026-09-12

### The claim I withdrew

R17 corrected me and the correction is right. I had written that a filter bug
could cost speed but never correctness. **False.** A false certification
returns an answer and the exact path never runs, so the error ships. Only
conservative UNCERTAINTY is free. The safety property is not structural: it
rests entirely on the enclosure being OUTWARD. That makes outwardness a proof
obligation, and it is now a test.

### What the plan got wrong, caught before implementing

The R11 plan said the filter would certify that a constructed coordinate is
correctly rounded, by checking the quotient interval contains no rounding
midpoint. **That is not achievable**, and measuring it first is the only reason
no effort went into building it.

Each outward widening costs a full ULP, and the quotient composes about ten
operations. Correct rounding needs the interval to sit inside HALF an ULP. Over
20,000 random crossings the quotient enclosure was 23 ULPs wide at the median,
13 at the tenth percentile and **6 at its narrowest** — it certified correct
rounding in **none** of them, and could not have.

**Stated precisely, because the shorter phrasing is wrong and the record
outlives the conversation.** The negative result is about ENCLOSURE-BASED
CERTIFICATION of a quotient of exact dyadics, not about floating-point division.
IEEE-754 division is correctly rounded by the standard; that is not in question
here and nothing in this issue should be read as doubting it. What cannot close
is the certification: an outward interval widened by a full ULP per operation
can never prove its content sits inside the half-ULP window that correct
rounding requires. The issue-0030 owner made this distinction after I put the
looser wording in a message, and it is recorded here rather than only there.

A correctly rounded quotient of two exact dyadics remains perfectly feasible —
exactly, not in float. `dyadic.rs` already does division with rounding in
`round_half_even_div_pow10`, by binary long division over the limbs with an
integer tie test and no float anywhere; a general `Dyadic::round_quotient`
would be that generalised to an arbitrary dyadic denominator. It is not needed
here and is not being requested, but it belongs to issue 0030 if it is ever
wanted.

### What the filter does instead

Signs, which is what an outward enclosure genuinely proves. Every early exit in
`intersection_point` is a sign question:

- a denominator interval strictly off zero certifies that the lines are not
  parallel;
- with the denominator's sign known, the `0 <= t <= 1` and `0 <= u <= 1`
  membership tests are interval comparisons, with the inequalities reversed for
  a negative denominator.

A pair that provably does not cross, or provably crosses outside a segment,
returns having allocated nothing. Anything the interval cannot decide falls
through to the exact path unchanged. In an arrangement most candidate pairs do
not cross, so this is the common case by frequency.

`Enclosure` moved from `distance.rs` into a shared `enclosure` module rather
than being copied, with numerics' agreement; their words were that a second
copy of an outward-rounding argument is how one gets fixed and the other does
not. `distance.rs` keeps its 22 tests and its guarantees, and gained
`product`, `minus`, `excludes_zero`, `sign` and `certainly_below`. The
`divided_by` method written for the abandoned approach was deleted rather than
left implying a capability that does not exist.

### Tests, as R17 required

Differential against the exact path on 40,000 random pairs and 20,000
deliberately near-parallel ones, requiring the filtered and unfiltered answers
to be identical: a disagreement IS the false certification. A denominator
interval containing zero deciding nothing. Overflow, subnormal and
smallest-positive magnitudes. A crossing sitting exactly on a rounding
midpoint. Outward containment of every enclosure operation, checked against
exact `Dyadic` arithmetic rather than against another float computation. The
44-case independent oracle still matches bit for bit through the filter.

Three mutations each break only what guards them: widening the enclosure
INWARD, admitting an interval that touches zero, and dropping the
reversed-denominator branch of the membership test. The second was VACUOUS at
first — outward widening moves an endpoint off zero, so no caller can reach
that boundary — and only bit once the primitive's contract was tested directly
rather than through `intersection_point`.

57 workspace test suites pass; clippy at `-D warnings` and `fmt --check` clean.

### Cost, still unmeasured

Operation counts only, as before: a filtered rejection is about twenty float
operations and no allocation, against ten or more arbitrary-precision
multiplications today. No wall-clock claim, no slot, and this host cannot be
made quiet.

## Closure

- **Implementation/spec commits:**
- **Follow-up issues:**

## Cross-issue fix from 0030 — 2026-10-08

`arrangement::cuts_along` now deduplicates cuts on `VertexKey` equality
instead of exact distance below `WELD_MM` (ED-0103 N-2: one identity rule).
The distance rule dropped a cut on one edge that the crossing edge kept,
and `union` lost most of a region at a 0.6 µm overlap. Details and the
regression test are recorded in issue 0030.
