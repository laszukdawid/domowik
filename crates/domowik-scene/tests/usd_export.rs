use domowik_format::{AssetInstance, Document, InstanceTransform, Opening, Point, Wall};
use std::collections::BTreeMap;
use uuid::Uuid;

/// An original 1 m red cube, 100 units per metre.
const CUBE: &str = r#"#usda 1.0
(
metersPerUnit = 0.01
)
def Mesh "Box" {
    int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
    int[] faceVertexIndices = [0, 1, 3, 2, 4, 6, 7, 5, 0, 4, 5, 1, 2, 3, 7, 6, 0, 2, 6, 4, 1, 5, 7, 3]
    point3f[] points = [(0, 0, 0), (100, 0, 0), (0, 100, 0), (100, 100, 0), (0, 0, 100), (100, 0, 100), (0, 100, 100), (100, 100, 100)]
    color3f[] primvars:displayColor = [(1, 0, 0)]
}
"#;

fn instance(level: Uuid, digest: &str, x: f64, colors: Option<&str>) -> AssetInstance {
    let mut unknown = serde_json::Map::new();
    if let Some(c) = colors {
        unknown.insert("materialColors".into(), serde_json::json!({ "default": c }));
    }
    AssetInstance {
        id: Uuid::now_v7(),
        level_id: level,
        digest: digest.into(),
        transform: InstanceTransform {
            translation_mm: [x, 1500.],
            ..Default::default()
        },
        display_name: Some("Cube".into()),
        unknown,
    }
}

fn plan() -> (Document, BTreeMap<String, Vec<u8>>) {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let corners = [(0., 0.), (6000., 0.), (6000., 4000.), (0., 4000.)];
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        doc.walls.push(Wall::new(
            level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.,
            2700.,
        ));
    }
    let mut door = Opening::door(doc.walls[0].id, 0.5, 900., 2100., 200.);
    door.mechanism = Some(domowik_format::OpeningMechanism::DoorHinged);
    door.unknown
        .insert("doorStyle".into(), serde_json::json!("glass"));
    doc.openings.push(door);
    let digest = "sha256:".to_owned() + &"ab".repeat(32);
    doc.asset_instances
        .push(instance(level, &digest, 1500., None));
    doc.asset_instances
        .push(instance(level, &digest, 3000., None));
    doc.asset_instances
        .push(instance(level, &digest, 4500., Some("#00FF00")));
    doc.asset_instances
        .push(instance(level, "sha256:missing", 5000., None));
    (doc, BTreeMap::from([(digest, CUBE.as_bytes().to_vec())]))
}

#[test]
fn plan_exports_a_deterministic_self_contained_usdz() {
    let (doc, assets) = plan();
    let export = domowik_scene::usd::usdz(&doc, &assets).unwrap();
    assert!(export.bytes.starts_with(b"PK"));
    assert!(
        export
            .diagnostics
            .iter()
            .any(|d| d.contains("model unavailable")),
        "{:?}",
        export.diagnostics
    );
    let again = domowik_scene::usd::usdz(&doc, &assets).unwrap();
    assert!(export.bytes == again.bytes, "byte-identical re-export");

    // Domowik's own USD reader composes the package back.
    let mesh = domowik_models::mesh(&export.bytes).unwrap();
    assert_eq!(mesh.source_triangles, export.triangles);
    assert!(
        mesh.materials.iter().all(|m| m.key.starts_with("usd:")),
        "bindings survive references: {:?}",
        mesh.materials.iter().map(|m| &m.key).collect::<Vec<_>>()
    );
    let colors: Vec<_> = mesh.materials.iter().map(|m| m.color).collect();
    assert!(
        colors.iter().any(|c| c[0] > 0.99 && c[1] < 0.01),
        "red cube"
    );
    assert!(
        colors.iter().any(|c| c[1] > 0.99 && c[0] < 0.01),
        "recoloured cube"
    );
    assert!(
        colors.iter().any(|c| c[3] < 0.99),
        "translucent glass door leaf"
    );
    let size = domowik_models::dimensions(&export.bytes).unwrap();
    assert!(size.physical_units);
    // Walls 6 m + 0.2 m thickness wide and 4.2 m deep.
    assert!(
        (size.dimensions_mm[0] - 6200.).abs() < 1.,
        "{:?}",
        size.dimensions_mm
    );
    // Depth adds the door frame/leaf standing proud of the wall face.
    assert!(
        (4200.0..4221.0).contains(&size.dimensions_mm[1]),
        "{:?}",
        size.dimensions_mm
    );
}

#[test]
fn repeated_models_are_written_once() {
    let (doc, assets) = plan();
    let export = domowik_scene::usd::usdz(&doc, &assets).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&export.bytes)).unwrap();
    let mut layer = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("plan.usdc").unwrap(), &mut layer).unwrap();
    let data = openusd::sdf::Layer::from_bytes("plan.usdc", layer).unwrap();
    let text = data.export_to_string().unwrap();
    // Two plain cubes share one prototype; the recoloured one has its own.
    let references = text.matches("references = </Prototypes/").count();
    let prototypes = text.matches("def Xform \"P").count();
    assert!(
        prototypes < references,
        "{prototypes} prototypes for {references} nodes"
    );
}

