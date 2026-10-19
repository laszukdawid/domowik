//! The integer half-edge graph, maintained edge by edge.
//!
//! # What it holds
//!
//! Vertices are hot-cell centres. An EDGE joins two cells and carries the net
//! weight of every route piece between them, in its `lo → hi` direction; it
//! exists while that weight is non-zero (back-to-back ring edges cancel, as in
//! the full builder). Half-edge `2e` runs `lo → hi`, `2e + 1` is its twin.
//! Each vertex keeps its outgoing half-edges in counter-clockwise order.
//!
//! A FACE is one cycle of the `next` rule (at the destination, the first
//! outgoing half-edge clockwise from the way back), so every half-edge has
//! exactly one face on its left. Faces store their cycle explicitly, which is
//! what lets an update discard a face without walking the OLD structure.
//!
//! Each face carries its WINDING NUMBER. Across a half-edge `h` from its left
//! face to its right face the winding drops by `weight(h)`; occupied means
//! winding > 0.
//!
//! # How an update stays local
//!
//! 1. Only vertices where an edge APPEARED or VANISHED change their rotation,
//!    so only faces with a half-edge ARRIVING at such a vertex can change
//!    their cycle. Those faces are dropped and re-traced; every other face
//!    keeps its cycle and its id.
//! 2. Windings of kept faces change only where the batch's net weight change
//!    `Δ` winds around them. `Δ` is a sum of closed cycles (whole routes are
//!    withdrawn and re-added), so it is zero outside the bounding box of the
//!    changed edges; only faces with an edge in that box are probed, and each
//!    probe sums `Δ` alone, not the level.
//! 3. A re-traced face takes its winding from a kept neighbour across a
//!    shared half-edge. A group of new faces with no kept neighbour — a whole
//!    new connected piece — is located by casting one ray against the edge
//!    index.
//!
//! The probe point of a face is the midpoint of one of its half-edges pushed
//! an infinitesimal `ε` to the left (into the face), and the ray rule is
//! decided exactly on doubled integer coordinates with that `ε` carried
//! symbolically. See [`ray_crossing`].

use super::index::{BucketIndex, Cover, FINE};
use crate::overlay::{angle_cmp, double_area, Cell, FastMap};

pub(super) type He = u32;
pub(super) type FaceId = u32;
const NONE: u32 = u32::MAX;

#[derive(Debug, Clone, Copy)]
struct Edge {
    lo: Cell,
    hi: Cell,
    weight: i32,
}

#[derive(Debug, Clone)]
pub(super) struct Face {
    /// The cycle, in `next` order.
    pub(super) hes: Vec<He>,
    /// Twice the signed area: positive for a bounded face, non-positive for
    /// the outer face of a connected piece.
    pub(super) area2: i128,
    pub(super) winding: i32,
}

/// What one update changed, for the ring layer and for diagnostics.
#[derive(Debug, Default, Clone)]
pub(super) struct DcelChange {
    /// Vertices whose rotation changed.
    pub(super) rot_vertices: Vec<Cell>,
    /// Half-edges that no longer exist.
    pub(super) removed_hes: Vec<He>,
    /// Faces created by re-tracing.
    pub(super) new_faces: Vec<FaceId>,
    /// Kept faces whose winding changed.
    pub(super) rewound: Vec<FaceId>,
    /// Edges whose net weight changed (including appear / vanish).
    pub(super) changed_edges: usize,
    /// Faces dropped for re-tracing.
    pub(super) dropped_faces: usize,
    /// New-face groups that needed a ray cast against the whole index.
    pub(super) ray_casts: usize,
}

#[derive(Debug, Clone)]
pub(super) struct Dcel {
    edges: Vec<Option<Edge>>,
    free_edges: Vec<u32>,
    edge_of: FastMap<(Cell, Cell), u32>,
    out: FastMap<Cell, Vec<He>>,
    face_of: Vec<FaceId>,
    faces: Vec<Option<Face>>,
    free_faces: Vec<FaceId>,
    index: BucketIndex<u32>,
}

