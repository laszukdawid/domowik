# 0059: Supply the marquee selection gesture

- **Phase:** 2
- **Priority:** Medium
- **Status:** In Progress (partial scope)
- **Depends on:** 0012, 0049
- **Deferred-closure gates:** 0012 (remaining numerical/declared-box
  conformance), 0033 (whole-app frame/input budgets), 0026 (remaining
  asset-policy conformance). Descriptor-footprint decisions, per-instance
  resolution integration and native-backed frontend PH1 coverage are delivered.
  The full-verification criterion remains unchecked for the recorded clean-tree
  failure.

## Current disposition — 2026-10-08

The frontend placeholder integration is delivered. Native marquee and point
hits consume revision-cached per-instance resolution; degraded instances use
§9.6 bounds, and resolved models stay on the separate editor-footprint path.
The owner approved transformed width × depth declared footprints on 2026-10-08;
the fallback remains the fixed 1000 mm square regardless of scale/rotation.
The asset list paints the same placeholder bounds used for selection.

Native-backed PH1/PH2 widget tests cover marquee selection, optional deletion,
selection clearing and undo, and required-degraded inspection with deletion
refusal. Rust boundary tests cover declared dimensions, coincident corners,
tangency, active-level exclusion, resolved/unsupported/undecodable controls,
and required normal-save refusal. A replay prerequisite defect was repaired:
snapshot labels, dimensions and asset instances now receive live metadata,
so deleting a snapshot instance is no longer silently discarded.

Keep partial scope for 0012/0033 numerical and whole-app performance gates,
and 0026's wider asset-policy obligations. This increment adds no configured
external cache or remote retrieval and does not close those parent issues.
See [the integration evidence](../docs/evidence/0059-marquee/2026-10-08.md).
The older disposition and handoffs below are historical and superseded here.

## Historical disposition — 2026-10-05

Keep partial for frontend placeholder marquee integration.

AssetInstance and PlaceholderBox, resolution snapshots and core PH1 tests
exist. However `frontend/native/src/lib.rs::domowik_plan_marquee` calls
`marquee` without a placeholder snapshot. Integrate per-instance degraded
resolution and the reviewed declared-extent footprint, preserve read-only
inspection, and add native-backed PH1 selection/deletion coverage.
Numerical/frame budgets remain with 0012/0033; no new entity or hit class is
needed.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

BEHAVIOR-SPEC section 7.3 specifies marquee selection. The core query exists —
issue 0012 delivered R7.3 marquee region selection in
`domowik-geometry::hit` — but no gesture reaches it.

Issue 0049 recorded the deliberate hole it left: "Marquee gestures,
placeholder instances and full snapping are separate work. This increment must
not pretend to implement modifier-drag marquee; its interaction must remain
non-destructive until that tool is supplied." Modifier-drag over empty canvas
is therefore wired to do nothing, and the editor README tells users so. No
issue was filed for the tool itself.

## Scope

Belongs here: the modifier-drag marquee gesture, its rubber-band feedback, the
ADDITIVE membership rule of section 7.3 (R7.3/R7.2.4 define no subtractive
marquee in v0.1), primary designation after a marquee per R7.4, and interaction
with hidden, placeholder and cross-level entities per R7.5. Locked entities are
excluded from v0.1 entirely (R7.5.2: no authoritative lock field exists) and are
out of scope here, not a membership rule this issue implements.

Explicitly excluded: the core region query, which issue 0012 already
delivered and which this consumes; snapping (issue 0058); marquee frame-time
budgets, which issue 0033 lists in its query mix and owns.

## Acceptance criteria

- [x] Modifier-drag over empty canvas performs the section 7.3 marquee rather
      than the current no-op, and plain drag behaviour is unchanged.
- [x] Additive membership, primary designation after the gesture (R7.4), and
      the R7.5 rules for hidden, placeholder and cross-level entities are
      implemented and tested. (Locked entities: excluded from v0.1 entirely
      per R7.5.2, not a rule this issue implements.)
      Delivered by the original gesture tests plus the 2026-10-08 native-backed
      PH1/PH2 integration. The older missing-entity statements below are
      superseded by the current disposition.
- [x] Cancellation restores the pre-press selection, consistent with the
      gesture lifecycle issue 0043 established.
