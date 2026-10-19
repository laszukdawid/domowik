//! A persistent uniform bucket grid with insertion AND removal.
//!
//! `overlay`'s bucket grid is rebuilt per call and sized to its input. The
//! incremental engine keeps its indexes for the lifetime of a level, so the
//! bucket size is fixed instead, and every item can be removed again by
//! recomputing the same cover it was inserted with.
//!
//! # Covers are deterministic, and that is what makes removal work
//!
//! An item is inserted into every bucket of its [`Cover`], and removed from
//! every bucket of the same cover. Covers are a pure function of the item's
//! geometry, so the caller only needs to keep the geometry, not the bucket
//! list.
//!
//! # Large items
//!
//! An item whose cover spans more than [`LARGE`] buckets — a kilometre-long
//! wall edge, a campus outline ring — is kept in a short side list with its
//! bucket range instead of being copied into every bucket: inserting a 10 km
//! edge filled ten thousand 1 m buckets, and a building-sized ring hundreds
//! of thousands of 16 m ones. Queries visit every large item whose bucket
//! range meets theirs, a superset of the buckets they share, which callers
//! already tolerate (they filter candidates exactly). Large items are few on
//! any real plan, so the per-query cost is a handful of range checks.
//!
//! # Units
//!
//! Everything is in SCALED units: topology cells, `mm · 1024`.

use crate::overlay::{boxes_overlap, walk_buckets, FastMap};
use domowik_format::Point;

/// Bucket side for segments, hot cells and edges: 2²⁰ cells ≈ 1.02 m, about
/// one wall's worth of plan per bucket.
pub(super) const FINE: f64 = 1_048_576.0;

/// Bucket side for boundary rings: 2²⁴ cells ≈ 16.4 m. A house outline is one
/// ring spanning the whole plan; on the fine grid it would occupy hundreds of
/// buckets.
pub(super) const COARSE: f64 = 16_777_216.0;

/// Covers spanning more buckets than this go to the large-item list.
const LARGE: i128 = 256;

/// What an item occupies, for bucketing purposes.
#[derive(Debug, Clone, Copy)]
pub(super) enum Cover {
    /// An axis-aligned box `[min_x, min_y, max_x, max_y]`.
    Box([f64; 4]),
    /// A segment. Short or axis-aligned ones use their box; a long diagonal
    /// walks its path (`overlay::walk_buckets`) so it does not fill a square
    /// of buckets.
    Segment(Point, Point),
}

impl Cover {
    pub(super) fn bounds(&self) -> [f64; 4] {
        match *self {
            Cover::Box(b) => b,
            Cover::Segment(a, b) => [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)],
        }
    }
}

/// A bucket grid of `T` with a fixed bucket side.
#[derive(Debug, Clone)]
pub(super) struct BucketIndex<T> {
    size: f64,
    /// `1 / size`, exact because `size` is a power of two: bucket keys are
    /// a multiplication, not a division, on every query.
    inv: f64,
    map: FastMap<(i64, i64), Vec<T>>,
    /// Inclusive bucket-x range ever touched, for rightward ray queries.
    min_bx: i64,
    max_bx: i64,
    /// Items whose cover spans more than [`LARGE`] buckets, with their
    /// inclusive bucket range `[x0, y0, x1, y1]`.
    large: Vec<(T, [i64; 4])>,
}

impl<T: Copy + PartialEq> BucketIndex<T> {
    pub(super) fn new(size: f64) -> Self {
        debug_assert!(
            size > 0.0 && size.log2().fract() == 0.0,
            "bucket size must be a power of two"
        );
        Self {
            size,
            inv: 1.0 / size,
            map: FastMap::default(),
            min_bx: i64::MAX,
            max_bx: i64::MIN,
            large: Vec::new(),
        }
    }

