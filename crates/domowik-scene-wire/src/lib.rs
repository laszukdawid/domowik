//! Byte transport for one derived 3D scene (issue 0088).
//!
//! In the browser the document core and the GPU renderer are separate
//! WebAssembly modules with separate memories, so a scene the core derives
//! must cross as bytes. This crate is that format, encoder and decoder
//! together, so the two sides cannot drift. It knows nothing about documents
//! or GPUs: meshes are opaque `f32` vertex arrays and instances are matrices.
//!
//! # Layout (little-endian, versions 1 and 2)
//!
//! | Bytes | Content |
//! |---|---|
//! | 4 | magic `DSW1` (colour-only) or `DSW2` (textured) |
//! | 4 | `u32` header length `h` |
//! | `h` | UTF-8 JSON [`Header`] |
//! | 0-3 | zero padding to a multiple of 4 |
//! | `4 * Σ meshFloats` | each mesh's `f32` vertices, in header order |
//! | `INSTANCE_BYTES * instances` | each instance: `u32` mesh index, `u32` flags (bit 0 transparent), then [`INSTANCE_FLOATS`] `f32`s: matrix (16), normal matrix (9), colour (4) |
//!
//! DSW1 ends at the instance table. DSW2 appends the bounded texture/UV
//! payloads described by [`TextureHeader`]. [`decode`] rejects a wrong magic, a
//! malformed header, a mesh index out of range, a non-finite float, a length
//! that disagrees with the header, and trailing bytes; it never panics on
//! input. The vertex layout inside a mesh is the renderer's concern.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The four bytes every frame begins with.
pub const MAGIC: [u8; 4] = *b"DSW1";
/// Texture-bearing frames use DSW2; DSW1 emission/decoding is unchanged.
pub const TEXTURED_MAGIC: [u8; 4] = *b"DSW2";

/// Derived, top-row-first sRGB RGBA8 array and five floats per vertex:
/// UV, layer (-1 for untextured), wrap S/T (0 repeat, 1 clamp, 2 mirror).
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub rgba: Vec<u8>,
    pub coordinates: Vec<f32>,
}

/// DSW2 payload descriptor. After the instance table, each descriptor carries
/// `coordinate_floats * 4` little-endian float bytes then width*height*layers*4
/// RGBA bytes. Both are naturally four-byte aligned. No bytes may follow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureHeader {
    pub mesh: String,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub coordinate_floats: u64,
}

fn texture_size(w: u32, h: u32, layers: u32) -> Result<usize, String> {
    if w == 0 || h == 0 || w > 2048 || h > 2048 || layers == 0 || layers > 16 {
        return Err("Invalid scene texture dimensions".into());
    }
    let n = u64::from(w) * u64::from(h) * u64::from(layers) * 4;
    if n > 64 * 1024 * 1024 {
        return Err("Scene texture exceeds 64 MiB".into());
    }
    Ok(n as usize)
}
/// Floats per instance after its two `u32` words.
pub const INSTANCE_FLOATS: usize = 29;
/// Bytes per instance record.
pub const INSTANCE_BYTES: usize = 8 + 4 * INSTANCE_FLOATS;

/// Scene-level facts, carried as JSON because they are small and varied.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<TextureHeader>,
    /// Plan millimetres of the world origin the geometry was rebased to.
    pub origin_mm: [f64; 2],
    /// Bounding-sphere centre in world metres.
    pub center: [f32; 3],
    /// Bounding-sphere radius in world metres.
    pub radius: f32,
    /// Non-blocking derivation diagnostics, for display.
    pub diagnostics: Vec<String>,
    /// Mesh keys, in data order.
    pub meshes: Vec<String>,
    /// Float count of each mesh, parallel to `meshes`.
    pub mesh_floats: Vec<u64>,
    /// Number of instance records.
    pub instances: u64,
    /// Producer extras (for example model-decode counters); opaque here.
    #[serde(default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// One placed mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// Index into the frame's meshes.
    pub mesh: u32,
    pub transparent: bool,
    pub matrix: [f32; 16],
    pub normal_matrix: [f32; 9],
    pub color: [f32; 4],
}

/// A decoded frame. Mesh data is copied out of the input.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub header: Header,
    /// `(key, vertices)`, in header order.
    pub meshes: Vec<(String, Vec<f32>)>,
    pub instances: Vec<Instance>,
    pub textures: BTreeMap<String, Texture>,
}

