//! Opening cuts: the occupied region amended by apertures (BEHAVIOR-SPEC
//! R4.2.1–R4.2.5, issue 0013).
//!
//! # The rule
//!
//! Each wall contributes its closed butt-capped rectangle MINUS the union of
//! the OPEN interiors of its OWN apertures' footprints; join patches are
//! united afterwards and never subtracted (R4.2.2). The result — the DRAWN
//! region — is what the editor, exports and the §4 fixtures show. Rooms,
//! snapping and hit testing use the UNAMENDED region ([`crate::walls::plan`]):
//! openings never alter room enclosure (R4.2.5), and aperture corners never
//! enter the unamended arrangement's hot cells, where they could reroute a
//! room boundary.
//!
//! # Output is regularized
//!
//! The literal closed-set difference keeps isolated boundary lines across a
//! full-thickness gap; the §4 fixtures omit them. The drawn region is the
//! regularized result: positive-area material only, its own boundaries
//! preserved, no epsilon (decision recorded in issue 0013).
//!
//! # How (Codex GPT-6-Astra's design)
//!
//! In a host's frame — `s` along the axis as a fraction of its length, `η`
//! across it with `+η` the `Left` face (R4.2.1) — every cut touches a face,
//! so the cuts covering a stretch of the axis remove `[t/2 − L, t/2]` and
//! `[−t/2, −t/2 + R]` for the greatest active Left depth `L` and Right depth
//! `R`. One sweep over the sorted span events (`O(m log m)` for `m` cuts)
//! gives the surviving interval `[−t/2 + R, t/2 − L]` per slab; equal slabs
//! coalesce. Slabs whose intervals overlap with positive length form one
//! simple step-shaped contour, traced in the frame (no internal seams) and
//! transformed to plan coordinates once. Seams matter: a tile boundary would
//! add hot cells to the snap-rounded arrangement (ED-0110) and bend the
//! outline; Astra's probe showed a rectangle split in two gaining two
//! vertices. A host with no effective cut contributes exactly its
//! `wall_polygon` rectangle.

use crate::polygon::Polygon;
use crate::walls::{self, JoinSpec, LevelSolids, WallSolid};
use domowik_format::{Document, Opening, Point, Side};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Paint-only source contours, using the same millimetre wall pieces and join
/// patches as the drawn region. No topology changes or new predicates. Empty
/// unless at least one wall has a colour. Walls paint in descending UUID order
/// (lowest wins overlap); a wall whose faces differ paints each half of its
/// thickness with that face's colour. Join patches split into wedges between
/// member axes; each takes the lower-UUID bounding face's colour. Callers clip
/// these contours to the canonical drawn region. Output is owned,
/// deterministic and bounded by the existing wall/opening input limits.
pub struct WallPaint {
    pub color: Option<domowik_format::appearance::RgbColor>,
    pub outer: Vec<[f64; 2]>,
}

pub fn wall_paints(doc: &Document, level: Uuid) -> Vec<WallPaint> {
    use domowik_format::appearance::wall_faces;
    if !doc
        .walls_on_level(level)
        .any(|w| wall_faces(w).iter().any(Option::is_some))
    {
        return vec![];
    }
    let solids = walls::level_solids(doc, level);
    let faces: Vec<_> = solids
        .ids
        .iter()
        .map(|id| doc.wall(*id).map(wall_faces).unwrap_or_default())
        .collect();
    let mut ordered: Vec<_> = solids.ids.iter().enumerate().collect();
    ordered.sort_by_key(|(_, id)| std::cmp::Reverse(**id));
    let mut result = vec![];
    let mut push = |color, ring: Vec<Point>| {
        result.push(WallPaint {
            color,
            outer: ring.into_iter().map(Into::into).collect(),
        })
    };
    for (i, id) in ordered {
        let cuts: Vec<_> = doc
            .openings_on_wall(*id)
            .into_iter()
            .filter_map(|o| Aperture::of(o, i, &solids.walls[i]))
            .collect();
        let [left, right] = faces[i];
        if left == right {
            for ring in wall_pieces(&solids.walls[i], &cuts) {
                push(left, ring);
            }
        } else {
            for (side, color) in [(Side::Left, left), (Side::Right, right)] {
                for ring in wall_side_pieces(&solids.walls[i], &cuts, side) {
                    push(color, ring);
                }
            }
        }
    }
    for join in &solids.joins {
        for wedge in walls::join_wedges(&solids.walls, join) {
            let (wall, side) = *wedge
                .faces
                .iter()
                .min_by_key(|(w, _)| solids.ids[*w])
                .expect("two faces");
            let color = faces[wall][usize::from(side == Side::Right)];
            for ring in wedge.pieces {
                push(color, ring);
            }
        }
    }
    result
}

