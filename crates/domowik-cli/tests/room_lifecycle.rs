//! BEHAVIOR-SPEC §5.4 fixtures F25 and F28–F31 as one wall-driven room
//! lifecycle, through the product's own wall commands and a `Session`
//! (issue 0013, criterion 3).
//!
//! What each step pins, independently of any second implementation:
//! the literal canonical face rings and exact display areas the fixtures
//! state, each record's exclusive status, and R5.2.5 — a wall batch carries
//! ZERO room operations however faces split, merge, vanish or reappear.
//! At every step the retained-engine projection (`project_retained`, the
//! editor's path) must equal the one-shot one (`project_level`), and a save,
//! reopen and rebuild from nothing must reproduce it (derived state is never
//! persisted; criterion 5).

use domowik_container::{read_package, Plan};
use domowik_document::operation::{Operation, OperationKind};
use domowik_document::session::Session;
use domowik_edit::{add_wall_operations, create_room_operations, remove_wall_operations};
use domowik_format::{
    Document, Entity, JoinMember, Level, Point, Role, RoomRecord, Wall, WallJoin,
};
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::rooms::{self, LevelRooms, RoomStatus};
use domowik_geometry::walls;
use std::collections::BTreeMap;
use uuid::Uuid;

const R1: Uuid = Uuid::from_u128(1);
const R2: Uuid = Uuid::from_u128(2);

/// B4: a 4000 × 3000 ring of t = 200 walls with four corner joins. Returns
/// the document, its level and the wall ids `W1..W4` in loop order.
fn b4() -> (Document, Uuid, [Uuid; 4]) {
    let mut doc = Document::new();
    let level = Level::new("L1");
    let level_id = level.id;
    doc.levels.push(level);
    let c = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
    let mut ids = [Uuid::nil(); 4];
    for i in 0..4 {
        let (a, b) = (c[i], c[(i + 1) % 4]);
        let wall = Wall::new(
            level_id,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.0,
            2500.0,
        );
        ids[i] = wall.id;
        doc.walls.push(wall);
    }
    for i in 0..4 {
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(ids[i], Role::End),
            JoinMember::new(ids[(i + 1) % 4], Role::Start),
        ]));
    }
    (doc, level_id, ids)
}

fn session(doc: Document) -> Session {
    Session::open_snapshot(Uuid::now_v7(), doc, &BTreeMap::new(), &[], Vec::new())
        .expect("an empty log over a valid snapshot opens")
}

/// Commits a wall batch, asserting it holds no room operation (R5.2.5).
fn commit_wall_batch(s: &mut Session, ops: Vec<Operation>) {
    let records: Vec<Uuid> = s.document().room_records.iter().map(|r| r.id).collect();
    for op in &ops {
        let creates_room = matches!(
            &op.kind,
            OperationKind::CreateEntity {
                entity: Entity::RoomRecord(_)
            }
        );
        assert!(
            !creates_room && !records.contains(&op.entity_id),
            "a wall batch carries a room operation: {op:?}"
        );
    }
    s.commit(ops).expect("the wall batch commits");
}

/// The projection, checked three ways: retained engine, one-shot, and a
/// rebuild from a saved and reopened package.
fn project(s: &Session, level: Uuid) -> LevelRooms {
    let doc = s.document();
    let one_shot = rooms::project_level(doc, level);
    let mut engine = LevelGeometry::with_provenance();
    engine.sync(&walls::level_solids(doc, level));
    assert_eq!(rooms::project_retained(doc, level, &engine), one_shot);

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    let mut plan = Plan::unsaved(session(doc.clone()));
    plan.save_as(&path).expect("saves");
    let reopened = read_package(&path)
        .expect("reads")
        .project()
        .expect("projects")
        .document;
    assert_eq!(&reopened, doc, "the document round-trips");
    assert_eq!(
        rooms::project_level(&reopened, level),
        one_shot,
        "rebuilt from nothing"
    );
    one_shot
}