    /// The inclusive bucket range of `cover`, and whether it is large.
    fn range(&self, cover: Cover) -> ([i64; 4], bool) {
        let b = cover.bounds();
        let r = [
            self.key(b[0]),
            self.key(b[1]),
            self.key(b[2]),
            self.key(b[3]),
        ];
        let (w, h) = (
            r[2] as i128 - r[0] as i128 + 1,
            r[3] as i128 - r[1] as i128 + 1,
        );
        let n = match cover {
            Cover::Segment(..) if w >= 3 && h >= 3 => w + h,
            _ => w * h,
        };
        (r, n > LARGE)
    }

    /// Visits every large item whose bucket range meets `r`.
    fn visit_large(&self, r: [i64; 4], visit: &mut impl FnMut(T)) {
        for &(t, b) in &self.large {
            if b[0] <= r[2] && r[0] <= b[2] && b[1] <= r[3] && r[1] <= b[3] {
                visit(t);
            }
        }
    }

    fn key(&self, v: f64) -> i64 {
        (v * self.inv).floor() as i64
    }

    /// Calls `visit` for every bucket of `cover`. A bucket may be reported
    /// more than once only for a walked segment, never for a box.
    pub(super) fn cover(&self, cover: Cover, mut visit: impl FnMut((i64, i64))) {
        let b = cover.bounds();
        let (x0, y0, x1, y1) = (
            self.key(b[0]),
            self.key(b[1]),
            self.key(b[2]),
            self.key(b[3]),
        );
        match cover {
            Cover::Segment(a, c) if x1 - x0 >= 2 && y1 - y0 >= 2 => {
                walk_buckets(self.size, a, c, 1.0, visit);
            }
            _ => {
                for bx in x0..=x1 {
                    for by in y0..=y1 {
                        visit((bx, by));
                    }
                }
            }
        }
    }

    pub(super) fn insert(&mut self, item: T, cover: Cover) {
        let (r, large) = self.range(cover);
        if large {
            self.min_bx = self.min_bx.min(r[0]);
            self.max_bx = self.max_bx.max(r[2]);
            self.large.push((item, r));
            return;
        }
        let mut keys = Vec::new();
        self.cover(cover, |k| keys.push(k));
        keys.sort_unstable();
        keys.dedup();
        for k in keys {
            self.min_bx = self.min_bx.min(k.0);
            self.max_bx = self.max_bx.max(k.0);
            self.map.entry(k).or_default().push(item);
        }
    }

    /// Removes `item` from every bucket of `cover`, which must be the cover it
    /// was inserted with. A missing entry is an invariant violation.
    pub(super) fn remove(&mut self, item: T, cover: Cover) {
        let (r, large) = self.range(cover);
        if large {
            let i = self
                .large
                .iter()
                .position(|&(t, b)| t == item && b == r)
                .expect("bucket index: large item missing");
            self.large.swap_remove(i);
            return;
        }
        let mut keys = Vec::new();
        self.cover(cover, |k| keys.push(k));
        keys.sort_unstable();
        keys.dedup();
        for k in keys {
            let list = self
                .map
                .get_mut(&k)
                .expect("bucket index: removing from a bucket that was never filled");
            let i = list
                .iter()
                .position(|x| *x == item)
                .expect("bucket index: item missing from a bucket of its own cover");
            list.swap_remove(i);
            if list.is_empty() {
                self.map.remove(&k);
            }
        }
    }

    /// Every item in a bucket of `cover`; duplicates possible.
    ///
    /// A box covering more buckets than are occupied scans the occupied
    /// buckets instead of the box, so a query costs `O(min(box, occupied))`
    /// buckets: a change spanning kilometres of 1 m buckets otherwise
    /// probed tens of millions of empty ones (a four-wall 10 km room took
    /// 430 ms to build). Visit order is unspecified either way.
    pub(super) fn query(&self, cover: Cover, mut visit: impl FnMut(T)) {
        if !self.large.is_empty() {
            self.visit_large(self.range(cover).0, &mut visit);
        }
        if let Cover::Box(b) = cover {
            let (x0, y0, x1, y1) = (
                self.key(b[0]),
                self.key(b[1]),
                self.key(b[2]),
                self.key(b[3]),
            );
            let span = (x1 as i128 - x0 as i128 + 1) * (y1 as i128 - y0 as i128 + 1);
            if span > self.map.len() as i128 {
                for (&(bx, by), list) in &self.map {
                    if (x0..=x1).contains(&bx) && (y0..=y1).contains(&by) {
                        for &t in list {
                            visit(t);
                        }
                    }
                }
                return;
            }
        }
        self.cover(cover, |k| {
            if let Some(list) = self.map.get(&k) {
                for &t in list {
                    visit(t);
                }
            }
        });
    }