/// One aperture's footprint in its host's frame (R4.2.1): the along-axis
/// span as fractions of the host length, the face it opens from and its
/// depth (clamped to the host thickness).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aperture {
    /// Index of the host in the `walls` slice.
    pub wall: usize,
    pub s: (f64, f64),
    pub side: Side,
    pub depth: f64,
}

impl Aperture {
    /// `opening`'s footprint on `host` (index `wall`); `None` when the host
    /// has no length.
    pub fn of(opening: &Opening, wall: usize, host: &WallSolid) -> Option<Self> {
        Some(Self {
            wall,
            s: opening.span(host.length_mm())?,
            side: opening.side,
            depth: opening.depth_mm.clamp(0.0, host.thickness_mm),
        })
    }

    /// The across-thickness span `[η0, η1]` on a host of thickness `t`.
    fn eta(&self, t: f64) -> (f64, f64) {
        match self.side {
            Side::Left => (t / 2.0 - self.depth, t / 2.0),
            Side::Right => (-t / 2.0, -t / 2.0 + self.depth),
        }
    }

    /// The footprint's four plan corners on `host` (R4.2.1), counter-
    /// clockwise. `None` for a host without length.
    pub fn footprint(&self, host: &WallSolid) -> Option<[Point; 4]> {
        let frame = Frame::of(host)?;
        let ((s0, s1), (e0, e1)) = (self.s, self.eta(host.thickness_mm));
        Some([
            frame.at(s0, e0),
            frame.at(s1, e0),
            frame.at(s1, e1),
            frame.at(s0, e1),
        ])
    }
}

/// A host's axis frame: `at(s, η)` is the plan point at fraction `s` along
/// the axis and `η` mm toward the Left face, computed as `wall_polygon`
/// computes its corners so an uncut corner is the rectangle's exactly.
struct Frame {
    start: Point,
    end: Point,
    d: (f64, f64),
    len: f64,
}

impl Frame {
    fn of(w: &WallSolid) -> Option<Self> {
        let d = (w.end.x - w.start.x, w.end.y - w.start.y);
        let len = d.0.hypot(d.1);
        (len.is_finite() && len != 0.0).then_some(Self {
            start: w.start,
            end: w.end,
            d,
            len,
        })
    }

    fn at(&self, s: f64, eta: f64) -> Point {
        let p = if s == 0.0 {
            self.start
        } else if s == 1.0 {
            self.end
        } else {
            Point::new(self.start.x + self.d.0 * s, self.start.y + self.d.1 * s)
        };
        Point::new(
            p.x + -self.d.1 / self.len * eta,
            p.y + self.d.0 / self.len * eta,
        )
    }
}

/// A multiset of non-negative depths with its maximum; `f64` bits order
/// non-negative floats correctly.
#[derive(Default)]
struct Depths(BTreeMap<u64, usize>);

impl Depths {
    fn add(&mut self, d: f64) {
        *self.0.entry(d.to_bits()).or_default() += 1;
    }

    fn remove(&mut self, d: f64) {
        let key = d.to_bits();
        let n = self.0.get_mut(&key).expect("removing an added depth");
        *n -= 1;
        if *n == 0 {
            self.0.remove(&key);
        }
    }

    fn max(&self) -> f64 {
        self.0
            .keys()
            .next_back()
            .map_or(0.0, |&b| f64::from_bits(b))
    }
}

/// One coalesced stretch of a host: `[from, to]` along the axis, surviving
/// `[lo, hi]` across it (empty when `hi <= lo`).
type Slab = (f64, f64, f64, f64);