fn ring(points: &[(f64, f64)]) -> Vec<Point> {
    points.iter().map(|&(x, y)| Point::new(x, y)).collect()
}

/// The face whose canonical outer ring is `expected`, with its display area.
fn face(p: &LevelRooms, expected: &[(f64, f64)]) -> (usize, String) {
    let expected = ring(expected);
    let i = p
        .faces
        .iter()
        .position(|f| f.outer == expected)
        .unwrap_or_else(|| panic!("no face {expected:?} among {:?}", p.faces));
    assert!(p.faces[i].holes.is_empty());
    (
        i,
        rooms::display_polygon_area_m2(&p.faces[i]).expect("displayable"),
    )
}

fn status(p: &LevelRooms, record: Uuid) -> (RoomStatus, Option<usize>) {
    let r = p.record(record).expect("record present");
    (r.status, r.face)
}

const WHOLE: [(f64, f64); 4] = [
    (100.0, 100.0),
    (3900.0, 100.0),
    (3900.0, 2900.0),
    (100.0, 2900.0),
];
const LEFT: [(f64, f64); 4] = [
    (100.0, 100.0),
    (900.0, 100.0),
    (900.0, 2900.0),
    (100.0, 2900.0),
];
const RIGHT: [(f64, f64); 4] = [
    (1100.0, 100.0),
    (3900.0, 100.0),
    (3900.0, 2900.0),
    (1100.0, 2900.0),
];

/// F25: B4 plus `R1` at (2000, 1500), associated with the whole face.
fn f25() -> (Session, Uuid, [Uuid; 4]) {
    let (doc, level, ids) = b4();
    let mut s = session(doc);
    let mut record = RoomRecord::new(R1, level, Point::new(2000.0, 1500.0));
    record.name = Some("Living".to_owned());
    let ops = create_room_operations(s.document(), record).expect("create");
    assert_eq!(ops.len(), 1, "F25: exactly one operation");
    s.commit(ops).expect("commits");
    let p = project(&s, level);
    assert_eq!(p.faces.len(), 1);
    let (whole, area) = face(&p, &WHOLE);
    assert_eq!(area, "10.64");
    assert_eq!(status(&p, R1), (RoomStatus::Associated, Some(whole)));
    (s, level, ids)
}

/// F28 partition then F29 merge, with undo and redo of the merge.
#[test]
fn f28_split_and_f29_merge_emit_no_room_operations() {
    let (mut s, level, _) = f25();

    // F28: a partition at x = 1000 splits W1 and W3 and the face.
    let p_wall = Wall::new(
        level,
        Point::new(1000.0, 0.0),
        Point::new(1000.0, 3000.0),
        200.0,
        2500.0,
    );
    let p_id = p_wall.id;
    let edit = add_wall_operations(s.document(), level, p_wall).expect("F28 classifies");
    commit_wall_batch(&mut s, edit.operations);
    let p = project(&s, level);
    assert_eq!(p.faces.len(), 2);
    let (left, left_area) = face(&p, &LEFT);
    let (right, right_area) = face(&p, &RIGHT);
    assert_eq!((left_area.as_str(), right_area.as_str()), ("2.24", "7.84"));
    assert_eq!(status(&p, R1), (RoomStatus::Associated, Some(right)));
    assert_eq!(
        p.effective_record(left),
        None,
        "the left face is unrecorded"
    );

    // F29 setup: R2 in the left face; R1 < R2 in the stable-UUID order.
    let mut bath = RoomRecord::new(R2, level, Point::new(500.0, 1500.0));
    bath.name = Some("Bath".to_owned());
    let ops = create_room_operations(s.document(), bath).expect("create R2");
    s.commit(ops).expect("commits");
    let p = project(&s, level);
    let (left, _) = face(&p, &LEFT);
    assert_eq!(status(&p, R2), (RoomStatus::Associated, Some(left)));

    // F29: deleting P merges the faces; R1 wins, R2 is superseded.
    let edit = remove_wall_operations(s.document(), p_id);
    commit_wall_batch(&mut s, edit.operations);
    let records_after_merge = s.document().room_records.clone();
    let p = project(&s, level);
    assert_eq!(p.faces.len(), 1);
    let (whole, area) = face(&p, &WHOLE);
    assert_eq!(area, "10.64");
    assert_eq!(status(&p, R1), (RoomStatus::Associated, Some(whole)));
    assert_eq!(status(&p, R2).0, RoomStatus::Superseded);
    assert_eq!(p.effective_record(whole), Some(R1));

    // Undo: the faces split again and R2 re-associates — pure projection.
    s.undo().expect("undo").expect("an entry");
    let p = project(&s, level);
    let (left, _) = face(&p, &LEFT);
    assert_eq!(status(&p, R2), (RoomStatus::Associated, Some(left)));
    // Redo: superseded again, and the records were never touched.
    s.redo().expect("redo").expect("an entry");
    let p = project(&s, level);
    assert_eq!(status(&p, R2).0, RoomStatus::Superseded);
    assert_eq!(s.document().room_records, records_after_merge);
}

