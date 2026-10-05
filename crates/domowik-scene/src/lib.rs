//! Renderer-neutral 3D scene derived from the authoritative document.
//! World coordinates are metres, +Y vertical, plan (x,y) maps to world (x,-z).
//! Geometry is origin-rebased for float32 GPU precision. This crate owns no GPU,
//! platform, network or document session. Camera state never enters this API.
//! Imported geometry is supplied through a digest-keyed caller cache; missing
//! models receive diagnostic boxes. See BEHAVIOR-SPEC §11 and issue-0078 evidence.

use domowik_format::{Document, Point};
use domowik_geometry::{polygon::Polygon, walls};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub origin_mm: [f64; 2],
    pub center: [f32; 3],
    pub radius: f32,
    pub nodes: Vec<Node>,
    pub meshes: BTreeMap<String, Vec<f32>>,
    pub required_models: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub mesh: String,
    pub matrix: [f32; 16],
    pub normal_matrix: [f32; 9],
    pub color: [f32; 4],
    pub transparent: bool,
}

fn matrix(position: [f64; 3], scale: [f64; 3], yaw: f64) -> ([f32; 16], [f32; 9]) {
    let (s, c) = yaw.rem_euclid(360.).to_radians().sin_cos();
    let m = [
        c * scale[0],
        0.,
        -s * scale[0],
        0.,
        0.,
        scale[1],
        0.,
        0.,
        s * scale[2],
        0.,
        c * scale[2],
        0.,
        position[0],
        position[1],
        position[2],
        1.,
    ];
    let n = [
        c / scale[0],
        0.,
        -s / scale[0],
        0.,
        1. / scale[1],
        0.,
        s / scale[2],
        0.,
        c / scale[2],
    ];
    (m.map(|v| v as f32), n.map(|v| v as f32))
}

fn cube() -> Vec<f32> {
    let p = [
        [-0.5, 0., -0.5],
        [0.5, 0., -0.5],
        [0.5, 1., -0.5],
        [-0.5, 1., -0.5],
        [-0.5, 0., 0.5],
        [0.5, 0., 0.5],
        [0.5, 1., 0.5],
        [-0.5, 1., 0.5],
    ];
    let faces = [
        ([0, 3, 2, 1], [0., 0., -1.]),
        ([4, 5, 6, 7], [0., 0., 1.]),
        ([0, 4, 7, 3], [-1., 0., 0.]),
        ([1, 2, 6, 5], [1., 0., 0.]),
        ([3, 7, 6, 2], [0., 1., 0.]),
        ([0, 1, 5, 4], [0., -1., 0.]),
    ];
    let mut out = Vec::new();
    for (f, n) in faces {
        for i in [0, 1, 2, 0, 2, 3] {
            out.extend(p[f[i]]);
            out.extend(n);
            out.extend([1.; 4]);
        }
    }
    out
}

