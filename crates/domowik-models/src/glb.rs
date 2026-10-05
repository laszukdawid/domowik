use super::*;
use serde_json::Value;
type Matrix = [f64; 16];
const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

fn integer(v: &Value) -> Result<usize, String> {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "Invalid glTF integer".into())
}
fn offset(v: &Value) -> Result<usize, String> {
    if v.is_null() {
        Ok(0)
    } else {
        integer(v)
    }
}
fn at<'a>(root: &'a Value, key: &str, i: usize) -> Result<&'a Value, String> {
    root[key]
        .as_array()
        .and_then(|a| a.get(i))
        .ok_or_else(|| format!("Invalid glTF {key} reference"))
}
fn u32_at(bytes: &[u8], i: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(i..i + 4)
            .ok_or("Truncated GLB")?
            .try_into()
            .map_err(|_| "Truncated GLB")?,
    ))
}
fn view<'a>(j: &Value, bin: &'a [u8], i: usize) -> Result<&'a [u8], String> {
    let v = at(j, "bufferViews", i)?;
    if integer(&v["buffer"])? != 0 {
        return Err("External glTF buffers are not fetched".into());
    }
    let start = offset(&v["byteOffset"])?;
    let len = integer(&v["byteLength"])?;
    bin.get(start..start.checked_add(len).ok_or("GLB buffer range overflow")?)
        .ok_or_else(|| "GLB buffer view is truncated".into())
}

fn accessor(j: &Value, bin: &[u8], id: usize, positions: bool) -> Result<Vec<f64>, String> {
    let a = at(j, "accessors", id)?;
    if !a["sparse"].is_null() {
        return Err("Sparse glTF accessors are not supported".into());
    }
    let count = integer(&a["count"])?;
    if count
        > if positions {
            MAX_VERTICES
        } else {
            MAX_VERTICES * 3
        }
    {
        return Err("glTF accessor exceeds the preview budget".into());
    }
    let components = if positions { 3 } else { 1 };
    if a["type"] != if positions { "VEC3" } else { "SCALAR" } {
        return Err("Invalid glTF accessor shape".into());
    }
    let kind = integer(&a["componentType"])?;
    if positions && kind != 5126 {
        return Err("glTF positions must be float32".into());
    }
    let size = match kind {
        5121 => 1,
        5123 => 2,
        5125 | 5126 => 4,
        _ => return Err("Unsupported glTF component type".into()),
    };
    let v = at(j, "bufferViews", integer(&a["bufferView"])?)?;
    let bytes = view(j, bin, integer(&a["bufferView"])?)?;
    let stride = if v["byteStride"].is_null() {
        size * components
    } else {
        integer(&v["byteStride"])?
    };
    if stride < size * components || stride > 256 {
        return Err("Invalid glTF accessor stride".into());
    }
    let start = offset(&a["byteOffset"])?;
    let needed = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(size * components))
        .and_then(|n| n.checked_add(start))
        .ok_or("glTF accessor range overflow")?;
    if count == 0 || needed > bytes.len() {
        return Err("Truncated glTF accessor".into());
    }
    let mut values = Vec::with_capacity(count * components);
    for i in 0..count {
        for c in 0..components {
            let p = start + i * stride + c * size;
            let value = match kind {
                5121 => bytes[p] as f64,
                5123 => u16::from_le_bytes([bytes[p], bytes[p + 1]]) as f64,
                5125 => u32_at(bytes, p)? as f64,
                _ => f32::from_le_bytes(
                    bytes[p..p + 4]
                        .try_into()
                        .map_err(|_| "Truncated position")?,
                ) as f64,
            };
            if !value.is_finite() {
                return Err("Non-finite glTF position".into());
            }
            values.push(value);
        }
    }
    Ok(values)
}

