//! Join-connected components and the R5.1.9 recomputation closure.
//!
//! # What this is for
//!
//! BEHAVIOR-SPEC R3.4.8 forbids an editor from rebuilding the whole document
//! after an edit, and R5.1.9 says exactly how much it MAY rebuild. This module
//! computes that permitted scope so it can be checked rather than asserted:
//! issue 0032 measures a gesture, and the number it needs is "how much of the
//! level was this edit ALLOWED to touch".
//!
//! # The rule, restated
//!
//! Partition a level's walls into components connected by JOINS. Each owns its
//! R3.2.1 occupied region. Two DISTINCT components are LINKED when their
//! regions stand in either relation, evaluated over the union of the pre-edit
//! and post-edit states:
//!
//! - **containment** — one region lies within the other's rings (an island
//!   loop standing in a courtyard); or
//! - **positive-area intersection** — the regions overlap with area even
//!   though no join connects them (`wall-area-overlap`, R3.1.2).
//!
//! Point contact is neither: two clusters meeting at a single vertex intersect
//! in zero area and are NOT linked, even though they may jointly bound a face.
//! That asymmetry is deliberate in the specification and reproduced here.
//!
//! Starting from the components holding the touched walls, take the transitive
//! closure under both relations. Exactly the faces whose closures meet an
//! occupied region of the closed set may recompute.
//!
//! # Identity across the two states
//!
//! The relation is evaluated over BOTH states, so components have to be
//! identified across an edit. A wall id is the only thing that survives one:
//! a pre-state component and a post-state component are the same node when
//! they share at least one wall id. Positional wall indices are not usable —
//! deleting a wall renumbers every wall after it.
//!
//! # Numerical behaviour
//!
//! Coordinates are millimetres. The positive-area test compares
//! `area(A) + area(B)` against `area(A ∪ B)` and is decided EXACTLY, by
//! [`crate::exact_area::positive_area`]. There is no tolerance floor: ED-0103
//! N-5 retired the `1e-12 × separate` relative term, whose stated reason —
//! shoelace error near the coordinate bound — ED-0100's translation fix had
//! already removed, and which by then only under-included, discarding a real
//! 0.5 mm² overlap between kilometre-scale regions.
//!
//! Regions that merely touch cancel to exactly zero and are not linked. What
//! exact arithmetic cannot settle is whether the RINGS are the true union:
//! see [`Link::Overlap`]'s note on an uncertain overlap.

use crate::arrangement::{classify_point, ring_contains_ring, Containment, WELD_MM};
use crate::boolean::union;
use crate::exact_area::{positive_area, PositiveArea};
use crate::polygon::Polygon;
use crate::walls::{join_patch, JoinSpec, WallSolid};
use domowik_format::Point;
use std::collections::{BTreeSet, HashMap};
use uuid::Uuid;

/// Why two components are linked (R5.1.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// One region lies within the other's rings.
    Containment,
    /// The regions intersect with positive area, unjoined.
    ///
    /// # An uncertain overlap is reported here, deliberately
    ///
    /// Exact arithmetic settles whether the RINGS SUPPLIED overlap; it cannot
    /// settle whether those rings are the true union. Two components that
    /// genuinely overlap construct their crossing points, and a crossing cut
    /// is `lerp(a, b, t)` with `t = num/denom` where `denom` carries a factor
    /// `sin θ`, so its displacement grows without limit as the segments
    /// approach parallel. [`crate::exact_area::positive_area`] therefore
    /// returns `Uncertain` for them rather than certifying from a constant.
    ///
    /// Uncertain is resolved as a link. This is OVER-INCLUSION and R5.1.9 says
    /// EXACTLY which faces recompute, so it is a deviation and is recorded as
    /// one rather than described as conformance. It is chosen because the two
    /// errors are not symmetric: recomputing a component that did not need it
    /// costs work and produces identical geometry, while skipping one that did
    /// leaves stale geometry in the document.
    ///
    /// The verdict is right either way — an overlapping pair should be linked.
    /// What `Uncertain` gives up is the ability to say WHY, not the answer.
    /// Components that merely TOUCH construct nothing, return an exact `No`,
    /// and are correctly not linked.
    Overlap,
}