/// F30 disappearance into the exterior, then F31 reappearance through a new
/// wall with NEW joins; undo of F30 re-associates too.
#[test]
fn f30_disappear_and_f31_reappear_emit_no_room_operations() {
    let (mut s, level, ids) = f25();
    let w2 = ids[1];
    let joins_before: Vec<Uuid> = s.document().joins.iter().map(|j| j.id).collect();

    // F30: deleting W2 opens the ring; J2 and J3 fall below two members.
    let edit = remove_wall_operations(s.document(), w2);
    commit_wall_batch(&mut s, edit.operations);
    assert_eq!(s.document().joins.len(), 2, "J2 and J3 are deleted");
    let p = project(&s, level);
    assert!(p.faces.is_empty(), "no bounded face remains");
    assert_eq!(status(&p, R1), (RoomStatus::SeedInExterior, None));

    // Undo of F30: the face reappears and R1 re-associates automatically.
    s.undo().expect("undo").expect("an entry");
    let p = project(&s, level);
    let (whole, _) = face(&p, &WHOLE);
    assert_eq!(status(&p, R1), (RoomStatus::Associated, Some(whole)));
    s.redo().expect("redo").expect("an entry");
    assert_eq!(
        status(&project(&s, level), R1).0,
        RoomStatus::SeedInExterior
    );

    // F31: a new W2' auto-joins at both ends with NEW join identities.
    let w2b = Wall::new(
        level,
        Point::new(4000.0, 0.0),
        Point::new(4000.0, 3000.0),
        200.0,
        2500.0,
    );
    let edit = add_wall_operations(s.document(), level, w2b).expect("F31 classifies");
    commit_wall_batch(&mut s, edit.operations);
    let joins_after: Vec<Uuid> = s.document().joins.iter().map(|j| j.id).collect();
    assert_eq!(joins_after.len(), 4);
    let new_joins: Vec<&Uuid> = joins_after
        .iter()
        .filter(|j| !joins_before.contains(j))
        .collect();
    assert_eq!(
        new_joins.len(),
        2,
        "J2' and J3' are new entities; nothing resurrected"
    );
    let p = project(&s, level);
    let (whole, area) = face(&p, &WHOLE);
    assert_eq!(area, "10.64");
    assert_eq!(status(&p, R1), (RoomStatus::Associated, Some(whole)));

    // Undo of F31 restores the F30 post-state.
    s.undo().expect("undo").expect("an entry");
    let p = project(&s, level);
    assert!(p.faces.is_empty());
    assert_eq!(status(&p, R1).0, RoomStatus::SeedInExterior);
}

/// A closed loop through `corners` with corner joins, `t = 200`.
fn loop_of(doc: &mut Document, level: Uuid, corners: &[(f64, f64)]) -> Vec<Uuid> {
    let n = corners.len();
    let ids: Vec<Uuid> = (0..n)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % n]);
            let wall = Wall::new(
                level,
                Point::new(a.0, a.1),
                Point::new(b.0, b.1),
                200.0,
                2500.0,
            );
            let id = wall.id;
            doc.walls.push(wall);
            id
        })
        .collect();
    for i in 0..n {
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(ids[i], Role::End),
            JoinMember::new(ids[(i + 1) % n], Role::Start),
        ]));
    }
    ids
}

