//! Union of simple polygons by boundary classification.
//!
//! # Why a union at all
//!
//! BEHAVIOR-SPEC R3.2.1 defines a wall cluster's occupied region as the UNION
//! of each wall's butt-capped rectangle and each join's patch, and R5.1.1
//! defines the rooms of a level as the bounded components of the complement of
//! that region. Both the drawing and the floor area therefore depend on one
//! operation: unioning overlapping rectangles into a single boundary.
//!
//! # How it works
//!
//! Every input ring is oriented counter-clockwise, so each directed edge
//! carries its polygon's interior on its LEFT. Edges are split at every mutual
//! intersection, after which an edge fragment is either wholly inside another
//! polygon, wholly outside every other polygon, or exactly coincident with a
//! fragment of another polygon. Each case has one rule:
//!
//! - A fragment with a REVERSED twin has union interior on both sides — the
//!   two polygons meet back to back — so it is not on the union boundary.
//! - A fragment strictly inside another polygon is not on the union boundary.
//! - Anything else is on the boundary; same-direction duplicates keep one copy.
//!
//! The surviving directed edges are then traced into rings by always taking
//! the first outgoing edge encountered when sweeping CLOCKWISE from the edge
//! we arrived on. That is the `next` rule of a doubly connected edge list for
//! a face lying to the left of its boundary, and it splits a vertex where two
//! components touch at a single point into two simple rings rather than one
//! figure-eight — which is what R5.1.2 requires of pinch points.

use crate::arrangement::classify_point;
use crate::arrangement::{cuts_along, ring_contains_ring, Containment, VertexKey, WELD_MM};
use crate::polygon::{
    canonical_ring, compare_polygons, compare_rings, signed_double_area, Polygon, Winding,
};
use domowik_format::Point;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// One input ring and its caller-defined source tag.
///
/// The tag is an opaque identity, not an index into this slice; gaps and
/// arbitrary values are preserved. Coordinates are millimetres and inherit
/// this module's existing welded/inexact predicate limits.
#[derive(Debug, Clone)]
pub struct SourceRing {
    pub points: Vec<Point>,
    pub source: usize,
}

/// The provenance of ONE canonical boundary edge, kept at the granularity the
/// arrangement actually resolved rather than at the granularity canonical
/// output happens to print.
///
/// Canonicalization deletes collinear vertices, so one canonical edge can span
/// several consecutive boundary pieces that different inputs contributed. The
/// union of their tags would answer "who touches this edge" but not "who owns
/// the part of it under THIS point", and an ownership rule built on the union
/// alone cannot tell a wall's own face from a collinear neighbour's. So the
/// pieces are retained: `breaks` holds the deleted interior vertices in order
/// from the edge's start vertex to its end vertex, and `sources` holds one tag
/// list per piece, so `sources.len() == breaks.len() + 1`. An edge that lost no
/// vertex has no breaks and exactly one piece.
///
/// Within a piece, co-ownership is real and not an artefact: two inputs whose
/// boundaries COINCIDE there both contributed it, and no finer split exists.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryRun {
    /// Interior vertices removed by collinear canonicalization, in order from
    /// the canonical edge's start vertex towards its end vertex.
    pub breaks: Vec<Point>,
    /// One sorted, deduplicated tag list per piece.
    pub sources: Vec<Vec<usize>>,
}

impl BoundaryRun {
    /// Every tag touching this edge, sorted and deduplicated.
    ///
    /// Lossy on purpose: it answers a membership question, never an ownership
    /// one. Use [`BoundaryRun::piece_at`] when the answer must be about a
    /// particular point.
    pub fn merged_sources(&self) -> Vec<usize> {
        let mut all: Vec<usize> = Vec::new();
        for piece in &self.sources {
            merge_sources(&mut all, piece);
        }
        all
    }

    /// The tags of the piece containing `point`, for an edge running
    /// `start -> end`.
    ///
    /// The piece is located by the point's position ALONG the edge, which is
    /// an ordering question, not a proximity one: no tolerance, radius, or
    /// nearest-feature search participates, so this does not reintroduce the
    /// distance-based ownership R6.1.3 forbids. A point at an interior break
    /// belongs to the piece that ENDS there, so the answer is total and the
    /// two pieces sharing a break never both claim it.
    ///
    /// Returns `None` when the edge is degenerate and has no direction to
    /// order positions along.
    pub fn piece_at(&self, start: Point, end: Point, point: Point) -> Option<&[usize]> {
        let (dx, dy) = (end.x - start.x, end.y - start.y);
        let dd = dx * dx + dy * dy;
        if !dd.is_finite() || dd.partial_cmp(&0.0) != Some(Ordering::Greater) {
            return None;
        }
        let along = |p: Point| ((p.x - start.x) * dx + (p.y - start.y) * dy) / dd;
        let t = along(point);
        for (i, brk) in self.breaks.iter().enumerate() {
            if t <= along(*brk) {
                return self.sources.get(i).map(Vec::as_slice);
            }
        }
        self.sources.last().map(Vec::as_slice)
    }
}

/// A canonical union polygon plus the provenance of each canonical boundary
/// edge. `outer_runs[i]` describes `outer[i] -> outer[(i + 1) % n]`; holes use
/// the same convention.
///
/// This metadata is descriptive only. It makes no ownership or exclusion
/// policy decision, and does not claim exact geometry beyond [`union`]'s
/// existing numerical behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcedPolygon {
    pub polygon: Polygon,
    pub outer_runs: Vec<BoundaryRun>,
    pub hole_runs: Vec<Vec<BoundaryRun>>,
}

