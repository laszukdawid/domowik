//! Original synthetic USD layers; no third-party sample content.
use domowik_models::{appearance, dimensions, mesh, preview, thumbnail};
use std::io::{Cursor, Write};

const CUBE: &str = r#"
def Mesh "Box" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
    int[] faceVertexIndices = [0, 1, 3, 2, 4, 6, 7, 5, 0, 4, 5, 1, 2, 3, 7, 6, 0, 2, 6, 4, 1, 5, 7, 3]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)]
    color3f[] primvars:displayColor = [(1, 0, 0)] (interpolation = "constant")
}
"#;

fn layer(header: &str, body: &str) -> Vec<u8> {
    format!("#usda 1.0\n(\n{header}\n)\n{body}").into_bytes()
}

fn usdz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .with_alignment(64);
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn png() -> Vec<u8> {
    let pixels = image::RgbaImage::from_fn(4, 4, |x, _| {
        image::Rgba(if x < 2 {
            [0, 0, 255, 255]
        } else {
            [255, 255, 0, 255]
        })
    });
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[test]
fn text_layer_uses_meters_per_unit_and_display_color() {
    let bytes = layer("metersPerUnit = 1\nupAxis = \"Y\"", CUBE);
    let d = dimensions(&bytes).unwrap();
    assert!(d.physical_units);
    assert_eq!(d.dimensions_mm.map(f64::round), [1000., 1000., 1000.]);
    let m = mesh(&bytes).unwrap();
    assert_eq!(m.source_triangles, 12);
    assert_eq!(m.materials.len(), 1);
    // Red display colour survives into per-vertex RGBA (offset 6..10 of 10 floats).
    assert!(m.vertices[6] > 0.9 && m.vertices[7] < 0.1);
    assert!(preview(&bytes).unwrap().covered_pixels > 0);
    assert!(thumbnail(&bytes).unwrap().covered_pixels > 0);
}

#[test]
fn default_units_are_centimetres_and_z_up_is_converted() {
    let tall = CUBE
        .replace("(0, 1, 1), (1, 1, 1)", "(0, 1, 3), (1, 1, 3)")
        .replace("(0, 0, 1), (1, 0, 1)", "(0, 0, 3), (1, 0, 3)");
    let d = dimensions(&layer("upAxis = \"Z\"", &tall)).unwrap();
    // 1 x 1 x 3 cm, Z up: width X 10 mm, depth 10 mm, height 30 mm.
    assert_eq!(d.dimensions_mm.map(f64::round), [10., 10., 30.]);
}

#[test]
fn nested_xform_ops_compose() {
    let body = format!(
        r#"def Xform "Root" {{
    double3 xformOp:scale = (2, 2, 2)
    uniform token[] xformOpOrder = ["xformOp:scale"]
    def Xform "Inner" {{
        double3 xformOp:translate = (5, 0, 0)
        float3 xformOp:scale = (1, 0.5, 1)
        uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:scale"]
        {CUBE}
    }}
}}"#
    );
    let d = dimensions(&layer("metersPerUnit = 1", &body)).unwrap();
    assert_eq!(d.dimensions_mm.map(f64::round), [2000., 2000., 1000.]);
}

#[test]
fn instances_expand_their_prototype() {
    let body = format!(
        r#"def Xform "Proto" {{ {CUBE} }}
def Xform "A" (instanceable = true references = </Proto>) {{}}
def Xform "B" (instanceable = true references = </Proto>) {{
    double3 xformOp:translate = (3, 0, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]
}}"#
    );
    let bytes = layer("metersPerUnit = 1", &body);
    // The prototype source plus two instances.
    assert_eq!(mesh(&bytes).unwrap().source_triangles, 36);
    assert_eq!(dimensions(&bytes).unwrap().dimensions_mm[0].round(), 4000.);
}

#[test]
fn invisible_and_guide_prims_are_skipped() {
    let hidden = CUBE
        .replace("def Mesh \"Box\" (", "def Mesh \"Hidden\" (")
        .replace(
            "{\n    int[]",
            "{\n    token visibility = \"invisible\"\n    int[]",
        );
    let body = format!("{CUBE}\n{hidden}");
    assert_eq!(
        mesh(&layer("metersPerUnit = 1", &body))
            .unwrap()
            .source_triangles,
        12
    );
}

