# 0013: Implement geometry engine and room reconciliation

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0001, 0002, 0019, 0006, 0007, 0011

## Problem

Implement wall outlines/joins, opening cuts, planar room topology, room
metadata reconciliation, measurements, and deterministic derived output using
the selected numerical kernel.

## Acceptance criteria

- [ ] All normative coordinate examples and adversarial spike fixtures pass.
- [ ] Numerical APIs state units, bounds, exact/tolerance guarantees, output
      normalization, and error behavior.
- [ ] Room split/merge/disappearance produces specified operation batches and
      metadata results.
- [ ] Dependency/component invalidation is correct; common local edits avoid
      whole-document rebuilds while topology-wide edits may recompute the
      affected connected component.
- [ ] Optional derived caches are non-authoritative, version/digest checked,
      and safely discardable.

## Progress — 2026-08-30 (Rust core)

Partially delivered by the Rust rewrite. See
`docs/decisions/0008-rust-core-implementation.md`.

Delivered: adaptive filtered orientation predicate (exact via error-free
transformations, 1.14 ns filtered / 1.76 ns fallback); wall polygon generation;
room detection by planar face traversal, including holes (a room inside a room
subtracts the contained area) and collinear-overlap splitting.

Not delivered: wall JOIN geometry. Rooms and wall polygons are computed to
centre lines, so reported area overstates usable floor area by roughly half the
wall thickness around the perimeter. Corners are drawn as overlapping
rectangles rather than mitred joins. Both are visible in any export and must be
closed before area is presented as "floor area".

## Progress — 2026-08-31 (joins, occupied geometry, rooms)

Closes the gap recorded above, and corrects one thing that gap statement got
wrong. **The specification does not ask for mitred joins.** R3.2.3 is explicit:
the join patch is the CONVEX HULL of the member cross-sections, and patches
"need no miter construction and carry no miter-ratio constant". The
implementation follows the specification; fixtures F2, F6 and F7 pin the hull
quads exactly, and the outer corner of an L join is a bevel through
`(4100,0)`, not a square corner.

Delivered:

- `wallJoin` entity in `domowik-format` (FORMAT-SPEC §3): `members` as an
  unordered set of `{wallId, role}`, no coordinates, no `levelId`. Validation
  covers R3.3.1 (at least two distinct members), R3.3.2 (an endpoint belongs to
  one join), R3.3.3 (`degenerate-join-membership`), R3.3.4 (coordinate
  agreement), a dangling-member case, and the level rule from R3.0.6.
- `geometry::polygon` — `Polygon` with holes, and the R3.2.4 canonical
  normalization in full: simple rings, collinear elimination, fixed winding,
  open rings rotated to their lexicographically least vertex, holes and
  components sorted by canonical tuple. Convex hull by Andrew's monotone chain,
  turn direction decided by the exact orientation predicate.
- `geometry::boolean::union` — union of simple polygons by boundary
  classification, with bounding-box rejection. This is the operation R3.2.1
  needs and the one issue 0007 scoped.
- `geometry::walls` — `cross_section` (R3.2.2), `join_patch` (R3.2.3),
  `occupied_region` (R3.2.1), and `rooms` (R5.1.1, R5.1.2, R5.1.4).
- CLI: `add-wall` auto-joins on endpoint-endpoint contact (R3.1.2) and merges
  into an occupied node rather than creating a second join there (R3.3.5);
  `remove-wall` drops memberships and deletes a join left below two members;
  `info` reports floor area measured to the inner faces; SVG draws the occupied
  region as one path per component with rooms beneath and apertures over.

**Rooms are now the bounded components of the complement of the occupied
region**, exactly as R5.1.1 defines them, so floor area is measured to the
inner wall faces. A 4000x3000 shell of 200 mm walls reports 10.64 m², not the
12.00 m² its centre lines enclose. The centre-line room detector was deleted
rather than kept alongside: it disagreed with the specification, and a second
answer to "what is a room" is worse than one.

Normative fixtures passing: F1, F2, F3, F4, F6, F7, and the reversal neutrality
of F8 (as an invariance test rather than a stored fixture).

Twenty-four of twenty-five new checks mutation-proven; see
`docs/evidence/wall-joins/MUTATION-PROOFS.md`, which also records the
twenty-fifth and what was done instead of counting it. Measurements in
`docs/evidence/wall-joins/PERFORMANCE.md`.

Adversarial review by Codex (`gpt-5.6-sol`, high effort) produced four grounded
findings, all accepted; three were fixed and one is now pinned as a documented
bound. `docs/evidence/wall-joins/ADVERSARIAL-REVIEW.md` records them with the
transcript. The most consequential was structural: rooms were being derived by
reading holes off each region component, which cannot see a void enclosed by
several components that touch only at points (R5.1.2). Rooms and the region are
now traced from the same boundary, in opposite directions.

### Still not delivered