fn record(doc: &mut Document, level: Uuid, id: u128, seed: (f64, f64)) -> Uuid {
    let r = RoomRecord::new(Uuid::from_u128(id), level, Point::new(seed.0, seed.1));
    let rid = r.id;
    doc.room_records.push(r);
    rid
}

/// F26: the L-shaped room's canonical ring, with the J3 patch diamond's
/// room-facing edge sealing the inner corner, and the exact tie 30.245 m²
/// displayed half-to-even as `30.24`.
#[test]
fn f26_l_shaped_room_ring_and_area() {
    let mut doc = Document::new();
    let level = Level::new("L1");
    let level_id = level.id;
    doc.levels.push(level);
    loop_of(
        &mut doc,
        level_id,
        &[
            (0.0, 0.0),
            (8000.0, 0.0),
            (8000.0, 3000.0),
            (3000.0, 3000.0),
            (3000.0, 6000.0),
            (0.0, 6000.0),
        ],
    );
    let r1 = record(&mut doc, level_id, 1, (1500.0, 1500.0));
    let s = session(doc);
    let p = project(&s, level_id);
    assert_eq!(p.faces.len(), 1);
    let (f, area) = face(
        &p,
        &[
            (100.0, 100.0),
            (7900.0, 100.0),
            (7900.0, 2900.0),
            (3000.0, 2900.0),
            (2900.0, 3000.0),
            (2900.0, 5900.0),
            (100.0, 5900.0),
        ],
    );
    assert_eq!(area, "30.24");
    assert_eq!(
        domowik_geometry::exact_area::display_hundredths_m2(&p.faces[f]),
        Some(3024)
    );
    assert_eq!(status(&p, r1), (RoomStatus::Associated, Some(f)));
}

/// F36: creating, editing and deleting an opening on W2 changes no face,
/// ring, area or status, and the batches carry no room operation.
#[test]
fn f36_openings_leave_the_room_projection_unchanged() {
    let (mut s, level, ids) = f25();
    let before = project(&s, level);
    let door = domowik_format::Opening::door(ids[1], 0.5, 900.0, 2100.0, 200.0);
    let door_id = door.id;
    commit_wall_batch(&mut s, vec![Operation::create(Entity::Opening(door))]);
    assert_eq!(project(&s, level), before, "create");
    commit_wall_batch(
        &mut s,
        vec![Operation::set_fields(
            door_id,
            serde_json::Map::from_iter([("u".to_owned(), serde_json::json!(0.3))]),
        )],
    );
    assert_eq!(project(&s, level), before, "edit");
    commit_wall_batch(&mut s, vec![Operation::delete(door_id)]);
    assert_eq!(project(&s, level), before, "delete");
    s.undo().expect("undo").expect("an entry");
    assert_eq!(project(&s, level), before, "undo");
}