fn array<const N: usize>(v: &Value, default: [f64; N]) -> Result<[f64; N], String> {
    if v.is_null() {
        return Ok(default);
    }
    let values = v
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or("Invalid glTF transform")?;
    let mut out = default;
    for i in 0..N {
        out[i] = values[i]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("Non-finite glTF transform")?;
    }
    Ok(out)
}
fn local(node: &Value) -> Result<Matrix, String> {
    if !node["matrix"].is_null() {
        return array(&node["matrix"], IDENTITY);
    }
    let [x, y, z, w] = array(&node["rotation"], [0., 0., 0., 1.])?;
    let s = array(&node["scale"], [1., 1., 1.])?;
    let t = array(&node["translation"], [0., 0., 0.])?;
    Ok([
        (1. - 2. * (y * y + z * z)) * s[0],
        2. * (x * y + z * w) * s[0],
        2. * (x * z - y * w) * s[0],
        0.,
        2. * (x * y - z * w) * s[1],
        (1. - 2. * (x * x + z * z)) * s[1],
        2. * (y * z + x * w) * s[1],
        0.,
        2. * (x * z + y * w) * s[2],
        2. * (y * z - x * w) * s[2],
        (1. - 2. * (x * x + y * y)) * s[2],
        0.,
        t[0],
        t[1],
        t[2],
        1.,
    ])
}
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[(i / 4) * 4 + k]).sum())
}
fn transform(m: Matrix, p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2] + m[12 + i])
}

