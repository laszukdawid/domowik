//! The still-applicable cases of the issue-0007 boolean/topology spike
//! corpus, retained as original Rust
//! tests against the current snap-rounded builder (ED-0110) and the
//! retained engine (issue 0013).
//!
//! Each case states its expectation independently: a literal canonical ring
//! derived by hand (with off-grid coordinates rounded to their 2⁻¹⁰ mm cell
//! here, not by the code under test), or an invariance that needs no
//! literal — translation by an exact grid multiple, reversing every wall,
//! permuting input order. Cases already pinned elsewhere are not repeated:
//! L, T and X joins (`walls.rs` F2/F4/F5 tests), the courtyard/island and
//! point pinch (`rooms.rs`, `tests/room_projection.rs`), wall reversal
//! (`walls.rs`). The spike's near-B union (B = 10⁹ mm) lies outside the
//! coordinate domain (`COORD_BOUND_MM` = 10⁷) and is not a supported input;
//! its opening-difference suite is superseded by R5.1.5 (openings are not
//! inputs to the occupied region; §5 F36).

use domowik_format::Point;
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::polygon::Polygon;
use domowik_geometry::walls::{plan, LevelSolids, WallSolid};
use uuid::Uuid;

type Plan = (Vec<Polygon>, Vec<Polygon>);

/// A wall as `(start, end, thickness)`.
type Spec = ((f64, f64), (f64, f64), f64);

fn solids(walls: &[Spec]) -> Vec<WallSolid> {
    walls
        .iter()
        .map(|&(a, b, t)| WallSolid::new(Point::new(a.0, a.1), Point::new(b.0, b.1), t))
        .collect()
}

/// The full build, checked against the retained engine and against every
/// reversal and a reordering of the input: all must give the same plan.
fn build(walls: &[WallSolid]) -> Plan {
    let full = plan(walls, &[]);
    let level = LevelSolids {
        walls: walls.to_vec(),
        ids: walls.iter().map(|_| Uuid::now_v7()).collect(),
        joins: Vec::new(),
    };
    assert_eq!(
        LevelGeometry::from_solids(&level).plan(),
        full,
        "engine == full build"
    );
    let reversed: Vec<WallSolid> = walls
        .iter()
        .map(|w| WallSolid::new(w.end, w.start, w.thickness_mm))
        .collect();
    assert_eq!(plan(&reversed, &[]), full, "wall reversal invariance");
    let mut rotated = walls.to_vec();
    rotated.rotate_left(walls.len() / 2 + 1);
    rotated.reverse();
    assert_eq!(plan(&rotated, &[]), full, "input order invariance");
    full
}

fn ring(points: &[(f64, f64)]) -> Vec<Point> {
    points.iter().map(|&(x, y)| Point::new(x, y)).collect()
}

/// `v` rounded to its 2⁻¹⁰ mm cell centre. Multiplying by 1024 is exact;
/// the test's values are far from half-cell ties.
fn cell(v: f64) -> f64 {
    (v * 1024.0).round() / 1024.0
}

/// A closed loop of four unjoined walls with the given thickness, whose
/// centre lines run through `corners`.
fn square(corners: [(f64, f64); 4], t: f64) -> Vec<Spec> {
    (0..4)
        .map(|i| (corners[i], corners[(i + 1) % 4], t))
        .collect()
}

#[test]
fn disjoint_rooms_are_separate_components() {
    let mut walls = square(
        [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)],
        200.0,
    );
    walls.extend(square(
        [
            (10_000.0, 0.0),
            (13_000.0, 0.0),
            (13_000.0, 2000.0),
            (10_000.0, 2000.0),
        ],
        200.0,
    ));
    let (occupied, rooms) = build(&solids(&walls));
    assert_eq!(occupied.len(), 2);
    assert_eq!(rooms.len(), 2);
    let rings: Vec<&Vec<Point>> = rooms.iter().map(|r| &r.outer).collect();
    assert!(rings.contains(&&ring(&[
        (100.0, 100.0),
        (3900.0, 100.0),
        (3900.0, 2900.0),
        (100.0, 2900.0)
    ])));
    assert!(rings.contains(&&ring(&[
        (10_100.0, 100.0),
        (12_900.0, 100.0),
        (12_900.0, 1900.0),
        (10_100.0, 1900.0)
    ])));
}