- [x] The gesture consumes the existing core query; the editor introduces no
      second definition of which entities a rectangle selects (ED-0101).
- [x] Pure state-machine tests and native-backed widget tests cover
      membership, cancellation, and interaction with cycling and modifier
      click from issue 0049.
- [ ] Full repository verification passes. The 2026-10-08 run passed all
      functional checks, including 307 Flutter tests and Linux bundle probes,
      but ended with `=== VERIFICATION FAILED ===` at `P13 clean tree`:
      existing tracker edits and this uncommitted increment remain visible.
      This is not an exit-0 verification claim.

## Provenance

Original Domowik specification (BEHAVIOR-SPEC section 7.3) and a repository
review on 2026-09-06. No comparable-application source, fixtures or assets
consulted.

## Progress: 2026-09-07

The editor now routes modifier-drag from empty canvas through the Rust
marquee query. ABI 11 adds `domowik_plan_marquee`; the shared Dart binding,
C harness and Flutter callers expect that revision. The rubber-band preview
uses local pointer coordinates. The editor queries native geometry once on
release and applies the returned order to additive selection. Selection never
becomes a document operation, and plain empty-space dragging still draws.

The selection controller covers replacement and additive membership, deduped
native ordering, an unset primary, and cycle invalidation when a marquee adds
nothing. An independent `flutter test test/selection_state_test.dart` run on
2026-09-07 passed all 26 tests (exit 0). Native-backed widget coverage also
includes cross-level exclusion, Escape, pointer cancellation, second-touch
cancellation and rubber-band lifetime. The rebuilt bridge passes both huge-band
regressions, the Flutter suite passes 171 tests, and the C/Dart harnesses pass
127/85 checks. The parallel native suite exposed a close/quiescence test failure
repaired in issue 0063. Full repository verification at commit `d0be026`
passed on 2026-09-07 with no skipped stages.
See [the checkpoint evidence](../docs/evidence/0059-marquee/PROGRESS.md)
for query measurements and verification limits.

At that date the issue remained in progress. Placeholder selection depends on
the missing placeholder entities in issues 0012/0026. R7.3.2's zero-area
wording also needs review: the core uses point-hit semantics only when both
corners coincide; a one-dimensional rectangle currently uses region
intersection. This transport
preserves that behavior and does not resolve the specification question.
Rounded polygon and intersection arithmetic remain a separate numerical gap.

## Progress: 2026-09-12

The zero-area question is answered rather than carried. R7.3.2a scopes
R7.3.2's degeneracy to COINCIDENT CORNERS: a line rectangle has zero area but
two corners, so it keeps R7.3.1 intersection semantics. Fixture L13 gives the
case where the two readings disagree, over the F2 join node, and R7.3.2's own
sentence now says "corners COINCIDE" instead of "zero-area". The core needed
no behavioural change; it needed the authority and a test that fails when the
reading changes. Flipping `marquee`'s `&&` to `||` turns both the core test
and the native-backed widget test red.

R7.4 after a marquee and the R7.5 rules v0.1 can express now have
native-backed coverage. Deleting a marquee-designated primary leaves the other
member selected with primary UNSET, undo re-creates the wall without
reselecting it, and idle Escape clears (L5). A rectangle spanning every level
yields the active level's walls whether the inactive levels are rendered at
full strength or hidden outright (R7.5.1), and activating another level clears
the result (L7). `a_marquee_never_reaches_another_level` asserts the same
exclusion in the core and answers the same rectangle against the upper level,
so the filter is shown to be a level filter.

No ABI change. `domowik_plan_marquee` was introduced in ABI 11; the bridge is
now at ABI 12 and nothing here bumps it. The editor's selection state machine
is unchanged, `main.dart` is untouched, plain drag still draws a wall, and
every cancellation path still restores the pre-gesture selection.

## What this does not deliver

Placeholder participation (R7.5.3), and the reason is missing PLUMBING rather
than a missing definition. An earlier draft of this section said v0.1 defines
no asset-instance entity. That was too broad and is withdrawn:

- FORMAT-SPEC section 2.3 reserves a typed `assetInstance` envelope in Tier B,
  with required `levelId`, `digest` and `transform`
  (`{translationMm, scaleUniform, rotationDeg}`), optional `displayName`, and
  a valid example. Placement SEMANTICS are deferred; the wire shape is not.
