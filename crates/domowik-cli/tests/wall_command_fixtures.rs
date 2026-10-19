//! BEHAVIOR-SPEC §3.5 fixtures F5–F19 through the product's wall commands
//! (issue 0013, criterion 1): post-state walls and joins, the literal
//! canonical outline (from the full builder AND the retained engine), and
//! undo back to the before-state. F16's outline has exact rational
//! vertices; its expected snap-rounded form is derived here with integer
//! arithmetic, as in `wall_edit_fixtures.rs`.

use domowik_document::session::Session;
use domowik_edit::{
    add_wall_operations, remove_wall_operations, reverse_wall_operations, set_wall_operations,
};
use domowik_format::{Document, Level, Point, Role, Wall};
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::polygon::{canonical_ring, Winding};
use domowik_geometry::walls;
use std::collections::BTreeMap;
use uuid::Uuid;

struct Plan {
    s: Session,
    level: Uuid,
}

impl Plan {
    fn new() -> (Self, Document) {
        let mut doc = Document::new();
        let level = Level::new("L1");
        let id = level.id;
        doc.levels.push(level);
        (
            Self {
                s: Session::open_snapshot(
                    Uuid::now_v7(),
                    Document::new(),
                    &BTreeMap::new(),
                    &[],
                    Vec::new(),
                )
                .expect("opens"),
                level: id,
            },
            doc,
        )
    }

    fn open(doc: Document, level: Uuid) -> Self {
        Self {
            s: Session::open_snapshot(Uuid::now_v7(), doc, &BTreeMap::new(), &[], Vec::new())
                .expect("opens"),
            level,
        }
    }

    fn doc(&self) -> &Document {
        self.s.document()
    }

    fn add(&mut self, a: (f64, f64), b: (f64, f64)) -> Uuid {
        let w = Wall::new(
            self.level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.0,
            2500.0,
        );
        let id = w.id;
        let edit = add_wall_operations(self.doc(), self.level, w).expect("classifies");
        self.s.commit(edit.operations).expect("commits");
        id
    }

    fn axis(&self, id: Uuid) -> ((f64, f64), (f64, f64)) {
        let w = self.doc().wall(id).expect("wall");
        ((w.start().x, w.start().y), (w.end().x, w.end().y))
    }

    /// The single occupied component's canonical outline; full build and
    /// retained engine must agree.
    fn outline(&self) -> Vec<Point> {
        let solids = walls::level_solids(self.doc(), self.level);
        let (occupied, _) = walls::plan(&solids.walls, &solids.joins);
        assert_eq!(LevelGeometry::from_solids(&solids).occupied(), occupied);
        assert_eq!(occupied.len(), 1, "one component: {occupied:?}");
        assert!(occupied[0].holes.is_empty());
        canonical_ring(&occupied[0].outer, Winding::CounterClockwise).expect("ring")
    }

    fn members(&self, at: (f64, f64)) -> Vec<(Uuid, Role)> {
        let p = Point::new(at.0, at.1);
        let join = self
            .doc()
            .joins
            .iter()
            .find(|j| {
                j.members.iter().any(|m| {
                    let w = self.doc().wall(m.wall_id).unwrap();
                    (if m.role == Role::Start {
                        w.start()
                    } else {
                        w.end()
                    }) == p
                })
            })
            .expect("a join at the point");
        join.members.iter().map(|m| (m.wall_id, m.role)).collect()
    }
}

fn ring(points: &[(f64, f64)]) -> Vec<Point> {
    let pts: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
    canonical_ring(&pts, Winding::CounterClockwise).expect("ring")
}

/// F1: W1 (0,0)→(4000,0). F2: plus W2 (4000,0)→(4000,3000) joined at
/// (4000,0).
fn f2() -> (Plan, Uuid, Uuid) {
    let (mut p, doc) = Plan::new();
    let level = p.level;
    p = Plan::open(doc, level);
    let w1 = p.add((0.0, 0.0), (4000.0, 0.0));
    let w2 = p.add((4000.0, 0.0), (4000.0, 3000.0));
    assert_eq!(p.doc().joins.len(), 1, "F2 auto-joins");
    (p, w1, w2)
}

