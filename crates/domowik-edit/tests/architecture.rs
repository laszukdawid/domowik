use domowik_document::{operation::Operation, session::Session};
use domowik_edit::{add_wall_operations, apply_preview, architecture::*};
use domowik_format::*;
use domowik_geometry::{architecture, rooms};
use uuid::Uuid;

fn levels() -> Document {
    let mut d = Document::with_ground_floor();
    let mut l = Level::new("Upper");
    l.elevation_mm = 2600.0;
    d.levels.push(l);
    d
}

#[test]
fn descending_drawing_uses_nearest_lower_datum_and_preview_matches_commit() {
    let mut d = levels();
    let upper = d.levels[1].id;
    let mut middle = Level::new("Middle");
    middle.elevation_mm = 1000.0;
    let middle_id = middle.id;
    d.levels.push(middle);
    d.levels.reverse(); // Display/collection ordering does not choose the target.
    let request = DescendingStairRequest {
        upper_level_id: upper,
        start_mm: [4000.0, 1000.0],
        end_mm: [1000.0, 1000.0],
        name: None,
        width_mm: None,
        risers: None,
        guard_height_mm: None,
    };
    let before = d.clone();
    let preview = preview_descending_stair(&d, &request).unwrap();
    assert_eq!(d, before);
    assert_eq!(preview[0].direction, "DOWN");
    let props = preview[0].staircase.as_ref().unwrap();
    assert_eq!(props.lower_level_id, middle_id);
    assert_eq!(props.start_elevation_mm, 2600.0);
    assert_eq!(props.end_elevation_mm, 1000.0);
    let ops = add_descending_stair_operations(&d, &request).unwrap();
    for op in ops {
        apply_preview(&mut d, &op);
    }
    let committed = architecture::plan(&d, upper).unwrap();
    assert_eq!(committed[0].outline_mm, preview[0].outline_mm);
    assert_eq!(committed[0].segments_mm, preview[0].segments_mm);
    assert_eq!(d.level_connections[0].to.position_mm, request.start_mm);
    assert_eq!(d.level_connections[0].from.position_mm, request.end_mm);
    let mut lowest = request;
    lowest.upper_level_id = before
        .levels
        .iter()
        .find(|l| l.elevation_mm == 0.0)
        .unwrap()
        .id;
    assert!(add_descending_stair_operations(&before, &lowest).is_err());
    assert!(preview_descending_stair(&before, &lowest).is_err());
}