- BEHAVIOR-SPEC section 9.6 already fixes deterministic placeholder geometry
  (descriptor dimensions, else a 1000 mm box centred on the instance origin)
  and states that a placeholder participates in hit testing exactly as its
  entity normally would, so instances stay selectable and deletable while
  degraded.
- Fixture PH1 is executable and normative, with the box at (2000,0), the
  marquee corners plan (1400,−600)/(2600,600) selecting `{instance}`, the
  inside click selecting it through the `placeholder-box` class, and deletion
  of the primary leaving selection EMPTY and primary UNSET.

What is absent is implementation, in three places, none of them this issue's:
`domowik-format` has no `AssetInstance` and `Document` has no collection for
one (issue 0010); no asset-store query answers "is this instance degraded, and
what extent does its descriptor declare" (issue 0026); and R7.1.3's
`placeholder-box` hit class is unimplemented, which issue 0012 records as its
own gap. The prerequisite interface is stated below so those can be supplied
and this finished, rather than left as a standing exclusion.

## Prerequisite interface for R7.5.3 and PH1

The minimum that unblocks this issue. Nothing here is furniture-catalog
scope: no descriptor authoring, no placement gestures, no browser, no
resolved-asset rendering. Each item cites the rule that forces its shape.

**From issue 0010, in `domowik-format`.** The Tier B envelope of FORMAT-SPEC
section 2.3, typed and carried on the document:

```rust
pub struct InstanceTransform {
    pub translation_mm: [f64; 2], // as Wall::start_mm, not Point
    pub scale_uniform: f64,       // > 0, finite
    pub rotation_deg: f64,        // finite; no range normalisation
}

pub struct AssetInstance {
    pub id: Uuid,
    pub level_id: Uuid,
    pub digest: String, // sha256:<64 lowercase hex>
    pub transform: InstanceTransform,
    pub display_name: Option<String>, // <= 1024 B (L19)
    pub unknown: serde_json::Map<String, serde_json::Value>,
}
```

The three transform fields stay loose; nothing here wants a pre-composed
matrix. Under the fixed-box reading the hit path reads `translation_mm` only,
and a composed matrix makes applying scale and rotation the easy accident
rather than a deliberate act.

Coordinates are `[f64; 2]` because that is what every stored entity uses
(`Wall::start_mm`); `Point` is the geometry-facing type reached through an
accessor, so an `origin() -> Point` mirroring `Wall::start()` saves the hit
path a hand conversion. The struct is `InstanceTransform` rather than a bare
`Transform`: FORMAT-SPEC 2.3 says this shape is "structurally analogous to,
but NOT derived from or owned by" the section 2.5 background-anchor pattern,
and a general name would invite exactly the merge that sentence forbids.

plus `Document.asset_instances`, an `asset_instances_on_level(level_id)`
iterator matching `walls_on_level`, an `asset_instance(id)` lookup, inclusion
in `entity_count`, and an `Entity::AssetInstance` variant so `createEntity`
replay can reconstruct one. Validation is STRUCTURAL only, which is all Tier B
authorises: digest well-formed, transform finite with `scale_uniform > 0`,
`level_id` resolving, `display_name` within its cap. No placement semantics.

**From issue 0026, in the asset store.** Hit testing needs two facts per
instance and nothing else, because R7.1.3 admits DEGRADED instances only and
section 9.6 sizes the box from the descriptor:

```rust
pub trait PlaceholderSource {
    /// Section 9.6: missing blob, digest mismatch, undecodable, unsupported
    /// media type, or unavailable offline. Only these participate as
    /// `placeholder-box` candidates (R7.1.3 class 4).
    fn is_degraded(&self, digest: &str) -> bool;
    /// Descriptor-declared box extent in millimetres, or `None` for the
    /// fixed 1000 mm default of section 9.6.
    fn declared_extent_mm(&self, digest: &str) -> Option<(f64, f64)>;
}
```

A resolved snapshot (`&[(Uuid, Aabb)]`) would serve equally and keeps the
geometry crate free of an asset dependency; either shape is acceptable, and
the choice belongs to whoever implements it. What must NOT happen is the
geometry crate deciding degradation for itself.

