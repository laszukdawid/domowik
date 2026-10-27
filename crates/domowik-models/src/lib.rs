//! Bounded, derived model previews. See BEHAVIOR-SPEC §10.1/§10.2 and
//! docs/PROVENANCE.md for public-format provenance.
//!
//! Input is opaque user content, not authoritative plan geometry. No filesystem,
//! network, callbacks or global state. Coordinates are model-local and finite;
//! +Y is up, X/Z form the footprint. Raster coverage is approximate at pixel
//! resolution, never a topology predicate. Returned PNG bytes are caller-owned.

pub mod appearance;
mod glb;
mod obj;
mod texture;
mod usd;
use serde::Serialize;
pub use texture::ModelTexture;

pub(crate) const MAX_TRIANGLES: usize = 1_000_000;
pub(crate) const MAX_VERTICES: usize = 1_000_000;
pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;
const SIZE: usize = 256;

#[derive(Clone)]
pub(crate) struct Triangle {
    pub points: [[f64; 3]; 3],
    pub color: [f64; 4],
    pub uv: [[f64; 2]; 3],
    pub texture: Option<texture::Surface>,
    pub material: std::sync::Arc<appearance::ModelMaterial>,
}

/// A rebuildable preview; model extents are in original model units, not mm.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub png: Vec<u8>,
    pub triangles: usize,
    pub covered_pixels: usize,
    pub model_extents: [f64; 3],
}

/// Decode a GLB, USD (`.usda`/`.usdc`/`.usdz`), OBJ or ZIP containing OBJ/MTL and render a 256-pixel square
/// transparent PNG. At most 64 MiB input/expanded relevant ZIP content, 4096
/// archive entries and one million vertices/triangles. Errors name unsupported
/// content or the exceeded budget; model bytes are never modified.
pub fn preview(bytes: &[u8]) -> Result<Preview, String> {
    render(&decode(bytes)?, false)
}

/// A 256-pixel transparent, aspect-preserving angled library thumbnail.
/// Uses the same input/decode/raster-work limits and ownership as [`preview`].
/// The fixed orthographic camera looks down toward the origin from +X/+Y/+Z;
/// all model extents fit inside an eight-pixel margin. This is approximate
/// display geometry, never an authoritative footprint or physical measurement.
/// Deterministic for the same bytes; no I/O, state, callbacks or cancellation.
pub fn thumbnail(bytes: &[u8]) -> Result<Preview, String> {
    render(&decode(bytes)?, true)
}

/// Initial object size in millimetres, measured from decoded scene geometry.
/// GLB and USD (`metersPerUnit`, default 0.01) use defined units; OBJ/ZIP
/// proportions use a 1000 mm longest axis.
/// +Y is up, width/depth are X/Z. Flat Y extents receive a 1 mm minimum.
/// This is derived display sizing, not an exact geometry predicate or a claim
/// about the real size of a unitless model. No source bytes are modified.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDimensions {
    pub dimensions_mm: [f64; 3],
    pub physical_units: bool,
    pub height_ratio: f64,
}

