//! Union of simple polygons, on the snap-rounded arrangement.
//!
//! # Why a union at all
//!
//! BEHAVIOR-SPEC R3.2.1 defines a wall cluster's occupied region as the UNION
//! of each wall's butt-capped rectangle and each join's patch, and R5.1.1
//! defines the rooms of a level as the bounded components of the complement of
//! that region. Both the drawing and the floor area therefore depend on one
//! operation: unioning overlapping rectangles into a single boundary.
//!
//! # How it works
//!
//! [`crate::overlay::Arrangement`] snap-rounds every input edge onto the
//! topology grid and assigns each face its winding number. The union is the
//! set of faces with positive winding; its boundary is every half-edge with an
//! occupied face on its left and a free one on its right. Rings are traced by
//! taking, at each vertex, the first boundary half-edge clockwise from the way
//! back, which keeps the region on the left and splits a pinch point — two
//! components touching at one vertex — into two simple rings rather than one
//! figure-eight, as R5.1.2 requires.
//!
//! Output coordinates are cell centres (multiples of 2⁻¹⁰ mm). Input on that
//! grid — every whole or binary-fraction millimetre value — is reproduced
//! exactly.

use crate::overlay::{assemble, to_points, Arrangement, CellPolygon, CellRing, InputRing};
use crate::polygon::Polygon;
use domowik_format::Point;
use std::cmp::Ordering;

/// One input ring and its caller-defined source tag.
///
/// The tag is an opaque identity, not an index into this slice; gaps and
/// arbitrary values are preserved.
#[derive(Debug, Clone)]
pub struct SourceRing {
    pub points: Vec<Point>,
    pub source: usize,
}

/// The provenance of ONE canonical boundary edge, kept at the granularity the
/// arrangement actually resolved rather than at the granularity canonical
/// output happens to print.
///
/// Canonicalization deletes collinear vertices, so one canonical edge can span
/// several consecutive boundary pieces that different inputs contributed. The
/// union of their tags would answer "who touches this edge" but not "who owns
/// the part of it under THIS point", and an ownership rule built on the union
/// alone cannot tell a wall's own face from a collinear neighbour's. So the
/// pieces are retained: `breaks` holds the deleted interior vertices in order
/// from the edge's start vertex to its end vertex, and `sources` holds one tag
/// list per piece, so `sources.len() == breaks.len() + 1`. An edge that lost no
/// vertex has no breaks and exactly one piece.
///
/// Within a piece, co-ownership is real and not an artefact: two inputs whose
/// boundaries COINCIDE there both contributed it, and no finer split exists.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryRun {
    /// Interior vertices removed by collinear canonicalization, in order from
    /// the canonical edge's start vertex towards its end vertex.
    pub breaks: Vec<Point>,
    /// One sorted, deduplicated tag list per piece.
    pub sources: Vec<Vec<usize>>,
}

impl BoundaryRun {
    /// Every tag touching this edge, sorted and deduplicated.
    ///
    /// Lossy on purpose: it answers a membership question, never an ownership
    /// one. Use [`BoundaryRun::piece_at`] when the answer must be about a
    /// particular point.
    pub fn merged_sources(&self) -> Vec<usize> {
        let mut all: Vec<usize> = Vec::new();
        for piece in &self.sources {
            merge_sources(&mut all, piece);
        }
        all
    }

    /// The tags of the piece containing `point`, for an edge running
    /// `start -> end`.
    ///
    /// The piece is located by the point's position ALONG the edge, which is
    /// an ordering question, not a proximity one: no tolerance, radius, or
    /// nearest-feature search participates, so this does not reintroduce the
    /// distance-based ownership R6.1.3 forbids. A point at an interior break
    /// belongs to the piece that ENDS there, so the answer is total and the
    /// two pieces sharing a break never both claim it.
    ///
    /// Returns `None` when the edge is degenerate and has no direction to
    /// order positions along.
    pub fn piece_at(&self, start: Point, end: Point, point: Point) -> Option<&[usize]> {
        let (dx, dy) = (end.x - start.x, end.y - start.y);
        let dd = dx * dx + dy * dy;
        if !dd.is_finite() || dd.partial_cmp(&0.0) != Some(Ordering::Greater) {
            return None;
        }
        let along = |p: Point| ((p.x - start.x) * dx + (p.y - start.y) * dy) / dd;
        let t = along(point);
        for (i, brk) in self.breaks.iter().enumerate() {
            if t <= along(*brk) {
                return self.sources.get(i).map(Vec::as_slice);
            }
        }
        self.sources.last().map(Vec::as_slice)
    }
}