**The section 9.6 box is `Aabb`, and it belongs in `domowik-geometry`.**
`domowik-geometry` depends on `domowik-format` and not the reverse, so a
function in the format crate cannot return an `Aabb` at all. `Aabb` is also
the right primary rather than a ring: R7.1.3's key for this class is the
EXACT min corner, which is a field of the box and a scan of a ring; marquee
intersection against an axis-aligned box is comparison-only and therefore
boundary-inclusive by construction; and the closest point to an AABB is a
clamp, which is exact for finite coordinates, so this class can avoid the
approximate projection the `region` class still carries. A renderer that
wants four corners derives them FROM the box, so the two cannot disagree.

The bound check earns its place downstream: `DIMENSION_CAP_MM` is 10^6 and the
box half-extent is 500 mm, so a translation inside `COORD_BOUND_MM` always
produces a box inside `DERIVED_COORD_BOUND_MM`. The marquee's envelope clip
therefore covers placeholder candidates with no widening.

**Then issue 0012 supplies the class** — `HitClass::PlaceholderBox`, ranked
below `Region`, total key = exact min-corner `(x, y)` then stable instance ID
— because 0012 records that gap and this issue's Scope excludes the core
query.

**This issue then finishes**, and the work is small because the transport is
already entity-agnostic: placeholder candidates enter `marquee`'s keyed list
with the class rank and their section 9.6 bounds, PH1 becomes a core test
(marquee containment selects the instance; the inside click selects it; the
box face contributes no snap candidate), the editor's selection state needs
nothing because it carries opaque IDs, and PH1's deletion step needs a
`deleteAssetInstance` gesture in `domowik-edit` plus its bridge call, which
IS an ABI change and therefore root's to sequence rather than mine to take.

### Decided: the fallback box ignores scale and rotation (ED-0106)

Root settled this on 2026-09-12. The no-dimensions fallback is a 1000 mm
square, AXIS-ALIGNED IN PLAN SPACE, centred at `translationMm`, ignoring
`scaleUniform` and `rotationDeg`. That ignoring is scoped to this fallback
alone: the descriptor-dimensions branch of section 9.6 remains OWED to asset
resolution and is not waived. ED-0106 records the refinement, and a fixture
with a non-identity transform discriminates it, because PH1 alone cannot.

**One argument this issue advanced for the decision was wrong, and is
withdrawn.** It claimed R7.1.3's EXACT min corner forbids a rotated box,
because rotation needs sine and cosine and so produces a rounded corner. Root
rejected the reasoning while accepting the conclusion, and root is right:
exact COMPARISON of stored values is a separate thing from exactness of
CONSTRUCTION. The proof is already in this repository — a wall's rectangle is
a rounded construction whose vertices are then compared exactly, which is why
the `region` class carries an approximate projection and a total ordering at
the same time. A rotated placeholder box would have been no different. The
surviving reasons are textual and substantive: "axis-aligned" and "rotated"
are contradictory unless the text means the bounding box OF a rotated box,
which it does not say and which would make the extent angle-dependent, so
"fixed 1000 mm" would be false; `scaleUniform` contradicts "fixed" the same
way; and a box scaled or rotated to match content asserts knowledge of content
that could not be decoded.

### Decided: candidacy comes from a per-instance resolution result

Root settled this on 2026-09-12, and the answer is NOT the one this issue
floated. Placeholder candidacy comes from an explicit per-instance resolution
result. A global assumption that every asset is degraded is refused, however
convenient it would be while no resolver exists. Content that is currently
missing or unsupported can yield a degraded result under R9.6, but a REQUIRED
asset may instead force read-only mode, and deletion must respect that. So
candidacy is per instance and carries a requirement class; it is not a
property of the v0.1 era.

### The PH1 core test, ready to apply

Written now so the prerequisite work has a target to satisfy rather than a
description to interpret. It does NOT compile yet: `AssetInstance` and the
degradation lookup do not exist, and nothing here invents their creation,
editing or validation semantics, which are Tier B open questions owned by
issue 0004. The fixture is PH1's own arithmetic: an optional instance with no
declared dimensions at origin (2000,0) is the fixed 1000 mm box of section
9.6, so its bounds are x in [1500,2500], y in [-500,500] — PH1's snap probe
independently fixes the right face at 2500.

