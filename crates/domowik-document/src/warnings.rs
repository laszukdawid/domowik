//! Same-level wall-pair area-overlap warnings (BEHAVIOR-SPEC R3.1.2 last row,
//! R3.1.7), and the single entry point that combines every document-level
//! diagnostic for a caller (CLI, native bridge) that wants them all.
//!
//! # What this is, and why it is not a reject
//!
//! Two same-level walls whose occupied rectangles overlap with POSITIVE AREA
//! but whose AXES share no contact at all — R3.1.2's last row — are legal:
//! "No join, no split; semantic warning `wall-area-overlap` after final-state
//! validation; batch commits." R3.1.7 makes this unconditional: "emitted
//! after ALL validation and resolution succeed, against final positions; it
//! does not abort and the batch commits. Warnings are diagnostics attached
//! to the batch result, not operations." So, unlike
//! [`crate::wall_contact::wall_contact_refusals`], this module never blocks
//! admission and takes no `before` state — every call judges the CURRENT
//! final document, unconditionally, from scratch.
//!
//! # The gate
//!
//! `axis_contact(..) == Some(AxisContact::Disjoint)` ONLY. Every other class
//! — including a non-collinear `EndpointTouch` — already HAS axis contact,
//! which R3.1.2 excludes from this row by definition: those pairs are either
//! auto-joined, split, or (for `Coincident`/`CollinearOverlap`) refused
//! outright by [`crate::wall_contact::wall_contact_refusals`], and never
//! reach this check.
//!
//! A `Disjoint` pair with an endpoint of each in ONE topology cell is in
//! contact too: the two endpoints are one point (ED-0103 N-2), and the
//! builder joins them (ED-0119). Exact axes a fraction of a micrometre apart
//! are not "no axis contact", so such a pair never warns.
//!
//! # Broad-phase, and why it must be conservative
//!
//! A same-level all-pairs scan is what the spatial index exists to avoid.
//! Walls are grouped by `levelId` BEFORE any index is built, and one index
//! covers one level's walls only: same-level is a precondition of contact at
//! all (R3.0.6), so a shared global index would waste every query on
//! same-XY candidates from OTHER levels it can only ever reject — real cost
//! for a multi-storey plan whose floors share a footprint. Each group's query
//! box is built directly from its walls' axis endpoints, expanded by their
//! FULL `thicknessMm` (not half — no `t/2` underflow to guard) and then
//! rounded one ULP further outward on every side, so IEEE round-to-nearest
//! can never narrow a box that must remain a superset of the true occupied
//! rectangle. It is built from the axis directly, never from `WallSolid`'s
//! rounded corner construction — reusing that construction's already-known
//! inexactness here would risk excluding a true positive from the broad
//! phase, not merely admitting a false one (which the exact filter below
//! discards for free). The query scratch buffer is built once per level
//! group and reused across every wall's query in it, not once per query, so
//! setup does not become quadratic in the wall count.
//!
//! # Invalid geometry is filtered first, not tolerated downstream
//!
//! A wall with a non-finite or out-of-`COORD_BOUND_MM` coordinate, a
//! zero-length axis, or a non-finite/non-positive `thicknessMm` is excluded
//! BEFORE it is grouped by level or given an `Aabb` at all. This is not
//! merely policy: an inverted or NaN-laden box built from such a wall (a
//! negative thickness inverts `expanded`'s min/max) can underflow
//! [`domowik_geometry::spatial::SpatialIndex`]'s cell-count arithmetic —
//! `report`/`validate` run this over documents that have not passed
//! [`domowik_format::Document::validate_plan`], so this filter is load
//! bearing, not defensive dressing. An excluded wall is simply invisible to
//! this check, the same tolerance the rest of this crate extends to a
//! legacy-invalid document undergoing an unrelated edit.
//!
//! # Exactness
//!
//! [`domowik_geometry::wall_overlap::rectangles_overlap`] decides the exact
//! answer; `None` (an out-of-domain call this module's own filter should
//! already have prevented) is treated as "no warning" rather than trusted
//! blindly.

use domowik_format::{
    topology_cell, Document, Point, ValidationWarning, Wall, COORD_BOUND_MM, DIMENSION_CAP_MM,
};
use domowik_geometry::spatial::{Aabb, QueryScratch, SpatialIndex};
use domowik_geometry::wall_contact::{axis_contact, AxisContact};
use domowik_geometry::wall_overlap::rectangles_overlap;
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

