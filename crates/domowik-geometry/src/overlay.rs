//! Snap-rounded, labelled planar arrangement (issue 0096).
//!
//! # What this replaces, and why
//!
//! The previous union found crossings on the original segments, moved
//! vertices onto weld representatives without re-noding, classified fragments
//! against the ORIGINAL rings, and traced rings that need not close. Those
//! stages saw three different geometries, and three plain wall rectangles
//! were enough to lose 60% of a union. This module builds ONE embedding and
//! answers every question from it.
//!
//! # Snap rounding
//!
//! Hobby, *Practical segment intersection with finite precision output*,
//! Computational Geometry 13 (1999) 199–214, doi:10.1016/S0925-7721(99)00021-8,
//! implemented from the paper (docs/CLEAN-ROOM-POLICY.md).
//!
//! 1. The plane is divided into half-open cells `[k − ½, k + ½)` of
//!    [`QUANTUM_MM`] = 2⁻¹⁰ mm. Scaling by 1024 is exact in binary64, so every
//!    cell boundary is an exact value and cell membership is decided without
//!    rounding.
//! 2. A cell is HOT when it holds a segment endpoint or a crossing.
//! 3. Every segment is replaced by the polyline through the centres of all hot
//!    cells it meets, in order along it.
//!
//! Hobby's Theorem 4.1: the resulting pieces meet only at common endpoints or
//! coincide. Everything after step 3 is integer arithmetic on cell indices,
//! exact by construction: angular order, face tracing, signed area and point
//! location use `i128` and no tolerance.
//!
//! A crossing is located from its correctly rounded construction
//! ([`crate::arrangement::intersection_point`]). The nearest binary64 to the
//! exact crossing can differ in cell from the exact crossing only by landing
//! exactly ON a cell boundary — every boundary is representable, so a value
//! strictly between would be nearer. That case marks both neighbours. Extra
//! hot cells never break the theorem; a missing one would.
//!
//! # Faces and labels
//!
//! Input rings are counter-clockwise, so each directed edge has its ring's
//! interior on the left. Every ring carries a LABEL, and each face of the
//! arrangement carries the per-label winding number. Crossing a half-edge
//! from its left face to its right face subtracts that half-edge's net
//! weight. A label covers a face when its winding is positive.
//!
//! The union of everything is the set of faces with positive total winding;
//! with one label per join-connected component, R5.1.9's positive-area
//! overlap is a bounded face covered by two labels, decided exactly.
//!
//! # Cost
//!
//! Broad phase is a uniform bucket grid, so isolated walls never meet. The
//! routing work is the number of (segment, hot cell) incidences, which
//! Hobby §2.3 warns can approach `N·(N + K)` for nearly collinear input.
//! Issue 0096 benchmarks that family explicitly.

use crate::arrangement::intersection_point;
use crate::dyadic::Dyadic;
use crate::polygon::{canonical_ring, compare_polygons, compare_rings, Polygon, Winding};
use crate::{orientation, Orientation};
use domowik_format::Point;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Cells per millimetre. A power of two, so scaling is exact.
pub const CELLS_PER_MM: f64 = 1024.0;

/// The topology quantum: one cell, 2⁻¹⁰ mm ≈ 0.977 µm.
pub const QUANTUM_MM: f64 = 1.0 / CELLS_PER_MM;

/// A topology-grid cell. Two points are one vertex iff they share a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cell {
    pub x: i64,
    pub y: i64,
}

impl Cell {
    /// The cell holding `p`: `floor(v · 1024 + ½)` per axis, exactly.
    pub fn of(p: Point) -> Self {
        Self {
            x: cell_index(p.x),
            y: cell_index(p.y),
        }
    }

    /// The cell's centre in millimetres, exact.
    pub fn center(self) -> Point {
        Point::new(self.x as f64 / CELLS_PER_MM, self.y as f64 / CELLS_PER_MM)
    }
}

/// The cell index of one millimetre coordinate, exactly.
pub fn cell_index(v: f64) -> i64 {
    scaled_index(v * CELLS_PER_MM)
}

/// `floor(s + ½)` without the rounding `s + 0.5` can introduce: the
/// fractional part `s − floor(s)` is always exactly representable.
fn scaled_index(s: f64) -> i64 {
    let f = s.floor();
    if s - f >= 0.5 {
        f as i64 + 1
    } else {
        f as i64
    }
}

/// Both candidate cells for a correctly rounded scaled coordinate: one, or two
/// when the value sits exactly on a boundary and the exact one may lie either
/// side of it.
fn scaled_candidates(s: f64) -> (i64, Option<i64>) {
    let k = scaled_index(s);
    if s - s.floor() == 0.5 {
        (k, Some(k - 1))
    } else {
        (k, None)
    }
}

// ---------------------------------------------------------------------------
// Fast hashing for integer keys. Std's SipHash is DoS-resistant, which this
// in-process geometry does not need, and it dominated the old pipeline's maps.

#[derive(Default)]
pub(crate) struct FastHasher(u64);

impl Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }
    fn write_u32(&mut self, v: u32) {
        self.write_u64(v as u64);
    }
    fn write_usize(&mut self, v: usize) {
        self.write_u64(v as u64);
    }
}

pub(crate) type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

// ---------------------------------------------------------------------------
// Input.

/// One input ring with its label and provenance tag.
#[derive(Debug, Clone)]
pub struct InputRing {
    /// Ring vertices in millimetres; any winding, open.
    pub points: Vec<Point>,
    /// Which coverage label this ring contributes to.
    pub label: u32,
    /// Caller-defined provenance carried onto boundary edges.
    pub tag: usize,
}