struct Builder {
    scene: Scene,
    low: [f64; 3],
    high: [f64; 3],
}
impl Builder {
    fn point(&self, p: Point, height: f64) -> [f64; 3] {
        [
            (p.x - self.scene.origin_mm[0]) / 1000.,
            height / 1000.,
            -(p.y - self.scene.origin_mm[1]) / 1000.,
        ]
    }
    fn see(&mut self, p: [f64; 3]) {
        for (k, value) in p.into_iter().enumerate() {
            self.low[k] = self.low[k].min(value);
            self.high[k] = self.high[k].max(value);
        }
    }
    #[allow(clippy::too_many_arguments)] // Named primitive dimensions keep units explicit.
    fn node(
        &mut self,
        id: String,
        mesh: String,
        pos: [f64; 3],
        scale: [f64; 3],
        yaw: f64,
        color: [f32; 4],
        height_ratio: f64,
    ) {
        if scale.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return;
        }
        if height_ratio > 0. {
            let (s, c) = yaw.to_radians().sin_cos();
            let rx = (c.abs() * scale[0] + s.abs() * scale[2]) / 2.;
            let rz = (s.abs() * scale[0] + c.abs() * scale[2]) / 2.;
            self.see([pos[0] - rx, pos[1], pos[2] - rz]);
            self.see([pos[0] + rx, pos[1] + scale[1] * height_ratio, pos[2] + rz]);
        }
        let (matrix, normal_matrix) = matrix(pos, scale, yaw);
        self.scene.nodes.push(Node {
            id,
            mesh,
            matrix,
            normal_matrix,
            color,
            transparent: color[3] < 0.99,
        });
    }
    #[allow(clippy::too_many_arguments)] // Private prism builder: axis, elevation and dimensions.
    fn box_segment(
        &mut self,
        id: String,
        a: Point,
        b: Point,
        bottom: f64,
        height: f64,
        thickness: f64,
        color: [f32; 4],
    ) {
        if height <= 0. || thickness <= 0. {
            return;
        }
        let delta = Point::new(b.x - a.x, b.y - a.y);
        let length = delta.x.hypot(delta.y);
        if length <= 0. {
            return;
        }
        self.node(
            id,
            "builtin:cube".into(),
            self.point(Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.), bottom),
            [length / 1000., height / 1000., thickness / 1000.],
            delta.y.atan2(delta.x).to_degrees(),
            color,
            1.,
        );
    }
    fn polygon(
        &mut self,
        id: String,
        p: &Polygon,
        bottom: f64,
        height: f64,
        color: [f32; 4],
    ) -> Result<(), String> {
        if p.outer.len() < 3 {
            return Ok(());
        }
        let mut coords = Vec::new();
        let mut holes = Vec::new();
        for (i, ring) in std::iter::once(&p.outer).chain(p.holes.iter()).enumerate() {
            if i > 0 {
                holes.push(coords.len() / 2);
            }
            for &v in ring {
                let v = self.point(v, bottom);
                coords.extend([v[0], v[2]]);
                self.see(v);
            }
        }
        if coords.len() > 200000 {
            return Err("Floor polygon exceeds scene vertex budget".into());
        }
        let triangles = earcutr::earcut(&coords, &holes, 2).map_err(|e| e.to_string())?;
        let mut vertices = Vec::new();
        for y in [bottom / 1000., (bottom + height) / 1000.] {
            let sign = if y == bottom / 1000. { -1. } else { 1. };
            let cap_indices: Vec<_> = triangles
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|t| if sign > 0. { [t[2], t[1], t[0]] } else { *t })
                .collect();
            for &i in &cap_indices {
                vertices.extend([
                    coords[i * 2] as f32,
                    y as f32,
                    coords[i * 2 + 1] as f32,
                    0.,
                    sign,
                    0.,
                    1.,
                    1.,
                    1.,
                    1.,
                ]);
            }
        }
        for ring in std::iter::once(&p.outer).chain(p.holes.iter()) {
            for i in 0..ring.len() {
                let a = self.point(ring[i], bottom);
                let b = self.point(ring[(i + 1) % ring.len()], bottom);
                let len = (b[0] - a[0]).hypot(b[2] - a[2]);
                if len == 0. {
                    continue;
                }
                let normal = [
                    ((b[2] - a[2]) / len) as f32,
                    0.,
                    (-(b[0] - a[0]) / len) as f32,
                ];
                for (v, up) in [
                    (a, false),
                    (b, false),
                    (b, true),
                    (a, false),
                    (b, true),
                    (a, true),
                ] {
                    vertices.extend([
                        v[0] as f32,
                        (v[1] + if up { height / 1000. } else { 0. }) as f32,
                        v[2] as f32,
                    ]);
                    vertices.extend(normal);
                    vertices.extend([1.; 4]);
                }
            }
        }
        let key = format!("geometry:{id}");
        self.scene.meshes.insert(key.clone(), vertices);
        self.node(id, key, [0.; 3], [1.; 3], 0., color, 0.);
        Ok(())
    }
}