/// A same-level positive-area occupied-rectangle overlap with no axis
/// contact (BEHAVIOR-SPEC R3.1.2's last row). Non-blocking: see the module
/// documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum WallAreaOverlapBlock {
    #[error(
        "wall-area-overlap: walls {wall} and {other} on level {level} occupy overlapping \
         area with no axis contact"
    )]
    WallAreaOverlap {
        level: Uuid,
        wall: Uuid,
        other: Uuid,
    },
}

/// Every same-level `wall-area-overlap` warning in `doc`'s CURRENT state.
///
/// Judged unconditionally (R3.1.7): there is no `before`, so a thickness-only
/// edit is reclassified exactly like any other call — this always recomputes
/// from the walls it is given, never from a cache of its own. Deterministic:
/// results sort by `(level, wall, other)`, the ascending UUID pair,
/// independent of document order (mirrors
/// [`crate::wall_contact::wall_contact_refusals`]).
///
/// A geometrically invalid wall (see the module documentation) never reaches
/// a level group, an `Aabb`, or the index built over it.
///
/// O(n) to filter and group by level, O(m log m) per level of m walls to
/// build that level's broad-phase index (`SpatialIndex::build` sorts extents
/// to size its grid), plus one query per wall; the exact filter runs only on
/// the candidates the broad phase admits.
pub fn wall_area_overlap_warnings(doc: &Document) -> Vec<WallAreaOverlapBlock> {
    if doc.walls.len() < 2 {
        return Vec::new();
    }

    let mut by_level: HashMap<Uuid, Vec<&Wall>> = HashMap::new();
    for wall in &doc.walls {
        if has_valid_geometry(wall) {
            by_level.entry(wall.level_id).or_default().push(wall);
        }
    }

    let mut blocks = Vec::new();
    for walls in by_level.values() {
        if walls.len() < 2 {
            continue;
        }
        let entries: Vec<(Aabb, usize)> = walls
            .iter()
            .enumerate()
            .map(|(i, w)| (conservative_bounds(w), i))
            .collect();
        let index = SpatialIndex::build(entries);
        let mut scratch = QueryScratch::default();

        for (i, wall) in walls.iter().enumerate() {
            let region = conservative_bounds(wall);
            for j in index.query_with_scratch(&region, &mut scratch) {
                if j <= i {
                    // Each unordered pair is judged once, from its lower
                    // index; `j == i` is the wall against itself.
                    continue;
                }
                let other = walls[j];
                // Reject and resolve classes have axis contact and never
                // reach this row; the gate is exact and admits ONLY
                // `Disjoint`.
                match axis_contact(wall.start(), wall.end(), other.start(), other.end()) {
                    Some(AxisContact::Disjoint) => {}
                    _ => continue,
                }
                let overlaps = rectangles_overlap(
                    wall.start(),
                    wall.end(),
                    wall.thickness_mm,
                    other.start(),
                    other.end(),
                    other.thickness_mm,
                );
                // The cell test runs only on an overlap, which is rare, so the
                // clean traversal pays nothing for it.
                if overlaps == Some(true) && !share_an_endpoint_cell(wall, other) {
                    let (a, b) = if wall.id < other.id {
                        (wall.id, other.id)
                    } else {
                        (other.id, wall.id)
                    };
                    blocks.push(WallAreaOverlapBlock::WallAreaOverlap {
                        level: wall.level_id,
                        wall: a,
                        other: b,
                    });
                }
            }
        }
    }
    blocks.sort_unstable();
    blocks
}

/// Whether `wall` has an occupied rectangle to test at all: a finite,
/// in-domain, positive-length axis (R3.0.4/R3.0.5) and a finite `thicknessMm`
/// in `(0, DIMENSION_CAP_MM]` (R3.0.4). A document `report`/`validate`
/// inspects has not necessarily passed
/// [`domowik_format::Document::validate_plan`], so this is checked
/// explicitly rather than assumed — see the module documentation for what an
/// unchecked negative thickness or non-finite coordinate does to
/// `SpatialIndex`'s cell arithmetic. The upper bound matters too, not only
/// the lower one: an in-domain axis with an absurd, merely-finite thickness
/// still inflates `conservative_bounds` far past what any real wall could
/// occupy, for a broad-phase box `rectangles_overlap` would answer `None`
/// for anyway.
fn has_valid_geometry(wall: &Wall) -> bool {
    let bounded = |p: Point| {
        p.x.is_finite()
            && p.y.is_finite()
            && p.x.abs() <= COORD_BOUND_MM
            && p.y.abs() <= COORD_BOUND_MM
    };
    let (start, end) = (wall.start(), wall.end());
    bounded(start)
        && bounded(end)
        && (start.x != end.x || start.y != end.y)
        && wall.thickness_mm.is_finite()
        && wall.thickness_mm > 0.0
        && wall.thickness_mm <= DIMENSION_CAP_MM
}