/// One level's walls partitioned into join-connected components.
///
/// Wall and join positions are indices into the slices passed to
/// [`components_of`]. A join naming a wall index that does not exist is
/// ignored, matching [`join_patch`], so a partially applied edit does not
/// panic here.
#[derive(Debug, Clone)]
pub struct Components {
    of_wall: Vec<usize>,
    walls: Vec<Vec<usize>>,
    joins: Vec<Vec<usize>>,
    rings: Vec<Vec<Vec<Point>>>,
    regions: Vec<Vec<Polygon>>,
    areas: Vec<f64>,
    bounds: Vec<Option<Bounds>>,
}

impl Components {
    /// Number of components.
    pub fn len(&self) -> usize {
        self.walls.len()
    }

    /// True when the level holds no walls.
    pub fn is_empty(&self) -> bool {
        self.walls.is_empty()
    }

    /// The component a wall belongs to, or `None` when the index is unknown.
    pub fn of_wall(&self, wall: usize) -> Option<usize> {
        self.of_wall.get(wall).copied()
    }

    /// Wall indices of one component, ascending.
    pub fn walls(&self, component: usize) -> &[usize] {
        &self.walls[component]
    }

    /// Join indices of one component, ascending.
    pub fn joins(&self, component: usize) -> &[usize] {
        &self.joins[component]
    }

    /// The component's occupied region (R3.2.1), canonically ordered.
    pub fn region(&self, component: usize) -> &[Polygon] {
        &self.regions[component]
    }

    /// The component's occupied area in mm².
    pub fn area_mm2(&self, component: usize) -> f64 {
        self.areas[component]
    }
}

/// Partitions `walls` into join-connected components and computes each one's
/// occupied region.
///
/// All walls must belong to the same level: R3.0.6 and R5.1.1 make levels
/// non-interacting, and mixing them would link geometry that never touches.
///
/// Cost is one region union per component, not one for the level, which is the
/// whole point — a component's region is what an edit inside it invalidates.
pub fn components_of(walls: &[WallSolid], joins: &[JoinSpec]) -> Components {
    let mut uf = UnionFind::new(walls.len());
    for join in joins {
        let mut members = join.members.iter().filter(|m| m.wall < walls.len());
        if let Some(first) = members.next() {
            for m in members {
                uf.union(first.wall, m.wall);
            }
        }
    }

    // Roots are renumbered to a dense 0..n in ascending order of their least
    // wall index, so component numbering is deterministic and independent of
    // union-find internals.
    let mut order: Vec<usize> = Vec::new();
    let mut dense: HashMap<usize, usize> = HashMap::new();
    let mut of_wall = vec![0usize; walls.len()];
    for (w, slot) in of_wall.iter_mut().enumerate() {
        let root = uf.find(w);
        *slot = *dense.entry(root).or_insert_with(|| {
            order.push(root);
            order.len() - 1
        });
    }

    let n = order.len();
    let mut wall_sets = vec![Vec::new(); n];
    for (w, &c) in of_wall.iter().enumerate() {
        wall_sets[c].push(w);
    }

    let mut join_sets = vec![Vec::new(); n];
    for (j, join) in joins.iter().enumerate() {
        if let Some(m) = join.members.iter().find(|m| m.wall < walls.len()) {
            join_sets[of_wall[m.wall]].push(j);
        }
    }

    let mut rings = Vec::with_capacity(n);
    let mut regions = Vec::with_capacity(n);
    let mut areas = Vec::with_capacity(n);
    let mut bounds = Vec::with_capacity(n);
    for c in 0..n {
        let mut component_rings: Vec<Vec<Point>> = Vec::new();
        for &w in &wall_sets[c] {
            if let Some(rect) = walls[w].rectangle() {
                component_rings.push(rect.to_vec());
            }
        }
        for &j in &join_sets[c] {
            let patch = join_patch(walls, &joins[j]);
            if patch.len() >= 3 {
                component_rings.push(patch);
            }
        }
        let region = union(&component_rings);
        areas.push(region.iter().map(|p| p.area_mm2()).sum());
        bounds.push(Bounds::of_polygons(&region));
        rings.push(component_rings);
        regions.push(region);
    }

    Components {
        of_wall,
        walls: wall_sets,
        joins: join_sets,
        rings,
        regions,
        areas,
        bounds,
    }
}

