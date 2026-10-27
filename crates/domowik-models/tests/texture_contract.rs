mod support;
use image::ImageFormat;
use serde_json::json;

#[test]
fn solid_override_suppresses_only_its_texture_and_preserves_opacity() {
    use domowik_format::appearance::{MaterialColors, RgbColor};
    let (mut j, bin) = support::fixture(ImageFormat::Png, false);
    j["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] = json!([1., 1., 1., 0.5]);
    let mut second = j["materials"][0].clone();
    second["name"] = "duplicate name".into();
    j["materials"].as_array_mut().unwrap().push(second);
    let mut p = j["meshes"][0]["primitives"][0].clone();
    p["material"] = 1.into();
    j["meshes"][0]["primitives"].as_array_mut().unwrap().push(p);
    let bytes = support::pack(&j, &bin);
    let source = domowik_models::mesh(&bytes).unwrap();
    let colored = source.recolored(&MaterialColors::from([(
        "gltf:0".into(),
        RgbColor([12, 34, 56]),
    )]));
    for (i, tri) in colored.vertices.as_chunks::<30>().0.iter().enumerate() {
        let overridden = source.materials[source.triangle_materials[i]].key == "gltf:0";
        assert_eq!(tri[9], 0.5);
        for uv in colored.texture.as_ref().unwrap().coordinates[i * 15..i * 15 + 15]
            .as_chunks::<5>()
            .0
        {
            assert_eq!(uv[2] < 0., overridden);
        }
    }
    assert_eq!(
        source.texture.as_ref().unwrap().rgba,
        colored.texture.as_ref().unwrap().rgba
    );
    assert_eq!(source.materials.len(), 2);
}

#[test]
fn embedded_formats_and_draco_uvs_render_quadrants_and_survive_lod() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
        for draco in [false, true] {
            let (j, bin) = support::fixture(format, draco);
            let bytes = support::pack(&j, &bin);
            let preview = domowik_models::preview(&bytes).unwrap();
            let angled = domowik_models::thumbnail(&bytes).unwrap();
            let thumbnail = tiny_skia::Pixmap::decode_png(&angled.png).unwrap();
            for channel in 0..3 {
                assert!(thumbnail
                    .data()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] != 0 && p[channel] > 180 && p[(channel + 1) % 3] < 50));
            }
            let png = tiny_skia::Pixmap::decode_png(&preview.png).unwrap();
            for (x, y, channel) in [(32, 32, 0), (220, 32, 1), (32, 220, 2)] {
                let pixel = &png.data()[(y * 256 + x) * 4..][..4];
                assert!(
                    pixel[channel] > 200 && pixel[(channel + 1) % 3] < 50,
                    "{format:?}, Draco={draco}, {pixel:?}"
                );
            }
            let mesh = domowik_models::mesh_with_budget(&bytes, 100).unwrap();
            let t = mesh.texture.unwrap();
            assert_eq!((t.width, t.height, t.layers), (8, 8, 1));
            assert_eq!(t.coordinates.len(), mesh.triangles * 15);
            assert_eq!(t.rgba.len(), 8 * 8 * 4);
        }
    }
}