fn f1(len: f64) -> (Plan, Uuid) {
    let (p, doc) = Plan::new();
    let level = p.level;
    let mut p = Plan::open(doc, level);
    let w1 = p.add((0.0, 0.0), (len, 0.0));
    (p, w1)
}

fn has(members: &[(Uuid, Role)], m: (Uuid, Role)) -> bool {
    members.contains(&m)
}

#[test]
fn f5_x_crossing_splits_both_walls() {
    let (mut p, w1) = f1(4000.0);
    let before = p.doc().clone();
    let w2 = p.add((2000.0, -2000.0), (2000.0, 2000.0));
    assert_eq!(p.axis(w1), ((0.0, 0.0), (2000.0, 0.0)));
    assert_eq!(p.axis(w2), ((2000.0, -2000.0), (2000.0, 0.0)));
    let others: Vec<_> = p
        .doc()
        .walls
        .iter()
        .filter(|w| w.id != w1 && w.id != w2)
        .map(|w| p.axis(w.id))
        .collect();
    assert!(others.contains(&((2000.0, 0.0), (4000.0, 0.0))));
    assert!(others.contains(&((2000.0, 0.0), (2000.0, 2000.0))));
    let m = p.members((2000.0, 0.0));
    assert_eq!(m.len(), 4);
    assert!(has(&m, (w1, Role::End)) && has(&m, (w2, Role::End)));
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (1900.0, -100.0),
            (1900.0, -2000.0),
            (2100.0, -2000.0),
            (2100.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (2100.0, 100.0),
            (2100.0, 2000.0),
            (1900.0, 2000.0),
            (1900.0, 100.0),
            (0.0, 100.0),
        ])
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc(), &before);
}

#[test]
fn f6_acute_and_f7_obtuse_joins() {
    let (mut p, _) = f1(3000.0);
    p.add((3000.0, 0.0), (1400.0, 1200.0));
    assert_eq!(p.doc().joins.len(), 1);
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (3000.0, -100.0),
            (3060.0, 80.0),
            (1460.0, 1280.0),
            (1340.0, 1120.0),
            (2700.0, 100.0),
            (0.0, 100.0)
        ])
    );
    let (mut p, _) = f1(3000.0);
    p.add((3000.0, 0.0), (3700.0, 2400.0));
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (3000.0, -100.0),
            (3096.0, -28.0),
            (3796.0, 2372.0),
            (3604.0, 2428.0),
            (2925.0, 100.0),
            (0.0, 100.0)
        ])
    );
}

#[test]
fn f8_reversal_is_neutral_in_plan() {
    let (mut p, w1) = f1(4000.0);
    let before = p.outline();
    let edit = reverse_wall_operations(p.doc(), w1);
    p.s.commit(edit.operations).expect("commits");
    assert_eq!(p.axis(w1), ((4000.0, 0.0), (0.0, 0.0)));
    assert_eq!(p.outline(), before);
    assert_eq!(
        before,
        ring(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (0.0, 100.0)
        ])
    );
}

#[test]
fn f13_parallel_overlap_commits_without_a_join() {
    let (mut p, _) = f1(4000.0);
    p.add((0.0, 50.0), (4000.0, 50.0));
    assert!(p.doc().joins.is_empty());
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 150.0),
            (0.0, 150.0)
        ])
    );
}

#[test]
fn f14_contact_at_a_join_point_merges() {
    let (mut p, w1, w2) = f2();
    let joins = p.doc().joins.len();
    let w3 = p.add((4000.0, 0.0), (5200.0, 900.0));
    assert_eq!(p.doc().joins.len(), joins, "no new join entity");
    let m = p.members((4000.0, 0.0));
    assert_eq!(m.len(), 3);
    assert!(has(&m, (w1, Role::End)) && has(&m, (w2, Role::Start)) && has(&m, (w3, Role::Start)));
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4060.0, -80.0),
            (5260.0, 820.0),
            (5140.0, 980.0),
            (4100.0, 200.0),
            (4100.0, 3000.0),
            (3900.0, 3000.0),
            (3900.0, 100.0),
            (0.0, 100.0),
        ])
    );
}