impl SourcedPolygon {
    /// Per-outer-edge merged tags, in `outer` order.
    pub fn outer_sources(&self) -> Vec<Vec<usize>> {
        self.outer_runs
            .iter()
            .map(BoundaryRun::merged_sources)
            .collect()
    }

    /// Per-hole, per-edge merged tags, in `holes` order.
    pub fn hole_sources(&self) -> Vec<Vec<Vec<usize>>> {
        self.hole_runs
            .iter()
            .map(|hole| hole.iter().map(BoundaryRun::merged_sources).collect())
            .collect()
    }
}

/// Canonical union with per-edge original input-ring tags.
///
/// Unlike [`union`], this allocates and transports metadata. The untagged
/// fast path remains unchanged. Dropped degenerate inputs contribute no tags.
pub fn union_with_sources(rings: &[SourceRing]) -> Vec<SourcedPolygon> {
    let inputs: Vec<(Vec<Point>, usize)> = rings
        .iter()
        .filter_map(|r| canonical_ring(&r.points, Winding::CounterClockwise).map(|p| (p, r.source)))
        .collect();
    if inputs.is_empty() {
        return Vec::new();
    }
    let points: Vec<Vec<Point>> = inputs.iter().map(|(p, _)| p.clone()).collect();
    let tags: Vec<usize> = inputs.iter().map(|(_, tag)| *tag).collect();
    let fragments = split_all(&points);
    let kept = classify_fragments_sourced(&points, &tags, &fragments);
    assemble_sourced(trace_rings_sourced(&kept))
}

/// The union of `rings`, as canonical polygons with holes.
///
/// Each input is treated as a simple closed ring; winding is normalized, so
/// callers need not supply a particular direction. Rings that enclose no area
/// contribute nothing: a degenerate join patch (R3.2.3) is a closed segment
/// through the join node and is always covered by the cross-section it came
/// from, so dropping it changes neither the region nor its boundary.
///
/// Output is deterministic: components are sorted by the R3.2.4 canonical
/// ordering, so the same document always renders the same sequence.
pub fn union(rings: &[Vec<Point>]) -> Vec<Polygon> {
    union_and_complement(rings).0
}

/// The union of `rings`, AND the bounded components of its complement.
///
/// The second value is what BEHAVIOR-SPEC R5.1.1 calls the rooms of a level:
/// the bounded connected components of `ℝ² ∖ O_L`. It cannot be recovered from
/// the first value alone. A hole of a union component is one such face, but not
/// every face is a hole of a component: four wall clusters meeting only at
/// corner points enclose a void that belongs to no single component, and
/// R5.1.2 is explicit that such a region is a face.
///
/// Both are read off the same boundary. Every surviving directed edge has the
/// occupied region on its LEFT, so tracing them gives the occupied faces;
/// tracing their REVERSES — free space on the left — gives the free ones. The
/// unbounded exterior face traverses clockwise and is discarded by the same
/// sign test that discards an outer face in [`assemble`], which is exactly the
/// rule R5.1.2 states: the unbounded component is exterior and is never a room.
pub fn union_and_complement(rings: &[Vec<Point>]) -> (Vec<Polygon>, Vec<Polygon>) {
    let inputs: Vec<Vec<Point>> = rings
        .iter()
        .filter_map(|r| canonical_ring(r, Winding::CounterClockwise))
        .collect();
    if inputs.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let fragments = split_all(&inputs);
    let kept = classify_fragments(&inputs, &fragments);
    let occupied = assemble(trace_rings(&kept));

    let reversed: Vec<(Point, Point)> = kept.iter().map(|&(a, b)| (b, a)).collect();
    let free = assemble(trace_rings(&reversed));

    (occupied, free)
}

/// A directed edge fragment and the index of the polygon it came from.
struct Fragment {
    owner: usize,
    from: Point,
    to: Point,
}

#[derive(Clone)]
struct SourcedEdge {
    from: Point,
    to: Point,
    sources: Vec<usize>,
}

fn merge_sources(into: &mut Vec<usize>, other: &[usize]) {
    into.extend_from_slice(other);
    into.sort_unstable();
    into.dedup();
}

fn classify_fragments_sourced(
    inputs: &[Vec<Point>],
    tags: &[usize],
    fragments: &[Fragment],
) -> Vec<SourcedEdge> {
    let mut directed: HashSet<(VertexKey, VertexKey)> = HashSet::new();
    for f in fragments {
        directed.insert((VertexKey::of(f.from), VertexKey::of(f.to)));
    }
    let boxes: Vec<Bounds> = inputs.iter().map(|r| Bounds::of(r)).collect();
    // Same-direction duplicates are one boundary edge, whose provenance is the
    // union of every surviving input fragment rather than first-seen order.
    let mut indices: HashMap<(VertexKey, VertexKey), usize> = HashMap::new();
    let mut kept: Vec<SourcedEdge> = Vec::new();
    for f in fragments {
        let key = (VertexKey::of(f.from), VertexKey::of(f.to));
        if directed.contains(&(key.1, key.0)) {
            continue;
        }
        let mid = Point::new((f.from.x + f.to.x) / 2.0, (f.from.y + f.to.y) / 2.0);
        if inputs.iter().enumerate().any(|(j, ring)| {
            j != f.owner
                && boxes[j].contains(mid)
                && classify_point(mid, ring) == Containment::Inside
        }) {
            continue;
        }
        if let Some(&index) = indices.get(&key) {
            merge_sources(&mut kept[index].sources, &[tags[f.owner]]);
        } else {
            indices.insert(key, kept.len());
            kept.push(SourcedEdge {
                from: f.from,
                to: f.to,
                sources: vec![tags[f.owner]],
            });
        }
    }
    kept
}

