//! Polygons, rings, and the canonical normalization of BEHAVIOR-SPEC R3.2.4.
//!
//! A *ring* here is an OPEN vertex sequence: the closing vertex is never
//! repeated. Exterior rings wind counter-clockwise, hole rings clockwise, and
//! every ring produced by this module has been reduced to a simple cycle with
//! duplicate and collinear vertices removed and rotated to begin at its
//! lexicographically least vertex.
//!
//! Canonicalization is not cosmetic. R3.2.4 and R5.1.3 require derived output
//! to be invariant under wall reversal and totally ordered, because the
//! occupied region and the room list are recomputed constantly and any
//! run-to-run instability would show up as spurious diffs in exports and in
//! room metadata reconciliation.

use crate::{orientation, Orientation};
use domowik_format::Point;

/// A polygon: one exterior ring plus zero or more hole rings.
///
/// Coordinates are millimetres in plan space (BEHAVIOR-SPEC §1.1). The
/// exterior ring winds counter-clockwise and holes wind clockwise once the
/// value has passed through [`Polygon::canonicalize`].
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    /// The exterior boundary, counter-clockwise, open (no repeated last point).
    pub outer: Vec<Point>,
    /// Enclosed voids, clockwise, open, sorted by canonical vertex tuple.
    pub holes: Vec<Vec<Point>>,
}

impl Polygon {
    /// Builds a polygon from an exterior ring with no holes.
    pub fn new(outer: Vec<Point>) -> Self {
        Self {
            outer,
            holes: Vec::new(),
        }
    }

    /// The area enclosed by the exterior ring, less the area of every hole.
    ///
    /// Millimetres squared. Always non-negative for a canonical polygon.
    pub fn area_mm2(&self) -> f64 {
        let gross = signed_double_area(&self.outer).abs() / 2.0;
        let holes: f64 = self
            .holes
            .iter()
            .map(|h| signed_double_area(h).abs() / 2.0)
            .sum();
        gross - holes
    }

    /// Total length of the exterior ring.
    pub fn perimeter_mm(&self) -> f64 {
        ring_perimeter(&self.outer)
    }

    /// Applies R3.2.4 in full: simple rings, fixed winding, least-vertex
    /// rotation, sorted holes.
    ///
    /// Returns `None` when the exterior ring degenerates to fewer than three
    /// distinct non-collinear vertices, which is not a polygon.
    pub fn canonicalize(mut self) -> Option<Self> {
        self.outer = canonical_ring(&self.outer, Winding::CounterClockwise)?;
        let mut holes: Vec<Vec<Point>> = self
            .holes
            .iter()
            .filter_map(|h| canonical_ring(h, Winding::Clockwise))
            .collect();
        holes.sort_by(|a, b| compare_rings(a, b));
        self.holes = holes;
        Some(self)
    }
}

/// Which way a canonical ring must wind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Winding {
    /// Positive signed area; the enclosed region is to the left of each edge.
    CounterClockwise,
    /// Negative signed area; used for hole rings.
    Clockwise,
}

/// Twice the signed area of a ring (the shoelace sum).
///
/// Positive for counter-clockwise rings. Twice the area is returned because
/// halving is the only step that can introduce a rounding error, and callers
/// that only need the SIGN should not pay for it.
pub fn signed_double_area(ring: &[Point]) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    // TRANSLATED TO THE FIRST VERTEX before summing, which is not a
    // micro-optimisation but a correctness fix.
    //
    // The untranslated form sums products of magnitude |coordinate|², so a
    // small polygon far from the origin is a difference of huge nearly equal
    // terms. At the coordinate bound the cancellation is total: a 0.002 mm
    // square with corners at (999999.998, 999999.998) evaluated to EXACTLY
    // zero area, and `canonicalize` then discarded a ring whose sides are
    // twice the weld quantum and therefore topologically real (adversarial
    // review, Codex gpt-5.6-sol, 2026-09-04).
    //
    // After translation the products scale with the polygon's own extent
    // instead of with its distance from the origin, so accuracy no longer
    // depends on where in the coordinate domain the plan was drawn.
    let origin = ring[0];
    let mut sum = 0.0;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let (ax, ay) = (a.x - origin.x, a.y - origin.y);
        let (bx, by) = (b.x - origin.x, b.y - origin.y);
        sum += ax * by - bx * ay;
    }
    sum
}

