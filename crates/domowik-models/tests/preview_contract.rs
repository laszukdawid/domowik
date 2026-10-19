use domowik_models::preview;
use std::io::{Cursor, Write};

fn image(bytes: &[u8]) -> tiny_skia::Pixmap {
    tiny_skia::Pixmap::decode_png(&preview(bytes).unwrap().png).unwrap()
}
fn pixel(image: &tiny_skia::Pixmap, x: usize, y: usize) -> &[u8] {
    &image.data()[(y * 256 + x) * 4..(y * 256 + x + 1) * 4]
}

#[test]
fn library_thumbnail_is_angled_padded_deterministic_and_keeps_original_extents() {
    // Original vertical facade: invisible from above, recognizable at an angle.
    let wall = b"v 0 0 0\nv 4 0 0\nv 4 2 1\nv 0 2 1\nf 1 2 3 4\n";
    let p = domowik_models::thumbnail(wall).unwrap();
    assert_eq!(p.model_extents, [4., 2., 1.]);
    assert_eq!(p.triangles, 2);
    assert!(p.covered_pixels > 1000);
    assert_eq!(p.png, domowik_models::thumbnail(wall).unwrap().png);
    assert_ne!(p.png, preview(wall).unwrap().png);
    let image = tiny_skia::Pixmap::decode_png(&p.png).unwrap();
    for x in 0..256 {
        for y in [0, 7, 248, 255] {
            assert_eq!(pixel(&image, x, y)[3], 0);
            assert_eq!(pixel(&image, y, x)[3], 0);
        }
    }
    let glb = domowik_models::thumbnail(&original_glb()).unwrap();
    assert_eq!(glb.model_extents, [4., 0., 6.]);
    assert!(glb.covered_pixels > 0);
    assert!(domowik_models::thumbnail(b"not a model").is_err());
}

#[test]
fn library_thumbnail_preserves_aspect_ratio_and_shows_vertical_surfaces() {
    // A tall narrow prism. Its thumbnail must not be stretched to a square.
    let prism = b"v 0 0 0\nv 1 0 0\nv 1 10 0\nv 0 10 0\nv 0 0 1\nv 1 0 1\nv 1 10 1\nv 0 10 1\nf 1 2 3 4\nf 5 6 7 8\nf 1 5 8 4\nf 2 6 7 3\nf 4 3 7 8\n";
    let p = domowik_models::thumbnail(prism).unwrap();
    let image = tiny_skia::Pixmap::decode_png(&p.png).unwrap();
    let occupied: Vec<_> = (0..256)
        .flat_map(|y| (0..256).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&image, x, y)[3] != 0)
        .collect();
    let width =
        occupied.iter().map(|p| p.0).max().unwrap() - occupied.iter().map(|p| p.0).min().unwrap();
    let height =
        occupied.iter().map(|p| p.1).max().unwrap() - occupied.iter().map(|p| p.1).min().unwrap();
    assert!(height > width * 4, "{width} by {height}");
    let vertical = b"v 0 0 1\nv 1 0 0\nv 1 2 0\nv 0 2 1\nf 1 2 3 4\n";
    assert!(preview(vertical).is_err());
    assert!(domowik_models::thumbnail(vertical).unwrap().covered_pixels > 0);
}

#[test]
fn a_concave_original_obj_renders_its_shape_not_its_bounding_rectangle() {
    let mesh = b"v 0 1 0\nv 3 1 0\nv 3 1 1\nv 1 1 1\nv 1 1 3\nv 0 1 3\nf 3 4 5 6 1 2\n";
    let p = preview(mesh).unwrap();
    assert_eq!(p.triangles, 4);
    let i = image(mesh);
    assert_eq!(
        pixel(&i, 200, 200)[3],
        0,
        "the L-shaped gap stays transparent"
    );
    assert_eq!(pixel(&i, 20, 200)[3], 255);
    assert_eq!(pixel(&i, 200, 20)[3], 255);
    assert!(p.covered_pixels < 50000);
    let reversed = b"v 0 1 0\nv 3 1 0\nv 3 1 1\nv 1 1 1\nv 1 1 3\nv 0 1 3\nf -1 -2 -3 -4 -5 -6\n";
    assert_eq!(
        image(reversed).data(),
        i.data(),
        "negative indices and winding do not change the art"
    );
}

