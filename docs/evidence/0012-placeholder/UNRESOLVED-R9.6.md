# Unresolved §9.6 inputs after the placeholder-box class

## Current disposition — 2026-10-08

This is historical checkpoint evidence. The
[2026-10-08 integration](../0059-marquee/2026-10-08.md) supersedes statements
below about missing descriptors, requirement levels, read-only mode, frontend
PH1 plumbing, deletion and the undecided declared footprint. Those inputs and
native-backed selection/deletion/refusal tests are now delivered. Declared
width × depth is scaled/rotated before taking axis-aligned bounds; the fixed
fallback is unchanged. Resolved model footprint clicking/dragging exists
separately; resolved footprints are not part of the placeholder marquee query.
External-cache/retrieval, wider offline-summary obligations and numerical/
whole-app performance conformance remain with their owning issues.
The older measurements and observations below retain their original dates.

## Historical checkpoint

The `placeholder-box` hit class is delivered and R7.5.3 is still not
implementable end to end. This is the specific list of what is missing, so the
remaining work is a list of inputs rather than a feeling that something is
absent. Each item names who owns it and what it blocks.

## 1. The resolution result itself — DELIVERED 2026-09-12 (issue 0026)

`domowik-assets` landed it in the agreed shape: `resolve(&[InstanceRef], …)`
returns a `ResolutionSnapshot` giving each instance a `requirement`, a `state`
of `Resolved` or `Degraded(reason)`, and `declared_extent`. It takes
`InstanceRef` rather than `domowik_format::AssetInstance`, so neither crate
depends on the other and the call site copies three fields across.

The geometry seam is unchanged and needs no rework: a caller keeps the
degraded entries, builds each box, and passes `&[Placeholder]`.

### Original entry, kept for the record

§9.6's matrix is indexed by three things: requirement level, package profile,
and blob condition (missing, digest mismatch, undecodable, unsupported
encoding or media type, offline). No Rust query answers any of them today.
`Placeholder { instance, bounds }` is the seam the geometry layer accepts, and
producing it is issue 0026's.

Blocks: every placeholder behaviour, because candidacy is a per-instance
verdict by decision of root on 2026-09-12 and not a property of the era.

## 2. Requirement level has no source (issue 0026)

The matrix's left column is requirement level, and it decides read-only versus
editable. The issue-0026 worker has confirmed that `placeholderDimensions` and
requirement class are DESCRIPTOR fields, not instance fields, and the
descriptor is not built yet. So nothing can currently tell a required asset
from an optional one.

Blocks: the deletion gate below, and the "Editable open blocked" row of the
matrix. It can be specified now and not enforced.

## 3. Read-only degraded mode is not represented in the core (issue 0029)

§9.6 says a required missing asset blocks editable open and leaves read-only
degraded inspection available, and root's ruling is that deletion must respect
that. Nothing in the core carries such a mode: a case-insensitive search for
`read_only`, `readonly` and `degraded` over `crates/` on this branch matches
one doc comment and no type.

Assigned to issue 0029 on 2026-09-12, with the shape root set: the core
refuses edits and saving, while selection and inspection continue. That split
is the one this evidence argued for, and it falls out of R7.1.2: selection is
view-local transient state that never reaches the document, so inspecting a
read-only plan changes nothing.

Issue 0029 reports tests holding reads and `read_package` open while degraded,
so the gate cannot be tightened into hit testing without a named test going
red. That covers the CONTAINER layer.

**One residual sits above it, and is owed rather than covered.** The spike
bridge caches one revision-keyed hit index per plan, so `domowik_plan_hit_test`
takes `&mut` on the SPIKE's own `Plan` — for `hits` and `hit_index_builds` —
while touching the session only through its shared accessor. Nothing is wrong
today, and issue 0029's tests cannot reach it: the spike is a separate Cargo
workspace, and no ABI call can currently produce a degraded plan, because
nothing creates an `assetInstance` and no descriptor persists. The obligation
is therefore to add the equivalent named test AT THE BRIDGE when read-only
mode reaches it, asserting that hit test and marquee still answer on a
read-only degraded plan. Without it, a gate applied to handle or `Plan`
mutability rather than to the session would break selection on exactly the
documents placeholders exist for, and nothing would go red.

Blocks: PH1's deletion step, and any honest claim that read-only mode is
enforced.

## 4. The descriptor-dimensions branch of the geometry (needs a decision)

§9.6's geometry clause has two branches and only the fallback is implemented.
It is no longer unreachable: `declared_extent` now exists as data, so the
branch goes live the moment descriptors persist, which waits on one manifest
field from issue 0029.

`placeholder_geometry` therefore returns `PlaceholderGeometry`, whose
`DeclaredUndecided` variant makes the gap visible at the call site instead of
guessing a box or dropping the instance from selection.

TWO questions have to be answered before that branch can be written, not one:

1. **Does the declared extent scale with `scaleUniform`?** ED-0106 settled this
   for the fallback and said in terms that the declared case is separate.
2. **Which declared dimensions form the PLAN footprint?** `Extent` carries
   `width_mm`, `height_mm` and `depth_mm`, and §9.6 asks for an axis-aligned
   box in plan. The specification does not say which axes map to it. The naive
   reading, width by height, is very likely wrong for a floor plan: a
   600 x 2100 x 400 door-like object has a footprint of 600 by 400, and
   height is vertical. `DeclaredUndecided` carries all three dimensions
   unprojected for exactly this reason.

Blocks: the declared branch only. The fallback is complete and is the only
branch reachable while no descriptor is persisted.

## 5. A resolved instance has no interaction shape at all (issue 0004 / §10)

R7.1.3 class 4 admits DEGRADED instances only. A successfully resolved
instance is therefore not a `placeholder-box` candidate, and BEHAVIOR-SPEC §10
is still "To define", so it has no other interaction shape either. With the
code as delivered, a resolved instance is invisible to clicking and to
marquee.

That may be correct for v0.1, where nothing renders resolved content anyway.
It is recorded because it is a silent consequence: the day resolution starts
succeeding, instances will start disappearing from selection, and the cause
will not be obvious from the call site.

Root ruled on 2026-09-12 that this stays a §10 gap and must be recorded as an
obligation rather than hidden. BEHAVIOR-SPEC §10 now carries it, including the
instruction that resolved instances must NOT be fed to `placeholder-box` to
make the symptom go away: that box is a deterministic stand-in for content
that could not be decoded, and it says nothing about content that decoded.
The query side honours that already: candidacy comes only from the caller's
resolution snapshot, and the test
`an_instance_absent_from_the_snapshot_is_not_a_candidate` holds it there.

## 6. The offline summary (issue 0026)

§9.6 requires one summary of how many assets are unavailable rather than
per-asset noise, and counts the offline cases separately. It consumes the same
resolution results as the snapshot above, so it belongs with whoever builds
them.

Blocks: nothing here; noted so the requirement is not lost between issues.

## 7. Deleting an instance has no gesture (root sequences; ABI 14)

PH1's last step deletes the primary placeholder. `domowik-edit` has no
`deleteAssetInstance`, and the bridge has no call for one. ABI 14 is reserved
for a later combined bridge integration, so this is sequenced by root rather
than taken here.

Unlike a wall, an instance has no dependents: no joins, no openings, no host.
Whoever writes the gesture should say so explicitly rather than leave readers
wondering what cascades, and R7.4's primary-unset propagation applies
unchanged.
