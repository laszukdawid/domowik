//! Segments, their hot-cell events, and their snap-rounded routes.
//!
//! Each input ring edge is a SEGMENT in scaled units. A segment:
//!
//! - holds a hot-cell reference for each endpoint;
//! - holds references for the cells of its proper crossings with segments of
//!   OTHER sources (a source's own ring is simple, as in the full builder);
//! - has a ROUTE: the hot cells it meets, in order along it (Hobby 1999), and
//!   contributes `±1` per consecutive pair to the net weight of the integer
//!   edge between them.
//!
//! A route is a pure function of the segment and the current hot set, so it
//! needs recomputing only when the segment is new or when a cell it meets
//! changed state. [`Routing::mark_meeting`] finds exactly those segments.

use super::hot::{cell_box, HotCells};
use super::index::{overlap, BucketIndex, Cover, FINE};
use super::provenance::TagDelta;
use crate::overlay::{
    proper_crossing_cells, scaled_index, segment_meets_cell, sign_of, Cell, FastMap,
};
use domowik_format::Point;

pub(super) type SegId = u32;

/// Net weight changes of integer edges, keyed `(lo, hi)` with `lo < hi`, in
/// the `lo → hi` direction. Accumulated over a batch, consumed by the DCEL.
pub(super) type EdgeDelta = FastMap<(Cell, Cell), i32>;

#[derive(Debug, Clone)]
struct Segment {
    /// Endpoints in scaled units, exact copies of the canonical input ring.
    a: Point,
    b: Point,
    /// Index of the owning source; crossings within one source are skipped.
    source: u32,
    /// Hot cells met, in order from `a` to `b`. Empty until routed.
    route: Vec<Cell>,
    /// Segments this one properly crosses (each pair's cells are in
    /// [`Routing::crossings`]).
    partners: Vec<SegId>,
}

impl Segment {
    fn bounds(&self) -> [f64; 4] {
        [
            self.a.x.min(self.b.x),
            self.a.y.min(self.b.y),
            self.a.x.max(self.b.x),
            self.a.y.max(self.b.y),
        ]
    }

    fn cover(&self) -> Cover {
        Cover::Segment(self.a, self.b)
    }
}

fn endpoint_cell(p: Point) -> Cell {
    Cell {
        x: scaled_index(p.x),
        y: scaled_index(p.y),
    }
}

fn pair(a: SegId, b: SegId) -> (SegId, SegId) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Adds `sign` times `route`'s pieces to `delta`, and to `tags` under
/// `source` when provenance is tracked.
fn contribute(
    route: &[Cell],
    sign: i32,
    delta: &mut EdgeDelta,
    source: u32,
    tags: Option<&mut TagDelta>,
) {
    for w in route.windows(2) {
        let (p, q) = (w[0], w[1]);
        let (key, dir) = if p < q { ((p, q), 1) } else { ((q, p), -1) };
        *delta.entry(key).or_insert(0) += sign * dir;
    }
    if let Some(tags) = tags {
        tags.extend(route.windows(2).map(|w| ((w[0], w[1]), source, sign)));
    }
}

#[derive(Debug, Clone)]
pub(super) struct Routing {
    segs: Vec<Option<Segment>>,
    free: Vec<SegId>,
    index: BucketIndex<SegId>,
    crossings: FastMap<(SegId, SegId), Vec<Cell>>,
    /// Segments whose route must be recomputed before the batch ends.
    pending: Vec<SegId>,
    /// Per-segment visit stamps, for deduplicating index queries.
    stamp: Vec<u32>,
    epoch: u32,
    /// Route-piece ownership changes of the current batch, when provenance
    /// is tracked (`None` otherwise, at no cost).
    tags: Option<TagDelta>,
}

impl Routing {
    pub(super) fn new(track_tags: bool) -> Self {
        Self {
            segs: Vec::new(),
            free: Vec::new(),
            index: BucketIndex::new(FINE),
            crossings: FastMap::default(),
            pending: Vec::new(),
            stamp: Vec::new(),
            epoch: 0,
            tags: track_tags.then(Vec::new),
        }
    }

    /// The batch's ownership changes, leaving an empty buffer behind.
    pub(super) fn take_tags(&mut self) -> Option<TagDelta> {
        self.tags.as_mut().map(std::mem::take)
    }

    /// Every live segment's source and route, for validation.
    pub(super) fn routes(&self) -> impl Iterator<Item = (u32, &[Cell])> {
        self.segs
            .iter()
            .flatten()
            .map(|s| (s.source, s.route.as_slice()))
    }

    pub(super) fn len(&self) -> usize {
        self.segs.len() - self.free.len()
    }