/// A segment in SCALED coordinates (cell units), exact copies of the input.
#[derive(Debug, Clone, Copy)]
struct Seg {
    a: Point,
    b: Point,
    ring: u32,
}

impl Seg {
    fn bounds(&self) -> [f64; 4] {
        [
            self.a.x.min(self.b.x),
            self.a.y.min(self.b.y),
            self.a.x.max(self.b.x),
            self.a.y.max(self.b.y),
        ]
    }
}

fn scale(p: Point) -> Point {
    Point::new(p.x * CELLS_PER_MM, p.y * CELLS_PER_MM)
}

// ---------------------------------------------------------------------------
// Broad phase.

/// A uniform bucket grid over boxes in scaled units.
struct Buckets {
    size: f64,
    map: FastMap<(i64, i64), Vec<u32>>,
}

impl Buckets {
    /// Sized so the average bucket holds a few boxes: the side is twice the
    /// square root of the scene area per box, never below 64 cells.
    fn sized_for(boxes: &[[f64; 4]]) -> Self {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for b in boxes {
            lo[0] = lo[0].min(b[0]);
            lo[1] = lo[1].min(b[1]);
            hi[0] = hi[0].max(b[2]);
            hi[1] = hi[1].max(b[3]);
        }
        let area = ((hi[0] - lo[0]) * (hi[1] - lo[1])).max(1.0);
        let n = boxes.len().max(1) as f64;
        let size = (2.0 * (area / n).sqrt()).max(64.0);
        Self {
            size,
            map: FastMap::default(),
        }
    }

    fn range(&self, b: &[f64; 4]) -> (i64, i64, i64, i64) {
        let f = |v: f64| (v / self.size).floor() as i64;
        (f(b[0]), f(b[1]), f(b[2]), f(b[3]))
    }

    fn insert(&mut self, id: u32, b: &[f64; 4]) {
        let (x0, y0, x1, y1) = self.range(b);
        for bx in x0..=x1 {
            for by in y0..=y1 {
                self.map.entry((bx, by)).or_default().push(id);
            }
        }
    }

    fn bucket_of(&self, x: f64, y: f64) -> (i64, i64) {
        (
            (x / self.size).floor() as i64,
            (y / self.size).floor() as i64,
        )
    }
}

fn boxes_overlap(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

// ---------------------------------------------------------------------------
// Hot cells.

fn hot_cells(segs: &[Seg]) -> Vec<Cell> {
    let mut hot: Vec<Cell> = Vec::with_capacity(segs.len() * 2);
    for s in segs {
        for p in [s.a, s.b] {
            hot.push(Cell {
                x: scaled_index(p.x),
                y: scaled_index(p.y),
            });
        }
    }

    let boxes: Vec<[f64; 4]> = segs.iter().map(Seg::bounds).collect();
    let mut grid = Buckets::sized_for(&boxes);
    for (i, b) in boxes.iter().enumerate() {
        grid.insert(i as u32, b);
    }
    for (&bucket, ids) in &grid.map {
        for (k, &i) in ids.iter().enumerate() {
            for &j in &ids[k + 1..] {
                let (s, t) = (&segs[i as usize], &segs[j as usize]);
                if s.ring == t.ring {
                    continue;
                }
                let (bi, bj) = (&boxes[i as usize], &boxes[j as usize]);
                if !boxes_overlap(bi, bj) {
                    continue;
                }
                // Each overlapping pair is tested in exactly one bucket: the
                // one holding the lower-left corner of the boxes' overlap.
                if grid.bucket_of(bi[0].max(bj[0]), bi[1].max(bj[1])) != bucket {
                    continue;
                }
                crossing_cells(s, t, &mut hot);
            }
        }
    }
    hot.sort_unstable();
    hot.dedup();
    hot
}

/// Marks the cell of a PROPER crossing of `s` and `t`. Every other contact —
/// an endpoint on the other segment, a collinear overlap — happens at an
/// input endpoint, whose cell is already hot.
fn crossing_cells(s: &Seg, t: &Seg, hot: &mut Vec<Cell>) {
    let o1 = orientation(s.a, s.b, t.a);
    let o2 = orientation(s.a, s.b, t.b);
    if o1 == Orientation::Collinear || o2 == Orientation::Collinear || o1 == o2 {
        return;
    }
    let o3 = orientation(t.a, t.b, s.a);
    let o4 = orientation(t.a, t.b, s.b);
    if o3 == Orientation::Collinear || o4 == Orientation::Collinear || o3 == o4 {
        return;
    }
    // Axis-aligned crossings — most wall geometry — are exact without
    // construction.
    let p = if s.a.y == s.b.y && t.a.x == t.b.x {
        Point::new(t.a.x, s.a.y)
    } else if s.a.x == s.b.x && t.a.y == t.b.y {
        Point::new(s.a.x, t.a.y)
    } else {
        match intersection_point(s.a, s.b, t.a, t.b) {
            Some(p) => p,
            None => return,
        }
    };
    let (x, x2) = scaled_candidates(p.x);
    let (y, y2) = scaled_candidates(p.y);
    for cx in std::iter::once(x).chain(x2) {
        for cy in std::iter::once(y).chain(y2) {
            hot.push(Cell { x: cx, y: cy });
        }
    }
}

// ---------------------------------------------------------------------------
// Routing.

/// Whether segment `a→b` (scaled) meets the half-open cell `c`, exactly.
///
/// Separating axes for a segment against a box are the two box axes and the
/// segment's normal. The half-open box is the limit of the closed box
/// `[x0, x1 − ε] × [y0, y1 − ε]` as ε → 0⁺: its axis tests become strict on
/// the excluded side, and a corner that lies exactly on the segment's line
/// takes the sign of the first-order perturbation.
fn segment_meets_cell(a: Point, b: Point, c: Cell) -> bool {
    let (x0, x1) = (c.x as f64 - 0.5, c.x as f64 + 0.5);
    let (y0, y1) = (c.y as f64 - 0.5, c.y as f64 + 0.5);
    if !(a.x.min(b.x) < x1 && a.x.max(b.x) >= x0) {
        return false;
    }
    if !(a.y.min(b.y) < y1 && a.y.max(b.y) >= y0) {
        return false;
    }
    let mut pos = false;
    let mut neg = false;
    for (cx, cy, sx, sy) in [
        (x0, y0, false, false),
        (x1, y0, true, false),
        (x0, y1, false, true),
        (x1, y1, true, true),
    ] {
        let sign = match orientation(a, b, Point::new(cx, cy)) {
            Orientation::CounterClockwise => 1,
            Orientation::Clockwise => -1,
            Orientation::Collinear => perturbed_sign(a, b, sx, sy),
        };
        match sign {
            0 => return true,
            s if s > 0 => pos = true,
            _ => neg = true,
        }
    }
    pos && neg
}

/// Sign of `orient(a, b, c − ε(sx, sy))` when `orient(a, b, c) = 0`:
/// `sign(sx·(b.y − a.y) − sy·(b.x − a.x))`.
fn perturbed_sign(a: Point, b: Point, sx: bool, sy: bool) -> i32 {
    let ord = match (sx, sy) {
        (false, false) => Ordering::Equal,
        (true, false) => b.y.partial_cmp(&a.y).unwrap_or(Ordering::Equal),
        (false, true) => a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal),
        (true, true) => {
            let d = |v: f64| Dyadic::from_f64(v);
            d(b.y).add(&d(a.x)).cmp(&d(a.y).add(&d(b.x)))
        }
    };
    match ord {
        Ordering::Greater => 1,
        Ordering::Less => -1,
        Ordering::Equal => 0,
    }
}

