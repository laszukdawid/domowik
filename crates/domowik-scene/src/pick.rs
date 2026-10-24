//! View-only queries over a built [`Scene`]: which plan entity a node draws,
//! what a camera ray hits first, and the bounds of chosen entities.
//!
//! Units are the scene's origin-rebased metres (+Y up, plan (x,y) → (x,-z)).
//! These answer "what is under the cursor" for presentation only, in `f64`
//! with a relative tolerance; they are not document predicates and never feed
//! joining, splitting or persistence. Results are deterministic for the same
//! nodes and meshes: ties keep the earlier node. Inputs that are not finite,
//! zero directions and singular node matrices produce no hit rather than an
//! error. Each query is linear in nodes, plus triangles of AABB-hit meshes.

use crate::Node;
use uuid::Uuid;

/// What a scene node depicts in the plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Wall,
    Opening,
    Asset,
    /// Floor slabs and wall-join patches: occluders with no selectable entity.
    Structure,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Wall => "wall",
            EntityKind::Opening => "opening",
            EntityKind::Asset => "asset",
            EntityKind::Structure => "structure",
        }
    }
}

/// The entity a node id produced by [`crate::build`] depicts. Structure nodes
/// return their level id; unknown id shapes return `None`.
pub fn node_entity(id: &str) -> Option<(EntityKind, Uuid)> {
    let mut parts = id.split(':');
    let first = parts.next()?;
    let Some(second) = parts.next() else {
        return Uuid::parse_str(first).ok().map(|u| (EntityKind::Asset, u));
    };
    let kind = match first {
        "wall" => EntityKind::Wall,
        "frame" | "door" | "window" | "lintel" | "recess" => EntityKind::Opening,
        "floor" | "join" => EntityKind::Structure,
        _ => return None,
    };
    Uuid::parse_str(second).ok().map(|u| (kind, u))
}

/// Selection tint mixed into highlighted nodes' colour.
pub const HIGHLIGHT_RGB: [f32; 3] = [0.18, 0.48, 1.0];

/// Tints every wall, opening and model node whose entity is in `selected`,
/// keeping alpha so transparency and ghosting are unchanged. Linear in nodes.
pub fn highlight(nodes: &mut [Node], selected: &std::collections::HashSet<Uuid>) {
    if selected.is_empty() {
        return;
    }
    for node in nodes {
        let chosen = node_entity(&node.id)
            .is_some_and(|(kind, id)| kind != EntityKind::Structure && selected.contains(&id));
        if chosen {
            for (channel, tint) in node.color.iter_mut().zip(HIGHLIGHT_RGB) {
                *channel = *channel * 0.45 + tint * 0.55;
            }
        }
    }
}

/// The nearest ray intersection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Index into the queried node slice.
    pub node: usize,
    /// Distance along the normalized ray, metres.
    pub distance: f64,
    pub point: [f64; 3],
}

/// Mesh-local axis-aligned box: `(low, high)` in mesh units.
pub type Bounds = ([f64; 3], [f64; 3]);

/// A mesh as the queries need it: triangle-list vertices and their
/// precomputed [`mesh_bounds`]. Callers cache bounds with the mesh so a query
/// never rescans vertices to reject a node.
#[derive(Clone, Copy)]
pub struct MeshView<'a> {
    pub vertices: &'a [f32],
    pub bounds: Bounds,
}

/// Axis-aligned bounds of a vertex buffer in mesh-local space. Linear in
/// vertices; compute once per mesh, not per query.
pub fn mesh_bounds(vertices: &[f32]) -> Option<Bounds> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for v in vertices.as_chunks::<{ crate::VERTEX_FLOATS }>().0 {
        for k in 0..3 {
            low[k] = low[k].min(f64::from(v[k]));
            high[k] = high[k].max(f64::from(v[k]));
        }
    }
    low[0].is_finite().then_some((low, high))
}

fn transform(m: &[f32; 16], p: [f64; 3], w: f64) -> [f64; 3] {
    std::array::from_fn(|r| {
        f64::from(m[r]) * p[0]
            + f64::from(m[4 + r]) * p[1]
            + f64::from(m[8 + r]) * p[2]
            + f64::from(m[12 + r]) * w
    })
}

