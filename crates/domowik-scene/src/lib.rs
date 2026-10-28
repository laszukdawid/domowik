//! Renderer-neutral 3D scene derived from the authoritative document.
//! World coordinates are metres, +Y vertical, plan (x,y) maps to world (x,-z).
//! Geometry is origin-rebased for float32 GPU precision. This crate owns no GPU,
//! platform, network or document session. Camera state never enters this API.
//! Imported geometry is supplied through a digest-keyed caller cache; missing
//! models receive diagnostic boxes. See BEHAVIOR-SPEC §11 and issue-0078 evidence.

use domowik_format::{Document, Point, Side};
use domowik_geometry::{polygon::Polygon, walls};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub mod asset_pose;
pub mod pick;
pub mod usd;

/// `f32`s per mesh vertex: position xyz, normal xyz, rgba.
pub const VERTEX_FLOATS: usize = 10;

/// Display-space wall finish when no explicit per-wall colour is stored.
/// Light warm beige (#E8DCC4); the technical 2D plan keeps its own ink default.
const DEFAULT_WALL_COLOR: domowik_format::appearance::RgbColor =
    domowik_format::appearance::RgbColor([232, 220, 196]);

fn wall_faces_default(alpha: f32) -> [f32; 4] {
    let [r, g, b] = DEFAULT_WALL_COLOR.rgb();
    [r, g, b, alpha]
}