/// The broad-phase query box for `wall`: its axis endpoints, expanded
/// outward by its full thickness (never half — no underflow to guard), then
/// rounded one further ULP outward on every side. See the module
/// documentation for why this is built from the axis directly rather than
/// from the rounded `WallSolid` polygon construction. Callers must have
/// already excluded anything [`has_valid_geometry`] rejects: this performs
/// no check of its own and an invalid input can produce an inverted box.
fn conservative_bounds(wall: &Wall) -> Aabb {
    let raw = Aabb::from_corners(wall.start(), wall.end()).expanded(wall.thickness_mm);
    Aabb {
        min_x: raw.min_x.next_down(),
        min_y: raw.min_y.next_down(),
        max_x: raw.max_x.next_up(),
        max_y: raw.max_y.next_up(),
    }
}

/// Every diagnostic BEHAVIOR-SPEC attaches to a document's CURRENT final
/// state: §2.1's level advisories, then R3.1.2's `wall-area-overlap`.
///
/// The order is fixed and deliberate: level warnings keep
/// [`domowik_format::Document::warnings`]'s EXISTING order unchanged, and
/// wall-area-overlap warnings — deterministically ordered among themselves —
/// are appended after. `Document::warnings` itself is UNCHANGED by this;
/// this function only adds a sibling check that needs `domowik-geometry`,
/// which `domowik-format` may not depend on (`domowik-geometry` depends on
/// `domowik-format`, never the reverse).
pub fn validation_warnings(doc: &Document) -> Vec<Warning> {
    let mut warnings: Vec<Warning> = doc.warnings().into_iter().map(Warning::Level).collect();
    warnings.extend(
        wall_area_overlap_warnings(doc)
            .into_iter()
            .map(Warning::WallAreaOverlap),
    );
    warnings
}

/// One document-level diagnostic, from either source.
///
/// `#[serde(untagged)]`: each inner type already self-describes via its own
/// `code` field ([`ValidationWarning`]'s existing tag, or
/// [`WallAreaOverlapBlock`]'s), so the wrapper adds nothing to the JSON
/// shape — a caller already reading `{code, ...}` objects from
/// [`domowik_format::Document::warnings`] sees the SAME shape, with a new
/// possible `code` value in the same array.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(untagged)]
pub enum Warning {
    #[error(transparent)]
    Level(ValidationWarning),
    #[error(transparent)]
    WallAreaOverlap(WallAreaOverlapBlock),
}