#[test]
fn packaged_preview_surface_texture_and_subsets() {
    let root = layer(
        "metersPerUnit = 1",
        r#"
def Xform "Model" {
    def Mesh "Quad" (prepend apiSchemas = ["MaterialBindingAPI"]) {
        int[] faceVertexCounts = [3, 3]
        int[] faceVertexIndices = [0, 1, 2, 0, 2, 3]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)]
        texCoord2f[] primvars:uv = [(0, 0), (1, 0), (1, 1), (0, 1)] (interpolation = "vertex")
        rel material:binding = </Model/Looks/Painted>
        def GeomSubset "Second" (prepend apiSchemas = ["MaterialBindingAPI"]) {
            uniform token elementType = "face"
            uniform token familyName = "materialBind"
            int[] indices = [1]
            rel material:binding = </Model/Looks/Plain>
        }
    }
    def Scope "Looks" {
        def Material "Painted" {
            token outputs:surface.connect = </Model/Looks/Painted/Surface.outputs:surface>
            def Shader "Surface" {
                uniform token info:id = "UsdPreviewSurface"
                color3f inputs:diffuseColor.connect = </Model/Looks/Painted/Image.outputs:rgb>
                token outputs:surface
            }
            def Shader "Image" {
                uniform token info:id = "UsdUVTexture"
                asset inputs:file = @textures/paint.png@
                token inputs:wrapS = "clamp"
                float2 inputs:st.connect = </Model/Looks/Painted/Reader.outputs:result>
                float3 outputs:rgb
            }
            def Shader "Reader" {
                uniform token info:id = "UsdPrimvarReader_float2"
                token inputs:varname = "uv"
                float2 outputs:result
            }
        }
        def Material "Plain" {
            token outputs:surface.connect = </Model/Looks/Plain/Surface.outputs:surface>
            def Shader "Surface" {
                uniform token info:id = "UsdPreviewSurface"
                color3f inputs:diffuseColor = (0, 1, 0)
                float inputs:opacity = 0.5
                token outputs:surface
            }
        }
    }
}
"#,
    );
    let texture = png();
    let bytes = usdz(&[("model.usda", &root), ("textures/paint.png", &texture)]);
    let m = mesh(&bytes).unwrap();
    let keys: Vec<_> = m.materials.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["usd:/Model/Looks/Painted", "usd:/Model/Looks/Plain"]);
    let plain = &m.materials[1];
    assert_eq!(plain.color[1], 1.);
    assert_eq!(plain.color[3], 0.5);
    let t = m.texture.as_ref().expect("embedded texture");
    assert_eq!((t.width, t.height, t.layers), (4, 4, 1));
    let listed = appearance::materials(&bytes).unwrap();
    assert_eq!(listed.len(), 2);
}

#[test]
fn binary_crate_layer_reads_like_text() {
    let text = layer("metersPerUnit = 1", CUBE);
    let layer = openusd::sdf::Layer::from_bytes("cube.usda", text).unwrap();
    let mut out = Cursor::new(Vec::new());
    openusd::usdc::CrateWriter::write(layer.data(), &mut out).unwrap();
    let bytes = out.into_inner();
    assert!(bytes.starts_with(b"PXR-USDC"));
    assert_eq!(mesh(&bytes).unwrap().source_triangles, 12);
    let packaged = usdz(&[("cube.usdc", &bytes)]);
    assert_eq!(mesh(&packaged).unwrap().source_triangles, 12);
}

#[test]
fn external_references_are_refused() {
    let body = r#"def Xform "Model" (references = @./elsewhere.usda@) {}"#;
    let e = mesh(&layer("metersPerUnit = 1", body)).err().unwrap();
    assert!(e.contains("self-contained"), "{e}");
    let escaped = layer(
        "metersPerUnit = 1",
        r#"def Xform "Model" (references = @../outside.usda@) {}"#,
    );
    assert!(mesh(&usdz(&[("model.usda", &escaped)])).is_err());
}

#[test]
fn malformed_meshes_error_instead_of_panicking() {
    let bad = CUBE.replace(
        "int[] faceVertexIndices = [0, 1, 3, 2,",
        "int[] faceVertexIndices = [99, 1, 3, 2,",
    );
    assert!(mesh(&layer("metersPerUnit = 1", &bad)).is_err());
    let short = CUBE.replace("[4, 4, 4, 4, 4, 4]", "[4, 4, 4, 4, 4, 40]");
    assert!(mesh(&layer("metersPerUnit = 1", &short)).is_err());
    assert!(mesh(b"#usda 1.0\n(\n").is_err());
    assert!(mesh(b"PXR-USDC garbage").is_err());
}

#[test]
fn zip_without_usd_layer_stays_obj() {
    let obj = b"v 0 0 0\nv 1 0 0\nv 0 0 1\nf 1 2 3\n";
    let bytes = usdz(&[("chair.obj", obj)]);
    assert!(dimensions(&bytes).is_ok());
    assert!(!dimensions(&bytes).unwrap().physical_units);
}

