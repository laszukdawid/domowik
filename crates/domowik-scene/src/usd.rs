//! A plan's derived 3D scene as one `.usdz` package (BEHAVIOR-SPEC §11.3).
//!
//! Every level is exported, unghosted, from the same [`build`](crate::build)
//! the editor draws, so walls, openings, floors, construction bodies, stairs
//! and placed models appear as they do in 3D. Output is metres, +Y up, in plan
//! coordinates (plan x → USD x, plan y → USD −z). Colours are those the
//! renderer shows: vertex colour times node tint, written as one
//! `UsdPreviewSurface` per distinct colour because many viewers ignore vertex
//! colours. Embedded model base-colour textures are packaged as PNG. Repeated
//! geometry is written once under `/Prototypes` and referenced.
//!
//! Model meshes are the bounded interactive meshes (at most 20,000 triangles
//! per model, see `domowik_models::mesh`), not the original model bytes.
//! The result is deterministic for the same document and asset bytes. Pure
//! and thread-safe: no filesystem, network or callbacks; the caller owns the
//! returned bytes. A model that cannot be decoded becomes the renderer's
//! placeholder box and is reported in `diagnostics`.
use crate::{Scene, VERTEX_FLOATS};
use domowik_format::Document;
use domowik_models::ModelMesh;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::Arc;

/// Aggregate decoded-model budgets, matching the scene's geometry bound and the
/// editor's model cache.
const MAX_MODEL_FLOATS: usize = 10_000_000;
const MAX_TEXTURE_BYTES: usize = 128 * 1024 * 1024;

/// A finished export.
pub struct UsdzExport {
    pub bytes: Vec<u8>,
    /// Triangles in the composed scene, counting every referenced instance.
    pub triangles: usize,
    pub diagnostics: Vec<String>,
}

/// Exports every level of `doc`. `assets` maps content digests to embedded
/// model bytes (the package's blob store).
pub fn usdz(doc: &Document, assets: &BTreeMap<String, Vec<u8>>) -> Result<UsdzExport, String> {
    let mut doc = doc.clone();
    let mut diagnostics = Vec::new();
    let mut models: BTreeMap<String, Arc<ModelMesh>> = BTreeMap::new();
    // Charged before a decoded model or colour variant is retained, so peak
    // memory stays within the budget plus one bounded model decode.
    let (mut floats, mut pixels) = (0usize, 0usize);
    let mut charge = |m: &ModelMesh| -> Result<(), String> {
        floats += m.vertices.len();
        pixels += m.texture.as_ref().map_or(0, |t| t.rgba.len());
        if floats > MAX_MODEL_FLOATS || pixels > MAX_TEXTURE_BYTES {
            return Err("Decoded models exceed the export budget \
                 (10,000,000 vertex floats, 128 MiB of texture pixels)"
                .into());
        }
        Ok(())
    };
    let digests: BTreeSet<_> = doc
        .asset_instances
        .iter()
        .map(|a| a.digest.clone())
        .collect();
    for digest in digests {
        let Some(bytes) = assets.get(&digest) else {
            continue;
        };
        match domowik_models::mesh(bytes) {
            Ok(mesh) => {
                charge(&mesh)?;
                models.insert(digest, Arc::new(mesh));
            }
            Err(e) => {
                let name = doc
                    .asset_instances
                    .iter()
                    .find(|a| a.digest == digest)
                    .and_then(|a| a.display_name.as_deref())
                    .unwrap_or("Model");
                diagnostics.push(format!("{name}: {e}"));
            }
        }
    }
    // Instance colour overrides become their own meshes, as in the editor.
    let mut variants: BTreeMap<(String, String), String> = BTreeMap::new();
    for asset in &mut doc.asset_instances {
        let colors = domowik_format::appearance::material_colors(asset)?;
        if colors.is_empty() {
            continue;
        }
        let Some(base) = models.get(&asset.digest).cloned() else {
            continue;
        };
        let palette = serde_json::to_string(&colors).map_err(|e| e.to_string())?;
        let next = variants.len();
        let key = variants
            .entry((asset.digest.clone(), palette))
            .or_insert_with(|| format!("{}#colors{next}", asset.digest))
            .clone();
        if !models.contains_key(&key) {
            charge(&base)?;
            models.insert(key.clone(), Arc::new(base.recolored(&colors)));
        }
        asset.digest = key;
    }
    let heights = models
        .iter()
        .map(|(k, m)| (k.clone(), m.height_ratio))
        .collect();
    let levels: Vec<_> = doc.levels.iter().map(|l| l.id).collect();
    let scene = crate::build(&doc, &levels, &BTreeSet::new(), &heights)?;
    for d in &scene.diagnostics {
        if !diagnostics.contains(d) {
            diagnostics.push(d.clone());
        }
    }
    let mut writer = Writer::default();
    let text = writer.layer(&scene, &models)?;
    let layer = openusd::sdf::Layer::from_bytes("plan.usda", text.into_bytes())
        .map_err(|e| format!("USD export: {e}"))?;
    let mut crate_bytes = std::io::Cursor::new(Vec::new());
    openusd::usdc::CrateWriter::write(layer.data(), &mut crate_bytes)
        .map_err(|e| format!("USD export: {e}"))?;
    let mut archive = openusd::usdz::ArchiveWriter::new(std::io::Cursor::new(Vec::new()));
    let package = |e: openusd::usdz::ArchiveError| format!("USD export: {e}");
    archive
        .add_layer("plan.usdc", crate_bytes.get_ref())
        .map_err(package)?;
    for (name, png) in &writer.textures {
        archive.add_layer(name, png).map_err(package)?;
    }
    Ok(UsdzExport {
        bytes: archive.finish().map_err(package)?.into_inner(),
        triangles: writer.triangles,
        diagnostics,
    })
}