/// Bounded decode under the same limits as [preview], without rasterization.
/// Static scene transforms are included. Missing/degenerate horizontal bounds
/// and invalid coordinates are errors; callers may explicitly label a fallback.
/// Output order is width, depth, height. Physical dimensions are not clamped.
/// Transformed coordinates must be finite and at most 1e12 model units in
/// magnitude. Bounds use deterministic binary64 evaluation, not exact predicates.
/// This pure, thread-safe call returns caller-owned data and has no callbacks.
pub fn dimensions(bytes: &[u8]) -> Result<ModelDimensions, String> {
    let triangles = decode(bytes)?;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for t in &triangles {
        for p in t.points {
            for k in 0..3 {
                if !p[k].is_finite() || p[k].abs() > 1e12 {
                    return Err("Invalid model coordinate".into());
                }
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    let extent = std::array::from_fn::<_, 3, _>(|k| max[k] - min[k]);
    if extent.iter().any(|v| !v.is_finite()) || extent[0] <= 0. || extent[2] <= 0. {
        return Err("Model has no usable horizontal bounds".into());
    }
    let physical_units = bytes.starts_with(b"glTF") || usd::detect(bytes).is_some();
    let factor = if physical_units {
        1000.
    } else {
        1000. / extent.iter().copied().fold(0., f64::max)
    };
    let dimensions_mm = [
        extent[0] * factor,
        extent[2] * factor,
        (extent[1] * factor).max(1.),
    ];
    let height_ratio = extent[1] / extent[0];
    if !height_ratio.is_finite() || dimensions_mm.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("Model proportions exceed the supported numerical range".into());
    }
    Ok(ModelDimensions {
        dimensions_mm,
        physical_units,
        height_ratio,
    })
}

fn decode(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Model preview input exceeds 64 MiB".into());
    }
    let triangles = if bytes.starts_with(b"glTF") {
        glb::decode(bytes)?
    } else if let Some(kind) = usd::detect(bytes) {
        usd::decode(bytes, kind)?
    } else if bytes.starts_with(b"PK") {
        obj::archive(bytes)?
    } else {
        obj::decode(bytes, &Default::default())?
    };
    Ok(triangles)
}

/// Shared GPU-ready model geometry, centered in X/Z and grounded at Y=0.
/// XYZ, normal XYZ, RGBA per vertex. Coordinates are normalized to original
/// width/depth, with height expressed as a ratio of the original width.
/// Fast LOD uses bounded vertex clustering over the normalized bounding box;
/// it changes derived display geometry only, never original asset bytes.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMesh {
    pub vertices: Vec<f32>,
    /// Embedded base-colour pixels and parallel UV/layer/wrap attributes.
    /// Absent for colour-only models. Not authoritative document content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<ModelTexture>,
    pub height_ratio: f64,
    pub source_triangles: usize,
    pub triangles: usize,
    pub materials: Vec<appearance::ModelMaterial>,
    /// Material membership of each retained triangle; CPU-only derivation.
    #[serde(skip)]
    pub triangle_materials: Vec<usize>,
}

/// Decode once for an interactive 3D renderer. At most 20,000 retained triangles
/// in fast mode. Cached by the caller's content digest; camera movement should
/// never call this function. Material colors and shape are approximated by LOD.
pub fn mesh(bytes: &[u8]) -> Result<ModelMesh, String> {
    mesh_with_budget(bytes, 20000)
}