fn edge_cover(e: &Edge) -> Cover {
    let p = |c: Cell| domowik_format::Point::new(c.x as f64, c.y as f64);
    Cover::Segment(p(e.lo), p(e.hi))
}

impl Dcel {
    pub(super) fn new() -> Self {
        Self {
            edges: Vec::new(),
            free_edges: Vec::new(),
            edge_of: FastMap::default(),
            out: FastMap::default(),
            face_of: Vec::new(),
            faces: Vec::new(),
            free_faces: Vec::new(),
            index: BucketIndex::new(FINE),
        }
    }

    // --- accessors -------------------------------------------------------

    fn edge(&self, he: He) -> &Edge {
        self.edges[(he / 2) as usize]
            .as_ref()
            .expect("dcel: dead half-edge")
    }

    pub(super) fn alive(&self, he: He) -> bool {
        matches!(self.edges.get((he / 2) as usize), Some(Some(_)))
    }

    pub(super) fn origin(&self, he: He) -> Cell {
        let e = self.edge(he);
        if he.is_multiple_of(2) {
            e.lo
        } else {
            e.hi
        }
    }

    /// The live half-edge running `from → to`, if that edge exists.
    pub(super) fn half_edge(&self, from: Cell, to: Cell) -> Option<He> {
        if from < to {
            self.edge_of.get(&(from, to)).map(|&e| 2 * e)
        } else {
            self.edge_of.get(&(to, from)).map(|&e| 2 * e + 1)
        }
    }

    pub(super) fn dest(&self, he: He) -> Cell {
        self.origin(he ^ 1)
    }

    /// Net weight in the half-edge's own direction.
    pub(super) fn weight(&self, he: He) -> i32 {
        let w = self.edge(he).weight;
        if he.is_multiple_of(2) {
            w
        } else {
            -w
        }
    }

    fn direction(&self, he: He) -> (i64, i64) {
        let (a, b) = (self.origin(he), self.dest(he));
        (b.x - a.x, b.y - a.y)
    }

    /// Outgoing half-edges of `v`, counter-clockwise.
    pub(super) fn outgoing(&self, v: Cell) -> &[He] {
        self.out.get(&v).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Position of `he` in its origin's rotation.
    pub(super) fn position(&self, he: He) -> usize {
        self.outgoing(self.origin(he))
            .iter()
            .position(|&x| x == he)
            .expect("dcel: half-edge missing from its origin's rotation")
    }

    /// The face-cycle successor of `he`.
    pub(super) fn next(&self, he: He) -> He {
        let list = self.outgoing(self.dest(he));
        let i = self.position(he ^ 1);
        list[(i + list.len() - 1) % list.len()]
    }

    pub(super) fn face_of(&self, he: He) -> FaceId {
        self.face_of[he as usize]
    }

    pub(super) fn face(&self, f: FaceId) -> &Face {
        self.faces[f as usize].as_ref().expect("dcel: dead face")
    }

    pub(super) fn winding_of(&self, he: He) -> i32 {
        self.face(self.face_of(he)).winding
    }

    pub(super) fn faces(&self) -> impl Iterator<Item = (FaceId, &Face)> {
        self.faces
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.as_ref().map(|f| (i as FaceId, f)))
    }