/// The source rings wall `w` contributes after its apertures are cut: one
/// seam-free simple contour per connected piece of surviving material, or
/// exactly `w.rectangle()` when nothing cuts it.
pub fn wall_pieces(w: &WallSolid, apertures: &[Aperture]) -> Vec<Vec<Point>> {
    banded_pieces(w, apertures, None)
}

/// [`wall_pieces`] restricted to one side's half of the thickness: `η ≥ 0`
/// for `Left`, `η ≤ 0` for `Right`. Paint only; the halves tile the pieces.
pub fn wall_side_pieces(w: &WallSolid, apertures: &[Aperture], side: Side) -> Vec<Vec<Point>> {
    banded_pieces(w, apertures, Some(side))
}

fn banded_pieces(w: &WallSolid, apertures: &[Aperture], band: Option<Side>) -> Vec<Vec<Point>> {
    let effective: Vec<&Aperture> = apertures
        .iter()
        .filter(|a| a.depth > 0.0 && a.s.1 > a.s.0 && a.s.1 > 0.0 && a.s.0 < 1.0)
        .collect();
    if effective.is_empty() && band.is_none() {
        return w.rectangle().map(|r| vec![r.to_vec()]).unwrap_or_default();
    }
    let Some(frame) = Frame::of(w) else {
        return Vec::new();
    };
    let half = w.thickness_mm / 2.0;
    let (floor, ceiling) = match band {
        None => (-half, half),
        Some(Side::Left) => (0.0, half),
        Some(Side::Right) => (-half, 0.0),
    };

    // Sweep: events at clamped span ends; between consecutive positions the
    // active cuts are those started and not yet ended.
    let mut events: Vec<(f64, bool, Side, f64)> = Vec::with_capacity(2 * effective.len());
    for a in &effective {
        events.push((a.s.0.max(0.0), true, a.side, a.depth));
        events.push((a.s.1.min(1.0), false, a.side, a.depth));
    }
    events.sort_by(|x, y| x.0.total_cmp(&y.0));
    let (mut left, mut right) = (Depths::default(), Depths::default());
    let mut slabs: Vec<Slab> = Vec::new();
    let mut at = 0.0;
    let mut i = 0;
    loop {
        let next = events.get(i).map_or(1.0, |e| e.0);
        if next > at {
            let (lo, hi) = (
                (-half + right.max()).max(floor),
                (half - left.max()).min(ceiling),
            );
            match slabs.last_mut() {
                Some(last) if last.2 == lo && last.3 == hi => last.1 = next,
                _ => slabs.push((at, next, lo, hi)),
            }
            at = next;
        }
        if i >= events.len() {
            break;
        }
        while let Some(&(x, starts, side, depth)) = events.get(i) {
            if x != at {
                break;
            }
            let set = match side {
                Side::Left => &mut left,
                Side::Right => &mut right,
            };
            if starts {
                set.add(depth);
            } else {
                set.remove(depth);
            }
            i += 1;
        }
    }

    // Pieces: runs of non-empty slabs whose neighbouring intervals overlap
    // with positive length. A point or edge contact separates pieces.
    let mut out = Vec::new();
    let mut piece: Vec<Slab> = Vec::new();
    for slab in slabs {
        let empty = slab.3 <= slab.2;
        let continues = piece
            .last()
            .is_some_and(|p| slab.2.max(p.2) < slab.3.min(p.3));
        if (empty || !continues) && !piece.is_empty() {
            out.push(contour(&frame, &piece));
            piece.clear();
        }
        if !empty {
            piece.push(slab);
        }
    }
    if !piece.is_empty() {
        out.push(contour(&frame, &piece));
    }
    out
}