fn trace_rings_sourced(edges: &[SourcedEdge]) -> Vec<(Vec<Point>, Vec<BoundaryRun>)> {
    let mut position = HashMap::new();
    let mut outgoing: HashMap<VertexKey, Vec<VertexKey>> = HashMap::new();
    let mut provenance: HashMap<(VertexKey, VertexKey), Vec<usize>> = HashMap::new();
    for edge in edges {
        let (a, b) = (VertexKey::of(edge.from), VertexKey::of(edge.to));
        remember(&mut position, a, edge.from);
        remember(&mut position, b, edge.to);
        outgoing.entry(a).or_default().push(b);
        outgoing.entry(b).or_default();
        provenance.insert((a, b), edge.sources.clone());
    }
    let mut starts: Vec<_> = provenance.keys().copied().collect();
    starts.sort();
    let total = starts.len();
    let mut visited = HashSet::new();
    let mut rings = Vec::new();
    for start in starts {
        if visited.contains(&start) {
            continue;
        }
        let (mut from, mut to) = start;
        let mut keys = Vec::new();
        let mut sources = Vec::new();
        loop {
            if !visited.insert((from, to)) {
                break;
            }
            keys.push(from);
            sources.push(BoundaryRun {
                breaks: Vec::new(),
                sources: vec![provenance[&(from, to)].clone()],
            });
            let Some(next) = next_clockwise(&outgoing, &position, from, to) else {
                break;
            };
            from = to;
            to = next;
            if (from, to) == start || keys.len() > total {
                break;
            }
        }
        if keys.len() >= 3 {
            rings.push((keys.iter().map(|k| position[k]).collect(), sources));
        }
    }
    rings
}

fn canonical_sourced_ring(
    mut points: Vec<Point>,
    mut runs: Vec<BoundaryRun>,
    winding: Winding,
) -> Option<(Vec<Point>, Vec<BoundaryRun>)> {
    // Trace output has one single-piece run per directed edge. The existing
    // union has already welded vertices; retain its geometry rules and only
    // concatenate provenance when canonicalization collapses a boundary run.
    if points.len() != runs.len() || points.len() < 3 {
        return None;
    }
    let mut i = 0;
    while i < points.len() && points.len() >= 3 {
        let n = points.len();
        if crate::orientation(points[(i + n - 1) % n], points[i], points[(i + 1) % n])
            == crate::Orientation::Collinear
        {
            // Removing vertex `i` joins the edge ENDING there to the edge
            // LEAVING it. The deleted vertex becomes the interior break
            // between their pieces, so which input owns which part of the
            // merged edge survives the collapse.
            let removed = runs.remove(i);
            let vertex = points.remove(i);
            let target = if i == 0 { runs.len() - 1 } else { i - 1 };
            runs[target].breaks.push(vertex);
            runs[target].breaks.extend(removed.breaks);
            runs[target].sources.extend(removed.sources);
        } else {
            i += 1;
        }
    }
    if points.len() < 3 || signed_double_area(&points) == 0.0 {
        return None;
    }
    if (signed_double_area(&points) > 0.0) != (winding == Winding::CounterClockwise) {
        points.reverse();
        // Reversing the ring reverses each edge's own direction too, so a
        // run's pieces must be re-read end-to-start or they would describe the
        // edge backwards.
        for run in &mut runs {
            run.breaks.reverse();
            run.sources.reverse();
        }
        runs.reverse();
        runs.rotate_left(1);
    }
    let mut best = 0;
    for i in 1..points.len() {
        if (points[i].x, points[i].y) < (points[best].x, points[best].y) {
            best = i;
        }
    }
    points.rotate_left(best);
    runs.rotate_left(best);
    Some((points, runs))
}

fn assemble_sourced(rings: Vec<(Vec<Point>, Vec<BoundaryRun>)>) -> Vec<SourcedPolygon> {
    let mut outers = Vec::new();
    let mut holes = Vec::new();
    for (points, runs) in rings {
        let signed = signed_double_area(&points);
        if signed > 0.0 {
            if let Some(r) = canonical_sourced_ring(points, runs, Winding::CounterClockwise) {
                outers.push(r);
            }
        } else if signed < 0.0 {
            if let Some(r) = canonical_sourced_ring(points, runs, Winding::Clockwise) {
                holes.push(r);
            }
        }
    }
    let mut polygons: Vec<SourcedPolygon> = outers
        .into_iter()
        .map(|(outer, outer_runs)| SourcedPolygon {
            polygon: Polygon::new(outer),
            outer_runs,
            hole_runs: Vec::new(),
        })
        .collect();
    for (hole, runs) in holes {
        let mut best = None;
        for (i, polygon) in polygons.iter().enumerate() {
            if ring_contains_ring(&polygon.polygon.outer, &hole) {
                let area = signed_double_area(&polygon.polygon.outer).abs();
                match best {
                    Some((a, _)) if a <= area => {}
                    _ => best = Some((area, i)),
                }
            }
        }
        if let Some((_, i)) = best {
            polygons[i].polygon.holes.push(hole);
            polygons[i].hole_runs.push(runs);
        }
    }
    // Canonical hole order is geometry-only, identical to Polygon::canonicalize.
    for polygon in &mut polygons {
        let mut holes: Vec<_> = polygon
            .polygon
            .holes
            .drain(..)
            .zip(polygon.hole_runs.drain(..))
            .collect();
        holes.sort_by(|a, b| compare_rings(&a.0, &b.0));
        let (h, s): (Vec<_>, Vec<_>) = holes.into_iter().unzip();
        polygon.polygon.holes = h;
        polygon.hole_runs = s;
    }
    polygons.sort_by(|a, b| compare_polygons(&a.polygon, &b.polygon));
    polygons
}