#[test]
fn zip_materials_and_depth_preserve_the_upper_visible_surface() {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("original.obj", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"mtllib original.mtl\nv 0 0 0\nv 2 0 0\nv 2 0 2\nv 0 0 2\nv 0.5 1 0.5\nv 1.5 1 0.5\nv 1.5 1 1.5\nv 0.5 1 1.5\nusemtl red\nf 1 2 3 4\nusemtl blue\nf 5 6 7 8\n").unwrap();
    zip.start_file("original.mtl", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"newmtl red\nKd 1 0 0\nnewmtl blue\nKd 0 0 1\n")
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let i = image(&bytes);
    assert_eq!(pixel(&i, 128, 128), [0, 0, 255, 255]);
    assert_eq!(pixel(&i, 10, 10), [255, 0, 0, 255]);
}

fn original_glb() -> Vec<u8> {
    let positions = [[0f32, 1., 0.], [2., 1., 0.], [0., 1., 2.]];
    let bin: Vec<u8> = positions
        .iter()
        .flatten()
        .flat_map(|p| p.to_le_bytes())
        .collect();
    let j = serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":bin.len()}],
        "bufferViews":[{"buffer":0,"byteLength":bin.len()}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
        "nodes":[{"translation":[10,20,30],"children":[1]},{"mesh":0,"scale":[2,1,3]}],
        "scenes":[{"nodes":[0]}],"scene":0});
    let mut json = serde_json::to_vec(&j).unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2u32.to_le_bytes());
    bytes.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(0x4e4f534au32.to_le_bytes());
    bytes.extend(json);
    bytes.extend((bin.len() as u32).to_le_bytes());
    bytes.extend(0x004e4942u32.to_le_bytes());
    bytes.extend(bin);
    bytes
}

#[test]
fn glb_uses_geometry_and_the_full_node_transform_chain() {
    let p = preview(&original_glb()).unwrap();
    assert_eq!(p.triangles, 1);
    assert_eq!(p.model_extents, [4., 0., 6.]);
    let i = tiny_skia::Pixmap::decode_png(&p.png).unwrap();
    assert_eq!(pixel(&i, 230, 230)[3], 0);
    assert_eq!(pixel(&i, 20, 20)[3], 255);
}

#[test]
fn initial_dimensions_use_physical_units_or_preserve_unitless_proportions() {
    let glb = domowik_models::dimensions(&original_glb()).unwrap();
    assert!(glb.physical_units);
    assert_eq!(glb.dimensions_mm, [4000., 6000., 1.]);
    let mesh = b"v 0 0 0\nv 4 2 0\nv 0 0 1\nf 1 2 3\n";
    let obj = domowik_models::dimensions(mesh).unwrap();
    assert!(!obj.physical_units);
    assert_eq!(obj.dimensions_mm, [1000., 250., 500.]);
    assert_eq!(obj.height_ratio, 0.5);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("original.obj", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(mesh).unwrap();
    let zipped = domowik_models::dimensions(&zip.finish().unwrap().into_inner()).unwrap();
    assert!(!zipped.physical_units);
    assert_eq!(zipped.dimensions_mm, obj.dimensions_mm);
    for invalid in [
        b"opaque model".as_slice(),
        b"v 0 0 0\nv 0 1 0\nv 0 2 0\nf 1 2 3\n",
    ] {
        assert!(domowik_models::dimensions(invalid).is_err());
    }
}

#[test]
fn malformed_input_is_an_error_without_partial_art_or_a_panic() {
    for bytes in [
        b"not a model".as_slice(),
        b"v NaN 0 0\nf 1 2 3",
        b"v 0 0 0\nf 0 1 2",
        b"PK malformed",
        b"glTF",
    ] {
        assert!(preview(bytes).is_err());
    }
    let bytes = original_glb();
    for end in 0..bytes.len() {
        assert!(preview(&bytes[..end]).is_err(), "truncated GLB at {end}");
    }
}

#[test]
fn gpu_lod_is_bounded_normalized_and_preserves_original_bytes() {
    let bytes = b"v -2 0 -1\nv 2 0 -1\nv 0 2 1\nf 1 2 3\n";
    let mesh = domowik_models::mesh(bytes).unwrap();
    assert_eq!(mesh.source_triangles, 1);
    assert_eq!(mesh.height_ratio, 0.5);
    assert!(mesh.triangles <= 20000);
    assert_eq!(mesh.vertices.len(), mesh.triangles * 30);
    for v in mesh.vertices.as_chunks::<10>().0 {
        assert!(v.iter().all(|v| v.is_finite()));
        assert!((-0.5..=0.5).contains(&v[0]));
        assert!((-0.5..=0.5).contains(&v[2]));
    }
    assert_eq!(
        mesh.vertices,
        domowik_models::mesh(bytes).unwrap().vertices,
        "LOD is deterministic and reusable"
    );
}