/// Hot cells met by each segment, ordered along it.
///
/// The order key is exact: half-open cells in different columns are
/// separated in x, so they are met in order of `sign(dx) · column`; two cells
/// in one column are met in order of `sign(dy) · row`.
fn route(segs: &[Seg], hot: &[Cell]) -> Vec<Vec<Cell>> {
    let hot_boxes: Vec<[f64; 4]> = hot
        .iter()
        .map(|c| {
            let (x, y) = (c.x as f64, c.y as f64);
            [x - 0.5, y - 0.5, x + 0.5, y + 0.5]
        })
        .collect();
    let seg_boxes: Vec<[f64; 4]> = segs.iter().map(Seg::bounds).collect();
    let mut sizing = seg_boxes.clone();
    sizing.extend_from_slice(&hot_boxes);
    let mut grid = Buckets::sized_for(&sizing);
    for (i, b) in hot_boxes.iter().enumerate() {
        grid.insert(i as u32, b);
    }

    let mut out = Vec::with_capacity(segs.len());
    let mut seen: Vec<u32> = Vec::new();
    for (s, sb) in segs.iter().zip(&seg_boxes) {
        let (x0, y0, x1, y1) = grid.range(&[sb[0] - 0.5, sb[1] - 0.5, sb[2] + 0.5, sb[3] + 0.5]);
        seen.clear();
        for bx in x0..=x1 {
            for by in y0..=y1 {
                if let Some(ids) = grid.map.get(&(bx, by)) {
                    for &id in ids {
                        let c = hot[id as usize];
                        // A cell spanning several buckets is reported once per
                        // bucket; the sort below removes duplicates.
                        if boxes_overlap(&hot_boxes[id as usize], sb)
                            && segment_meets_cell(s.a, s.b, c)
                        {
                            seen.push(id);
                        }
                    }
                }
            }
        }
        let sx = sign_of(s.b.x, s.a.x);
        let sy = sign_of(s.b.y, s.a.y);
        let mut cells: Vec<Cell> = seen.iter().map(|&i| hot[i as usize]).collect();
        cells.sort_unstable_by_key(|c| (sx * c.x, sy * c.y));
        cells.dedup();
        debug_assert_eq!(
            cells.first().copied(),
            Some(Cell {
                x: scaled_index(s.a.x),
                y: scaled_index(s.a.y)
            })
        );
        debug_assert_eq!(
            cells.last().copied(),
            Some(Cell {
                x: scaled_index(s.b.x),
                y: scaled_index(s.b.y)
            })
        );
        out.push(cells);
    }
    out
}

