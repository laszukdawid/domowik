//! Plan-local user models and original 2D footprint semantics (BEHAVIOR-SPEC
//! §10.1/§10.4). Original asset bytes are preserved. Independently implemented
//! format decoders derive size/art; no external application catalog is parsed.

use super::*;
use domowik_format::{AssetInstance, InstanceTransform};
use domowik_geometry::spatial::SpatialIndex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::Path;

/// Cached decoded model meshes by asset digest.
pub(crate) type SceneModels =
    std::collections::BTreeMap<String, std::sync::Arc<domowik_models::ModelMesh>>;

/// A renderer-view baseline, keyed by document/content revision and visibility.
/// Stores nodes/framing and the scene's own small derived meshes (slabs, join
/// patches, the unit cube) for picking, never another copy of decoded model
/// vertices. Its nodes hold authoritative, untinted poses, so cancelling a
/// preview or changing selection restores them directly.
pub(super) struct CachedScene {
    token: u64,
    revision: ContentRevision,
    levels: Vec<Uuid>,
    ghosted: Vec<Uuid>,
    origin: [f64; 2],
    center: [f32; 3],
    radius: f32,
    diagnostics: Vec<String>,
    nodes: Vec<domowik_scene::Node>,
    meshes: BTreeMap<String, Vec<f32>>,
    mesh_bounds: HashMap<String, domowik_scene::pick::Bounds>,
}

fn update_pose_node(
    plan: &Plan,
    asset: &AssetInstance,
    origin: [f64; 2],
    node: &mut domowik_scene::Node,
) {
    let Some(level) = plan.session().document().level(asset.level_id) else {
        return;
    };
    let ratio = plan
        .scene_models
        .get(&node.mesh)
        .map(|m| m.height_ratio)
        .map(|h| if h > 0. { h } else { 0.001 })
        .unwrap_or(1.);
    let p = domowik_scene::asset_pose::placement(asset, origin, level.elevation_mm, ratio);
    (node.matrix, node.normal_matrix) =
        domowik_scene::asset_pose::matrices(p.position_m, p.scale_m, p.angles_deg, ratio);
}

fn cached_pose_scene(
    plan: &Plan,
    levels: &[Uuid],
    ghosted: &[Uuid],
    token: Option<u64>,
) -> Option<domowik_scene::Scene> {
    let cache = plan.view_scene.as_ref()?;
    if token != Some(cache.token)
        || cache.revision != plan.content_revision()
        || cache.levels != levels
        || cache.ghosted != ghosted
    {
        return None;
    }
    let nodes = presented_nodes(plan, cache).into_owned();
    // Names-only table: receivers must reuse resident mesh buffers. Keeping
    // names lets scene-wire retain its indexed-instance representation.
    let meshes = nodes.iter().map(|n| (n.mesh.clone(), Vec::new())).collect();
    Some(domowik_scene::Scene {
        instance_only: true,
        scene_token: cache.token,
        pose_version: plan.pose_version,
        origin_mm: cache.origin,
        center: cache.center,
        radius: cache.radius,
        diagnostics: cache.diagnostics.clone(),
        nodes,
        meshes,
        required_models: vec![],
    })
}

/// Plan entity ids among editor selection ids. Selection is a display hint:
/// ids that are not entities are ignored rather than refusing the request.
fn entity_set(ids: &[String]) -> HashSet<Uuid> {
    ids.iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect()
}

/// The cached view scene when it still matches the presenter's baseline and
/// the presenter has drawn the current pose overlay.
fn current_view_scene<'a>(
    plan: &'a Plan,
    levels: &[Uuid],
    ghosted: &[Uuid],
    token: u64,
    pose_version: u64,
) -> Option<&'a CachedScene> {
    plan.view_scene.as_ref().filter(|cache| {
        pose_version == plan.pose_version
            && cache.token == token
            && cache.revision == plan.content_revision()
            && cache.levels == levels
            && cache.ghosted == ghosted
    })
}

/// The cached nodes as presented: committed poses with the live pose overlay
/// applied, so queries see what the last instance update drew. Borrowed
/// unless an overlay is active.
fn presented_nodes<'a>(
    plan: &Plan,
    cache: &'a CachedScene,
) -> std::borrow::Cow<'a, [domowik_scene::Node]> {
    let Some(previews) = plan.pose_previews().filter(|p| !p.is_empty()) else {
        return std::borrow::Cow::Borrowed(&cache.nodes);
    };
    let mut nodes = cache.nodes.clone();
    for node in &mut nodes {
        let preview = Uuid::parse_str(&node.id)
            .ok()
            .and_then(|id| previews.get(&id));
        if let Some(preview) = preview {
            update_pose_node(plan, preview, cache.origin, node);
        }
    }
    std::borrow::Cow::Owned(nodes)
}

/// Local bounds for a node's mesh key: the scene's own meshes, else a model.
fn scene_bounds(
    plan: &Plan,
    cache: &CachedScene,
    key: &str,
) -> Option<domowik_scene::pick::Bounds> {
    cache
        .mesh_bounds
        .get(key)
        .or_else(|| plan.scene_model_bounds.get(key))
        .copied()
}

/// A node's mesh with its cached bounds: the scene's own, else a model.
fn scene_mesh<'a>(
    plan: &'a Plan,
    cache: &'a CachedScene,
    key: &str,
) -> Option<domowik_scene::pick::MeshView<'a>> {
    let vertices = cache
        .meshes
        .get(key)
        .or_else(|| plan.scene_models.get(key).map(|m| &m.vertices))?;
    Some(domowik_scene::pick::MeshView {
        vertices,
        bounds: scene_bounds(plan, cache, key)?,
    })
}

/// The first visible surface along a camera ray within `range` metres, in
/// the presented scene. Ghosted levels are transparent to picking.
/// `{"hit":null}` when the ray meets nothing.
fn pick_3d(
    plan: &Plan,
    cache: &CachedScene,
    ghosted: &[Uuid],
    range: [f64; 2],
    origin: [f64; 3],
    direction: [f64; 3],
) -> Value {
    use domowik_scene::pick::{self, EntityKind};
    let doc = plan.session().document();
    let ghosted_levels: HashSet<Uuid> = ghosted.iter().copied().collect();
    let mut hidden: HashSet<Uuid> = ghosted_levels.clone();
    let ghost_walls: HashSet<Uuid> = doc
        .walls
        .iter()
        .filter(|w| ghosted_levels.contains(&w.level_id))
        .map(|w| w.id)
        .collect();
    hidden.extend(&ghost_walls);
    hidden.extend(
        doc.openings
            .iter()
            .filter(|o| ghost_walls.contains(&o.host_wall_id))
            .map(|o| o.id),
    );
    hidden.extend(
        doc.asset_instances
            .iter()
            .filter(|a| ghosted_levels.contains(&a.level_id))
            .map(|a| a.id),
    );
    let nodes = presented_nodes(plan, cache);
    let hit = pick::pick(
        &nodes,
        origin,
        direction,
        range,
        |key| scene_mesh(plan, cache, key),
        |node| pick::node_entity(&node.id).is_none_or(|(_, id)| hidden.contains(&id)),
    );
    let Some(hit) = hit else {
        return json!({"hit": null});
    };
    let (kind, id) = pick::node_entity(&nodes[hit.node].id).expect("skip rejects unknown ids");
    let level = match kind {
        EntityKind::Structure => Some(id),
        EntityKind::Wall => doc.wall(id).map(|w| w.level_id),
        EntityKind::Opening => doc
            .openings
            .iter()
            .find(|o| o.id == id)
            .and_then(|o| doc.wall(o.host_wall_id))
            .map(|w| w.level_id),
        EntityKind::Asset => doc.asset_instance(id).map(|a| a.level_id),
    };
    json!({"hit": {
        "kind": kind.as_str(),
        "entityId": (kind != EntityKind::Structure).then_some(id),
        "levelId": level,
        "pointM": hit.point,
        "distanceM": hit.distance,
    }})
}

const DIRECTORIES: &str = "editorAssetDirectories";
const FOOTPRINT: &str = "editorFootprintMm";
const MAX_DIMENSION_MM: f64 = 1_000_000.0;
const MAX_PROVENANCE_TEXT_BYTES: usize = domowik_assets::descriptor::l19::DISPLAY_BYTES;

/// A plan-session clipboard of captured instances (issue 0113). The clones
/// keep their original levels and translations; paste rebases them so the
/// reference instance's origin lands at the pointer.
pub(crate) struct AssetClipboard {
    instances: Vec<AssetInstance>,
    reference: Uuid,
}

/// Like the wall hit cache, retain one active-level index per accepted revision.
pub(super) struct FootprintIndex {
    level: Uuid,
    revision: ContentRevision,
    instances: Vec<AssetInstance>,
    spatial: SpatialIndex<usize>,
    /// The conservative bounds each indexed instance contributed to the tree,
    /// in index order — the axis-aligned envelope of the displayed corners.
    /// Marquee candidacy (issue 0111) reuses them verbatim rather than
    /// recomputing, so one revision has one footprint per instance.
    footprint_bounds: Vec<(Aabb, Uuid)>,
}

impl FootprintIndex {
    fn build(
        doc: &Document,
        level: Uuid,
        revision: ContentRevision,
        degraded: &std::collections::HashSet<Uuid>,
    ) -> Self {
        let instances: Vec<_> = doc
            .asset_instances_on_level(level)
            .filter(|a| !degraded.contains(&a.id))
            .cloned()
            .collect();
        let mut footprint_bounds = Vec::new();
        let spatial = SpatialIndex::build(instances.iter().enumerate().filter_map(|(i, a)| {
            let points = corners(a);
            if points
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > DERIVED_COORD_BOUND_MM)
            {
                return None;
            }
            let min_x = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
            let min_y = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            let max_x = points
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let max_y = points
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max);
            let bounds = Aabb::from_corners(
                domowik_format::Point::new(min_x, min_y),
                domowik_format::Point::new(max_x, max_y),
            );
            footprint_bounds.push((bounds, a.id));
            Some((bounds, i))
        }));
        Self {
            level,
            revision,
            instances,
            spatial,
            footprint_bounds,
        }
    }

    /// Marquee candidacies for every indexed (resolved, non-degraded)
    /// instance on the built level (issue 0111): the conservative footprint
    /// bounds the tree was built from. Degraded instances never appear here;
    /// they keep their §9.6 placeholder candidacy in the geometry marquee.
    pub(super) fn footprints(&self) -> Vec<domowik_geometry::hit::AssetFootprint> {
        self.footprint_bounds
            .iter()
            .map(|&(bounds, instance)| domowik_geometry::hit::AssetFootprint { instance, bounds })
            .collect()
    }
}