/// Fast interactive rendering can request a smaller visual triangle budget.
/// The original bytes and 2D geometry are unaffected. Budgets are bounded to
/// 100..20,000 triangles and applied before GPU upload, never during navigation.
pub fn mesh_with_budget(bytes: &[u8], budget: usize) -> Result<ModelMesh, String> {
    let budget = budget.clamp(100, 20000);
    let triangles = decode(bytes)?;
    if triangles.is_empty() {
        return Err("Model has no triangles".into());
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for t in &triangles {
        for p in t.points {
            for k in 0..3 {
                if !p[k].is_finite() || p[k].abs() > 1e12 {
                    return Err("Invalid model coordinate".into());
                }
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    let extent = std::array::from_fn::<_, 3, _>(|k| max[k] - min[k]);
    if extent[0] <= 0. || extent[2] <= 0. {
        return Err("Model has no horizontal extent".into());
    }
    let mut vertices = Vec::new();
    let mut coordinates = Vec::new();
    let materials = appearance::collect(&triangles);
    let material_indices: std::collections::BTreeMap<_, _> = materials
        .iter()
        .enumerate()
        .map(|(i, m)| (m.key.as_str(), i))
        .collect();
    let mut triangle_materials = Vec::new();
    let mut images: Vec<std::sync::Arc<texture::Image>> = Vec::new();
    for t in &triangles {
        if let Some(surface) = &t.texture {
            if !images
                .iter()
                .any(|i| std::sync::Arc::ptr_eq(i, &surface.image))
            {
                images.push(surface.image.clone());
            }
        }
    }
    for grid in [128., 64., 32., 16., 8.] {
        vertices.clear();
        coordinates.clear();
        triangle_materials.clear();
        let mut seen = std::collections::BTreeSet::new();
        let mut seen_plain = std::collections::BTreeSet::new();
        for t in &triangles {
            let keys = t.points.map(|p| {
                std::array::from_fn::<_, 3, _>(|k| {
                    if extent[k] == 0. {
                        0i32
                    } else {
                        ((p[k] - min[k]) / extent[k] * grid).round() as i32
                    }
                })
            });
            if keys[0] == keys[1] || keys[1] == keys[2] || keys[2] == keys[0] {
                continue;
            }
            let mut ordered = keys;
            ordered.sort();
            let color = t.color.map(|c| (c.clamp(0., 1.) * 255.).round() as u8);
            let layer = t
                .texture
                .as_ref()
                .map(|s| {
                    images
                        .iter()
                        .position(|i| std::sync::Arc::ptr_eq(i, &s.image))
                        .unwrap() as i32
                })
                .unwrap_or(-1);
            let uv_key = t.uv.map(|uv| uv.map(f64::to_bits));
            let wrap = t.texture.as_ref().map(|s| s.wrap).unwrap_or([0; 2]);
            let unique = if images.is_empty() {
                // Colour-only models retain the compact pre-texture LOD key
                // and do not allocate a redundant derived UV array.
                seen_plain.insert((ordered, color, &t.material.key))
            } else {
                seen.insert((ordered, color, layer, uv_key, wrap, &t.material.key))
            };
            if !unique {
                continue;
            }
            let p = keys.map(|p| {
                [
                    (p[0] as f64 / grid) - 0.5,
                    p[1] as f64 / grid * extent[1] / extent[0],
                    (p[2] as f64 / grid) - 0.5,
                ]
            });
            let a = std::array::from_fn::<_, 3, _>(|k| p[1][k] - p[0][k]);
            let b = std::array::from_fn::<_, 3, _>(|k| p[2][k] - p[0][k]);
            let n = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let length = n[0].hypot(n[1]).hypot(n[2]);
            if length <= 1e-12 {
                continue;
            }
            triangle_materials.push(material_indices[t.material.key.as_str()]);
            for (i, p) in p.into_iter().enumerate() {
                vertices.extend(p.map(|v| v as f32));
                vertices.extend(n.map(|v| (v / length) as f32));
                vertices.extend(t.color.map(|v| v.clamp(0., 1.) as f32));
                if !images.is_empty() {
                    coordinates.extend([
                        t.uv[i][0] as f32,
                        t.uv[i][1] as f32,
                        layer as f32,
                        wrap[0] as f32,
                        wrap[1] as f32,
                    ]);
                }
            }
        }
        if vertices.len() / 30 <= budget {
            break;
        }
    }
    if vertices.len() / 30 > budget || vertices.is_empty() {
        return Err("Model cannot fit the interactive LOD budget".into());
    }
    if vertices.iter().any(|v| !v.is_finite()) || !(extent[1] / extent[0]).is_finite() {
        return Err("Model proportions exceed GPU coordinate range".into());
    }
    Ok(ModelMesh {
        texture: texture::array(&images, coordinates)?,
        height_ratio: extent[1] / extent[0],
        source_triangles: triangles.len(),
        triangles: vertices.len() / 30,
        vertices,
        materials,
        triangle_materials,
    })
}

fn render(triangles: &[Triangle], angled: bool) -> Result<Preview, String> {
    if triangles.is_empty() {
        return Err("Model contains no supported triangle surfaces".into());
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for t in triangles {
        for p in t.points {
            for k in 0..3 {
                if !p[k].is_finite() || p[k].abs() > 1e12 {
                    return Err("Model coordinate is non-finite or too large".into());
                }
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    let extent = std::array::from_fn(|k| max[k] - min[k]);
    if extent[0] <= 0.0 || extent[2] <= 0.0 {
        return Err("Model has no horizontal extent".into());
    }
    // An orthonormal camera basis. Keep coordinates centered before projection
    // so a model's large translation cannot dominate thumbnail precision.
    let center = std::array::from_fn::<_, 3, _>(|k| (min[k] + max[k]) * 0.5);
    let project = |v: [f64; 3]| {
        let [x, y, z] = std::array::from_fn::<_, 3, _>(|k| v[k] - center[k]);
        [
            (x - z) / 2f64.sqrt(),
            (x + z - 2.0 * y) / 6f64.sqrt(),
            (x + y + z) / 3f64.sqrt(),
        ]
    };
    let mut camera_min = [f64::INFINITY; 2];
    let mut camera_max = [f64::NEG_INFINITY; 2];
    if angled {
        for p in triangles.iter().flat_map(|t| t.points).map(project) {
            for k in 0..2 {
                camera_min[k] = camera_min[k].min(p[k]);
                camera_max[k] = camera_max[k].max(p[k]);
            }
        }
    }
    let camera_extent = [camera_max[0] - camera_min[0], camera_max[1] - camera_min[1]];
    if angled && camera_extent[0].max(camera_extent[1]) <= 0.0 {
        return Err("Model has no visible thumbnail extent".into());
    }
    let scale = (SIZE - 17) as f64 / camera_extent[0].max(camera_extent[1]);
    if angled && (!scale.is_finite() || scale <= 0.0) {
        return Err("Model thumbnail scale exceeds the numerical range".into());
    }
    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    let mut depth = vec![f64::NEG_INFINITY; SIZE * SIZE];
    let mut work = 0usize;
    for t in triangles {
        if t.color[3] <= 0.01 {
            continue;
        }
        let p = t.points.map(|v| {
            if angled {
                let p = project(v);
                [
                    (p[0] - camera_min[0] - camera_extent[0] * 0.5) * scale
                        + (SIZE - 1) as f64 * 0.5,
                    (p[1] - camera_min[1] - camera_extent[1] * 0.5) * scale
                        + (SIZE - 1) as f64 * 0.5,
                    p[2],
                ]
            } else {
                [
                    (v[0] - min[0]) / extent[0] * (SIZE - 1) as f64,
                    (v[2] - min[2]) / extent[2] * (SIZE - 1) as f64,
                    v[1],
                ]
            }
        });
        let edge = |a: [f64; 3], b: [f64; 3], x: f64, y: f64| {
            (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])
        };
        let area = edge(p[0], p[1], p[2][0], p[2][1]);
        if area.abs() < 1e-12 {
            continue;
        }
        let lo_x = p
            .iter()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let hi_x = p
            .iter()
            .map(|p| p[0])
            .fold(0.0, f64::max)
            .ceil()
            .min((SIZE - 1) as f64) as usize;
        let lo_y = p
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let hi_y = p
            .iter()
            .map(|p| p[1])
            .fold(0.0, f64::max)
            .ceil()
            .min((SIZE - 1) as f64) as usize;
        work += (hi_x - lo_x + 1) * (hi_y - lo_y + 1);
        if work > 100_000_000 {
            return Err("Model preview exceeds the raster-work budget".into());
        }
        let a = std::array::from_fn::<_, 3, _>(|i| t.points[1][i] - t.points[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| t.points[2][i] - t.points[0][i]);
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = n[0].hypot(n[1]).hypot(n[2]);
        let shade = if length > 0.0 {
            0.65 + 0.35 * n[1].abs() / length
        } else {
            1.0
        };
        for y in lo_y..=hi_y {
            for x in lo_x..=hi_x {
                let w = [
                    edge(p[1], p[2], x as f64 + 0.5, y as f64 + 0.5) / area,
                    edge(p[2], p[0], x as f64 + 0.5, y as f64 + 0.5) / area,
                ];
                let w2 = 1.0 - w[0] - w[1];
                if w[0] < -1e-9 || w[1] < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let z = w[0] * p[0][2] + w[1] * p[1][2] + w2 * p[2][2];
                let i = y * SIZE + x;
                if z < depth[i] {
                    continue;
                }
                let color = if let Some(surface) = &t.texture {
                    let uv = std::array::from_fn(|c| {
                        w[0] * t.uv[0][c] + w[1] * t.uv[1][c] + w2 * t.uv[2][c]
                    });
                    texture::sample(surface, uv, t.color)
                } else {
                    t.color
                };
                let alpha = color[3].clamp(0.0, 1.0);
                if alpha <= 0.01 {
                    continue;
                }
                depth[i] = z;
                for c in 0..3 {
                    pixels[i * 4 + c] =
                        (color[c].clamp(0.0, 1.0) * shade * alpha * 255.0).round() as u8;
                }
                pixels[i * 4 + 3] = (alpha * 255.0).round() as u8;
            }
        }
    }
    let covered_pixels = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] != 0)
        .count();
    if covered_pixels == 0 {
        return Err(if angled {
            "Model has no visible thumbnail surfaces"
        } else {
            "Model has no visible top-down surfaces"
        }
        .into());
    }
    let mut image =
        tiny_skia::Pixmap::new(SIZE as u32, SIZE as u32).ok_or("Preview allocation failed")?;
    image.data_mut().copy_from_slice(&pixels);
    Ok(Preview {
        png: image.encode_png().map_err(|e| e.to_string())?,
        triangles: triangles.len(),
        covered_pixels,
        model_extents: extent,
    })
}