#[test]
fn f14b_an_interior_through_a_joint_splits_only_the_newcomer() {
    let (mut p, w1, w2) = f2();
    let w3 = p.add((2800.0, -1600.0), (4600.0, 800.0));
    assert_eq!(p.axis(w1), ((0.0, 0.0), (4000.0, 0.0)));
    assert_eq!(p.axis(w2), ((4000.0, 0.0), (4000.0, 3000.0)));
    assert_eq!(p.axis(w3), ((2800.0, -1600.0), (4000.0, 0.0)));
    let m = p.members((4000.0, 0.0));
    assert_eq!(m.len(), 4);
    assert!(has(&m, (w3, Role::End)));
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (3800.0, -100.0),
            (2720.0, -1540.0),
            (2880.0, -1660.0),
            (4680.0, 740.0),
            (4520.0, 860.0),
            (4100.0, 300.0),
            (4100.0, 3000.0),
            (3900.0, 3000.0),
            (3900.0, 100.0),
            (0.0, 100.0),
        ])
    );
}

#[test]
fn f15_double_crossing_chains_and_remaps_deterministically() {
    let (mut p, w1) = f1(6000.0);
    let w2 = p.add((2000.0, -1000.0), (2000.0, 1000.0));
    let b = p
        .doc()
        .walls
        .iter()
        .find(|w| p.axis(w.id) == ((2000.0, 0.0), (6000.0, 0.0)))
        .expect("idB")
        .id;
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (1900.0, -100.0),
            (1900.0, -1000.0),
            (2100.0, -1000.0),
            (2100.0, -100.0),
            (6000.0, -100.0),
            (6000.0, 100.0),
            (2100.0, 100.0),
            (2100.0, 1000.0),
            (1900.0, 1000.0),
            (1900.0, 100.0),
            (0.0, 100.0),
        ])
    );
    let step1 = p.doc().clone();
    let w3 = p.add((4000.0, -1000.0), (4000.0, 1000.0));
    assert_eq!(
        p.axis(b),
        ((2000.0, 0.0), (4000.0, 0.0)),
        "idB keeps its id and start"
    );
    assert_eq!(p.axis(w3), ((4000.0, -1000.0), (4000.0, 0.0)));
    let j1 = p.members((2000.0, 0.0));
    assert!(has(&j1, (w1, Role::End)) && has(&j1, (b, Role::Start)) && has(&j1, (w2, Role::End)));
    let j2 = p.members((4000.0, 0.0));
    assert_eq!(j2.len(), 4);
    assert!(has(&j2, (b, Role::End)) && has(&j2, (w3, Role::End)));
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (1900.0, -100.0),
            (1900.0, -1000.0),
            (2100.0, -1000.0),
            (2100.0, -100.0),
            (3900.0, -100.0),
            (3900.0, -1000.0),
            (4100.0, -1000.0),
            (4100.0, -100.0),
            (6000.0, -100.0),
            (6000.0, 100.0),
            (4100.0, 100.0),
            (4100.0, 1000.0),
            (3900.0, 1000.0),
            (3900.0, 100.0),
            (2100.0, 100.0),
            (2100.0, 1000.0),
            (1900.0, 1000.0),
            (1900.0, 100.0),
            (0.0, 100.0),
        ])
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc(), &step1, "undo restores the step-1 state");
}

/// The cell of the exact rational `p / q` mm, asserted far from a boundary.
fn cell(p: i128, q: i128) -> f64 {
    let twice = 2 * 1024 * p + q;
    let (floor, rem) = (twice.div_euclid(2 * q), twice.rem_euclid(2 * q));
    assert!(
        rem.min(2 * q - rem) * (1 << 20) >= 2 * q,
        "{p}/{q} too close to a cell boundary"
    );
    floor as f64 / 1024.0
}