/// Ensures `plan`'s footprint index matches `level` and the current content
/// revision — the issue-0012 reuse rule, shared by the `hit` request and the
/// marquee's asset candidacy (issue 0111) so both see one revision's verdicts.
pub(super) fn ensure_footprint_index(plan: &mut Plan, level_id: Uuid) {
    let revision = plan.content_revision();
    if !matches!(&plan.asset_hits, Some(index) if index.level == level_id && index.revision == revision)
    {
        let degraded = plan
            .resolutions()
            .iter()
            .filter(|e| e.state.is_degraded())
            .map(|e| e.instance_id)
            .collect();
        plan.asset_hits = Some(FootprintIndex::build(
            plan.session().document(),
            level_id,
            revision,
            &degraded,
        ));
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Request {
    List,
    #[serde(rename_all = "camelCase")]
    Scene {
        level_ids: Vec<Uuid>,
        #[serde(default)]
        ghosted_ids: Vec<Uuid>,
        #[serde(default)]
        selected_ids: Vec<String>,
        #[serde(default)]
        pose_only: bool,
        #[serde(default)]
        scene_token: Option<u64>,
    },
    Mesh {
        digest: String,
    },
    Preview {
        digest: String,
        #[serde(default, rename = "materialColors")]
        material_colors: domowik_format::appearance::MaterialColors,
    },
    Materials {
        digest: String,
    },
    #[serde(rename_all = "camelCase")]
    Colors {
        instance_id: Uuid,
        material_colors: domowik_format::appearance::MaterialColors,
    },
    #[serde(rename_all = "camelCase")]
    Thumbnail {
        digest: Option<String>,
        file: Option<String>,
        source_directory: Option<String>,
    },
    Size {
        digest: String,
    },
    Attach {
        directory: String,
    },
    #[serde(rename_all = "camelCase")]
    ReplaceDirectory {
        directory: String,
        previous_directory: String,
    },
    #[serde(rename_all = "camelCase")]
    Place {
        file: Option<String>,
        source_directory: Option<String>,
        digest: Option<String>,
        level_id: Uuid,
        position_mm: [f64; 2],
        dimensions_mm: Option<[f64; 2]>,
    },
    #[serde(rename_all = "camelCase")]
    Set {
        instance_id: Uuid,
        position_mm: [f64; 2],
        dimensions_mm: [f64; 2],
        rotation_deg: f64,
        height_mm: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Pose {
        instance_id: Uuid,
        position_mm: [f64; 3],
        rotation_deg: [f64; 3],
        height_mm: Option<f64>,
        #[serde(default)]
        preview: bool,
    },
    #[serde(rename_all = "camelCase")]
    TranslatePreview {
        instance_ids: Vec<Uuid>,
        delta_mm: [f64; 2],
    },
    ClearPosePreview,
    #[serde(rename_all = "camelCase")]
    Pick3d {
        level_ids: Vec<Uuid>,
        #[serde(default)]
        ghosted_ids: Vec<Uuid>,
        scene_token: u64,
        pose_version: u64,
        #[serde(default)]
        range_m: Option<[f64; 2]>,
        origin_m: [f64; 3],
        direction: [f64; 3],
    },
    #[serde(rename_all = "camelCase")]
    Bounds3d {
        level_ids: Vec<Uuid>,
        #[serde(default)]
        ghosted_ids: Vec<Uuid>,
        scene_token: u64,
        pose_version: u64,
        entity_ids: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Delete {
        instance_id: Uuid,
    },
    #[serde(rename_all = "camelCase")]
    Hit {
        level_id: Uuid,
        point_mm: [f64; 2],
        radius_mm: f64,
    },
    #[serde(rename_all = "camelCase")]
    Copy {
        instance_ids: Vec<Uuid>,
        reference_id: Uuid,
    },
    #[serde(rename_all = "camelCase")]
    Cut {
        instance_ids: Vec<Uuid>,
        reference_id: Uuid,
    },
    #[serde(rename_all = "camelCase")]
    Paste {
        level_id: Uuid,
        position_mm: [f64; 2],
    },
}

fn directories(plan: &Plan) -> Vec<String> {
    plan.file
        .as_ref()
        .and_then(|f| f.manifest.unknown.get(DIRECTORIES))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn dimensions(instance: &AssetInstance) -> [f64; 2] {
    instance
        .unknown
        .get(FOOTPRINT)
        .and_then(|v| serde_json::from_value::<[f64; 2]>(v.clone()).ok())
        .filter(|v| valid_dimensions(*v))
        .unwrap_or([1000.0, 1000.0])
}

fn valid_dimensions(d: [f64; 2]) -> bool {
    d.iter()
        .all(|v| v.is_finite() && *v > 0.0 && *v <= MAX_DIMENSION_MM)
}

fn validate(position: [f64; 2], size: [f64; 2], rotation: f64) -> Result<(), String> {
    if !valid_dimensions(size) {
        return Err("Width and depth must be positive and at most 1,000,000 mm".into());
    }
    if !rotation.is_finite()
        || position
            .iter()
            .any(|v| !v.is_finite() || v.abs() > domowik_format::COORD_BOUND_MM)
    {
        return Err("Position or rotation is outside the supported coordinate range".into());
    }
    Ok(())
}

/// Centered, rotated corners in mm. Trigonometry only transforms a user-sized
/// rectangle; no model bounds are inferred. Stable corner order: local lower
/// left, lower right, upper right, upper left.
fn corners(instance: &AssetInstance) -> [[f64; 2]; 4] {
    let [w, d] = interaction_dimensions(instance);
    let t = &instance.transform;
    let (sin, cos) = t.rotation_deg.rem_euclid(360.0).to_radians().sin_cos();
    [
        [-w / 2.0, -d / 2.0],
        [w / 2.0, -d / 2.0],
        [w / 2.0, d / 2.0],
        [-w / 2.0, d / 2.0],
    ]
    .map(|[x, y]| {
        [
            t.translation_mm[0] + t.scale_uniform * (cos * x - sin * y),
            t.translation_mm[1] + t.scale_uniform * (sin * x + cos * y),
        ]
    })
}

fn interaction_dimensions(instance: &AssetInstance) -> [f64; 2] {
    let [w, d] = dimensions(instance);
    let [_, rx, ry] = domowik_scene::asset_pose::extra_pose(instance);
    if rx == 0. && ry == 0. {
        return [w, d];
    }
    let h = instance
        .unknown
        .get("editorHeightMm")
        .and_then(Value::as_f64)
        .filter(|h| h.is_finite() && *h > 0.)
        .unwrap_or(w);
    domowik_scene::asset_pose::projected_dimensions([w, d, h], [rx, ry])
}

fn list(plan: &mut Plan) -> Result<Value, String> {
    let levels: Vec<_> = plan
        .session()
        .document()
        .levels
        .iter()
        .map(|l| l.id)
        .collect();
    let mut placeholders = HashMap::new();
    for level in levels {
        let boxes = plan.placeholders(level)?;
        placeholders.extend(boxes.into_iter().map(|p| (p.instance, p.bounds)));
    }
    let models: Vec<Value> = plan
        .file
        .as_ref()
        .into_iter()
        .flat_map(|f| f.descriptors().iter())
        .filter(|(_, d)| d["purpose"] == "furniture-model")
        .map(|(digest, d)| {
            json!({"digest": digest, "name": d["provenance"]["displayName"],
            "source": d["provenance"]["sourceDescription"]})
        })
        .collect();
    let mut instances: Vec<Value> = plan.session().document().asset_instances.iter().map(|a| plan.display_asset(a)).map(|a| json!({
        "id": a.id, "levelId": a.level_id, "digest": a.digest,
        "name": a.display_name.as_deref().unwrap_or("Model"),
        "positionMm": a.transform.translation_mm, "rotationDeg": a.transform.rotation_deg,
        "pose3d": domowik_scene::asset_pose::extra_pose(a),
        "materialColors": domowik_format::appearance::material_colors(a).unwrap_or_default(),
        "interactionDimensionsMm": interaction_dimensions(a),
        "scaleUniform": a.transform.scale_uniform, "dimensionsMm": dimensions(a), "cornersMm":
            placeholders.get(&a.id).map(|b| [[b.min_x,b.min_y],[b.max_x,b.min_y],[b.max_x,b.max_y],[b.min_x,b.max_y]]).unwrap_or_else(|| corners(a)).into_iter().filter(|p| p.iter().all(|v| v.is_finite() && v.abs() <= DERIVED_COORD_BOUND_MM)).collect::<Vec<_>>(),
        "heightMm": a.unknown.get("editorHeightMm").and_then(Value::as_f64)
            .filter(|h| h.is_finite() && *h > 0. && *h <= MAX_DIMENSION_MM)
    })).collect();
    instances.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(
        json!({"directories": directories(plan), "models": models, "instances": instances,
        "clipboardCount": plan.asset_clipboard.as_ref().map_or(0, |c| c.instances.len())}),
    )
}

/// The descriptor media type for a model file, from its extension.
fn model_media_type(path: &Path) -> Result<&'static str, String> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "glb" => Ok("model/gltf-binary"),
        "obj" => Ok("model/obj"),
        "zip" => Ok("application/zip"),
        "usdz" => Ok("model/vnd.usdz+zip"),
        "usd" | "usda" | "usdc" => Ok("model/vnd.pixar.usd"),
        _ => Err("Supported model files: GLB, OBJ, ZIP and USD (.usd, .usda, .usdc, .usdz)".into()),
    }
}

/// The §7.4 furniture-model descriptor for `bytes`, keyed by their digest.
fn model_descriptor(bytes: &[u8], media: &str, name: &str, source: &str) -> (String, Value) {
    let digest = domowik_container::digest_of(bytes);
    let descriptor = json!({"digest": digest, "byteLength": bytes.len(), "mediaType": media,
        "purpose": "furniture-model", "encoding": "raw", "requirement": "optional",
        "provenance": {"displayName": name, "sourceDescription": source}});
    (digest, descriptor)
}

/// Embeds an uploaded model in the plan's library without placing it.
///
/// For hosts that receive a file's bytes rather than a path (the browser
/// editor, issue 0088). `file_name` may be a bare filename or a relative/
/// absolute source path; paths are retained in `provenance.sourceDescription`.
/// Applies the same media-type rule, L5 size bound and descriptor as a
/// directory placement. Re-importing identical bytes keeps the first
/// descriptor. Returns `{digest, name}`.
pub(crate) fn import_model(
    plan: &mut Plan,
    file_name: &str,
    bytes: &[u8],
) -> Result<Value, String> {
    plan.session_mut().map_err(|e| e.to_string())?;
    let path = Path::new(file_name);
    let media = model_media_type(path)?;
    if bytes.is_empty() || bytes.len() as u64 > domowik_container::Limits::MAX_ENTRY_BYTES {
        return Err("Model is empty or exceeds the per-blob byte limit".into());
    }
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Model")
        .to_owned();
    let source = if file_name.contains('/') || file_name.contains('\\') {
        file_name.to_owned()
    } else {
        format!("uploaded file {file_name}")
    };
    if name.len() > MAX_PROVENANCE_TEXT_BYTES {
        return Err("Model name exceeds the asset descriptor byte limit".into());
    }
    if source.len() > MAX_PROVENANCE_TEXT_BYTES {
        return Err("Import path exceeds the asset descriptor byte limit".into());
    }
    let (digest, descriptor) = model_descriptor(bytes, media, &name, &source);
    let f = plan.ensure_file();
    let mut changed = false;
    let had_bytes = f.assets.contains_key(&digest);
    if !had_bytes {
        f.assets.insert(digest.clone(), bytes.to_vec());
        changed = true;
    }
    if !f.descriptors().contains_key(&digest) {
        f.insert_descriptor(&digest, descriptor)
            .expect("digest was computed from content");
        changed = true;
    }
    if changed {
        plan.metadata_dirty = true;
        plan.metadata_changes += 1;
        // The document revision does not move, yet the new bytes can resolve
        // an instance that was degraded for want of them: every cache keyed
        // on resolution must see a new key, and the editor reloads on it.
        if !had_bytes {
            plan.asset_revision += 1;
        }
    }
    let name = plan
        .file
        .as_ref()
        .and_then(|f| f.descriptors().get(&digest))
        .and_then(|d| d["provenance"]["displayName"].as_str().map(str::to_owned))
        .unwrap_or(name);
    Ok(json!({"digest": digest, "name": name}))
}

fn read_model(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("Model is not a regular file or exceeds the per-blob byte limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err("Model is empty or exceeds the per-blob byte limit".into());
    }
    Ok(bytes)
}

fn update_directory(
    plan: &mut Plan,
    directory: &str,
    previous: Option<&str>,
) -> Result<(), String> {
    plan.session_mut().map_err(|e| e.to_string())?;
    let path = std::fs::canonicalize(directory).map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("Choose a directory containing models".into());
    }
    let path = path
        .to_str()
        .ok_or("Directory path is not UTF-8")?
        .to_owned();
    let mut dirs = directories(plan);
    let original = dirs.clone();
    if let Some(previous) = previous {
        let index = dirs
            .iter()
            .position(|d| d == previous)
            .ok_or("Directory is no longer attached to this plan")?;
        if previous == path {
            return Ok(());
        }
        if dirs.contains(&path) {
            dirs.remove(index);
        } else {
            dirs[index] = path;
        }
    } else if !dirs.contains(&path) {
        if dirs.len() >= 64 {
            return Err("At most 64 asset directories may be attached".into());
        }
        dirs.push(path);
    }
    if dirs != original {
        plan.ensure_file()
            .manifest
            .unknown
            .insert(DIRECTORIES.into(), json!(dirs));
        plan.metadata_dirty = true;
        plan.metadata_changes += 1;
    }
    Ok(())
}

fn run(plan: &mut Plan, request: Request) -> Result<Value, String> {
    match request {
        Request::ClearPosePreview => {
            plan.set_pose_preview(None);
        }
        Request::Pick3d {
            level_ids,
            ghosted_ids,
            scene_token,
            pose_version,
            range_m,
            origin_m,
            direction,
        } => {
            let Some(cache) =
                current_view_scene(plan, &level_ids, &ghosted_ids, scene_token, pose_version)
            else {
                return Ok(json!({"stale": true}));
            };
            return Ok(pick_3d(
                plan,
                cache,
                &ghosted_ids,
                range_m.unwrap_or([0., f64::INFINITY]),
                origin_m,
                direction,
            ));
        }
        Request::Bounds3d {
            level_ids,
            ghosted_ids,
            scene_token,
            pose_version,
            entity_ids,
        } => {
            let Some(cache) =
                current_view_scene(plan, &level_ids, &ghosted_ids, scene_token, pose_version)
            else {
                return Ok(json!({"stale": true}));
            };
            let wanted = entity_set(&entity_ids);
            let presented = presented_nodes(plan, cache);
            let nodes = presented.iter().filter(|n| {
                domowik_scene::pick::node_entity(&n.id).is_some_and(|(kind, id)| {
                    kind != domowik_scene::pick::EntityKind::Structure && wanted.contains(&id)
                })
            });
            let bounds =
                domowik_scene::pick::world_bounds(nodes, |key| scene_bounds(plan, cache, key));
            return Ok(
                json!({"bounds": bounds.map(|(low, high)| json!({"lowM": low, "highM": high}))}),
            );
        }
        Request::TranslatePreview {
            instance_ids,
            delta_mm,
        } => {
            if delta_mm.iter().any(|v| !v.is_finite()) {
                return Err("Preview offset must be finite".into());
            }
            let degraded: std::collections::HashSet<Uuid> = plan
                .resolutions()
                .iter()
                .filter(|r| r.state.is_degraded())
                .map(|r| r.instance_id)
                .collect();
            let doc = plan.session().document();
            let mut previews = HashMap::with_capacity(instance_ids.len());
            for id in instance_ids {
                // Degraded models keep their committed pose until release.
                if degraded.contains(&id) {
                    continue;
                }
                let mut asset = doc
                    .asset_instance(id)
                    .ok_or("Unknown asset instance")?
                    .clone();
                let [x, y] = asset.transform.translation_mm;
                let moved = [x + delta_mm[0], y + delta_mm[1]];
                if moved
                    .iter()
                    .any(|v| v.abs() > domowik_format::COORD_BOUND_MM)
                {
                    return Err("Preview moves a model out of bounds".into());
                }
                asset.transform.translation_mm = moved;
                previews.insert(id, asset);
            }
            plan.set_pose_preview(Some((document_revision(plan.session()), previews)));
            return Ok(json!({}));
        }
        Request::Pose {
            instance_id,
            position_mm,
            rotation_deg,
            height_mm,
            preview,
        } => {
            if height_mm.is_some_and(|h| !h.is_finite() || h <= 0. || h > MAX_DIMENSION_MM) {
                return Err("Height must be positive and at most 1,000,000 mm".into());
            }
            if position_mm
                .iter()
                .any(|v| !v.is_finite() || v.abs() > domowik_format::COORD_BOUND_MM)
                || rotation_deg.iter().any(|v| !v.is_finite())
            {
                return Err("Pose requires bounded finite coordinates and finite angles".into());
            }
            if plan
                .resolutions()
                .iter()
                .any(|r| r.instance_id == instance_id && r.state.is_degraded())
            {
                return Err("Cannot position a degraded model".into());
            }
            let mut asset = plan
                .session()
                .document()
                .asset_instance(instance_id)
                .ok_or("Unknown asset instance")?
                .clone();
            asset.transform.translation_mm = [position_mm[0], position_mm[1]];
            asset.transform.rotation_deg = rotation_deg[2];
            if let Some(height) = height_mm {
                asset.unknown.insert("editorHeightMm".into(), json!(height));
            }
            for (key, value) in [
                "editorElevationMm",
                "editorRotationXDeg",
                "editorRotationYDeg",
            ]
            .into_iter()
            .zip([position_mm[2], rotation_deg[0], rotation_deg[1]])
            {
                asset.unknown.insert(key.into(), json!(value));
            }
            if preview {
                let revision = document_revision(plan.session());
                plan.set_pose_preview(Some((revision, HashMap::from([(instance_id, asset)]))));
            } else {
                let mut fields = serde_json::Map::from_iter([
                    ("transform".into(), json!(asset.transform)),
                    ("editorElevationMm".into(), json!(position_mm[2])),
                    ("editorRotationXDeg".into(), json!(rotation_deg[0])),
                    ("editorRotationYDeg".into(), json!(rotation_deg[1])),
                ]);
                if let Some(height) = height_mm {
                    fields.insert("editorHeightMm".into(), json!(height));
                }
                plan.session_mut()
                    .map_err(|e| e.to_string())?
                    .commit(vec![Operation::set_fields(instance_id, fields)])
                    .map_err(|e| e.to_string())?;
                plan.set_pose_preview(None);
            }
        }
        Request::List => return list(plan),
        Request::Preview { .. }
        | Request::Size { .. }
        | Request::Thumbnail { .. }
        | Request::Materials { .. } => {
            return Err("Model decoding must run outside the plan lock".into())
        }
        Request::Scene { .. } | Request::Mesh { .. } => {
            return Err("Scene decoding must run outside the plan lock".into())
        }
        Request::Hit {
            level_id,
            point_mm: [x, y],
            radius_mm,
        } => {
            if !x.is_finite()
                || !y.is_finite()
                || !radius_mm.is_finite()
                || !(0.0..=MAX_HIT_RADIUS_MM).contains(&radius_mm)
            {
                return Err("Invalid footprint hit query".into());
            }
            ensure_footprint_index(plan, level_id);
            let index = plan.asset_hits.as_ref().expect("just ensured");
            let mut hits: Vec<_> = index.spatial.near(domowik_format::Point::new(x, y), radius_mm).into_iter().filter_map(|i| {
                let a = &index.instances[i];
                let [w, d] = interaction_dimensions(a);
                let t = &a.transform;
                let (sin, cos) = t.rotation_deg.rem_euclid(360.0).to_radians().sin_cos();
                let dx = x - t.translation_mm[0];
                let dy = y - t.translation_mm[1];
                let local_x = cos * dx + sin * dy;
                let local_y = -sin * dx + cos * dy;
                let cx = local_x.clamp(-w * t.scale_uniform / 2.0, w * t.scale_uniform / 2.0);
                let cy = local_y.clamp(-d * t.scale_uniform / 2.0, d * t.scale_uniform / 2.0);
                let distance = (local_x - cx).hypot(local_y - cy);
                (distance <= radius_mm).then(|| json!({"class":"asset-footprint", "entityId": a.id,
                    "closestMm": [t.translation_mm[0] + cos * cx - sin * cy, t.translation_mm[1] + sin * cx + cos * cy]}))
            }).collect();
            hits.sort_by(|a, b| a["entityId"].as_str().cmp(&b["entityId"].as_str()));
            return Ok(json!(hits));
        }
        Request::Attach { directory } => {
            update_directory(plan, &directory, None)?;
        }
        Request::ReplaceDirectory {
            directory,
            previous_directory,
        } => {
            update_directory(plan, &directory, Some(&previous_directory))?;
        }
        Request::Place {
            file,
            source_directory,
            digest,
            level_id,
            position_mm,
            dimensions_mm,
        } => {
            if let Some(size) = dimensions_mm {
                validate(position_mm, size, 0.0)?;
            }
            plan.session_mut().map_err(|e| e.to_string())?;
            if plan.session().document().level(level_id).is_none() {
                return Err("Unknown level".into());
            }
            let source = source_directory
                .map(|p| {
                    let root = std::fs::canonicalize(p).map_err(|e| e.to_string())?;
                    if !root.is_dir() {
                        return Err("Source library is not a directory".to_owned());
                    }
                    root.to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "Source library path is not UTF-8".to_owned())
                })
                .transpose()?;
            if source
                .as_ref()
                .is_some_and(|p| !directories(plan).contains(p))
                && directories(plan).len() >= 64
            {
                return Err("Plan already has 64 asset directory locations".into());
            }
            let (digest, bytes, descriptor, name) = match (file, digest) {
                (Some(file), None) => {
                    let path = std::fs::canonicalize(&file).map_err(|e| e.to_string())?;
                    if !source
                        .as_ref()
                        .is_some_and(|root| path.starts_with(Path::new(root)))
                        && !directories(plan)
                            .iter()
                            .any(|d| path.starts_with(Path::new(d)))
                    {
                        return Err("Model must belong to an attached directory".into());
                    }
                    let media = model_media_type(&path)?;
                    let bytes = read_model(&path, domowik_container::Limits::MAX_ENTRY_BYTES)?;
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Model")
                        .to_owned();
                    let (digest, descriptor) =
                        model_descriptor(&bytes, media, &name, &path.to_string_lossy());
                    (digest, Some(bytes), Some(descriptor), name)
                }
                (None, Some(digest)) => {
                    let f = plan.file.as_ref().ok_or("No embedded model library")?;
                    if !f.assets.contains_key(&digest) {
                        return Err("Model bytes are unavailable; reattach its directory".into());
                    }
                    let d = f
                        .descriptors()
                        .get(&digest)
                        .ok_or("Missing model descriptor")?;
                    if d["purpose"] != "furniture-model" {
                        return Err("Asset is not a furniture model".into());
                    }
                    let name = d["provenance"]["displayName"]
                        .as_str()
                        .unwrap_or("Model")
                        .to_owned();
                    (digest, None, None, name)
                }
                _ => return Err("Choose exactly one file or embedded model".into()),
            };
            let content = bytes
                .as_deref()
                .or_else(|| {
                    plan.file
                        .as_ref()
                        .and_then(|f| f.assets.get(&digest))
                        .map(Vec::as_slice)
                })
                .ok_or("Model bytes are unavailable")?;
            let (size, sizing_source, diagnostic) = match domowik_models::dimensions(content) {
                Ok(measured) => (
                    measured.dimensions_mm,
                    if measured.physical_units {
                        "physical"
                    } else {
                        "proportions"
                    },
                    None,
                ),
                Err(error) => ([1000.; 3], "fallback", Some(error)),
            };
            let footprint = dimensions_mm.unwrap_or([size[0], size[1]]);
            let height = if dimensions_mm.is_some() {
                // Legacy bridge callers may supply a footprint. Keep a uniform
                // vertical scale relative to its width, while new GUI placement
                // always uses the measured full size.
                size[2] * footprint[0] / size[0]
            } else {
                size[2]
            };
            validate(position_mm, footprint, 0.0)?;
            if !height.is_finite() || height <= 0. || height > MAX_DIMENSION_MM {
                return Err("Model height exceeds the supported dimension range".into());
            }
            let id = Uuid::now_v7();
            let instance = AssetInstance {
                id,
                level_id,
                digest: digest.clone(),
                transform: InstanceTransform {
                    translation_mm: position_mm,
                    ..Default::default()
                },
                display_name: Some(name),
                unknown: serde_json::Map::from_iter([
                    (FOOTPRINT.into(), json!(footprint)),
                    ("editorHeightMm".into(), json!(height)),
                ]),
            };
            // Commit first: failure leaves package metadata and content untouched.
            plan.session_mut()
                .map_err(|e| e.to_string())?
                .commit(vec![Operation::create(Entity::AssetInstance(instance))])
                .map_err(|e| e.to_string())?;
            let f = plan.ensure_file();
            if let Some(source) = source {
                let mut dirs = f
                    .manifest
                    .unknown
                    .get(DIRECTORIES)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !dirs.contains(&json!(source)) {
                    dirs.push(json!(source));
                    f.manifest.unknown.insert(DIRECTORIES.into(), json!(dirs));
                }
            }
            if let Some(bytes) = bytes {
                f.assets.entry(digest.clone()).or_insert(bytes);
            }
            if let Some(descriptor) = descriptor {
                if !f.descriptors().contains_key(&digest) {
                    f.insert_descriptor(&digest, descriptor)
                        .expect("digest was computed from content");
                }
            }
            return Ok(
                json!({"instanceId": id, "sizingSource":sizing_source, "sizingDiagnostic":diagnostic}),
            );
        }
        Request::Set {
            instance_id,
            position_mm,
            dimensions_mm,
            rotation_deg,
            height_mm,
        } => {
            validate(position_mm, dimensions_mm, rotation_deg)?;
            if height_mm.is_some_and(|h| !h.is_finite() || h <= 0. || h > MAX_DIMENSION_MM) {
                return Err("Height must be positive and at most 1,000,000 mm".into());
            }
            let a = plan
                .session()
                .document()
                .asset_instance(instance_id)
                .ok_or("Unknown asset instance")?;
            let mut transform = a.transform.clone();
            transform.translation_mm = position_mm;
            transform.rotation_deg = rotation_deg;
            let mut fields = serde_json::Map::from_iter([
                ("transform".into(), json!(transform)),
                (FOOTPRINT.into(), json!(dimensions_mm)),
            ]);
            if let Some(height) = height_mm {
                fields.insert("editorHeightMm".into(), json!(height));
            }
            plan.session_mut()
                .map_err(|e| e.to_string())?
                .commit(vec![Operation::set_fields(instance_id, fields)])
                .map_err(|e| e.to_string())?;
        }
        Request::Colors {
            instance_id,
            material_colors,
        } => {
            let operations = domowik_edit::appearance::asset(
                plan.session().document(),
                instance_id,
                &material_colors,
            )?;
            if !operations.is_empty() {
                plan.session_mut()
                    .map_err(|e| e.to_string())?
                    .commit(operations)
                    .map_err(|e| e.to_string())?;
            }
        }
        Request::Delete { instance_id } => {
            if plan
                .session()
                .document()
                .asset_instance(instance_id)
                .is_none()
            {
                return Err("Unknown asset instance".into());
            }
            plan.session_mut()
                .map_err(|e| e.to_string())?
                .commit(vec![Operation::delete(instance_id)])
                .map_err(|e| e.to_string())?;
        }
        Request::Copy {
            instance_ids,
            reference_id,
        } => {
            let instances = clipboard_snapshot(plan, &instance_ids, reference_id)?;
            plan.asset_clipboard = Some(AssetClipboard {
                instances,
                reference: reference_id,
            });
        }
        Request::Cut {
            instance_ids,
            reference_id,
        } => {
            // Capture first, delete second: a refused delete (required content
            // unresolved) must leave the clipboard exactly as it was, and the
            // snapshot must describe what existed BEFORE the batch.
            let instances = clipboard_snapshot(plan, &instance_ids, reference_id)?;
            let operations: Vec<Operation> = instances
                .iter()
                .map(|instance| Operation::delete(instance.id))
                .collect();
            plan.session_mut()
                .map_err(|e| e.to_string())?
                .commit(operations)
                .map_err(|e| e.to_string())?;
            plan.asset_clipboard = Some(AssetClipboard {
                instances,
                reference: reference_id,
            });
        }
        Request::Paste {
            level_id,
            position_mm,
        } => {
            let (operations, reference) = paste_operations(plan, level_id, position_mm)?;
            plan.session_mut()
                .map_err(|e| e.to_string())?
                .commit(operations)
                .map_err(|e| e.to_string())?;
            let mut value = list(plan)?;
            value["instanceId"] = json!(reference);
            return Ok(value);
        }
    }
    list(plan)
}

/// Validates a copy/cut request and captures its instances in request order.
///
/// Every named instance must exist and the anchor must be one of them: one
/// unknown id refuses the whole request rather than copying a subset. Duplicate
/// ids collapse to their first occurrence. Instances are captured with their
/// ORIGINAL level and translation; only paste rebases them.
fn clipboard_snapshot(
    plan: &Plan,
    instance_ids: &[Uuid],
    reference_id: Uuid,
) -> Result<Vec<AssetInstance>, String> {
    if instance_ids.is_empty() {
        return Err("Nothing is selected to copy".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut instances = Vec::new();
    for id in instance_ids {
        if !seen.insert(*id) {
            continue;
        }
        let instance = plan
            .session()
            .document()
            .asset_instance(*id)
            .ok_or_else(|| format!("unknown asset instance {id}"))?;
        instances.push(instance.clone());
    }
    if !seen.contains(&reference_id) {
        return Err("The copy anchor must be one of the copied instances".into());
    }
    Ok(instances)
}

/// Builds the create operations for one paste (issue 0113).
///
/// Every captured instance is cloned with a fresh identity onto `level_id`,
/// translated so the clipboard anchor's origin lands at `position_mm`.
/// Relative arrangement, transform, resize metadata and pose are preserved
/// verbatim. Every captured digest must still have embedded bytes, so a paste
/// cannot silently degrade or duplicate an unresolved model. Returns the
/// operations and the new anchor id.
fn paste_operations(
    plan: &Plan,
    level_id: Uuid,
    position_mm: [f64; 2],
) -> Result<(Vec<Operation>, Uuid), String> {
    let clipboard = plan
        .asset_clipboard
        .as_ref()
        .ok_or("The clipboard is empty")?;
    if plan.session().document().level(level_id).is_none() {
        return Err("Unknown level".into());
    }
    if !position_mm
        .iter()
        .all(|v| v.is_finite() && v.abs() <= domowik_format::COORD_BOUND_MM)
    {
        return Err("Paste position is outside the supported coordinate range".into());
    }
    let reference = clipboard
        .instances
        .iter()
        .find(|a| a.id == clipboard.reference)
        .ok_or("The clipboard anchor is missing")?;
    let delta = [
        position_mm[0] - reference.transform.translation_mm[0],
        position_mm[1] - reference.transform.translation_mm[1],
    ];
    // The digest must still resolve: placing by digest refuses the same way,
    // and a placeholder would not be the furniture the user copied.
    let file = plan.file.as_ref().ok_or("No embedded model library")?;
    for instance in &clipboard.instances {
        if !file.assets.contains_key(&instance.digest) {
            return Err("Model bytes are unavailable; reattach its directory".into());
        }
    }
    let mut operations = Vec::with_capacity(clipboard.instances.len());
    let mut new_reference = None;
    for instance in &clipboard.instances {
        let mut copy = instance.clone();
        copy.id = Uuid::now_v7();
        copy.level_id = level_id;
        copy.transform.translation_mm = [
            instance.transform.translation_mm[0] + delta[0],
            instance.transform.translation_mm[1] + delta[1],
        ];
        if copy.transform.translation_mm.iter().any(|v| !v.is_finite()) {
            return Err("Pasted position is outside the supported coordinate range".into());
        }
        if instance.id == clipboard.reference {
            new_reference = Some(copy.id);
        }
        operations.push(Operation::create(Entity::AssetInstance(copy)));
    }
    Ok((
        operations,
        new_reference.expect("the clipboard anchor is a captured instance"),
    ))
}

/// Derives the renderer-neutral scene for `level_ids` from one revision of
/// `plan`, decoding and caching any models those levels need. Returns the
/// scene and every cached model mesh by digest. Shared by the JSON scene
/// request and the macOS renderer's direct load (issue 0101), so both see the
/// same document, floors and model cache. The caller holds the plan's lease;
/// the plan lock is taken only for short copies, never across decoding.
pub(crate) fn derive_scene(
    plan: &Arc<Mutex<Plan>>,
    level_ids: &[Uuid],
    ghosted_ids: Vec<Uuid>,
    selected_ids: &[String],
    pose_only: bool,
    scene_token: Option<u64>,
) -> Result<(domowik_scene::Scene, SceneModels), String> {
    if level_ids.len() > 256 {
        return Err("Too many visible 3D levels".into());
    }
    let selected = entity_set(selected_ids);
    if pose_only {
        let mut locked = plan.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut scene) = cached_pose_scene(&locked, level_ids, &ghosted_ids, scene_token) {
            locked.pose_scene_updates += 1;
            domowik_scene::pick::highlight(&mut scene.nodes, &selected);
            return Ok((scene, SceneModels::new()));
        }
    }
    let variant = |asset: &AssetInstance| {
        let colors = domowik_format::appearance::material_colors(asset)?;
        if colors.is_empty() {
            return Ok(None);
        }
        let key = domowik_container::digest_of(
            &serde_json::to_vec(&("solid-colors-v1", &asset.digest, &colors))
                .map_err(|e| e.to_string())?,
        );
        Ok::<_, String>(Some((key, colors)))
    };
    // One lock acquisition captures the document, its retained per-level
    // floors and the model cache together, so all describe one revision.
    let (mut doc, floors, mut models, missing, revision, (preview_ids, pose_version)) = {
        let mut guard = plan.lock().unwrap_or_else(|e| e.into_inner());
        let locked = &mut *guard;
        // Every Apply of a new palette makes a variant (a full mesh and texture
        // copy). Keep only those the live document uses, on any level, so
        // trying colours cannot fill the cache. Evicting here, under the
        // snapshot's lock and before new models are admitted, means a stale
        // derivation never drops a newer scene's variant. Undoing to an
        // evicted palette recolours again without decoding.
        let used: std::collections::BTreeSet<String> = locked
            .session()
            .document()
            .asset_instances
            .iter()
            // Invalid colours on another level must not fail this scene.
            .filter_map(|a| variant(a).ok().flatten().map(|(key, _)| key))
            .collect();
        let stale: Vec<String> = locked
            .scene_color_variants
            .difference(&used)
            .cloned()
            .collect();
        for key in stale {
            locked.scene_color_variants.remove(&key);
            locked.scene_models.remove(&key);
            locked.scene_model_bounds.remove(&key);
        }
        let mut doc = locked.session().document().clone();
        let mut preview_ids = Vec::new();
        let pose_version = locked.pose_version;
        if let Some(previews) = locked.pose_previews() {
            for asset in &mut doc.asset_instances {
                if let Some(preview) = previews.get(&asset.id) {
                    *asset = preview.clone();
                    preview_ids.push(asset.id);
                }
            }
        }
        let floors: std::collections::BTreeMap<_, _> = doc
            .levels
            .iter()
            .filter(|l| level_ids.contains(&l.id))
            .map(|l| (l.id, locked.synced_snap(l.id).geometry().rooms()))
            .collect();
        let missing = doc
            .asset_instances
            .iter()
            .filter(|a| {
                level_ids.contains(&a.level_id)
                    && !locked.scene_models.contains_key(&a.digest)
                    && !locked.scene_model_errors.contains_key(&a.digest)
            })
            .map(|a| a.digest.clone())
            .collect::<std::collections::BTreeSet<_>>();
        (
            doc,
            floors,
            locked.scene_models.clone(),
            missing,
            locked.content_revision(),
            (preview_ids, pose_version),
        )
    };
    for digest in missing {
        let bytes = {
            let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
            locked
                .file
                .as_ref()
                .and_then(|f| f.assets.get(&digest))
                .filter(|b| b.len() <= 64 * 1024 * 1024)
                .cloned()
        };
        if let Some(bytes) = bytes {
            match domowik_models::mesh_with_budget(&bytes, 5000) {
                Ok(mesh) => {
                    let bounds = domowik_scene::pick::mesh_bounds(&mesh.vertices);
                    let mesh = std::sync::Arc::new(mesh);
                    let mut locked = plan.lock().unwrap_or_else(|e| e.into_inner());
                    let total = locked
                        .scene_models
                        .values()
                        .map(|m| m.cache_bytes())
                        .sum::<usize>();
                    if total + mesh.cache_bytes() <= 128 * 1024 * 1024 {
                        locked.scene_model_decodes += 1;
                        locked.scene_models.insert(digest.clone(), mesh.clone());
                        if let Some(bounds) = bounds {
                            locked.scene_model_bounds.insert(digest.clone(), bounds);
                        }
                        models.insert(digest, mesh);
                    } else {
                        locked
                            .scene_model_errors
                            .insert(digest, "Decoded 3D model cache exceeds 128 MiB".into());
                    }
                }
                Err(error) => {
                    plan.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .scene_model_errors
                        .insert(digest, error);
                }
            }
        }
    }
    // Colour variants are immutable content-addressed meshes. Presenters never
    // replace a resident model's bytes under an existing key.
    for asset in &mut doc.asset_instances {
        if !level_ids.contains(&asset.level_id) {
            continue;
        }
        let Some((key, colors)) = variant(asset)? else {
            continue;
        };
        let Some(base) = models.get(&asset.digest).cloned() else {
            continue;
        };
        if let std::collections::btree_map::Entry::Vacant(entry) = models.entry(key.clone()) {
            let mesh = Arc::new(base.recolored(&colors));
            let mut locked = plan.lock().unwrap_or_else(|e| e.into_inner());
            // A stale derivation keeps its variant to itself: caching it would
            // spend the budget on a palette the document may no longer use.
            if locked.content_revision() != revision {
                entry.insert(mesh);
                asset.digest = key;
                continue;
            }
            let total: usize = locked.scene_models.values().map(|m| m.cache_bytes()).sum();
            let size = mesh.cache_bytes();
            // Known limit, kept for simplicity: each variant copies its model's
            // texture, so many distinct palettes in use at once on large
            // textured models can still exceed the budget. Sharing texture
            // pixels between variants would lift it; plans are not that heavy.
            // The Linux presenter has the same limit for its GPU textures.
            if total + size > 128 * 1024 * 1024 {
                return Err("Decoded 3D colour cache exceeds 128 MiB".into());
            }
            if let Some(bounds) = locked.scene_model_bounds.get(&asset.digest).copied() {
                locked.scene_model_bounds.insert(key.clone(), bounds);
            }
            locked.scene_color_variants.insert(key.clone());
            locked.scene_models.insert(key.clone(), mesh.clone());
            entry.insert(mesh);
        }
        asset.digest = key;
    }
    let heights = models
        .iter()
        .map(|(d, m)| (d.clone(), m.height_ratio))
        .collect();
    let mut scene = domowik_scene::build_with_floors(
        &doc,
        level_ids,
        &ghosted_ids.iter().copied().collect(),
        &heights,
        &floors,
    )?;
    // Slabs, join patches and the cube are small; bound them once per build.
    let mesh_bounds: HashMap<_, _> = scene
        .meshes
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), domowik_scene::pick::mesh_bounds(v)?)))
        .collect();
    {
        let mut locked = plan.lock().unwrap_or_else(|e| e.into_inner());
        if locked.content_revision() == revision {
            locked.scene_builds += 1;
            scene.scene_token = locked.scene_builds;
            scene.pose_version = pose_version;
            let mut nodes = scene.nodes.clone();
            // The cache keeps committed poses; pose-only reads re-apply the overlay.
            for id in preview_ids {
                let text_id = id.to_string();
                if let (Some(asset), Some(node)) = (
                    locked.session().document().asset_instance(id),
                    nodes.iter_mut().find(|n| n.id == text_id),
                ) {
                    update_pose_node(&locked, asset, scene.origin_mm, node);
                }
            }
            locked.view_scene = Some(CachedScene {
                token: scene.scene_token,
                revision,
                levels: level_ids.to_vec(),
                ghosted: ghosted_ids,
                origin: scene.origin_mm,
                center: scene.center,
                radius: scene.radius,
                diagnostics: scene.diagnostics.clone(),
                nodes,
                meshes: scene.meshes.clone(),
                mesh_bounds,
            });
        }
    }
    domowik_scene::pick::highlight(&mut scene.nodes, &selected);
    Ok((scene, models))
}

