use domowik_format::{AssetInstance, Document, InstanceTransform, Level, Point, Wall};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

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
