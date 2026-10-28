use domowik_document::{operation::Operation, session::Session};
use domowik_edit::appearance;
use domowik_format::{
    appearance::{wall_color, RgbColor},
    Document, Point, Wall,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn door_color_and_style_are_one_undoable_edit_and_reset_independently() {
    use domowik_format::{
        appearance::{door_color, door_style, DoorStyle},
        Opening,
    };
    let mut doc = Document::with_ground_floor();
    let wall = Wall::new(
        doc.levels[0].id,
        Point::new(0., 0.),
        Point::new(4000., 0.),
        120.,
        2500.,
    );
    let mut door = Opening::door(wall.id, 0.3, 900., 2100., 120.);
    door.unknown
        .insert("future".into(), json!({"preserve":true}));
    door.style_refs.push("opaque-token".into());
    let id = door.id;
    let window = Opening::window(wall.id, 0.8, 600., 1000., 900., 120.);
    let window_id = window.id;
    doc.walls.push(wall);
    doc.openings.extend([door, window]);
    let mut session = Session::open_snapshot(
        Uuid::now_v7(),
        doc.clone(),
        &Default::default(),
        &[],
        vec![],
    )
    .unwrap();
    let color = Some(RgbColor([220, 200, 180]));
    let ops = appearance::door(session.document(), id, color, DoorStyle::Glass).unwrap();
    assert_eq!(ops.len(), 1);
    session.commit(ops).unwrap();
    assert_eq!(session.undo_depth(), 1);
    assert!(
        appearance::door(session.document(), id, color, DoorStyle::Glass)
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.document().walls, doc.walls);
    assert_eq!(session.document().openings[1], doc.openings[1]);
    let changed = session.document().clone();
    assert_eq!(changed.openings[0].style_refs, doc.openings[0].style_refs);
    assert_eq!(
        changed.openings[0].unknown["future"],
        json!({"preserve":true})
    );
    session.undo().unwrap();
    assert_eq!(session.document(), &doc);
    session.redo().unwrap();
    assert_eq!(session.document(), &changed);
    assert!(appearance::door(session.document(), window_id, color, DoorStyle::Glass).is_err());
    assert!(appearance::door(session.document(), Uuid::nil(), color, DoorStyle::Glass).is_err());
    session
        .commit(appearance::door(session.document(), id, None, DoorStyle::Glass).unwrap())
        .unwrap();
    assert_eq!(door_color(&session.document().openings[0]).unwrap(), None);
    assert_eq!(
        door_style(&session.document().openings[0]).unwrap(),
        DoorStyle::Glass
    );
    session
        .commit(appearance::door(session.document(), id, color, DoorStyle::Solid).unwrap())
        .unwrap();
    assert_eq!(door_color(&session.document().openings[0]).unwrap(), color);
    assert!(!session.document().openings[0]
        .unknown
        .contains_key("doorStyle"));
    session
        .commit(appearance::door(session.document(), id, None, DoorStyle::Solid).unwrap())
        .unwrap();
    assert_eq!(session.document(), &doc);
    for (field, value) in [("color", json!("bad")), ("doorStyle", json!("bad"))] {
        let op = Operation::set_fields(id, serde_json::Map::from_iter([(field.into(), value)]));
        assert!(session.commit(vec![op]).is_err());
        assert_eq!(session.document(), &doc);
    }
}

#[test]
fn bulk_colors_are_atomic_undoable_noop_aware_and_preserve_unknown_fields() {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    for y in [0., 1000.] {
        let mut wall = Wall::new(level, Point::new(0., y), Point::new(3000., y), 120., 2500.);
        wall.unknown
            .insert("futureFinish".into(), json!({"keep":true}));
        doc.walls.push(wall);
    }
    let ids: Vec<_> = doc.walls.iter().map(|w| w.id).collect();
    let mut s = Session::open_snapshot(
        Uuid::now_v7(),
        doc.clone(),
        &Default::default(),
        &[],
        vec![],
    )
    .unwrap();
    let color = Some(RgbColor::parse("#D8CDB7").unwrap());
    s.commit(appearance::walls(s.document(), &ids, None, color).unwrap())
        .unwrap();
    assert_eq!(s.undo_depth(), 1);
    assert!(appearance::walls(s.document(), &ids, None, color)
        .unwrap()
        .is_empty());
    for w in &s.document().walls {
        assert_eq!(wall_color(w).unwrap(), color);
        assert_eq!(w.unknown["futureFinish"], json!({"keep":true}));
    }
    assert!(appearance::walls(s.document(), &[ids[0], Uuid::nil()], None, color).is_err());
    assert!(appearance::walls(s.document(), &[ids[0], ids[0]], None, color).is_err());
    s.undo().unwrap();
    assert_eq!(s.document(), &doc);
    s.redo().unwrap();
    let serialized = serde_json::to_value(s.document()).unwrap();
    assert_eq!(serialized["walls"][0]["color"], "#D8CDB7");
    s.commit(appearance::walls(s.document(), &ids, None, None).unwrap())
        .unwrap();
    assert!(s
        .document()
        .walls
        .iter()
        .all(|w| !w.unknown.contains_key("color")));
    let bad = Operation::set_fields(
        ids[0],
        serde_json::Map::from_iter([("color".into(), json!("#bad"))]),
    );
    assert!(s.commit(vec![bad]).is_err());
    assert_eq!(s.document(), &doc);
}

#[test]
fn colored_wall_splits_inherit_the_optional_field_without_reinterpreting_styles() {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let mut parent = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(4000., 0.),
        120.,
        2500.,
    );
    parent.unknown.insert("color".into(), json!("#D8CDB7"));
    parent.style_refs.push("opaque-style".into());
    doc.walls.push(parent);
    let branch = Wall::new(
        level,
        Point::new(2000., 0.),
        Point::new(2000., 2000.),
        120.,
        2500.,
    );
    let ops = domowik_edit::add_wall_operations(&doc, level, branch)
        .unwrap()
        .operations;
    let mut session =
        Session::open_snapshot(Uuid::now_v7(), doc, &Default::default(), &[], vec![]).unwrap();
    session.commit(ops).unwrap();
    let children: Vec<_> = session
        .document()
        .walls
        .iter()
        .filter(|w| w.style_refs == ["opaque-style"])
        .collect();
    assert_eq!(children.len(), 2);
    assert!(children
        .iter()
        .all(|w| wall_color(w).unwrap() == Some(RgbColor::parse("#D8CDB7").unwrap())));
}