#[test]
fn stair_names_and_endpoint_offsets_edit_atomically_and_follow_levels() {
    let mut d = levels();
    add(&mut d);
    let id = d.staircases[0].id;
    let mut session =
        Session::open_snapshot(Uuid::now_v7(), d.clone(), &Default::default(), &[], vec![])
            .unwrap();
    let changes = StairFields {
        name: Some("Garden stairs".into()),
        start_elevation_mm: Some(2450.0),
        end_elevation_mm: Some(200.0),
        risers: Some(13),
        ..Default::default()
    };
    let ops = set_stair_fields_operations(session.document(), id, &changes).unwrap();
    session.commit(ops).unwrap();
    let changed = session.document().clone();
    let c = &changed.level_connections[0];
    assert_eq!(c.to.offset_mm, -150.0);
    assert_eq!(c.from.offset_mm, 200.0);
    assert_eq!(c.name, "Garden stairs");
    assert_eq!(changed.staircases[0].name, "Garden stairs");
    let g = architecture::stair(&changed, &changed.staircases[0]).unwrap();
    assert_eq!(g.rise_mm, 2250.0);
    assert_eq!(g.upper_elevation_mm, 2450.0);
    assert_eq!(g.lower_elevation_mm, 200.0);
    session.undo().unwrap();
    assert_eq!(session.document(), &d);
    session.redo().unwrap();
    assert_eq!(session.document(), &changed);
    for bad in [200.0, -200.0, f64::NAN, f64::INFINITY] {
        assert!(set_stair_fields_operations(
            session.document(),
            id,
            &StairFields {
                start_elevation_mm: Some(bad),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(session.document(), &changed);
    }
    let mut raised = changed;
    raised.levels[1].elevation_mm += 100.0;
    assert_eq!(
        architecture::stair(&raised, &raised.staircases[0])
            .unwrap()
            .upper_elevation_mm,
        2550.0
    );
}
fn request(d: &Document) -> StairRequest {
    StairRequest {
        name: "Original test stair".into(),
        from: ConnectionEndpoint {
            level_id: d.levels[0].id,
            position_mm: [0.0, 0.0],
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: d.levels[1].id,
            position_mm: [3780.0, 0.0],
            offset_mm: 0.0,
        },
        width_mm: 924.0,
        risers: 15,
        guard_height_mm: 1000.0,
        cut_upper_floor: true,
    }
}
fn add(d: &mut Document) {
    for op in add_stair_operations(d, &request(d)).unwrap() {
        apply_preview(d, &op);
    }
}

#[test]
fn edge_resizes_preview_and_commit_identically_update_cuts_and_undo_atomically() {
    use domowik_geometry::architecture::StairResizeEdge;
    let mut d = levels();
    add(&mut d);
    let id = d.staircases[0].id;
    for fields in [
        &mut d.staircases[0].unknown,
        &mut d.level_connections[0].unknown,
        &mut d.hosted_openings[0].unknown,
    ] {
        fields.insert("syntheticMetadata".into(), serde_json::json!({"kept":true}));
    }
    for level_id in [d.levels[0].id, d.levels[1].id] {
        for (edge, delta_mm) in [
            (StairResizeEdge::Left, [100.0, -200.0]),
            (StairResizeEdge::Right, [100.0, 200.0]),
            (StairResizeEdge::Lower, [400.0, 100.0]),
            (StairResizeEdge::Upper, [-400.0, 100.0]),
        ] {
            let request = ResizeStairRequest {
                staircase_id: id,
                level_id,
                edge,
                delta_mm,
                snap_angle: false,
            };
            let preview = preview_resize_stair(&d, &request).unwrap();
            let mut session =
                Session::open_snapshot(Uuid::now_v7(), d.clone(), &Default::default(), &[], vec![])
                    .unwrap();
            let before = session.document().clone();
            let ops = resize_stair_operations(&d, &request).unwrap();
            assert_eq!(session.document(), &before);
            session.commit(ops).unwrap();
            assert_eq!(session.undo_depth(), 1);
            let changed = session.document().clone();
            let c = &changed.level_connections[0];
            let s = &changed.staircases[0];
            let cut = &changed.hosted_openings[0];
            assert_eq!(cut.footprint.start_mm, c.from.position_mm);
            assert_eq!(cut.footprint.end_mm, c.to.position_mm);
            assert_eq!(cut.footprint.width_mm, s.width_mm);
            assert_eq!(s.risers, d.staircases[0].risers);
            assert_eq!(s.unknown, d.staircases[0].unknown);
            assert_eq!(c.unknown, d.level_connections[0].unknown);
            assert_eq!(cut.unknown, d.hosted_openings[0].unknown);
            assert_eq!(c.from.level_id, d.levels[0].id);
            assert_eq!(c.to.level_id, d.levels[1].id);
            assert_eq!(c.from.offset_mm, d.level_connections[0].from.offset_mm);
            assert_eq!(c.to.offset_mm, d.level_connections[0].to.offset_mm);
            let committed = architecture::plan(&changed, level_id).unwrap();
            assert_eq!(preview[0].outline_mm, committed[0].outline_mm);
            assert_eq!(preview[0].segments_mm, committed[0].segments_mm);
            session.undo().unwrap();
            assert_eq!(session.document(), &d);
            session.redo().unwrap();
            assert_eq!(session.document(), &changed);
        }
    }
    let mut request = ResizeStairRequest {
        staircase_id: id,
        level_id: d.levels[0].id,
        edge: StairResizeEdge::Left,
        delta_mm: [0.0, 0.0],
        snap_angle: false,
    };
    assert!(resize_stair_operations(&d, &request).unwrap().is_empty());
    request.delta_mm = [0.0, -2000.0];
    assert!(resize_stair_operations(&d, &request).is_err());
    assert!(preview_resize_stair(&d, &request).is_err());
    request.delta_mm = [0.0, 200.0];
    let mut exterior = d.clone();
    exterior.staircases[0].opening_ids.clear();
    let mut resized_exterior = exterior.clone();
    for op in resize_stair_operations(&exterior, &request).unwrap() {
        apply_preview(&mut resized_exterior, &op);
    }
    assert_eq!(resized_exterior.hosted_openings, exterior.hosted_openings);
    assert!(resized_exterior.staircases[0].width_mm > exterior.staircases[0].width_mm);
    let mut custom = d.clone();
    custom.hosted_openings[0].footprint.width_mm += 10.0;
    assert!(resize_stair_operations(&custom, &request).is_err());
    assert!(preview_resize_stair(&custom, &request).is_err());
    request.level_id = Uuid::now_v7();
    assert!(resize_stair_operations(&d, &request).is_err());
}

#[test]
fn rotated_stair_ends_and_angle_constraints_keep_preview_cuts_and_history_in_agreement() {
    use domowik_geometry::architecture::StairResizeEdge;
    let mut d = levels();
    add(&mut d);
    for edge in [StairResizeEdge::Lower, StairResizeEdge::Upper] {
        for snap_angle in [false, true] {
            let request = ResizeStairRequest {
                staircase_id: d.staircases[0].id,
                level_id: d.levels[1].id,
                edge,
                delta_mm: [400.0, 300.0],
                snap_angle,
            };
            let preview = preview_resize_stair(&d, &request).unwrap();
            let mut session =
                Session::open_snapshot(Uuid::now_v7(), d.clone(), &Default::default(), &[], vec![])
                    .unwrap();
            session
                .commit(resize_stair_operations(&d, &request).unwrap())
                .unwrap();
            let changed = session.document().clone();
            let c = &changed.level_connections[0];
            let before = &d.level_connections[0];
            if matches!(edge, StairResizeEdge::Lower) {
                assert_eq!(c.to, before.to);
            } else {
                assert_eq!(c.from, before.from);
            }
            assert_eq!(changed.staircases, d.staircases);
            assert_eq!(
                changed.hosted_openings[0].footprint.start_mm,
                c.from.position_mm
            );
            assert_eq!(
                changed.hosted_openings[0].footprint.end_mm,
                c.to.position_mm
            );
            assert_eq!(
                preview[0].outline_mm,
                architecture::plan(&changed, request.level_id).unwrap()[0].outline_mm
            );
            session.undo().unwrap();
            assert_eq!(session.document(), &d);
            session.redo().unwrap();
            assert_eq!(session.document(), &changed);
        }
    }
}

#[test]
fn translating_stairs_carries_cuts_preserves_parameters_and_undoes_as_one_batch() {
    let mut d = levels();
    let mut r = request(&d);
    r.from.offset_mm = 100.0;
    r.to.offset_mm = -150.0;
    for op in add_stair_operations(&d, &r).unwrap() {
        apply_preview(&mut d, &op);
    }
    let id = d.staircases[0].id;
    d.level_connections[0]
        .unknown
        .insert("originalMetadata".into(), serde_json::json!(true));
    d.hosted_openings[0]
        .unknown
        .insert("originalMetadata".into(), serde_json::json!(true));
    let mut session =
        Session::open_snapshot(Uuid::now_v7(), d.clone(), &Default::default(), &[], vec![])
            .unwrap();
    let delta = Point::new(350.0, -600.0);
    let ops = domowik_edit::translate_selection_operations(&d, &[id, id], delta).unwrap();
    assert_eq!(ops.len(), 2);
    let direct = translate_stair_operations(&d, id, delta).unwrap();
    for (a, b) in ops.iter().zip(&direct) {
        assert_eq!(a.entity_id, b.entity_id);
        assert_eq!(a.kind, b.kind);
    }
    session.commit(ops).unwrap();
    let changed = session.document().clone();
    let c = &changed.level_connections[0];
    assert_eq!(c.from.position_mm, [350.0, -600.0]);
    assert_eq!(c.to.position_mm, [4130.0, -600.0]);
    assert_eq!(c.from.offset_mm, 100.0);
    assert_eq!(c.to.offset_mm, -150.0);
    assert_eq!(changed.staircases, d.staircases);
    assert_eq!(
        changed.hosted_openings[0].footprint.start_mm,
        c.from.position_mm
    );
    assert_eq!(
        changed.hosted_openings[0].footprint.end_mm,
        c.to.position_mm
    );
    assert_eq!(
        changed.hosted_openings[0].unknown,
        d.hosted_openings[0].unknown
    );
    assert_eq!(c.unknown, d.level_connections[0].unknown);
    for level in &d.levels {
        let before = architecture::plan(&d, level.id).unwrap();
        let after = architecture::plan(&changed, level.id).unwrap();
        for (a, b) in before[0].outline_mm.iter().zip(&after[0].outline_mm) {
            assert_eq!(*b, [a[0] + delta.x, a[1] + delta.y]);
        }
    }
    session.undo().unwrap();
    assert_eq!(session.document(), &d);
    session.redo().unwrap();
    assert_eq!(session.document(), &changed);
    assert!(translate_stair_operations(&d, id, Point::new(0.0, 0.0))
        .unwrap()
        .is_empty());
    for delta in [
        Point::new(f64::NAN, 0.0),
        Point::new(f64::INFINITY, 0.0),
        Point::new(COORD_BOUND_MM, 0.0),
    ] {
        assert!(translate_stair_operations(&d, id, delta).is_err());
    }
    let mut custom = d.clone();
    custom.hosted_openings[0].footprint.width_mm += 100.0;
    assert!(translate_stair_operations(&custom, id, delta).is_err());
    let mut shared = d.clone();
    let mut other = shared.staircases[0].clone();
    other.id = Uuid::now_v7();
    shared.staircases.push(other);
    assert!(translate_stair_operations(&shared, id, delta).is_err());
    let mut exterior = d.clone();
    exterior.staircases[0].opening_ids.clear();
    assert_eq!(
        translate_stair_operations(&exterior, id, delta)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn stairs_in_groups_require_a_common_endpoint_floor_and_move_only_once() {
    let mut d = levels();
    add(&mut d);
    let id = d.staircases[0].id;
    for level in [d.levels[0].id, d.levels[1].id] {
        let mut with_wall = d.clone();
        with_wall.walls.push(Wall::new(
            level,
            Point::new(5000.0, 2000.0),
            Point::new(7000.0, 2000.0),
            200.0,
            2500.0,
        ));
        let ids = [id, with_wall.walls[0].id];
        let ops = domowik_edit::translate_selection_operations(
            &with_wall,
            &ids,
            Point::new(100.0, 200.0),
        )
        .unwrap();
        let mut session = Session::open_snapshot(
            Uuid::now_v7(),
            with_wall.clone(),
            &Default::default(),
            &[],
            vec![],
        )
        .unwrap();
        session.commit(ops).unwrap();
        assert_eq!(
            session.document().level_connections[0].from.position_mm,
            [100.0, 200.0]
        );
        assert_eq!(session.document().walls[0].start_mm, [5100.0, 2200.0]);
        session.undo().unwrap();
        assert_eq!(session.document(), &with_wall);
        with_wall.walls[0].level_id = Uuid::now_v7();
        assert!(domowik_edit::translate_selection_operations(
            &with_wall,
            &ids,
            Point::new(100.0, 200.0)
        )
        .is_err());
    }
}

#[test]
fn primitives_and_assembly_are_distinct_and_derive_both_level_symbols() {
    let mut d = levels();
    add(&mut d);
    assert!(d.validate_plan().is_empty());
    assert!(
        d.construction_bodies.is_empty(),
        "treads are derived, not stored"
    );
    assert_eq!(d.level_connections.len(), 1);
    assert_eq!(d.hosted_openings.len(), 1);
    assert_eq!(d.staircases.len(), 1);
    let g = architecture::stair(&d, &d.staircases[0]).unwrap();
    assert_eq!(g.rise_mm, 2600.0);
    assert_eq!(g.going_mm, 270.0);
    assert!((g.riser_mm - 2600.0 / 15.0).abs() < 1e-10);
    assert_eq!(
        g.bodies
            .iter()
            .filter(|b| b.role == BodyRole::Platform)
            .count(),
        14
    );
    let lower = architecture::plan(&d, d.levels[0].id).unwrap();
    let upper = architecture::plan(&d, d.levels[1].id).unwrap();
    assert_eq!(lower[0].id, upper[0].id);
    assert_eq!(lower[0].direction, "UP");
    assert_eq!(upper[0].direction, "DOWN");
    assert_ne!(lower[0].segments_mm, upper[0].segments_mm);
    let old = architecture::stair(&d, &d.staircases[0]).unwrap().riser_mm;
    d.levels[1].elevation_mm = 3000.0;
    assert!(architecture::stair(&d, &d.staircases[0]).unwrap().riser_mm > old);
    assert_eq!(d.staircases[0].risers, 15);
}

#[test]
fn invalid_parameters_references_and_collapsed_stairs_refuse() {
    let d = levels();
    for n in [0, 1, 257, u16::MAX] {
        let mut r = request(&d);
        r.risers = n;
        assert!(add_stair_operations(&d, &r).is_err());
    }
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY, DIMENSION_CAP_MM + 1.0] {
        let mut r = request(&d);
        r.width_mm = width;
        assert!(add_stair_operations(&d, &r).is_err());
    }
    let mut r = request(&d);
    r.to.level_id = Uuid::now_v7();
    assert!(add_stair_operations(&d, &r).is_err());
    let mut r = request(&d);
    r.to.position_mm = r.from.position_mm;
    assert!(add_stair_operations(&d, &r).is_err());
    let mut r = request(&d);
    std::mem::swap(&mut r.from, &mut r.to);
    assert!(add_stair_operations(&d, &r).is_err());
}

#[test]
fn session_undo_redo_deletion_guards_and_unknown_fields_cover_new_entities() {
    let d = levels();
    let mut session = Session::new();
    for l in &d.levels {
        session
            .commit(vec![Operation::create(Entity::Level(l.clone()))])
            .unwrap();
    }
    let before = session.document().clone();
    let ops = add_stair_operations(session.document(), &request(&d)).unwrap();
    session.commit(ops).unwrap();
    let after = session.document().clone();
    session.undo().unwrap();
    assert_eq!(session.document(), &before);
    session.redo().unwrap();
    assert_eq!(session.document(), &after);
    let s = &session.document().staircases[0];
    let id = s.id;
    let c = s.connection_id;
    let o = s.opening_ids[0];
    for id in [c, o, d.levels[0].id, d.levels[1].id] {
        assert!(session.commit(vec![Operation::delete(id)]).is_err());
        assert_eq!(session.document(), &after);
    }
    assert!(session
        .commit(vec![Operation::set_fields(
            d.levels[1].id,
            serde_json::Map::from_iter([("elevationMm".into(), serde_json::json!(0.0))])
        )])
        .is_err());
    let ops =
        set_stair_operations(session.document(), id, Some(1000.0), Some(16), Some(0.0)).unwrap();
    session.commit(ops).unwrap();
    assert_eq!(session.document().staircase(id).unwrap().risers, 16);
    session.undo().unwrap();
    assert_eq!(session.document(), &after);
    let ops = delete_stair_operations(session.document(), id).unwrap();
    session.commit(ops).unwrap();
    assert!(session.document().staircases.is_empty());
    assert_eq!(session.document().hosted_openings.len(), 1);
    session.undo().unwrap();
    assert_eq!(session.document(), &after);
    let mut value = serde_json::to_value(session.document()).unwrap();
    value["staircases"][0]["futureField"] = serde_json::json!({"a":[1,2]});
    let decoded: Document = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn rectangular_room_conversion_preserves_shared_room_and_undo_restores_everything() {
    let mut d = levels();
    let upper = d.levels[1].id;
    let lower = d.levels[0].id;
    // Two original synthetic adjacent rectangles, deliberately unlike personal data.
    for (a, b) in [
        ([0.0, 0.0], [3000.0, 0.0]),
        ([3000.0, 0.0], [6000.0, 0.0]),
        ([6000.0, 0.0], [6000.0, 2000.0]),
        ([6000.0, 2000.0], [3000.0, 2000.0]),
        ([3000.0, 2000.0], [0.0, 2000.0]),
        ([0.0, 2000.0], [0.0, 0.0]),
        ([3000.0, 0.0], [3000.0, 2000.0]),
    ] {
        let w = Wall::new(upper, a.into(), b.into(), 100.0, 1000.0);
        let edit = add_wall_operations(&d, upper, w).unwrap();
        for op in edit.operations {
            apply_preview(&mut d, &op);
        }
    }
    let mut placeholder = RoomRecord::new(Uuid::now_v7(), upper, Point::new(1500.0, 1000.0));
    placeholder.name = Some("Test steps".into());
    let room = placeholder.id;
    d.room_records.push(placeholder);
    let mut deck = RoomRecord::new(Uuid::now_v7(), upper, Point::new(4500.0, 1000.0));
    deck.name = Some("Test deck".into());
    let deck_id = deck.id;
    d.room_records.push(deck);
    let deck_before = rooms::project_level(&d, upper);
    assert_eq!(deck_before.faces.len(), 2);
    let ops =
        room_to_stair_operations(&d, room, lower, UpperEdge::East, 270.0, 15, 1000.0).unwrap();
    let mut session =
        Session::open_snapshot(Uuid::now_v7(), d.clone(), &Default::default(), &[], vec![])
            .unwrap();
    session.commit(ops.clone()).unwrap();
    let after = session.document().clone();
    session.undo().unwrap();
    // Entity collection order is non-semantic. Undo recreates removed walls
    // and records with their original identities and fields, not their indices.
    let canonical = |d: &Document| {
        let mut d = d.clone();
        d.walls.sort_by_key(|w| w.id);
        d.joins.sort_by_key(|j| j.id);
        d.room_records.sort_by_key(|r| r.id);
        d
    };
    assert_eq!(canonical(session.document()), canonical(&d));
    session.redo().unwrap();
    assert_eq!(canonical(session.document()), canonical(&after));
    for op in ops {
        apply_preview(&mut d, &op);
    }
    assert!(d.validate_plan().is_empty());
    assert!(d.room_record(room).is_none());
    let deck_after = rooms::project_level(&d, upper);
    assert_eq!(deck_after.faces.len(), 1);
    assert_eq!(deck_after.effective_record(0), Some(deck_id));
    let c = &d.level_connections[0];
    assert_eq!(c.to.position_mm, [2950.0, 1000.0]);
    assert_eq!(c.from.position_mm, [-830.0, 1000.0]);
}