/// A package whose root references an internal leaf through `path`.
fn referencing(path: &str) -> Vec<u8> {
    let root = layer(
        "metersPerUnit = 1",
        &format!(r#"def Xform "Model" (references = @{path}@</Leaf>) {{}}"#),
    );
    let leaf = layer(
        "metersPerUnit = 1",
        &format!(r#"def Xform "Leaf" {{ {CUBE} }}"#),
    );
    usdz(&[("model.usda", &root), ("leaf.usda", &leaf)])
}

#[test]
fn rooted_drive_unc_and_uri_paths_never_alias_package_entries() {
    assert!(
        mesh(&referencing("./leaf.usda")).is_ok(),
        "relative stays inside"
    );
    for path in [
        "/leaf.usda",
        r"\leaf.usda",
        r"\\server\share\leaf.usda",
        r"C:\leaf.usda",
        "C:leaf.usda",
        "file:///leaf.usda",
        "omniverse://host/leaf.usda",
    ] {
        assert!(mesh(&referencing(path)).is_err(), "{path} must be external");
    }
}

#[test]
fn package_entry_count_nesting_and_compression_are_bounded() {
    let root = layer("metersPerUnit = 1", CUBE);
    let mut entries: Vec<(String, Vec<u8>)> = vec![("model.usda".into(), root.clone())];
    for i in 0..4096 {
        entries.push((format!("pad/{i}.txt"), vec![]));
    }
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    assert!(mesh(&usdz(&refs)).err().unwrap().contains("4096"));
    assert!(
        mesh(&usdz(&refs[..4096])).is_ok(),
        "exactly 4096 entries are read"
    );

    let inner = usdz(&[("model.usda", &root)]);
    let nested = usdz(&[("model.usda", &root), ("inner.usdz", &inner)]);
    assert!(mesh(&nested).err().unwrap().contains("nested"));

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("model.usda", deflated).unwrap();
    zip.write_all(&root).unwrap();
    let compressed = zip.finish().unwrap().into_inner();
    assert!(mesh(&compressed).err().unwrap().contains("uncompressed"));
}

#[test]
fn deep_text_nesting_is_refused_before_parsing() {
    let deep = format!("{}{}", "[".repeat(129), "]".repeat(129));
    let body = format!("def Xform \"X\" {{ custom int[] v = {deep} }}");
    let e = mesh(&layer("metersPerUnit = 1", &body)).err().unwrap();
    assert!(e.contains("nests deeper"), "{e}");
    let packaged = usdz(&[("model.usda", &layer("metersPerUnit = 1", &body))]);
    assert!(mesh(&packaged).err().unwrap().contains("nests deeper"));
    // Brackets inside strings and comments do not count.
    let quoted = format!("# {deep}\ndef Xform \"X\" {{ custom string s = \"{deep}\" }}\n{CUBE}");
    assert!(mesh(&layer("metersPerUnit = 1", &quoted)).is_ok());
}

#[test]
fn instance_bindings_apply_to_prototype_geometry() {
    let body = format!(
        r#"class Xform "Proto" {{ {CUBE} }}
def Xform "Red" (instanceable = true references = </Proto> prepend apiSchemas = ["MaterialBindingAPI"]) {{
    rel material:binding = </Looks/Red>
}}
def Xform "Green" (instanceable = true references = </Proto> prepend apiSchemas = ["MaterialBindingAPI"]) {{
    double3 xformOp:translate = (3, 0, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]
    rel material:binding = </Looks/Green>
}}
def Scope "Looks" {{
    def Material "Red" {{
        token outputs:surface.connect = </Looks/Red/S.outputs:surface>
        def Shader "S" {{ uniform token info:id = "UsdPreviewSurface"
            color3f inputs:diffuseColor = (1, 0, 0) token outputs:surface }}
    }}
    def Material "Green" {{
        token outputs:surface.connect = </Looks/Green/S.outputs:surface>
        def Shader "S" {{ uniform token info:id = "UsdPreviewSurface"
            color3f inputs:diffuseColor = (0, 1, 0) token outputs:surface }}
    }}
}}"#
    );
    let m = mesh(&layer("metersPerUnit = 1", &body)).unwrap();
    let keys: Vec<_> = m.materials.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["usd:/Looks/Green", "usd:/Looks/Red"]);
}

#[test]
fn corner_display_colours_are_averaged_per_triangle() {
    let body = r#"def Mesh "Tri" {
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 0, 1)]
    color3f[] primvars:displayColor = [(1, 0, 0), (0, 1, 0), (0, 0, 1)] (interpolation = "vertex")
}"#;
    let m = mesh(&layer("metersPerUnit = 1", body)).unwrap();
    let rgb = &m.vertices[6..9];
    assert!(
        rgb.iter().all(|c| (c - rgb[0]).abs() < 1e-6 && *c > 0.5),
        "{rgb:?}"
    );
}