/// The R5.1.9 relation between two components of the SAME state, or `None`
/// when they are unlinked.
///
/// Returns [`Link::Containment`] in preference to [`Link::Overlap`] when both
/// hold, which happens only for degenerate input; the distinction is reported
/// because the two relations exist for different reasons and a caller
/// diagnosing an unexpected closure wants to know which one fired.
pub fn link(components: &Components, a: usize, b: usize) -> Option<Link> {
    let (Some(ba), Some(bb)) = (components.bounds[a], components.bounds[b]) else {
        return None;
    };
    if !ba.overlaps(&bb) {
        return None;
    }

    let (ra, rb) = (&components.regions[a], &components.regions[b]);
    for pa in ra {
        for pb in rb {
            if ring_contains_ring(&pa.outer, &pb.outer) || ring_contains_ring(&pb.outer, &pa.outer)
            {
                return Some(Link::Containment);
            }
        }
    }

    let mut both: Vec<Vec<Point>> = components.rings[a].clone();
    both.extend(components.rings[b].iter().cloned());
    let separate: Vec<Polygon> = components.regions[a]
        .iter()
        .chain(components.regions[b].iter())
        .cloned()
        .collect();
    match positive_area(&separate, &union(&both)) {
        Some(PositiveArea::Yes) => Some(Link::Overlap),
        Some(PositiveArea::No) => None,
        // Deliberate over-inclusion, in the one direction that cannot produce
        // wrong geometry. See `Link::Overlap`.
        Some(PositiveArea::Uncertain) => Some(Link::Overlap),
        // A non-finite coordinate reached the region builder. Linking is the
        // conservative answer here too; the document is already invalid and
        // validation, not the closure, is where that gets reported.
        None => Some(Link::Overlap),
    }
}

/// One state of a level, as the closure computation needs to see it.
///
/// `ids` is parallel to `walls`: `ids[i]` is the document identity of
/// `walls[i]`. It is what identifies a component across an edit, since
/// positions renumber whenever a wall is deleted.
#[derive(Debug, Clone, Copy)]
pub struct State<'a> {
    /// Wall solids of one level, in any order.
    pub walls: &'a [WallSolid],
    /// Document ids, parallel to `walls`.
    pub ids: &'a [Uuid],
    /// Joins of the same level, addressing `walls` positionally.
    pub joins: &'a [JoinSpec],
}

/// The proven recomputation scope of one edit (R3.4.8, R5.1.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    /// Every wall id inside the closure, over both states.
    pub walls: BTreeSet<Uuid>,
    /// Components of the pre-edit state inside the closure.
    pub components_pre: BTreeSet<usize>,
    /// Components of the post-edit state inside the closure.
    pub components_post: BTreeSet<usize>,
}

impl Closure {
    /// True when no component is in scope, which means the edit touched no
    /// wall present in either state.
    pub fn is_empty(&self) -> bool {
        self.components_pre.is_empty() && self.components_post.is_empty()
    }
}

