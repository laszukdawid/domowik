# Mutation proofs — wall joins, occupied geometry, and rooms

Every check introduced with the join, occupied-region and room work, mutated at
its source, with the test that caught the mutation. ED-0041 and ED-0053: a
check never shown to fail is not evidence of anything.

**Method.** Each mutation is applied to the working tree, the named test is run
alone with `cargo test -p <crate> <test>`, and the source is restored before the
next. A mutation that fails to compile is recorded as inconclusive, not as a
pass. **Twenty-five mutations, twenty-four caught.** The one that was not is
analysed below rather than counted.

The last five cover the fixes made after adversarial review; see
`ADVERSARIAL-REVIEW.md` for what prompted them.

| # | Check | Mutation applied | Test | Caught |
|---:|---|---|---|:--:|
| 1 | union: reversed-twin fragments are dropped | `if directed.contains(&(key.1, key.0)) { continue; }` → `if false && directed.contains(&(key.1, key.0)) { continue; }` | `union_of_edge_to_edge_rectangles_merges_and_drops_the_shared_edge` | yes |
| 2 | union: fragments inside another ring are dropped | `if swallowed { continue; }` → `if false && swallowed { continue; }` | `union_of_overlapping_rectangles_is_one_component_of_the_right_area` | yes |
| 3 | union: bounding-box prune keeps touching rings | `self.min_x <= other.max_x + WELD_MM && other.min_x <= self.max_x + WELD_MM && self.min_y <= other.max_y + WELD` → `self.min_x < other.max_x - WELD_MM && other.min_x < self.max_x - WELD_MM && self.min_y < other.max_y - WELD_MM` | `bounding_box_pruning_keeps_rings_that_only_touch` | yes |
| 4 | union: hole goes to the SMALLEST containing outer (nested frames) | `match best { Some((a, _)) if a <= area => {} _ => best = Some((area, i)), }` → `match best { Some((a, _)) if a >= area => {} _ => best = Some((area, i)), }` | `a_hole_is_charged_to_the_smallest_ring_that_contains_it` | yes |
| 5 | union: clockwise sweep uses the half-open interval (0, 360] | `while delta <= 0.0 { delta += std::f64::consts::TAU; }` → `while delta < 0.0 { delta += std::f64::consts::TAU; }` | `rectangles_touching_at_a_single_point_stay_two_rings` | **no** |
| 6 | polygon: collinear elimination handles a run that wraps | `if orientation(prev, cur, next) == Orientation::Collinear { pts.remove(i); } else { i += 1; }` → `if orientation(prev, cur, next) == Orientation::Collinear { pts.remove(i); i += 1; } else { i += 1; }` | `canonical_ring_removes_a_chain_of_collinear_vertices` | yes |
| 7 | walls: room holes are IMMEDIATE containers only | `!inside.iter().any(|other| { !std::ptr::eq(*other, **c) && ring_contains_ring(&other.outer, &c.outer) })` → `true` | `a_nested_island_belongs_to_the_courtyard_that_immediately_contains_it` | yes |
| 8 | polygon: rings rotate to their least vertex | `rotate_to_least(&mut pts);` → `// rotate_to_least(&mut pts);` | `canonical_ring_rotates_to_the_least_vertex_and_is_rotation_invariant` | yes |
| 9 | walls: join patch is the hull of the member cross-sections | `if let Some(cs) = cross_section(wall, m.endpoint) { points.extend_from_slice(&cs); }` → `if let Some(cs) = cross_section(wall, m.endpoint) { points.push(cs[0]); }` | `fixture_f2_l_join_patch_and_outline` | yes |
| 10 | arrangement: on-boundary points are not reported as inside | `return Containment::Boundary;` → `return Containment::Inside;` | `classify_point_separates_inside_boundary_and_outside` | yes |
| 11 | format: R3.3.2 an endpoint belongs to one join | `Some(other) if *other != join.id => {` → `Some(other) if false && *other != join.id => {` | `an_endpoint_cannot_belong_to_two_joins` | yes |
| 12 | format: R3.3.3 a wall cannot join to itself | `if a.wall_id == b.wall_id {` → `if false && a.wall_id == b.wall_id {` | `a_wall_cannot_join_to_itself` | yes |
| 13 | format: R3.3.4 members must meet at one point | `.any(|p| (p.x - first.x).hypot(p.y - first.y) > JOIN_COINCIDENCE_MM)` → `.any(|p| (p.x - first.x).hypot(p.y - first.y) > f64::INFINITY)` | `join_members_must_meet_at_one_point` | yes |
| 14 | format: a join may not span levels | `if levels.len() > 1 {` → `if false && levels.len() > 1 {` | `a_join_cannot_span_levels` | yes |
| 15 | format: a dangling member suppresses the coordinate complaint | `if dangling { // Coordinate agreement is unanswerable with a member missing; // reporting it as well would be ` → `if false && dangling { continue; }` | `a_join_to_a_missing_wall_is_reported_once` | yes |
| 16 | cli: R3.3.5 a contact at an occupied node merges into that join | `if let Some(existing) = doc.join_at(level_id, point).map(|j| j.id) {` → `if let Some(existing) = None::<Uuid> {` | `a_third_wall_at_an_occupied_node_merges_into_that_join` | yes |
| 17 | cli: removing a wall drops its join memberships | `for join in &mut pkg.document.joins { join.members.retain(|m| m.wall_id != wall.id); } pkg.document.joins.reta` → `pkg.document.joins.retain(|j| j.members.len() >= 2);` | `removing_a_wall_drops_its_join_memberships` | yes |
| 18 | cli: info reports floor area, not the centre-line area | `pub fn rooms_on_level(doc: &Document, level_id: Uuid) -> Vec<Polygon> { walls::rooms(&occupied_on_level(doc, l` → `pub fn rooms_on_level(doc: &Document, level_id: Uuid) -> Vec<Polygon> { let (solids, joins) = level_solids(doc` | `info_reports_floor_area_not_the_centre_line_area` | yes |
| 19 | cli: auto-join on endpoint-endpoint contact | `let joined = attach_endpoint(&mut pkg.document, level_id, wall_id, Role::Start, start) + attach_endpoint(&mut ` → `let joined = 0; let _ = (start, end);` | `a_wall_placed_on_another_walls_endpoint_auto_joins` | yes |
| 20 | cli: walls on different levels never join | `for other in doc.walls_on_level(level_id) { if other.id == wall_id {` → `for other in doc.walls.clone().iter() { if other.id == wall_id {` | `walls_on_different_levels_never_join` | yes |