/// F39: the F27 courtyard and island plus a remote room, mutated in both
/// directions by body drags. Pins the literal areas, zero room operations,
/// and R5.1.9 locality: the remote room is never rebuilt — checked against
/// the editor's engine audit, not a count.
#[test]
fn f39_nested_and_enclosing_edits_leave_the_remote_room_alone() {
    let mut doc = Document::new();
    let level = Level::new("L1");
    let level_id = level.id;
    doc.levels.push(level);
    let w = loop_of(
        &mut doc,
        level_id,
        &[(0.0, 0.0), (8000.0, 0.0), (8000.0, 6000.0), (0.0, 6000.0)],
    );
    let i = loop_of(
        &mut doc,
        level_id,
        &[
            (2500.0, 2000.0),
            (5500.0, 2000.0),
            (5500.0, 4000.0),
            (2500.0, 4000.0),
        ],
    );
    loop_of(
        &mut doc,
        level_id,
        &[
            (12000.0, 0.0),
            (15000.0, 0.0),
            (15000.0, 2000.0),
            (12000.0, 2000.0),
        ],
    );
    let rc = record(&mut doc, level_id, 1, (1200.0, 3000.0));
    let ri = record(&mut doc, level_id, 2, (4000.0, 3000.0));
    let rd = record(&mut doc, level_id, 3, (13500.0, 1000.0));
    let remote = ring(&[
        (12100.0, 100.0),
        (14900.0, 100.0),
        (14900.0, 1900.0),
        (12100.0, 1900.0),
    ]);
    let mut s = session(doc);

    let displays = |p: &LevelRooms| {
        [rc, ri, rd].map(|r| {
            let face = status(p, r).1.expect("associated");
            rooms::display_polygon_area_m2(&p.faces[face]).expect("displayable")
        })
    };
    let p = project(&s, level_id);
    assert_eq!(displays(&p), ["38.22", "5.04", "5.04"]);
    assert!(p.faces.iter().any(|f| f.outer == remote));

    // A body drag of `wall` by `d`, audited on the editor's engine.
    let drag = |s: &mut Session, wall: Uuid, d: (f64, f64)| {
        let w = s.document().wall(wall).expect("wall").clone();
        let (a, b) = (w.start(), w.end());
        let edit = domowik_edit::set_wall_operations(
            s.document(),
            wall,
            Some(Point::new(a.x + d.0, a.y + d.1)),
            Some(Point::new(b.x + d.0, b.y + d.1)),
            None,
            None,
        )
        .expect("the drag classifies");
        let mut engine = LevelGeometry::with_provenance();
        engine.sync(&walls::level_solids(s.document(), level_id));
        engine.set_audit(true);
        commit_wall_batch(s, edit.operations);
        engine.sync(&walls::level_solids(s.document(), level_id));
        let audit = engine.last_audit().expect("auditing").clone();
        for rebuilt in audit.rooms_before.iter().chain(&audit.rooms_after) {
            assert_ne!(rebuilt, &remote, "the remote room was rebuilt");
        }
        assert!(
            !audit.rooms_after.is_empty(),
            "the edit rebuilt nothing at all"
        );
    };

    // Step 1 (nested → enclosing): I1 down by 300.
    drag(&mut s, i[0], (0.0, -300.0));
    let p = project(&s, level_id);
    let (island, _) = face(
        &p,
        &[
            (2600.0, 1800.0),
            (5400.0, 1800.0),
            (5400.0, 3900.0),
            (2600.0, 3900.0),
        ],
    );
    assert_eq!(status(&p, ri).1, Some(island));
    assert_eq!(displays(&p), ["37.26", "5.88", "5.04"]);

    // Step 2 (enclosing → nested): undo, then W1 down by 500.
    s.undo().expect("undo").expect("an entry");
    assert_eq!(displays(&project(&s, level_id)), ["38.22", "5.04", "5.04"]);
    drag(&mut s, w[0], (0.0, -500.0));
    let p = project(&s, level_id);
    assert_eq!(displays(&p), ["42.12", "5.04", "5.04"]);
    s.undo().expect("undo").expect("an entry");
    assert_eq!(displays(&project(&s, level_id)), ["38.22", "5.04", "5.04"]);
}

/// F25 in full: one creation operation, the label anchored at the seed with
/// the record's name, and undo deleting the record while the face stays.
#[test]
fn f25_label_and_undo() {
    let (mut s, level, _) = f25();
    let p = project(&s, level);
    let (face_i, _) = face(&p, &WHOLE);
    let effective = p.effective_record(face_i).expect("labelled");
    let record = s.document().room_record(effective).expect("record");
    assert_eq!((effective, record.display_name()), (R1, "Living"));
    assert_eq!(record.seed(), Point::new(2000.0, 1500.0), "label anchor");
    let before_faces = p.faces.clone();
    s.undo().expect("undo").expect("an entry");
    assert!(s.document().room_records.is_empty());
    assert_eq!(
        project(&s, level).faces,
        before_faces,
        "the face is unchanged"
    );
}