#[test]
fn f16_joint_drag_moves_the_node_rigidly() {
    let (mut p, w1, w2) = f2();
    let before = p.doc().clone();
    let edit = set_wall_operations(
        p.doc(),
        w1,
        None,
        Some(Point::new(4000.0, 900.0)),
        None,
        None,
    )
    .expect("classifies");
    p.s.commit(edit.operations).expect("commits");
    assert_eq!(p.axis(w1), ((0.0, 0.0), (4000.0, 900.0)));
    assert_eq!(p.axis(w2), ((4000.0, 900.0), (4000.0, 3000.0)));
    let q = 41;
    let v = |x: i128, y: i128| (cell(x, q), cell(y, q));
    assert_eq!(
        p.outline(),
        ring(&[
            v(-900, 4000),
            v(900, -4000),
            v(164_900, 32_900),
            v(4100 * q, 900 * q),
            v(4100 * q, 3000 * q),
            v(3900 * q, 3000 * q),
            v(3900 * q, 980 * q),
        ])
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc(), &before);
}

#[test]
fn f17_body_drag_stretches_the_neighbour_only() {
    let (mut p, w1, w2) = f2();
    let edit = set_wall_operations(
        p.doc(),
        w1,
        Some(Point::new(0.0, 300.0)),
        Some(Point::new(4000.0, 300.0)),
        None,
        None,
    )
    .expect("classifies");
    p.s.commit(edit.operations).expect("commits");
    assert_eq!(p.axis(w1), ((0.0, 300.0), (4000.0, 300.0)));
    assert_eq!(p.axis(w2), ((4000.0, 300.0), (4000.0, 3000.0)));
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, 200.0),
            (4000.0, 200.0),
            (4100.0, 300.0),
            (4100.0, 3000.0),
            (3900.0, 3000.0),
            (3900.0, 400.0),
            (0.0, 400.0)
        ])
    );
}

/// F18: the explicit detach command removes (W2, Start) from J1; J1 drops
/// below two members and is deleted; endpoints do not move; the patch goes
/// and the outline gains the vertex (4000, 0); undo recreates J1.
#[test]
fn f18_explicit_detach() {
    let (mut p, w1, w2) = f2();
    let before = p.doc().clone();
    let j1 = domowik_edit::member_join(p.doc(), w2, Role::Start)
        .expect("joined")
        .id;
    let edit = domowik_edit::detach_operations(p.doc(), j1, w2, Role::Start).expect("detaches");
    assert_eq!(edit.operations.len(), 1, "[delete Join J1]");
    p.s.commit(edit.operations).expect("commits");
    assert!(p.doc().joins.is_empty());
    assert_eq!(p.axis(w1), ((0.0, 0.0), (4000.0, 0.0)));
    assert_eq!(p.axis(w2), ((4000.0, 0.0), (4000.0, 3000.0)));
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 0.0),
            (4100.0, 0.0),
            (4100.0, 3000.0),
            (3900.0, 3000.0),
            (3900.0, 100.0),
            (0.0, 100.0)
        ])
    );
    // Subsequent drags are independent.
    let edit = set_wall_operations(
        p.doc(),
        w2,
        Some(Point::new(4000.0, 500.0)),
        None,
        None,
        None,
    )
    .expect("classifies");
    p.s.commit(edit.operations).expect("commits");
    assert_eq!(p.axis(w1), ((0.0, 0.0), (4000.0, 0.0)), "W1 stays");
    assert_eq!(p.axis(w2).0, (4000.0, 500.0));
    p.s.undo().expect("undo").expect("entry");
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc(), &before, "undo recreates J1");
}