/// A canonical union polygon plus the provenance of each canonical boundary
/// edge. `outer_runs[i]` describes `outer[i] -> outer[(i + 1) % n]`; holes use
/// the same convention.
///
/// This metadata is descriptive only. It makes no ownership or exclusion
/// policy decision, and does not claim exact geometry beyond [`union`]'s
/// existing numerical behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcedPolygon {
    pub polygon: Polygon,
    pub outer_runs: Vec<BoundaryRun>,
    pub hole_runs: Vec<Vec<BoundaryRun>>,
}

impl SourcedPolygon {
    /// Per-outer-edge merged tags, in `outer` order.
    pub fn outer_sources(&self) -> Vec<Vec<usize>> {
        self.outer_runs
            .iter()
            .map(BoundaryRun::merged_sources)
            .collect()
    }

    /// Per-hole, per-edge merged tags, in `holes` order.
    pub fn hole_sources(&self) -> Vec<Vec<Vec<usize>>> {
        self.hole_runs
            .iter()
            .map(|hole| hole.iter().map(BoundaryRun::merged_sources).collect())
            .collect()
    }
}

fn merge_sources(into: &mut Vec<usize>, other: &[usize]) {
    into.extend_from_slice(other);
    into.sort_unstable();
    into.dedup();
}

/// Canonical union with per-edge original input-ring tags.
///
/// Unlike [`union`], this allocates and transports metadata. Rings that
/// enclose no area contribute no tags.
pub fn union_with_sources(rings: &[SourceRing]) -> Vec<SourcedPolygon> {
    let inputs: Vec<InputRing> = rings
        .iter()
        .map(|r| InputRing {
            points: r.points.clone(),
            label: 0,
            tag: r.source,
        })
        .collect();
    let arr = Arrangement::build(&inputs, true);
    let polygons = assemble(arr.boundary(arr.all_half_edges(), |f| arr.winding(f, 0) > 0));
    polygons.iter().map(sourced).collect()
}

/// The union of `rings`, as canonical polygons with holes.
///
/// Each input is treated as a simple closed ring; winding is normalized, so
/// callers need not supply a particular direction. Rings that enclose no area
/// contribute nothing: a degenerate join patch (R3.2.3) is a closed segment
/// through the join node and is always covered by the cross-section it came
/// from, so dropping it changes neither the region nor its boundary.
///
/// Output is deterministic: components are sorted by the R3.2.4 canonical
/// ordering, so the same document always renders the same sequence.
pub fn union(rings: &[Vec<Point>]) -> Vec<Polygon> {
    let arr = Arrangement::build(&plain(rings), false);
    assemble(arr.boundary(arr.all_half_edges(), |f| arr.winding(f, 0) > 0))
        .iter()
        .map(CellPolygon::to_polygon)
        .collect()
}

/// The union of `rings`, AND the bounded components of its complement.
///
/// The second value is what BEHAVIOR-SPEC R5.1.1 calls the rooms of a level:
/// the bounded connected components of `ℝ² ∖ O_L`. It cannot be recovered from
/// the first value alone. A hole of a union component is one such face, but not
/// every face is a hole of a component: four wall clusters meeting only at
/// corner points enclose a void that belongs to no single component, and
/// R5.1.2 is explicit that such a region is a face.
///
/// Both are read off one arrangement. The free region's boundary is the
/// occupied boundary reversed; the unbounded free component traces clockwise,
/// is enclosed by no ring, and is discarded — exactly R5.1.2's rule that the
/// exterior is never a room.
pub fn union_and_complement(rings: &[Vec<Point>]) -> (Vec<Polygon>, Vec<Polygon>) {
    let arr = Arrangement::build(&plain(rings), false);
    let occupied = |f: usize| arr.winding(f, 0) > 0;
    let occ = assemble(arr.boundary(arr.all_half_edges(), occupied));
    let free = assemble(arr.boundary(arr.all_half_edges(), |f| !occupied(f)));
    (
        occ.iter().map(CellPolygon::to_polygon).collect(),
        free.iter().map(CellPolygon::to_polygon).collect(),
    )
}

fn plain(rings: &[Vec<Point>]) -> Vec<InputRing> {
    rings
        .iter()
        .map(|r| InputRing {
            points: r.clone(),
            label: 0,
            tag: 0,
        })
        .collect()
}