/// Splits every ring edge at every intersection with every other ring's edges.
///
/// Edges of the SAME ring are not cut against each other: inputs are simple
/// rings, so their own edges meet only at shared endpoints, and passing them
/// through the collinear-overlap test would cut every edge at its own
/// neighbours for nothing.
fn split_all(inputs: &[Vec<Point>]) -> Vec<Fragment> {
    let edges_of: Vec<Vec<(Point, Point)>> = inputs
        .iter()
        .map(|ring| {
            (0..ring.len())
                .map(|i| (ring[i], ring[(i + 1) % ring.len()]))
                .collect()
        })
        .collect();

    // Rings that cannot touch cannot cut each other. Without this every ring
    // is tested against every other, which is the whole cost on a plan where
    // most walls are nowhere near each other.
    let boxes: Vec<Bounds> = inputs.iter().map(|r| Bounds::of(r)).collect();

    // One canonical representative per welded input vertex.
    //
    // A cut parameter that lands on another ring's CORNER has an exact answer
    // — that corner — but `lerp` rebuilds it from `a + (b - a) * t` and can
    // miss by an ulp. Two touching rectangles produced a ring carrying
    // `y = 3591.8203124999995` where the input corner is `3591.8203125`,
    // and the whole pipeline downstream then reasons about a vertex the plan
    // does not contain: it is what left a residual ~1e-9 mm^2 in areas that
    // must cancel exactly (R5.1.9), and what let two faces of one rectangle
    // disagree about which snap candidate is nearer.
    //
    // Points sharing a `VertexKey` are ONE vertex under ED-0103's N-2, so
    // choosing which of them to carry is free of geometric consequence — and
    // an input vertex is the only choice that reproduces the caller's own
    // coordinates. The least `(x, y)` breaks a tie between several, so the
    // representative never depends on iteration order (R6.3.3).
    let mut vertices: HashMap<VertexKey, Point> = HashMap::new();
    for ring in inputs {
        for p in ring {
            vertices
                .entry(VertexKey::of(*p))
                .and_modify(|q| {
                    if (p.x, p.y) < (q.x, q.y) {
                        *q = *p;
                    }
                })
                .or_insert(*p);
        }
    }
    let snap = |p: Point| vertices.get(&VertexKey::of(p)).copied().unwrap_or(p);

    let mut out = Vec::new();
    for (i, edges) in edges_of.iter().enumerate() {
        let others: Vec<(Point, Point)> = edges_of
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i && boxes[i].overlaps(&boxes[*j]))
            .flat_map(|(_, e)| e.iter().copied())
            .collect();

        for &(a, b) in edges {
            if VertexKey::of(a) == VertexKey::of(b) {
                continue;
            }
            // Two separate steps, deliberately not folded together: the cut
            // POINTS are already correctly rounded constructions or reused
            // input vertices, and `snap` then replaces each by the canonical
            // representative of its weld class (ED-0103 N-2).
            for w in cuts_along(a, b, &others).windows(2) {
                let (p, q) = (snap(w[0]), snap(w[1]));
                if VertexKey::of(p) != VertexKey::of(q) {
                    out.push(Fragment {
                        owner: i,
                        from: p,
                        to: q,
                    });
                }
            }
        }
    }
    out
}

/// An axis-aligned bounding box, used only to reject pairs that cannot
/// interact before any exact predicate runs.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn of(ring: &[Point]) -> Self {
        let mut b = Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        };
        for p in ring {
            b.min_x = b.min_x.min(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_x = b.max_x.max(p.x);
            b.max_y = b.max_y.max(p.y);
        }
        b
    }

    /// Overlap is tested with a weld-width margin, so two rings that only
    /// TOUCH — the normal case for a wall meeting a join patch — are never
    /// rejected by a boundary comparison that rounding could tip either way.
    fn overlaps(&self, other: &Self) -> bool {
        self.min_x <= other.max_x + WELD_MM
            && other.min_x <= self.max_x + WELD_MM
            && self.min_y <= other.max_y + WELD_MM
            && other.min_y <= self.max_y + WELD_MM
    }

    fn contains(&self, p: Point) -> bool {
        p.x >= self.min_x - WELD_MM
            && p.x <= self.max_x + WELD_MM
            && p.y >= self.min_y - WELD_MM
            && p.y <= self.max_y + WELD_MM
    }
}