#[derive(Default)]
struct Writer {
    prototypes: BTreeMap<(String, [u32; 4]), usize>,
    body: String,
    textures: Vec<(String, Vec<u8>)>,
    texture_names: BTreeMap<(String, u32), String>,
    names: BTreeSet<String>,
    triangles: usize,
}

/// One material group inside a prototype.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Look {
    rgba: [u8; 4],
    layer: i32,
    wrap: [u8; 2],
}

fn linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// USD prim names: identifier characters only, unique within the plan.
fn prim_name(id: &str, used: &mut BTreeSet<String>) -> String {
    let mut name: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        name.insert(0, 'N');
    }
    let mut unique = name.clone();
    let mut n = 1;
    while !used.insert(unique.clone()) {
        n += 1;
        unique = format!("{name}_{n}");
    }
    unique
}

fn finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

impl Writer {
    fn layer(
        &mut self,
        scene: &Scene,
        models: &BTreeMap<String, Arc<ModelMesh>>,
    ) -> Result<String, String> {
        let mut nodes = String::new();
        for node in &scene.nodes {
            if !finite(&node.matrix) || !finite(&node.color) {
                continue;
            }
            let (vertices, texture) = match scene.meshes.get(&node.mesh) {
                Some(v) => (v.as_slice(), None),
                None => match models.get(&node.mesh) {
                    Some(m) => (m.vertices.as_slice(), m.texture.as_ref()),
                    None => continue,
                },
            };
            if vertices.len() < 3 * VERTEX_FLOATS || !finite(vertices) {
                continue;
            }
            let tint = node.color.map(f32::to_bits);
            let next = self.prototypes.len();
            let prototype = match self.prototypes.get(&(node.mesh.clone(), tint)) {
                Some(&p) => p,
                None => {
                    self.prototypes.insert((node.mesh.clone(), tint), next);
                    self.prototype(next, &node.mesh, vertices, texture, node.color);
                    next
                }
            };
            self.triangles += vertices.len() / VERTEX_FLOATS / 3;
            let name = prim_name(&node.id, &mut self.names);
            let m = node.matrix.map(f64::from);
            // Column-major for column vectors is USD's row-major for row vectors.
            let rows: Vec<String> = m
                .chunks(4)
                .map(|r| format!("({}, {}, {}, {})", r[0], r[1], r[2], r[3]))
                .collect();
            let _ = write!(
                nodes,
                "    def Xform \"{name}\" (\n        prepend references = </Prototypes/P{prototype}>\n    )\n    {{\n        \
                 matrix4d xformOp:transform = ({})\n        uniform token[] xformOpOrder = [\"xformOp:transform\"]\n    }}\n",
                rows.join(", ")
            );
        }
        let origin = scene.origin_mm.map(|v| v / 1000.);
        let mut out = String::new();
        let _ = write!(
            out,
            "#usda 1.0\n(\n    defaultPrim = \"Plan\"\n    metersPerUnit = 1\n    upAxis = \"Y\"\n    doc = \"Domowik plan export\"\n)\n\n\
             def Xform \"Plan\" (\n    kind = \"assembly\"\n)\n{{\n    double3 xformOp:translate = ({}, 0, {})\n    \
             uniform token[] xformOpOrder = [\"xformOp:translate\"]\n{nodes}}}\n\nclass \"Prototypes\"\n{{\n{}}}\n",
            origin[0], -origin[1], self.body
        );
        Ok(out)
    }