/// Regularized polygon difference, using the existing filtered/exact,
/// snap-rounded overlay contract. Holes retain their negative winding.
/// Deterministic canonical components; empty when cuts consume all material.
pub fn difference(polygons: &[Polygon], cuts: &[Polygon]) -> Vec<Polygon> {
    if cuts.is_empty() {
        return polygons.to_vec();
    }
    let rings: Vec<_> = [(polygons, 0), (cuts, 1)]
        .into_iter()
        .flat_map(|(ps, label)| {
            ps.iter().flat_map(move |p| {
                std::iter::once(&p.outer)
                    .chain(p.holes.iter())
                    .map(move |r| InputRing {
                        points: r.clone(),
                        label,
                        tag: 0,
                    })
            })
        })
        .collect();
    let arr = Arrangement::build(&rings, false);
    assemble(arr.boundary(arr.all_half_edges(), |f| {
        arr.winding(f, 0) > 0 && arr.winding(f, 1) == 0
    }))
    .iter()
    .map(CellPolygon::to_polygon)
    .collect()
}

fn runs_of(ring: &CellRing) -> Vec<BoundaryRun> {
    ring.runs
        .iter()
        .map(|(breaks, pieces)| BoundaryRun {
            breaks: to_points(breaks),
            sources: pieces
                .iter()
                .map(|p| {
                    let mut t = Vec::new();
                    merge_sources(&mut t, p);
                    t
                })
                .collect(),
        })
        .collect()
}