/// The step-shaped contour of a run of slabs, counter-clockwise in the
/// frame: along the lower boundary, up the far end, back along the upper
/// boundary, down the near end. Repeated vertices and vertices between two
/// collinear neighbours are dropped (exact: frame edges are axis-parallel).
fn contour(frame: &Frame, slabs: &[Slab]) -> Vec<Point> {
    let mut v: Vec<(f64, f64)> = Vec::with_capacity(4 * slabs.len());
    for &(a, b, lo, _) in slabs {
        v.push((a, lo));
        v.push((b, lo));
    }
    for &(a, b, _, hi) in slabs.iter().rev() {
        v.push((b, hi));
        v.push((a, hi));
    }
    v.dedup();
    if v.len() > 1 && v.first() == v.last() {
        v.pop();
    }
    // One linear pass with a stack drops every vertex lying between two
    // collinear neighbours (frame edges are axis-parallel, so collinear
    // means equal `s` or equal `η`, compared exactly); a final pass handles
    // the wrap-around at the start. Removing in place restarted the scan
    // and was quadratic in the number of slabs (Codex GPT-6.1-Sol review).
    let collinear = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
        (p.0 == q.0 && q.0 == r.0) || (p.1 == q.1 && q.1 == r.1)
    };
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(v.len());
    for q in v {
        while out.len() >= 2 && collinear(out[out.len() - 2], out[out.len() - 1], q) {
            out.pop();
        }
        out.push(q);
    }
    loop {
        let n = out.len();
        if n > 4 && collinear(out[n - 2], out[n - 1], out[0]) {
            out.pop();
        } else if n > 4 && collinear(out[n - 1], out[0], out[1]) {
            out.remove(0);
        } else {
            break;
        }
    }
    let v = out;
    v.into_iter().map(|(s, eta)| frame.at(s, eta)).collect()
}

/// The source rings of a level's drawn region: every wall's pieces after its
/// own cuts, then every join patch (never cut).
pub fn drawn_sources(
    walls: &[WallSolid],
    joins: &[JoinSpec],
    apertures: &[Aperture],
) -> Vec<Vec<Point>> {
    let mut by_wall: Vec<Vec<Aperture>> = vec![Vec::new(); walls.len()];
    for a in apertures {
        if let Some(list) = by_wall.get_mut(a.wall) {
            list.push(*a);
        }
    }
    let mut rings: Vec<Vec<Point>> = Vec::with_capacity(walls.len() + joins.len());
    for (w, cuts) in walls.iter().zip(&by_wall) {
        rings.extend(wall_pieces(w, cuts));
    }
    for join in joins {
        let patch = walls::join_patch(walls, join);
        if patch.len() >= 3 {
            rings.push(patch);
        }
    }
    rings
}

/// The drawn region (R4.2.2) of `walls` and `joins` with `apertures` cut:
/// canonical polygons, as `walls::plan(..).0` gives for no cuts.
pub fn drawn_region(
    walls: &[WallSolid],
    joins: &[JoinSpec],
    apertures: &[Aperture],
) -> Vec<Polygon> {
    crate::boolean::union(&drawn_sources(walls, joins, apertures))
}

/// The apertures of every opening hosted on the level of `solids`.
pub fn level_apertures(doc: &Document, solids: &LevelSolids) -> Vec<Aperture> {
    let index: crate::overlay::FastMap<Uuid, usize> = solids
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    doc.openings
        .iter()
        .filter_map(|o| {
            let &w = index.get(&o.host_wall_id)?;
            Aperture::of(o, w, &solids.walls[w])
        })
        .collect()
}

/// Whether any opening is hosted by a wall on `level_id`: `O(walls +
/// openings)`, so a caller holding the unamended region can skip building
/// the drawn one, which then equals it.
pub fn level_has_openings(doc: &Document, level_id: Uuid) -> bool {
    if doc.openings.is_empty() {
        return false;
    }
    let walls: crate::overlay::FastMap<Uuid, ()> =
        doc.walls_on_level(level_id).map(|w| (w.id, ())).collect();
    doc.openings
        .iter()
        .any(|o| walls.contains_key(&o.host_wall_id))
}

/// [`drawn_region`] of one level of `doc`.
pub fn level_drawn_region(doc: &Document, level_id: Uuid) -> Vec<Polygon> {
    let solids = walls::level_solids(doc, level_id);
    let apertures = level_apertures(doc, &solids);
    drawn_region(&solids.walls, &solids.joins, &apertures)
}