/// Two rooms, 0..2000 and 2000..4000 by 0..3000, sharing the partition at
/// x = 2000 drawn bottom-to-top (its Left face looks at the west room).
fn two_rooms() -> (Session, [Uuid; 2], Uuid) {
    let doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let mut s =
        Session::open_snapshot(Uuid::now_v7(), doc, &Default::default(), &[], vec![]).unwrap();
    let corners = [(0., 0.), (4000., 0.), (4000., 3000.), (0., 3000.), (0., 0.)];
    for pair in corners.windows(2) {
        let w = Wall::new(
            level,
            Point::new(pair[0].0, pair[0].1),
            Point::new(pair[1].0, pair[1].1),
            120.,
            2500.,
        );
        let ops = domowik_edit::add_wall_operations(s.document(), level, w)
            .unwrap()
            .operations;
        s.commit(ops).unwrap();
    }
    let partition = Wall::new(
        level,
        Point::new(2000., 0.),
        Point::new(2000., 3000.),
        120.,
        2500.,
    );
    let partition_id = partition.id;
    let ops = domowik_edit::add_wall_operations(s.document(), level, partition)
        .unwrap()
        .operations;
    s.commit(ops).unwrap();
    let rooms = [Uuid::now_v7(), Uuid::now_v7()];
    let mut doc = s.document().clone();
    for (id, x) in rooms.iter().zip([1000., 3000.]) {
        doc.room_records.push(domowik_format::RoomRecord::new(
            *id,
            level,
            Point::new(x, 1500.),
        ));
    }
    let s = Session::open_snapshot(Uuid::now_v7(), doc, &Default::default(), &[], vec![]).unwrap();
    (s, rooms, partition_id)
}