#[test]
fn bands_touching_along_an_edge_unite() {
    let walls = solids(&[
        ((0.0, 0.0), (4000.0, 0.0), 200.0),
        ((0.0, 200.0), (4000.0, 200.0), 200.0),
    ]);
    let (occupied, rooms) = build(&walls);
    assert!(rooms.is_empty());
    assert_eq!(occupied.len(), 1, "touching bands are one component");
    assert_eq!(
        occupied[0].outer,
        ring(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 300.0),
            (0.0, 300.0)
        ])
    );
    assert!(occupied[0].holes.is_empty());
}

/// Two 4 m bands overlapping by 0.2 mm (20,000 : 1): one component whose
/// off-grid top edge (199.8 + 100 mm) is rounded to its cell.
#[test]
fn a_sliver_overlap_unites_without_a_gap() {
    let walls = solids(&[
        ((0.0, 0.0), (4000.0, 0.0), 200.0),
        ((0.0, 199.8), (4000.0, 199.8), 200.0),
    ]);
    let (occupied, _) = build(&walls);
    assert_eq!(occupied.len(), 1);
    let top = cell(199.8 + 100.0);
    assert_eq!(
        occupied[0].outer,
        ring(&[(0.0, -100.0), (4000.0, -100.0), (4000.0, top), (0.0, top)])
    );
    assert!(occupied[0].holes.is_empty(), "no sliver hole");
}

/// A room translated by 4·10⁶ mm (an exact multiple of the cell) builds to
/// the translated plan exactly.
#[test]
fn a_large_translation_is_exact() {
    let base = square(
        [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)],
        200.0,
    );
    let d = 4_000_000.0;
    let moved: Vec<_> = base
        .iter()
        .map(|&(a, b, t)| ((a.0 + d, a.1 + d), (b.0 + d, b.1 + d), t))
        .collect();
    let shift = |p: &Plan| -> Plan {
        let s = |poly: &Polygon| Polygon {
            outer: poly
                .outer
                .iter()
                .map(|q| Point::new(q.x + d, q.y + d))
                .collect(),
            holes: poly
                .holes
                .iter()
                .map(|h| h.iter().map(|q| Point::new(q.x + d, q.y + d)).collect())
                .collect(),
        };
        (p.0.iter().map(s).collect(), p.1.iter().map(s).collect())
    };
    assert_eq!(build(&solids(&moved)), shift(&build(&solids(&base))));
}

/// A 45° L: irrational (√2) wall corners, snap-rounded. Invariant under
/// reversal, ordering and the incremental engine; the inner corner seals so
/// the occupied region is one component.
#[test]
fn a_45_degree_l_join_is_invariant() {
    let walls = solids(&[
        ((0.0, 0.0), (3000.0, 3000.0), 200.0),
        ((3000.0, 3000.0), (6000.0, 0.0), 200.0),
    ]);
    let (occupied, rooms) = build(&walls);
    assert_eq!(occupied.len(), 1);
    assert!(rooms.is_empty());
    for p in &occupied[0].outer {
        assert_eq!(cell(p.x), p.x, "vertices are cell centres");
        assert_eq!(cell(p.y), p.y, "vertices are cell centres");
    }
}

/// The spike's non-grid quantization probe: a room whose walls run at
/// 0.03 / 2.03 mm offsets. Every output vertex is a cell centre, and the
/// room is the literal rounded ring.
#[test]
fn off_grid_coordinates_round_to_their_cells() {
    let corners = [
        (0.03, 0.03),
        (2000.03, 0.03),
        (2000.03, 1002.03),
        (0.03, 1002.03),
    ];
    let (occupied, rooms) = build(&solids(&square(corners, 100.0)));
    assert_eq!(occupied.len(), 1);
    assert_eq!(rooms.len(), 1);
    let (lo, hi_x, hi_y) = (
        cell(0.03 + 50.0),
        cell(2000.03 - 50.0),
        cell(1002.03 - 50.0),
    );
    assert_eq!(
        rooms[0].outer,
        ring(&[(lo, lo), (hi_x, lo), (hi_x, hi_y), (lo, hi_y)])
    );
}