/// Computes the R5.1.9 closure for an edit that touched `touched` wall ids.
///
/// `touched` is the set of walls the batch created, modified or deleted. A id
/// absent from both states contributes nothing: a wall created and deleted
/// inside one batch has no region in either state to invalidate.
///
/// The returned scope is what the edit is PERMITTED to recompute. It is not a
/// claim about what any particular implementation does recompute; comparing
/// the two is the locality check issue 0032 owns.
pub fn recomputation_closure(pre: &State, post: &State, touched: &[Uuid]) -> Closure {
    let cpre = components_of(pre.walls, pre.joins);
    let cpost = components_of(post.walls, post.joins);
    closure_from(pre, &cpre, post, &cpost, touched)
}

/// [`recomputation_closure`] against partitions the caller already built.
///
/// Building a partition costs one region union per component, so a caller
/// measuring many gestures against the same pre-state should build it once.
pub fn closure_from(
    pre: &State,
    cpre: &Components,
    post: &State,
    cpost: &Components,
    touched: &[Uuid],
) -> Closure {
    // Nodes 0..cpre.len() are pre-state components; the rest are post-state.
    let offset = cpre.len();
    let total = offset + cpost.len();
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); total];
    let connect = |a: usize, b: usize, adj: &mut Vec<Vec<usize>>| {
        adj[a].push(b);
        adj[b].push(a);
    };

    for a in 0..cpre.len() {
        for b in (a + 1)..cpre.len() {
            if link(cpre, a, b).is_some() {
                connect(a, b, &mut adjacency);
            }
        }
    }
    for a in 0..cpost.len() {
        for b in (a + 1)..cpost.len() {
            if link(cpost, a, b).is_some() {
                connect(offset + a, offset + b, &mut adjacency);
            }
        }
    }

    // A surviving wall ties its pre-state component to its post-state one:
    // that is what "evaluated over the union of both states" means once the
    // two partitions have different shapes.
    let mut post_of_id: HashMap<Uuid, usize> = HashMap::new();
    for (i, id) in post.ids.iter().enumerate() {
        if let Some(c) = cpost.of_wall(i) {
            post_of_id.insert(*id, offset + c);
        }
    }
    let mut pre_of_id: HashMap<Uuid, usize> = HashMap::new();
    for (i, id) in pre.ids.iter().enumerate() {
        if let Some(c) = cpre.of_wall(i) {
            pre_of_id.insert(*id, c);
            if let Some(&q) = post_of_id.get(id) {
                connect(c, q, &mut adjacency);
            }
        }
    }

    let mut seeds: Vec<usize> = Vec::new();
    for id in touched {
        if let Some(&c) = pre_of_id.get(id) {
            seeds.push(c);
        }
        if let Some(&c) = post_of_id.get(id) {
            seeds.push(c);
        }
    }

    let mut seen = vec![false; total];
    let mut stack = Vec::new();
    for s in seeds {
        if !seen[s] {
            seen[s] = true;
            stack.push(s);
        }
    }
    while let Some(node) = stack.pop() {
        for &next in &adjacency[node] {
            if !seen[next] {
                seen[next] = true;
                stack.push(next);
            }
        }
    }

    let mut closure = Closure {
        walls: BTreeSet::new(),
        components_pre: BTreeSet::new(),
        components_post: BTreeSet::new(),
    };
    for (node, &hit) in seen.iter().enumerate() {
        if !hit {
            continue;
        }
        if node < offset {
            closure.components_pre.insert(node);
            for &w in cpre.walls(node) {
                if let Some(id) = pre.ids.get(w) {
                    closure.walls.insert(*id);
                }
            }
        } else {
            let c = node - offset;
            closure.components_post.insert(c);
            for &w in cpost.walls(c) {
                if let Some(id) = post.ids.get(w) {
                    closure.walls.insert(*id);
                }
            }
        }
    }
    closure
}

/// Indices of `faces` whose closure meets an occupied region of `closed`.
///
/// These are exactly the faces R5.1.9 permits to recompute. A face's closure
/// meets a region when a vertex of the face lies ON that region's boundary,
/// which is the only way the two closed sets can meet: a face is an open
/// component of the complement, so no face vertex is interior to the region,
/// and every face vertex that is not on the region's boundary is at positive
/// distance from it.
pub fn faces_in_scope(faces: &[Polygon], closed: &[Polygon]) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, face) in faces.iter().enumerate() {
        if face_meets(face, closed) {
            out.push(i);
        }
    }
    out
}

