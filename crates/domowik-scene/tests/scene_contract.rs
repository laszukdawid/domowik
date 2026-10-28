use domowik_format::{AssetInstance, Document, InstanceTransform, Level, Point, Wall};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[test]
fn door_style_changes_only_leaf_finish_and_preserves_frames_geometry_and_picking() {
    use domowik_format::{Opening, OpeningMechanism};
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let wall = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(6000., 0.),
        200.,
        3000.,
    );
    let door = Opening::door(wall.id, 0.5, 900., 2100., 200.);
    let id = format!("door:{}", door.id);
    doc.walls.push(wall);
    doc.openings.push(door);
    for mechanism in [OpeningMechanism::DoorHinged, OpeningMechanism::DoorSliding] {
        doc.openings[0].mechanism = Some(mechanism);
        doc.openings[0].unknown.clear();
        let before =
            domowik_scene::build(&doc, &[level], &BTreeSet::new(), &BTreeMap::new()).unwrap();
        doc.openings[0]
            .unknown
            .insert("doorStyle".into(), serde_json::json!("glass"));
        doc.openings[0]
            .unknown
            .insert("color".into(), serde_json::json!("#DCC8B4"));
        for ghosted in [false, true] {
            let ghost = if ghosted {
                BTreeSet::from([level])
            } else {
                BTreeSet::new()
            };
            let after = domowik_scene::build(&doc, &[level], &ghost, &BTreeMap::new()).unwrap();
            assert_eq!(before.meshes, after.meshes);
            for (old, new) in before.nodes.iter().zip(&after.nodes) {
                assert_eq!(old.id, new.id);
                assert_eq!(old.matrix, new.matrix);
                assert_eq!(old.normal_matrix, new.normal_matrix);
                assert_eq!(old.mesh, new.mesh);
                if new.id == id {
                    assert!(new.transparent);
                    assert_eq!(&new.color[..3], &[220. / 255., 200. / 255., 180. / 255.]);
                    assert!((new.color[3] - if ghosted { 0.075 } else { 0.25 }).abs() < 1e-6);
                    assert_eq!(
                        domowik_scene::pick::node_entity(&new.id),
                        domowik_scene::pick::node_entity(&old.id)
                    );
                    // Shoot through the leaf's local centre along its normal:
                    // opacity changes must not make the door unselectable.
                    let m = new.matrix.map(f64::from);
                    let origin = [
                        m[12] + m[8] * 50.,
                        m[13] + m[5] * 0.5 + m[9] * 50.,
                        m[14] + m[10] * 50.,
                    ];
                    let direction = [-m[8], -m[9], -m[10]];
                    let hit = domowik_scene::pick::pick(
                        &after.nodes,
                        origin,
                        direction,
                        [0., 100.],
                        |key| {
                            let vertices = after.meshes.get(key)?;
                            Some(domowik_scene::pick::MeshView {
                                vertices,
                                bounds: domowik_scene::pick::mesh_bounds(vertices)?,
                            })
                        },
                        |node| node.id != id,
                    );
                    assert_eq!(
                        after.nodes[hit.expect("glass leaf remains pickable").node].id,
                        id
                    );
                } else {
                    assert_eq!(&old.color[..3], &new.color[..3]);
                }
            }
        }
        doc.openings[0]
            .unknown
            .insert("doorStyle".into(), serde_json::json!("solid"));
        let solid =
            domowik_scene::build(&doc, &[level], &BTreeSet::new(), &BTreeMap::new()).unwrap();
        let leaf = solid.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(!leaf.transparent);
        assert_eq!(leaf.color, [220. / 255., 200. / 255., 180. / 255., 1.]);
    }
}

#[test]
fn wall_color_replaces_rgb_and_keeps_ghost_alpha_and_default_neighbors() {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let mut wall = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(4000., 0.),
        120.,
        2500.,
    );
    wall.unknown
        .insert("color".into(), serde_json::json!("#D8CDB7"));
    let colored = wall.id;
    doc.walls.push(wall);
    doc.walls.push(Wall::new(
        level,
        Point::new(0., 1000.),
        Point::new(4000., 1000.),
        120.,
        2500.,
    ));
    let scene =
        domowik_scene::build(&doc, &[level], &BTreeSet::from([level]), &BTreeMap::new()).unwrap();
    for node in &scene.nodes {
        if node.id.starts_with(&format!("wall:{colored}")) {
            assert_eq!(node.color, [216. / 255., 205. / 255., 183. / 255., 0.3]);
        } else if node.id.starts_with("wall:") {
            assert_eq!(node.color, [232. / 255., 220. / 255., 196. / 255., 0.3]);
        }
    }
}