/// Detaching one member of a THREE-member join keeps the join; dragging the
/// detached endpoint afterwards moves it alone and leaves the join — still
/// at the same point — untouched (membership is by (wall, role), not by
/// coincidence).
#[test]
fn detaching_from_a_three_member_join_and_dragging_the_detached_end() {
    let (mut p, w1, w2) = f2();
    let w3 = p.add((4000.0, 0.0), (5200.0, 900.0));
    let j = domowik_edit::member_join(p.doc(), w3, Role::Start)
        .expect("merged into J1")
        .id;
    let edit = domowik_edit::detach_operations(p.doc(), j, w3, Role::Start).expect("detaches");
    p.s.commit(edit.operations).expect("commits");
    let m = p.members((4000.0, 0.0));
    assert_eq!(m.len(), 2, "J1 survives with two members");
    assert!(has(&m, (w1, Role::End)) && has(&m, (w2, Role::Start)));
    assert!(domowik_edit::member_join(p.doc(), w3, Role::Start).is_none());
    // W3's start still coincides with J1's node; dragging it must move W3
    // only.
    let edit = set_wall_operations(
        p.doc(),
        w3,
        Some(Point::new(4200.0, 300.0)),
        None,
        None,
        None,
    )
    .expect("classifies");
    p.s.commit(edit.operations).expect("commits");
    assert_eq!(p.axis(w3).0, (4200.0, 300.0), "the detached end moved");
    assert_eq!(p.axis(w1), ((0.0, 0.0), (4000.0, 0.0)), "the join did not");
    assert_eq!(p.axis(w2), ((4000.0, 0.0), (4000.0, 3000.0)));
    // Detaching something that is not a member is refused.
    assert!(domowik_edit::detach_operations(p.doc(), j, w3, Role::Start).is_err());
}

#[test]
fn f19_deleting_a_wall_drops_its_join() {
    let (mut p, w1, w2) = f2();
    let before = p.doc().clone();
    let edit = remove_wall_operations(p.doc(), w2);
    p.s.commit(edit.operations).expect("commits");
    assert!(p.doc().joins.is_empty());
    assert_eq!(p.axis(w1), ((0.0, 0.0), (4000.0, 0.0)), "W1 untouched");
    assert_eq!(
        p.outline(),
        ring(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (0.0, 100.0)
        ])
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc(), &before);
}

/// `refuse!(p, edit)`: evaluates `edit` (which may read `p`) before
/// borrowing `p` mutably.
macro_rules! refuse {
    ($p:expr, $e:expr $(,)?) => {{
        let edit = $e;
        refuse_impl(&mut $p, edit)
    }};
}

/// A wall batch that must be refused, by the command (`EditError`) or by
/// admission (`CommandError`), leaving the document unchanged; returns the
/// refusal's debug text for matching its variant.
fn refuse_impl(
    p: &mut Plan,
    edit: Result<domowik_edit::WallEdit, domowik_edit::EditError>,
) -> String {
    let before = p.doc().clone();
    let why = match edit {
        Err(e) => format!("{e:?}"),
        Ok(edit) => match p.s.commit(edit.operations) {
            Err(e) => format!("{e:?}"),
            Ok(_) => panic!("the batch was accepted"),
        },
    };
    assert_eq!(p.doc(), &before, "a refused batch changes nothing");
    why
}

