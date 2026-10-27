use domowik_document::{operation::Operation, session::Session};
use domowik_edit::appearance;
use domowik_format::{
    appearance::{wall_color, RgbColor},
    Document, Point, Wall,
};
use serde_json::json;
use uuid::Uuid;

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
    s.commit(appearance::walls(s.document(), &ids, color).unwrap())
        .unwrap();
    assert_eq!(s.undo_depth(), 1);
    assert!(appearance::walls(s.document(), &ids, color)
        .unwrap()
        .is_empty());
    for w in &s.document().walls {
        assert_eq!(wall_color(w).unwrap(), color);
        assert_eq!(w.unknown["futureFinish"], json!({"keep":true}));
    }
    assert!(appearance::walls(s.document(), &[ids[0], Uuid::nil()], color).is_err());
    assert!(appearance::walls(s.document(), &[ids[0], ids[0]], color).is_err());
    s.undo().unwrap();
    assert_eq!(s.document(), &doc);
    s.redo().unwrap();
    let serialized = serde_json::to_value(s.document()).unwrap();
    assert_eq!(serialized["walls"][0]["color"], "#D8CDB7");
    s.commit(appearance::walls(s.document(), &ids, None).unwrap())
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