    pub(super) fn half_edges(&self) -> impl Iterator<Item = He> + '_ {
        self.edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_some())
            .flat_map(|(i, _)| [2 * i as He, 2 * i as He + 1])
    }

    pub(super) fn edge_count(&self) -> usize {
        self.edge_of.len()
    }

    // --- update ----------------------------------------------------------

    /// Applies a batch of net edge-weight changes. Freed edge and face ids are
    /// not reused until the batch ends, so ids recorded in `DcelChange` never
    /// alias a new object.
    pub(super) fn apply(&mut self, delta: &FastMap<(Cell, Cell), i32>) -> DcelChange {
        let mut change = DcelChange::default();
        let mut changes: Vec<((Cell, Cell), i32, i32)> = delta
            .iter()
            .filter(|(_, &d)| d != 0)
            .map(|(&k, &d)| {
                let old = self.edge_of.get(&k).map_or(0, |&e| {
                    self.edges[e as usize]
                        .as_ref()
                        .expect("indexed edge")
                        .weight
                });
                (k, old, old + d)
            })
            .collect();
        changes.sort_unstable_by_key(|c| c.0);
        change.changed_edges = changes.len();
        if changes.is_empty() {
            return change;
        }

        // 1. Faces whose cycle can change: any with a half-edge arriving at a
        //    vertex whose rotation changes. Read from the OLD structure.
        let mut rot: Vec<Cell> = changes
            .iter()
            .filter(|(_, old, new)| (*old == 0) != (*new == 0))
            .flat_map(|((lo, hi), _, _)| [*lo, *hi])
            .collect();
        rot.sort_unstable();
        rot.dedup();
        let mut dropped: Vec<FaceId> = Vec::new();
        for v in &rot {
            for &he in self.outgoing(*v) {
                let f = self.face_of[(he ^ 1) as usize];
                if f != NONE {
                    dropped.push(f);
                }
            }
        }
        dropped.sort_unstable();
        dropped.dedup();
        let mut seeds: Vec<He> = Vec::new();
        for &f in &dropped {
            let face = self.faces[f as usize].take().expect("dropped face is live");
            for &he in &face.hes {
                self.face_of[he as usize] = NONE;
            }
            seeds.extend(face.hes);
        }
        change.dropped_faces = dropped.len();

        // 2. Apply the edge changes.
        let mut freed_edges = Vec::new();
        for &((lo, hi), old, new) in &changes {
            match (old != 0, new != 0) {
                (true, false) => {
                    let e = self.edge_of.remove(&(lo, hi)).expect("indexed edge");
                    let edge = self.edges[e as usize].expect("live edge");
                    self.index.remove(e, edge_cover(&edge));
                    for (v, he) in [(lo, 2 * e), (hi, 2 * e + 1)] {
                        let list = self.out.get_mut(&v).expect("rotation of a live edge");
                        list.retain(|&x| x != he);
                        if list.is_empty() {
                            self.out.remove(&v);
                        }
                        change.removed_hes.push(he);
                    }
                    self.edges[e as usize] = None;
                    freed_edges.push(e);
                }
                (false, true) => {
                    let e = match self.free_edges.pop() {
                        Some(e) => e,
                        None => {
                            self.edges.push(None);
                            self.face_of.extend([NONE, NONE]);
                            (self.edges.len() - 1) as u32
                        }
                    };
                    let edge = Edge {
                        lo,
                        hi,
                        weight: new,
                    };
                    self.edges[e as usize] = Some(edge);
                    self.edge_of.insert((lo, hi), e);
                    self.index.insert(e, edge_cover(&edge));
                    for (v, he) in [(lo, 2 * e), (hi, 2 * e + 1)] {
                        let d = self.direction(he);
                        let edges = &self.edges;
                        let dir_of = |h: He| {
                            let ed = edges[(h / 2) as usize].as_ref().expect("live");
                            let (a, b) = if h.is_multiple_of(2) {
                                (ed.lo, ed.hi)
                            } else {
                                (ed.hi, ed.lo)
                            };
                            (b.x - a.x, b.y - a.y)
                        };
                        let list = self.out.entry(v).or_default();
                        let at = list.partition_point(|&x| angle_cmp(dir_of(x), d).is_lt());
                        list.insert(at, he);
                        seeds.push(he);
                    }
                }
                (true, true) => {
                    let e = self.edge_of[&(lo, hi)];
                    self.edges[e as usize].as_mut().expect("live edge").weight = new;
                }
                (false, false) => unreachable!("zero deltas are filtered"),
            }
        }
        change.rot_vertices = rot;

        // 3. Re-trace.
        let mut is_new = FastMap::<FaceId, ()>::default();
        for he in seeds {
            if self.alive(he) && self.face_of[he as usize] == NONE {
                let f = self.trace(he);
                is_new.insert(f, ());
                change.new_faces.push(f);
            }
        }

        // 4. Windings of kept faces.
        let delta_edges: Vec<(Cell, Cell, i32)> = changes
            .iter()
            .map(|&((lo, hi), old, new)| (lo, hi, new - old))
            .collect();
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for &(lo, hi, _) in &delta_edges {
            for c in [lo, hi] {
                bbox[0] = bbox[0].min(c.x as f64);
                bbox[1] = bbox[1].min(c.y as f64);
                bbox[2] = bbox[2].max(c.x as f64);
                bbox[3] = bbox[3].max(c.y as f64);
            }
        }
        let mut probed = FastMap::<FaceId, ()>::default();
        let mut near_edges = Vec::new();
        self.index.query(Cover::Box(bbox), |e| near_edges.push(e));
        for e in near_edges {
            for he in [2 * e, 2 * e + 1] {
                let f = self.face_of[he as usize];
                if f == NONE || is_new.contains_key(&f) || probed.insert(f, ()).is_some() {
                    continue;
                }
                let (base, dir) = self.probe(f);
                let shift: i32 = delta_edges
                    .iter()
                    .map(|&(lo, hi, d)| d * ray_crossing(lo, hi, base, dir))
                    .sum();
                if shift != 0 {
                    self.faces[f as usize].as_mut().expect("live").winding += shift;
                    change.rewound.push(f);
                }
            }
        }

        // 5. Windings of new faces: from kept neighbours, else by ray cast.
        let mut assigned = FastMap::<FaceId, ()>::default();
        let mut queue: Vec<FaceId> = Vec::new();
        for &f in &change.new_faces {
            let hes = self.face(f).hes.clone();
            for he in hes {
                let g = self.face_of[(he ^ 1) as usize];
                if !is_new.contains_key(&g) {
                    let w = self.face(g).winding + self.weight(he);
                    self.faces[f as usize].as_mut().expect("live").winding = w;
                    assigned.insert(f, ());
                    queue.push(f);
                    break;
                }
            }
        }
        let new_faces = change.new_faces.clone();
        let mut cursor = 0;
        loop {
            while let Some(f) = queue.pop() {
                let hes = self.face(f).hes.clone();
                let wf = self.face(f).winding;
                for he in hes {
                    let g = self.face_of[(he ^ 1) as usize];
                    if is_new.contains_key(&g) && assigned.insert(g, ()).is_none() {
                        self.faces[g as usize].as_mut().expect("live").winding =
                            wf - self.weight(he);
                        queue.push(g);
                    }
                }
            }
            while cursor < new_faces.len() && assigned.contains_key(&new_faces[cursor]) {
                cursor += 1;
            }
            let Some(&f) = new_faces.get(cursor) else {
                break;
            };
            let w = self.absolute_winding(f);
            self.faces[f as usize].as_mut().expect("live").winding = w;
            assigned.insert(f, ());
            queue.push(f);
            change.ray_casts += 1;
        }

        self.free_edges.extend(freed_edges);
        self.free_faces.extend(dropped);
        change
    }

    fn trace(&mut self, start: He) -> FaceId {
        let f = match self.free_faces.pop() {
            Some(f) => f,
            None => {
                self.faces.push(None);
                (self.faces.len() - 1) as FaceId
            }
        };
        let limit = 2 * self.edge_of.len() + 1;
        let mut hes = Vec::new();
        let mut cells = Vec::new();
        let mut h = start;
        loop {
            assert!(
                self.face_of[h as usize] == NONE,
                "dcel: face walk from {start} reached half-edge {h}, already in face {}",
                self.face_of[h as usize]
            );
            self.face_of[h as usize] = f;
            hes.push(h);
            cells.push(self.origin(h));
            h = self.next(h);
            if h == start {
                break;
            }
            assert!(
                hes.len() <= limit,
                "dcel: face walk from {start} did not close"
            );
        }
        let area2 = double_area(&cells);
        self.faces[f as usize] = Some(Face {
            hes,
            area2,
            winding: 0,
        });
        f
    }

    /// A point just inside face `f`: the midpoint of its first half-edge,
    /// shifted `ε` along the left normal. Doubled coordinates.
    pub(super) fn probe(&self, f: FaceId) -> ((i128, i128), (i128, i128)) {
        let he = self.face(f).hes[0];
        let (u, v) = (self.origin(he), self.dest(he));
        let base = ((u.x + v.x) as i128, (u.y + v.y) as i128);
        let dir = (-((v.y - u.y) as i128), (v.x - u.x) as i128);
        (base, dir)
    }

    /// The winding of face `f` from every edge, by one rightward ray cast.
    /// Cost is the edges in the probe's bucket rows to its right.
    pub(super) fn absolute_winding(&self, f: FaceId) -> i32 {
        let (base, dir) = self.probe(f);
        let (x, y) = (base.0 as f64 / 2.0, base.1 as f64 / 2.0);
        let mut seen = FastMap::<u32, ()>::default();
        let mut total = 0;
        self.index.ray_right(x - 1.0, y - 1.0, y + 1.0, |e| {
            if seen.insert(e, ()).is_some() {
                return;
            }
            let edge = self.edges[e as usize]
                .as_ref()
                .expect("indexed edge is live");
            total += edge.weight * ray_crossing(edge.lo, edge.hi, base, dir);
        });
        total
    }

    /// Checks every structural invariant against the current state. Panics
    /// with a description on the first violation. For tests and debugging;
    /// cost is proportional to the whole graph.
    pub(super) fn validate(&self) {
        for (v, list) in &self.out {
            for w in list.windows(2) {
                assert!(
                    angle_cmp(self.direction(w[0]), self.direction(w[1])).is_lt(),
                    "dcel: rotation at {v:?} is not strictly counter-clockwise"
                );
            }
            for &he in list {
                assert_eq!(self.origin(he), *v, "dcel: half-edge in the wrong rotation");
            }
        }
        for he in self.half_edges() {
            let f = self.face_of(he);
            assert!(f != NONE, "dcel: half-edge {he} has no face");
            let face = self.face(f);
            let i = face.hes.iter().position(|&x| x == he).expect("in its face");
            let n = face.hes[(i + 1) % face.hes.len()];
            assert_eq!(
                self.next(he),
                n,
                "dcel: face {f} cycle disagrees with next()"
            );
            let g = self.face_of(he ^ 1);
            assert_eq!(
                face.winding - self.face(g).winding,
                self.weight(he),
                "dcel: winding jump across half-edge {he} ({:?} -> {:?})",
                self.origin(he),
                self.dest(he)
            );
        }
        for (f, face) in self.faces() {
            let cells: Vec<Cell> = face.hes.iter().map(|&h| self.origin(h)).collect();
            assert_eq!(
                double_area(&cells),
                face.area2,
                "dcel: face {f} has a stale area"
            );
            assert_eq!(
                self.absolute_winding(f),
                face.winding,
                "dcel: face {f} winding disagrees with a ray cast"
            );
        }
    }
}