- **Contact classification beyond endpoint-endpoint.** R3.1.2 also requires
  endpoint-on-interior and proper crossings to SPLIT the walls they touch
  (fixtures F4, F5, F14, F14b, F15). The geometry for the post-split state is
  implemented and F4's outline passes when given split walls, but no command
  performs the split, so a wall dropped mid-span of another overlaps it instead.
  The union still closes the region, so the drawing and the floor area stay
  correct; what is missing is the join entity and the entity-identity change.
  Splitting mints and rewrites entities, so it belongs with the operation log
  (issue 0011) and cannot land honestly before it.
- **Explicit detach** (R3.3.7) and **joint/body drag** (R3.4.2, R3.4.3): no
  command exists for either.
- **Room metadata reconciliation** (§5.2) and **operation batches for room
  split/merge/disappearance**: unstarted, and gated on issue 0011.
- **Dependency/component invalidation** (R5.1.9): the whole level recomputes on
  every query. Correct, and within budget at present scale, but it is not the
  scoped closure the rule requires.
- `wall-area-overlap` and `collinear-overlap` diagnostics from R3.1.2.

The acceptance criteria remain open on those points; the issue stays
In Progress.

## Wall splitting delivered — 2026-08-31

R3.1.2's remaining two contact rows, unblocked by the operation log (issue
0011) and delivered in the same shape the specification describes: one atomic
batch per accepted creation, including its incidental split and join effects,
and therefore one undo entry.

- **Endpoint-on-interior** — a new wall's endpoint landing strictly inside
  another wall's axis splits that wall, and the mirror case splits the new
  wall. R3.1.2 does not privilege the wall being drawn.
- **Proper crossing** — interior × interior splits BOTH walls, and the four
  children resolve into ONE join at the crossing (fixture F5).
- **R3.1.3 identity** — the child containing the original START keeps the
  original wall id, so the parent is SHORTENED rather than deleted; every
  other child is a fresh entity inheriting level, thickness and height.
- **R3.1.4 join remapping** — a membership on the original End role follows
  the original end endpoint into the FINAL child's End, so a join at the far
  end stays attached to the wall that still reaches it.
- Split points are deduplicated: several walls can meet one point, and cutting
  a wall repeatedly at the same place would produce zero-length walls, which
  R3.0.5 rejects.

Fixtures F4 and F5 pass end to end through the CLI. Six mutation proofs.
Probe P12 exercises the split, the join cardinality, and the undo.

### Refused rather than approximated

**Splitting a wall that hosts openings.** R4.3 remaps an opening to whichever
child contains it and rejects atomically with `opening-split-ambiguous` when a
span straddles the split point. Neither is implemented, and leaving an opening
on a shortened host puts it outside its wall — which validation then rejects on
the NEXT command, blaming the wrong one. The command refuses with a message
saying what to do instead.

**Positive-length collinear overlap.** R3.1.2's fifth row requires the whole
batch to reject with `collinear-overlap`. That diagnostic does not exist, so
two overlapping collinear walls are still accepted; the union closes over them
and the drawing is right, but the batch should have been refused.

Still not delivered from this issue: explicit detach (R3.3.7), joint and body
drag (R3.4.2 canvas gestures, though the NUMERIC edit path is delivered), room
metadata reconciliation (§5.2), R5.1.9 scoped invalidation, and the
`wall-area-overlap` warning.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** Joins, splitting, occupied geometry and rooms delivered. NOT delivered: explicit detach (R3.3.7), canvas joint/body drag, room metadata reconciliation (§5.2), R5.1.9 scoped invalidation, and two R3.1.2 diagnostics.

## Progress — 2026-09-02 (the invalidation criterion is now measured, and unmet)

Issue 0032's corpus supplies the number this issue's fourth acceptance
criterion asks for, and the answer is that the criterion is NOT met.

`walls::plan` is the only derived-geometry entry point and it takes a whole
level, so every accepted batch rebuilds every component of that level. On a
single-component plan that is what BEHAVIOR-SPEC R5.1.9 permits anyway. On a
multi-cluster level it exceeds the permitted closure, measurably:

| Corpus | Gesture | Closure permits | Engine recomputes |
|---|---|---|---|
| `wings-l` | `body-drag` | 24 of 211 walls, 5 of 60 faces | all 211, all 60 |
| `campus-xl` | `joint-drag` | 54 of 329 walls, 13 of 84 faces | all 329, all 84 |
| `campus-xl` | `wall-create-free` | 1 of 330 walls, 0 of 84 faces | all 330, all 84 |

The closure itself is implemented and tested — `domowik_geometry::locality`
computes the R5.1.9 component partition, both link relations, and the
transitive closure over the pre- and post-edit states — so the scoped engine
has an oracle to be checked against, and
`crates/domowik-bench/tests/locality.rs` carries a canary that FAILS when the
scope is fixed.

**What this does not solve.** Locality is vacuous on an ordinary floor plan:
every partition meets the shell at a shared node, so the level is one
join-connected component and the closure is the whole level. Scoping
recomputation helps multi-cluster levels; making a single-component flat
cheaper needs incremental union along the touched boundary, which R5.1.9 does
not ask for and which is not covered by this criterion.

See ED-0098 and [../docs/BENCHMARK-INTERACTION.md](../docs/BENCHMARK-INTERACTION.md).