fn sign_of(to: f64, from: f64) -> i64 {
    match to.partial_cmp(&from) {
        Some(Ordering::Greater) => 1,
        Some(Ordering::Less) => -1,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Integer geometry.

fn cross(o: Cell, a: Cell, b: Cell) -> i128 {
    let (ax, ay) = ((a.x - o.x) as i128, (a.y - o.y) as i128);
    let (bx, by) = ((b.x - o.x) as i128, (b.y - o.y) as i128);
    ax * by - ay * bx
}

/// Angular order of direction vectors, counter-clockwise from +x.
fn angle_cmp(a: (i64, i64), b: (i64, i64)) -> Ordering {
    let half = |d: (i64, i64)| {
        if d.1 > 0 || (d.1 == 0 && d.0 > 0) {
            0
        } else {
            1
        }
    };
    half(a).cmp(&half(b)).then_with(|| {
        let c = a.0 as i128 * b.1 as i128 - a.1 as i128 * b.0 as i128;
        0.cmp(&c)
    })
}

/// Twice the signed area of a ring of cells.
fn double_area(ring: &[Cell]) -> i128 {
    let o = ring[0];
    let mut sum = 0i128;
    for i in 1..ring.len().saturating_sub(1) {
        sum += cross(o, ring[i], ring[i + 1]);
    }
    sum
}

/// Where a point lies relative to a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Inside,
    Boundary,
    Outside,
}

/// Classifies a point given in DOUBLED cell coordinates against a ring of
/// cells, exactly. Doubling admits edge midpoints as query points.
fn classify_doubled(p: (i128, i128), ring: &[Cell]) -> Side {
    let n = ring.len();
    let mut winding = 0i32;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (ax, ay) = (2 * a.x as i128, 2 * a.y as i128);
        let (bx, by) = (2 * b.x as i128, 2 * b.y as i128);
        let c = (bx - ax) * (p.1 - ay) - (by - ay) * (p.0 - ax);
        if c == 0
            && p.0 >= ax.min(bx)
            && p.0 <= ax.max(bx)
            && p.1 >= ay.min(by)
            && p.1 <= ay.max(by)
        {
            return Side::Boundary;
        }
        if ay <= p.1 {
            if by > p.1 && c > 0 {
                winding += 1;
            }
        } else if by <= p.1 && c < 0 {
            winding -= 1;
        }
    }
    if winding != 0 {
        Side::Inside
    } else {
        Side::Outside
    }
}

/// Whether ring `inner` lies within the closed disk of ring `outer`, with some
/// point strictly inside. Both rings must come from one arrangement, so they
/// meet only at shared vertices or shared edges: then an edge's open interior
/// lies wholly on one side, and its midpoint decides it.
pub(crate) fn ring_within(inner: &[Cell], outer: &[Cell]) -> bool {
    let mut strictly = false;
    let n = inner.len();
    for i in 0..n {
        let (a, b) = (inner[i], inner[(i + 1) % n]);
        for p in [
            (2 * a.x as i128, 2 * a.y as i128),
            (a.x as i128 + b.x as i128, a.y as i128 + b.y as i128),
        ] {
            match classify_doubled(p, outer) {
                Side::Outside => return false,
                Side::Inside => strictly = true,
                Side::Boundary => {}
            }
        }
    }
    strictly
}

// ---------------------------------------------------------------------------
// The arrangement.

/// Sparse per-label weights or windings, sorted by label, zeros omitted.
type Weights = Vec<(u32, i32)>;

fn add_into(acc: &mut Weights, label: u32, w: i32) {
    match acc.binary_search_by_key(&label, |e| e.0) {
        Ok(i) => {
            acc[i].1 += w;
            if acc[i].1 == 0 {
                acc.remove(i);
            }
        }
        Err(i) => acc.insert(i, (label, w)),
    }
}

/// `a − sign·b`, both sparse.
fn combine(a: &Weights, b: &Weights, sign: i32) -> Weights {
    let mut out = a.clone();
    for &(l, w) in b {
        add_into(&mut out, l, -sign * w);
    }
    out
}

fn winding_of(w: &Weights, label: u32) -> i32 {
    w.binary_search_by_key(&label, |e| e.0)
        .map(|i| w[i].1)
        .unwrap_or(0)
}

/// A snap-rounded arrangement with per-face label windings.
#[derive(Debug, Clone)]
pub struct Arrangement {
    verts: Vec<Cell>,
    /// Half-edge `2e` runs `ends[e].0 → ends[e].1`; `2e + 1` is its twin.
    ends: Vec<(u32, u32)>,
    /// Net label weights of edge `e` in its `2e` direction.
    weights: Vec<Weights>,
    /// Provenance tags of pieces running in each half-edge's direction.
    tags: Vec<Vec<usize>>,
    /// Outgoing half-edges per vertex, counter-clockwise.
    out: Vec<Vec<u32>>,
    /// Index of each half-edge within its origin's `out` list.
    pos: Vec<u32>,
    /// Face to the left of each half-edge.
    face_of: Vec<u32>,
    /// One half-edge per face.
    face_edge: Vec<u32>,
    /// Twice the signed area of each face's boundary cycle.
    face_area: Vec<i128>,
    /// Per-label winding of each face.
    face_winding: Vec<Weights>,
}

