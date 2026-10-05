//! Plan-local user models and original 2D footprint semantics (BEHAVIOR-SPEC
//! §10.1/§10.4). Original asset bytes are preserved. Independently implemented
//! format decoders derive size/art; no external application catalog is parsed.

use super::*;
use domowik_format::{AssetInstance, InstanceTransform};
use domowik_geometry::spatial::SpatialIndex;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

const DIRECTORIES: &str = "editorAssetDirectories";
const FOOTPRINT: &str = "editorFootprintMm";
const MAX_DIMENSION_MM: f64 = 1_000_000.0;

/// Like the wall hit cache, retain one active-level index per accepted revision.
pub(super) struct FootprintIndex {
    level: Uuid,
    revision: usize,
    instances: Vec<AssetInstance>,
    spatial: SpatialIndex<usize>,
}

impl FootprintIndex {
    fn build(doc: &Document, level: Uuid, revision: usize) -> Self {
        let instances: Vec<_> = doc.asset_instances_on_level(level).cloned().collect();
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
            Some((
                Aabb::from_corners(
                    domowik_format::Point::new(min_x, min_y),
                    domowik_format::Point::new(max_x, max_y),
                ),
                i,
            ))
        }));
        Self {
            level,
            revision,
            instances,
            spatial,
        }
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
    },
    Mesh {
        digest: String,
    },
    Preview {
        digest: String,
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
    Delete {
        instance_id: Uuid,
    },
    #[serde(rename_all = "camelCase")]
    Hit {
        level_id: Uuid,
        point_mm: [f64; 2],
        radius_mm: f64,
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
    let [w, d] = dimensions(instance);
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

fn list(plan: &Plan) -> Value {
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
    let mut instances: Vec<Value> = plan.session().document().asset_instances.iter().map(|a| json!({
        "id": a.id, "levelId": a.level_id, "digest": a.digest,
        "name": a.display_name.as_deref().unwrap_or("Model"),
        "positionMm": a.transform.translation_mm, "rotationDeg": a.transform.rotation_deg,
        "scaleUniform": a.transform.scale_uniform, "dimensionsMm": dimensions(a), "cornersMm":
            corners(a).into_iter().filter(|p| p.iter().all(|v| v.is_finite() && v.abs() <= DERIVED_COORD_BOUND_MM)).collect::<Vec<_>>(),
        "heightMm": a.unknown.get("editorHeightMm").and_then(Value::as_f64)
            .filter(|h| h.is_finite() && *h > 0. && *h <= MAX_DIMENSION_MM)
    })).collect();
    instances.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    json!({"directories": directories(plan), "models": models, "instances": instances})
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
    }
    Ok(())
}

fn run(plan: &mut Plan, request: Request) -> Result<Value, String> {
    match request {
        Request::List => return Ok(list(plan)),
        Request::Preview { .. } | Request::Size { .. } => {
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
            let revision = document_revision(plan.session());
            if !matches!(&plan.asset_hits, Some(index) if index.level == level_id && index.revision == revision)
            {
                plan.asset_hits = Some(FootprintIndex::build(
                    plan.session().document(),
                    level_id,
                    revision,
                ));
            }
            let index = plan.asset_hits.as_ref().expect("just ensured");
            let mut hits: Vec<_> = index.spatial.near(domowik_format::Point::new(x, y), radius_mm).into_iter().filter_map(|i| {
                let a = &index.instances[i];
                let [w, d] = dimensions(a);
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
                    let extension = path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let media = match extension.as_str() {
                        "glb" => "model/gltf-binary",
                        "obj" => "model/obj",
                        "zip" => "application/zip",
                        _ => return Err("Supported model files: GLB, OBJ and ZIP".into()),
                    };
                    let bytes = read_model(&path, domowik_container::Limits::MAX_ENTRY_BYTES)?;
                    let digest = domowik_container::digest_of(&bytes);
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Model")
                        .to_owned();
                    let descriptor = json!({"digest": digest, "byteLength": bytes.len(), "mediaType":media,
                        "purpose":"furniture-model", "encoding":"raw", "requirement":"optional",
                        "provenance":{"displayName":name, "sourceDescription":path.to_string_lossy()}});
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
    }
    Ok(list(plan))
}

/// JSON asset requests on one plan. Uses the same serialization lock, lease,
/// panic containment and buffer ownership as `domowik_plan_execute`. No callbacks
/// or network access; file reads are synchronous and bounded by L5. Cancellation
/// is not supported during this bounded import. Available on packaged bridge
/// platforms with filesystem access; directory permissions belong to the host.
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
        } = request
        {
            if level_ids.len() > 256 {
                return fail(&plan, status::REJECTED, "Too many visible 3D levels");
            }
            let (doc, mut models, missing) = {
                let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
                let doc = locked.session().document().clone();
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
                (doc, locked.scene_models.clone(), missing)
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
                            let mesh = std::sync::Arc::new(mesh);
                            let mut locked = plan.lock().unwrap_or_else(|e| e.into_inner());
                            let total = locked
                                .scene_models
                                .values()
                                .map(|m| m.vertices.len() * 4)
                                .sum::<usize>();
                            if total + mesh.vertices.len() * 4 <= 128 * 1024 * 1024 {
                                locked.scene_model_decodes += 1;
                                locked.scene_models.insert(digest.clone(), mesh.clone());
                                models.insert(digest, mesh);
                            } else {
                                locked.scene_model_errors.insert(
                                    digest,
                                    "Decoded 3D model cache exceeds 128 MiB".into(),
                                );
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
            let heights = models
                .iter()
                .map(|(d, m)| (d.clone(), m.height_ratio))
                .collect();
            let scene = match domowik_scene::build(
                &doc,
                &level_ids,
                &ghosted_ids.into_iter().collect(),
                &heights,
            ) {
                Ok(s) => s,
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
        if let Request::Preview { ref digest } | Request::Size { ref digest } = request {
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
            let result = if matches!(request, Request::Size { .. }) {
                domowik_models::dimensions(&bytes)
                    .and_then(|p| serde_json::to_vec(&p).map_err(|e| e.to_string()))
            } else {
                domowik_models::preview(&bytes)
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
    fn directory_import_deduplicates_and_survives_source_removal_and_reopen() {
        let root = std::env::temp_dir().join(format!("domowik-library-{}", Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("original chair.glb");
        std::fs::write(&file, b"original inert model content").unwrap();
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
}