/// Inverse of an affine column-major matrix, or `None` when singular.
fn inverse_affine(m: &[f32; 16]) -> Option<[f64; 16]> {
    // a[r][c]: row r, column c of the linear part.
    let a: [[f64; 3]; 3] =
        std::array::from_fn(|r| std::array::from_fn(|c| f64::from(m[c * 4 + r])));
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if !det.is_finite() || det.abs() < 1e-18 {
        return None;
    }
    let mut out = [0.; 16];
    for r in 0..3 {
        for c in 0..3 {
            // Adjugate: inverse[r][c] is the cofactor of a[c][r] over det.
            let (c1, c2) = ((c + 1) % 3, (c + 2) % 3);
            let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
            out[c * 4 + r] = (a[c1][r1] * a[c2][r2] - a[c1][r2] * a[c2][r1]) / det;
        }
    }
    for r in 0..3 {
        out[12 + r] = -(0..3)
            .map(|k| out[k * 4 + r] * f64::from(m[12 + k]))
            .sum::<f64>();
    }
    out[15] = 1.;
    Some(out)
}

fn transform64(m: &[f64; 16], p: [f64; 3], w: f64) -> [f64; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r] * w)
}

/// Slab test; returns the entry parameter (clamped to 0) when the ray meets
/// the box at or before `limit`.
fn ray_box(o: [f64; 3], d: [f64; 3], low: [f64; 3], high: [f64; 3], limit: f64) -> Option<f64> {
    let (mut near, mut far) = (0f64, limit);
    for k in 0..3 {
        if d[k].abs() < 1e-300 {
            if o[k] < low[k] || o[k] > high[k] {
                return None;
            }
            continue;
        }
        let (mut t0, mut t1) = ((low[k] - o[k]) / d[k], (high[k] - o[k]) / d[k]);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        near = near.max(t0);
        far = far.min(t1);
        if near > far {
            return None;
        }
    }
    Some(near)
}

/// Möller–Trumbore, two-sided. Returns the ray parameter.
fn ray_triangle(o: [f64; 3], d: [f64; 3], v: [[f64; 3]; 3]) -> Option<f64> {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (e1, e2) = (sub(v[1], v[0]), sub(v[2], v[0]));
    let p = cross(d, e2);
    let det = dot(e1, p);
    let scale = dot(e1, e1).max(dot(e2, e2)) * dot(d, d).sqrt();
    if det.abs() <= 1e-12 * scale.max(1e-300) {
        return None;
    }
    let s = sub(o, v[0]);
    let u = dot(s, p) / det;
    if !(-1e-9..=1. + 1e-9).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let w = dot(d, q) / det;
    if w < -1e-9 || u + w > 1. + 1e-9 {
        return None;
    }
    let t = dot(e2, q) / det;
    (t >= 0.).then_some(t)
}

/// The nearest node surface the ray from `origin` along `direction` meets.
///
/// `mesh` resolves a node's mesh key ([`crate::VERTEX_FLOATS`] floats per
/// vertex, triangle list); nodes whose mesh it cannot resolve are skipped, as
/// are nodes `skip` rejects. Only nodes whose box the ray reaches before the
/// best hit so far have their triangles tested. Only intersections whose
/// distance lies in `range` [near, far] count, so surfaces the camera clips
/// are transparent and the search continues behind them.
pub fn pick<'a>(
    nodes: &[Node],
    origin: [f64; 3],
    direction: [f64; 3],
    range: [f64; 2],
    mesh: impl Fn(&str) -> Option<MeshView<'a>>,
    skip: impl Fn(&Node) -> bool,
) -> Option<Hit> {
    let length = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
    let [near, far] = range;
    if !length.is_finite()
        || length == 0.
        || origin.iter().any(|v| !v.is_finite())
        || near.is_nan()
        || far.is_nan()
        || near > far
    {
        return None;
    }
    let d = direction.map(|v| v / length);
    let mut best: Option<Hit> = None;
    for (index, node) in nodes.iter().enumerate() {
        if skip(node) {
            continue;
        }
        let Some(MeshView {
            vertices,
            bounds: (low, high),
        }) = mesh(&node.mesh)
        else {
            continue;
        };
        let Some(inv) = inverse_affine(&node.matrix) else {
            continue;
        };
        // Local ray with an unnormalized direction keeps `t` in world metres.
        let (lo, ld) = (transform64(&inv, origin, 1.), transform64(&inv, d, 0.));
        let limit = best.map_or(far, |b| b.distance);
        if ray_box(lo, ld, low, high, limit).is_none() {
            continue;
        }
        let mut nearest = f64::INFINITY;
        for tri in vertices.as_chunks::<{ crate::VERTEX_FLOATS * 3 }>().0 {
            let v = std::array::from_fn(|i| {
                std::array::from_fn(|k| f64::from(tri[i * crate::VERTEX_FLOATS + k]))
            });
            if let Some(t) = ray_triangle(lo, ld, v).filter(|t| *t >= near) {
                nearest = nearest.min(t);
            }
        }
        if nearest < limit {
            best = Some(Hit {
                node: index,
                distance: nearest,
                point: std::array::from_fn(|k| origin[k] + d[k] * nearest),
            });
        }
    }
    best
}

