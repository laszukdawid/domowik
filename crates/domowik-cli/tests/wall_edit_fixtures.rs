//! BEHAVIOR-SPEC §3.5 fixtures F22 and F23 through the product's wall-edit
//! commands (issue 0013, criterion 1).
//!
//! F23's outlines have exact rational vertices (denominator 41). Under
//! ED-0110 the derived outline is snap-rounded to 2⁻¹⁰ mm cells, so the
//! expected outline is DERIVED here, independently of the geometry code:
//! each exact rational vertex is rounded to its cell with integer arithmetic,
//! after asserting it is far enough from a cell boundary that the binary64
//! wall corners cannot land in a different cell. The fixture's vertices are
//! at least 32 cells apart, so no edge passes through another vertex's cell
//! and the snap-rounded outline is exactly the rounded vertices.

use domowik_document::operation::Operation;
use domowik_document::session::Session;
use domowik_edit::set_wall_operations;
use domowik_format::{Document, JoinMember, Level, Point, Role, RoomRecord, Wall, WallJoin};
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::polygon::{canonical_ring, Winding};
use domowik_geometry::rooms::{self, RoomStatus};
use domowik_geometry::walls;
use std::collections::BTreeMap;
use uuid::Uuid;

fn session(doc: Document) -> Session {
    Session::open_snapshot(Uuid::now_v7(), doc, &BTreeMap::new(), &[], Vec::new())
        .expect("an empty log over a valid snapshot opens")
}

fn level(doc: &mut Document) -> Uuid {
    let level = Level::new("L1");
    let id = level.id;
    doc.levels.push(level);
    id
}

fn wall(doc: &mut Document, level: Uuid, a: (f64, f64), b: (f64, f64)) -> Uuid {
    let w = Wall::new(
        level,
        Point::new(a.0, a.1),
        Point::new(b.0, b.1),
        200.0,
        2500.0,
    );
    let id = w.id;
    doc.walls.push(w);
    id
}

fn join(doc: &mut Document, members: &[(Uuid, Role)]) {
    doc.joins.push(WallJoin::new(
        members
            .iter()
            .map(|&(w, r)| JoinMember::new(w, r))
            .collect(),
    ));
}

/// The cell of the exact rational `p / q` mm: `floor(v · 1024 + ½)`,
/// computed exactly, after asserting `v` is at least 2⁻²⁰ of a cell from a
/// rounding boundary (binary64 inputs near 10⁴ mm err by ~10⁻¹² mm).
fn cell(p: i128, q: i128) -> i128 {
    assert!(q > 0);
    let twice = 2 * 1024 * p + q; // 2q · (v·1024 + ½)
    let (floor, rem) = (twice.div_euclid(2 * q), twice.rem_euclid(2 * q));
    let margin = rem.min(2 * q - rem);
    assert!(
        margin * (1 << 20) >= 2 * q,
        "{p}/{q} lies too close to a cell boundary for this oracle"
    );
    floor
}

/// The snap-rounded outline of exact rational vertices `(px/q, py/q)`, in
/// canonical form.
fn rounded(vertices: &[(i128, i128)], q: i128) -> Vec<Point> {
    let pts: Vec<Point> = vertices
        .iter()
        .map(|&(x, y)| Point::new(cell(x, q) as f64 / 1024.0, cell(y, q) as f64 / 1024.0))
        .collect();
    canonical_ring(&pts, Winding::CounterClockwise).expect("a ring")
}

/// The level's single occupied polygon, from the full builder AND the
/// retained engine (which must agree), in canonical form.
fn outline(doc: &Document, level: Uuid) -> Vec<Point> {
    let solids = walls::level_solids(doc, level);
    let (occupied, _) = walls::plan(&solids.walls, &solids.joins);
    let engine = LevelGeometry::from_solids(&solids);
    assert_eq!(engine.occupied(), occupied, "engine and full build agree");
    assert_eq!(occupied.len(), 1, "one occupied component");
    assert!(occupied[0].holes.is_empty());
    canonical_ring(&occupied[0].outer, Winding::CounterClockwise).expect("a ring")
}