/// Total edge length of a closed ring.
pub fn ring_perimeter(ring: &[Point]) -> f64 {
    if ring.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        total += (b.x - a.x).hypot(b.y - a.y);
    }
    total
}

/// Reduces `ring` to the canonical form R3.2.4 requires.
///
/// Removes repeated vertices (including a repeated closing vertex), removes
/// vertices collinear with their neighbours using the exact orientation
/// predicate, forces `winding`, and rotates the result to begin at the
/// lexicographically least vertex. Returns `None` if fewer than three vertices
/// survive.
pub fn canonical_ring(ring: &[Point], winding: Winding) -> Option<Vec<Point>> {
    let mut pts: Vec<Point> = Vec::with_capacity(ring.len());
    for &p in ring {
        if pts.last().is_none_or(|last| *last != p) {
            pts.push(p);
        }
    }
    while pts.len() > 1 && pts[0] == pts[pts.len() - 1] {
        pts.pop();
    }
    if pts.len() < 3 {
        return None;
    }

    // Drop collinear vertices. Removing one does NOT advance the cursor, so a
    // run of collinear points collapses within this single pass.
    //
    // ONE PASS SUFFICES, including across the wrap. The only vertex a later
    // removal could invalidate without re-testing is index 0, whose `prev` is
    // the last element. That would need the last element to be removed —
    // which requires `pts[n-2], pts[n-1], pts[0]` collinear — AND index 0 to
    // have become collinear, which requires `pts[n-2], pts[0], pts[1]`
    // collinear. Both hold only if all four lie on the line through
    // `pts[n-2]` and `pts[0]`, in which case `pts[n-1], pts[0], pts[1]` were
    // collinear too and index 0 was removed on its first visit. An outer
    // repeat-until-stable loop stood here and no mutation could make it
    // matter; an unreachable guard reads as a tested one, so it is gone.
    let mut i = 0;
    while i < pts.len() && pts.len() >= 3 {
        let prev = pts[(i + pts.len() - 1) % pts.len()];
        let cur = pts[i];
        let next = pts[(i + 1) % pts.len()];
        if orientation(prev, cur, next) == Orientation::Collinear {
            pts.remove(i);
        } else {
            i += 1;
        }
    }
    if pts.len() < 3 {
        return None;
    }

    let area = signed_double_area(&pts);
    if area == 0.0 {
        return None;
    }
    let is_ccw = area > 0.0;
    if is_ccw != (winding == Winding::CounterClockwise) {
        pts.reverse();
    }

    rotate_to_least(&mut pts);
    Some(pts)
}

/// Rotates a ring in place so it begins at its lexicographically least vertex.
fn rotate_to_least(pts: &mut [Point]) {
    let mut best = 0;
    for i in 1..pts.len() {
        if less(pts[i], pts[best]) {
            best = i;
        }
    }
    pts.rotate_left(best);
}

/// Lexicographic comparison of two points: x first, then y.
fn less(a: Point, b: Point) -> bool {
    (a.x, a.y) < (b.x, b.y)
}

/// Total order over canonical rings (R3.2.4 tuple comparison).
///
/// Compares vertex sequences lexicographically over exact `(x, y)` pairs; a
/// prefix-shorter sequence compares less. Both rings are assumed already
/// rotated to their least vertex, which is what makes the comparison
/// rotation-invariant.
pub fn compare_rings(a: &[Point], b: &[Point]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    for (p, q) in a.iter().zip(b.iter()) {
        match (p.x, p.y).partial_cmp(&(q.x, q.y)) {
            Some(Ordering::Equal) | None => continue,
            Some(other) => return other,
        }
    }
    a.len().cmp(&b.len())
}

