//! Original synthetic quadrant texture and quad; no catalog content.
use serde_json::{json, Value};
use std::io::Cursor;

pub fn pack(j: &Value, bin: &[u8]) -> Vec<u8> {
    let mut json = serde_json::to_vec(j).unwrap();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut bin = bin.to_vec();
    bin.resize(bin.len().next_multiple_of(4), 0);
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(0x4e4f534au32.to_le_bytes());
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(0x004e4942u32.to_le_bytes());
    out.extend(bin);
    out
}

pub fn fixture(format: image::ImageFormat, draco: bool) -> (Value, Vec<u8>) {
    fixture_with_uv(format, draco, false)
}

#[allow(dead_code)] // Used by texture tests, not the shared benchmark/bridge fixture.
pub fn normalized_draco_fixture() -> (Value, Vec<u8>) {
    fixture_with_uv(image::ImageFormat::Png, true, true)
}

fn fixture_with_uv(format: image::ImageFormat, draco: bool, normalized: bool) -> (Value, Vec<u8>) {
    let pixels = image::RgbaImage::from_fn(8, 8, |x, y| {
        image::Rgba(match (x < 4, y < 4) {
            (true, true) => [255, 0, 0, 255],
            (false, true) => [0, 255, 0, 255],
            (true, false) => [0, 0, 255, 255],
            (false, false) => [255, 255, 0, 255],
        })
    });
    let mut encoded = Cursor::new(Vec::new());
    let pixels = image::DynamicImage::ImageRgba8(pixels);
    if format == image::ImageFormat::Jpeg {
        pixels.to_rgb8().write_to(&mut encoded, format).unwrap();
    } else {
        pixels.write_to(&mut encoded, format).unwrap();
    }
    let positions = [[0f32, 1., 0.], [1., 1., 0.], [0., 1., 1.], [1., 1., 1.]];
    let uv = [[0f32, 0.], [1., 0.], [0., 1.], [1., 1.]];
    let mut bin = Vec::new();
    let mut views = Vec::new();
    let mut push = |data: &[u8]| {
        bin.resize(bin.len().next_multiple_of(4), 0);
        let id = views.len();
        views.push(json!({"buffer":0,"byteOffset":bin.len(),"byteLength":data.len()}));
        bin.extend(data);
        id
    };
    let mut accessors = json!([
        {"componentType":5126,"type":"VEC3","count":4},
        {"componentType":5126,"type":"VEC2","count":4}
    ]);
    let primitive = if draco {
        let mut mesh = draco_core::Mesh::new();
        for (kind, components, data) in [
            (
                draco_core::GeometryAttributeType::Position,
                3,
                positions.iter().flatten().copied().collect::<Vec<_>>(),
            ),
            (
                draco_core::GeometryAttributeType::TexCoord,
                2,
                uv.iter().flatten().copied().collect(),
            ),
        ] {
            let mut a = draco_core::PointAttribute::new();
            let integer_uv =
                normalized && matches!(kind, draco_core::GeometryAttributeType::TexCoord);
            a.init(
                kind,
                components,
                if integer_uv {
                    draco_core::DataType::Uint16
                } else {
                    draco_core::DataType::Float32
                },
                false,
                4,
            );
            for (i, v) in data.iter().enumerate() {
                if integer_uv {
                    a.buffer_mut()
                        .write(i * 2, &((*v * 65535.) as u16).to_le_bytes());
                } else {
                    a.buffer_mut().write(i * 4, &v.to_le_bytes());
                }
            }
            mesh.add_attribute(a);
        }
        mesh.set_num_faces(2);
        mesh.set_face(
            draco_core::FaceIndex(0),
            [0u32.into(), 1u32.into(), 2u32.into()],
        );
        mesh.set_face(
            draco_core::FaceIndex(1),
            [1u32.into(), 3u32.into(), 2u32.into()],
        );
        let mut encoder = draco_core::MeshEncoder::new();
        encoder.set_mesh(mesh);
        let mut encoded = draco_core::EncoderBuffer::new();
        let mut options = draco_core::EncoderOptions::new();
        options.set_global_int("encoding_speed", 10);
        encoder.encode(&options, &mut encoded).unwrap();
        let view = push(encoded.data());
        if normalized {
            accessors[1]["componentType"] = 5123.into();
            accessors[1]["normalized"] = true.into();
        }
        json!({"material":0,"attributes":{"POSITION":0,"TEXCOORD_0":1},"extensions":{"KHR_draco_mesh_compression":{"bufferView":view,"attributes":{"POSITION":0,"TEXCOORD_0":1}}}})
    } else {
        accessors[0]["bufferView"] = push(
            &positions
                .iter()
                .flatten()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        )
        .into();
        accessors[1]["bufferView"] = push(
            &uv.iter()
                .flatten()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        )
        .into();
        let view = push(
            &[0u16, 1, 2, 1, 3, 2]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        accessors
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":view,"componentType":5123,"type":"SCALAR","count":6}));
        json!({"material":0,"attributes":{"POSITION":0,"TEXCOORD_0":1},"indices":2})
    };
    let image_view = push(encoded.get_ref());
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        _ => "image/webp",
    };
    let texture = if format == image::ImageFormat::WebP {
        json!({"extensions":{"EXT_texture_webp":{"source":0}},"sampler":0})
    } else {
        json!({"source":0,"sampler":0})
    };
    let j = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":bin.len()}],"bufferViews":views,"accessors":accessors,
        "images":[{"mimeType":mime,"bufferView":image_view}],"textures":[texture],"samplers":[{"wrapS":33071,"wrapT":33071}],
        "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],
        "meshes":[{"primitives":[primitive]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}]});
    (j, bin)
}