/// F33: a second seed in the same face; one creation effect, R1 keeps the
/// label, R2 is superseded; undo removes R2 only.
#[test]
fn f33_two_seeds_one_face() {
    let (mut s, level, _) = f25();
    let mut study = RoomRecord::new(R2, level, Point::new(3000.0, 1500.0));
    study.name = Some("Study".to_owned());
    let ops = create_room_operations(s.document(), study).expect("create");
    assert_eq!(ops.len(), 1, "no other effects");
    s.commit(ops).expect("commits");
    let p = project(&s, level);
    let (whole, _) = face(&p, &WHOLE);
    assert_eq!(status(&p, R1), (RoomStatus::Associated, Some(whole)));
    assert_eq!(status(&p, R2).0, RoomStatus::Superseded);
    assert_eq!(p.effective_record(whole), Some(R1));
    s.undo().expect("undo").expect("an entry");
    let p = project(&s, level);
    assert!(p.record(R2).is_none());
    assert_eq!(status(&p, R1).0, RoomStatus::Associated);
}

/// F27's literal rings: the courtyard's outer ring, its clockwise
/// eight-vertex chamfered hole, and the island.
#[test]
fn f27_courtyard_and_island_rings() {
    let mut doc = Document::new();
    let level = Level::new("L1");
    let level_id = level.id;
    doc.levels.push(level);
    loop_of(
        &mut doc,
        level_id,
        &[(0.0, 0.0), (8000.0, 0.0), (8000.0, 6000.0), (0.0, 6000.0)],
    );
    loop_of(
        &mut doc,
        level_id,
        &[
            (2500.0, 2000.0),
            (5500.0, 2000.0),
            (5500.0, 4000.0),
            (2500.0, 4000.0),
        ],
    );
    let p = project(&session(doc), level_id);
    assert_eq!(p.faces.len(), 2);
    let courtyard = p
        .faces
        .iter()
        .find(|f| {
            f.outer
                == ring(&[
                    (100.0, 100.0),
                    (7900.0, 100.0),
                    (7900.0, 5900.0),
                    (100.0, 5900.0),
                ])
        })
        .expect("courtyard");
    assert_eq!(
        courtyard.holes,
        vec![ring(&[
            (2400.0, 2000.0),
            (2400.0, 4000.0),
            (2500.0, 4100.0),
            (5500.0, 4100.0),
            (5600.0, 4000.0),
            (5600.0, 2000.0),
            (5500.0, 1900.0),
            (2500.0, 1900.0),
        ])]
    );
    assert_eq!(
        rooms::display_polygon_area_m2(courtyard).as_deref(),
        Some("38.22")
    );
    let (_, island) = face(
        &p,
        &[
            (2600.0, 2100.0),
            (5400.0, 2100.0),
            (5400.0, 3900.0),
            (2600.0, 3900.0),
        ],
    );
    assert_eq!(island, "5.04");
}