/// Applies the three boundary rules and returns the surviving directed edges.
fn classify_fragments(inputs: &[Vec<Point>], fragments: &[Fragment]) -> Vec<(Point, Point)> {
    let mut directed: HashSet<(VertexKey, VertexKey)> = HashSet::new();
    for f in fragments {
        directed.insert((VertexKey::of(f.from), VertexKey::of(f.to)));
    }

    let boxes: Vec<Bounds> = inputs.iter().map(|r| Bounds::of(r)).collect();
    let mut emitted: HashSet<(VertexKey, VertexKey)> = HashSet::new();
    let mut kept = Vec::new();

    for f in fragments {
        let key = (VertexKey::of(f.from), VertexKey::of(f.to));
        // Rule 1: a reversed twin means interior on both sides.
        if directed.contains(&(key.1, key.0)) {
            continue;
        }
        // Rule 3 (dedupe): identical direction from two polygons is one edge.
        if emitted.contains(&key) {
            continue;
        }
        // Rule 2: strictly inside any other polygon.
        let mid = Point::new((f.from.x + f.to.x) / 2.0, (f.from.y + f.to.y) / 2.0);
        let swallowed = inputs.iter().enumerate().any(|(j, ring)| {
            j != f.owner
                && boxes[j].contains(mid)
                && classify_point(mid, ring) == Containment::Inside
        });
        if swallowed {
            continue;
        }
        emitted.insert(key);
        kept.push((f.from, f.to));
    }
    kept
}

/// Walks the surviving directed edges into closed rings.
fn trace_rings(edges: &[(Point, Point)]) -> Vec<Vec<Point>> {
    let mut position: HashMap<VertexKey, Point> = HashMap::new();
    let mut outgoing: HashMap<VertexKey, Vec<VertexKey>> = HashMap::new();
    for &(a, b) in edges {
        let (ka, kb) = (VertexKey::of(a), VertexKey::of(b));
        // Two distinct points can weld to one key. Which of them represents the
        // vertex must not depend on which was seen first, or the same document
        // renders different coordinates depending on the order its walls were
        // created. The lexicographically least candidate is order-independent
        // and is always one of the real input coordinates, so exact input still
        // produces exact output.
        remember(&mut position, ka, a);
        remember(&mut position, kb, b);
        outgoing.entry(ka).or_default().push(kb);
        outgoing.entry(kb).or_default();
    }

    // Deterministic start order: HashMap iteration order must never reach the
    // output, or the same document renders a different component sequence
    // between runs.
    let mut starts: Vec<(VertexKey, VertexKey)> = edges
        .iter()
        .map(|&(a, b)| (VertexKey::of(a), VertexKey::of(b)))
        .collect();
    starts.sort();
    starts.dedup();

    let total = starts.len();
    let mut visited: HashSet<(VertexKey, VertexKey)> = HashSet::new();
    let mut rings = Vec::new();

    for start in starts {
        if visited.contains(&start) {
            continue;
        }
        let mut ring_keys = Vec::new();
        let (mut from, mut to) = start;

        loop {
            if !visited.insert((from, to)) {
                break;
            }
            ring_keys.push(from);
            let Some(next) = next_clockwise(&outgoing, &position, from, to) else {
                break;
            };
            from = to;
            to = next;
            if (from, to) == start {
                break;
            }
            // A ring cannot use more half-edges than exist; this bounds the
            // walk against a malformed adjacency rather than hanging.
            if ring_keys.len() > total {
                break;
            }
        }

        if ring_keys.len() >= 3 {
            rings.push(ring_keys.iter().map(|k| position[k]).collect());
        }
    }
    rings
}

/// The DCEL `next` rule: from the edge `from → to`, the continuation is the
/// first outgoing edge of `to` met when sweeping clockwise from the direction
/// back to `from`.
///
/// Sweeping clockwise keeps the face on the LEFT of the traversal.
///
/// The offset is normalized into the half-open interval `(0, 360]` rather than
/// `[0, 360)`: an offset of exactly zero means the only continuation is back
/// the way we came, and ranking that LAST rather than first stops a spike from
/// being preferred over a real continuation. This is DEFENSIVE, in the same
/// class as the walk bound in `trace_rings` — no mutation of it changes any
/// test, because a union of simple rings does not produce a dangling edge. It
/// is kept because it costs nothing and the alternative is a silent
/// mis-traversal if one ever does.
fn next_clockwise(
    outgoing: &HashMap<VertexKey, Vec<VertexKey>>,
    position: &HashMap<VertexKey, Point>,
    from: VertexKey,
    to: VertexKey,
) -> Option<VertexKey> {
    let candidates = outgoing.get(&to)?;
    if candidates.is_empty() {
        return None;
    }
    let origin = position[&to];
    let back = angle_of(origin, position[&from]);

    let mut best: Option<(f64, VertexKey)> = None;
    for &c in candidates {
        let mut delta = back - angle_of(origin, position[&c]);
        while delta <= 0.0 {
            delta += std::f64::consts::TAU;
        }
        while delta > std::f64::consts::TAU {
            delta -= std::f64::consts::TAU;
        }
        match best {
            Some((d, _)) if d <= delta => {}
            _ => best = Some((delta, c)),
        }
    }
    best.map(|(_, c)| c)
}

/// Records `p` as the representative of `key`, keeping the lexicographically
/// least candidate so the choice cannot depend on iteration or insertion order.
fn remember(position: &mut HashMap<VertexKey, Point>, key: VertexKey, p: Point) {
    match position.get(&key) {
        Some(existing) if (existing.x, existing.y) <= (p.x, p.y) => {}
        _ => {
            position.insert(key, p);
        }
    }
}

fn angle_of(origin: Point, to: Point) -> f64 {
    (to.y - origin.y).atan2(to.x - origin.x)
}