#[test]
fn explicit_height_resizes_y_without_changing_width_depth_or_legacy_instances() {
    let mut doc = Document::new();
    doc.levels.push(Level::new("Ground"));
    let level = doc.levels[0].id;
    let digest = format!("sha256:{}", "1".repeat(64));
    let asset = AssetInstance {
        id: Uuid::now_v7(),
        level_id: level,
        digest: digest.clone(),
        transform: InstanceTransform {
            scale_uniform: 2.,
            ..Default::default()
        },
        display_name: None,
        unknown: serde_json::Map::from_iter([
            ("editorFootprintMm".into(), serde_json::json!([1200., 600.])),
            ("editorHeightMm".into(), serde_json::json!(900.)),
        ]),
    };
    doc.asset_instances.push(asset.clone());
    let mut legacy = asset;
    legacy.id = Uuid::now_v7();
    legacy.unknown.remove("editorHeightMm");
    doc.asset_instances.push(legacy);
    let scene = domowik_scene::build(
        &doc,
        &[level],
        &BTreeSet::new(),
        &BTreeMap::from([(digest, 0.5)]),
    )
    .unwrap();
    assert!((scene.nodes[0].matrix[0] - 2.4).abs() < 1e-6);
    assert!((scene.nodes[0].matrix[5] - 3.6).abs() < 1e-6);
    assert!((scene.nodes[0].matrix[10] - 1.2).abs() < 1e-6);
    assert!((scene.nodes[1].matrix[5] - 2.4).abs() < 1e-6);
    let fallback =
        domowik_scene::build(&doc, &[level], &BTreeSet::new(), &BTreeMap::new()).unwrap();
    assert!((fallback.nodes[0].matrix[5] - 1.8).abs() < 1e-6);
    let thin = domowik_scene::build(
        &doc,
        &[level],
        &BTreeSet::new(),
        &BTreeMap::from([(doc.asset_instances[0].digest.clone(), 0.0005)]),
    )
    .unwrap();
    assert!((thin.nodes[0].matrix[5] * 0.0005 - 1.8).abs() < 1e-6);
}

#[test]
fn furniture_is_instanced_grounded_and_rebased_at_real_level_elevation() {
    let mut doc = Document::new();
    doc.levels.push(Level::new("Ground"));
    let level = doc.levels[0].id;
    doc.levels[0].elevation_mm = 3000.;
    let digest = format!("sha256:{}", "1".repeat(64));
    for i in 0..25 {
        doc.asset_instances.push(AssetInstance {
            id: Uuid::now_v7(),
            level_id: level,
            digest: digest.clone(),
            transform: InstanceTransform {
                translation_mm: [1e9 - 10000. + i as f64 * 200., 2e8],
                rotation_deg: 90.,
                ..Default::default()
            },
            display_name: Some("Original model".into()),
            unknown: serde_json::Map::from_iter([(
                "editorFootprintMm".into(),
                serde_json::json!([1200, 600]),
            )]),
        });
    }
    let before = doc.clone();
    let scene = domowik_scene::build(
        &doc,
        &[level],
        &BTreeSet::new(),
        &BTreeMap::from([(digest.clone(), 2.)]),
    )
    .unwrap();
    assert_eq!(doc, before, "scene generation cannot edit the document");
    assert_eq!(
        scene.required_models.as_slice(),
        std::slice::from_ref(&digest)
    );
    assert_eq!(scene.nodes.len(), 25);
    assert_eq!(
        scene.meshes.len(),
        1,
        "furniture vertices are not resent in scene snapshots"
    );
    for node in &scene.nodes {
        assert_eq!(node.mesh, digest);
        assert_eq!(node.matrix[13], 3.);
        assert!(node.matrix[12].abs() < 10.);
        assert!(node.matrix[0].abs() < 1e-6);
        assert!((node.matrix[2] + 1.2).abs() < 1e-6);
    }
    assert!(scene.center.iter().all(|v| v.is_finite()));
}