/// JSON asset requests on one plan. Uses the same serialization lock, lease,
/// panic containment and buffer ownership as `domowik_plan_execute`. No callbacks
/// or network access; file reads are synchronous and bounded by L5. Cancellation
/// is not supported during this bounded import. Available on packaged bridge
/// platforms with filesystem access; directory permissions belong to the host.
/// `pose` accepts positionMm [plan-x, plan-y, elevation-offset] and rotationDeg
/// [plan-X, plan-Y, plan-Z] plus optional unscaled heightMm. With preview:true it
/// changes only view derivation; the document, save revision and undo are
/// untouched. `translatePreview` replaces the overlay with `instanceIds`
/// shifted by `deltaMm` [plan-x, plan-y] from their committed poses, skipping
/// degraded models; it is refused for unknown ids or out-of-bound results.
/// `clearPosePreview` discards that plan-local overlay. Preview is invalidated
/// by an accepted document revision and dies with the plan handle.
///
/// `pick3d` and `bounds3d` query the last presented 3D scene identified by
/// `levelIds`, `ghostedIds` and `sceneToken`, in that scene's origin-rebased
/// metres. `pick3d` casts `originM` along `direction` and answers
/// `{"hit":{kind,entityId,levelId,pointM,distanceM}}` (`entityId` is null for
/// floors and join patches) or `{"hit":null}`; ghosted levels are skipped,
/// and optional `rangeM` [near, far] limits hits to the camera's visible
/// distances along the ray. `bounds3d` answers `{"bounds":{lowM,highM}}` for
/// `entityIds`, or null. Both name the `poseVersion` of the last presented
/// frame and answer `{"stale":true}` when that scene, or the pose overlay it
/// drew, is no longer current.
/// Both are view queries, linear in scene nodes, and never change the plan.
///
/// `copy`/`cut` capture asset instances into the plan-local clipboard (issue
/// 0113); `cut` also deletes them in one batch, refused atomically. `paste`
/// creates fresh-identity copies on a level, anchored at a point; it is
/// refused when the clipboard is empty or a digest's bytes are unavailable.
/// The clipboard is view state: not persisted, not in the operation log, never
/// restored by undo, and gone with the plan handle. `list` reports its size as
/// `clipboardCount`.
///
/// # Safety
/// `req` addresses `req_len` readable bytes for the call; `out` is writable.
/// Success transfers one token-owned buffer, released by `domowik_buffer_free`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_assets(
    handle: u64,
    req: *const u8,
    req_len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let Some(bytes) = borrow(req, req_len) else {
            return status::INVALID_ARGUMENT;
        };
        let request = match serde_json::from_slice(bytes) {
            Ok(r) => r,
            Err(e) => return fail(&plan, status::MALFORMED_REQUEST, e.to_string()),
        };
        if let Request::Thumbnail {
            digest,
            file,
            source_directory,
        } = request
        {
            // Read-only: no ensure_file/import/commit or metadata updates. The
            // lease lasts through bounded I/O and decoding, but neither holds
            // the session lock. A configured root is borrowed for this request,
            // not attached to the plan just because someone browsed it.
            let limit = domowik_container::Limits::MAX_ENTRY_BYTES.min(64 * 1024 * 1024);
            let bytes = match (digest, file) {
                (Some(digest), None) if source_directory.is_none() => {
                    let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
                    locked
                        .file
                        .as_ref()
                        .and_then(|f| f.assets.get(&digest))
                        .filter(|b| b.len() as u64 <= limit)
                        .cloned()
                        .ok_or_else(|| {
                            "Model bytes are unavailable or exceed the 64 MiB preview limit"
                                .to_owned()
                        })
                }
                (None, Some(file)) => {
                    let mut roots = {
                        let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
                        directories(&locked)
                    };
                    if let Some(root) = source_directory {
                        roots.push(root);
                    }
                    (|| {
                        let path = std::fs::canonicalize(file).map_err(|e| e.to_string())?;
                        model_media_type(&path)?;
                        if !roots
                            .iter()
                            .filter_map(|p| std::fs::canonicalize(p).ok())
                            .any(|root| root.is_dir() && path.starts_with(root))
                        {
                            return Err(
                                "Model must belong to a selected library directory".to_owned()
                            );
                        }
                        read_model(&path, limit)
                    })()
                }
                _ => Err("Choose exactly one embedded model or library file".to_owned()),
            };
            let result = bytes
                .and_then(|bytes| domowik_models::thumbnail(&bytes))
                .and_then(|preview| serde_json::to_vec(&preview).map_err(|e| e.to_string()));
            return match result {
                Ok(bytes) => release(bytes, out),
                Err(e) => fail(&plan, status::REJECTED, e),
            };
        }
        if let Request::Mesh { ref digest } = request {
            let mesh = {
                let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
                locked.scene_models.get(digest).cloned()
            };
            return match mesh.and_then(|m| serde_json::to_vec(m.as_ref()).ok()) {
                Some(bytes) => release(bytes, out),
                None => fail(
                    &plan,
                    status::REJECTED,
                    "Model mesh is not available; refresh the scene first",
                ),
            };
        }
        if let Request::Scene {
            level_ids,
            ghosted_ids,
            selected_ids,
            pose_only,
            scene_token,
        } = request
        {
            let scene = match derive_scene(
                &plan,
                &level_ids,
                ghosted_ids,
                &selected_ids,
                pose_only,
                scene_token,
            ) {
                Ok((scene, _)) => scene,
                Err(e) => return fail(&plan, status::REJECTED, e),
            };
            let mut value = serde_json::to_value(scene).expect("finite derived scene");
            let decodes = plan
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .scene_model_decodes;
            value["modelDecodes"] = serde_json::json!(decodes);
            return match serde_json::to_vec(&value) {
                Ok(bytes) => release(bytes, out),
                Err(e) => fail(&plan, status::REJECTED, e.to_string()),
            };
        }
        if let Request::Preview { ref digest, .. }
        | Request::Size { ref digest }
        | Request::Materials { ref digest } = request
        {
            // Copy a bounded blob while locked, then release the session for
            // interaction while decoding. The call lease still brackets work.
            let bytes = {
                let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
                let Some(bytes) = locked.file.as_ref().and_then(|f| f.assets.get(digest)) else {
                    drop(locked);
                    return fail(&plan, status::REJECTED, "Model bytes are unavailable");
                };
                if bytes.len() > 64 * 1024 * 1024 {
                    drop(locked);
                    return fail(
                        &plan,
                        status::REJECTED,
                        "Model preview input exceeds 64 MiB",
                    );
                }
                bytes.clone()
            };
            let result = if matches!(request, Request::Materials { .. }) {
                domowik_models::appearance::materials(&bytes)
                    .and_then(|p| serde_json::to_vec(&p).map_err(|e| e.to_string()))
            } else if matches!(request, Request::Size { .. }) {
                domowik_models::dimensions(&bytes)
                    .and_then(|p| serde_json::to_vec(&p).map_err(|e| e.to_string()))
            } else {
                let Request::Preview {
                    material_colors, ..
                } = &request
                else {
                    unreachable!()
                };
                domowik_models::appearance::preview(&bytes, material_colors)
                    .and_then(|p| serde_json::to_vec(&p).map_err(|e| e.to_string()))
            };
            return match result {
                Ok(bytes) => release(bytes, out),
                Err(e) => fail(&plan, status::REJECTED, e),
            };
        }
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        match run(&mut plan, request)
            .and_then(|v| serde_json::to_vec(&v).map_err(|e| e.to_string()))
        {
            Ok(bytes) => release(bytes, out),
            Err(e) => {
                plan.last_error = e;
                status::REJECTED
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn physical_glb(scale: f64) -> Vec<u8> {
        let bin: Vec<u8> = [[0f32, 0., 0.], [2., 0., 0.], [0., 1., 1.]]
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let root = json!({"asset":{"version":"2.0"}, "buffers":[{"byteLength":bin.len()}],
            "bufferViews":[{"buffer":0,"byteLength":bin.len()}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
            "nodes":[{"mesh":0,"scale":[scale,scale,scale]}],"scenes":[{"nodes":[0]}],"scene":0});
        let mut text = serde_json::to_vec(&root).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let mut bytes = b"glTF".to_vec();
        for value in [
            2u32,
            (28 + text.len() + bin.len()) as u32,
            text.len() as u32,
            0x4e4f534a,
        ] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(text);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942u32.to_le_bytes());
        bytes.extend(bin);
        bytes
    }

    #[test]
    fn automatic_sizing_resize_history_and_persistence_are_atomic() {
        let root = std::env::temp_dir().join(format!("domowik-size-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        let mut ids = Vec::new();
        for (name, bytes, footprint, height, source) in [
            (
                "physical.glb",
                physical_glb(1.),
                [2000., 1000.],
                1000.,
                "physical",
            ),
            (
                "proportions.obj",
                b"v 0 0 0\nv 4 2 0\nv 0 0 1\nf 1 2 3\n".to_vec(),
                [1000., 250.],
                500.,
                "proportions",
            ),
            (
                "unknown.zip",
                b"opaque content".to_vec(),
                [1000., 1000.],
                1000.,
                "fallback",
            ),
        ] {
            let file = root.join(name);
            std::fs::write(&file, bytes).unwrap();
            let result = request(
                h,
                json!({"kind":"place", "file":file,
                "levelId":level,"positionMm":[0,0]}),
            )
            .unwrap();
            assert_eq!(result["sizingSource"], source);
            assert_eq!(result["sizingDiagnostic"].is_string(), source == "fallback");
            let id = result["instanceId"].clone();
            let state = request(h, json!({"kind":"list"})).unwrap();
            let instance = state["instances"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .unwrap();
            assert_eq!(instance["dimensionsMm"], json!(footprint));
            assert_eq!(instance["heightMm"], height);
            ids.push(id);
        }
        let before = request(h, json!({"kind":"list"})).unwrap();
        let huge = root.join("huge.glb");
        std::fs::write(&huge, physical_glb(1_000_000.)).unwrap();
        assert_eq!(
            request(
                h,
                json!({"kind":"place", "file":huge,
            "levelId":level,"positionMm":[0,0]})
            ),
            Err(status::REJECTED)
        );
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), before);
        let edit = json!({"kind":"set", "instanceId":ids[0], "positionMm":[0,0],
            "dimensionsMm":[3000.,1200.], "heightMm":750., "rotationDeg":0});
        let mut invalid = edit.clone();
        invalid["heightMm"] = json!(-1);
        assert_eq!(request(h, invalid), Err(status::REJECTED));
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), before);
        let after = request(h, edit).unwrap();
        let (plan, lease) = resolve(h).unwrap();
        {
            let mut locked = plan.lock().unwrap();
            locked.session_mut().unwrap().undo().unwrap();
        }
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), before);
        plan.lock().unwrap().session_mut().unwrap().redo().unwrap();
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), after);
        let target = root.join("saved.domo");
        let path = CString::new(target.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        drop(lease);
        assert_eq!(domowik_plan_close(h), status::OK);
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        assert_eq!(request(reopened, json!({"kind":"list"})).unwrap(), after);
        let digest = after["instances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == ids[0])
            .unwrap()["digest"]
            .clone();
        let reuse = request(
            reopened,
            json!({"kind":"place", "digest":digest,
            "levelId":level, "positionMm":[5000,0]}),
        )
        .unwrap();
        assert_eq!(reuse["sizingSource"], "physical");
        assert_eq!(domowik_plan_close(reopened), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn request(handle: u64, value: Value) -> Result<Value, i32> {
        let bytes = serde_json::to_vec(&value).unwrap();
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code =
            unsafe { domowik_plan_assets(handle, bytes.as_ptr(), bytes.len() as u64, &mut out) };
        if code != status::OK {
            return Err(code);
        }
        let result = unsafe {
            serde_json::from_slice(std::slice::from_raw_parts(out.data, out.len as usize)).unwrap()
        };
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        Ok(result)
    }

    fn create() -> (u64, Uuid) {
        let mut handle = 0;
        assert_eq!(
            unsafe { domowik_plan_create_default(&mut handle) },
            status::OK
        );
        let (plan, _lease) = resolve(handle).unwrap();
        let level = plan.lock().unwrap().session().document().levels[0].id;
        (handle, level)
    }

    #[test]
    fn instance_colors_share_source_bytes_but_not_appearance_and_survive_history_and_save() {
        let root = std::env::temp_dir().join(format!("domowik-colors-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("original.obj");
        let bytes = b"v 0 1 0\nv 2 1 0\nv 0 1 2\nv 2 1 2\nusemtl cabinet\nf 1 2 3\nusemtl counter\nf 2 4 3\n";
        std::fs::write(&file, bytes).unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach","directory":root})).unwrap();
        let first = request(
            h,
            json!({"kind":"place","file":file,"levelId":level,"positionMm":[0,0]}),
        )
        .unwrap()["instanceId"]
            .clone();
        let initial = request(h, json!({"kind":"list"})).unwrap();
        let digest = initial["instances"][0]["digest"].clone();
        let second = request(
            h,
            json!({"kind":"place","digest":digest,"levelId":level,"positionMm":[2000,0]}),
        )
        .unwrap()["instanceId"]
            .clone();
        let base_scene = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        let original_preview = request(h, json!({"kind":"preview","digest":digest})).unwrap();
        let materials = request(h, json!({"kind":"materials","digest":digest})).unwrap();
        assert_eq!(materials[0]["key"], "obj:cabinet");
        let before = state(h)["document_revision"].clone();
        let edit = json!({"kind":"colors","instanceId":first,"materialColors":{"obj:cabinet":"#F2F0E8","obj:counter":"#6B5A48"}});
        request(h, edit.clone()).unwrap();
        let changed = request(h, json!({"kind":"list"})).unwrap();
        let changed_scene = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        assert_eq!(
            changed_scene["modelDecodes"], base_scene["modelDecodes"],
            "recolor must not decode geometry again"
        );
        let nodes = changed_scene["nodes"].as_array().unwrap();
        let first_node = nodes.iter().find(|n| n["id"] == first).unwrap();
        let second_node = nodes.iter().find(|n| n["id"] == second).unwrap();
        assert_ne!(first_node["mesh"], second_node["mesh"]);
        assert_eq!(second_node["mesh"], digest);
        let mesh = request(h, json!({"kind":"mesh","digest":first_node["mesh"]})).unwrap();
        assert_ne!(
            mesh["vertices"],
            request(h, json!({"kind":"mesh","digest":digest})).unwrap()["vertices"]
        );
        let colored_preview = request(
            h,
            json!({"kind":"preview","digest":digest,"materialColors":edit["materialColors"]}),
        )
        .unwrap();
        assert_ne!(colored_preview["png"], original_preview["png"]);
        assert_eq!(
            request(h, json!({"kind":"preview","digest":digest})).unwrap(),
            original_preview
        );
        let revision = state(h)["document_revision"].clone();
        assert_ne!(revision, before);
        request(h, edit).unwrap();
        assert_eq!(
            state(h)["document_revision"],
            revision,
            "unchanged Apply is not a command"
        );
        assert!(request(
            h,
            json!({"kind":"colors","instanceId":first,"materialColors":{"obj:cabinet":"invalid"}})
        )
        .is_err());
        assert_eq!(state(h)["document_revision"], revision);
        undo(h).unwrap();
        assert_eq!(
            request(h, json!({"kind":"scene","levelIds":[level]})).unwrap()["nodes"],
            base_scene["nodes"]
        );
        redo(h).unwrap();
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), changed);
        let target = root.join("saved.domo");
        let path = CString::new(target.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
        let saved = domowik_container::Plan::open(&target).unwrap();
        assert_eq!(saved.assets[digest.as_str().unwrap()], bytes);
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        assert_eq!(request(reopened, json!({"kind":"list"})).unwrap(), changed);
        assert_eq!(
            request(reopened, json!({"kind":"scene","levelIds":[level]})).unwrap()["nodes"],
            changed_scene["nodes"]
        );
        assert_eq!(domowik_plan_close(reopened), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trying_colours_keeps_only_variants_the_document_uses() {
        let model = b"v 0 0 0\nv 1 0 0\nv 1 1 1\nv 0 1 1\nusemtl body\nf 1 2 3\nf 1 3 4\n";
        let (h, level) = create();
        let digest = import(h, "chair.obj", model).unwrap()["digest"].clone();
        let id = request(
            h,
            json!({"kind":"place","digest":digest,"levelId":level,"positionMm":[0,0]}),
        )
        .unwrap()["instanceId"]
            .clone();
        let cached = || {
            let (plan, _lease) = resolve(h).unwrap();
            let plan = plan.lock().unwrap();
            (plan.scene_models.len(), plan.scene_color_variants.len())
        };
        let base = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        for i in 0..20u32 {
            let color = format!("#{:06X}", 0x102030 + i);
            request(
                h,
                json!({"kind":"colors","instanceId":id,"materialColors":{"obj:body":color}}),
            )
            .unwrap();
            request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
            assert_eq!(cached(), (2, 1), "model plus the one palette in use");
        }
        // Undo to an evicted palette recolours again without decoding.
        undo(h).unwrap();
        let scene = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        assert_eq!(scene["modelDecodes"], base["modelDecodes"]);
        assert_eq!(cached(), (2, 1));
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn library_thumbnails_read_files_or_embedded_bytes_without_import_or_mutation() {
        let root = std::env::temp_dir().join(format!("domowik-thumbnails-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("original.obj");
        let model = b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n";
        std::fs::write(&file, model).unwrap();
        let (h, _) = create();
        let before = state(h);
        let library = request(h, json!({"kind":"list"})).unwrap();
        let preview = request(
            h,
            json!({"kind":"thumbnail","file":file,"sourceDirectory":root}),
        )
        .unwrap();
        assert!(preview["coveredPixels"].as_u64().unwrap() > 0);
        assert_eq!(preview["triangles"], 1);
        assert_eq!(state(h), before);
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), library);
        assert_eq!(std::fs::read(&file).unwrap(), model);

        request(h, json!({"kind":"attach","directory":root})).unwrap();
        let attached_state = state(h);
        assert_eq!(
            request(h, json!({"kind":"thumbnail","file":file})).unwrap(),
            preview
        );
        assert_eq!(state(h), attached_state);
        assert!(request(h, json!({"kind":"list"})).unwrap()["models"]
            .as_array()
            .unwrap()
            .is_empty());

        let digest = import(h, "original.obj", model).unwrap()["digest"].clone();
        std::fs::remove_file(&file).unwrap();
        let before = state(h);
        let library = request(h, json!({"kind":"list"})).unwrap();
        let embedded = request(h, json!({"kind":"thumbnail","digest":digest})).unwrap();
        assert_eq!(embedded, preview);
        assert_eq!(state(h), before);
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), library);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn library_thumbnail_refusals_and_symlink_escape_do_not_mutate_the_plan() {
        let root =
            std::env::temp_dir().join(format!("domowik-thumbnail-refusal-{}", Uuid::now_v7()));
        let library = root.join("library");
        std::fs::create_dir_all(&library).unwrap();
        let outside = root.join("outside.obj");
        std::fs::write(&outside, b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let malformed = library.join("broken.obj");
        std::fs::write(&malformed, b"not a model").unwrap();
        let oversized = library.join("oversized.obj");
        std::fs::File::create(&oversized)
            .unwrap()
            .set_len(64 * 1024 * 1024 + 1)
            .unwrap();
        let (h, _) = create();
        let before = state(h);
        for req in [
            json!({"kind":"thumbnail"}),
            json!({"kind":"thumbnail","file":outside,"digest":"sha256:missing","sourceDirectory":library}),
            json!({"kind":"thumbnail","file":outside,"sourceDirectory":library}),
            json!({"kind":"thumbnail","file":outside}),
            json!({"kind":"thumbnail","file":malformed,"sourceDirectory":library}),
            json!({"kind":"thumbnail","file":oversized,"sourceDirectory":library}),
            json!({"kind":"thumbnail","digest":"sha256:missing"}),
        ] {
            assert_eq!(request(h, req), Err(status::REJECTED));
        }
        #[cfg(unix)]
        {
            let link = library.join("escape.obj");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert_eq!(
                request(
                    h,
                    json!({"kind":"thumbnail","file":link,"sourceDirectory":library})
                ),
                Err(status::REJECTED)
            );
        }
        assert_eq!(state(h), before);
        let (plan, _lease) = resolve(h).unwrap();
        assert!(
            plan.lock().unwrap().file.is_none(),
            "Browsing must not create package metadata"
        );
        drop(_lease);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn marquee_ids(handle: u64, level: Uuid, a: [f64; 2], b: [f64; 2]) -> Vec<Value> {
        let req =
            serde_json::to_vec(&json!({"levelId":level,"corner1Mm":a,"corner2Mm":b})).unwrap();
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(
            unsafe { domowik_plan_marquee(handle, req.as_ptr(), req.len() as u64, &mut out) },
            status::OK
        );
        let reply: Value = serde_json::from_slice(unsafe {
            std::slice::from_raw_parts(out.data, out.len as usize)
        })
        .unwrap();
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        reply["entityIds"].as_array().unwrap().clone()
    }

    /// Issue 0111: the marquee selects RESOLVED instances, ordered with walls
    /// under the R7.3.2 key, without touching the document or history.
    #[test]
    fn marquee_selects_resolved_instances_ordered_with_walls() {
        let root = std::env::temp_dir().join(format!("domowik-marquee-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("sofa.obj");
        std::fs::write(&file, b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        // Two 600x400 footprints centered at (0,0) and (3000,0), plus a wall
        // whose ideal rectangle spans x ∈ [0,4000], y ∈ [-100,100].
        let first = request(
            h,
            json!({"kind":"place","file":file,"sourceDirectory":root,
            "levelId":level,"positionMm":[0,0],"dimensionsMm":[600,400]}),
        )
        .unwrap()["instanceId"]
            .clone();
        let second = request(
            h,
            json!({"kind":"place","file":file,"sourceDirectory":root,
            "levelId":level,"positionMm":[3000,0],"dimensionsMm":[600,400]}),
        )
        .unwrap()["instanceId"]
            .clone();

        // A wall through both models: built by the shared core edit builder.
        {
            let (plan, _lease) = resolve(h).unwrap();
            let wall = domowik_format::Wall::new(
                level,
                domowik_format::Point::new(0.0, 0.0),
                domowik_format::Point::new(4000.0, 0.0),
                200.0,
                2500.0,
            );
            let operations = domowik_edit::add_wall_operations(
                plan.lock().unwrap().session().document(),
                level,
                wall,
            )
            .unwrap()
            .operations;
            plan.lock()
                .unwrap()
                .session_mut()
                .unwrap()
                .commit(operations)
                .unwrap();
        }

        let (plan, _lease) = resolve(h).unwrap();
        let before = plan.lock().unwrap().session().document().clone();
        let depth = plan.lock().unwrap().session().undo_depth();
        drop(_lease);

        // Over both models and the wall: all three, ordered by min corner.
        let marquee = |a, b| marquee_ids(h, level, a, b);
        let wall_id = before.walls[0].id.to_string();
        let first_id = first.as_str().unwrap().to_string();
        let second_id = second.as_str().unwrap().to_string();
        // R7.3.2's key interleaves classes by position, never appends: the
        // first footprint's min corner (-300, -200) precedes the wall's
        // (0, -100), which precedes the second footprint's (2700, -200).
        assert_eq!(
            marquee([-1000.0, -1000.0], [5000.0, 1000.0]),
            vec![
                serde_json::json!(first_id.clone()),
                serde_json::json!(wall_id.clone()),
                serde_json::json!(second_id)
            ]
        );
        // Clear of both footprints and the wall: nothing.
        assert!(marquee([-4000.0, -4000.0], [-3000.0, -3000.0]).is_empty());
        // Tangent to the first footprint's left face: boundary contact selects.
        assert_eq!(
            marquee([-1000.0, -100.0], [-300.0, 100.0]),
            vec![serde_json::json!(first_id)]
        );

        // Read-only: no document change, no history entry.
        let (plan, _lease) = resolve(h).unwrap();
        assert_eq!(plan.lock().unwrap().session().document(), &before);
        assert_eq!(plan.lock().unwrap().session().undo_depth(), depth);
        drop(_lease);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_import_deduplicates_and_survives_source_removal_and_reopen() {
        let root = std::env::temp_dir().join(format!("domowik-library-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("original chair.obj");
        std::fs::write(&file, b"v 0 0 0\nv 1 0 0\nv 0 0 1\nf 1 2 3\n").unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        let (plan, _lease) = resolve(h).unwrap();
        assert!(plan.lock().unwrap().metadata_dirty);
        assert!(
            plan.lock().unwrap().ensure_file().assets.is_empty(),
            "browsing embeds nothing"
        );
        let place = json!({"kind":"place", "file":file, "levelId":level, "positionMm":[100,200], "dimensionsMm":[600,400]});
        let id = request(h, place.clone()).unwrap()["instanceId"].clone();
        request(h, place).unwrap();
        assert_eq!(plan.lock().unwrap().ensure_file().assets.len(), 1);
        let list = request(h, json!({"kind":"list"})).unwrap();
        let digest = list["models"][0]["digest"].clone();
        let baseline = list["instances"].clone();
        request(h, json!({"kind":"set", "instanceId":id, "positionMm":[1000,2000], "dimensionsMm":[800,500], "rotationDeg":90})).unwrap();
        let edited = request(h, json!({"kind":"list"})).unwrap()["instances"].clone();
        assert_ne!(baseline, edited);
        let changed = edited
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap();
        let corner = changed["cornersMm"][0].as_array().unwrap();
        assert!((corner[0].as_f64().unwrap() - 1250.0).abs() < 1e-9);
        assert!((corner[1].as_f64().unwrap() - 1600.0).abs() < 1e-9);
        let hit = request(
            h,
            json!({"kind":"hit", "levelId":level, "pointMm":[1000,2000], "radiusMm":0}),
        )
        .unwrap();
        assert_eq!(hit[0]["entityId"], id);
        let miss = request(
            h,
            json!({"kind":"hit", "levelId":level, "pointMm":[1400,2000], "radiusMm":0}),
        )
        .unwrap();
        assert!(
            miss.as_array().unwrap().is_empty(),
            "rotation changes interaction extent"
        );
        {
            let mut p = plan.lock().unwrap();
            p.session_mut().unwrap().undo().unwrap();
        }
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap()["instances"],
            baseline
        );
        plan.lock().unwrap().session_mut().unwrap().redo().unwrap();
        request(h, json!({"kind":"delete", "instanceId":id})).unwrap();
        plan.lock().unwrap().session_mut().unwrap().undo().unwrap();
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap()["instances"],
            edited
        );
        let output = root.with_extension("domo");
        let path = CString::new(output.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert!(!plan.lock().unwrap().metadata_dirty);
        drop(_lease);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(&root).unwrap();
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        let persisted = request(reopened, json!({"kind":"list"})).unwrap();
        assert_eq!(persisted["instances"], edited);
        assert_eq!(persisted["directories"], list["directories"]);
        request(reopened, json!({"kind":"place", "digest":digest, "levelId":level, "positionMm":[0,0], "dimensionsMm":[600,400]})).unwrap();
        assert_eq!(
            request(reopened, json!({"kind":"list"})).unwrap()["instances"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(domowik_plan_close(reopened), status::OK);
        let (fresh, _) = create();
        assert_eq!(
            request(fresh, json!({"kind":"list"})).unwrap()["directories"],
            json!([])
        );
        assert_eq!(domowik_plan_close(fresh), status::OK);
        std::fs::remove_file(output).unwrap();
    }

    #[test]
    fn failed_placement_preserves_history_content_and_library_metadata() {
        let root = std::env::temp_dir().join(format!("domowik-bad-library-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("model.zip");
        std::fs::write(&file, b"original opaque archive bytes").unwrap();
        let (h, level) = create();
        assert_eq!(
            request(h, json!({"kind":"attach", "directory":root.join("absent")})),
            Err(status::REJECTED)
        );
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        let before = request(h, json!({"kind":"list"})).unwrap();
        let (plan, lease) = resolve(h).unwrap();
        let depth = plan.lock().unwrap().session().undo_depth();
        for (path, dims, id) in [
            (file.clone(), [0, 400], level),
            (root.join("absent.glb"), [600, 400], level),
            (file.clone(), [600, 400], Uuid::now_v7()),
        ] {
            assert_eq!(
                request(
                    h,
                    json!({"kind":"place", "file":path, "levelId":id, "positionMm":[0,0], "dimensionsMm":dims})
                ),
                Err(status::REJECTED)
            );
            assert_eq!(request(h, json!({"kind":"list"})).unwrap(), before);
            assert_eq!(plan.lock().unwrap().session().undo_depth(), depth);
            assert!(plan.lock().unwrap().ensure_file().assets.is_empty());
        }
        drop(lease);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_replacement_validates_deduplicates_and_preserves_embedded_models() {
        let root = std::env::temp_dir().join(format!("domowik-library-edit-{}", Uuid::now_v7()));
        let old = root.join("old");
        let new = root.join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir(&new).unwrap();
        let model = old.join("original.glb");
        std::fs::write(&model, b"original model for directory editing").unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":old})).unwrap();
        request(h, json!({"kind":"place", "file":model, "levelId":level, "positionMm":[0,0], "dimensionsMm":[600,400]})).unwrap();
        let before = request(h, json!({"kind":"list"})).unwrap();
        let old_path = before["directories"][0].clone();
        let output = root.join("plan.domo");
        let path = CString::new(output.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        let (plan, lease) = resolve(h).unwrap();
        for invalid in [root.join("missing"), model.clone()] {
            assert_eq!(
                request(
                    h,
                    json!({"kind":"replaceDirectory", "previousDirectory":old_path,"directory":invalid})
                ),
                Err(status::REJECTED)
            );
            assert_eq!(request(h, json!({"kind":"list"})).unwrap(), before);
            assert!(!plan.lock().unwrap().metadata_dirty);
        }
        request(
            h,
            json!({"kind":"replaceDirectory", "previousDirectory":old_path,"directory":old}),
        )
        .unwrap();
        assert!(
            !plan.lock().unwrap().metadata_dirty,
            "unchanged path is clean"
        );
        request(
            h,
            json!({"kind":"replaceDirectory", "previousDirectory":old_path,"directory":new}),
        )
        .unwrap();
        let changed = request(h, json!({"kind":"list"})).unwrap();
        assert_eq!(
            changed["directories"],
            json!([std::fs::canonicalize(&new).unwrap()])
        );
        assert_eq!(changed["instances"], before["instances"]);
        assert_eq!(changed["models"], before["models"]);
        assert_eq!(plan.lock().unwrap().ensure_file().assets.len(), 1);
        assert!(plan.lock().unwrap().metadata_dirty);
        // Duplicate destination collapses only the edited library row.
        request(h, json!({"kind":"attach", "directory":old})).unwrap();
        request(
            h,
            json!({"kind":"replaceDirectory", "previousDirectory":old_path,"directory":new}),
        )
        .unwrap();
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), changed);
        assert_eq!(
            request(
                h,
                json!({"kind":"replaceDirectory", "previousDirectory":old_path,"directory":old})
            ),
            Err(status::REJECTED),
            "a stale edit cannot reattach the old row"
        );
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        drop(lease);
        assert_eq!(domowik_plan_close(h), status::OK);
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        assert_eq!(request(reopened, json!({"kind":"list"})).unwrap(), changed);
        assert_eq!(domowik_plan_close(reopened), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn global_library_placement_and_preview_survive_reopen_without_source() {
        let root = std::env::temp_dir().join(format!("domowik-preview-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("original.obj");
        std::fs::write(&file, b"v 0 1 0\nv 2 1 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let (h, level) = create();
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap()["directories"],
            json!([])
        );
        request(h,json!({"kind":"place","file":file,"sourceDirectory":root,"levelId":level,"positionMm":[0,0],"dimensionsMm":[600,400]})).unwrap();
        let before = request(h, json!({"kind":"list"})).unwrap();
        let digest = before["models"][0]["digest"].clone();
        let preview = request(h, json!({"kind":"preview","digest":digest})).unwrap();
        assert_eq!(preview["triangles"], 1);
        assert!(preview["coveredPixels"].as_u64().unwrap() > 10000);
        assert_eq!(preview["png"][0], 137);
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap(),
            before,
            "preview creates no document or metadata mutation"
        );
        let output = root.with_extension("domo");
        let path = CString::new(output.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(&root).unwrap();
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        assert_eq!(
            request(reopened, json!({"kind":"preview","digest":digest})).unwrap(),
            preview
        );
        assert_eq!(domowik_plan_close(reopened), status::OK);
        std::fs::remove_file(output).unwrap();
    }

    #[test]
    fn repeated_3d_scene_reads_share_one_decoded_model_and_preserve_the_plan() {
        let root = std::env::temp_dir().join(format!("domowik-3d-model-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("original.obj");
        std::fs::write(&file, b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let (h, level) = create();
        for x in [0, 1000, 2000] {
            request(h,json!({"kind":"place","file":file,"sourceDirectory":root,"levelId":level,"positionMm":[x,0],"dimensionsMm":[600,400]})).unwrap();
        }
        let before = request(h, json!({"kind":"list"})).unwrap();
        let digest = before["models"][0]["digest"].clone();
        let scene = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        assert_eq!(scene["modelDecodes"], 1);
        assert_eq!(scene["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(scene["requiredModels"], json!([digest]));
        let mesh = request(h, json!({"kind":"mesh","digest":digest})).unwrap();
        assert!(mesh["triangles"].as_u64().unwrap() <= 20000);
        assert_eq!(
            request(h, json!({"kind":"scene","levelIds":[level]})).unwrap()["modelDecodes"],
            1
        );
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), before);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pose_preview_is_shared_by_views_but_never_enters_document_or_undo() {
        let (h, level) = create();
        let model = import(h, "tv.obj", b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let placed = request(
            h,
            json!({"kind":"place", "digest":model["digest"],
            "levelId":level,"positionMm":[0,0]}),
        )
        .unwrap();
        let id = placed["instanceId"].clone();
        request(
            h,
            json!({"kind":"set","instanceId":id,"positionMm":[0,0],
            "dimensionsMm":[1200,700],"heightMm":50,"rotationDeg":0}),
        )
        .unwrap();
        let (plan, _lease) = resolve(h).unwrap();
        let before = plan.lock().unwrap().session().document().clone();
        let depth = plan.lock().unwrap().session().undo_depth();
        let original_scene = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        let pose = json!({"kind":"pose","instanceId":id,"positionMm":[200,300,1500],
            "rotationDeg":[100,10,30],"preview":true});
        let preview = request(h, pose.clone()).unwrap();
        assert_eq!(preview["instances"][0]["pose3d"], json!([1500., 100., 10.]));
        let preview_scene = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        assert_ne!(
            preview_scene["nodes"][0]["matrix"],
            original_scene["nodes"][0]["matrix"]
        );
        assert_eq!(
            preview_scene["modelDecodes"], 1,
            "pose does not re-decode models"
        );
        assert_eq!(plan.lock().unwrap().session().document(), &before);
        assert_eq!(plan.lock().unwrap().session().undo_depth(), depth);
        request(h, json!({"kind":"clearPosePreview"})).unwrap();
        assert_eq!(
            request(h, json!({"kind":"scene","levelIds":[level]})).unwrap()["nodes"],
            original_scene["nodes"]
        );
        let mut commit = pose;
        commit["preview"] = json!(false);
        request(h, commit).unwrap();
        assert_eq!(plan.lock().unwrap().session().undo_depth(), depth + 1);
        assert_eq!(
            request(h, json!({"kind":"scene","levelIds":[level]})).unwrap()["nodes"],
            preview_scene["nodes"]
        );
        plan.lock().unwrap().session_mut().unwrap().undo().unwrap();
        assert_eq!(plan.lock().unwrap().session().document(), &before);
        plan.lock().unwrap().session_mut().unwrap().redo().unwrap();
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap()["instances"],
            preview["instances"]
        );
        drop(_lease);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn translate_preview_moves_several_models_without_rebuilding_or_committing() {
        let (h, level) = create();
        let model = import(h, "tv.obj", b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let mut ids = Vec::new();
        for x in [0, 2000] {
            let placed = request(
                h,
                json!({"kind":"place","digest":model["digest"],
                "levelId":level,"positionMm":[x,0]}),
            )
            .unwrap();
            let id = placed["instanceId"].clone();
            request(
                h,
                json!({"kind":"set","instanceId":id,"positionMm":[x,0],
                "dimensionsMm":[1200,700],"heightMm":50,"rotationDeg":0}),
            )
            .unwrap();
            ids.push(id);
        }
        let full = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        let token = full["sceneToken"].clone();
        let (plan, lease) = resolve(h).unwrap();
        let before = plan.lock().unwrap().session().document().clone();
        let depth = plan.lock().unwrap().session().undo_depth();
        let builds = plan.lock().unwrap().scene_builds;
        let warm = json!({"kind":"scene","levelIds":[level],"poseOnly":true,"sceneToken":token});

        request(
            h,
            json!({"kind":"translatePreview","instanceIds":ids,"deltaMm":[500,-250]}),
        )
        .unwrap();
        let fast = request(h, warm.clone()).unwrap();
        assert_eq!(fast["instanceOnly"], true);
        assert_eq!(plan.lock().unwrap().scene_builds, builds);
        for i in 0..2 {
            assert_ne!(fast["nodes"][i]["matrix"], full["nodes"][i]["matrix"]);
        }
        assert_eq!(plan.lock().unwrap().session().document(), &before);
        assert_eq!(plan.lock().unwrap().session().undo_depth(), depth);
        // A cold build of the overlay agrees with the warm transform path once
        // each scene's own origin is added back (cold builds recentre).
        let cold = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        let plan_xy = |scene: &Value, i: usize| {
            let m = &scene["nodes"][i]["matrix"];
            let o = &scene["originMm"];
            [
                o[0].as_f64().unwrap() + m[12].as_f64().unwrap() * 1000.,
                o[1].as_f64().unwrap() - m[14].as_f64().unwrap() * 1000.,
            ]
        };
        for i in 0..2 {
            let ([fx, fy], [cx, cy]) = (plan_xy(&fast, i), plan_xy(&cold, i));
            assert!((fx - cx).abs() < 0.01 && (fy - cy).abs() < 0.01);
        }

        for bad in [
            json!({"kind":"translatePreview","instanceIds":["00000000-0000-4000-8000-000000000001"],"deltaMm":[1,1]}),
            json!({"kind":"translatePreview","instanceIds":ids,"deltaMm":[1e300,0]}),
        ] {
            assert!(request(h, bad).is_err());
        }

        request(h, json!({"kind":"clearPosePreview"})).unwrap();
        assert_eq!(
            request(h, json!({"kind":"scene","levelIds":[level]})).unwrap()["nodes"],
            full["nodes"]
        );

        // Committing the same offset shows exactly what the preview showed.
        for (id, x) in ids.iter().zip([500, 2500]) {
            request(
                h,
                json!({"kind":"set","instanceId":id,"positionMm":[x,-250],
                "dimensionsMm":[1200,700],"heightMm":50,"rotationDeg":0}),
            )
            .unwrap();
        }
        assert_eq!(
            request(h, json!({"kind":"scene","levelIds":[level]})).unwrap()["nodes"],
            cold["nodes"]
        );
        drop(lease);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn picking_highlight_and_bounds_query_the_presented_scene_without_rebuilding() {
        let (h, level) = create();
        let wall = Uuid::now_v7();
        execute(
            h,
            json!({"operations":[{"kind":"createEntity","operationId":Uuid::now_v7(),
            "entityId":wall,"entity":{"type":"wall","id":wall,"levelId":level,
            "startMm":[-3000,3000],"endMm":[3000,3000],"thicknessMm":200,"heightMm":2500}}]}),
        )
        .unwrap();
        let model = import(h, "tv.obj", b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let asset = request(
            h,
            json!({"kind":"place","digest":model["digest"],"levelId":level,"positionMm":[0,0]}),
        )
        .unwrap()["instanceId"]
            .clone();
        request(
            h,
            json!({"kind":"set","instanceId":asset,"positionMm":[0,0],
            "dimensionsMm":[1200,700],"heightMm":800,"rotationDeg":0}),
        )
        .unwrap();
        let full = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        // Origin is the content centre: the wall lies at z=-1.5 m, the model at z=+1.5 m.
        assert_eq!(full["originMm"], json!([0.0, 1500.0]));
        let token = full["sceneToken"].as_u64().unwrap();
        let (plan, lease) = resolve(h).unwrap();
        let builds = plan.lock().unwrap().scene_builds;
        // The pose version of the last presented frame, as a presenter tracks it.
        let drawn = std::cell::Cell::new(full["poseVersion"].as_u64().unwrap());
        let pick = |origin: [f64; 3], direction: [f64; 3], ghosted: Value, token: u64| {
            request(
                h,
                json!({"kind":"pick3d","levelIds":[level],"ghostedIds":ghosted,
                "sceneToken":token,"poseVersion":drawn.get(),
                "originM":origin,"direction":direction}),
            )
            .unwrap()
        };
        let present = |token: u64| {
            let warm = request(
                h,
                json!({"kind":"scene","levelIds":[level],"poseOnly":true,"sceneToken":token}),
            )
            .unwrap();
            drawn.set(warm["poseVersion"].as_u64().unwrap());
        };

        let down = pick([0., 10., 1.5], [0., -1., 0.], json!([]), token);
        assert_eq!(down["hit"]["kind"], "asset");
        assert_eq!(down["hit"]["entityId"], asset);
        assert_eq!(down["hit"]["levelId"], json!(level));
        // A ray over the low model reaches the wall's near face.
        let across = pick([0., 1.5, 5.], [0., 0., -1.], json!([]), token);
        assert_eq!(across["hit"]["kind"], "wall");
        assert_eq!(across["hit"]["entityId"], json!(wall));
        assert!((across["hit"]["distanceM"].as_f64().unwrap() - 6.4).abs() < 1e-4);
        assert_eq!(
            pick([20., 1., 5.], [0., 0., -1.], json!([]), token)["hit"],
            Value::Null
        );
        assert_eq!(
            pick([0., 10., 1.5], [0., -1., 0.], json!([]), token + 7),
            json!({"stale": true})
        );

        let bounds = request(
            h,
            json!({"kind":"bounds3d","levelIds":[level],"sceneToken":token,"poseVersion":drawn.get(),"entityIds":[wall]}),
        )
        .unwrap();
        let close = |v: &Value, expected: [f64; 3]| {
            for (k, e) in expected.into_iter().enumerate() {
                assert!(
                    (v[k].as_f64().unwrap() - e).abs() < 1e-4,
                    "{v} vs {expected:?}"
                );
            }
        };
        close(&bounds["bounds"]["lowM"], [-3., 0., -1.6]);
        close(&bounds["bounds"]["highM"], [3., 2.5, -1.4]);

        // Picks and bounds follow the presented preview pose, not the
        // committed one, while the baseline token stays current.
        request(
            h,
            json!({"kind":"translatePreview","instanceIds":[asset],"deltaMm":[2000,0]}),
        )
        .unwrap();
        assert_eq!(
            pick([2., 10., 1.5], [0., -1., 0.], json!([]), token),
            json!({"stale": true}),
            "an overlay the presenter has not drawn yet is not queried"
        );
        present(token);
        assert_eq!(
            pick([0., 10., 1.5], [0., -1., 0.], json!([]), token)["hit"],
            Value::Null
        );
        assert_eq!(
            pick([2., 10., 1.5], [0., -1., 0.], json!([]), token)["hit"]["entityId"],
            asset
        );
        let moved = request(
            h,
            json!({"kind":"bounds3d","levelIds":[level],"sceneToken":token,"poseVersion":drawn.get(),"entityIds":[asset]}),
        )
        .unwrap();
        close(&moved["bounds"]["lowM"], [1.4, 0., 1.15]);
        close(&moved["bounds"]["highM"], [2.6, 0.8, 1.85]);
        request(h, json!({"kind":"clearPosePreview"})).unwrap();
        present(token);
        assert_eq!(
            pick([0., 10., 1.5], [0., -1., 0.], json!([]), token)["hit"]["entityId"],
            asset
        );
        // Picks honour the camera's visible range along the ray.
        let clipped = request(
            h,
            json!({"kind":"pick3d","levelIds":[level],"sceneToken":token,
            "poseVersion":drawn.get(),"rangeM":[0.,5.],
            "originM":[0.,1.5,5.],"direction":[0.,0.,-1.]}),
        )
        .unwrap();
        assert_eq!(
            clipped["hit"],
            Value::Null,
            "the wall at 6.4 m is beyond far"
        );

        // Selection tints through the warm path and never rebuilds or sticks.
        let warm = |selected: Value| {
            request(
                h,
                json!({"kind":"scene","levelIds":[level],"poseOnly":true,
                "sceneToken":token,"selectedIds":selected}),
            )
            .unwrap()
        };
        let color_of = |scene: &Value, id: &str| {
            scene["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"].as_str().unwrap().contains(id))
                .unwrap()["color"]
                .clone()
        };
        let tinted = warm(json!([asset]));
        assert_eq!(tinted["instanceOnly"], true);
        let asset_text = asset.as_str().unwrap();
        let wall_text = wall.to_string();
        assert_ne!(color_of(&tinted, asset_text), color_of(&full, asset_text));
        assert_eq!(color_of(&tinted, &wall_text), color_of(&full, &wall_text));
        assert_eq!(warm(json!([]))["nodes"], full["nodes"]);
        assert_eq!(
            warm(json!(["not-an-entity"]))["nodes"],
            full["nodes"],
            "ids that are not entities never refuse the scene"
        );
        assert_eq!(plan.lock().unwrap().scene_builds, builds);
        let cold = request(
            h,
            json!({"kind":"scene","levelIds":[level],"selectedIds":[wall]}),
        )
        .unwrap();
        assert_ne!(color_of(&cold, &wall_text), color_of(&full, &wall_text));
        let cold_token = cold["sceneToken"].as_u64().unwrap();
        assert_eq!(
            pick([0., 1.5, 5.], [0., 0., -1.], json!([]), cold_token)["hit"]["entityId"],
            json!(wall),
            "the cached baseline stays untinted and pickable"
        );

        // Ghosted levels are transparent to picking.
        let ghosted = request(
            h,
            json!({"kind":"scene","levelIds":[level],"ghostedIds":[level]}),
        )
        .unwrap();
        let ghost_token = ghosted["sceneToken"].as_u64().unwrap();
        assert_eq!(
            pick([0., 10., 1.5], [0., -1., 0.], json!([level]), ghost_token)["hit"],
            Value::Null
        );
        drop(lease);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn warm_pose_updates_send_no_vertices_and_do_not_rebuild_scene_geometry() {
        let (h, level) = create();
        let imported = import(h, "tv.obj", b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let placed = request(
            h,
            json!({"kind":"place","digest":imported["digest"],
            "levelId":level,"positionMm":[0,0]}),
        )
        .unwrap();
        let id = placed["instanceId"].clone();
        let full = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        let token = full["sceneToken"].clone();
        let (plan, lease) = resolve(h).unwrap();
        let before = plan.lock().unwrap().session().document().clone();
        let depth = plan.lock().unwrap().session().undo_depth();
        let full_builds = plan.lock().unwrap().scene_builds;
        for angle in [10., 30., 70., 100.] {
            request(
                h,
                json!({"kind":"pose","instanceId":id,"positionMm":[0,0,1500],
                "rotationDeg":[angle,10,30],"preview":true}),
            )
            .unwrap();
            let fast = request(
                h,
                json!({"kind":"scene","levelIds":[level],
                "poseOnly":true,"sceneToken":token}),
            )
            .unwrap();
            assert_eq!(fast["instanceOnly"], true);
            assert!(fast["meshes"]
                .as_object()
                .unwrap()
                .values()
                .all(|v| v.as_array().unwrap().is_empty()));
            assert!(fast["requiredModels"].as_array().unwrap().is_empty());
            assert_ne!(fast["nodes"], full["nodes"]);
            assert_eq!(plan.lock().unwrap().scene_builds, full_builds);
        }
        let frame = scene_frame_of(
            h,
            json!({"levelIds":[level],"poseOnly":true,"sceneToken":token}),
        )
        .unwrap();
        assert_eq!(frame.header.extra["instanceOnly"], true);
        assert_eq!(frame.instances.len(), 1);
        assert!(
            frame.meshes.iter().all(|(_, vertices)| vertices.is_empty()),
            "binary transport contains names, never model vertices"
        );
        assert_eq!(plan.lock().unwrap().session().document(), &before);
        assert_eq!(plan.lock().unwrap().session().undo_depth(), depth);
        request(h, json!({"kind":"clearPosePreview"})).unwrap();
        let cancelled = request(
            h,
            json!({"kind":"scene","levelIds":[level],
            "poseOnly":true,"sceneToken":token}),
        )
        .unwrap();
        assert_eq!(cancelled["nodes"], full["nodes"]);
        // Stale renderer baseline tokens and visibility changes force a full
        // frame, so resident buffers never get used with another origin/layout.
        let fallback = request(
            h,
            json!({"kind":"scene","levelIds":[level],
            "poseOnly":true,"sceneToken":0}),
        )
        .unwrap();
        assert_eq!(fallback["instanceOnly"], false);
        assert_ne!(fallback["sceneToken"], token);
        assert_eq!(plan.lock().unwrap().scene_builds, full_builds + 1);
        let ghosted = request(
            h,
            json!({"kind":"scene","levelIds":[level],"ghostedIds":[level],
            "poseOnly":true,"sceneToken":fallback["sceneToken"]}),
        )
        .unwrap();
        assert_eq!(ghosted["instanceOnly"], false);
        // Commit changes the document revision: a previously valid token must
        // no longer authorize instance-only data.
        request(
            h,
            json!({"kind":"pose","instanceId":id,"positionMm":[200,300,1500],
            "rotationDeg":[100,10,30]}),
        )
        .unwrap();
        let committed = scene_frame_of(
            h,
            json!({"levelIds":[level],"ghostedIds":[level],
            "poseOnly":true,"sceneToken":ghosted["sceneToken"]}),
        )
        .unwrap();
        assert_eq!(committed.header.extra["instanceOnly"], false);
        assert!(committed
            .meshes
            .iter()
            .any(|(_, vertices)| !vertices.is_empty()));
        drop(lease);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    fn import(handle: u64, name: &str, bytes: &[u8]) -> Result<Value, i32> {
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code = unsafe {
            domowik_plan_import_model(
                handle,
                name.as_ptr(),
                name.len() as u64,
                bytes.as_ptr(),
                bytes.len() as u64,
                &mut out,
            )
        };
        if code != status::OK {
            return Err(code);
        }
        let result = unsafe {
            serde_json::from_slice(std::slice::from_raw_parts(out.data, out.len as usize)).unwrap()
        };
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        Ok(result)
    }

    fn scene_frame_of(handle: u64, request: Value) -> Result<domowik_scene_wire::Frame, i32> {
        let bytes = serde_json::to_vec(&request).unwrap();
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code = unsafe {
            domowik_plan_scene_frame(handle, bytes.as_ptr(), bytes.len() as u64, &mut out)
        };
        if code != status::OK {
            return Err(code);
        }
        let frame = domowik_scene_wire::decode(unsafe {
            std::slice::from_raw_parts(out.data, out.len as usize)
        })
        .unwrap();
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        Ok(frame)
    }

    #[test]
    fn an_uploaded_model_joins_the_library_and_places_by_digest() {
        let model = b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n";
        let (h, level) = create();
        let state = |h| {
            let (plan, _lease) = resolve(h).unwrap();
            let plan = plan.lock().unwrap();
            (
                plan.session().is_dirty() || plan.metadata_dirty,
                plan.session().undo_depth(),
            )
        };
        let imported = import(h, "Chair.OBJ", model).unwrap();
        assert_eq!(imported["name"], "Chair");
        assert_eq!(imported["digest"], domowik_container::digest_of(model));
        assert_eq!(state(h), (true, 0), "an embed is unsaved but not undoable");
        let library = request(h, json!({"kind":"list"})).unwrap();
        assert_eq!(library["models"].as_array().unwrap().len(), 1);
        assert_eq!(library["models"][0]["source"], "uploaded file Chair.OBJ");
        // Identical bytes under another name keep the first descriptor.
        assert_eq!(import(h, "copy.obj", model).unwrap()["name"], "Chair");
        assert_eq!(request(h, json!({"kind":"list"})).unwrap(), library);
        let placed = request(
            h,
            json!({"kind":"place","digest":imported["digest"],
            "levelId":level,"positionMm":[0,0]}),
        )
        .unwrap();
        assert_eq!(placed["sizingSource"], "proportions");
        assert_eq!(state(h).1, 1);
        // Refusals: no extension, an unsupported one, no bytes.
        for (name, bytes) in [
            ("chair", &model[..]),
            ("chair.fbx", &model[..]),
            ("chair.obj", &b""[..]),
        ] {
            assert_eq!(import(h, name, bytes), Err(status::REJECTED), "{name}");
        }
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap()["models"],
            library["models"]
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn usd_models_import_place_and_mesh_with_physical_units() {
        let model = br#"#usda 1.0
(
metersPerUnit = 1
)
def Mesh "Lamp" {
    int[] faceVertexCounts = [3, 3]
    int[] faceVertexIndices = [0, 1, 2, 0, 2, 3]
    point3f[] points = [(0, 0, 0), (0.5, 0, 0), (0.5, 2, 0.25), (0, 2, 0.25)]
}
"#;
        let (h, level) = create();
        let imported = import(h, "Lamp.usda", model).unwrap();
        assert_eq!(imported["name"], "Lamp");
        let placed = request(
            h,
            json!({"kind":"place","digest":imported["digest"],"levelId":level,"positionMm":[0,0]}),
        )
        .unwrap();
        let listed = request(h, json!({"kind":"list"})).unwrap();
        let instance = &listed["instances"][0];
        assert_eq!(instance["dimensionsMm"], json!([500.0, 250.0]), "{placed}");
        let digest = imported["digest"].as_str().unwrap();
        let frame = scene_frame_of(h, json!({"levelIds":[level]})).unwrap();
        assert!(
            frame
                .meshes
                .iter()
                .any(|(k, v)| k == digest && !v.is_empty()),
            "USD passes the media gate"
        );
        let mesh = request(h, json!({"kind":"mesh","digest":digest})).unwrap();
        assert_eq!(mesh["sourceTriangles"], 2);
        assert!(import(h, "Lamp.fbx", model).is_err());
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn embedded_textures_cross_json_and_scene_frames_and_warm_updates_reuse_them() {
        let (j, bin) = crate::texture_fixture::fixture(image::ImageFormat::WebP, true);
        let bytes = crate::texture_fixture::pack(&j, &bin);
        let (h, level) = create();
        let imported = import(h, "quadrants.glb", &bytes).unwrap();
        let placed = request(
            h,
            json!({"kind":"place","digest":imported["digest"],"levelId":level,"positionMm":[0,0]}),
        )
        .unwrap();
        let before = state(h);
        let frame = scene_frame_of(h, json!({"levelIds":[level]})).unwrap();
        let digest = imported["digest"].as_str().unwrap();
        let texture = &frame.textures[digest];
        assert_eq!((texture.width, texture.height, texture.layers), (8, 8, 1));
        assert_eq!(texture.rgba.len(), 256);
        let mesh = request(h, json!({"kind":"mesh","digest":digest})).unwrap();
        assert_eq!(mesh["texture"]["rgba"].as_array().unwrap().len(), 256);
        assert_eq!(
            mesh["texture"]["coordinates"].as_array().unwrap().len(),
            texture.coordinates.len()
        );
        request(h, json!({"kind":"pose","instanceId":placed["instanceId"],"positionMm":[10,20,0],"rotationDeg":[0,0,30],"preview":true})).unwrap();
        let warm = scene_frame_of(h, json!({"levelIds":[level],"poseOnly":true,"sceneToken":frame.header.extra["sceneToken"]})).unwrap();
        assert_eq!(warm.header.extra["instanceOnly"], true);
        assert!(warm.textures.is_empty());
        assert_eq!(
            warm.header.extra["modelDecodes"],
            frame.header.extra["modelDecodes"]
        );
        assert_eq!(state(h)["document_revision"], before["document_revision"]);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn an_import_path_is_retained_as_asset_source_provenance() {
        let model = b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n";
        let (h, _) = create();
        let path = "/assets/furniture/living room/chair.obj";
        assert_eq!(import(h, path, model).unwrap()["name"], "chair");
        let library = request(h, json!({"kind":"list"})).unwrap();
        assert_eq!(library["models"][0]["source"], path);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn an_import_path_too_long_for_provenance_is_refused_without_mutation() {
        let model = b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n";
        let (h, _) = create();
        let before = state(h);
        let path = format!("{}/chair.obj", "a".repeat(MAX_PROVENANCE_TEXT_BYTES));
        assert_eq!(import(h, &path, model), Err(status::REJECTED));
        assert!(request(h, json!({"kind":"list"})).unwrap()["models"]
            .as_array()
            .unwrap()
            .is_empty());
        let after = state(h);
        assert_eq!(after["document_revision"], before["document_revision"]);
        assert_eq!(after["asset_revision"], before["asset_revision"]);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    fn state(handle: u64) -> Value {
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(unsafe { domowik_plan_state(handle, &mut out) }, status::OK);
        let value = serde_json::from_slice(unsafe {
            std::slice::from_raw_parts(out.data, out.len as usize)
        })
        .unwrap();
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        value
    }

    fn hit_classes(handle: u64, request: &Value) -> Vec<String> {
        let bytes = serde_json::to_vec(request).unwrap();
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code = unsafe {
            crate::domowik_plan_hit_test(handle, bytes.as_ptr(), bytes.len() as u64, &mut out)
        };
        assert_eq!(code, status::OK);
        let hits: Vec<Value> = serde_json::from_slice(unsafe {
            std::slice::from_raw_parts(out.data, out.len as usize)
        })
        .unwrap();
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        hits.iter()
            .map(|h| h["class"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn uploading_a_missing_model_refreshes_every_cache_keyed_on_it() {
        // A portable plan with one 4 m model, then the same plan without the
        // model's bytes: a valid thin package whose instance is a placeholder.
        let model = b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n";
        let (h, level) = create();
        let digest = import(h, "chair.obj", model).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        request(
            h,
            json!({"kind":"place","digest":digest,"levelId":level,
            "positionMm":[2000,0],"dimensionsMm":[4000,4000]}),
        )
        .unwrap();
        let bytes = {
            let (plan, _lease) = resolve(h).unwrap();
            let mut plan = plan.lock().unwrap();
            plan.ensure_file().encode().unwrap()
        };
        assert_eq!(domowik_plan_close(h), status::OK);
        let mut package =
            domowik_container::read_package_bytes("plan", &bytes, &Default::default()).unwrap();
        assert!(package.assets.remove(&digest).is_some());
        let thin = domowik_container::encode_package(&package).unwrap();
        let mut h = 0u64;
        assert_eq!(
            unsafe { crate::domowik_plan_open_bytes(thin.as_ptr(), thin.len() as u64, &mut h) },
            status::OK
        );

        // Warm the hit index, footprint index and resolutions on the
        // placeholder, as an editor that has drawn the plan has.
        let point = json!({"levelId":level,"pointMm":[2000,0],"radiusMm":0});
        let footprint = json!({"kind":"hit","levelId":level,"pointMm":[2000,0],"radiusMm":0});
        assert_eq!(hit_classes(h, &point), ["placeholder-box"]);
        assert_eq!(request(h, footprint.clone()).unwrap(), json!([]));
        let selected_missing = marquee_ids(h, level, [-1000., -3000.], [5000., 3000.]);
        assert_eq!(
            selected_missing.len(),
            1,
            "degraded instance appears only as a placeholder"
        );
        let before = state(h);

        import(h, "chair.obj", model).unwrap();
        let after = state(h);
        assert_eq!(after["document_revision"], before["document_revision"]);
        assert_eq!(
            after["asset_revision"].as_u64().unwrap(),
            before["asset_revision"].as_u64().unwrap() + 1,
            "restored bytes are a content change the editor must see"
        );
        assert!(hit_classes(h, &point).is_empty(), "the placeholder is gone");
        let hits = request(h, footprint.clone()).unwrap();
        assert_eq!(hits[0]["class"], "asset-footprint");
        assert_eq!(
            marquee_ids(h, level, [-1000., -3000.], [5000., 3000.]),
            selected_missing,
            "restoring bytes replaces placeholder candidacy; it never duplicates the instance"
        );

        // Bytes the plan already holds change nothing.
        import(h, "again.obj", model).unwrap();
        assert_eq!(state(h)["asset_revision"], after["asset_revision"]);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn the_scene_frame_carries_the_derived_scene_and_decoded_models() {
        let model = b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n";
        let (h, level) = create();
        let digest = import(h, "table.obj", model).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        for x in [0, 1500] {
            request(
                h,
                json!({"kind":"place","digest":digest,"levelId":level,"positionMm":[x,0]}),
            )
            .unwrap();
        }
        let json_scene = request(h, json!({"kind":"scene","levelIds":[level]})).unwrap();
        let frame = scene_frame_of(h, json!({"levelIds":[level]})).unwrap();
        assert_eq!(
            frame.instances.len(),
            json_scene["nodes"].as_array().unwrap().len()
        );
        assert_eq!(
            frame.header.radius as f64,
            json_scene["radius"].as_f64().unwrap()
        );
        assert_eq!(frame.header.extra["modelDecodes"], 1);
        let keys: Vec<&str> = frame.meshes.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&digest.as_str()));
        let model_index = keys.iter().position(|k| *k == digest).unwrap() as u32;
        assert_eq!(
            frame
                .instances
                .iter()
                .filter(|i| i.mesh == model_index)
                .count(),
            2
        );
        assert_eq!(
            scene_frame_of(h, json!({"levelIds":[level],"bogus":1})).unwrap_err(),
            status::MALFORMED_REQUEST
        );
        assert_eq!(
            scene_frame_of(0, json!({"levelIds":[]})).unwrap_err(),
            status::UNKNOWN_HANDLE
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn read_is_bounded_and_rejects_empty_models() {
        let path = std::env::temp_dir().join(format!("domowik-model-{}.glb", Uuid::now_v7()));
        std::fs::write(&path, b"original inert model bytes").unwrap();
        assert!(read_model(&path, 4).is_err());
        assert_eq!(
            read_model(&path, 100).unwrap(),
            b"original inert model bytes"
        );
        std::fs::write(&path, []).unwrap();
        assert!(read_model(&path, 100).is_err());
        std::fs::remove_file(path).unwrap();
    }

    /// Raw operations through `domowik_plan_execute`, for state the asset
    /// requests do not create themselves (a second level, a dangling digest).
    fn execute(handle: u64, value: Value) -> Result<Value, i32> {
        let bytes = serde_json::to_vec(&value).unwrap();
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code =
            unsafe { domowik_plan_execute(handle, bytes.as_ptr(), bytes.len() as u64, &mut out) };
        if code != status::OK {
            return Err(code);
        }
        let result = unsafe {
            serde_json::from_slice(std::slice::from_raw_parts(out.data, out.len as usize)).unwrap()
        };
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        Ok(result)
    }

    fn undo(handle: u64) -> Result<Value, i32> {
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code = unsafe { domowik_plan_undo(handle, &mut out) };
        if code != status::OK {
            return Err(code);
        }
        let result = unsafe {
            serde_json::from_slice(std::slice::from_raw_parts(out.data, out.len as usize)).unwrap()
        };
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        Ok(result)
    }

    fn redo(handle: u64) -> Result<Value, i32> {
        let mut out = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code = unsafe { domowik_plan_redo(handle, &mut out) };
        if code != status::OK {
            return Err(code);
        }
        let result = unsafe {
            serde_json::from_slice(std::slice::from_raw_parts(out.data, out.len as usize)).unwrap()
        };
        assert_eq!(domowik_buffer_free(out.token), status::OK);
        Ok(result)
    }

    fn add_level(handle: u64, level: Uuid) {
        execute(
            handle,
            json!({"operations":[{
                "operationId": Uuid::now_v7(), "entityId": level,
                "kind": "createEntity",
                "entity": {"type": "level", "id": level, "name": "Level 2",
                    "elevationMm": 3000.0, "floorThicknessMm": 200.0,
                    "defaultWallHeightMm": 2500.0, "defaultCeilingHeightMm": 2500.0}}]}),
        )
        .unwrap();
    }

    fn instance(list: &Value, id: &str) -> Value {
        list["instances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap_or_else(|| panic!("instance {id} missing from {list}"))
            .clone()
    }

    #[test]
    fn copy_and_paste_recreate_a_posed_model_on_another_level() {
        let root = std::env::temp_dir().join(format!("domowik-clipboard-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("sofa.glb");
        std::fs::write(&file, physical_glb(1.)).unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        let second = Uuid::now_v7();
        add_level(h, second);

        let placed = request(
            h,
            json!({"kind":"place", "file":file, "levelId":level,
                "positionMm":[100.0,200.0]}),
        )
        .unwrap();
        let id = placed["instanceId"].as_str().unwrap().to_string();
        request(
            h,
            json!({"kind":"set", "instanceId":id, "positionMm":[100.0,200.0],
                "dimensionsMm":[2000.0,800.0], "rotationDeg":90.0, "heightMm":600.0}),
        )
        .unwrap();
        request(
            h,
            json!({"kind":"pose", "instanceId":id, "positionMm":[100.0,200.0,1500.0],
                "rotationDeg":[100.0,10.0,30.0]}),
        )
        .unwrap();

        let copied = request(
            h,
            json!({"kind":"copy", "instanceIds":[id], "referenceId":id}),
        )
        .unwrap();
        assert_eq!(copied["clipboardCount"], 1);

        let pasted = request(
            h,
            json!({"kind":"paste", "levelId":second, "positionMm":[7000.0,8000.0]}),
        )
        .unwrap();
        let new_id = pasted["instanceId"].as_str().unwrap().to_string();
        assert_ne!(new_id, id, "paste mints a fresh identity");
        assert_eq!(pasted["clipboardCount"], 1, "paste keeps the clipboard");

        let original = instance(&pasted, &id);
        let copy = instance(&pasted, &new_id);
        assert_eq!(copy["levelId"], second.to_string());
        assert_eq!(copy["digest"], original["digest"]);
        assert_eq!(copy["name"], original["name"]);
        assert_eq!(copy["dimensionsMm"], original["dimensionsMm"]);
        assert_eq!(copy["heightMm"], original["heightMm"]);
        assert_eq!(copy["rotationDeg"], original["rotationDeg"]);
        assert_eq!(copy["pose3d"], original["pose3d"]);
        assert_eq!(copy["scaleUniform"], original["scaleUniform"]);
        assert_eq!(copy["positionMm"], json!([7000.0, 8000.0]));

        // The paste is one undoable batch; the clipboard is view state and
        // survives both directions of history.
        undo(h).unwrap();
        let after_undo = request(h, json!({"kind":"list"})).unwrap();
        assert_eq!(after_undo["instances"].as_array().unwrap().len(), 1);
        assert_eq!(after_undo["clipboardCount"], 1);
        redo(h).unwrap();
        let after_redo = request(h, json!({"kind":"list"})).unwrap();
        assert_eq!(after_redo["instances"].as_array().unwrap().len(), 2);

        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cut_is_one_undoable_batch_and_keeps_relative_arrangement() {
        let root = std::env::temp_dir().join(format!("domowik-cut-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("chair.obj");
        std::fs::write(&file, b"v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n").unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        let second = Uuid::now_v7();
        add_level(h, second);

        let first = request(
            h,
            json!({"kind":"place", "file":file, "levelId":level,
                "positionMm":[1000.0,1000.0]}),
        )
        .unwrap()["instanceId"]
            .as_str()
            .unwrap()
            .to_string();
        let second_id = request(
            h,
            json!({"kind":"place", "file":file, "levelId":level,
                "positionMm":[2500.0,1000.0]}),
        )
        .unwrap()["instanceId"]
            .as_str()
            .unwrap()
            .to_string();

        request(
            h,
            json!({"kind":"cut", "instanceIds":[first, second_id],
                "referenceId":second_id}),
        )
        .unwrap();
        let cut_state = request(h, json!({"kind":"list"})).unwrap();
        assert!(cut_state["instances"].as_array().unwrap().is_empty());
        assert_eq!(cut_state["clipboardCount"], 2);

        // ONE undo restores both instances: the cut was a single batch.
        undo(h).unwrap();
        let restored = request(h, json!({"kind":"list"})).unwrap();
        assert_eq!(restored["instances"].as_array().unwrap().len(), 2);

        // Cut again, then paste the pair onto the other level: the anchor
        // (the second chair) lands at the pointer and the first keeps its
        // 1500 mm offset to the left.
        request(
            h,
            json!({"kind":"cut", "instanceIds":[first, second_id],
                "referenceId":second_id}),
        )
        .unwrap();
        let pasted = request(
            h,
            json!({"kind":"paste", "levelId":second, "positionMm":[5000.0,6000.0]}),
        )
        .unwrap();
        let anchor = pasted["instanceId"].as_str().unwrap();
        assert_eq!(
            instance(&pasted, anchor)["positionMm"],
            json!([5000.0, 6000.0])
        );
        let others: Vec<[f64; 2]> = pasted["instances"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["id"] != json!(anchor) && a["levelId"] == second.to_string())
            .map(|a| serde_json::from_value(a["positionMm"].clone()).unwrap())
            .collect();
        assert_eq!(others, vec![[3500.0, 6000.0]]);

        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copy_cut_and_paste_validate_their_targets_without_mutation() {
        let root = std::env::temp_dir().join(format!("domowik-clip-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("sofa.glb");
        std::fs::write(&file, physical_glb(1.)).unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        let id = request(
            h,
            json!({"kind":"place", "file":file, "levelId":level,
                "positionMm":[0.0,0.0]}),
        )
        .unwrap()["instanceId"]
            .as_str()
            .unwrap()
            .to_string();

        // Empty, unknown and anchor-less requests refuse whole; the clipboard
        // stays empty and the instance set is untouched.
        for bad in [
            json!({"kind":"copy", "instanceIds":[], "referenceId":id}),
            json!({"kind":"copy", "instanceIds":[Uuid::now_v7()], "referenceId":id}),
            json!({"kind":"copy", "instanceIds":[id], "referenceId":Uuid::now_v7()}),
            json!({"kind":"cut", "instanceIds":[id], "referenceId":Uuid::now_v7()}),
            json!({"kind":"paste", "levelId":level, "positionMm":[0.0,0.0]}),
        ] {
            let before = request(h, json!({"kind":"list"})).unwrap();
            assert_eq!(
                request(h, bad.clone()).unwrap_err(),
                status::REJECTED,
                "{bad}"
            );
            let after = request(h, json!({"kind":"list"})).unwrap();
            assert_eq!(after["instances"], before["instances"], "{bad}");
            assert_eq!(after["clipboardCount"], before["clipboardCount"], "{bad}");
        }

        // A paste with a clipboard but an unknown level or out-of-range
        // position refuses whole as well. JSON has no NaN, so a non-finite
        // coordinate arrives as `null` and is malformed, never rejected.
        request(
            h,
            json!({"kind":"copy", "instanceIds":[id], "referenceId":id}),
        )
        .unwrap();
        let before = request(h, json!({"kind":"list"})).unwrap();
        for bad in [
            json!({"kind":"paste", "levelId":Uuid::now_v7(), "positionMm":[0.0,0.0]}),
            json!({"kind":"paste", "levelId":level,
                "positionMm":[domowik_format::COORD_BOUND_MM + 1.0, 0.0]}),
        ] {
            assert_eq!(
                request(h, bad.clone()).unwrap_err(),
                status::REJECTED,
                "{bad}"
            );
        }
        assert_eq!(
            request(
                h,
                json!({"kind":"paste", "levelId":level, "positionMm":[f64::NAN, 0.0]})
            )
            .unwrap_err(),
            status::MALFORMED_REQUEST
        );
        let after = request(h, json!({"kind":"list"})).unwrap();
        assert_eq!(after["instances"], before["instances"]);
        // A valid paste after those refusals still works: refusals left the
        // clipboard intact.
        request(
            h,
            json!({"kind":"paste", "levelId":level, "positionMm":[1000.0,1000.0]}),
        )
        .unwrap();
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap()["instances"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paste_refuses_a_digest_whose_bytes_are_not_embedded() {
        let root = std::env::temp_dir().join(format!("domowik-ghost-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("sofa.glb");
        std::fs::write(&file, physical_glb(1.)).unwrap();
        let (h, level) = create();
        request(h, json!({"kind":"attach", "directory":root})).unwrap();
        // Give the plan an embedded library first, then a dangling instance.
        request(
            h,
            json!({"kind":"place", "file":file, "levelId":level,
                "positionMm":[0.0,0.0]}),
        )
        .unwrap();
        let ghost = Uuid::now_v7();
        execute(
            h,
            json!({"operations":[{
                "operationId": Uuid::now_v7(), "entityId": ghost,
                "kind": "createEntity",
                "entity": {"type": "assetInstance", "id": ghost, "levelId": level,
                    "digest": format!("sha256:{}", "ab".repeat(32)),
                    "transform": {"translationMm":[2000.0,0.0],"scaleUniform":1.0,"rotationDeg":0.0},
                    "displayName": "Ghost"}}]}),
        )
        .unwrap();
        request(
            h,
            json!({"kind":"copy", "instanceIds":[ghost], "referenceId":ghost}),
        )
        .unwrap();
        assert_eq!(
            request(
                h,
                json!({"kind":"paste", "levelId":level, "positionMm":[3000.0,0.0]})
            )
            .unwrap_err(),
            status::REJECTED
        );
        assert_eq!(
            request(h, json!({"kind":"list"})).unwrap()["instances"]
                .as_array()
                .unwrap()
                .len(),
            2,
            "the refused paste created nothing"
        );

        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Embeds an uploaded model file in the plan's library (ABI 16, issue 0088).
///
/// `name` is the file's UTF-8 name or source path. Its final component supplies
/// the media type and display name; a path is retained in asset provenance.
/// `bytes` are its contents. Both are borrowed for the call. Success writes JSON
/// `{"digest":…,"name":…}`; place it with `plan_assets` `place` by `digest`.
/// Refusals leave the plan unchanged and set the sticky last error.
///
/// # Safety
///
/// `name` and `bytes` must address `name_len` and `len` readable bytes for
/// the call; `out` must be a writable `domowik_buf*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_import_model(
    handle: u64,
    name: *const u8,
    name_len: u64,
    bytes: *const u8,
    len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let (Some(name), Some(bytes)) = (borrow(name, name_len), borrow(bytes, len)) else {
            return status::INVALID_ARGUMENT;
        };
        let Ok(name) = std::str::from_utf8(name) else {
            return status::INVALID_ARGUMENT;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        match import_model(&mut plan, name, bytes)
            .and_then(|v| serde_json::to_vec(&v).map_err(|e| e.to_string()))
        {
            Ok(bytes) => release(bytes, out),
            Err(e) => {
                plan.last_error = e;
                status::REJECTED
            }
        }
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SceneFrameRequest {
    level_ids: Vec<Uuid>,
    #[serde(default)]
    ghosted_ids: Vec<Uuid>,
    #[serde(default)]
    selected_ids: Vec<String>,
    #[serde(default)]
    pose_only: bool,
    #[serde(default)]
    scene_token: Option<u64>,
}

/// Derives the visible levels' 3D scene as one `domowik-scene-wire` frame
/// (ABI 16, issue 0088).
///
/// The browser's renderer is a separate module that cannot read this plan,
/// so the scene crosses as bytes: the same derivation as
/// `domowik_r3d_load_scene`, with imported models already resolved to their
/// decoded meshes. Request: `{"levelIds":[…],"ghostedIds":[…]}`. The header's
/// `extra.modelDecodes` is the plan's model-decode counter. Optional request
/// `poseOnly:true,sceneToken:<last installed token>` permits a warm transform
/// update without geometry derivation or model bytes. Such frames set
/// `extra.instanceOnly:true`: mesh entries carry names with zero vertices and
/// receivers MUST reuse resident buffers, not replace them. `extra.sceneToken`
/// identifies the baseline. Revision/visibility/token mismatch returns a full
/// frame with a fresh token. Omitting these request fields keeps full-frame
/// behavior for existing consumers. Preview framing/origin remains fixed.
/// Optional `selectedIds` tints the nodes of those walls, openings and models
/// in either kind of frame; selection never invalidates the baseline.
///
/// # Safety
///
/// `req` must address `req_len` readable bytes for the call; `out` must be a
/// writable `domowik_buf*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_scene_frame(
    handle: u64,
    req: *const u8,
    req_len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let Some(bytes) = borrow(req, req_len) else {
            return status::INVALID_ARGUMENT;
        };
        let request: SceneFrameRequest = match serde_json::from_slice(bytes) {
            Ok(r) => r,
            Err(e) => return fail(&plan, status::MALFORMED_REQUEST, e.to_string()),
        };
        let (scene, models) = match derive_scene(
            &plan,
            &request.level_ids,
            request.ghosted_ids,
            &request.selected_ids,
            request.pose_only,
            request.scene_token,
        ) {
            Ok(v) => v,
            Err(e) => return fail(&plan, status::REJECTED, e),
        };
        let decodes = plan
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .scene_model_decodes;
        match scene_frame(&scene, &models, decodes) {
            Ok(bytes) => release(bytes, out),
            Err(e) => fail(&plan, status::REJECTED, e),
        }
    })
}

/// Encodes `scene` with its resolved models. Mesh order is the scene's own
/// meshes (sorted keys) then required models in scene order; a required model
/// that did not decode is omitted, as the native presenter omits it.
pub(crate) fn scene_frame(
    scene: &domowik_scene::Scene,
    models: &SceneModels,
    model_decodes: u64,
) -> Result<Vec<u8>, String> {
    let meshes: Vec<(&str, &[f32])> = scene
        .meshes
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_slice()))
        .chain(
            scene
                .required_models
                .iter()
                .filter_map(|d| models.get(d).map(|m| (d.as_str(), m.vertices.as_slice()))),
        )
        .collect();
    let index: HashMap<&str, u32> = meshes
        .iter()
        .enumerate()
        .map(|(i, (k, _))| (*k, i as u32))
        .collect();
    let instances: Vec<domowik_scene_wire::Instance> = scene
        .nodes
        .iter()
        .filter_map(|n| {
            Some(domowik_scene_wire::Instance {
                mesh: *index.get(n.mesh.as_str())?,
                transparent: n.transparent,
                matrix: n.matrix,
                normal_matrix: n.normal_matrix,
                color: n.color,
            })
        })
        .collect();
    let header = domowik_scene_wire::Header {
        origin_mm: scene.origin_mm,
        center: scene.center,
        radius: scene.radius,
        diagnostics: scene.diagnostics.clone(),
        extra: serde_json::Map::from_iter([
            ("modelDecodes".into(), json!(model_decodes)),
            ("instanceOnly".into(), json!(scene.instance_only)),
            ("sceneToken".into(), json!(scene.scene_token)),
            ("poseVersion".into(), json!(scene.pose_version)),
        ]),
        ..Default::default()
    };
    let textures = models
        .iter()
        .filter(|(d, _)| index.contains_key(d.as_str()))
        .filter_map(|(d, m)| {
            m.texture.as_ref().map(|t| {
                (
                    d.clone(),
                    domowik_scene_wire::Texture {
                        width: t.width,
                        height: t.height,
                        layers: t.layers,
                        rgba: t.rgba.clone(),
                        coordinates: t.coordinates.clone(),
                    },
                )
            })
        })
        .collect();
    domowik_scene_wire::encode_textured(header, meshes, &instances, &textures)
}