/// Build only when a document/visibility revision changes. Model `height_ratio`
/// describes normalized cached meshes; their vertices are requested separately
/// by the platform uploader, so a scene refresh does not resend furniture bytes.
pub fn build(
    doc: &Document,
    levels: &[Uuid],
    ghosted: &BTreeSet<Uuid>,
    models: &BTreeMap<String, f64>,
) -> Result<Scene, String> {
    let ids: BTreeSet<_> = levels.iter().copied().collect();
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for w in &doc.walls {
        if ids.contains(&w.level_id) {
            for p in [w.start(), w.end()] {
                for (k, v) in [p.x, p.y].into_iter().enumerate() {
                    low[k] = low[k].min(v);
                    high[k] = high[k].max(v);
                }
            }
        }
    }
    for a in &doc.asset_instances {
        if ids.contains(&a.level_id) {
            for (k, v) in a.transform.translation_mm.into_iter().enumerate() {
                low[k] = low[k].min(v);
                high[k] = high[k].max(v);
            }
        }
    }
    let origin_mm = std::array::from_fn(|k| {
        if low[k].is_finite() {
            (low[k] + high[k]) / 2.
        } else {
            0.
        }
    });
    let mut b = Builder {
        scene: Scene {
            origin_mm,
            center: [0.; 3],
            radius: 5.,
            nodes: vec![],
            meshes: BTreeMap::from([("builtin:cube".into(), cube())]),
            required_models: vec![],
            diagnostics: vec![],
        },
        low: [f64::INFINITY; 3],
        high: [f64::NEG_INFINITY; 3],
    };
    let mut required = BTreeSet::new();
    for level in &doc.levels {
        if !ids.contains(&level.id) {
            continue;
        }
        let alpha = if ghosted.contains(&level.id) { 0.3 } else { 1. };
        let solids = walls::level_solids(doc, level.id);
        let (_, rooms) = walls::plan(&solids.walls, &solids.joins);
        for (i, room) in rooms.iter().enumerate() {
            b.polygon(
                format!("floor:{}:{i}", level.id),
                room,
                level.elevation_mm - level.floor_thickness_mm,
                level.floor_thickness_mm.max(1.),
                [0.72, 0.67, 0.57, alpha],
            )?;
        }
        for w in doc.walls_on_level(level.id) {
            let a = w.start();
            let z = w.end();
            let length = (z.x - a.x).hypot(z.y - a.y);
            if length == 0. {
                continue;
            }
            let mut cuts = vec![0., length];
            let mut openings = Vec::new();
            for o in doc.openings_on_wall(w.id) {
                let x = o.u * length - o.width_mm / 2.;
                let y = o.u * length + o.width_mm / 2.;
                cuts.extend([x, y]);
                openings.push((x, y, o));
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup();
            let at = |x: f64| {
                Point::new(
                    a.x + (z.x - a.x) * x / length,
                    a.y + (z.y - a.y) * x / length,
                )
            };
            for (i, span) in cuts.windows(2).enumerate() {
                let opening = openings
                    .iter()
                    .find(|(x, y, _)| {
                        (span[0] + span[1]) / 2. > *x && (span[0] + span[1]) / 2. < *y
                    })
                    .map(|(_, _, o)| *o);
                let mut intervals = vec![(0., w.height_mm)];
                if let Some(o) = opening {
                    intervals = vec![(0., o.sill_mm), (o.sill_mm + o.height_mm, w.height_mm)];
                }
                for (j, (bottom, top)) in intervals.into_iter().enumerate() {
                    b.box_segment(
                        format!("wall:{}:{i}:{j}", w.id),
                        at(span[0]),
                        at(span[1]),
                        level.elevation_mm + bottom,
                        top - bottom,
                        w.thickness_mm,
                        [0.87, 0.86, 0.82, alpha],
                    );
                }
            }
            for (x, y, o) in openings {
                let left = at(x);
                let right = at(y);
                let bottom = level.elevation_mm + o.sill_mm;
                for (j, p) in [left, right].into_iter().enumerate() {
                    b.box_segment(
                        format!("frame:{}:{j}", o.id),
                        at(if j == 0 { x + 15. } else { y - 15. }),
                        at(if j == 0 { x + 45. } else { y - 45. }),
                        bottom,
                        o.height_mm,
                        w.thickness_mm + 20.,
                        [0.95, 0.95, 0.94, alpha],
                    );
                    let _ = p;
                }
                if o.kind == domowik_format::OpeningKind::Door {
                    let (hinge, tip) = if o.effective_mechanism()
                        == domowik_format::OpeningMechanism::DoorSliding
                    {
                        (left, right)
                    } else {
                        walls::leaf_endpoints(o, left, right)
                    };
                    b.box_segment(
                        format!("door:{}", o.id),
                        hinge,
                        tip,
                        bottom,
                        o.height_mm,
                        35.,
                        [0.62, 0.42, 0.25, alpha],
                    );
                } else {
                    let (left, right) = if o.effective_mechanism()
                        == domowik_format::OpeningMechanism::WindowSideHung
                    {
                        walls::leaf_endpoints(o, left, right)
                    } else {
                        (left, right)
                    };
                    b.box_segment(
                        format!("window:{}", o.id),
                        left,
                        right,
                        bottom,
                        o.height_mm,
                        8.,
                        [0.5, 0.75, 0.9, 0.25 * alpha],
                    );
                }
                b.box_segment(
                    format!("lintel:{}", o.id),
                    left,
                    right,
                    bottom + o.height_mm - 40.,
                    40.,
                    w.thickness_mm + 20.,
                    [0.95, 0.95, 0.94, alpha],
                );
                if o.depth_mm < w.thickness_mm {
                    let side = if o.side == domowik_format::Side::Left {
                        -1.
                    } else {
                        1.
                    };
                    let shift = Point::new(
                        -(z.y - a.y) / length * o.depth_mm * side / 2.,
                        (z.x - a.x) / length * o.depth_mm * side / 2.,
                    );
                    b.box_segment(
                        format!("recess:{}", o.id),
                        Point::new(left.x + shift.x, left.y + shift.y),
                        Point::new(right.x + shift.x, right.y + shift.y),
                        bottom,
                        o.height_mm,
                        w.thickness_mm - o.depth_mm,
                        [0.87, 0.86, 0.82, alpha],
                    );
                }
            }
        }
        for (i, join) in solids.joins.iter().enumerate() {
            let patch = walls::join_patch(&solids.walls, join);
            if patch.len() < 3 {
                continue;
            }
            let height = join
                .members
                .iter()
                .filter_map(|m| doc.wall(solids.ids[m.wall]))
                .map(|w| w.height_mm)
                .fold(f64::INFINITY, f64::min);
            if height.is_finite() {
                b.polygon(
                    format!("join:{}:{i}", level.id),
                    &Polygon {
                        outer: patch,
                        holes: vec![],
                    },
                    level.elevation_mm,
                    height,
                    [0.87, 0.86, 0.82, alpha],
                )?;
            }
        }
        for a in doc.asset_instances_on_level(level.id) {
            let dims = a
                .unknown
                .get("editorFootprintMm")
                .and_then(|v| serde_json::from_value::<[f64; 2]>(v.clone()).ok())
                .filter(|p| p.iter().all(|v| v.is_finite() && *v > 0.))
                .unwrap_or([1000.; 2]);
            let uniform = a.transform.scale_uniform;
            let model = models.get(&a.digest);
            let (key, height_ratio) = if let Some(m) = model {
                required.insert(a.digest.clone());
                // Positive model proportions must remain exact even for thin
                // objects. A zero-height surface needs only a bounds divisor.
                (a.digest.clone(), if *m > 0. { *m } else { 0.001 })
            } else {
                b.scene.diagnostics.push(format!(
                    "{}: model unavailable for 3D",
                    a.display_name.as_deref().unwrap_or("Model")
                ));
                ("builtin:cube".into(), 1.)
            };
            let vertical_scale = a
                .unknown
                .get("editorHeightMm")
                .and_then(serde_json::Value::as_f64)
                .filter(|h| h.is_finite() && *h > 0. && *h <= 1_000_000.)
                .map(|h| h / height_ratio)
                .unwrap_or(dims[0]);
            b.node(
                a.id.to_string(),
                key,
                b.point(a.origin(), level.elevation_mm),
                [
                    dims[0] * uniform / 1000.,
                    vertical_scale * uniform / 1000.,
                    dims[1] * uniform / 1000.,
                ],
                a.transform.rotation_deg,
                [1., 1., 1., alpha],
                height_ratio,
            );
        }
    }
    if b.low[0].is_finite() {
        for k in 0..3 {
            b.scene.center[k] = ((b.low[k] + b.high[k]) / 2.) as f32;
        }
        b.scene.radius = ((b.high[0] - b.low[0])
            .hypot(b.high[1] - b.low[1])
            .hypot(b.high[2] - b.low[2])
            / 2.)
            .max(1.) as f32;
    }
    b.scene.required_models = required.into_iter().collect();
    if b.scene.nodes.len() > 20000
        || b.scene.meshes.values().map(Vec::len).sum::<usize>() > 10_000_000
    {
        return Err("Scene exceeds interactive geometry budget".into());
    }
    Ok(b.scene)
}