#[test]
fn factors_transform_uvs_and_repeat_modes_are_applied() {
    let (mut j, bin) = support::fixture(ImageFormat::Png, false);
    j["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] = json!([0.25, 1, 1, 1]);
    j["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["extensions"] =
        json!({"KHR_texture_transform":{"offset":[1.5,0],"scale":[0,0]}});
    for (wrap, channel) in [(10497, 0), (33071, 1), (33648, 0)] {
        j["samplers"][0]["wrapS"] = wrap.into();
        // 1.5 is the seam; choose 1.25 for repeat red / clamp green / mirror green.
        j["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["extensions"]
            ["KHR_texture_transform"]["offset"] =
            json!([if wrap == 33648 { 2.25 } else { 1.25 }, 0.25]);
        let p = domowik_models::preview(&support::pack(&j, &bin)).unwrap();
        let image = tiny_skia::Pixmap::decode_png(&p.png).unwrap();
        let px = &image.data()[(32 * 256 + 32) * 4..][..4];
        if channel == 0 {
            assert!((px[0] as i32 - 137).abs() <= 1, "{px:?}");
        } else {
            assert_eq!(px[1], 255);
        }
    }
}

#[test]
fn referenced_external_corrupt_and_oversized_images_fail_closed() {
    let (j, bin) = support::fixture(ImageFormat::Png, false);
    let mut external = j.clone();
    external["images"][0]["uri"] = "https://example.invalid/image.png".into();
    assert!(domowik_models::preview(&support::pack(&external, &bin))
        .err()
        .unwrap()
        .contains("not fetched"));
    let view = j["images"][0]["bufferView"].as_u64().unwrap() as usize;
    let start = j["bufferViews"][view]["byteOffset"].as_u64().unwrap() as usize;
    let mut corrupt = bin.clone();
    corrupt[start] = 0;
    assert!(domowik_models::preview(&support::pack(&j, &corrupt)).is_err());
    // A valid, very compressible oversized PNG proves this is a dimension
    // refusal, not merely a corrupt IHDR/CRC diagnostic.
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::RgbaImage::new(5000, 1)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    let mut huge = bin[..start].to_vec();
    huge.extend(encoded.get_ref());
    let mut oversized = j.clone();
    oversized["bufferViews"][view]["byteLength"] = encoded.get_ref().len().into();
    oversized["buffers"][0]["byteLength"] = huge.len().into();
    assert!(domowik_models::preview(&support::pack(&oversized, &huge))
        .err()
        .unwrap()
        .to_lowercase()
        .contains("limit"));
    let mut missing = j.clone();
    missing["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = serde_json::Value::Null;
    assert!(domowik_models::mesh(&support::pack(&missing, &bin)).is_err());
    let mut nan = bin.clone();
    let uv = j["bufferViews"][1]["byteOffset"].as_u64().unwrap() as usize;
    nan[uv..uv + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(domowik_models::mesh(&support::pack(&j, &nan)).is_err());
}

#[test]
fn normalized_strided_uvs_and_secondary_uv_transforms_are_supported() {
    let (j, bin) = support::normalized_draco_fixture();
    let preview = domowik_models::preview(&support::pack(&j, &bin)).unwrap();
    let image = tiny_skia::Pixmap::decode_png(&preview.png).unwrap();
    assert_eq!(&image.data()[(32 * 256 + 32) * 4..][..4], &[255, 0, 0, 255]);
    assert_eq!(
        &image.data()[(220 * 256 + 32) * 4..][..4],
        &[0, 0, 255, 255]
    );
    let (mut j, mut bin) = support::fixture(ImageFormat::Png, false);
    let uv_start = j["bufferViews"][1]["byteOffset"].as_u64().unwrap() as usize;
    j["bufferViews"][1]["byteStride"] = 8.into();
    j["accessors"][1]["componentType"] = 5121.into();
    j["accessors"][1]["normalized"] = true.into();
    j["accessors"][1]["byteOffset"] = 4.into();
    for (i, uv) in [[0u8, 0], [255, 0], [0, 255], [255, 255]]
        .iter()
        .enumerate()
    {
        bin[uv_start + i * 8 + 4..uv_start + i * 8 + 6].copy_from_slice(uv);
    }
    let attributes = j["meshes"][0]["primitives"][0]["attributes"]
        .as_object_mut()
        .unwrap();
    let accessor = attributes.remove("TEXCOORD_0").unwrap();
    attributes.insert("TEXCOORD_1".into(), accessor);
    j["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["extensions"] = json!({"KHR_texture_transform":{"texCoord":1,"rotation":std::f64::consts::FRAC_PI_2,"offset":[1,0]}});
    let bytes = support::pack(&j, &bin);
    let preview = domowik_models::preview(&bytes).unwrap();
    let image = tiny_skia::Pixmap::decode_png(&preview.png).unwrap();
    assert_eq!(&image.data()[(32 * 256 + 32) * 4..][..4], &[0, 255, 0, 255]);
    let mesh = domowik_models::mesh(&bytes).unwrap();
    assert!(mesh
        .texture
        .unwrap()
        .coordinates
        .iter()
        .all(|v| v.is_finite()));
}

#[test]
fn multiple_materials_keep_their_layers_and_untextured_surfaces() {
    let (mut j, bin) = support::fixture(ImageFormat::Png, false);
    let image = j["images"][0].clone();
    j["images"].as_array_mut().unwrap().push(image);
    j["textures"]
        .as_array_mut()
        .unwrap()
        .push(json!({"source":1}));
    j["materials"]
        .as_array_mut()
        .unwrap()
        .push(json!({"pbrMetallicRoughness":{"baseColorTexture":{"index":1}}}));
    let mut primitive = j["meshes"][0]["primitives"][0].clone();
    primitive["material"] = 1.into();
    j["meshes"][0]["primitives"]
        .as_array_mut()
        .unwrap()
        .push(primitive.clone());
    primitive.as_object_mut().unwrap().remove("material");
    j["meshes"][0]["primitives"]
        .as_array_mut()
        .unwrap()
        .push(primitive);
    let mesh = domowik_models::mesh(&support::pack(&j, &bin)).unwrap();
    let texture = mesh.texture.unwrap();
    assert_eq!(texture.layers, 2);
    let layers: std::collections::BTreeSet<_> = texture
        .coordinates
        .as_chunks::<5>()
        .0
        .iter()
        .map(|v| v[2] as i32)
        .collect();
    assert_eq!(layers, [-1, 0, 1].into_iter().collect());
}

#[test]
fn too_many_referenced_images_are_refused() {
    let (mut j, bin) = support::fixture(ImageFormat::Png, false);
    let image = j["images"][0].clone();
    let primitive = j["meshes"][0]["primitives"][0].clone();
    for i in 1..17 {
        j["images"].as_array_mut().unwrap().push(image.clone());
        j["textures"]
            .as_array_mut()
            .unwrap()
            .push(json!({"source":i}));
        j["materials"]
            .as_array_mut()
            .unwrap()
            .push(json!({"pbrMetallicRoughness":{"baseColorTexture":{"index":i}}}));
        let mut p = primitive.clone();
        p["material"] = i.into();
        j["meshes"][0]["primitives"].as_array_mut().unwrap().push(p);
    }
    assert!(domowik_models::mesh(&support::pack(&j, &bin))
        .err()
        .unwrap()
        .contains("budget"));
}