### Post-review additions

| # | Check | Mutation applied | Test | Caught |
|---:|---|---|---|:--:|
| 21 | weld representative is the least candidate, not first-seen | `Some(existing) if (existing.x, existing.y) <= (p.x, p.y) => {}` → `Some(_) => {}` | `output_coordinates_do_not_depend_on_input_order_at_the_weld_tolerance` | yes |
| 22 | rooms come from the reversed boundary, not from component holes | reversed-edge trace → holes-of-components | `a_void_enclosed_by_components_that_only_touch_at_points_is_still_a_room` | yes |
| 23 | rooms come from the reversed boundary (Codex's triangle cycle) | reversed-edge trace → holes-of-components | `a_cycle_of_point_touching_triangles_encloses_one_room` | yes |
| 24 | the cut-collapse tolerance is spatial, not parametric | `WELD_MM / len` → fixed `1e-3` | `cut_params_resolves_a_weld_sized_feature_on_a_full_length_edge` | yes |
| 25 | cuts within one weld collapse to one | `epsilon` → `epsilon * 1e-9` | `cut_params_collapses_cuts_closer_together_than_one_weld` | yes |

## The one that was not caught, and what was done about it

**Mutation 5** widens `next_clockwise`'s angle normalization from `(0, 360]` to
`[0, 360)`. No test changed colour.

The interval matters only when the offset is exactly zero — when the sole
continuation from a vertex is back the edge we arrived on, a dangling spike.
A union of simple rings does not produce one: every kept fragment comes from a
closed ring, and cancellation removes fragments in pairs that leave the
remaining vertex degrees even. No input was found that reaches it.

The code was NOT deleted and the result was NOT recorded as a pass. The
normalization is kept, and its doc comment now says plainly that it is
defensive — in the same class as the walk bound in `trace_rings` — rather than
implying it is a rule under test. A guard that reads as tested when it is not is
the failure mode ED-0089 was written about.

## A check that WAS deleted for the same reason

`canonical_ring` carried an outer repeat-until-stable loop around its collinear
elimination, commented as necessary because "removing a vertex can make its
neighbours collinear in turn". No mutation of it changed any test.

Unlike mutation 5, this one is provably unreachable, and the proof is short
enough to state: the only vertex a later removal can invalidate without
re-testing is index 0, whose `prev` is the last element. Reaching that needs the
last element removed — so `pts[n-2], pts[n-1], pts[0]` collinear — and index 0
newly collinear — so `pts[n-2], pts[0], pts[1]` collinear. Both hold only if all
four lie on one line, in which case `pts[n-1], pts[0], pts[1]` were collinear
too and index 0 was removed on its first visit.

The loop is gone and the proof is in the source. A test
(`canonical_ring_removes_a_collinear_run_that_wraps_the_end_of_the_ring`) pins
the wrap case that motivated it.

## Two tests that were passing for the wrong reason

Both were found by the mutation run, not by reading:

- `bounding_box_pruning_keeps_rings_that_only_touch` originally used two
  rectangles meeting along their FULL shared edge. Those edges already coincide
  exactly and cancel as a reversed twin with no cutting at all, so disabling
  the bounding-box margin changed nothing. Rewritten to a PARTIAL edge overlap,
  where the cut is what creates the twin — and it now fails under mutation.
- `a_join_to_a_missing_wall_is_reported_once` used a two-member join. With one
  member missing, the coordinate-agreement check has a single point to compare
  and is vacuously satisfied, so the dangling short-circuit it claimed to test
  could be removed with no effect. Rewritten to three members, one missing and
  the two survivors in disagreement.