#[test]
fn hidden_levels_are_absent_and_ghosting_changes_only_render_alpha() {
    let mut doc = Document::new();
    doc.levels.push(Level::new("Ground"));
    let active = doc.levels[0].id;
    let second = Level::new("Upper");
    let other = second.id;
    doc.levels.push(second);
    for level in [active, other] {
        doc.walls.push(Wall::new(
            level,
            Point::new(0., 0.),
            Point::new(5000., 0.),
            200.,
            2500.,
        ));
    }
    let scene = domowik_scene::build(
        &doc,
        &[active],
        &BTreeSet::from([active]),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(scene.nodes.len(), 1);
    assert!(scene.nodes[0].transparent);
    assert_eq!(scene.nodes[0].color[3], 0.3);
}

#[test]
fn room_floors_are_closed_slabs_and_walls_use_stored_height() {
    let mut doc = Document::new();
    doc.levels.push(Level::new("Ground"));
    let level = doc.levels[0].id;
    let points = [
        Point::new(0., 0.),
        Point::new(6000., 0.),
        Point::new(6000., 4000.),
        Point::new(0., 4000.),
    ];
    for i in 0..4 {
        doc.walls.push(Wall::new(
            level,
            points[i],
            points[(i + 1) % 4],
            200.,
            2700.,
        ));
    }
    let scene =
        domowik_scene::build(&doc, &[level], &Default::default(), &Default::default()).unwrap();
    let floor = scene
        .meshes
        .iter()
        .find(|(id, _)| id.starts_with("geometry:floor"))
        .unwrap()
        .1;
    assert!(floor.as_chunks::<10>().0.iter().any(|v| v[1] == -0.2));
    assert!(floor.as_chunks::<10>().0.iter().any(|v| v[1] == 0.));
    for n in scene.nodes.iter().filter(|n| n.id.starts_with("wall:")) {
        assert!((n.matrix[5] - 2.7).abs() < 1e-6);
    }
}

#[test]
fn doors_and_windows_cut_real_vertical_openings_and_preserve_the_leaf_orientation() {
    let mut doc = Document::new();
    doc.levels.push(Level::new("Ground"));
    let level = doc.levels[0].id;
    let wall = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(6000., 0.),
        200.,
        3000.,
    );
    let id = wall.id;
    doc.walls.push(wall);
    let door = domowik_format::Opening::door(id, 0.25, 900., 2100., 200.);
    let door_id = door.id;
    let window = domowik_format::Opening::window(id, 0.7, 1200., 1200., 1000., 200.);
    let window_id = window.id;
    doc.openings.extend([door, window]);
    let scene =
        domowik_scene::build(&doc, &[level], &Default::default(), &Default::default()).unwrap();
    let door_header = scene
        .nodes
        .iter()
        .find(|n| n.id.starts_with("wall:") && (n.matrix[12] + 1.5).abs() < 1e-6)
        .unwrap();
    assert!((door_header.matrix[13] - 2.1).abs() < 1e-6);
    assert!((door_header.matrix[5] - 0.9).abs() < 1e-6);
    let leaf = scene
        .nodes
        .iter()
        .find(|n| n.id == format!("door:{door_id}"))
        .unwrap();
    assert!(leaf.matrix[0].abs() < 1e-6);
    assert!((leaf.matrix[2] + 0.9).abs() < 1e-6);
    assert!(
        scene
            .nodes
            .iter()
            .find(|n| n.id == format!("window:{window_id}"))
            .unwrap()
            .transparent
    );
}