#[test]
fn one_face_keeps_the_other_and_equal_faces_collapse_to_color() {
    use domowik_format::{appearance::wall_face_color, Side};
    let (mut s, _, id) = two_rooms();
    let green = RgbColor::parse("#779383").ok();
    let red = RgbColor::parse("#B77964").ok();
    s.commit(appearance::walls(s.document(), &[id], None, green).unwrap())
        .unwrap();
    s.commit(appearance::walls(s.document(), &[id], Some(Side::Left), red).unwrap())
        .unwrap();
    let w = s.document().wall(id).unwrap();
    assert_eq!(wall_face_color(w, Side::Left).unwrap(), red);
    assert_eq!(wall_face_color(w, Side::Right).unwrap(), green);
    assert!(
        !w.unknown.contains_key("color"),
        "differing faces store sides only"
    );
    assert!(
        appearance::walls(s.document(), &[id], Some(Side::Left), red)
            .unwrap()
            .is_empty()
    );
    s.commit(appearance::wall_faces(s.document(), id, [green, green]).unwrap())
        .unwrap();
    let w = s.document().wall(id).unwrap();
    assert_eq!(w.unknown["color"], json!("#779383"));
    assert!(!w.unknown.contains_key("colorLeft") && !w.unknown.contains_key("colorRight"));
    s.commit(appearance::wall_faces(s.document(), id, [None, red]).unwrap())
        .unwrap();
    let w = s.document().wall(id).unwrap();
    assert_eq!(wall_face_color(w, Side::Left).unwrap(), None);
    assert_eq!(wall_face_color(w, Side::Right).unwrap(), red);
    s.undo().unwrap();
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(
        wall_face_color(s.document().wall(id).unwrap(), Side::Left).unwrap(),
        green
    );
}

#[test]
fn painting_a_room_colors_only_faces_looking_into_it() {
    use domowik_format::appearance::wall_faces;
    let (mut s, [west, east], partition) = two_rooms();
    let blue = RgbColor::parse("#90A4AE").ok();
    let ops = appearance::room(s.document(), west, blue).unwrap();
    s.commit(ops).unwrap();
    assert_eq!(s.undo_depth(), 1);
    assert_eq!(
        wall_faces(s.document().wall(partition).unwrap()),
        [blue, None]
    );
    // Every outer wall runs counter-clockwise, so its Left face is inside.
    // The bottom and top walls were split by the partition; only their west
    // halves face the west room.
    let painted: Vec<_> = s
        .document()
        .walls
        .iter()
        .filter(|w| wall_faces(w) != [None, None])
        .collect();
    assert_eq!(painted.len(), 4, "three west walls and the partition");
    for w in &painted {
        let mid = (w.start().x + w.end().x) / 2.;
        assert!(mid <= 2000., "{:?}", w.start());
        if w.id != partition {
            assert_eq!(wall_faces(w), [blue, None]);
        }
    }
    assert!(appearance::room(s.document(), west, blue)
        .unwrap()
        .is_empty());
    let ops = appearance::room(s.document(), east, blue).unwrap();
    s.commit(ops).unwrap();
    let p = s.document().wall(partition).unwrap();
    assert_eq!(p.unknown["color"], json!("#90A4AE"), "both faces now agree");
    assert!(appearance::room(s.document(), Uuid::nil(), blue).is_err());
}

#[test]
fn reversal_keeps_each_finish_on_its_physical_face_and_splits_copy_both() {
    use domowik_format::appearance::wall_faces;
    let (mut s, [west, _], partition) = two_rooms();
    let blue = RgbColor::parse("#90A4AE").ok();
    let ops = appearance::room(s.document(), west, blue).unwrap();
    s.commit(ops).unwrap();
    let ops = domowik_edit::reverse_wall_operations(s.document(), partition).operations;
    s.commit(ops).unwrap();
    assert_eq!(
        wall_faces(s.document().wall(partition).unwrap()),
        [None, blue]
    );
    // Re-painting the west room after reversal is a no-op.
    assert!(appearance::room(s.document(), west, blue)
        .unwrap()
        .is_empty());
    let level = s.document().levels[0].id;
    let branch = Wall::new(
        level,
        Point::new(2000., 1500.),
        Point::new(3000., 1500.),
        120.,
        2500.,
    );
    let ops = domowik_edit::add_wall_operations(s.document(), level, branch)
        .unwrap()
        .operations;
    s.commit(ops).unwrap();
    let halves: Vec<_> = s
        .document()
        .walls
        .iter()
        .filter(|w| w.start().x == 2000. && w.end().x == 2000.)
        .collect();
    assert_eq!(halves.len(), 2);
    assert!(halves.iter().all(|w| wall_faces(w) == [None, blue]));
}