impl Arrangement {
    /// Builds the arrangement of `rings`.
    ///
    /// Each ring is normalized to counter-clockwise; a ring enclosing no area
    /// contributes nothing (R3.2.3's degenerate join patch). `track_tags`
    /// keeps per-half-edge provenance, which costs one allocation per edge.
    pub fn build(rings: &[InputRing], track_tags: bool) -> Self {
        let mut segs: Vec<Seg> = Vec::new();
        let mut seg_label: Vec<(u32, usize)> = Vec::new();
        for (r, ring) in rings.iter().enumerate() {
            let Some(pts) = canonical_ring(&ring.points, Winding::CounterClockwise) else {
                continue;
            };
            for i in 0..pts.len() {
                let (a, b) = (scale(pts[i]), scale(pts[(i + 1) % pts.len()]));
                segs.push(Seg {
                    a,
                    b,
                    ring: r as u32,
                });
                seg_label.push((ring.label, ring.tag));
            }
        }

        let hot = hot_cells(&segs);
        let routes = route(&segs, &hot);

        let mut vindex: FastMap<Cell, u32> = FastMap::default();
        let mut verts: Vec<Cell> = Vec::new();
        let mut vid = |c: Cell, verts: &mut Vec<Cell>| -> u32 {
            *vindex.entry(c).or_insert_with(|| {
                verts.push(c);
                (verts.len() - 1) as u32
            })
        };
        let mut eindex: FastMap<(u32, u32), u32> = FastMap::default();
        let mut ends: Vec<(u32, u32)> = Vec::new();
        let mut weights: Vec<Weights> = Vec::new();
        let mut tags: Vec<Vec<usize>> = Vec::new();
        for (cells, &(label, tag)) in routes.iter().zip(&seg_label) {
            for w in cells.windows(2) {
                let (u, v) = (vid(w[0], &mut verts), vid(w[1], &mut verts));
                let (key, dir) = if verts[u as usize] < verts[v as usize] {
                    ((u, v), 1)
                } else {
                    ((v, u), -1)
                };
                let e = *eindex.entry(key).or_insert_with(|| {
                    ends.push(key);
                    weights.push(Vec::new());
                    if track_tags {
                        tags.push(Vec::new());
                        tags.push(Vec::new());
                    }
                    (ends.len() - 1) as u32
                });
                add_into(&mut weights[e as usize], label, dir);
                if track_tags {
                    let h = 2 * e as usize + if dir == 1 { 0 } else { 1 };
                    tags[h].push(tag);
                }
            }
        }

        // Edges whose every label cancels separate faces of equal winding and
        // are dropped; this is what removes back-to-back ring edges.
        let keep: Vec<bool> = weights.iter().map(|w| !w.is_empty()).collect();
        let mut remap = vec![u32::MAX; ends.len()];
        let mut kept_ends = Vec::new();
        let mut kept_weights = Vec::new();
        let mut kept_tags = Vec::new();
        for e in 0..ends.len() {
            if keep[e] {
                remap[e] = kept_ends.len() as u32;
                kept_ends.push(ends[e]);
                kept_weights.push(std::mem::take(&mut weights[e]));
                if track_tags {
                    let mut f = std::mem::take(&mut tags[2 * e]);
                    let mut r = std::mem::take(&mut tags[2 * e + 1]);
                    f.sort_unstable();
                    f.dedup();
                    r.sort_unstable();
                    r.dedup();
                    kept_tags.push(f);
                    kept_tags.push(r);
                }
            }
        }
        let ends = kept_ends;
        let weights = kept_weights;
        let tags = kept_tags;

        let nh = ends.len() * 2;
        let origin = |h: usize| -> u32 {
            let (u, v) = ends[h / 2];
            if h.is_multiple_of(2) {
                u
            } else {
                v
            }
        };
        let mut out: Vec<Vec<u32>> = vec![Vec::new(); verts.len()];
        for h in 0..nh {
            out[origin(h) as usize].push(h as u32);
        }
        let dir = |h: u32| -> (i64, i64) {
            let (u, v) = ends[h as usize / 2];
            let (a, b) = if h.is_multiple_of(2) { (u, v) } else { (v, u) };
            let (a, b) = (verts[a as usize], verts[b as usize]);
            (b.x - a.x, b.y - a.y)
        };
        let mut pos = vec![0u32; nh];
        for list in &mut out {
            list.sort_unstable_by(|&p, &q| angle_cmp(dir(p), dir(q)));
            for (i, &h) in list.iter().enumerate() {
                pos[h as usize] = i as u32;
            }
        }

        let mut arr = Self {
            verts,
            ends,
            weights,
            tags,
            out,
            pos,
            face_of: vec![u32::MAX; nh],
            face_edge: Vec::new(),
            face_area: Vec::new(),
            face_winding: Vec::new(),
        };
        arr.trace_faces();
        arr.assign_windings();
        arr
    }

    fn origin(&self, h: u32) -> u32 {
        let (u, v) = self.ends[h as usize / 2];
        if h.is_multiple_of(2) {
            u
        } else {
            v
        }
    }

    fn dest(&self, h: u32) -> u32 {
        self.origin(h ^ 1)
    }

    /// The next half-edge around the face on `h`'s left: at its destination,
    /// the first outgoing half-edge clockwise from the way back.
    fn next(&self, h: u32) -> u32 {
        let v = self.dest(h) as usize;
        let list = &self.out[v];
        let i = self.pos[(h ^ 1) as usize] as usize;
        list[(i + list.len() - 1) % list.len()]
    }

    fn half_weight(&self, h: u32) -> Weights {
        let w = &self.weights[h as usize / 2];
        if h.is_multiple_of(2) {
            w.clone()
        } else {
            w.iter().map(|&(l, x)| (l, -x)).collect()
        }
    }

    fn trace_faces(&mut self) {
        let nh = self.ends.len() * 2;
        for start in 0..nh as u32 {
            if self.face_of[start as usize] != u32::MAX {
                continue;
            }
            let f = self.face_edge.len() as u32;
            let mut ring = Vec::new();
            let mut h = start;
            loop {
                self.face_of[h as usize] = f;
                ring.push(self.verts[self.origin(h) as usize]);
                h = self.next(h);
                if h == start {
                    break;
                }
            }
            self.face_edge.push(start);
            self.face_area.push(double_area(&ring));
        }
    }