/// Floors supplied from a retained `LevelGeometry` give the same scene as the
/// one-shot builder after every edit of a sequence that splits, merges and
/// nests rooms (issue 0013); a requested level without an entry is an error.
#[test]
fn retained_floors_build_the_same_scene_as_the_one_shot_builder() {
    use domowik_geometry::incremental::LevelGeometry;
    use domowik_geometry::walls;

    let mut doc = Document::new();
    doc.levels.push(Level::new("Ground"));
    let level = doc.levels[0].id;
    let wall = |a: (f64, f64), b: (f64, f64), t: f64| {
        Wall::new(level, Point::new(a.0, a.1), Point::new(b.0, b.1), t, 2500.)
    };
    let square = |x0: f64, y0: f64, x1: f64, y1: f64, t: f64| {
        vec![
            wall((x0, y0), (x1, y0), t),
            wall((x1, y0), (x1, y1), t),
            wall((x1, y1), (x0, y1), t),
            wall((x0, y1), (x0, y0), t),
        ]
    };
    let mut retained = LevelGeometry::new();
    let check = |doc: &Document, retained: &mut LevelGeometry, rooms: usize| {
        retained.sync(&walls::level_solids(doc, level));
        let floors = BTreeMap::from([(level, retained.rooms())]);
        let none = BTreeSet::new();
        let models = BTreeMap::new();
        let ours =
            domowik_scene::build_with_floors(doc, &[level], &none, &models, &floors).unwrap();
        let oracle = domowik_scene::build(doc, &[level], &none, &models).unwrap();
        assert_eq!(
            serde_json::to_value(&ours).unwrap(),
            serde_json::to_value(&oracle).unwrap()
        );
        let floor_nodes = ours
            .nodes
            .iter()
            .filter(|n| n.id.starts_with("floor:"))
            .count();
        assert_eq!(floor_nodes, rooms);
    };

    check(&doc, &mut retained, 0);
    doc.walls.extend(square(0., 0., 6000., 4000., 200.));
    check(&doc, &mut retained, 1);
    let divider = wall((3000., 0.), (3000., 4000.), 100.);
    let divider_id = divider.id;
    doc.walls.push(divider);
    check(&doc, &mut retained, 2);
    // An island room inside the left room: the floor around it has a hole.
    doc.walls.extend(square(1000., 1000., 2000., 2000., 100.));
    check(&doc, &mut retained, 3);
    doc.walls.retain(|w| w.id != divider_id);
    check(&doc, &mut retained, 2);

    let err = domowik_scene::build_with_floors(
        &doc,
        &[level],
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .err()
    .unwrap();
    assert!(err.contains("no floor geometry"), "{err}");
}

#[test]
fn differing_faces_split_walls_and_corners_while_equal_faces_stay_whole() {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let mut a = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(4000., 0.),
        120.,
        2500.,
    );
    let mut b = Wall::new(
        level,
        Point::new(4000., 0.),
        Point::new(4000., 3000.),
        120.,
        2500.,
    );
    for w in [&mut a, &mut b] {
        w.unknown
            .insert("colorLeft".into(), serde_json::json!("#B77964"));
    }
    let (a_id, b_id) = (a.id, b.id);
    doc.joins.push(domowik_format::WallJoin::new(vec![
        domowik_format::JoinMember::new(a_id, domowik_format::Role::End),
        domowik_format::JoinMember::new(b_id, domowik_format::Role::Start),
    ]));
    doc.walls.extend([a, b]);
    let red = [183. / 255., 121. / 255., 100. / 255., 1.];
    let beige = [232. / 255., 220. / 255., 196. / 255., 1.];
    let scene = domowik_scene::build(&doc, &[level], &BTreeSet::new(), &BTreeMap::new()).unwrap();
    let node = |id: String| scene.nodes.iter().find(|n| n.id == id).unwrap();
    let (left, right) = (
        node(format!("wall:{a_id}:0:0:L")),
        node(format!("wall:{a_id}:0:0:R")),
    );
    assert_eq!((left.color, right.color), (red, beige));
    // Each half is half the thickness, its centre a quarter-thickness off axis.
    for half in [left, right] {
        assert!((half.matrix[10] - 0.06).abs() < 1e-6, "{:?}", half.matrix);
    }
    let gap = (left.matrix[14] - right.matrix[14]).abs();
    assert!((gap - 0.06).abs() < 1e-6, "{gap}");
    // The corner splits into its inner (red, both Left faces) and outer wedges.
    let joins: Vec<_> = scene
        .nodes
        .iter()
        .filter(|n| n.id.starts_with("join:"))
        .collect();
    assert!(
        joins.len() >= 2,
        "{:?}",
        joins.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    assert!(joins.iter().any(|n| n.color == red) && joins.iter().any(|n| n.color == beige));
    assert!(domowik_scene::pick::node_entity(&left.id).is_some());

    for w in &mut doc.walls {
        w.unknown.remove("colorLeft");
        w.unknown
            .insert("color".into(), serde_json::json!("#B77964"));
    }
    let whole = domowik_scene::build(&doc, &[level], &BTreeSet::new(), &BTreeMap::new()).unwrap();
    assert!(whole
        .nodes
        .iter()
        .all(|n| !n.id.ends_with(":L") && !n.id.ends_with(":R")));
    assert_eq!(
        whole
            .nodes
            .iter()
            .filter(|n| n.id.starts_with("join:"))
            .count(),
        1
    );
}