#[test]
fn f9_to_f12_refusals() {
    let (mut p, w1) = f1(4000.0);
    let level = p.level;
    let wall = |a: (f64, f64), b: (f64, f64), t: f64, h: f64| {
        Wall::new(level, Point::new(a.0, a.1), Point::new(b.0, b.1), t, h)
    };
    // F9: zero-length-wall.
    let why = refuse!(
        p,
        add_wall_operations(
            p.doc(),
            level,
            wall((1000.0, 1000.0), (1000.0, 1000.0), 200.0, 2500.0),
        ),
    );
    assert!(why.contains("ZeroLengthWall"), "{why}");
    // F10: non-finite end, zero thickness, negative height.
    let why = refuse!(
        p,
        set_wall_operations(
            p.doc(),
            w1,
            None,
            Some(Point::new(f64::NAN, 0.0)),
            None,
            None,
        ),
    );
    assert!(
        why.contains("Coordinate") || why.contains("NonFinite") || why.contains("finite"),
        "{why}"
    );
    let why = refuse!(
        p,
        set_wall_operations(p.doc(), w1, None, None, Some(0.0), None),
    );
    assert!(
        why.contains("DimensionOutOfRange") && why.contains("thickness"),
        "{why}"
    );
    let why = refuse!(
        p,
        set_wall_operations(p.doc(), w1, None, None, None, Some(-5.0)),
    );
    assert!(
        why.contains("DimensionOutOfRange") && why.contains("height"),
        "{why}"
    );
    // F11: collinear-overlap.
    let why = refuse!(
        p,
        add_wall_operations(
            p.doc(),
            level,
            wall((2000.0, 0.0), (6000.0, 0.0), 200.0, 2500.0),
        ),
    );
    assert!(why.contains("CollinearOverlap"), "{why}");
    // F12: wall-coincidence, whatever the thickness.
    let why = refuse!(
        p,
        add_wall_operations(
            p.doc(),
            level,
            wall((0.0, 0.0), (4000.0, 0.0), 150.0, 2500.0),
        ),
    );
    assert!(why.contains("WallCoincidence"), "{why}");
}

/// F20: deleting a host cascades its two openings FIRST, then the wall, in
/// one batch; undo restores the wall before its openings.
#[test]
fn f20_host_deletion_cascades_in_dependency_order() {
    let (mut p, wh) = f1(3000.0);
    let mut ow = domowik_format::Opening::door(wh, 0.5, 800.0, 2100.0, 200.0);
    ow.opens_toward = domowik_format::Side::Right;
    let mut ow2 = domowik_format::Opening::window(wh, 0.5, 600.0, 400.0, 2100.0, 100.0);
    ow2.side = domowik_format::Side::Right;
    let (a, b) = (ow.id, ow2.id);
    p.s.commit(vec![
        domowik_document::operation::Operation::create(domowik_format::Entity::Opening(ow)),
        domowik_document::operation::Operation::create(domowik_format::Entity::Opening(ow2)),
    ])
    .expect("both openings are legal (vertical touch at 2100)");
    let before = p.doc().clone();
    let edit = remove_wall_operations(p.doc(), wh);
    assert_eq!(edit.cascaded_openings, 2);
    let targets: Vec<Uuid> = edit.operations.iter().map(|o| o.entity_id).collect();
    let wall_at = targets.iter().position(|t| *t == wh).expect("deletes Wh");
    for o in [a, b] {
        assert!(
            targets
                .iter()
                .position(|t| *t == o)
                .expect("deletes the opening")
                < wall_at,
            "openings first"
        );
    }
    p.s.commit(edit.operations).expect("commits");
    assert!(p.doc().walls.is_empty() && p.doc().openings.is_empty());
    p.s.undo().expect("undo").expect("entry");
    // The two openings are independent of each other, so the order they
    // come back in is incidental; the wall precedes both.
    let sorted = |d: &Document| {
        let mut d = d.clone();
        d.openings.sort_by_key(|o| o.id);
        d
    };
    assert_eq!(sorted(p.doc()), sorted(&before));
}