    /// Every item in the buckets of rows covering `[y0, y1]`, from the bucket
    /// holding `x` rightwards to the last bucket ever used. This is the
    /// candidate set for a horizontal ray cast to `+x`.
    pub(super) fn ray_right(&self, x: f64, y0: f64, y1: f64, mut visit: impl FnMut(T)) {
        if self.max_bx < self.min_bx {
            return;
        }
        let (r0, r1, c0) = (self.key(y0), self.key(y1), self.key(x).max(self.min_bx));
        if !self.large.is_empty() {
            self.visit_large([c0, r0, self.max_bx, r1], &mut visit);
        }
        // As in `query`: scan the occupied buckets when there are fewer.
        let span = (r1 as i128 - r0 as i128 + 1) * (self.max_bx as i128 - c0 as i128 + 1);
        if span > self.map.len() as i128 {
            for (&(bx, by), list) in &self.map {
                if bx >= c0 && (r0..=r1).contains(&by) {
                    for &t in list {
                        visit(t);
                    }
                }
            }
            return;
        }
        for by in r0..=r1 {
            for bx in c0..=self.max_bx {
                if let Some(list) = self.map.get(&(bx, by)) {
                    for &t in list {
                        visit(t);
                    }
                }
            }
        }
    }
}

/// Whether two `[min_x, min_y, max_x, max_y]` boxes share a point.
pub(super) fn overlap(a: &[f64; 4], b: &[f64; 4]) -> bool {
    boxes_overlap(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_undoes_insertion_for_boxes_and_walked_segments() {
        let mut idx: BucketIndex<u32> = BucketIndex::new(64.0);
        let diag = Cover::Segment(Point::new(0.0, 0.0), Point::new(1000.0, 700.0));
        let boxed = Cover::Box([5.0, 5.0, 300.0, 10.0]);
        idx.insert(1, diag);
        idx.insert(2, boxed);
        let mut seen = Vec::new();
        idx.query(Cover::Box([0.0, 0.0, 1000.0, 1000.0]), |t| seen.push(t));
        assert!(seen.contains(&1) && seen.contains(&2));
        idx.remove(1, diag);
        idx.remove(2, boxed);
        assert!(idx.map.is_empty());
    }

    /// A cover wider than `LARGE` buckets is kept once, found by any query
    /// whose buckets meet its range (including a rightward ray), and removed
    /// again by the same cover.
    #[test]
    fn large_items_are_found_and_removed_without_filling_buckets() {
        let mut idx: BucketIndex<u32> = BucketIndex::new(1.0);
        let long = Cover::Segment(Point::new(0.0, 5.0), Point::new(10_000.0, 5.0));
        let wide = Cover::Box([-500.0, -500.0, 500.0, 500.0]);
        idx.insert(1, long);
        idx.insert(2, wide);
        assert!(idx.map.is_empty(), "large items fill no bucket");
        let at = |x: f64, y: f64| {
            let mut seen = Vec::new();
            idx.query(Cover::Box([x, y, x, y]), |t| seen.push(t));
            seen.sort_unstable();
            seen
        };
        assert_eq!(at(9_999.5, 5.5), vec![1]);
        assert_eq!(at(0.0, 0.0), vec![2]);
        assert_eq!(at(400.0, 5.0), vec![1, 2]);
        assert_eq!(at(20_000.0, 5.0), Vec::<u32>::new());
        let mut ray = Vec::new();
        idx.ray_right(-2_000.0, 5.0, 5.0, |t| ray.push(t));
        ray.sort_unstable();
        assert_eq!(ray, vec![1, 2]);
        idx.remove(1, long);
        idx.remove(2, wide);
        assert!(idx.large.is_empty());
    }
}