/// Total order over canonical polygons: outer ring first, then hole tuples.
pub fn compare_polygons(a: &Polygon, b: &Polygon) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match compare_rings(&a.outer, &b.outer) {
        Ordering::Equal => {}
        other => return other,
    }
    for (x, y) in a.holes.iter().zip(b.holes.iter()) {
        match compare_rings(x, y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    a.holes.len().cmp(&b.holes.len())
}

/// The convex hull of `points`, counter-clockwise, without collinear vertices.
///
/// Andrew's monotone chain (A. M. Andrew, "Another efficient algorithm for
/// convex hulls in two dimensions", Information Processing Letters 9(5), 1979)
/// — an independently published algorithm, implemented here from its
/// description (docs/CLEAN-ROOM-POLICY.md).
///
/// Turn direction is decided by the exact orientation predicate, so a hull
/// vertex is never dropped or kept because of rounding. Degenerate input is
/// returned as-is in reduced form: an empty slice yields an empty hull, and
/// collinear input yields the two extreme points, which callers must treat as
/// a lower-dimensional closed set (R3.2.3).
pub fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut pts: Vec<Point> = points.to_vec();
    pts.sort_by(|a, b| {
        (a.x, a.y)
            .partial_cmp(&(b.x, b.y))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }

    let chain = |input: &[Point]| -> Vec<Point> {
        let mut out: Vec<Point> = Vec::with_capacity(input.len());
        for &p in input {
            while out.len() >= 2
                && orientation(out[out.len() - 2], out[out.len() - 1], p)
                    != Orientation::CounterClockwise
            {
                out.pop();
            }
            out.push(p);
        }
        out
    };

    let mut lower = chain(&pts);
    let upper_input: Vec<Point> = pts.iter().rev().copied().collect();
    let mut upper = chain(&upper_input);

    // The last point of each chain is the first of the other.
    lower.pop();
    upper.pop();
    lower.append(&mut upper);
    if lower.len() < 3 {
        // Every point was collinear; report the two extremes.
        return vec![pts[0], pts[pts.len() - 1]];
    }
    lower
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    /// REGRESSION (ED-0100). Before `signed_double_area` translated to a
    /// local origin, a small polygon far from the origin was a difference of
    /// huge nearly-equal products. This 2 µm square at the old coordinate
    /// bound evaluated to EXACTLY zero, so `canonicalize` discarded a ring
    /// whose sides are twice the weld quantum.
    #[test]
    fn a_tiny_polygon_far_from_the_origin_keeps_its_area() {
        let ring = vec![
            P(999_999.998, 999_999.998),
            P(1_000_000.0, 999_999.998),
            P(1_000_000.0, 1_000_000.0),
            P(999_999.998, 1_000_000.0),
        ];
        let doubled = signed_double_area(&ring);
        assert!(doubled > 0.0, "area collapsed to {doubled}");
        // 0.002 x 0.002 mm = 4e-6 mm^2, so the doubled area is 8e-6.
        assert!((doubled - 8e-6).abs() < 1e-9, "doubled area was {doubled}");
    }

    /// Translation must not change an area that was already exact.
    #[test]
    fn translation_does_not_disturb_an_exact_area() {
        let ring = vec![
            P(0.0, 0.0),
            P(4000.0, 0.0),
            P(4000.0, 3000.0),
            P(0.0, 3000.0),
        ];
        assert_eq!(signed_double_area(&ring), 24_000_000.0);
    }

    #[test]
    fn signed_area_is_positive_for_counter_clockwise_rings() {
        let ccw = [P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)];
        assert_eq!(signed_double_area(&ccw), 200.0);
        let cw: Vec<Point> = ccw.iter().rev().copied().collect();
        assert_eq!(signed_double_area(&cw), -200.0);
    }

    #[test]
    fn canonical_ring_drops_collinear_vertices() {
        // The midpoint of the bottom edge carries no information.
        let ring = [
            P(0.0, 0.0),
            P(5.0, 0.0),
            P(10.0, 0.0),
            P(10.0, 10.0),
            P(0.0, 10.0),
        ];
        let out = canonical_ring(&ring, Winding::CounterClockwise).expect("a square");
        assert_eq!(
            out,
            vec![P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)]
        );
    }

    #[test]
    fn canonical_ring_removes_a_chain_of_collinear_vertices() {
        // Two collinear vertices in a row: a single pass that only advances
        // would leave the second behind.
        let ring = [
            P(0.0, 0.0),
            P(3.0, 0.0),
            P(6.0, 0.0),
            P(10.0, 0.0),
            P(10.0, 10.0),
            P(0.0, 10.0),
        ];
        let out = canonical_ring(&ring, Winding::CounterClockwise).expect("a square");
        assert_eq!(out.len(), 4, "got {out:?}");
    }

    #[test]
    fn canonical_ring_removes_a_collinear_run_that_wraps_the_end_of_the_ring() {
        // The run straddles the last vertex and the first, which is the case a
        // single forward pass could in principle miss.
        let ring = [
            P(20.0, 0.0),
            P(30.0, 0.0),
            P(15.0, 10.0),
            P(0.0, 0.0),
            P(10.0, 0.0),
        ];
        let out = canonical_ring(&ring, Winding::CounterClockwise).expect("a triangle");
        assert_eq!(out, vec![P(0.0, 0.0), P(30.0, 0.0), P(15.0, 10.0)]);
    }

    #[test]
    fn canonical_ring_rotates_to_the_least_vertex_and_is_rotation_invariant() {
        let a = [P(10.0, 10.0), P(0.0, 10.0), P(0.0, 0.0), P(10.0, 0.0)];
        let b = [P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)];
        let ca = canonical_ring(&a, Winding::CounterClockwise).expect("square");
        let cb = canonical_ring(&b, Winding::CounterClockwise).expect("square");
        assert_eq!(ca, cb);
        assert_eq!(ca[0], P(0.0, 0.0));
    }

    #[test]
    fn canonical_ring_is_invariant_under_reversal() {
        // R3.2.4: "The result is invariant under wall reversal."
        let ring = [P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)];
        let reversed: Vec<Point> = ring.iter().rev().copied().collect();
        assert_eq!(
            canonical_ring(&ring, Winding::CounterClockwise),
            canonical_ring(&reversed, Winding::CounterClockwise)
        );
    }

    #[test]
    fn canonical_ring_forces_the_requested_winding() {
        let ccw = [P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)];
        let hole = canonical_ring(&ccw, Winding::Clockwise).expect("square");
        assert!(signed_double_area(&hole) < 0.0);
    }

    #[test]
    fn canonical_ring_rejects_degenerate_input() {
        assert!(canonical_ring(&[P(0.0, 0.0), P(1.0, 0.0)], Winding::CounterClockwise).is_none());
        // Collinear: zero area, not a polygon.
        assert!(canonical_ring(
            &[P(0.0, 0.0), P(1.0, 0.0), P(2.0, 0.0)],
            Winding::CounterClockwise
        )
        .is_none());
        // A repeated closing vertex must not fool the vertex count.
        assert!(canonical_ring(
            &[P(0.0, 0.0), P(1.0, 0.0), P(0.0, 0.0)],
            Winding::CounterClockwise
        )
        .is_none());
    }

    #[test]
    fn convex_hull_of_a_square_with_an_interior_point() {
        let pts = [
            P(0.0, 0.0),
            P(10.0, 0.0),
            P(10.0, 10.0),
            P(0.0, 10.0),
            P(5.0, 5.0),
        ];
        let hull = convex_hull(&pts);
        assert_eq!(
            hull,
            vec![P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)]
        );
    }

    #[test]
    fn convex_hull_of_collinear_points_returns_the_two_extremes() {
        // R3.2.3's degenerate patch: the hull is a closed segment, not an area.
        let hull = convex_hull(&[P(0.0, -100.0), P(0.0, 0.0), P(0.0, 100.0)]);
        assert_eq!(hull, vec![P(0.0, -100.0), P(0.0, 100.0)]);
    }

    #[test]
    fn convex_hull_drops_collinear_boundary_points() {
        let hull = convex_hull(&[
            P(0.0, 0.0),
            P(5.0, 0.0),
            P(10.0, 0.0),
            P(10.0, 10.0),
            P(0.0, 10.0),
        ]);
        assert_eq!(hull.len(), 4, "got {hull:?}");
    }

    #[test]
    fn polygon_area_subtracts_holes() {
        let outer = vec![P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)];
        let hole = vec![P(2.0, 2.0), P(2.0, 4.0), P(4.0, 4.0), P(4.0, 2.0)];
        let poly = Polygon {
            outer,
            holes: vec![hole],
        };
        assert_eq!(poly.area_mm2(), 96.0);
    }

    #[test]
    fn compare_rings_orders_by_vertex_tuples_then_length() {
        use std::cmp::Ordering;
        let a = [P(0.0, 0.0), P(1.0, 0.0), P(0.0, 1.0)];
        let b = [P(0.0, 0.0), P(2.0, 0.0), P(0.0, 1.0)];
        assert_eq!(compare_rings(&a, &b), Ordering::Less);
        let short = [P(0.0, 0.0), P(1.0, 0.0)];
        assert_eq!(compare_rings(&short, &a), Ordering::Less);
    }
}