/// F21: host edits keep u and revalidate — (a) shorten, (b) reverse,
/// (c) body drag (ONE update), (d) a shorten that breaks fit is refused —
/// each against the same before-state, with the literal cut outline.
#[test]
fn f21_host_edits_keep_u() {
    use domowik_geometry::cuts::level_drawn_region;
    let before_state = || {
        let (mut p, wh) = f1(4000.0);
        let mut ow = domowik_format::Opening::window(wh, 0.3, 1200.0, 1300.0, 900.0, 100.0);
        ow.hinge = domowik_format::Hinge::NearStart;
        ow.opens_toward = domowik_format::Side::Right;
        ow.swing_deg = 60.0;
        let id = ow.id;
        p.s.commit(vec![domowik_document::operation::Operation::create(
            domowik_format::Entity::Opening(ow),
        )])
        .expect("commits");
        (p, wh, id)
    };
    let drawn = |p: &Plan| -> Vec<Vec<Point>> {
        level_drawn_region(p.doc(), p.level)
            .iter()
            .map(|c| canonical_ring(&c.outer, Winding::CounterClockwise).expect("ring"))
            .collect()
    };
    let outline = |dy: f64, len: f64, lo: f64, hi: f64| {
        vec![ring(&[
            (0.0, -100.0 + dy),
            (len, -100.0 + dy),
            (len, 100.0 + dy),
            (hi, 100.0 + dy),
            (hi, dy),
            (lo, dy),
            (lo, 100.0 + dy),
            (0.0, 100.0 + dy),
        ])]
    };
    let u = |p: &Plan, id: Uuid| p.doc().openings.iter().find(|o| o.id == id).unwrap().u;

    // (a)
    let (mut p, wh, id) = before_state();
    let base = p.doc().clone();
    assert_eq!(drawn(&p), outline(0.0, 4000.0, 600.0, 1800.0));
    let edit = set_wall_operations(p.doc(), wh, None, Some(Point::new(3600.0, 0.0)), None, None)
        .expect("classifies");
    p.s.commit(edit.operations).expect("(a) commits");
    assert_eq!(u(&p, id), 0.3);
    assert_eq!(drawn(&p), outline(0.0, 3600.0, 480.0, 1680.0));
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc(), &base);
    // (b)
    let edit = reverse_wall_operations(p.doc(), wh);
    p.s.commit(edit.operations).expect("(b) commits");
    assert!((u(&p, id) - 0.7).abs() < 1e-12);
    assert_eq!(
        drawn(&p),
        outline(0.0, 4000.0, 600.0, 1800.0),
        "outline identical"
    );
    p.s.undo().expect("undo").expect("entry");
    // (c)
    let edit = set_wall_operations(
        p.doc(),
        wh,
        Some(Point::new(0.0, 300.0)),
        Some(Point::new(4000.0, 300.0)),
        None,
        None,
    )
    .expect("classifies");
    let mut targets: Vec<Uuid> = edit.operations.iter().map(|o| o.entity_id).collect();
    targets.dedup();
    assert_eq!(targets, vec![wh], "one wall update, the opening untouched");
    p.s.commit(edit.operations).expect("(c) commits");
    assert_eq!(
        drawn(&p),
        outline(300.0, 4000.0, 600.0, 1800.0),
        "translated rigidly"
    );
    p.s.undo().expect("undo").expect("entry");
    // (d)
    let why = refuse!(
        p,
        set_wall_operations(p.doc(), wh, None, Some(Point::new(1700.0, 0.0)), None, None),
    );
    assert!(why.contains("OpeningDoesNotFitHost"), "{why}");
    assert_eq!(p.doc(), &base);
}

/// F24: identical XY on another level: no contact, no join, each level's
/// region is the F1 band; undo deletes W2.
#[test]
fn f24_cross_level_isolation() {
    let (mut p, _) = f1(4000.0);
    let l1 = p.level;
    let mut doc = p.doc().clone();
    let l2 = Level::new("L2");
    let l2_id = l2.id;
    doc.levels.push(l2);
    p = Plan::open(doc, l1);
    let w2 = Wall::new(
        l2_id,
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2700.0,
    );
    let edit = add_wall_operations(p.doc(), l2_id, w2).expect("no contact across levels");
    assert_eq!(edit.operations.len(), 1, "[create Wall W2]");
    p.s.commit(edit.operations).expect("commits");
    assert!(p.doc().joins.is_empty());
    let band = ring(&[
        (0.0, -100.0),
        (4000.0, -100.0),
        (4000.0, 100.0),
        (0.0, 100.0),
    ]);
    assert_eq!(p.outline(), band);
    let upper = Plan::open(p.doc().clone(), l2_id);
    assert_eq!(upper.outline(), band);
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc().walls.len(), 1);
}