/// F22: dragging W1's body by (0, −500) stretches W2 and W4, emits exactly
/// the three wall updates and no room operation, and the room grows to the
/// literal ring and 12.54 m²; undo restores it.
#[test]
fn f22_body_drag_inside_a_room_bearing_component() {
    let mut doc = Document::new();
    let l = level(&mut doc);
    let c = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
    let w: Vec<Uuid> = (0..4)
        .map(|i| wall(&mut doc, l, c[i], c[(i + 1) % 4]))
        .collect();
    for i in 0..4 {
        join(
            &mut doc,
            &[(w[i], Role::End), (w[(i + 1) % 4], Role::Start)],
        );
    }
    let r1 = RoomRecord::new(Uuid::from_u128(1), l, Point::new(2000.0, 1500.0));
    doc.room_records.push(r1);
    let mut s = session(doc);
    let before = rooms::project_level(s.document(), l);

    let edit = set_wall_operations(
        s.document(),
        w[0],
        Some(Point::new(0.0, -500.0)),
        Some(Point::new(4000.0, -500.0)),
        None,
        None,
    )
    .expect("the drag classifies");
    // The spec lists one update per entity; the product writes one field
    // operation per field, so W1's start and end are two operations.
    let mut targets: Vec<Uuid> = edit
        .operations
        .iter()
        .map(|o: &Operation| o.entity_id)
        .collect();
    targets.sort();
    targets.dedup();
    let mut expected = vec![w[0], w[1], w[3]];
    expected.sort();
    assert_eq!(targets, expected, "exactly the updates of W1, W2 and W4");
    s.commit(edit.operations).expect("commits");
    let d = s.document();
    assert_eq!(
        (d.wall(w[1]).unwrap().start(), d.wall(w[1]).unwrap().end()),
        (Point::new(4000.0, -500.0), Point::new(4000.0, 3000.0))
    );
    assert_eq!(
        (d.wall(w[3]).unwrap().start(), d.wall(w[3]).unwrap().end()),
        (Point::new(0.0, 3000.0), Point::new(0.0, -500.0))
    );

    let p = rooms::project_level(s.document(), l);
    assert_eq!(p.faces.len(), 1);
    let ring: Vec<Point> = [
        (100.0, -400.0),
        (3900.0, -400.0),
        (3900.0, 2900.0),
        (100.0, 2900.0),
    ]
    .iter()
    .map(|&(x, y)| Point::new(x, y))
    .collect();
    assert_eq!(p.faces[0].outer, ring);
    assert_eq!(
        rooms::display_polygon_area_m2(&p.faces[0]).as_deref(),
        Some("12.54")
    );
    assert_eq!(
        p.record(Uuid::from_u128(1)).unwrap().status,
        RoomStatus::Associated
    );

    s.undo().expect("undo").expect("an entry");
    assert_eq!(rooms::project_level(s.document(), l), before);
}

/// F23(a): moving W1's joined end moves the node; the outline is the
/// snap-rounded image of the fixture's denominator-41 rational outline.
#[test]
fn f23a_a_joined_endpoint_moves_the_node() {
    let mut doc = Document::new();
    let l = level(&mut doc);
    let w1 = wall(&mut doc, l, (0.0, 0.0), (4000.0, 0.0));
    let w2 = wall(&mut doc, l, (4000.0, 0.0), (4000.0, 3000.0));
    join(&mut doc, &[(w1, Role::End), (w2, Role::Start)]);
    let mut s = session(doc);
    let edit = set_wall_operations(
        s.document(),
        w1,
        None,
        Some(Point::new(4000.0, -900.0)),
        None,
        None,
    )
    .expect("classifies");
    let mut targets: Vec<Uuid> = edit.operations.iter().map(|o| o.entity_id).collect();
    targets.sort();
    targets.dedup();
    let mut expected = vec![w1, w2];
    expected.sort();
    assert_eq!(targets, expected, "W1's end and W2's start: the node moves");
    s.commit(edit.operations).expect("commits");
    let d = s.document();
    assert_eq!(d.wall(w2).unwrap().start(), Point::new(4000.0, -900.0));
    assert_eq!(d.joins.len(), 1, "J1 unchanged at the node");

    let q = 41;
    let expected = rounded(
        &[
            (-900, -4000),
            (163_100, -40_900),
            (4100 * q, -900 * q),
            (4100 * q, 3000 * q),
            (3900 * q, 3000 * q),
            (3900 * q, -775 * q),
            (900, 4000),
        ],
        q,
    );
    assert_eq!(outline(s.document(), l), expected);
}