/// The contribution of the directed edge `a → b` to the winding number of
/// the point `p = base/2 + ε·dir` (doubled coordinates, `ε → 0⁺`), by the
/// half-open crossing rule: `+1` for an upward edge with `p` on its left,
/// `−1` for a downward edge with `p` on its right, otherwise `0`.
///
/// `ε` is decided lexicographically: `p.y` compares as `(base.y, dir.y)`, and
/// the side test falls back to the first-order term when `p`'s base lies on
/// the edge's line. When both vanish `p` cannot be inside the edge's y-span
/// (it would lie on the edge), so the edge does not cross.
pub(super) fn ray_crossing(a: Cell, b: Cell, base: (i128, i128), dir: (i128, i128)) -> i32 {
    let (ax, ay) = (2 * a.x as i128, 2 * a.y as i128);
    let (bx, by) = (2 * b.x as i128, 2 * b.y as i128);
    let below_or_at = |y: i128| y < base.1 || (y == base.1 && dir.1 >= 0);
    let side = || {
        let c0 = (bx - ax) * (base.1 - ay) - (by - ay) * (base.0 - ax);
        if c0 != 0 {
            c0.signum()
        } else {
            ((bx - ax) * dir.1 - (by - ay) * dir.0).signum()
        }
    };
    if below_or_at(ay) {
        if !below_or_at(by) && side() > 0 {
            return 1;
        }
    } else if below_or_at(by) && side() < 0 {
        return -1;
    }
    0
}