#[test]
fn embedded_model_textures_are_packaged() {
    let pixels = image::RgbaImage::from_fn(4, 4, |x, _| {
        image::Rgba(if x < 2 {
            [0, 0, 255, 255]
        } else {
            [255, 255, 0, 255]
        })
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let layer = r#"#usda 1.0
(
metersPerUnit = 1
)
def Mesh "Quad" {
    int[] faceVertexCounts = [3, 3]
    int[] faceVertexIndices = [0, 1, 2, 0, 2, 3]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 0.1, 1), (0, 0.1, 1)]
    texCoord2f[] primvars:st = [(0, 0), (1, 0), (1, 1), (0, 1)] (interpolation = "vertex")
    rel material:binding = </Looks/Paint>
}
def Scope "Looks" {
    def Material "Paint" {
        token outputs:surface.connect = </Looks/Paint/Surface.outputs:surface>
        def Shader "Surface" {
            uniform token info:id = "UsdPreviewSurface"
            color3f inputs:diffuseColor.connect = </Looks/Paint/Image.outputs:rgb>
            token outputs:surface
        }
        def Shader "Image" {
            uniform token info:id = "UsdUVTexture"
            asset inputs:file = @paint.png@
            float3 outputs:rgb
        }
    }
}
"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .with_alignment(64);
    for (name, bytes) in [
        ("quad.usda", layer.as_bytes()),
        ("paint.png", png.get_ref()),
    ] {
        zip.start_file(name, options).unwrap();
        std::io::Write::write_all(&mut zip, bytes).unwrap();
    }
    let model = zip.finish().unwrap().into_inner();
    assert!(domowik_models::mesh(&model).unwrap().texture.is_some());

    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let digest = "sha256:".to_owned() + &"cd".repeat(32);
    doc.asset_instances.push(instance(level, &digest, 0., None));
    let export = domowik_scene::usd::usdz(&doc, &BTreeMap::from([(digest, model)])).unwrap();
    let archive = zip::ZipArchive::new(std::io::Cursor::new(&export.bytes)).unwrap();
    assert!(archive.file_names().any(|n| n == "textures/t0.png"));
    let mut layer = Vec::new();
    std::io::Read::read_to_end(
        &mut zip::ZipArchive::new(std::io::Cursor::new(&export.bytes))
            .unwrap()
            .by_name("plan.usdc")
            .unwrap(),
        &mut layer,
    )
    .unwrap();
    let text = openusd::sdf::Layer::from_bytes("plan.usdc", layer)
        .unwrap()
        .export_to_string()
        .unwrap();
    // Opacity follows the texel alpha, scaled by the material alpha.
    assert!(
        text.contains("inputs:opacity.connect = </Prototypes/"),
        "{text}"
    );
    assert!(text.contains("Texture.outputs:a>"));
    assert!(text.contains("float outputs:a"));
    // UsdPrimvarReader declares varname as a string (usdchecker --arkit).
    assert!(text.contains("string inputs:varname = \"st\""));
    let back = domowik_models::mesh(&export.bytes).unwrap();
    let texture = back.texture.expect("texture survives export");
    assert_eq!((texture.width, texture.height), (4, 4));
}

#[test]
fn undecodable_models_are_named_and_aggregate_budget_is_enforced() {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.asset_instances
        .push(instance(level, "sha256:broken", 0., None));
    let assets = BTreeMap::from([("sha256:broken".to_owned(), b"not a model".to_vec())]);
    let export = domowik_scene::usd::usdz(&doc, &assets).unwrap();
    assert!(
        export.diagnostics.iter().any(|d| d.starts_with("Cube: ")),
        "{:?}",
        export.diagnostics
    );

    // 18 distinct 20,000-triangle models retain about 10.5 million floats.
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let mut assets = BTreeMap::new();
    for m in 0..18 {
        let mut obj = String::new();
        for i in 0..20_002 {
            let t = (i * 17 + m) as f64 * 0.618_034;
            obj += &format!(
                "v {} {} {}\n",
                t.fract(),
                (t * 0.37).fract(),
                (t * 0.71).fract()
            );
        }
        for i in 1..=20_000 {
            obj += &format!("f {i} {} {}\n", i + 1, i + 2);
        }
        let digest = format!("sha256:{m:064}");
        doc.asset_instances
            .push(instance(level, &digest, m as f64 * 1000., None));
        assets.insert(digest, obj.into_bytes());
    }
    let error = domowik_scene::usd::usdz(&doc, &assets).err().unwrap();
    assert!(error.contains("export budget"), "{error}");
}