```rust
/// PH1. A degraded instance with no declared dimensions is the fixed 1000 mm
/// box of section 9.6, and it participates in the marquee exactly as a wall
/// region does, including boundary contact (R7.3.1, R7.5.3).
#[test]
fn a_marquee_containing_a_placeholder_box_selects_the_instance() {
    let (doc, level, instance) = ph1();
    // PH1's own corners, and the active level has NO wall features, so the
    // instance is the only thing that can answer.
    let containing = Aabb::from_corners(P(1400.0, -600.0), P(2600.0, 600.0));
    assert_eq!(marquee(&doc, level, containing), vec![instance]);

    // Boundary-inclusive over the section 9.6 bounds, not merely over wall
    // rectangles: a rectangle whose right edge lies exactly on the box's
    // LEFT face still selects.
    let tangent = Aabb::from_corners(P(1000.0, -100.0), P(1500.0, 100.0));
    assert_eq!(marquee(&doc, level, tangent), vec![instance]);

    // And one that stops short of it does not, so the test is not passing
    // because everything matches.
    let short = Aabb::from_corners(P(1000.0, -100.0), P(1499.0, 100.0));
    assert!(marquee(&doc, level, short).is_empty());
}

/// PH1's hit step: a click inside the box selects the instance through the
/// `placeholder-box` class, which R7.1.3 ranks below `region`.
#[test]
fn a_click_inside_a_placeholder_box_selects_the_instance() {
    let (doc, level, instance) = ph1();
    let hits = hit_test(&doc, level, P(2000.0, 0.0), 0.0);
    assert_eq!(hits[0].entity, instance);
    assert_eq!(hits[0].class, HitClass::PlaceholderBox);
}

/// The marquee key spans classes (R7.3.2), so a placeholder and a wall order
/// by min corner FIRST and only then by class rank. This is what makes
/// "primary is the LAST entity of that order" total across a mixed result.
/// Its own fixture, not PH1's: PH1 deliberately has no wall features.
#[test]
fn placeholder_and_region_candidates_share_one_marquee_key_order() {
    let (mut doc, level, instance) = ph1();
    // Box min corners (0,-100) and (3000,-100), either side of the
    // placeholder's (1500,-500) in x.
    doc.walls
        .push(Wall::new(level, P(0.0, 0.0), P(500.0, 0.0), 200.0, 2500.0));
    doc.walls
        .push(Wall::new(level, P(3000.0, 0.0), P(3500.0, 0.0), 200.0, 2500.0));
    let all = Aabb::from_corners(P(-500.0, -1000.0), P(4000.0, 1000.0));
    assert_eq!(
        marquee(&doc, level, all),
        vec![doc.walls[0].id, instance, doc.walls[1].id],
        "ascending min-corner x interleaves the classes, never groups them"
    );
}
```

Two assertions belong elsewhere and are NOT claimed here. PH1's snap probe
(the box face contributes no candidate, R6.0.5) is snapping, which issue 0058
owns this round. PH1's deletion step needs the gesture named above.

Native-backed PH1 coverage in the editor needs one more thing, named here so
it is not discovered late: the placeholder box has to reach the painted scene
before a widget test can observe an instance being selected. That is
`plan_painter.dart`, which another worker owns this round. The core-side PH1
test has no such dependency and can land first.

The core's numerical gaps also stand: rounded polygon construction and binary64
intersection parameters remain issue 0012's, and marquee frame-time budgets
remain issue 0033's.

## Closure

- **Implementation/spec commits:** `d0be026` (gesture, ABI 11 query,
  huge-corner repair) and `1557522` (R7.3.2a, fixture L13, and the R7.4/R7.5
  regression coverage).
- **Verification:** see the evidence below for the per-commit runs.
- **Follow-up issues:** none filed. Placeholder marquee participation (R7.5.3)
  stays with issue 0012, which owns the `placeholder-box` hit class and records
  the gap; it rests on the section 9.6 asset handling of issue 0026 and on
  BEHAVIOR-SPEC section 10, which is still "To define". Numerical exactness
  remains issue 0012's and marquee frame-time budgets remain issue 0033's.
- **Evidence:**
  [marquee checkpoint](../docs/evidence/0059-marquee/PROGRESS.md).