    fn texture(
        &mut self,
        key: &str,
        texture: &domowik_models::ModelTexture,
        layer: u32,
    ) -> Option<String> {
        if let Some(name) = self.texture_names.get(&(key.to_owned(), layer)) {
            return Some(name.clone());
        }
        let size = texture.width as usize * texture.height as usize * 4;
        let pixels = texture
            .rgba
            .get(layer as usize * size..(layer as usize + 1) * size)?;
        let image = image::RgbaImage::from_raw(texture.width, texture.height, pixels.to_vec())?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .ok()?;
        let name = format!("textures/t{}.png", self.textures.len());
        self.textures.push((name.clone(), png.into_inner()));
        self.texture_names
            .insert((key.to_owned(), layer), name.clone());
        Some(name)
    }

    fn prototype(
        &mut self,
        index: usize,
        key: &str,
        vertices: &[f32],
        texture: Option<&domowik_models::ModelTexture>,
        tint: [f32; 4],
    ) {
        let count = vertices.len() / VERTEX_FLOATS;
        let triangles = count / 3;
        let coordinates = texture.map(|t| t.coordinates.as_slice());
        let mut groups: BTreeMap<Look, Vec<usize>> = BTreeMap::new();
        for t in 0..triangles {
            let v = &vertices[t * 3 * VERTEX_FLOATS..];
            let rgba: [u8; 4] =
                std::array::from_fn(|k| ((v[6 + k] * tint[k]).clamp(0., 1.) * 255.).round() as u8);
            let (layer, wrap) = match coordinates.and_then(|c| c.get(t * 15..t * 15 + 5)) {
                Some(c) if c[2] >= 0. => (c[2] as i32, [c[3] as u8, c[4] as u8]),
                _ => (-1, [0, 0]),
            };
            groups
                .entry(Look { rgba, layer, wrap })
                .or_default()
                .push(t);
        }
        let textured = groups.keys().any(|l| l.layer >= 0);
        let base = format!("/Prototypes/P{index}");
        let mut out = String::new();
        let list = |values: &mut dyn Iterator<Item = String>| values.collect::<Vec<_>>().join(", ");
        let _ = write!(
            out,
            "    def Xform \"P{index}\"\n    {{\n        def Mesh \"Mesh\" (\n            prepend apiSchemas = [\"MaterialBindingAPI\"]\n        )\n        {{\n            \
             uniform bool doubleSided = 1\n            uniform token subdivisionScheme = \"none\"\n            \
             int[] faceVertexCounts = [{}]\n            int[] faceVertexIndices = [{}]\n            point3f[] points = [{}]\n            \
             normal3f[] normals = [{}] (\n                interpolation = \"vertex\"\n            )\n",
            vec!["3"; triangles].join(", "),
            list(&mut (0..triangles * 3).map(|i| i.to_string())),
            list(&mut (0..triangles * 3).map(|i| {
                let v = &vertices[i * VERTEX_FLOATS..];
                format!("({}, {}, {})", v[0], v[1], v[2])
            })),
            list(&mut (0..triangles * 3).map(|i| {
                let v = &vertices[i * VERTEX_FLOATS..];
                format!("({}, {}, {})", v[3], v[4], v[5])
            })),
        );
        if let (true, Some(c)) = (textured, coordinates) {
            let _ = write!(
                out,
                "            texCoord2f[] primvars:st = [{}] (\n                interpolation = \"vertex\"\n            )\n",
                list(&mut (0..triangles * 3).map(|i| {
                    let uv = c.get(i * 5..i * 5 + 2).unwrap_or(&[0., 0.]);
                    format!("({}, {})", uv[0], 1. - uv[1])
                }))
            );
        }
        let looks: Vec<_> = groups.keys().copied().collect();
        if looks.len() == 1 {
            let _ = writeln!(out, "            rel material:binding = <{base}/Looks/M0>");
        } else {
            for (i, faces) in groups.values().enumerate() {
                let _ = write!(
                    out,
                    "            def GeomSubset \"G{i}\" (\n                prepend apiSchemas = [\"MaterialBindingAPI\"]\n            )\n            {{\n                \
                     uniform token elementType = \"face\"\n                uniform token familyName = \"materialBind\"\n                \
                     int[] indices = [{}]\n                rel material:binding = <{base}/Looks/M{i}>\n            }}\n",
                    list(&mut faces.iter().map(usize::to_string))
                );
            }
        }
        out.push_str("        }\n        def Scope \"Looks\"\n        {\n");
        for (i, look) in looks.iter().enumerate() {
            let material = format!("{base}/Looks/M{i}");
            let [r, g, b, a] = look.rgba.map(|c| f64::from(c) / 255.);
            let color = format!("({:.6}, {:.6}, {:.6})", linear(r), linear(g), linear(b));
            let file = match (look.layer, texture) {
                (layer, Some(t)) if layer >= 0 => self.texture(key, t, layer as u32),
                _ => None,
            };
            let _ = write!(
                out,
                "            def Material \"M{i}\"\n            {{\n                \
                 token outputs:surface.connect = <{material}/Surface.outputs:surface>\n                \
                 def Shader \"Surface\"\n                {{\n                    uniform token info:id = \"UsdPreviewSurface\"\n"
            );
            match &file {
                Some(_) => {
                    let _ = writeln!(
                        out,
                        "                    color3f inputs:diffuseColor.connect = <{material}/Texture.outputs:rgb>"
                    );
                }
                None => {
                    let _ = writeln!(
                        out,
                        "                    color3f inputs:diffuseColor = {color}"
                    );
                }
            }
            let opacity = match &file {
                // Material alpha scales the texel alpha, as the renderer does.
                Some(_) => format!("float inputs:opacity.connect = <{material}/Texture.outputs:a>"),
                None => format!("float inputs:opacity = {a:.6}"),
            };
            let _ = write!(
                out,
                "                    {opacity}\n                    float inputs:roughness = 0.8\n                    \
                 token outputs:surface\n                }}\n"
            );
            if let Some(file) = file {
                let wrap = |w: u8| match w {
                    1 => "clamp",
                    2 => "mirror",
                    _ => "repeat",
                };
                let _ = write!(
                    out,
                    "                def Shader \"Texture\"\n                {{\n                    uniform token info:id = \"UsdUVTexture\"\n                    \
                     asset inputs:file = @{file}@\n                    token inputs:sourceColorSpace = \"sRGB\"\n                    \
                     float4 inputs:scale = ({:.6}, {:.6}, {:.6}, {a:.6})\n                    token inputs:wrapS = \"{}\"\n                    \
                     token inputs:wrapT = \"{}\"\n                    float2 inputs:st.connect = <{material}/Reader.outputs:result>\n                    \
                     float3 outputs:rgb\n                    float outputs:a\n                }}\n                def Shader \"Reader\"\n                {{\n                    \
                     uniform token info:id = \"UsdPrimvarReader_float2\"\n                    string inputs:varname = \"st\"\n                    \
                     float2 outputs:result\n                }}\n",
                    linear(r),
                    linear(g),
                    linear(b),
                    wrap(look.wrap[0]),
                    wrap(look.wrap[1]),
                );
            }
            out.push_str("            }\n");
        }
        out.push_str("        }\n    }\n");
        self.body.push_str(&out);
    }
}