/// Undo of every §3 creation and edit fixture not undone above: F1 create,
/// F2 auto-join, F3 collinear continuation (a degenerate patch, still a
/// join), F4 split, F6/F7 joins, F8 reversal, F14 merge, F14b split through
/// a joint, F17 body drag — each restores the before-state exactly.
#[test]
fn f1_to_f17_undo_restores_the_before_state() {
    type Step = fn(&mut Plan);
    let steps: [(&str, Step); 10] = [
        ("F1", |p| {
            p.add((0.0, 0.0), (4000.0, 0.0));
        }),
        ("F2", |p| {
            p.add((4000.0, 0.0), (4000.0, 3000.0));
            assert_eq!(p.doc().joins.len(), 1);
        }),
        ("F3", |p| {
            p.add((4000.0, 0.0), (8000.0, 0.0));
            assert_eq!(p.doc().joins.len(), 1, "a collinear continuation joins");
        }),
        ("F4", |p| {
            p.add((2000.0, 0.0), (2000.0, 3000.0));
            assert_eq!(p.doc().walls.len(), 3, "W1 split");
        }),
        ("F6", |p| {
            p.add((4000.0, 0.0), (2400.0, 1200.0));
        }),
        ("F7", |p| {
            p.add((4000.0, 0.0), (4700.0, 2400.0));
        }),
        ("F8", |p| {
            let w1 = p.doc().walls[0].id;
            let edit = reverse_wall_operations(p.doc(), w1);
            p.s.commit(edit.operations).expect("commits");
        }),
        ("F14", |p| {
            p.add((4000.0, 0.0), (4000.0, 3000.0));
            p.add((4000.0, 0.0), (5200.0, 900.0));
        }),
        ("F14b", |p| {
            p.add((4000.0, 0.0), (4000.0, 3000.0));
            p.add((2800.0, -1600.0), (4600.0, 800.0));
        }),
        ("F17", |p| {
            p.add((4000.0, 0.0), (4000.0, 3000.0));
            let w1 = p.doc().walls[0].id;
            let edit = set_wall_operations(
                p.doc(),
                w1,
                Some(Point::new(0.0, 300.0)),
                Some(Point::new(4000.0, 300.0)),
                None,
                None,
            )
            .expect("classifies");
            p.s.commit(edit.operations).expect("commits");
        }),
    ];
    for (name, step) in steps {
        let (mut p, _) = f1(4000.0);
        // F1 starts from an empty level; the rest from the F1 state.
        if name == "F1" {
            p.s.undo().expect("undo").expect("entry");
        }
        let before = p.doc().clone();
        let depth = p.s.undo_depth();
        step(&mut p);
        while p.s.undo_depth() > depth {
            p.s.undo().expect("undo").expect("entry");
        }
        assert_eq!(p.doc(), &before, "{name}: undo restores the before-state");
    }
}

/// F17's "no recursion": W2's FAR end is joined to a third wall; dragging
/// W1's body moves only W2's near end, never the far node or W3.
#[test]
fn f17_body_drag_does_not_recurse_through_the_far_join() {
    let (mut p, w1, w2) = f2();
    let w3 = p.add((4000.0, 3000.0), (0.0, 3000.0));
    assert!(
        domowik_edit::member_join(p.doc(), w2, Role::End).is_some(),
        "far end joined"
    );
    let edit = set_wall_operations(
        p.doc(),
        w1,
        Some(Point::new(0.0, 300.0)),
        Some(Point::new(4000.0, 300.0)),
        None,
        None,
    )
    .expect("classifies");
    p.s.commit(edit.operations).expect("commits");
    assert_eq!(
        p.axis(w2),
        ((4000.0, 300.0), (4000.0, 3000.0)),
        "W2 stretches"
    );
    assert_eq!(
        p.axis(w3),
        ((4000.0, 3000.0), (0.0, 3000.0)),
        "W3 untouched"
    );
}