/// F35 through the product's wall command: drawing loop B onto loop A's
/// corner merges the contact into ONE four-member join K; the two lobes
/// stay distinct rooms of 3.24 m² each.
#[test]
fn f35_point_pinch_lobes_stay_distinct() {
    let mut doc = Document::new();
    let level = Level::new("L1");
    let level_id = level.id;
    doc.levels.push(level);
    let mut s = session(doc);
    let draw = |s: &mut Session, corners: &[(f64, f64)]| {
        for i in 0..corners.len() {
            let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
            let w = Wall::new(
                level_id,
                Point::new(a.0, a.1),
                Point::new(b.0, b.1),
                200.0,
                2500.0,
            );
            let edit = add_wall_operations(s.document(), level_id, w).expect("classifies");
            commit_wall_batch(s, edit.operations);
        }
    };
    draw(
        &mut s,
        &[(0.0, 0.0), (2000.0, 0.0), (2000.0, 2000.0), (0.0, 2000.0)],
    );
    draw(
        &mut s,
        &[
            (2000.0, 2000.0),
            (4000.0, 2000.0),
            (4000.0, 4000.0),
            (2000.0, 4000.0),
        ],
    );
    let pinch = Point::new(2000.0, 2000.0);
    let k: Vec<&WallJoin> = s
        .document()
        .joins
        .iter()
        .filter(|j| {
            j.members.iter().any(|m| {
                let w = s.document().wall(m.wall_id).unwrap();
                (if m.role == Role::Start {
                    w.start()
                } else {
                    w.end()
                }) == pinch
            })
        })
        .collect();
    assert_eq!(k.len(), 1, "one join at the pinch");
    assert_eq!(k[0].members.len(), 4, "four members");

    let mut doc = s.document().clone();
    let ra = record(&mut doc, level_id, 1, (1000.0, 1000.0));
    let rb = record(&mut doc, level_id, 2, (3000.0, 3000.0));
    let p = project(&session(doc), level_id);
    assert_eq!(p.faces.len(), 2);
    let (a, a_area) = face(
        &p,
        &[
            (100.0, 100.0),
            (1900.0, 100.0),
            (1900.0, 1900.0),
            (100.0, 1900.0),
        ],
    );
    let (b, b_area) = face(
        &p,
        &[
            (2100.0, 2100.0),
            (3900.0, 2100.0),
            (3900.0, 3900.0),
            (2100.0, 3900.0),
        ],
    );
    assert_eq!((a_area.as_str(), b_area.as_str()), ("3.24", "3.24"));
    assert_eq!(status(&p, ra), (RoomStatus::Associated, Some(a)));
    assert_eq!(status(&p, rb), (RoomStatus::Associated, Some(b)));
}

fn update(s: &mut Session, fields: domowik_edit::RoomFields) {
    let ops = domowik_edit::update_room_operations(s.document(), R1, &fields).expect("update");
    s.commit(ops).expect("commits");
}

/// F37 (b) seed moves within the face, (c) out of it, (e)/(f) style set →
/// change → clear with undo in order, (i) a move to another level and back
/// in one undo boundary. Faces never change.
#[test]
fn f37_room_metadata_edit_vectors() {
    use domowik_edit::{FieldEdit, RoomFields};
    let (mut s, level, _) = f25();
    let faces = project(&s, level).faces;

    // (b)
    update(
        &mut s,
        RoomFields {
            seed: Some(Point::new(3500.0, 2500.0)),
            ..Default::default()
        },
    );
    let p = project(&s, level);
    assert_eq!(status(&p, R1).0, RoomStatus::Associated);
    assert_eq!(
        s.document().room_record(R1).unwrap().seed(),
        Point::new(3500.0, 2500.0),
        "label anchor"
    );
    s.undo().expect("undo").expect("entry");
    assert_eq!(
        s.document().room_record(R1).unwrap().seed(),
        Point::new(2000.0, 1500.0)
    );

    // (c)
    update(
        &mut s,
        RoomFields {
            seed: Some(Point::new(10000.0, 1500.0)),
            ..Default::default()
        },
    );
    let p = project(&s, level);
    assert_eq!(status(&p, R1), (RoomStatus::SeedInExterior, None));
    assert_eq!(p.effective_record(0), None, "the label disappears");
    s.undo().expect("undo").expect("entry");
    assert_eq!(status(&project(&s, level), R1).0, RoomStatus::Associated);

    // (e) and (f): set, change, clear; undo in order.
    for (field, values) in [
        ("floor", ["F-wood", "F-tile"]),
        ("ceiling", ["C-white", "C-black"]),
    ] {
        let edit = |v: FieldEdit<String>| {
            if field == "floor" {
                RoomFields {
                    floor_style_ref: v,
                    ..Default::default()
                }
            } else {
                RoomFields {
                    ceiling_style_ref: v,
                    ..Default::default()
                }
            }
        };
        let get = |s: &Session| {
            let r = s.document().room_record(R1).unwrap();
            if field == "floor" {
                r.floor_style_ref.clone()
            } else {
                r.ceiling_style_ref.clone()
            }
        };
        update(&mut s, edit(FieldEdit::Set(values[0].to_owned())));
        update(&mut s, edit(FieldEdit::Set(values[1].to_owned())));
        update(&mut s, edit(FieldEdit::Clear));
        assert_eq!(get(&s), None);
        s.undo().expect("undo").expect("entry");
        assert_eq!(get(&s).as_deref(), Some(values[1]));
        s.undo().expect("undo").expect("entry");
        assert_eq!(get(&s).as_deref(), Some(values[0]));
        s.undo().expect("undo").expect("entry");
        assert_eq!(get(&s), None, "{field}: absent again");
        assert_eq!(project(&s, level).faces, faces, "faces never change");
    }

    // (i) level move in one batch, and undo in one boundary.
    let l2 = Level::new("L2");
    let l2_id = l2.id;
    s.commit(vec![Operation::create(Entity::Level(l2))])
        .expect("L2");
    update(
        &mut s,
        RoomFields {
            level_id: Some(l2_id),
            ..Default::default()
        },
    );
    let on_l2 = project(&s, l2_id);
    assert_eq!(
        status(&on_l2, R1),
        (RoomStatus::SeedInExterior, None),
        "no faces on L2"
    );
    let on_l1 = project(&s, level);
    assert_eq!(on_l1.faces, faces, "L1's faces unchanged");
    assert!(
        on_l1.record(R1).is_none() && on_l1.effective_record(0).is_none(),
        "L1 loses its label"
    );
    s.undo().expect("undo").expect("entry");
    assert_eq!(
        status(&project(&s, level), R1).0,
        RoomStatus::Associated,
        "both projections restored"
    );
    assert!(project(&s, l2_id).record(R1).is_none());
}