    /// Winding of every face. Within one connected piece of the graph,
    /// windings propagate across half-edges from its outer face; the outer
    /// face's own winding is the winding of a vertex of the piece with respect
    /// to every OTHER piece, which no other piece's edge passes through.
    fn assign_windings(&mut self) {
        let nv = self.verts.len();
        let nf = self.face_edge.len();
        self.face_winding = vec![Vec::new(); nf];
        if nf == 0 {
            return;
        }

        let mut parent: Vec<u32> = (0..nv as u32).collect();
        fn find(p: &mut [u32], mut x: u32) -> u32 {
            while p[x as usize] != x {
                p[x as usize] = p[p[x as usize] as usize];
                x = p[x as usize];
            }
            x
        }
        for &(u, v) in &self.ends {
            let (a, b) = (find(&mut parent, u), find(&mut parent, v));
            if a != b {
                parent[a as usize] = b;
            }
        }
        let mut piece_of_vertex = vec![u32::MAX; nv];
        let mut pieces: Vec<Vec<u32>> = Vec::new();
        let mut root_index: FastMap<u32, u32> = FastMap::default();
        for v in 0..nv as u32 {
            if self.out[v as usize].is_empty() {
                continue;
            }
            let r = find(&mut parent, v);
            let i = *root_index.entry(r).or_insert_with(|| {
                pieces.push(Vec::new());
                (pieces.len() - 1) as u32
            });
            piece_of_vertex[v as usize] = i;
        }
        for (e, &(u, _)) in self.ends.iter().enumerate() {
            pieces[piece_of_vertex[u as usize] as usize].push(e as u32);
        }

        let mut bbox: Vec<[i64; 4]> = vec![[i64::MAX, i64::MAX, i64::MIN, i64::MIN]; pieces.len()];
        for (i, edges) in pieces.iter().enumerate() {
            for &e in edges {
                let (u, v) = self.ends[e as usize];
                for c in [self.verts[u as usize], self.verts[v as usize]] {
                    let b = &mut bbox[i];
                    b[0] = b[0].min(c.x);
                    b[1] = b[1].min(c.y);
                    b[2] = b[2].max(c.x);
                    b[3] = b[3].max(c.y);
                }
            }
        }

        // The outer face of each piece is its most negative cycle.
        let mut outer_face = vec![u32::MAX; pieces.len()];
        for f in 0..nf {
            let h = self.face_edge[f];
            let p = piece_of_vertex[self.origin(h) as usize] as usize;
            let cur = outer_face[p];
            if cur == u32::MAX || self.face_area[f] < self.face_area[cur as usize] {
                outer_face[p] = f as u32;
            }
        }

        let mut assigned = vec![false; nf];
        // Only a piece whose bounds hold the probe can wind around it; the
        // bucket grid finds those without testing every pair of pieces.
        let fboxes: Vec<[f64; 4]> = bbox
            .iter()
            .map(|b| [b[0] as f64, b[1] as f64, b[2] as f64, b[3] as f64])
            .collect();
        let mut grid = Buckets::sized_for(&fboxes);
        for (q, b) in fboxes.iter().enumerate() {
            grid.insert(q as u32, b);
        }

        let mut queue: Vec<u32> = Vec::new();
        for (p, edges) in pieces.iter().enumerate() {
            let probe = self.verts[self.ends[edges[0] as usize].0 as usize];
            let mut w0: Weights = Vec::new();
            let near = grid
                .map
                .get(&grid.bucket_of(probe.x as f64, probe.y as f64))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            for &q in near {
                let q = q as usize;
                if q == p {
                    continue;
                }
                let b = bbox[q];
                if probe.x < b[0] || probe.x > b[2] || probe.y < b[1] || probe.y > b[3] {
                    continue;
                }
                for &e in &pieces[q] {
                    self.accumulate_crossing(e, probe, &mut w0);
                }
            }
            let f0 = outer_face[p];
            self.face_winding[f0 as usize] = w0;
            assigned[f0 as usize] = true;
            queue.push(f0);
            while let Some(f) = queue.pop() {
                let start = self.face_edge[f as usize];
                let mut h = start;
                loop {
                    let g = self.face_of[(h ^ 1) as usize];
                    if !assigned[g as usize] {
                        let w = combine(&self.face_winding[f as usize], &self.half_weight(h), 1);
                        self.face_winding[g as usize] = w;
                        assigned[g as usize] = true;
                        queue.push(g);
                    }
                    h = self.next(h);
                    if h == start {
                        break;
                    }
                }
            }
        }
    }

    /// Adds edge `e`'s contribution to the winding of point `p`, which lies on
    /// no edge of `e`'s piece. Half-open in y, exact.
    fn accumulate_crossing(&self, e: u32, p: Cell, acc: &mut Weights) {
        let (u, v) = self.ends[e as usize];
        let (a, b) = (self.verts[u as usize], self.verts[v as usize]);
        let c = cross(a, b, p);
        let sign = if a.y <= p.y {
            if b.y > p.y && c > 0 {
                1
            } else {
                0
            }
        } else if b.y <= p.y && c < 0 {
            -1
        } else {
            0
        };
        if sign != 0 {
            for &(l, w) in &self.weights[e as usize] {
                add_into(acc, l, sign * w);
            }
        }
    }

    /// Number of faces.
    pub fn face_count(&self) -> usize {
        self.face_edge.len()
    }

    /// True for a bounded face (positive area); the outer face of each
    /// connected piece is not.
    pub fn is_bounded(&self, f: usize) -> bool {
        self.face_area[f] > 0
    }