#[test]
fn package_relative_paths_are_confined_too() {
    let leaf = layer(
        "metersPerUnit = 1",
        &format!(r#"def Xform "Leaf" {{ {CUBE} }}"#),
    );
    for (path, entry) in [
        ("model.usdz[/leaf.usda]", "/leaf.usda"),
        ("model.usdz[../leaf.usda]", "leaf.usda"),
    ] {
        let root = layer(
            "metersPerUnit = 1",
            &format!(r#"def Xform "Model" (references = @{path}@</Leaf>) {{}}"#),
        );
        assert!(
            mesh(&usdz(&[("model.usda", &root), (entry, &leaf)])).is_err(),
            "{path}"
        );
    }
}

#[test]
fn leading_whitespace_does_not_hide_a_text_layer() {
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let body = format!(
        "{}#usda 1.0\ndef Xform \"X\" {{ custom int[] v = {deep} }}",
        " ".repeat(64)
    );
    let e = mesh(&usdz(&[("model.usda", body.as_bytes())]))
        .err()
        .unwrap();
    assert!(e.contains("nests deeper"), "{e}");
    let loose = format!(
        "\n\n{}",
        String::from_utf8(layer("metersPerUnit = 1", CUBE)).unwrap()
    );
    assert_eq!(mesh(loose.as_bytes()).unwrap().source_triangles, 12);
}

#[test]
fn layer_count_and_prim_count_are_bounded_before_parsing() {
    // A chain of sublayers recurses in the reader; 129 layers are refused.
    let mut entries = Vec::new();
    for i in 0..129 {
        let next = if i < 128 {
            format!("subLayers = [@l{}.usda@]", i + 1)
        } else {
            String::new()
        };
        let name = if i == 0 {
            "model.usda".to_owned()
        } else {
            format!("l{i}.usda")
        };
        entries.push((name, format!("#usda 1.0\n(\n{next}\n)\n").into_bytes()));
    }
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    assert!(mesh(&usdz(&refs)).err().unwrap().contains("layers"));

    let many: String = (0..100_001)
        .map(|i| format!("def \"P{i}\" {{}}\n"))
        .collect();
    let e = mesh(&layer("metersPerUnit = 1", &many)).err().unwrap();
    assert!(e.contains("too many prims"), "{e}");
}

#[test]
fn texture_driven_opacity_and_scale_are_read() {
    let root = layer(
        "metersPerUnit = 1",
        r#"
def Mesh "Quad" (prepend apiSchemas = ["MaterialBindingAPI"]) {
    int[] faceVertexCounts = [4]
    int[] faceVertexIndices = [0, 1, 2, 3]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)]
    texCoord2f[] primvars:st = [(0, 0), (1, 0), (1, 1), (0, 1)] (interpolation = "vertex")
    rel material:binding = </Looks/Glass>
}
def Scope "Looks" {
    def Material "Glass" {
        token outputs:surface.connect = </Looks/Glass/S.outputs:surface>
        def Shader "S" {
            uniform token info:id = "UsdPreviewSurface"
            color3f inputs:diffuseColor.connect = </Looks/Glass/T.outputs:rgb>
            float inputs:opacity.connect = </Looks/Glass/T.outputs:a>
            token outputs:surface
        }
        def Shader "T" {
            uniform token info:id = "UsdUVTexture"
            asset inputs:file = @paint.png@
            float4 inputs:scale = (1, 1, 1, 0.25)
            float3 outputs:rgb
            float outputs:a
        }
    }
}
"#,
    );
    let texture = png();
    let m = mesh(&usdz(&[("model.usda", &root), ("paint.png", &texture)])).unwrap();
    assert_eq!(m.materials[0].color[3], 0.25);
}

#[test]
fn directory_records_aliasing_one_payload_are_refused() {
    let root = layer("metersPerUnit = 1", CUBE);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("model.usda", stored).unwrap();
    zip.write_all(&root).unwrap();
    for i in 0..3 {
        zip.shallow_copy_file("model.usda", &format!("alias{i}.txt"))
            .unwrap();
    }
    let aliased = zip.finish().unwrap().into_inner();
    let e = mesh(&aliased).err().unwrap();
    assert!(e.contains("overlap"), "{e}");
}
