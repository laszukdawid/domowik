use domowik_cli::appearance;
use domowik_container::{digest_of, write_package, Package, Plan};
use domowik_format::{AssetInstance, Document, InstanceTransform, Point, Wall};
use serde_json::json;
use uuid::Uuid;

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
    appearance::wall(&path, &wall_id, Some("#D8CDB7")).unwrap();
    let saved = std::fs::read(&path).unwrap();
    appearance::asset(
        &path,
        &id.to_string(),
        &["obj:cabinet=#F2F0E8".into()],
        false,
    )
    .unwrap();
    appearance::wall(&path, &wall_id, Some("#D8CDB7")).unwrap();
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
    appearance::wall(&path, &wall_id, None).unwrap();
    let plan = Plan::open(&path).unwrap();
    assert!(!plan.document().asset_instances[0]
        .unknown
        .contains_key("materialColors"));
    assert!(!plan.document().walls[0].unknown.contains_key("color"));
}