    /// Labels covering face `f` (winding > 0), ascending.
    pub fn covering(&self, f: usize) -> impl Iterator<Item = u32> + '_ {
        self.face_winding[f].iter().filter(|e| e.1 > 0).map(|e| e.0)
    }

    /// Winding of `label` on face `f`.
    pub fn winding(&self, f: usize, label: u32) -> i32 {
        winding_of(&self.face_winding[f], label)
    }

    /// Boundary rings of the region `{faces where inside(face)}`, each with
    /// the region on its left, as cell rings with per-edge tags. Every walk
    /// closes; an open one is an invariant violation.
    pub(crate) fn boundary<F: Fn(usize) -> bool>(
        &self,
        candidates: impl Iterator<Item = u32>,
        inside: F,
    ) -> Vec<(Vec<Cell>, Vec<Vec<usize>>)> {
        let is_boundary = |h: u32| {
            inside(self.face_of[h as usize] as usize)
                && !inside(self.face_of[(h ^ 1) as usize] as usize)
        };
        let mut starts: Vec<u32> = candidates.filter(|&h| is_boundary(h)).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut visited: FastMap<u32, ()> = FastMap::default();
        let mut rings = Vec::new();
        for start in starts {
            if visited.contains_key(&start) {
                continue;
            }
            let mut cells = Vec::new();
            let mut runs = Vec::new();
            let mut h = start;
            loop {
                visited.insert(h, ());
                cells.push(self.verts[self.origin(h) as usize]);
                runs.push(self.tags.get(h as usize).cloned().unwrap_or_default());
                // Clockwise from the way back, skipping half-edges that are
                // not on this region's boundary.
                let v = self.dest(h) as usize;
                let list = &self.out[v];
                let mut i = self.pos[(h ^ 1) as usize] as usize;
                let mut next = None;
                for _ in 0..list.len() {
                    i = (i + list.len() - 1) % list.len();
                    if is_boundary(list[i]) {
                        next = Some(list[i]);
                        break;
                    }
                }
                let next = next.expect("region boundary is closed at every vertex");
                if next == start {
                    break;
                }
                assert!(
                    !visited.contains_key(&next),
                    "region boundary walk revisited a half-edge without closing"
                );
                h = next;
            }
            rings.push((cells, runs));
        }
        rings
    }

    /// Half-edges carrying each label `0..n`, grouped in one pass.
    pub(crate) fn half_edges_by_label(&self, n: usize) -> Vec<Vec<u32>> {
        let mut out = vec![Vec::new(); n];
        for (e, w) in self.weights.iter().enumerate() {
            for &(l, _) in w {
                if let Some(list) = out.get_mut(l as usize) {
                    list.push(2 * e as u32);
                    list.push(2 * e as u32 + 1);
                }
            }
        }
        out
    }

    /// Every half-edge.
    pub(crate) fn all_half_edges(&self) -> std::ops::Range<u32> {
        0..(self.ends.len() * 2) as u32
    }
}

// ---------------------------------------------------------------------------
// Rings to polygons.

/// A canonical ring of cells with one run of provenance per canonical edge.
#[derive(Debug, Clone)]
pub(crate) struct CellRing {
    pub cells: Vec<Cell>,
    /// Per canonical edge: interior break cells, then one tag list per piece.
    pub runs: Vec<(Vec<Cell>, Vec<Vec<usize>>)>,
}

/// Removes collinear vertices, forces `ccw`, rotates to the least cell.
fn canonical_cells(cells: Vec<Cell>, tags: Vec<Vec<usize>>, ccw: bool) -> Option<CellRing> {
    let mut cells = cells;
    let mut runs: Vec<(Vec<Cell>, Vec<Vec<usize>>)> =
        tags.into_iter().map(|t| (Vec::new(), vec![t])).collect();
    let mut i = 0;
    while i < cells.len() && cells.len() >= 3 {
        let n = cells.len();
        if cross(cells[(i + n - 1) % n], cells[i], cells[(i + 1) % n]) == 0 {
            // Removing vertex i joins the edge ending there to the edge
            // leaving it; the vertex becomes an interior break.
            let removed = runs.remove(i);
            let vertex = cells.remove(i);
            let target = if i == 0 { runs.len() - 1 } else { i - 1 };
            runs[target].0.push(vertex);
            runs[target].0.extend(removed.0);
            runs[target].1.extend(removed.1);
        } else {
            i += 1;
        }
    }
    if cells.len() < 3 {
        return None;
    }
    let area = double_area(&cells);
    if area == 0 {
        return None;
    }
    if (area > 0) != ccw {
        cells.reverse();
        for run in &mut runs {
            run.0.reverse();
            run.1.reverse();
        }
        runs.reverse();
        runs.rotate_left(1);
    }
    let best = (0..cells.len()).min_by_key(|&i| cells[i]).unwrap_or(0);
    cells.rotate_left(best);
    runs.rotate_left(best);
    Some(CellRing { cells, runs })
}

/// A polygon of cell rings.
#[derive(Debug, Clone)]
pub(crate) struct CellPolygon {
    pub outer: CellRing,
    pub holes: Vec<CellRing>,
}

