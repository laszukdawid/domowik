//! Original owner-request regressions for issue 0107; no external fixtures.
use domowik_document::session::Session;
use domowik_edit::translate_selection_operations;
use domowik_format::{Document, JoinMember, Opening, Point, Role, Wall, WallJoin};
use uuid::Uuid;

fn corner() -> Document {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let a = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(1000., 0.),
        120.,
        2500.,
    );
    let b = Wall::new(
        level,
        Point::new(1000., 0.),
        Point::new(1000., 1000.),
        120.,
        2500.,
    );
    doc.joins.push(WallJoin::new(vec![
        JoinMember {
            wall_id: a.id,
            role: Role::End,
        },
        JoinMember {
            wall_id: b.id,
            role: Role::Start,
        },
    ]));
    doc.openings
        .push(Opening::door(a.id, 0.5, 100., 2100., 120.));
    doc.walls.extend([a, b]);
    doc
}

#[test]
fn shared_nodes_move_once_and_one_undo_restores_everything() {
    let doc = corner();
    let ids = [
        doc.walls[1].id,
        doc.walls[0].id,
        doc.walls[1].id,
        doc.openings[0].id,
    ];
    let operations = translate_selection_operations(&doc, &ids, Point::new(200., 300.)).unwrap();
    let mut session = Session::open_snapshot(
        Uuid::now_v7(),
        doc.clone(),
        &Default::default(),
        &[],
        vec![],
    )
    .unwrap();
    session.commit(operations).unwrap();
    assert_eq!(session.undo_depth(), 1);
    for wall in &doc.walls {
        let moved = session.document().wall(wall.id).unwrap();
        assert_eq!(
            moved.start_mm,
            [wall.start_mm[0] + 200., wall.start_mm[1] + 300.]
        );
        assert_eq!(moved.end_mm, [wall.end_mm[0] + 200., wall.end_mm[1] + 300.]);
    }
    assert_eq!(session.document().openings, doc.openings);
    let after = session.document().clone();
    session.undo().unwrap().unwrap();
    assert_eq!(session.document(), &doc);
    session.redo().unwrap().unwrap();
    assert_eq!(session.document(), &after);
}

#[test]
fn attached_unselected_endpoint_follows_without_moving_far_end() {
    let doc = corner();
    let ops =
        translate_selection_operations(&doc, &[doc.walls[0].id], Point::new(200., 300.)).unwrap();
    let mut session = Session::open_snapshot(
        Uuid::now_v7(),
        doc.clone(),
        &Default::default(),
        &[],
        vec![],
    )
    .unwrap();
    session.commit(ops).unwrap();
    let neighbor = session.document().wall(doc.walls[1].id).unwrap();
    assert_eq!(neighbor.start_mm, [1200., 300.]);
    assert_eq!(neighbor.end_mm, [1000., 1000.]);
}

#[test]
fn invalid_targets_and_final_overlap_refuse_the_whole_group() {
    let mut doc = corner();
    let ids: Vec<_> = doc.walls.iter().map(|w| w.id).collect();
    assert!(
        translate_selection_operations(&doc, &[doc.openings[0].id], Point::new(1., 0.)).is_err()
    );
    assert!(translate_selection_operations(&doc, &[Uuid::nil()], Point::new(1., 0.)).is_err());
    assert!(translate_selection_operations(&doc, &ids, Point::new(f64::NAN, 0.)).is_err());
    assert!(
        translate_selection_operations(&doc, &ids, Point::new(0., 0.))
            .unwrap()
            .is_empty()
    );
    doc.walls.push(Wall::new(
        doc.levels[0].id,
        Point::new(0., 2000.),
        Point::new(1000., 2000.),
        120.,
        2500.,
    ));
    assert!(translate_selection_operations(&doc, &ids, Point::new(0., 2000.)).is_err());
}

#[test]
fn mixed_model_and_wall_translation_preserves_transform_and_undoes() {
    let mut doc = corner();
    let asset_id = Uuid::now_v7();
    doc.asset_instances.push(
        serde_json::from_value(serde_json::json!({
            "id": asset_id, "levelId": doc.levels[0].id,
            "digest": format!("sha256:{}", "a".repeat(64)),
            "transform": {"translationMm": [2000., 3000.], "rotationDeg": 37., "scaleUniform": 2.},
            "editorFootprintMm": [400., 600.]
        }))
        .unwrap(),
    );
    let ids = [doc.walls[0].id, doc.walls[1].id, asset_id];
    let ops = translate_selection_operations(&doc, &ids, Point::new(100., 200.)).unwrap();
    let mut session = Session::open_snapshot(
        Uuid::now_v7(),
        doc.clone(),
        &Default::default(),
        &[],
        vec![],
    )
    .unwrap();
    session.commit(ops).unwrap();
    let moved = session.document().asset_instance(asset_id).unwrap();
    assert_eq!(moved.transform.translation_mm, [2100., 3200.]);
    assert_eq!(moved.transform.rotation_deg, 37.);
    assert_eq!(moved.transform.scale_uniform, 2.);
    assert_eq!(moved.unknown, doc.asset_instances[0].unknown);
    session.undo().unwrap().unwrap();
    assert_eq!(session.document(), &doc);
    let other = domowik_format::Level::new("Other");
    let mut mixed = doc.clone();
    mixed.asset_instances[0].level_id = other.id;
    mixed.levels.push(other);
    assert!(translate_selection_operations(&mixed, &ids, Point::new(1., 0.)).is_err());
}
