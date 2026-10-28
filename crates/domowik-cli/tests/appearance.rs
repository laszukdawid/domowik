use domowik_cli::appearance;
use domowik_container::{digest_of, write_package, Package, Plan};
use domowik_format::{AssetInstance, Document, InstanceTransform, Point, Wall};
use serde_json::json;
use uuid::Uuid;

#[test]
fn door_colors_and_styles_persist_compact_and_export_without_changing_geometry() {
    use domowik_format::{appearance::DoorStyle, Opening};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("door.domo");
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let wall = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(4000., 0.),
        120.,
        2500.,
    );
    let mut door = Opening::door(wall.id, 0.3, 900., 2100., 120.);
    let id = door.id.to_string();
    door.style_refs.push("opaque-token".into());
    let window = Opening::window(wall.id, 0.8, 600., 1000., 900., 120.);
    let window_id = window.id.to_string();
    doc.openings.extend([door, window]);
    doc.walls.push(wall);
    write_package(&path, &Package::new(doc.clone())).unwrap();
    let cli = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_domowik"))
            .args(["color", "door"])
            .arg(&path)
            .arg(&id)
            .args(args)
            .output()
            .unwrap()
    };
    for args in [
        vec![],
        vec!["--style", "invisible"],
        vec!["--reset", "--style", "glass"],
    ] {
        let original = std::fs::read(&path).unwrap();
        assert_eq!(cli(&args).status.code(), Some(2));
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    assert!(cli(&["--style", "glass"]).status.success());
    assert!(cli(&["--reset"]).status.success());
    assert_eq!(Plan::open(&path).unwrap().document(), &doc);
    appearance::door(&path, &id, Some("#1234ab"), Some(DoorStyle::Glass), false).unwrap();
    let saved = std::fs::read(&path).unwrap();
    appearance::door(&path, &id, Some("#1234AB"), Some(DoorStyle::Glass), false).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    for target in [&window_id, "00000000-0000-0000-0000-000000000000"] {
        assert!(appearance::door(&path, target, None, Some(DoorStyle::Glass), false).is_err());
    }
    assert!(appearance::door(&path, &id, Some("bad"), None, false).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    let plan = Plan::open(&path).unwrap();
    assert_eq!(plan.document().walls, doc.walls);
    assert_eq!(
        plan.document().openings[0].style_refs,
        doc.openings[0].style_refs
    );
    let inventory: serde_json::Value =
        serde_json::from_str(&appearance::list(&path).unwrap()).unwrap();
    assert_eq!(inventory["doors"][0]["color"], "#1234AB");
    assert_eq!(inventory["doors"][0]["doorStyle"], "glass");
    let compacted = domowik_container::read_package(&path)
        .unwrap()
        .compacted()
        .unwrap();
    assert_eq!(compacted.snapshot.openings[0].unknown["color"], "#1234AB");
    assert_eq!(compacted.snapshot.openings[0].unknown["doorStyle"], "glass");
    let svg = domowik_cli::svg::render_level(plan.document(), level);
    assert!(svg.contains("stroke=\"#1234AB\""));
    let colored = domowik_cli::png::render_level(plan.document(), level, 0.1).unwrap();
    let original = domowik_cli::png::render_level(&doc, level, 0.1).unwrap();
    assert_ne!(colored.png, original.png);
    appearance::door(&path, &id, None, Some(DoorStyle::Solid), false).unwrap();
    assert_eq!(
        Plan::open(&path).unwrap().document().openings[0].unknown["color"],
        "#1234AB"
    );
    appearance::door(&path, &id, Some("original"), Some(DoorStyle::Glass), false).unwrap();
    let plan = Plan::open(&path).unwrap();
    assert!(!plan.document().openings[0].unknown.contains_key("color"));
    assert_eq!(plan.document().openings[0].unknown["doorStyle"], "glass");
    assert!(domowik_cli::svg::render_level(plan.document(), level).contains("stroke=\"#80BFE6\""));
    appearance::door(&path, &id, None, None, true).unwrap();
    assert_eq!(Plan::open(&path).unwrap().document(), &doc);
}

#[test]
fn cli_palette_and_wall_colors_roundtrip_without_mutating_source_and_noops_do_not_save() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("original.domo");
    let bytes = b"v 0 1 0\nv 2 1 0\nv 0 1 2\nusemtl cabinet\nf 1 2 3\n";
    let digest = digest_of(bytes);
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let wall = Wall::new(
        level,
        Point::new(0., 0.),
        Point::new(4000., 0.),
        120.,
        2500.,
    );
    let wall_id = wall.id.to_string();
    doc.walls.push(wall);
    let id = Uuid::now_v7();
    doc.asset_instances.push(AssetInstance {
        id,
        level_id: level,
        digest: digest.clone(),
        transform: InstanceTransform::default(),
        display_name: Some("Original cabinet".into()),
        unknown: Default::default(),
    });
    let mut package = Package::new(doc);
    package.assets.insert(digest.clone(), bytes.to_vec());
    write_package(&path, &package).unwrap();
    appearance::asset(
        &path,
        "Original cabinet",
        &["obj:cabinet=#f2f0e8".into()],
        false,
    )
    .unwrap();
    appearance::wall(&path, &wall_id, None, Some("#D8CDB7")).unwrap();
    let saved = std::fs::read(&path).unwrap();
    appearance::asset(
        &path,
        &id.to_string(),
        &["obj:cabinet=#F2F0E8".into()],
        false,
    )
    .unwrap();
    appearance::wall(&path, &wall_id, None, Some("#D8CDB7")).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(appearance::asset(&path, &id.to_string(), &["unknown=#FFFFFF".into()], false).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    let list: serde_json::Value = serde_json::from_str(&appearance::list(&path).unwrap()).unwrap();
    assert_eq!(
        list["assets"][0]["materialColors"],
        json!({"obj:cabinet":"#F2F0E8"})
    );
    assert_eq!(list["walls"][0]["color"], "#D8CDB7");
    assert_eq!(Plan::open(&path).unwrap().assets[&digest], bytes);
    let plan = Plan::open(&path).unwrap();
    let svg = domowik_cli::svg::render_level(plan.document(), level);
    assert!(svg.contains("#D8CDB7"));
    let raster = domowik_cli::png::render_level(plan.document(), level, 0.1).unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&raster.png).unwrap();
    let x = ((2000. - raster.origin_mm[0]) * raster.scale) as usize;
    let y = (raster.origin_mm[1] * raster.scale) as usize;
    assert_eq!(
        &pixmap.data()[(y * pixmap.width() as usize + x) * 4..][..4],
        &[216, 205, 183, 255]
    );
    let compacted = domowik_container::read_package(&path)
        .unwrap()
        .compacted()
        .unwrap();
    assert_eq!(
        compacted.snapshot.asset_instances[0].unknown["materialColors"],
        json!({"obj:cabinet":"#F2F0E8"})
    );
    appearance::asset(&path, &id.to_string(), &[], true).unwrap();
    appearance::wall(&path, &wall_id, None, None).unwrap();
    let plan = Plan::open(&path).unwrap();
    assert!(!plan.document().asset_instances[0]
        .unknown
        .contains_key("materialColors"));
    assert!(!plan.document().walls[0].unknown.contains_key("color"));
}

#[test]
fn cli_paints_one_face_and_lists_effective_faces() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("faces.domo");
    let mut doc = Document::with_ground_floor();
    let wall = Wall::new(
        doc.levels[0].id,
        Point::new(0., 0.),
        Point::new(4000., 0.),
        120.,
        2500.,
    );
    let wall_id = wall.id.to_string();
    doc.walls.push(wall);
    write_package(&path, &Package::new(doc)).unwrap();
    appearance::wall(&path, &wall_id, None, Some("#D8CDB7")).unwrap();
    appearance::wall(
        &path,
        &wall_id,
        Some(domowik_format::Side::Left),
        Some("#B77964"),
    )
    .unwrap();
    let listed: serde_json::Value =
        serde_json::from_str(&appearance::list(&path).unwrap()).unwrap();
    let w = &listed["walls"][0];
    assert_eq!(
        (&w["colorLeft"], &w["colorRight"]),
        (&json!("#B77964"), &json!("#D8CDB7"))
    );
    assert_eq!(w["color"], serde_json::Value::Null);
    assert!(appearance::room(&path, "nowhere", Some("#B77964")).is_err());
}