/// Sorts traced boundary rings into outers and holes. A hole belongs to the
/// smallest outer that encloses it; one enclosed by none (a courtyard closed
/// only by pinch points) bounds no polygon of this region and is dropped.
pub(crate) fn assemble(rings: Vec<(Vec<Cell>, Vec<Vec<usize>>)>) -> Vec<CellPolygon> {
    let mut outers: Vec<(CellRing, i128, [i64; 4])> = Vec::new();
    let mut holes: Vec<CellRing> = Vec::new();
    for (cells, tags) in rings {
        let area = double_area(&cells);
        if area > 0 {
            if let Some(r) = canonical_cells(cells, tags, true) {
                let a = double_area(&r.cells);
                let b = cell_bounds(&r.cells);
                outers.push((r, a, b));
            }
        } else if area < 0 {
            if let Some(r) = canonical_cells(cells, tags, false) {
                holes.push(r);
            }
        }
    }
    let mut order: Vec<usize> = (0..outers.len()).collect();
    order.sort_by_key(|&i| outers[i].1);
    let mut polygons: Vec<CellPolygon> = outers
        .iter()
        .map(|(r, _, _)| CellPolygon {
            outer: r.clone(),
            holes: Vec::new(),
        })
        .collect();
    for hole in holes {
        let hb = cell_bounds(&hole.cells);
        for &i in &order {
            let ob = outers[i].2;
            if hb[0] < ob[0] || hb[1] < ob[1] || hb[2] > ob[2] || hb[3] > ob[3] {
                continue;
            }
            if ring_within(&hole.cells, &outers[i].0.cells) {
                polygons[i].holes.push(hole);
                break;
            }
        }
    }
    for p in &mut polygons {
        p.holes.sort_by(|a, b| a.cells.cmp(&b.cells));
    }
    polygons.sort_by(|a, b| {
        a.outer.cells.cmp(&b.outer.cells).then_with(|| {
            for (x, y) in a.holes.iter().zip(&b.holes) {
                match x.cells.cmp(&y.cells) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            a.holes.len().cmp(&b.holes.len())
        })
    });
    polygons
}

pub(crate) fn cell_bounds(cells: &[Cell]) -> [i64; 4] {
    let mut b = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
    for c in cells {
        b[0] = b[0].min(c.x);
        b[1] = b[1].min(c.y);
        b[2] = b[2].max(c.x);
        b[3] = b[3].max(c.y);
    }
    b
}

pub(crate) fn to_points(cells: &[Cell]) -> Vec<Point> {
    cells.iter().map(|c| c.center()).collect()
}

impl CellPolygon {
    pub(crate) fn to_polygon(&self) -> Polygon {
        Polygon {
            outer: to_points(&self.outer.cells),
            holes: self.holes.iter().map(|h| to_points(&h.cells)).collect(),
        }
    }
}

/// Cell-ring order agrees with point order: centres are a monotone image.
#[allow(dead_code)]
fn order_is_monotone(a: &[Cell], b: &[Cell]) -> bool {
    a.cmp(b) == compare_rings(&to_points(a), &to_points(b))
}

#[allow(dead_code)]
fn polygon_order(a: &Polygon, b: &Polygon) -> Ordering {
    compare_polygons(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    #[test]
    fn cell_index_is_half_open_and_exact() {
        let q = QUANTUM_MM;
        assert_eq!(cell_index(0.0), 0);
        assert_eq!(
            cell_index(0.5 * q),
            1,
            "upper boundary belongs to the next cell"
        );
        assert_eq!(
            cell_index(-0.5 * q),
            0,
            "lower boundary belongs to this cell"
        );
        assert_eq!(cell_index(0.4999 * q), 0);
        assert_eq!(cell_index(-0.5001 * q), -1);
        assert_eq!(cell_index(4000.0), 4_096_000);
    }

    #[test]
    fn segment_cell_test_respects_half_open_edges() {
        // A horizontal segment along y = 0.5 lies on the boundary shared by
        // rows 0 and 1; it belongs to row 1 only.
        let (a, b) = (P(-3.0, 0.5), P(3.0, 0.5));
        assert!(segment_meets_cell(a, b, Cell { x: 0, y: 1 }));
        assert!(!segment_meets_cell(a, b, Cell { x: 0, y: 0 }));
        // A diagonal through the corner (0.5, 0.5) meets cells (0,0) and (1,1)
        // but not the off-diagonal neighbours.
        let (a, b) = (P(-2.0, -2.0), P(3.0, 3.0));
        assert!(segment_meets_cell(a, b, Cell { x: 0, y: 0 }));
        assert!(segment_meets_cell(a, b, Cell { x: 1, y: 1 }));
        assert!(!segment_meets_cell(a, b, Cell { x: 1, y: 0 }));
        assert!(!segment_meets_cell(a, b, Cell { x: 0, y: 1 }));
    }

    #[test]
    fn angular_order_is_counter_clockwise_from_positive_x() {
        let mut d = vec![(0, -1), (-1, 0), (1, 0), (0, 1), (1, 1)];
        d.sort_by(|a, b| angle_cmp(*a, *b));
        assert_eq!(d, vec![(1, 0), (1, 1), (0, 1), (-1, 0), (0, -1)]);
    }

    #[test]
    fn edge_midpoints_decide_an_inscribed_diamond() {
        let square = vec![
            Cell { x: 0, y: 0 },
            Cell { x: 4, y: 0 },
            Cell { x: 4, y: 4 },
            Cell { x: 0, y: 4 },
        ];
        let diamond = vec![
            Cell { x: 2, y: 0 },
            Cell { x: 4, y: 2 },
            Cell { x: 2, y: 4 },
            Cell { x: 0, y: 2 },
        ];
        assert!(ring_within(&diamond, &square));
        assert!(!ring_within(&square, &diamond));
    }
}