/// World bounds of `nodes` (their cached mesh boxes transformed), or `None`
/// when no node's mesh resolves. Constant work per node.
pub fn world_bounds<'a>(
    nodes: impl IntoIterator<Item = &'a Node>,
    bounds: impl Fn(&str) -> Option<Bounds>,
) -> Option<Bounds> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for node in nodes {
        let Some((a, b)) = bounds(&node.mesh) else {
            continue;
        };
        for corner in 0..8 {
            let p = [
                if corner & 1 == 0 { a[0] } else { b[0] },
                if corner & 2 == 0 { a[1] } else { b[1] },
                if corner & 4 == 0 { a[2] } else { b[2] },
            ];
            let w = transform(&node.matrix, p, 1.);
            for k in 0..3 {
                low[k] = low[k].min(w[k]);
                high[k] = high[k].max(w[k]);
            }
        }
    }
    low[0].is_finite().then_some((low, high))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [f64; 2] = [0., f64::INFINITY];

    fn cube_view(cube: &[f32]) -> MeshView<'_> {
        MeshView {
            vertices: cube,
            bounds: mesh_bounds(cube).unwrap(),
        }
    }

    fn cube_node(id: &str, at: [f32; 3], scale: [f32; 3]) -> Node {
        let mut matrix = [0f32; 16];
        matrix[0] = scale[0];
        matrix[5] = scale[1];
        matrix[10] = scale[2];
        matrix[12..15].copy_from_slice(&at);
        matrix[15] = 1.;
        Node {
            id: id.into(),
            mesh: "builtin:cube".into(),
            matrix,
            normal_matrix: [0.; 9],
            color: [1.; 4],
            transparent: false,
        }
    }

    #[test]
    fn node_ids_map_to_their_entities() {
        let id = Uuid::from_u128(7);
        assert_eq!(
            node_entity(&format!("wall:{id}:0:1")),
            Some((EntityKind::Wall, id))
        );
        for prefix in ["frame", "door", "window", "lintel", "recess"] {
            assert_eq!(
                node_entity(&format!("{prefix}:{id}:0")),
                Some((EntityKind::Opening, id))
            );
        }
        assert_eq!(node_entity(&id.to_string()), Some((EntityKind::Asset, id)));
        assert_eq!(
            node_entity(&format!("floor:{id}:3")),
            Some((EntityKind::Structure, id))
        );
        assert_eq!(node_entity("geometry:x"), None);
        assert_eq!(node_entity("nonsense"), None);
    }

    #[test]
    fn the_nearest_surface_wins_and_skipped_nodes_are_transparent_to_rays() {
        let cube = crate::cube();
        let view = cube_view(&cube);
        let mesh = |k: &str| (k == "builtin:cube").then_some(view);
        let nodes = [
            cube_node("far", [0., 0., -10.], [2., 2., 2.]),
            cube_node("near", [0., 0., -5.], [1., 3., 1.]),
        ];
        let hit = pick(&nodes, [0., 1., 0.], [0., 0., -1.], ALL, mesh, |_| false).unwrap();
        assert_eq!(hit.node, 1);
        assert!((hit.distance - 4.5).abs() < 1e-9);
        assert!((hit.point[2] + 4.5).abs() < 1e-9);
        let hit = pick(&nodes, [0., 1., 0.], [0., 0., -2.], ALL, mesh, |n| {
            n.id == "near"
        })
        .unwrap();
        assert_eq!(hit.node, 0);
        assert!((hit.distance - 9.).abs() < 1e-9, "direction is normalized");
        assert!(pick(&nodes, [5., 1., 0.], [0., 0., -1.], ALL, mesh, |_| false).is_none());
        assert!(pick(&nodes, [0., 1., 0.], [0., 0., 0.], ALL, mesh, |_| false).is_none());
        assert!(
            pick(&nodes, [f64::NAN, 1., 0.], [0., 0., -1.], ALL, mesh, |_| {
                false
            })
            .is_none()
        );
    }

    #[test]
    fn rotated_and_scaled_nodes_use_their_exact_triangles() {
        let cube = crate::cube();
        let view = cube_view(&cube);
        let mesh = |_: &str| Some(view);
        // A 4 m × 0.2 m wall turned 90° about Y: long along Z, thin along X.
        let (s, c) = (1f32, 0f32);
        let mut node = cube_node("wall", [0., 0., -6.], [1., 1., 1.]);
        node.matrix[..12].copy_from_slice(&[
            c * 4.,
            0.,
            -s * 4.,
            0.,
            0.,
            2.5,
            0.,
            0.,
            s * 0.2,
            0.,
            c * 0.2,
            0.,
        ]);
        let nodes = [node];
        assert!(pick(&nodes, [0.5, 1., 0.], [0., 0., -1.], ALL, mesh, |_| false).is_none());
        let hit = pick(&nodes, [0.05, 1., 0.], [0., 0., -1.], ALL, mesh, |_| false).unwrap();
        assert!((hit.distance - 4.).abs() < 1e-9);
        let (low, high) = world_bounds(&nodes, |_| Some(view.bounds)).unwrap();
        for (a, b) in low.into_iter().zip([-0.1, 0., -8.]) {
            assert!((a - b).abs() < 1e-6);
        }
        for (a, b) in high.into_iter().zip([0.1, 2.5, -4.]) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn the_inverse_round_trips_a_general_pose() {
        let (m, _) =
            crate::asset_pose::matrices([1.5, 0.3, -2.], [0.8, 1.2, 0.5], [10., 25., 70.], 1.3);
        let inv = inverse_affine(&m).unwrap();
        for p in [[0.3, -0.2, 0.9], [-1., 2., 0.], [0.; 3]] {
            let back = transform64(&inv, transform(&m, p, 1.), 1.);
            for k in 0..3 {
                assert!((back[k] - p[k]).abs() < 1e-5, "{back:?} vs {p:?}");
            }
        }
    }

    #[test]
    fn clipped_surfaces_are_transparent_and_the_search_continues_behind() {
        let cube = crate::cube();
        let view = cube_view(&cube);
        let nodes = [
            cube_node("near", [0., 0., -0.5], [0.2, 2., 0.01]),
            cube_node("far", [0., 0., -5.], [1., 2., 1.]),
        ];
        let hit = |range| {
            pick(
                &nodes,
                [0., 1., 0.],
                [0., 0., -1.],
                range,
                |_| Some(view),
                |_| false,
            )
        };
        assert_eq!(hit(ALL).unwrap().node, 0);
        // A near plane at 0.6 m clips the thin panel; the far box shows.
        let behind = hit([0.6, f64::INFINITY]).unwrap();
        assert_eq!(behind.node, 1);
        assert!((behind.distance - 4.5).abs() < 1e-9);
        // A far plane before the box hides it as well.
        assert!(hit([0.6, 4.]).is_none());
        assert!(hit([1., 0.5]).is_none(), "an empty range never hits");
    }

    #[test]
    fn singular_matrices_never_hit() {
        let cube = crate::cube();
        let nodes = [cube_node("flat", [0., 0., -5.], [1., 0., 1.])];
        assert!(pick(
            &nodes,
            [0., 0., 0.],
            [0., 0., -1.],
            ALL,
            |_| Some(cube_view(&cube)),
            |_| false
        )
        .is_none());
    }
}