    fn next_epoch(&mut self) -> u32 {
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.epoch = 1;
        }
        self.epoch
    }

    /// Distinct live segments in buckets of `cover`.
    fn candidates(&mut self, cover: Cover) -> Vec<SegId> {
        let epoch = self.next_epoch();
        let mut out = Vec::new();
        let (index, stamp) = (&self.index, &mut self.stamp);
        index.query(cover, |id| {
            let s = &mut stamp[id as usize];
            if *s != epoch {
                *s = epoch;
                out.push(id);
            }
        });
        out
    }

    /// Inserts a segment and acquires its endpoint and crossing events. Its
    /// route is computed later, in [`Routing::route_pending`], once every
    /// change of the batch has settled the hot set.
    pub(super) fn add(&mut self, a: Point, b: Point, source: u32, hot: &mut HotCells) -> SegId {
        let seg = Segment {
            a,
            b,
            source,
            route: Vec::new(),
            partners: Vec::new(),
        };
        let bounds = seg.bounds();
        let cover = seg.cover();
        let id = match self.free.pop() {
            Some(id) => id,
            None => {
                self.segs.push(None);
                self.stamp.push(0);
                (self.segs.len() - 1) as SegId
            }
        };
        hot.acquire(endpoint_cell(a));
        hot.acquire(endpoint_cell(b));

        let mut cells = Vec::new();
        let mut partners = Vec::new();
        for other in self.candidates(cover) {
            let Some(t) = &self.segs[other as usize] else {
                continue;
            };
            if t.source == source || !overlap(&bounds, &t.bounds()) {
                continue;
            }
            cells.clear();
            proper_crossing_cells(a, b, t.a, t.b, &mut cells);
            cells.sort_unstable();
            cells.dedup();
            if cells.is_empty() {
                continue;
            }
            for &c in &cells {
                hot.acquire(c);
            }
            self.crossings.insert(pair(id, other), cells.clone());
            self.segs[other as usize]
                .as_mut()
                .expect("candidate is live")
                .partners
                .push(id);
            partners.push(other);
        }
        self.segs[id as usize] = Some(Segment { partners, ..seg });
        self.index.insert(id, cover);
        self.pending.push(id);
        id
    }

    /// Removes a segment: releases its events and withdraws its route's
    /// contribution from `delta`.
    pub(super) fn remove(&mut self, id: SegId, hot: &mut HotCells, delta: &mut EdgeDelta) {
        let seg = self.segs[id as usize]
            .take()
            .expect("routing: removing a dead segment");
        hot.release(endpoint_cell(seg.a));
        hot.release(endpoint_cell(seg.b));
        for &p in &seg.partners {
            if let Some(cells) = self.crossings.remove(&pair(id, p)) {
                for c in cells {
                    hot.release(c);
                }
            }
            if let Some(other) = self.segs[p as usize].as_mut() {
                other.partners.retain(|&x| x != id);
            }
        }
        self.index.remove(id, seg.cover());
        contribute(&seg.route, -1, delta, seg.source, self.tags.as_mut());
        self.free.push(id);
    }

    /// Marks every live segment that meets one of `cells` for rerouting.
    /// Returns how many were marked.
    pub(super) fn mark_meeting(&mut self, cells: &[Cell]) -> usize {
        let mut marked = 0;
        for &c in cells {
            for id in self.candidates(Cover::Box(cell_box(c))) {
                if let Some(s) = &self.segs[id as usize] {
                    if segment_meets_cell(s.a, s.b, c) {
                        self.pending.push(id);
                        marked += 1;
                    }
                }
            }
        }
        marked
    }

    /// Recomputes the route of every pending segment against the final hot
    /// set and records the net edge changes. Returns how many were routed.
    pub(super) fn route_pending(&mut self, hot: &HotCells, delta: &mut EdgeDelta) -> usize {
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_unstable();
        pending.dedup();
        let mut routed = 0;
        for id in pending {
            let Some(seg) = self.segs[id as usize].as_ref() else {
                continue;
            };
            let route = route_of(seg.a, seg.b, hot);
            if route == seg.route {
                continue;
            }
            let source = seg.source;
            contribute(&seg.route, -1, delta, source, self.tags.as_mut());
            contribute(&route, 1, delta, source, self.tags.as_mut());
            self.segs[id as usize].as_mut().expect("live").route = route;
            routed += 1;
        }
        routed
    }
}

/// The hot cells `a→b` meets, ordered along it. Identical to the full
/// builder's routing for the same hot set: candidates from the index, the
/// same exact half-open test, the same order key.
fn route_of(a: Point, b: Point, hot: &HotCells) -> Vec<Cell> {
    let bounds = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
    let mut cells = Vec::new();
    hot.near(Cover::Segment(a, b), |c| {
        if overlap(&cell_box(c), &bounds) && segment_meets_cell(a, b, c) {
            cells.push(c);
        }
    });
    let (sx, sy) = (sign_of(b.x, a.x), sign_of(b.y, a.y));
    cells.sort_unstable_by_key(|c| (sx * c.x, sy * c.y));
    cells.dedup();
    debug_assert_eq!(cells.first().copied(), Some(endpoint_cell(a)));
    debug_assert_eq!(cells.last().copied(), Some(endpoint_cell(b)));
    cells
}

impl Routing {
    /// Checks that every hot reference is accounted for by a live event and
    /// that every route equals routing against the current hot set. Panics
    /// naming the first violation.
    pub(super) fn validate(&self, hot: &HotCells) {
        let mut expected: FastMap<Cell, u32> = FastMap::default();
        for seg in self.segs.iter().flatten() {
            *expected.entry(endpoint_cell(seg.a)).or_insert(0) += 1;
            *expected.entry(endpoint_cell(seg.b)).or_insert(0) += 1;
        }
        for cells in self.crossings.values() {
            for &c in cells {
                *expected.entry(c).or_insert(0) += 1;
            }
        }
        hot.assert_counts(&expected);
        for (id, seg) in self.segs.iter().enumerate() {
            let Some(seg) = seg else { continue };
            assert_eq!(
                route_of(seg.a, seg.b, hot),
                seg.route,
                "routing: segment {id} has a stale route"
            );
            for &p in &seg.partners {
                assert!(
                    self.crossings.contains_key(&pair(id as SegId, p)),
                    "routing: segment {id} lists partner {p} with no crossing record"
                );
            }
        }
    }
}