fn primitive(
    j: &Value,
    bin: &[u8],
    p: &Value,
    matrix: Matrix,
    triangles: &mut Vec<Triangle>,
) -> Result<(), String> {
    let mut color = [0.8, 0.8, 0.8, 1.0];
    if !p["material"].is_null() {
        let m = at(j, "materials", integer(&p["material"])?)?;
        color = array(
            &m["pbrMetallicRoughness"]["baseColorFactor"],
            [1., 1., 1., 1.],
        )?;
        for c in &mut color[..3] {
            *c = if *c <= 0.0031308 {
                *c * 12.92
            } else {
                1.055 * c.powf(1. / 2.4) - 0.055
            };
        }
    }
    let draco = &p["extensions"]["KHR_draco_mesh_compression"];
    let (vertices, indices) = if !draco.is_null() {
        let bytes = view(j, bin, integer(&draco["bufferView"])?)?;
        let limits = draco_core::DecodeLimits::default()
            .with_max_points(MAX_VERTICES as u64)
            .with_max_faces(MAX_TRIANGLES as u64)
            .with_max_decoded_bytes(MAX_BYTES as u64);
        let mut buffer = draco_core::DecoderBuffer::new(bytes).with_limits(limits);
        let mut mesh = draco_core::Mesh::new();
        draco_core::MeshDecoder::new()
            .decode(&mut buffer, &mut mesh)
            .map_err(|e| format!("Draco decode: {e}"))?;
        if mesh.num_points() > MAX_VERTICES || mesh.num_faces() > MAX_TRIANGLES {
            return Err("Decoded Draco geometry exceeds the preview budget".into());
        }
        let attribute_id = u32::try_from(integer(&draco["attributes"]["POSITION"])?)
            .map_err(|_| "Invalid Draco attribute id")?;
        let attribute = mesh
            .attribute_by_unique_id(attribute_id)
            .ok_or("Missing Draco positions")?;
        let positions = attribute.read_f32s(mesh.num_points(), 3);
        let vertices = positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
            .collect::<Vec<_>>();
        let indices = (0..mesh.num_faces())
            .flat_map(|i| {
                mesh.face(draco_core::FaceIndex(i as u32))
                    .map(|p| p.0 as usize)
            })
            .collect::<Vec<_>>();
        (vertices, indices)
    } else {
        let positions = accessor(j, bin, integer(&p["attributes"]["POSITION"])?, true)?;
        if positions.len() / 3 > MAX_VERTICES {
            return Err("glTF exceeds vertex budget".into());
        }
        let vertices = positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|v| [v[0], v[1], v[2]])
            .collect::<Vec<_>>();
        let indices = if p["indices"].is_null() {
            (0..vertices.len()).collect()
        } else {
            accessor(j, bin, integer(&p["indices"])?, false)?
                .iter()
                .map(|v| *v as usize)
                .collect()
        };
        (vertices, indices)
    };
    if indices.len() < 3 {
        return Err("glTF primitive has fewer than three indices".into());
    }
    let mode = if p["mode"].is_null() {
        4
    } else {
        integer(&p["mode"])?
    };
    let faces: Box<dyn Iterator<Item = [usize; 3]>> = match mode {
        4 => Box::new(indices.as_chunks::<3>().0.iter().copied()),
        5 => Box::new(indices.windows(3).map(|v| [v[0], v[1], v[2]])),
        6 => Box::new(indices[1..].windows(2).map(|v| [indices[0], v[0], v[1]])),
        _ => return Err("Unsupported non-triangle glTF primitive".into()),
    };
    for face in faces {
        if triangles.len() >= MAX_TRIANGLES {
            return Err("Model exceeds triangle budget".into());
        }
        let mut points = [[0.; 3]; 3];
        for i in 0..3 {
            points[i] = transform(
                matrix,
                *vertices.get(face[i]).ok_or("Invalid glTF vertex index")?,
            );
        }
        triangles.push(Triangle { points, color });
    }
    Ok(())
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    if u32_at(bytes, 4)? != 2 || u32_at(bytes, 8)? as usize != bytes.len() {
        return Err("Invalid GLB 2.0 header".into());
    }
    let json_len = u32_at(bytes, 12)? as usize;
    if u32_at(bytes, 16)? != 0x4e4f534a || json_len > 16 * 1024 * 1024 {
        return Err("Invalid GLB JSON chunk".into());
    }
    let end = 20usize.checked_add(json_len).ok_or("GLB chunk overflow")?;
    let j: Value = serde_json::from_slice(bytes.get(20..end).ok_or("Truncated GLB JSON")?)
        .map_err(|e| e.to_string())?;
    if j["asset"]["version"] != "2.0" {
        return Err("Only glTF 2.0 is supported".into());
    }
    let bin_len = u32_at(bytes, end)? as usize;
    if u32_at(bytes, end + 4)? != 0x004e4942 {
        return Err("Missing GLB binary buffer".into());
    }
    let bin = bytes
        .get(end + 8..(end + 8).checked_add(bin_len).ok_or("GLB chunk overflow")?)
        .ok_or("Truncated GLB binary chunk")?;
    let mut triangles = Vec::new();
    let nodes = j["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if nodes.len() > 10000 {
        return Err("Too many glTF nodes".into());
    }
    let roots = if let Some(scenes) = j["scenes"].as_array().filter(|s| !s.is_empty()) {
        let scene = scenes
            .get(offset(&j["scene"])?)
            .ok_or("Invalid glTF scene")?;
        scene["nodes"]
            .as_array()
            .ok_or("Missing glTF scene nodes")?
            .iter()
            .map(integer)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut children = std::collections::BTreeSet::new();
        for node in nodes {
            if let Some(c) = node["children"].as_array() {
                for n in c {
                    children.insert(integer(n)?);
                }
            }
        }
        (0..nodes.len()).filter(|i| !children.contains(i)).collect()
    };
    let mut stack = roots
        .into_iter()
        .map(|i| (i, IDENTITY, 0usize))
        .collect::<Vec<_>>();
    let mut seen = std::collections::BTreeSet::new();
    while let Some((id, parent, depth)) = stack.pop() {
        if depth > 128 || !seen.insert(id) {
            return Err("Invalid cyclic/shared glTF node hierarchy".into());
        }
        let node = nodes.get(id).ok_or("Invalid glTF node")?;
        if !node["skin"].is_null() {
            return Err("Skinned model preview is unsupported".into());
        }
        let matrix = multiply(parent, local(node)?);
        if !node["mesh"].is_null() {
            let mesh = at(&j, "meshes", integer(&node["mesh"])?)?;
            for p in mesh["primitives"]
                .as_array()
                .ok_or("Missing glTF primitives")?
            {
                primitive(&j, bin, p, matrix, &mut triangles)?;
            }
        }
        if let Some(children) = node["children"].as_array() {
            for child in children.iter().rev() {
                stack.push((integer(child)?, matrix, depth + 1));
            }
        }
    }
    if nodes.is_empty() {
        if let Some(meshes) = j["meshes"].as_array() {
            for mesh in meshes {
                for p in mesh["primitives"]
                    .as_array()
                    .ok_or("Missing glTF primitives")?
                {
                    primitive(&j, bin, p, IDENTITY, &mut triangles)?;
                }
            }
        }
    }
    Ok(triangles)
}