/// F23(b): a free endpoint landing on W2's interior splits W2 — the child
/// holding W2's original start keeps its id — and a NEW three-member join is
/// created; the outline is the snap-rounded rational one.
#[test]
fn f23b_a_free_endpoint_on_an_interior_splits_it() {
    let mut doc = Document::new();
    let l = level(&mut doc);
    let w1 = wall(&mut doc, l, (0.0, 0.0), (4000.0, 0.0));
    let w2 = wall(&mut doc, l, (4000.0, 0.0), (4000.0, 3000.0));
    let mut s = session(doc);
    let edit = set_wall_operations(
        s.document(),
        w1,
        None,
        Some(Point::new(4000.0, 900.0)),
        None,
        None,
    )
    .expect("classifies");
    s.commit(edit.operations).expect("commits");
    let d = s.document();
    assert_eq!(d.walls.len(), 3);
    assert_eq!(
        (d.wall(w1).unwrap().start(), d.wall(w1).unwrap().end()),
        (Point::new(0.0, 0.0), Point::new(4000.0, 900.0))
    );
    assert_eq!(
        (d.wall(w2).unwrap().start(), d.wall(w2).unwrap().end()),
        (Point::new(4000.0, 0.0), Point::new(4000.0, 900.0))
    );
    let g = d
        .walls
        .iter()
        .find(|w| w.id != w1 && w.id != w2)
        .expect("the new child");
    assert_eq!(
        (g.start(), g.end()),
        (Point::new(4000.0, 900.0), Point::new(4000.0, 3000.0))
    );
    assert_eq!(d.joins.len(), 1);
    let members: Vec<(Uuid, Role)> = d.joins[0]
        .members
        .iter()
        .map(|m| (m.wall_id, m.role))
        .collect();
    assert_eq!(members.len(), 3);
    for m in [(w1, Role::End), (w2, Role::End), (g.id, Role::Start)] {
        assert!(members.contains(&m), "{m:?} in {members:?}");
    }

    let q = 41;
    let expected = rounded(
        &[
            (-900, 4000),
            (900, -4000),
            (3900 * q, 775 * q),
            (3900 * q, 0),
            (4100 * q, 0),
            (4100 * q, 3000 * q),
            (3900 * q, 3000 * q),
            (3900 * q, 980 * q),
        ],
        q,
    );
    assert_eq!(outline(s.document(), l), expected);
}

/// F23(c): zero length, zero thickness and negative height each reject and
/// leave the document unchanged.
#[test]
fn f23c_invalid_numeric_edits_reject() {
    let mut doc = Document::new();
    let l = level(&mut doc);
    let w1 = wall(&mut doc, l, (0.0, 0.0), (4000.0, 0.0));
    let mut s = session(doc);
    let before = s.document().clone();
    let attempts = [
        set_wall_operations(
            s.document(),
            w1,
            None,
            Some(Point::new(0.0, 0.0)),
            None,
            None,
        ),
        set_wall_operations(s.document(), w1, None, None, Some(0.0), None),
        set_wall_operations(s.document(), w1, None, None, None, Some(-5.0)),
    ];
    for (i, attempt) in attempts.into_iter().enumerate() {
        let refused = match attempt {
            Err(_) => true,
            Ok(edit) => s.commit(edit.operations).is_err(),
        };
        assert!(refused, "variant {i} must reject");
        assert_eq!(
            s.document(),
            &before,
            "variant {i} left the document unchanged"
        );
    }
}