fn sourced(p: &CellPolygon) -> SourcedPolygon {
    SourcedPolygon {
        polygon: p.to_polygon(),
        outer_runs: runs_of(&p.outer),
        hole_runs: p.holes.iter().map(runs_of).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::QUANTUM_MM;
    use crate::polygon::signed_double_area;

    const P: fn(f64, f64) -> Point = Point::new;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![P(x0, y0), P(x1, y0), P(x1, y1), P(x0, y1)]
    }

    #[test]
    fn union_of_one_rectangle_is_that_rectangle() {
        let out = union(&[rect(0.0, 0.0, 10.0, 5.0)]);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].outer,
            vec![P(0.0, 0.0), P(10.0, 0.0), P(10.0, 5.0), P(0.0, 5.0)]
        );
        assert!(out[0].holes.is_empty());
    }

    #[test]
    fn sourced_union_matches_plain_union_and_preserves_canonical_edge_tags() {
        let rings = vec![
            SourceRing {
                points: rect(0.0, 0.0, 10.0, 10.0),
                source: 7,
            },
            SourceRing {
                points: rect(10.0, 0.0, 20.0, 10.0),
                source: 42,
            },
        ];
        let sourced = union_with_sources(&rings);
        assert_eq!(
            sourced
                .iter()
                .map(|p| p.polygon.clone())
                .collect::<Vec<_>>(),
            union(&rings.iter().map(|r| r.points.clone()).collect::<Vec<_>>())
        );
        assert_eq!(sourced.len(), 1);
        assert_eq!(
            sourced[0].outer_sources().len(),
            sourced[0].polygon.outer.len()
        );
        assert!(sourced[0].outer_sources().iter().any(|tags| tags == &[7]));
        assert!(sourced[0].outer_sources().iter().any(|tags| tags == &[42]));
    }

    #[test]
    fn sourced_union_merges_duplicate_tags_and_drops_hidden_edges() {
        let rings = vec![
            SourceRing {
                points: rect(0.0, 0.0, 10.0, 10.0),
                source: 9,
            },
            SourceRing {
                points: rect(0.0, 0.0, 10.0, 10.0),
                source: 3,
            },
            SourceRing {
                points: rect(2.0, 2.0, 8.0, 8.0),
                source: 99,
            },
        ];
        let sourced = union_with_sources(&rings);
        assert_eq!(sourced.len(), 1);
        assert!(sourced[0]
            .outer_sources()
            .iter()
            .all(|tags| tags == &[3, 9]));
        assert!(sourced[0]
            .outer_sources()
            .iter()
            .all(|tags| !tags.contains(&99)));
    }

    #[test]
    fn collinear_pieces_keep_their_owners_and_the_break_between_them() {
        // Two unit squares side by side: the union's bottom and top edges each
        // span both inputs. Canonicalization deletes the vertex at x = 1, and
        // that vertex must survive as the break between the two owners, read
        // in opposite order on the two edges.
        let out = union_with_sources(&[
            SourceRing {
                points: rect(0.0, 0.0, 1.0, 1.0),
                source: 50,
            },
            SourceRing {
                points: rect(1.0, 0.0, 2.0, 1.0),
                source: 10,
            },
        ]);
        assert_eq!(out.len(), 1);
        let ring = &out[0].polygon.outer;
        assert_eq!(ring.len(), 4);
        for (i, run) in out[0].outer_runs.iter().enumerate() {
            assert_eq!(run.sources.len(), run.breaks.len() + 1);
            let (start, end) = (ring[i], ring[(i + 1) % ring.len()]);
            if start.y == end.y {
                assert_eq!(run.breaks, vec![P(1.0, start.y)]);
                assert_eq!(run.merged_sources(), vec![10, 50]);
                let left = run.piece_at(start, end, P(0.5, start.y));
                let right = run.piece_at(start, end, P(1.5, start.y));
                assert_eq!(left, Some([50].as_slice()), "edge {i}");
                assert_eq!(right, Some([10].as_slice()), "edge {i}");
            }
        }
    }

    #[test]
    fn union_of_disjoint_rectangles_keeps_two_components() {
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(20.0, 0.0, 30.0, 10.0)]);
        assert_eq!(out.len(), 2, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 100.0);
        assert_eq!(out[1].area_mm2(), 100.0);
    }

    #[test]
    fn union_of_overlapping_rectangles_is_one_component_of_the_right_area() {
        // 10x10 and 10x10 overlapping in a 5x10 band: 100 + 100 - 50 = 150.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(5.0, 0.0, 15.0, 10.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 150.0);
        assert_eq!(
            out[0].outer,
            vec![P(0.0, 0.0), P(15.0, 0.0), P(15.0, 10.0), P(0.0, 10.0)]
        );
    }

    #[test]
    fn union_of_edge_to_edge_rectangles_merges_and_drops_the_shared_edge() {
        // The shared edge carries interior on both sides; if it survived, the
        // result would be two rings sharing a wall instead of one rectangle.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 0.0, 20.0, 10.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(
            out[0].outer,
            vec![P(0.0, 0.0), P(20.0, 0.0), P(20.0, 10.0), P(0.0, 10.0)]
        );
    }

    #[test]
    fn union_of_a_closed_frame_produces_a_hole() {
        // Four bars forming a square annulus: outer 12x12, inner void 8x8.
        let out = union(&[
            rect(0.0, 0.0, 12.0, 2.0),
            rect(0.0, 10.0, 12.0, 12.0),
            rect(0.0, 0.0, 2.0, 12.0),
            rect(10.0, 0.0, 12.0, 12.0),
        ]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].holes.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 144.0 - 64.0);
        // The hole winds clockwise and starts at its least vertex.
        assert_eq!(out[0].holes[0][0], P(2.0, 2.0));
        assert!(signed_double_area(&out[0].holes[0]) < 0.0);
    }

    #[test]
    fn union_output_is_independent_of_input_order() {
        let a = union(&[
            rect(0.0, 0.0, 12.0, 2.0),
            rect(0.0, 10.0, 12.0, 12.0),
            rect(0.0, 0.0, 2.0, 12.0),
            rect(10.0, 0.0, 12.0, 12.0),
        ]);
        let b = union(&[
            rect(10.0, 0.0, 12.0, 12.0),
            rect(0.0, 0.0, 2.0, 12.0),
            rect(0.0, 10.0, 12.0, 12.0),
            rect(0.0, 0.0, 12.0, 2.0),
        ]);
        assert_eq!(a, b);
    }

    #[test]
    fn output_coordinates_do_not_depend_on_input_order_at_the_weld_tolerance() {
        // Two squares whose corners are 0.4 µm apart, in one grid cell, so
        // they are one vertex. Which of the two coordinates represents that vertex
        // must not depend on which square was listed first.
        //
        // Found by adversarial review (Codex, gpt-5.6-sol, 2026-08-31). The
        // representative was previously first-seen, so `union(&[a, b])` and
        // `union(&[b, a])` returned rings differing in a real coordinate, and
        // the existing order-independence test could not see it because its
        // inputs coincided exactly.
        let a = rect(0.0, 0.0, 1000.0, 1000.0);
        let b = rect(1000.0004, 1000.0004, 2000.0, 2000.0);
        assert_eq!(
            union(&[a.clone(), b.clone()]),
            union(&[b, a]),
            "the weld representative must be order-independent"
        );
    }

    #[test]
    fn a_cycle_of_point_touching_triangles_encloses_one_room() {
        // Adversarial review's exact case (Codex, gpt-5.6-sol, 2026-08-31).
        // Four right triangles meet cyclically at (5,0), (10,5), (5,10) and
        // (0,5). The union is four separate components — correct, per R5.1.2's
        // pinch rule — and the diamond they enclose is a bounded component of
        // the complement, so it is a room. It is a hole of NO component.
        let tri = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
            vec![P(a.0, a.1), P(b.0, b.1), P(c.0, c.1)]
        };
        let rings = vec![
            tri((0.0, 0.0), (5.0, 0.0), (0.0, 5.0)),
            tri((5.0, 0.0), (10.0, 0.0), (10.0, 5.0)),
            tri((10.0, 5.0), (10.0, 10.0), (5.0, 10.0)),
            tri((5.0, 10.0), (0.0, 10.0), (0.0, 5.0)),
        ];
        let (occupied, free) = union_and_complement(&rings);
        assert_eq!(occupied.len(), 4, "got {occupied:#?}");
        assert!(
            occupied.iter().all(|c| c.holes.is_empty()),
            "the room is a hole of no component: {occupied:#?}"
        );
        assert_eq!(free.len(), 1, "got {free:#?}");
        assert_eq!(free[0].area_mm2(), 50.0);
        assert_eq!(
            free[0].outer,
            vec![P(0.0, 5.0), P(5.0, 0.0), P(10.0, 5.0), P(5.0, 10.0)]
        );
    }

    #[test]
    fn a_gap_narrower_than_one_weld_is_closed_and_the_error_is_bounded() {
        // Two rectangles 0.4 µm apart: the facing edges round into one cell
        // column, so the union is a single component that includes the gap.
        // What matters is the BOUND — the area wrongly claimed is at most one
        // quantum times the contact length.
        //
        // Raised by adversarial review (Codex, gpt-5.6-sol, 2026-08-31) as a
        // wrong answer. It is a documented consequence; this test pins it so a
        // future change to WELD_MM shows up here rather than as a surprise.
        let out = union(&[
            rect(0.0, 0.0, 1000.0, 1000.0),
            rect(1000.0004, 0.0, 2000.0, 1000.0),
        ]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        let claimed = out[0].area_mm2();
        let real = 1000.0 * 1000.0 + (2000.0 - 1000.0004) * 1000.0;
        let over = claimed - real;
        assert!(over > 0.0, "the gap is claimed as occupied");
        assert!(
            over <= QUANTUM_MM * 1000.0,
            "over-claim {over} exceeds one weld across the contact length"
        );
    }

    #[test]
    fn a_sub_weld_overlap_keeps_the_whole_union_on_either_side_of_a_cell_boundary() {
        // B sits on A's top edge, dipping in by `dy`, either side of a cell
        // boundary. The previous pipeline returned a triangle at 0.6 µm.
        for dy in [0.0004, 0.0006, 0.0014] {
            let out = union(&[
                rect(0.0, 0.0, 4000.0, 100.0),
                rect(1000.0, 100.0 - dy, 3000.0, 300.0),
            ]);
            assert_eq!(out.len(), 1, "dy={dy}: {out:#?}");
            assert!(out[0].holes.is_empty(), "dy={dy}: {out:#?}");
            assert_eq!(out[0].outer.len(), 8, "dy={dy}: {out:#?}");
            let area = out[0].area_mm2();
            assert!(
                (area - 800_000.0).abs() < 1.0,
                "dy={dy}: area {area} is not the two rectangles"
            );
        }
    }

    #[test]
    fn union_is_invariant_under_ring_reversal() {
        // R3.2.4: derived output is invariant under wall reversal.
        let forward = rect(0.0, 0.0, 10.0, 10.0);
        let reversed: Vec<Point> = forward.iter().rev().copied().collect();
        assert_eq!(union(&[forward]), union(&[reversed]));
    }

    #[test]
    fn union_ignores_a_degenerate_ring() {
        let segment = vec![P(0.0, -1.0), P(0.0, 1.0), P(0.0, -1.0)];
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), segment]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].area_mm2(), 100.0);
    }

    #[test]
    fn bounding_box_pruning_keeps_rings_that_only_touch() {
        // Bounding boxes that TOUCH without overlapping: the second rectangle
        // meets the first along PART of its right edge, so that edge must be
        // split at y=2 and y=8 for the shared span to cancel.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 2.0, 20.0, 8.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 100.0 + 60.0);
    }

    #[test]
    fn a_hole_is_charged_to_the_smallest_ring_that_contains_it() {
        // Two nested frames. The inner frame's courtyard lies inside BOTH
        // outer rings; charging it to the outer building would subtract it
        // from the wrong component and leave the inner one overstated.
        let frame = |a: f64, b: f64, t: f64| {
            vec![
                rect(a, a, b, a + t),
                rect(a, b - t, b, b),
                rect(a, a, a + t, b),
                rect(b - t, a, b, b),
            ]
        };
        let mut rings = frame(0.0, 30.0, 4.0);
        rings.extend(frame(8.0, 22.0, 2.0));
        let out = union(&rings);
        assert_eq!(out.len(), 2, "got {out:#?}");

        let outer = &out[0];
        let inner = &out[1];
        assert_eq!(outer.outer[0], P(0.0, 0.0));
        assert_eq!(inner.outer[0], P(8.0, 8.0));
        assert_eq!(outer.holes.len(), 1);
        assert_eq!(inner.holes.len(), 1);
        // Outer frame: 30² less its own 22² courtyard.
        assert_eq!(outer.area_mm2(), 900.0 - 484.0);
        // Inner frame: 14² less its own 10² courtyard.
        assert_eq!(inner.area_mm2(), 196.0 - 100.0);
    }

    #[test]
    fn union_of_nothing_is_nothing() {
        assert!(union(&[]).is_empty());
    }

    #[test]
    fn a_ring_inside_a_hole_stays_a_separate_component() {
        // An island in a courtyard: the frame's hole must NOT swallow it, and
        // the island must not be reported as a hole of the frame.
        let mut rings = vec![
            rect(0.0, 0.0, 20.0, 2.0),
            rect(0.0, 18.0, 20.0, 20.0),
            rect(0.0, 0.0, 2.0, 20.0),
            rect(18.0, 0.0, 20.0, 20.0),
        ];
        rings.push(rect(8.0, 8.0, 12.0, 12.0));
        let out = union(&rings);
        assert_eq!(out.len(), 2, "got {out:#?}");
        // Sorted canonically: the frame starts at (0,0), the island at (8,8).
        assert_eq!(out[0].outer[0], P(0.0, 0.0));
        assert_eq!(out[0].holes.len(), 1);
        assert_eq!(out[1].outer[0], P(8.0, 8.0));
        assert!(out[1].holes.is_empty());
    }

    #[test]
    fn rectangles_touching_at_a_single_point_stay_two_rings() {
        // R5.1.2's pinch: the traversal must split at the shared point rather
        // than producing one figure-eight ring.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 10.0, 20.0, 20.0)]);
        assert_eq!(out.len(), 2, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 100.0);
        assert_eq!(out[1].area_mm2(), 100.0);
    }

    #[test]
    fn union_of_an_l_shape_traces_the_reflex_corner() {
        let out = union(&[rect(0.0, 0.0, 10.0, 4.0), rect(0.0, 0.0, 4.0, 10.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 40.0 + 40.0 - 16.0);
        assert_eq!(
            out[0].outer,
            vec![
                P(0.0, 0.0),
                P(10.0, 0.0),
                P(10.0, 4.0),
                P(4.0, 4.0),
                P(4.0, 10.0),
                P(0.0, 10.0),
            ]
        );
    }
}

#[cfg(test)]
mod regression_0096 {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    }

    /// Issue 0096's first counterexample: the previous pipeline returned
    /// 799.994 mm² here, because a welded fragment's midpoint was classified
    /// against unwelded rings and an unclosed walk was emitted as a ring.
    #[test]
    fn three_offset_rectangles_keep_their_whole_union() {
        let a = rect(10.0006, 30.0006, 20.0006, 70.0006);
        let b = rect(20.0003, 30.0006, 60.0003, 60.0006);
        let c = rect(20.0002, 30.0007, 60.0002, 70.0007);
        for rings in [
            vec![a.clone(), b.clone(), c.clone()],
            vec![c.clone(), b.clone(), a.clone()],
        ] {
            let out = union(&rings);
            let area: f64 = out.iter().map(|p| p.area_mm2()).sum();
            assert!((area - 2000.0).abs() < 0.1, "area {area}: {out:#?}");
        }
        let inside_a = Point::new(15.0006, 50.0006);
        let out = union(&[a, b, c]);
        assert!(out
            .iter()
            .any(|p| crate::arrangement::classify_point(inside_a, &p.outer)
                == crate::arrangement::Containment::Inside));
    }
}