/// Whether an endpoint of `a` and an endpoint of `b` are one topology point.
fn share_an_endpoint_cell(a: &Wall, b: &Wall) -> bool {
    let ends = |w: &Wall| [topology_cell(w.start()), topology_cell(w.end())];
    let (ea, eb) = (ends(a), ends(b));
    ea.iter().any(|c| eb.contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{Level, Point};

    fn wall(level: Uuid, id: u128, a: (f64, f64), b: (f64, f64), thickness_mm: f64) -> Wall {
        let mut w = Wall::new(
            level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            thickness_mm,
            2500.0,
        );
        w.id = Uuid::from_u128(id);
        w
    }

    fn one_level_doc() -> (Document, Uuid) {
        let mut doc = Document::new();
        let level = Level::new("Level 1");
        let id = level.id;
        doc.levels.push(level);
        (doc, id)
    }

    #[test]
    fn the_spec_nonparallel_fixture_warns() {
        // BEHAVIOR-SPEC R3.1.2's own example: axes cross only at (2050,0),
        // outside W1's segment, while the t=200 bands overlap near W1's
        // right cap.
        let (mut doc, level) = one_level_doc();
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (2000.0, 0.0), 200.0));
        doc.walls
            .push(wall(level, 2, (850.0, 900.0), (3250.0, -900.0), 200.0));
        let warnings = wall_area_overlap_warnings(&doc);
        assert_eq!(
            warnings,
            vec![WallAreaOverlapBlock::WallAreaOverlap {
                level,
                wall: Uuid::from_u128(1),
                other: Uuid::from_u128(2),
            }]
        );
    }

    #[test]
    fn parallel_strips_touching_at_one_ulp_are_distinguished() {
        // A t=200 strip's band is +/-100 either side of its axis. Two axes
        // 200mm apart have bands that touch at exactly one line (zero area);
        // 200mm minus one ULP overlaps by a hair; the exact filter — not the
        // broad phase — must be what draws this line.
        let (mut doc, level) = one_level_doc();
        let touching = wall(level, 1, (0.0, 200.0), (4000.0, 200.0), 200.0);
        let base = wall(level, 2, (0.0, 0.0), (4000.0, 0.0), 200.0);
        doc.walls.push(base.clone());
        doc.walls.push(touching);
        assert!(
            wall_area_overlap_warnings(&doc).is_empty(),
            "bands meeting at a zero-area line must not warn"
        );

        let nudged_y = f64::from_bits(200.0f64.to_bits() - 1);
        doc.walls.pop();
        doc.walls
            .push(wall(level, 1, (0.0, nudged_y), (4000.0, nudged_y), 200.0));
        assert_eq!(
            wall_area_overlap_warnings(&doc).len(),
            1,
            "one ULP inside must warn"
        );
    }

    #[test]
    fn order_is_independent_of_document_order_reversal_and_levels() {
        let (mut doc, level) = one_level_doc();
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (4000.0, 0.0), 200.0));
        doc.walls
            .push(wall(level, 2, (0.0, 50.0), (4000.0, 50.0), 200.0));
        let forward = wall_area_overlap_warnings(&doc);
        doc.walls.reverse();
        let reversed = wall_area_overlap_warnings(&doc);
        assert_eq!(forward, reversed);
        assert_eq!(
            forward,
            vec![WallAreaOverlapBlock::WallAreaOverlap {
                level,
                wall: Uuid::from_u128(1),
                other: Uuid::from_u128(2),
            }]
        );

        // Different levels never interact (R3.0.6), even at identical XY.
        let other_level = Uuid::from_u128(99);
        doc.walls[1].level_id = other_level;
        assert!(wall_area_overlap_warnings(&doc).is_empty());
    }

    #[test]
    fn reversing_either_walls_stored_direction_does_not_change_the_warning() {
        let (mut doc, level) = one_level_doc();
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (4000.0, 0.0), 200.0));
        doc.walls
            .push(wall(level, 2, (0.0, 50.0), (4000.0, 50.0), 200.0));
        let baseline = wall_area_overlap_warnings(&doc);
        assert_eq!(baseline.len(), 1);

        // Reversal swaps stored roles, never the occupied axis.
        let w1 = doc
            .walls
            .iter_mut()
            .find(|w| w.id == Uuid::from_u128(1))
            .unwrap();
        std::mem::swap(&mut w1.start_mm, &mut w1.end_mm);
        assert_eq!(wall_area_overlap_warnings(&doc), baseline);

        let w2 = doc
            .walls
            .iter_mut()
            .find(|w| w.id == Uuid::from_u128(2))
            .unwrap();
        std::mem::swap(&mut w2.start_mm, &mut w2.end_mm);
        assert_eq!(wall_area_overlap_warnings(&doc), baseline);
    }

    #[test]
    fn a_touching_endpoint_has_axis_contact_and_never_warns() {
        // The gate is `Disjoint` ONLY: a non-collinear endpoint touch, an
        // endpoint-on-interior, a proper crossing and a collinear overlap
        // all have SOME axis contact and must never reach this row, however
        // much their bands overlap.
        let (mut doc, level) = one_level_doc();
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (2000.0, 0.0), 400.0));
        doc.walls
            .push(wall(level, 2, (2000.0, 0.0), (2000.0, 2000.0), 400.0));
        assert!(
            wall_area_overlap_warnings(&doc).is_empty(),
            "an endpoint touch has axis contact and must not warn"
        );
    }

    /// An endpoint a fraction of a micrometre past another wall's end, in the
    /// same topology cell, is one point with it (ED-0103 N-2) and is joined
    /// there. In the next cell it is a separate point, and the overlap warns.
    #[test]
    fn endpoints_in_one_topology_cell_are_in_contact_and_never_warn() {
        let (mut doc, level) = one_level_doc();
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (4000.0, 0.0), 200.0));
        doc.walls
            .push(wall(level, 2, (4000.0004, 0.0), (4000.0004, 3000.0), 200.0));
        assert!(wall_area_overlap_warnings(&doc).is_empty());
        doc.walls[1].start_mm = [4000.0009, 0.0];
        doc.walls[1].end_mm = [4000.0009, 3000.0];
        assert_eq!(wall_area_overlap_warnings(&doc).len(), 1);
    }

    #[test]
    fn validation_warnings_orders_level_warnings_before_wall_area_overlap() {
        let (mut doc, level) = one_level_doc();
        doc.levels[0].default_wall_height_mm = 3000.0;
        doc.levels[0].default_ceiling_height_mm = 2000.0; // CeilingBelowWallHeight
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (4000.0, 0.0), 200.0));
        doc.walls
            .push(wall(level, 2, (0.0, 50.0), (4000.0, 50.0), 200.0));

        let all = validation_warnings(&doc);
        assert_eq!(all.len(), 2, "one level warning, one wall-area-overlap");
        assert!(matches!(
            all[0],
            Warning::Level(ValidationWarning::CeilingBelowWallHeight { .. })
        ));
        assert!(matches!(all[1], Warning::WallAreaOverlap(_)));
    }

    #[test]
    fn thickness_only_change_is_reclassified_from_scratch() {
        // No cache lives in this function: a thickness-only change (no axis
        // write at all) must still be reflected the very next call.
        let (mut doc, level) = one_level_doc();
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (4000.0, 0.0), 200.0));
        doc.walls
            .push(wall(level, 2, (0.0, 250.0), (4000.0, 250.0), 200.0));
        assert!(
            wall_area_overlap_warnings(&doc).is_empty(),
            "250mm apart, t=200: clear"
        );

        doc.walls[1].thickness_mm = 400.0; // band now reaches down to 50mm
        assert_eq!(
            wall_area_overlap_warnings(&doc).len(),
            1,
            "widening the second wall alone must be enough to warn"
        );
    }

    #[test]
    fn invalid_axes_and_thickness_are_filtered_before_any_index_and_never_panic() {
        // `report`/`validate` run this over documents that have not passed
        // `validate_plan`. Most of these would build an inverted or
        // NaN-laden `Aabb` if they ever reached `conservative_bounds`, which
        // can underflow `SpatialIndex`'s cell-count arithmetic; the
        // over-cap thickness is merely finite but absurd. The point of this
        // test is that none of them get that far, and the otherwise valid
        // pair among them still warns.
        let (mut doc, level) = one_level_doc();
        doc.walls
            .push(wall(level, 1, (0.0, 0.0), (4000.0, 0.0), 200.0));
        doc.walls
            .push(wall(level, 2, (0.0, 50.0), (4000.0, 50.0), 200.0));

        let mut nan_wall = wall(level, 3, (0.0, 0.0), (4000.0, 0.0), 200.0);
        nan_wall.start_mm[0] = f64::NAN;
        doc.walls.push(nan_wall);

        let mut negative_thickness = wall(level, 4, (0.0, 100.0), (4000.0, 100.0), 200.0);
        negative_thickness.thickness_mm = -5.0;
        doc.walls.push(negative_thickness);

        let mut out_of_bound = wall(level, 5, (0.0, 200.0), (4000.0, 200.0), 200.0);
        out_of_bound.end_mm[0] = COORD_BOUND_MM * 2.0;
        doc.walls.push(out_of_bound);

        let mut zero_length = wall(level, 6, (0.0, 300.0), (4000.0, 300.0), 200.0);
        zero_length.end_mm = zero_length.start_mm;
        doc.walls.push(zero_length);

        let mut zero_thickness = wall(level, 7, (0.0, 400.0), (4000.0, 400.0), 0.0);
        zero_thickness.thickness_mm = 0.0;
        doc.walls.push(zero_thickness);

        let mut infinite_thickness = wall(level, 8, (0.0, 500.0), (4000.0, 500.0), 200.0);
        infinite_thickness.thickness_mm = f64::INFINITY;
        doc.walls.push(infinite_thickness);

        // Merely finite is not enough: past `DIMENSION_CAP_MM` it inflates
        // the broad-phase box far past any real wall, for a pair
        // `rectangles_overlap` would answer `None` for regardless.
        let mut over_cap_thickness = wall(level, 9, (0.0, 600.0), (4000.0, 600.0), 200.0);
        over_cap_thickness.thickness_mm = DIMENSION_CAP_MM * 2.0;
        doc.walls.push(over_cap_thickness);

        let warnings = wall_area_overlap_warnings(&doc);
        assert_eq!(
            warnings,
            vec![WallAreaOverlapBlock::WallAreaOverlap {
                level,
                wall: Uuid::from_u128(1),
                other: Uuid::from_u128(2),
            }],
            "the six invalid walls are invisible; the one valid pair still warns"
        );
    }

    #[test]
    fn an_invalid_wall_alone_on_its_level_leaves_that_level_silent() {
        let (mut doc, level) = one_level_doc();
        let mut nan_wall = wall(level, 1, (0.0, 0.0), (4000.0, 0.0), 200.0);
        nan_wall.start_mm[1] = f64::NAN;
        doc.walls.push(nan_wall);
        assert!(wall_area_overlap_warnings(&doc).is_empty());
    }
}