/// Sorts traced rings into outer boundaries with their holes.
fn assemble(rings: Vec<Vec<Point>>) -> Vec<Polygon> {
    let mut outers: Vec<Vec<Point>> = Vec::new();
    let mut holes: Vec<Vec<Point>> = Vec::new();

    for ring in rings {
        let signed = signed_double_area(&ring);
        if signed > 0.0 {
            if let Some(r) = canonical_ring(&ring, Winding::CounterClockwise) {
                outers.push(r);
            }
        } else if signed < 0.0 {
            if let Some(r) = canonical_ring(&ring, Winding::Clockwise) {
                holes.push(r);
            }
        }
    }

    let mut polygons: Vec<Polygon> = outers.into_iter().map(Polygon::new).collect();

    // Each hole belongs to the SMALLEST outer ring that contains it. Charging
    // it to every ancestor would subtract the same void more than once.
    for hole in holes {
        let mut best: Option<(f64, usize)> = None;
        for (i, poly) in polygons.iter().enumerate() {
            if ring_contains_ring(&poly.outer, &hole) {
                let area = signed_double_area(&poly.outer).abs();
                match best {
                    Some((a, _)) if a <= area => {}
                    _ => best = Some((area, i)),
                }
            }
        }
        if let Some((_, i)) = best {
            polygons[i].holes.push(hole);
        }
    }

    let mut out: Vec<Polygon> = polygons
        .into_iter()
        .filter_map(|p| p.canonicalize())
        .collect();
    out.sort_by(compare_polygons);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![P(x0, y0), P(x1, y0), P(x1, y1), P(x0, y1)]
    }

    #[test]
    fn union_of_one_rectangle_is_that_rectangle() {
        let out = union(&[rect(0.0, 0.0, 10.0, 5.0)]);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].outer,
            vec![P(0.0, 0.0), P(10.0, 0.0), P(10.0, 5.0), P(0.0, 5.0)]
        );
        assert!(out[0].holes.is_empty());
    }

    #[test]
    fn sourced_union_matches_plain_union_and_preserves_canonical_edge_tags() {
        let rings = vec![
            SourceRing {
                points: rect(0.0, 0.0, 10.0, 10.0),
                source: 7,
            },
            SourceRing {
                points: rect(10.0, 0.0, 20.0, 10.0),
                source: 42,
            },
        ];
        let sourced = union_with_sources(&rings);
        assert_eq!(
            sourced
                .iter()
                .map(|p| p.polygon.clone())
                .collect::<Vec<_>>(),
            union(&rings.iter().map(|r| r.points.clone()).collect::<Vec<_>>())
        );
        assert_eq!(sourced.len(), 1);
        assert_eq!(
            sourced[0].outer_sources().len(),
            sourced[0].polygon.outer.len()
        );
        assert!(sourced[0].outer_sources().iter().any(|tags| tags == &[7]));
        assert!(sourced[0].outer_sources().iter().any(|tags| tags == &[42]));
    }

    #[test]
    fn sourced_union_merges_duplicate_tags_and_drops_hidden_edges() {
        let rings = vec![
            SourceRing {
                points: rect(0.0, 0.0, 10.0, 10.0),
                source: 9,
            },
            SourceRing {
                points: rect(0.0, 0.0, 10.0, 10.0),
                source: 3,
            },
            SourceRing {
                points: rect(2.0, 2.0, 8.0, 8.0),
                source: 99,
            },
        ];
        let sourced = union_with_sources(&rings);
        assert_eq!(sourced.len(), 1);
        assert!(sourced[0]
            .outer_sources()
            .iter()
            .all(|tags| tags == &[3, 9]));
        assert!(sourced[0]
            .outer_sources()
            .iter()
            .all(|tags| !tags.contains(&99)));
    }

    #[test]
    fn sourced_canonicalization_keeps_index_zero_collinear_tags_on_joined_edge() {
        let points = vec![
            P(1.0, 0.0),
            P(2.0, 0.0),
            P(2.0, 1.0),
            P(0.0, 1.0),
            P(0.0, 0.0),
        ];
        let runs: Vec<BoundaryRun> = [10, 20, 30, 40, 50]
            .into_iter()
            .map(|tag| BoundaryRun {
                breaks: Vec::new(),
                sources: vec![vec![tag]],
            })
            .collect();
        for winding in [Winding::CounterClockwise, Winding::Clockwise] {
            let (ring, edge_runs) =
                canonical_sourced_ring(points.clone(), runs.clone(), winding).unwrap();
            assert_eq!(ring.len(), 4);
            assert_eq!(edge_runs.len(), ring.len());
            let i = edge_runs
                .iter()
                .position(|run| run.merged_sources().contains(&10))
                .unwrap();
            let (start, end) = (ring[i], ring[(i + 1) % ring.len()]);
            // The collapse merged two pieces; the union answer is unchanged...
            assert_eq!(edge_runs[i].merged_sources(), vec![10, 50]);
            // ...and the deleted vertex survives as the break between them, so
            // the two halves of the merged edge remain separately attributable
            // in BOTH windings, where the pieces are read in opposite order.
            assert_eq!(edge_runs[i].breaks, vec![P(1.0, 0.0)]);
            assert_eq!(
                edge_runs[i].piece_at(start, end, P(0.5, 0.0)),
                Some([50].as_slice()),
                "left half, winding {winding:?}"
            );
            assert_eq!(
                edge_runs[i].piece_at(start, end, P(1.5, 0.0)),
                Some([10].as_slice()),
                "right half, winding {winding:?}"
            );
            // Every metadata entry still aligns with a real canonical edge,
            // including after winding reversal and least-vertex rotation.
            for (j, run) in edge_runs.iter().enumerate() {
                assert_eq!(run.sources.len(), run.breaks.len() + 1);
                assert!(
                    run.sources.iter().all(|piece| !piece.is_empty()),
                    "edge {:?} -> {:?}",
                    ring[j],
                    ring[(j + 1) % ring.len()]
                );
            }
        }
    }

    #[test]
    fn union_of_disjoint_rectangles_keeps_two_components() {
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(20.0, 0.0, 30.0, 10.0)]);
        assert_eq!(out.len(), 2, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 100.0);
        assert_eq!(out[1].area_mm2(), 100.0);
    }

    #[test]
    fn union_of_overlapping_rectangles_is_one_component_of_the_right_area() {
        // 10x10 and 10x10 overlapping in a 5x10 band: 100 + 100 - 50 = 150.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(5.0, 0.0, 15.0, 10.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 150.0);
        assert_eq!(
            out[0].outer,
            vec![P(0.0, 0.0), P(15.0, 0.0), P(15.0, 10.0), P(0.0, 10.0)]
        );
    }

    #[test]
    fn union_of_edge_to_edge_rectangles_merges_and_drops_the_shared_edge() {
        // The shared edge carries interior on both sides; if it survived, the
        // result would be two rings sharing a wall instead of one rectangle.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 0.0, 20.0, 10.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(
            out[0].outer,
            vec![P(0.0, 0.0), P(20.0, 0.0), P(20.0, 10.0), P(0.0, 10.0)]
        );
    }

    #[test]
    fn union_of_a_closed_frame_produces_a_hole() {
        // Four bars forming a square annulus: outer 12x12, inner void 8x8.
        let out = union(&[
            rect(0.0, 0.0, 12.0, 2.0),
            rect(0.0, 10.0, 12.0, 12.0),
            rect(0.0, 0.0, 2.0, 12.0),
            rect(10.0, 0.0, 12.0, 12.0),
        ]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].holes.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 144.0 - 64.0);
        // The hole winds clockwise and starts at its least vertex.
        assert_eq!(out[0].holes[0][0], P(2.0, 2.0));
        assert!(signed_double_area(&out[0].holes[0]) < 0.0);
    }

    #[test]
    fn union_output_is_independent_of_input_order() {
        let a = union(&[
            rect(0.0, 0.0, 12.0, 2.0),
            rect(0.0, 10.0, 12.0, 12.0),
            rect(0.0, 0.0, 2.0, 12.0),
            rect(10.0, 0.0, 12.0, 12.0),
        ]);
        let b = union(&[
            rect(10.0, 0.0, 12.0, 12.0),
            rect(0.0, 0.0, 2.0, 12.0),
            rect(0.0, 10.0, 12.0, 12.0),
            rect(0.0, 0.0, 12.0, 2.0),
        ]);
        assert_eq!(a, b);
    }

    #[test]
    fn output_coordinates_do_not_depend_on_input_order_at_the_weld_tolerance() {
        // Two squares whose corners are 0.4 µm apart — inside WELD_MM, so they
        // are one vertex. Which of the two coordinates represents that vertex
        // must not depend on which square was listed first.
        //
        // Found by adversarial review (Codex, gpt-5.6-sol, 2026-08-31). The
        // representative was previously first-seen, so `union(&[a, b])` and
        // `union(&[b, a])` returned rings differing in a real coordinate, and
        // the existing order-independence test could not see it because its
        // inputs coincided exactly.
        let a = rect(0.0, 0.0, 1000.0, 1000.0);
        let b = rect(1000.0004, 1000.0004, 2000.0, 2000.0);
        assert_eq!(
            union(&[a.clone(), b.clone()]),
            union(&[b, a]),
            "the weld representative must be order-independent"
        );
    }

    #[test]
    fn a_cycle_of_point_touching_triangles_encloses_one_room() {
        // Adversarial review's exact case (Codex, gpt-5.6-sol, 2026-08-31).
        // Four right triangles meet cyclically at (5,0), (10,5), (5,10) and
        // (0,5). The union is four separate components — correct, per R5.1.2's
        // pinch rule — and the diamond they enclose is a bounded component of
        // the complement, so it is a room. It is a hole of NO component.
        let tri = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
            vec![P(a.0, a.1), P(b.0, b.1), P(c.0, c.1)]
        };
        let rings = vec![
            tri((0.0, 0.0), (5.0, 0.0), (0.0, 5.0)),
            tri((5.0, 0.0), (10.0, 0.0), (10.0, 5.0)),
            tri((10.0, 5.0), (10.0, 10.0), (5.0, 10.0)),
            tri((5.0, 10.0), (0.0, 10.0), (0.0, 5.0)),
        ];
        let (occupied, free) = union_and_complement(&rings);
        assert_eq!(occupied.len(), 4, "got {occupied:#?}");
        assert!(
            occupied.iter().all(|c| c.holes.is_empty()),
            "the room is a hole of no component: {occupied:#?}"
        );
        assert_eq!(free.len(), 1, "got {free:#?}");
        assert_eq!(free[0].area_mm2(), 50.0);
        assert_eq!(
            free[0].outer,
            vec![P(0.0, 5.0), P(5.0, 0.0), P(10.0, 5.0), P(5.0, 10.0)]
        );
    }

    #[test]
    fn a_gap_narrower_than_one_weld_is_closed_and_the_error_is_bounded() {
        // Two rectangles 0.4 µm apart. WELD_MM is 1 µm, so their facing edges
        // are one edge and the union is a single component that includes the
        // gap. This is the tolerance behaving as specified, not a defect: below
        // WELD_MM there are no distinct features. What matters is the BOUND —
        // the area wrongly claimed is at most the gap width times its length,
        // and the gap width is under one weld by construction.
        //
        // Raised by adversarial review (Codex, gpt-5.6-sol, 2026-08-31) as a
        // wrong answer. It is a documented consequence; this test pins it so a
        // future change to WELD_MM shows up here rather than as a surprise.
        let out = union(&[
            rect(0.0, 0.0, 1000.0, 1000.0),
            rect(1000.0004, 0.0, 2000.0, 1000.0),
        ]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        let claimed = out[0].area_mm2();
        let real = 1000.0 * 1000.0 + (2000.0 - 1000.0004) * 1000.0;
        let over = claimed - real;
        assert!(over > 0.0, "the gap is claimed as occupied");
        assert!(
            over <= WELD_MM * 1000.0,
            "over-claim {over} exceeds one weld across the contact length"
        );
    }

    #[test]
    fn union_is_invariant_under_ring_reversal() {
        // R3.2.4: derived output is invariant under wall reversal.
        let forward = rect(0.0, 0.0, 10.0, 10.0);
        let reversed: Vec<Point> = forward.iter().rev().copied().collect();
        assert_eq!(union(&[forward]), union(&[reversed]));
    }

    #[test]
    fn union_ignores_a_degenerate_ring() {
        let segment = vec![P(0.0, -1.0), P(0.0, 1.0), P(0.0, -1.0)];
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), segment]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].area_mm2(), 100.0);
    }

    #[test]
    fn bounding_box_pruning_keeps_rings_that_only_touch() {
        // The margin exists for exactly this: bounding boxes that TOUCH
        // without overlapping. The second rectangle meets the first along a
        // PART of its right edge, so that edge must still be cut at y=2 and
        // y=8. Reject the pair and the shared span never acquires a reversed
        // twin, so it survives as boundary and the union stays two rings.
        //
        // Full edge-to-edge contact would not detect this: there the two edges
        // already coincide exactly and cancel with no cutting at all.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 2.0, 20.0, 8.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 100.0 + 60.0);
    }

    #[test]
    fn a_hole_is_charged_to_the_smallest_ring_that_contains_it() {
        // Two nested frames. The inner frame's courtyard lies inside BOTH
        // outer rings; charging it to the outer building would subtract it
        // from the wrong component and leave the inner one overstated.
        let frame = |a: f64, b: f64, t: f64| {
            vec![
                rect(a, a, b, a + t),
                rect(a, b - t, b, b),
                rect(a, a, a + t, b),
                rect(b - t, a, b, b),
            ]
        };
        let mut rings = frame(0.0, 30.0, 4.0);
        rings.extend(frame(8.0, 22.0, 2.0));
        let out = union(&rings);
        assert_eq!(out.len(), 2, "got {out:#?}");

        let outer = &out[0];
        let inner = &out[1];
        assert_eq!(outer.outer[0], P(0.0, 0.0));
        assert_eq!(inner.outer[0], P(8.0, 8.0));
        assert_eq!(outer.holes.len(), 1);
        assert_eq!(inner.holes.len(), 1);
        // Outer frame: 30² less its own 22² courtyard.
        assert_eq!(outer.area_mm2(), 900.0 - 484.0);
        // Inner frame: 14² less its own 10² courtyard.
        assert_eq!(inner.area_mm2(), 196.0 - 100.0);
    }

    #[test]
    fn union_of_nothing_is_nothing() {
        assert!(union(&[]).is_empty());
    }

    #[test]
    fn a_ring_inside_a_hole_stays_a_separate_component() {
        // An island in a courtyard: the frame's hole must NOT swallow it, and
        // the island must not be reported as a hole of the frame.
        let mut rings = vec![
            rect(0.0, 0.0, 20.0, 2.0),
            rect(0.0, 18.0, 20.0, 20.0),
            rect(0.0, 0.0, 2.0, 20.0),
            rect(18.0, 0.0, 20.0, 20.0),
        ];
        rings.push(rect(8.0, 8.0, 12.0, 12.0));
        let out = union(&rings);
        assert_eq!(out.len(), 2, "got {out:#?}");
        // Sorted canonically: the frame starts at (0,0), the island at (8,8).
        assert_eq!(out[0].outer[0], P(0.0, 0.0));
        assert_eq!(out[0].holes.len(), 1);
        assert_eq!(out[1].outer[0], P(8.0, 8.0));
        assert!(out[1].holes.is_empty());
    }

    #[test]
    fn rectangles_touching_at_a_single_point_stay_two_rings() {
        // R5.1.2's pinch: the traversal must split at the shared point rather
        // than producing one figure-eight ring.
        let out = union(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 10.0, 20.0, 20.0)]);
        assert_eq!(out.len(), 2, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 100.0);
        assert_eq!(out[1].area_mm2(), 100.0);
    }

    #[test]
    fn union_of_an_l_shape_traces_the_reflex_corner() {
        let out = union(&[rect(0.0, 0.0, 10.0, 4.0), rect(0.0, 0.0, 4.0, 10.0)]);
        assert_eq!(out.len(), 1, "got {out:#?}");
        assert_eq!(out[0].area_mm2(), 40.0 + 40.0 - 16.0);
        assert_eq!(
            out[0].outer,
            vec![
                P(0.0, 0.0),
                P(10.0, 0.0),
                P(10.0, 4.0),
                P(4.0, 4.0),
                P(4.0, 10.0),
                P(0.0, 10.0),
            ]
        );
    }
}