/// Encodes a frame. `header.meshes`, `header.mesh_floats` and
/// `header.instances` are overwritten from `meshes` and `instances`.
///
/// Fails only if a mesh index is out of range or the header is too large for
/// its `u32` length field; floats are written as given.
pub fn encode<'a>(
    header: Header,
    meshes: impl IntoIterator<Item = (&'a str, &'a [f32])>,
    instances: &[Instance],
) -> Result<Vec<u8>, String> {
    encode_textured(header, meshes, instances, &BTreeMap::new())
}

/// Encode a scene including bounded texture arrays. Old readers reject the
/// DSW2 magic, rather than silently dropping textures. No document format or
/// C ABI change. Texture bytes are copied; callers retain ownership.
pub fn encode_textured<'a>(
    mut header: Header,
    meshes: impl IntoIterator<Item = (&'a str, &'a [f32])>,
    instances: &[Instance],
    textures: &BTreeMap<String, Texture>,
) -> Result<Vec<u8>, String> {
    let meshes: Vec<(&str, &[f32])> = meshes.into_iter().collect();
    header.textures.clear();
    let mut total = 0;
    for (key, t) in textures {
        let mesh = meshes
            .iter()
            .find(|(k, _)| *k == key)
            .ok_or("Texture references missing mesh")?;
        let size = texture_size(t.width, t.height, t.layers)?;
        total += size;
        if total > 128 * 1024 * 1024
            || t.rgba.len() != size
            || mesh.1.len() % 30 != 0
            || t.coordinates.len() != mesh.1.len() / 10 * 5
            || t.coordinates.iter().any(|v| !v.is_finite())
        {
            return Err("Invalid scene texture payload".into());
        }
        header.textures.push(TextureHeader {
            mesh: key.clone(),
            width: t.width,
            height: t.height,
            layers: t.layers,
            coordinate_floats: t.coordinates.len() as u64,
        });
    }
    header.meshes = meshes.iter().map(|(k, _)| (*k).to_owned()).collect();
    header.mesh_floats = meshes.iter().map(|(_, v)| v.len() as u64).collect();
    header.instances = instances.len() as u64;
    if let Some(bad) = instances.iter().find(|i| i.mesh as usize >= meshes.len()) {
        return Err(format!("instance mesh index {} is out of range", bad.mesh));
    }
    let json = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    let json_len = u32::try_from(json.len()).map_err(|_| "scene header is too large")?;
    let floats: usize = meshes.iter().map(|(_, v)| v.len()).sum();
    let mut out =
        Vec::with_capacity(8 + json.len() + 3 + 4 * floats + INSTANCE_BYTES * instances.len());
    out.extend_from_slice(if textures.is_empty() {
        &MAGIC
    } else {
        &TEXTURED_MAGIC
    });
    out.extend_from_slice(&json_len.to_le_bytes());
    out.extend_from_slice(&json);
    out.resize(out.len().next_multiple_of(4), 0);
    for (_, vertices) in &meshes {
        for v in *vertices {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for i in instances {
        out.extend_from_slice(&i.mesh.to_le_bytes());
        out.extend_from_slice(&u32::from(i.transparent).to_le_bytes());
        for v in i.matrix.iter().chain(&i.normal_matrix).chain(&i.color) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for t in textures.values() {
        for v in &t.coordinates {
            out.extend(v.to_le_bytes());
        }
        out.extend(&t.rgba);
    }
    Ok(out)
}

/// Decodes and validates a frame. See the crate documentation for what is
/// rejected.
pub fn decode(bytes: &[u8]) -> Result<Frame, String> {
    let mut at = Reader { bytes, at: 0 };
    let magic = at.take(4)?;
    if magic != MAGIC && magic != TEXTURED_MAGIC {
        return Err("not a scene frame".into());
    }
    let json_len = at.u32()? as usize;
    if json_len > 16 * 1024 * 1024 {
        return Err("Scene header exceeds 16 MiB".into());
    }
    let header: Header =
        serde_json::from_slice(at.take(json_len)?).map_err(|e| format!("scene header: {e}"))?;
    at.take(at.at.next_multiple_of(4) - at.at)?;
    if header.meshes.len() != header.mesh_floats.len() {
        return Err("scene header mesh lists disagree".into());
    }
    let mut meshes = Vec::with_capacity(header.meshes.len());
    for (key, &count) in header.meshes.iter().zip(&header.mesh_floats) {
        let count = usize::try_from(count).map_err(|_| "mesh is too large")?;
        let raw = at.take(count.checked_mul(4).ok_or("mesh is too large")?)?;
        meshes.push((key.clone(), floats(raw)?));
    }
    let count = usize::try_from(header.instances).map_err(|_| "too many instances")?;
    let table = at.take(
        count
            .checked_mul(INSTANCE_BYTES)
            .ok_or("too many instances")?,
    )?;
    let mut instances = Vec::with_capacity(count);
    for record in table.as_chunks::<INSTANCE_BYTES>().0 {
        let mesh = u32::from_le_bytes(record[0..4].try_into().expect("4 bytes"));
        if mesh as usize >= meshes.len() {
            return Err(format!("instance mesh index {mesh} is out of range"));
        }
        let flags = u32::from_le_bytes(record[4..8].try_into().expect("4 bytes"));
        let values = floats(&record[8..])?;
        instances.push(Instance {
            mesh,
            transparent: flags & 1 == 1,
            matrix: values[0..16].try_into().expect("16 floats"),
            normal_matrix: values[16..25].try_into().expect("9 floats"),
            color: values[25..29].try_into().expect("4 floats"),
        });
    }
    let mut textures = BTreeMap::new();
    let mut total = 0;
    if magic == MAGIC && !header.textures.is_empty() {
        return Err("Textures require DSW2".into());
    }
    for t in &header.textures {
        let size = texture_size(t.width, t.height, t.layers)?;
        total += size;
        if total > 128 * 1024 * 1024 {
            return Err("Scene textures exceed 128 MiB".into());
        }
        let (_, vertices) = meshes
            .iter()
            .find(|(k, _)| *k == t.mesh)
            .ok_or("Texture references missing mesh")?;
        let count = usize::try_from(t.coordinate_floats).map_err(|_| "Too many UV coordinates")?;
        if vertices.len() % 30 != 0 || count != vertices.len() / 10 * 5 {
            return Err("Texture coordinates disagree with mesh".into());
        }
        let coordinates = floats(at.take(count.checked_mul(4).ok_or("Too many UV coordinates")?)?)?;
        let rgba = at.take(size)?.to_vec();
        if textures
            .insert(
                t.mesh.clone(),
                Texture {
                    width: t.width,
                    height: t.height,
                    layers: t.layers,
                    coordinates,
                    rgba,
                },
            )
            .is_some()
        {
            return Err("Duplicate scene texture".into());
        }
    }
    if at.at != bytes.len() {
        return Err(format!(
            "{} trailing bytes after the scene frame",
            bytes.len() - at.at
        ));
    }
    Ok(Frame {
        header,
        meshes,
        instances,
        textures,
    })
}

fn floats(raw: &[u8]) -> Result<Vec<f32>, String> {
    let values: Vec<f32> = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    if values.iter().any(|v| !v.is_finite()) {
        return Err("scene frame carries a non-finite float".into());
    }
    Ok(values)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let Some(end) = end else {
            return Err("scene frame is truncated".into());
        };
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_round_trip_and_truncations_or_invalid_shapes_are_refused() {
        let (header, meshes, instances) = sample();
        let texture = Texture {
            width: 2,
            height: 1,
            layers: 1,
            rgba: vec![255; 8],
            coordinates: [0.25, 0.5, 0., 1., 1.].repeat(3),
        };
        let mut textures = BTreeMap::from([("wall".into(), texture)]);
        let bytes = encode_textured(
            header.clone(),
            meshes.iter().map(|(k, v)| (k.as_str(), v.as_slice())),
            &instances,
            &textures,
        )
        .unwrap();
        assert_eq!(&bytes[..4], &TEXTURED_MAGIC);
        assert_eq!(decode(&bytes).unwrap().textures, textures);
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end]).is_err());
        }
        let mut bad = bytes.clone();
        bad[..4].copy_from_slice(&MAGIC);
        assert!(decode(&bad).is_err());
        textures.get_mut("wall").unwrap().layers = 17;
        assert!(encode_textured(
            header.clone(),
            meshes.iter().map(|(k, v)| (k.as_str(), v.as_slice())),
            &instances,
            &textures
        )
        .is_err());
        textures.get_mut("wall").unwrap().layers = 1;
        textures.get_mut("wall").unwrap().coordinates[0] = f32::NAN;
        assert!(encode_textured(
            header,
            meshes.iter().map(|(k, v)| (k.as_str(), v.as_slice())),
            &instances,
            &textures
        )
        .is_err());
    }

    type Meshes = Vec<(String, Vec<f32>)>;

    fn sample() -> (Header, Meshes, Vec<Instance>) {
        let header = Header {
            origin_mm: [1500.0, -250.5],
            center: [1.0, 1.25, -2.0],
            radius: 7.5,
            diagnostics: vec!["missing model".into()],
            extra: serde_json::Map::from_iter([("modelDecodes".into(), 3.into())]),
            ..Header::default()
        };
        let meshes = vec![
            ("wall".to_owned(), (0..30).map(|v| v as f32 * 0.5).collect()),
            ("empty".to_owned(), vec![]),
            ("sha256:ab".to_owned(), vec![-1.0; 10]),
        ];
        let mut matrix = [0.0; 16];
        matrix[0] = 1.0;
        matrix[15] = 1.0;
        let instances = vec![
            Instance {
                mesh: 0,
                transparent: false,
                matrix,
                normal_matrix: [0.5; 9],
                color: [1.0, 0.0, 0.0, 1.0],
            },
            Instance {
                mesh: 2,
                transparent: true,
                matrix,
                normal_matrix: [1.0; 9],
                color: [0.2, 0.3, 0.4, 0.5],
            },
        ];
        (header, meshes, instances)
    }

    fn encoded() -> (Vec<u8>, Header, Meshes, Vec<Instance>) {
        let (header, meshes, instances) = sample();
        let bytes = encode(
            header.clone(),
            meshes.iter().map(|(k, v)| (k.as_str(), v.as_slice())),
            &instances,
        )
        .unwrap();
        (bytes, header, meshes, instances)
    }

    #[test]
    fn a_frame_round_trips_exactly() {
        let (bytes, header, meshes, instances) = encoded();
        let frame = decode(&bytes).unwrap();
        assert_eq!(frame.meshes, meshes);
        assert_eq!(frame.instances, instances);
        assert_eq!(frame.header.origin_mm, header.origin_mm);
        assert_eq!(frame.header.diagnostics, header.diagnostics);
        assert_eq!(frame.header.extra, header.extra);
        assert_eq!(frame.header.meshes, ["wall", "empty", "sha256:ab"]);
        assert_eq!(frame.header.mesh_floats, [30, 0, 10]);
        assert_eq!(bytes.len() % 4, 0);
    }

    #[test]
    fn every_truncation_and_any_extension_is_refused() {
        let (bytes, ..) = encoded();
        for len in 0..bytes.len() {
            assert!(
                decode(&bytes[..len]).is_err(),
                "prefix of {len} bytes accepted"
            );
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(decode(&longer).unwrap_err().contains("trailing"));
    }

    #[test]
    fn corrupt_fields_are_refused_without_panicking() {
        let (bytes, ..) = encoded();
        let mut magic = bytes.clone();
        magic[0] = b'X';
        assert!(decode(&magic).is_err());
        // The last instance's mesh index, then one of its floats.
        let record = bytes.len() - INSTANCE_BYTES;
        let mut index = bytes.clone();
        index[record..record + 4].copy_from_slice(&9u32.to_le_bytes());
        assert!(decode(&index).unwrap_err().contains("out of range"));
        let mut nan = bytes.clone();
        nan[record + 8..record + 12].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(decode(&nan).unwrap_err().contains("non-finite"));
        // Flip every byte in turn: refused or decoded, never a panic.
        for i in 0..bytes.len() {
            let mut flipped = bytes.clone();
            flipped[i] ^= 0xff;
            let _ = decode(&flipped);
        }
    }

    #[test]
    fn an_out_of_range_instance_is_not_encoded() {
        let (header, meshes, mut instances) = sample();
        instances[0].mesh = 3;
        let result = encode(
            header,
            meshes.iter().map(|(k, v)| (k.as_str(), v.as_slice())),
            &instances,
        );
        assert!(result.is_err());
    }
}