/// Display `[Left, Right]` face colours of a wall, defaults applied.
fn wall_faces(wall: &domowik_format::Wall, alpha: f32) -> [[f32; 4]; 2] {
    domowik_format::appearance::wall_faces(wall).map(|c| {
        let [r, g, b] = c.unwrap_or(DEFAULT_WALL_COLOR).rgb();
        [r, g, b, alpha]
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    /// Internal presenter update: mesh entries carry names only and must reuse
    /// resident geometry. Ordinary document-derived scenes always set false.
    pub instance_only: bool,
    /// Presenter baseline identity, assigned by the bridge (zero in pure builds).
    pub scene_token: u64,
    /// The bridge's pose-overlay version these node transforms reflect (zero in
    /// pure builds). View queries name it so they match what was drawn.
    pub pose_version: u64,
    pub origin_mm: [f64; 2],
    pub center: [f32; 3],
    pub radius: f32,
    pub nodes: Vec<Node>,
    pub meshes: BTreeMap<String, Vec<f32>>,
    pub required_models: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Serialize)]
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

/// The unit `builtin:cube` mesh: X/Z in [-0.5, 0.5], Y in [0, 1].
pub fn cube() -> Vec<f32> {
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
    /// Render a reusable rectangular/sloped prism, with only its own cuts.
    fn construction(
        &mut self,
        doc: &Document,
        body: &domowik_format::ConstructionBody,
        id: &str,
        alpha: f32,
        cut: bool,
    ) -> Result<(), String> {
        use domowik_geometry::architecture::{cut_surfaces, rectangle};
        let surfaces = if cut {
            cut_surfaces(
                doc,
                domowik_format::OpeningHost::ConstructionBody(body.id),
                &[rectangle(&body.footprint)],
            )
        } else {
            vec![rectangle(&body.footprint)]
        };
        let level = doc
            .level(body.level_id)
            .ok_or("missing construction level")?;
        let [ax, ay] = body.footprint.start_mm;
        let [bx, by] = body.footprint.end_mm;
        let dx = bx - ax;
        let dy = by - ay;
        let len2 = dx * dx + dy * dy;
        let delta = body.bottom_offsets_mm[1] - body.bottom_offsets_mm[0];
        let gx = delta * dx / len2;
        let gz = -delta * dy / len2;
        for (i, p) in surfaces.iter().enumerate() {
            let id = format!("{id}:{i}");
            let color = if body.role == domowik_format::BodyRole::Guard {
                [0.36, 0.4, 0.43, alpha]
            } else {
                [0.65, 0.56, 0.43, alpha]
            };
            self.polygon(id.clone(), p, 0.0, body.thickness_mm, color)?;
            let key = format!("geometry:{id}");
            if let Some(vertices) = self.scene.meshes.get_mut(&key) {
                for v in vertices.as_chunks_mut::<10>().0 {
                    let x = f64::from(v[0]) * 1000.0 + self.scene.origin_mm[0];
                    let y = -f64::from(v[2]) * 1000.0 + self.scene.origin_mm[1];
                    let t = ((x - ax) * dx + (y - ay) * dy) / len2;
                    v[1] += ((level.elevation_mm + body.bottom_offsets_mm[0] + delta * t) / 1000.0)
                        as f32;
                    let n = [
                        f64::from(v[3]) - gx * f64::from(v[4]),
                        f64::from(v[4]),
                        f64::from(v[5]) - gz * f64::from(v[4]),
                    ];
                    let length = n[0].hypot(n[1]).hypot(n[2]);
                    for k in 0..3 {
                        v[k + 3] = (n[k] / length) as f32;
                    }
                    for (k, value) in v.iter().take(3).enumerate() {
                        self.low[k] = self.low[k].min(f64::from(*value));
                        self.high[k] = self.high[k].max(f64::from(*value));
                    }
                }
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn asset_node(
        &mut self,
        id: String,
        mesh: String,
        pos: [f64; 3],
        scale: [f64; 3],
        angles: [f64; 3],
        color: [f32; 4],
        height_ratio: f64,
    ) {
        if scale.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return;
        }
        if angles[0] == 0. && angles[1] == 0. {
            self.node(id, mesh, pos, scale, angles[2], color, height_ratio);
            return;
        }
        let (matrix, normal_matrix) = asset_pose::matrices(pos, scale, angles, height_ratio);
        for x in [-0.5, 0.5] {
            for y in [0., height_ratio] {
                for z in [-0.5, 0.5] {
                    self.see(std::array::from_fn(|row| {
                        f64::from(matrix[12 + row])
                            + x * f64::from(matrix[row])
                            + y * f64::from(matrix[4 + row])
                            + z * f64::from(matrix[8 + row])
                    }));
                }
            }
        }
        self.scene.nodes.push(Node {
            id,
            mesh,
            matrix,
            normal_matrix,
            color,
            transparent: color[3] < 0.99,
        });
    }
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
    /// A wall slab across `[η0, η1]` of the thickness (`η` toward Left),
    /// split at the axis into `:L`/`:R` halves when the face colours differ.
    #[allow(clippy::too_many_arguments)]
    fn wall_slab(
        &mut self,
        id: String,
        a: Point,
        b: Point,
        bottom: f64,
        height: f64,
        (e0, e1): (f64, f64),
        [left, right]: [[f32; 4]; 2],
    ) {
        let length = (b.x - a.x).hypot(b.y - a.y);
        if length <= 0. {
            return;
        }
        let n = Point::new(-(b.y - a.y) / length, (b.x - a.x) / length);
        let mut slab = |id: String, e0: f64, e1: f64, color| {
            let c = (e0 + e1) / 2.;
            self.box_segment(
                id,
                Point::new(a.x + n.x * c, a.y + n.y * c),
                Point::new(b.x + n.x * c, b.y + n.y * c),
                bottom,
                height,
                e1 - e0,
                color,
            );
        };
        if left == right || e0 >= 0. {
            slab(id, e0, e1, left);
        } else if e1 <= 0. {
            slab(id, e0, e1, right);
        } else {
            slab(format!("{id}:R"), e0, 0., right);
            slab(format!("{id}:L"), 0., e1, left);
        }
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
///
/// Floors are each level's rooms from a one-shot `walls::plan`. A caller that
/// already retains them per level uses [`build_with_floors`] instead.
pub fn build(
    doc: &Document,
    levels: &[Uuid],
    ghosted: &BTreeSet<Uuid>,
    models: &BTreeMap<String, f64>,
) -> Result<Scene, String> {
    let ids: BTreeSet<_> = levels.iter().copied().collect();
    let floors = doc
        .levels
        .iter()
        .filter(|level| ids.contains(&level.id))
        .map(|level| {
            let solids = walls::level_solids(doc, level.id);
            (level.id, walls::plan(&solids.walls, &solids.joins).1)
        })
        .collect();
    build_with_floors(doc, levels, ghosted, models, &floors)
}

/// [`build`] with each requested level's rooms supplied by the caller.
///
/// `floors` must hold, for every level of `doc` named in `levels`, exactly the
/// rooms `walls::plan` computes for that level of THIS `doc`, in its canonical
/// order (floor node ids use the room's ordinal). A level with no rooms needs
/// an explicit empty entry: a missing one is an error, never an empty floor.
/// Entries for other levels are ignored.
pub fn build_with_floors(
    doc: &Document,
    levels: &[Uuid],
    ghosted: &BTreeSet<Uuid>,
    models: &BTreeMap<String, f64>,
    floors: &BTreeMap<Uuid, Vec<Polygon>>,
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
    for f in doc
        .construction_bodies
        .iter()
        .filter(|b| ids.contains(&b.level_id))
        .map(|b| b.footprint.clone())
        .chain(doc.staircases.iter().filter_map(|s| {
            let c = doc.connection(s.connection_id)?;
            (ids.contains(&c.from.level_id) || ids.contains(&c.to.level_id)).then_some(
                domowik_format::RectFootprint {
                    start_mm: c.from.position_mm,
                    end_mm: c.to.position_mm,
                    width_mm: s.width_mm,
                },
            )
        }))
    {
        for p in f.corners() {
            for k in 0..2 {
                low[k] = low[k].min(p[k]);
                high[k] = high[k].max(p[k]);
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
            instance_only: false,
            scene_token: 0,
            pose_version: 0,
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
        let rooms = floors
            .get(&level.id)
            .ok_or_else(|| format!("no floor geometry supplied for level {}", level.id))?;
        for (i, room) in rooms.iter().enumerate() {
            for (part, p) in domowik_geometry::architecture::cut_surfaces(
                doc,
                domowik_format::OpeningHost::LevelFloor(level.id),
                std::slice::from_ref(room),
            )
            .iter()
            .enumerate()
            {
                b.polygon(
                    if part == 0 {
                        format!("floor:{}:{i}", level.id)
                    } else {
                        format!("floor:{}:{i}:{part}", level.id)
                    },
                    p,
                    level.elevation_mm - level.floor_thickness_mm,
                    level.floor_thickness_mm.max(1.),
                    [0.72, 0.67, 0.57, alpha],
                )?;
            }
        }
        for body in doc
            .construction_bodies
            .iter()
            .filter(|body| body.level_id == level.id)
        {
            b.construction(doc, body, &format!("body:{}", body.id), alpha, true)?;
        }
        for w in doc.walls_on_level(level.id) {
            let faces = wall_faces(w, alpha);
            let half = w.thickness_mm / 2.;
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
                    b.wall_slab(
                        format!("wall:{}:{i}:{j}", w.id),
                        at(span[0]),
                        at(span[1]),
                        level.elevation_mm + bottom,
                        top - bottom,
                        (-half, half),
                        faces,
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
                    let style = domowik_format::appearance::door_style(o)?;
                    let [red, green, blue] = domowik_format::appearance::door_color(o)?
                        .unwrap_or_else(|| style.color())
                        .rgb();
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
                        [red, green, blue, style.opacity() * alpha],
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
                    let backing = if o.side == domowik_format::Side::Left {
                        (-half, half - o.depth_mm)
                    } else {
                        (-half + o.depth_mm, half)
                    };
                    b.wall_slab(
                        format!("recess:{}", o.id),
                        left,
                        right,
                        bottom,
                        o.height_mm,
                        backing,
                        faces,
                    );
                }
            }
        }
        for (i, join) in solids.joins.iter().enumerate() {
            let members: Vec<_> = join
                .members
                .iter()
                .filter_map(|m| doc.wall(solids.ids[m.wall]))
                .collect();
            let height = members
                .iter()
                .map(|w| w.height_mm)
                .fold(f64::INFINITY, f64::min);
            if !height.is_finite() {
                continue;
            }
            // Each wedge between member axes takes the lower-UUID bounding
            // face; a join whose wedges agree stays one patch.
            let wedges: Vec<_> = walls::join_wedges(&solids.walls, join)
                .into_iter()
                .map(|wedge| {
                    let (wall, side) = *wedge
                        .faces
                        .iter()
                        .min_by_key(|(w, _)| solids.ids[*w])
                        .expect("two faces");
                    let color = doc
                        .wall(solids.ids[wall])
                        .map(|w| wall_faces(w, alpha)[usize::from(side == Side::Right)])
                        .unwrap_or(wall_faces_default(alpha));
                    (color, wedge.pieces)
                })
                .collect();
            let Some(first) = wedges.first() else {
                continue;
            };
            if wedges.iter().all(|(c, _)| *c == first.0) {
                let patch = walls::join_patch(&solids.walls, join);
                b.polygon(
                    format!("join:{}:{i}", level.id),
                    &Polygon {
                        outer: patch,
                        holes: vec![],
                    },
                    level.elevation_mm,
                    height,
                    first.0,
                )?;
                continue;
            }
            let mut k = 0;
            for (color, pieces) in wedges {
                for outer in pieces {
                    b.polygon(
                        format!("join:{}:{i}:{k}", level.id),
                        &Polygon {
                            outer,
                            holes: vec![],
                        },
                        level.elevation_mm,
                        height,
                        color,
                    )?;
                    k += 1;
                }
            }
        }
        for a in doc.asset_instances_on_level(level.id) {
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
            let pose =
                asset_pose::placement(a, b.scene.origin_mm, level.elevation_mm, height_ratio);
            b.asset_node(
                a.id.to_string(),
                key,
                pose.position_m,
                pose.scale_m,
                pose.angles_deg,
                // Only the level fade marks a model transparent. Source material
                // opacity (USD glass, say) is kept in vertex alpha but drawn in
                // the opaque pass, so it shows solid. Splitting models into
                // opaque and blended parts is not worth it for current plans.
                [1., 1., 1., alpha],
                height_ratio,
            );
        }
    }
    for s in &doc.staircases {
        let Some(c) = doc.connection(s.connection_id) else {
            b.scene
                .diagnostics
                .push(domowik_geometry::architecture::stair_diagnostic(
                    s,
                    "missing stair connection",
                ));
            continue;
        };
        let visible: Vec<_> = [c.from.level_id, c.to.level_id]
            .into_iter()
            .filter(|id| ids.contains(id))
            .collect();
        if visible.is_empty() {
            continue;
        }
        let alpha = if visible.iter().all(|id| ghosted.contains(id)) {
            0.3
        } else {
            1.0
        };
        let geometry = match domowik_geometry::architecture::stair(doc, s) {
            Ok(geometry) => geometry,
            Err(error) => {
                b.scene
                    .diagnostics
                    .push(domowik_geometry::architecture::stair_diagnostic(s, &error));
                continue;
            }
        };
        for (i, body) in geometry.bodies.iter().enumerate() {
            b.construction(doc, body, &format!("stair:{}:{i}", s.id), alpha, false)?;
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
