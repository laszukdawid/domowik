//! Original material identities and non-destructive instance colour replacement.
use super::{ModelMesh, Preview, Triangle};
use domowik_format::appearance::MaterialColors;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMaterial {
    pub key: String,
    pub name: String,
    /// Original display-space sRGB plus opacity, in [0,1].
    pub color: [f64; 4],
}
impl ModelMaterial {
    pub(crate) fn new(key: String, name: String, color: [f64; 4]) -> Self {
        Self {
            key,
            name,
            color: color.map(|c| c.clamp(0., 1.)),
        }
    }
}

pub(crate) fn collect(triangles: &[Triangle]) -> Vec<ModelMaterial> {
    let mut materials = BTreeMap::new();
    for t in triangles {
        materials
            .entry(&t.material.key)
            .or_insert_with(|| (*t.material).clone());
    }
    materials.into_values().collect()
}

/// Lists used materials, sorted by stable key, under the ordinary bounded model
/// decode limits. Pure/thread-safe, caller-owned result; no source bytes change.
pub fn materials(bytes: &[u8]) -> Result<Vec<ModelMaterial>, String> {
    Ok(collect(&super::decode(bytes)?))
}

/// An instance's plan preview. Solid overrides replace RGB (not multiply), retain
/// source opacity and suppress an existing base-colour texture only on that
/// material. Other materials and source bytes remain untouched.
pub fn preview(bytes: &[u8], colors: &MaterialColors) -> Result<Preview, String> {
    domowik_format::appearance::validate_material_colors(colors)?;
    let mut triangles = super::decode(bytes)?;
    for t in &mut triangles {
        if let Some(color) = colors.get(&t.material.key) {
            t.color[..3].copy_from_slice(&color.rgb().map(f64::from));
            t.texture = None;
        }
    }
    super::render(&triangles, false)
}

impl ModelMesh {
    /// Retained CPU payload size for the caller's bounded cache (allocator
    /// overhead excluded). Includes material membership and labels as well as
    /// vertices and embedded texture/UV arrays.
    pub fn cache_bytes(&self) -> usize {
        self.vertices.len() * 4
            + self.triangle_materials.len() * std::mem::size_of::<usize>()
            + self
                .materials
                .iter()
                .map(|m| std::mem::size_of::<ModelMaterial>() + m.key.len() + m.name.len())
                .sum::<usize>()
            + self
                .texture
                .as_ref()
                .map_or(0, |t| t.rgba.len() + t.coordinates.len() * 4)
    }

    /// Reuses decoded geometry and material membership without another decode or
    /// LOD pass. Caller caches by model digest + canonical palette. The returned
    /// mesh owns its arrays; no mutation of shared meshes or GPU state occurs.
    pub fn recolored(&self, colors: &MaterialColors) -> Self {
        let mut mesh = self.clone();
        for (i, triangle) in mesh.vertices.as_chunks_mut::<30>().0.iter_mut().enumerate() {
            let key = &self.materials[self.triangle_materials[i]].key;
            if let Some(color) = colors.get(key) {
                for vertex in triangle.as_chunks_mut::<10>().0 {
                    vertex[6..9].copy_from_slice(&color.rgb());
                }
                if let Some(texture) = &mut mesh.texture {
                    for uv in texture.coordinates[i * 15..i * 15 + 15]
                        .as_chunks_mut::<5>()
                        .0
                    {
                        uv[2] = -1.;
                    }
                }
            }
        }
        mesh
    }
}