/// F38: deleting L1 is refused listing Room 1, Wall 4, Join 4; deleting the
/// empty L2 succeeds and touches no room data.
#[test]
fn f38_level_deletion_blocked_by_a_record() {
    use domowik_document::replay::LevelDeletionBlock;
    use domowik_document::session::CommandError;
    let (mut s, level, _) = f25();
    let l2 = Level::new("L2");
    let l2_id = l2.id;
    s.commit(vec![Operation::create(Entity::Level(l2))])
        .expect("L2");
    let before = s.document().clone();
    let err = s
        .commit(vec![Operation::delete(level)])
        .expect_err("refused");
    let CommandError::LevelDeletion(blocks) = err else {
        panic!("expected a level-deletion refusal, got {err:?}");
    };
    let count = |kind: &str| {
        blocks.iter().find_map(|b| match b {
            LevelDeletionBlock::Occupied { kind: k, count, .. } if k.starts_with(kind) => {
                Some(*count)
            }
            _ => None,
        })
    };
    assert_eq!(
        (count("room"), count("wall"), count("join")),
        (Some(1), Some(4), Some(4)),
        "{blocks:?}"
    );
    assert_eq!(s.document(), &before, "state unchanged");
    s.commit(vec![Operation::delete(l2_id)])
        .expect("the empty level deletes");
    assert_eq!(s.document().room_records, before.room_records);
}

/// F28 undo: the inverse batch restores the F25 projection exactly; R1
/// re-spans the single face.
#[test]
fn f28_undo_restores_the_single_face() {
    let (mut s, level, _) = f25();
    let before = project(&s, level);
    let p_wall = Wall::new(
        level,
        Point::new(1000.0, 0.0),
        Point::new(1000.0, 3000.0),
        200.0,
        2500.0,
    );
    let edit = add_wall_operations(s.document(), level, p_wall).expect("F28 classifies");
    commit_wall_batch(&mut s, edit.operations);
    assert_eq!(project(&s, level).faces.len(), 2);
    s.undo().expect("undo").expect("an entry");
    assert_eq!(project(&s, level), before);
}