fn face_meets(face: &Polygon, closed: &[Polygon]) -> bool {
    let rings = std::iter::once(&face.outer).chain(face.holes.iter());
    for ring in rings {
        for &v in ring {
            for polygon in closed {
                if classify_point(v, &polygon.outer) == Containment::Boundary {
                    return true;
                }
                if polygon
                    .holes
                    .iter()
                    .any(|h| classify_point(v, h) == Containment::Boundary)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Disjoint-set forest over wall indices.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

/// Axis-aligned bounds with the weld tolerance folded into the overlap test.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn of_polygons(polygons: &[Polygon]) -> Option<Self> {
        let mut b: Option<Bounds> = None;
        for p in polygons {
            for &v in &p.outer {
                b = Some(match b {
                    None => Bounds {
                        min_x: v.x,
                        min_y: v.y,
                        max_x: v.x,
                        max_y: v.y,
                    },
                    Some(cur) => Bounds {
                        min_x: cur.min_x.min(v.x),
                        min_y: cur.min_y.min(v.y),
                        max_x: cur.max_x.max(v.x),
                        max_y: cur.max_y.max(v.y),
                    },
                });
            }
        }
        b
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.min_x <= other.max_x + WELD_MM
            && other.min_x <= self.max_x + WELD_MM
            && self.min_y <= other.max_y + WELD_MM
            && other.min_y <= self.max_y + WELD_MM
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walls::{Endpoint, JoinMember};

    /// The arrangement welds on [`WELD_MM`] and `domowik-format` decides join
    /// agreement on `JOIN_COINCIDENCE_MM`. ED-0103 N-2 makes those ONE rule,
    /// and the two crates cannot share a constant because geometry depends on
    /// format and not the reverse. This is the guard that makes the coupling
    /// deliberate instead of a coincidence: if either constant moves without
    /// the other, a document's joins and its topology stop agreeing.
    #[test]
    fn the_weld_quantum_and_the_join_quantum_are_the_same_number() {
        assert_eq!(WELD_MM, domowik_format::JOIN_COINCIDENCE_MM);
    }

    const P: fn(f64, f64) -> Point = Point::new;
    const T: f64 = 200.0;

    fn wall(x0: f64, y0: f64, x1: f64, y1: f64) -> WallSolid {
        WallSolid::new(P(x0, y0), P(x1, y1), T)
    }

    /// Walls around a closed rectangle, joined corner to corner. `base` is the
    /// index the first wall will occupy in the caller's slice.
    fn rect(x: f64, y: f64, w: f64, h: f64, base: usize) -> (Vec<WallSolid>, Vec<JoinSpec>) {
        let c = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
        let walls: Vec<WallSolid> = (0..4)
            .map(|i| {
                let (x0, y0) = c[i];
                let (x1, y1) = c[(i + 1) % 4];
                wall(x0, y0, x1, y1)
            })
            .collect();
        let joins = (0..4)
            .map(|i| JoinSpec {
                members: vec![
                    JoinMember {
                        wall: base + i,
                        endpoint: Endpoint::End,
                    },
                    JoinMember {
                        wall: base + (i + 1) % 4,
                        endpoint: Endpoint::Start,
                    },
                ],
            })
            .collect();
        (walls, joins)
    }

    fn ids(n: usize) -> Vec<Uuid> {
        (0..n).map(|_| Uuid::now_v7()).collect()
    }

    #[test]
    fn joins_decide_component_membership() {
        let (mut walls, mut joins) = rect(0.0, 0.0, 4000.0, 3000.0, 0);
        let (w2, j2) = rect(20000.0, 0.0, 4000.0, 3000.0, 4);
        walls.extend(w2);
        joins.extend(j2);

        let c = components_of(&walls, &joins);
        assert_eq!(c.len(), 2);
        assert_eq!(c.walls(0), &[0, 1, 2, 3]);
        assert_eq!(c.walls(1), &[4, 5, 6, 7]);
        assert_eq!(c.joins(0).len(), 4);
        assert_eq!(c.joins(1).len(), 4);
    }

    #[test]
    fn far_apart_components_are_unlinked() {
        let (mut walls, mut joins) = rect(0.0, 0.0, 4000.0, 3000.0, 0);
        let (w2, j2) = rect(20000.0, 0.0, 4000.0, 3000.0, 4);
        walls.extend(w2);
        joins.extend(j2);
        let c = components_of(&walls, &joins);
        assert_eq!(link(&c, 0, 1), None);
    }

    /// An island loop standing inside a courtyard: no join, no shared area,
    /// but R5.1.9 links them by containment because editing the courtyard
    /// changes the face the island sits in.
    #[test]
    fn containment_links_an_island_to_its_courtyard() {
        let (mut walls, mut joins) = rect(0.0, 0.0, 20000.0, 20000.0, 0);
        let (w2, j2) = rect(8000.0, 8000.0, 3000.0, 3000.0, 4);
        walls.extend(w2);
        joins.extend(j2);

        let c = components_of(&walls, &joins);
        assert_eq!(c.len(), 2);
        assert_eq!(link(&c, 0, 1), Some(Link::Containment));
    }

    /// Two unjoined walls crossing: the `wall-area-overlap` class of R3.1.2.
    #[test]
    fn positive_area_intersection_links_unjoined_walls() {
        let walls = vec![
            wall(0.0, 0.0, 4000.0, 0.0),
            wall(2000.0, -2000.0, 2000.0, 2000.0),
        ];
        let c = components_of(&walls, &[]);
        assert_eq!(c.len(), 2, "no join, so two components");
        assert_eq!(link(&c, 0, 1), Some(Link::Overlap));
    }

    /// Point contact is NOT a link. Two rectangles whose corners meet exactly
    /// intersect in zero area, and R5.1.9 links only containment and
    /// POSITIVE-area intersection. This is the case that makes the rule
    /// non-obvious: the two clusters can jointly bound a face and still be
    /// unlinked.
    #[test]
    fn point_contact_does_not_link() {
        // Two 200 mm-thick horizontal walls meeting corner to corner. Their
        // rectangles share exactly the point (2000, 100)...(2000, -100) edge?
        // No: offset vertically by exactly the thickness so only the corners
        // coincide.
        let walls = vec![
            wall(0.0, 0.0, 2000.0, 0.0),
            wall(2000.0, 200.0, 4000.0, 200.0),
        ];
        let c = components_of(&walls, &[]);
        assert_eq!(c.len(), 2);
        assert_eq!(
            link(&c, 0, 1),
            None,
            "corner-to-corner contact has zero area and must not link"
        );
    }

    /// Edge contact along a shared face is also zero-area and must not link.
    #[test]
    fn flush_edge_contact_does_not_link() {
        let walls = vec![
            wall(0.0, 0.0, 4000.0, 0.0),
            // Directly above, touching along y = 100.
            wall(0.0, 200.0, 4000.0, 200.0),
        ];
        let c = components_of(&walls, &[]);
        assert_eq!(c.len(), 2);
        assert_eq!(link(&c, 0, 1), None);
    }

    #[test]
    fn closure_of_an_isolated_edit_is_one_component() {
        let (mut walls, mut joins) = rect(0.0, 0.0, 4000.0, 3000.0, 0);
        let (w2, j2) = rect(20000.0, 0.0, 4000.0, 3000.0, 4);
        walls.extend(w2);
        joins.extend(j2);
        let id = ids(8);

        let state = State {
            walls: &walls,
            ids: &id,
            joins: &joins,
        };
        let closure = recomputation_closure(&state, &state, &[id[0]]);
        assert_eq!(closure.components_post.len(), 1);
        assert_eq!(closure.walls.len(), 4);
        assert!(closure.walls.contains(&id[0]));
        assert!(!closure.walls.contains(&id[4]));
    }

    /// An edit inside the courtyard drags the island in with it, transitively.
    #[test]
    fn closure_follows_containment() {
        let (mut walls, mut joins) = rect(0.0, 0.0, 20000.0, 20000.0, 0);
        let (w2, j2) = rect(8000.0, 8000.0, 3000.0, 3000.0, 4);
        walls.extend(w2);
        joins.extend(j2);
        let id = ids(8);
        let state = State {
            walls: &walls,
            ids: &id,
            joins: &joins,
        };

        let closure = recomputation_closure(&state, &state, &[id[0]]);
        assert_eq!(closure.components_post.len(), 2);
        assert_eq!(closure.walls.len(), 8);
    }

    /// A wall deleted by the edit is in the closure through its PRE state
    /// component, which no longer exists afterwards.
    #[test]
    fn closure_covers_a_deleted_wall_through_the_pre_state() {
        let (walls, joins) = rect(0.0, 0.0, 4000.0, 3000.0, 0);
        let id = ids(4);
        let pre = State {
            walls: &walls,
            ids: &id,
            joins: &joins,
        };

        // Post state: wall 3 gone, and its joins with it.
        let post_walls = walls[..3].to_vec();
        let post_ids = id[..3].to_vec();
        let post_joins = vec![joins[1].clone()];
        let post = State {
            walls: &post_walls,
            ids: &post_ids,
            joins: &post_joins,
        };

        let closure = recomputation_closure(&pre, &post, &[id[3]]);
        assert!(closure.walls.contains(&id[3]), "the deleted wall's own id");
        assert!(
            closure.walls.contains(&id[0]),
            "its pre-state component travels with it"
        );
        assert_eq!(closure.components_pre.len(), 1);
    }

    /// The closure is bounded by the level, never by the document: a component
    /// on another level is never reachable because it is never passed in.
    #[test]
    fn an_untouched_component_stays_out_of_scope() {
        let (mut walls, mut joins) = rect(0.0, 0.0, 4000.0, 3000.0, 0);
        for k in 1..5 {
            let (w, j) = rect(20000.0 * k as f64, 0.0, 4000.0, 3000.0, 4 * k);
            walls.extend(w);
            joins.extend(j);
        }
        let id = ids(walls.len());
        let state = State {
            walls: &walls,
            ids: &id,
            joins: &joins,
        };
        let closure = recomputation_closure(&state, &state, &[id[0]]);
        assert_eq!(closure.components_post.len(), 1);
        assert_eq!(
            closure.walls.len(),
            4,
            "one of five components, not the level"
        );
    }

    #[test]
    fn faces_in_scope_selects_only_touching_faces() {
        let (mut walls, mut joins) = rect(0.0, 0.0, 4000.0, 3000.0, 0);
        let (w2, j2) = rect(20000.0, 0.0, 4000.0, 3000.0, 4);
        walls.extend(w2);
        joins.extend(j2);

        let (_, faces) = crate::walls::plan(&walls, &joins);
        assert_eq!(faces.len(), 2, "two disjoint rooms");

        let c = components_of(&walls, &joins);
        let scoped = faces_in_scope(&faces, c.region(0));
        assert_eq!(scoped.len(), 1, "only the room of the touched component");
    }

    #[test]
    fn empty_level_has_no_components_and_an_empty_closure() {
        let c = components_of(&[], &[]);
        assert!(c.is_empty());
        let state = State {
            walls: &[],
            ids: &[],
            joins: &[],
        };
        let closure = recomputation_closure(&state, &state, &[Uuid::now_v7()]);
        assert!(closure.is_empty());
    }
}
