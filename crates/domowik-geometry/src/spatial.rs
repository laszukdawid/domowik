//! A spatial index over axis-aligned bounds, for culling and proximity.
//!
//! # Why a uniform grid
//!
//! Floor plans are spatially even: walls are small relative to the plan, they
//! do not clump into one corner, and the whole set changes shape on every
//! edit. A uniform grid rebuilds in linear time, answers a rectangle query by
//! walking the cells it covers, and has no rebalancing to get wrong. An R-tree
//! would query better on adversarial distributions and cost more on every one
//! of the edits a plan actually receives.
//!
//! The failure mode is stated so it is not a surprise: ONE entity far larger
//! than the cell size occupies many cells, and a query touching any of them
//! pays for it. [`SpatialIndex::build`] sizes cells from the median extent
//! rather than the mean for exactly that reason — a single 100 m wall must not
//! coarsen the grid for the two hundred normal ones around it.
//!
//! # What this is NOT
//!
//! It is not a persistent structure. Every query is answered from a snapshot
//! built for it, because a plan's derived geometry is rebuilt on every edit
//! anyway (BEHAVIOR-SPEC R3.0.7) and an index maintained incrementally
//! alongside it would be a second thing to keep true.

use domowik_format::Point;

/// An axis-aligned bounding box in plan millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Aabb {
    /// The box containing exactly `p`.
    pub fn point(p: Point) -> Self {
        Self {
            min_x: p.x,
            min_y: p.y,
            max_x: p.x,
            max_y: p.y,
        }
    }

    /// The box containing every point in `points`. `None` for an empty slice.
    pub fn of(points: &[Point]) -> Option<Self> {
        let first = points.first()?;
        let mut b = Self::point(*first);
        for p in &points[1..] {
            b.min_x = b.min_x.min(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_x = b.max_x.max(p.x);
            b.max_y = b.max_y.max(p.y);
        }
        Some(b)
    }

    /// The box from two corners, in any order.
    pub fn from_corners(a: Point, b: Point) -> Self {
        Self {
            min_x: a.x.min(b.x),
            min_y: a.y.min(b.y),
            max_x: a.x.max(b.x),
            max_y: a.y.max(b.y),
        }
    }

    /// Grows the box by `by` on every side.
    pub fn expanded(self, by: f64) -> Self {
        Self {
            min_x: self.min_x - by,
            min_y: self.min_y - by,
            max_x: self.max_x + by,
            max_y: self.max_y + by,
        }
    }

    /// Whether two boxes share any point.
    ///
    /// Boundary-inclusive, which R7.3.1 requires of marquee selection:
    /// "tangential or boundary-only contact selects".
    pub fn intersects(&self, other: &Self) -> bool {
        self.min_x <= other.max_x
            && other.min_x <= self.max_x
            && self.min_y <= other.max_y
            && other.min_y <= self.max_y
    }

    /// Whether `p` lies inside or on the boundary.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }

    /// The smallest box containing both.
    pub fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    /// The squared distance from `p` to the nearest point of the box.
    ///
    /// Zero when `p` is inside. Squared so callers comparing distances never
    /// pay for a square root, and so comparisons stay exact on exact inputs
    /// (R6.3.1 compares "exact squared screen distance").
    pub fn distance_squared(&self, p: Point) -> f64 {
        let dx = (self.min_x - p.x).max(0.0).max(p.x - self.max_x);
        let dy = (self.min_y - p.y).max(0.0).max(p.y - self.max_y);
        dx * dx + dy * dy
    }
}

/// A uniform-grid index over `(Aabb, T)` pairs.
#[derive(Debug, Clone)]
pub struct SpatialIndex<T> {
    entries: Vec<(Aabb, T)>,
    bounds: Aabb,
    cell: f64,
    cols: usize,
    rows: usize,
    /// Cell index → indices into `entries`.
    cells: Vec<Vec<u32>>,
}

/// How many cells the grid is allowed to have.
///
/// A bound rather than a tuning constant: without it, one plan a thousand
/// times wider than its cell size allocates a million empty cells and the
/// index costs more than the scan it replaces.
const MAX_CELLS: usize = 1 << 16;

/// Reusable visit marks for a sequence of queries against one or more indexes.
///
/// One mark per entry is allocated on growth. Subsequent queries advance an
/// epoch instead of zeroing an index-sized array, so n local queries do not
/// add n-squared initialization work. Epoch wrap clears the retained marks
/// once every `u32::MAX` queries. The caller owns this scratch and must borrow
/// it exclusively for each query; it retains no reference to an index.
#[derive(Debug, Default)]
pub struct QueryScratch {
    marks: Vec<u32>,
    epoch: u32,
}

impl QueryScratch {
    fn begin(&mut self, entries: usize) {
        // Retain marks sized for the largest index seen. Shrinking here makes
        // alternating large and small indexes allocate and initialise again
        // when the large index returns.
        if self.marks.len() < entries {
            self.marks.resize(entries, 0);
        }
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.marks.fill(0);
            self.epoch = 1;
        }
    }

    fn first_visit(&mut self, entry: usize) -> bool {
        if self.marks[entry] == self.epoch {
            return false;
        }
        self.marks[entry] = self.epoch;
        true
    }
}

impl<T: Copy> SpatialIndex<T> {
    /// Builds an index over `items`.
    ///
    /// Cell size is the MEDIAN entry extent, clamped so the grid stays within
    /// [`MAX_CELLS`]. Median rather than mean: one wall an order of magnitude
    /// longer than the rest must not coarsen the grid for all the others,
    /// and a mean is exactly the statistic that lets it.
    pub fn build(items: impl IntoIterator<Item = (Aabb, T)>) -> Self {
        let entries: Vec<(Aabb, T)> = items.into_iter().collect();
        if entries.is_empty() {
            return Self {
                entries,
                bounds: Aabb {
                    min_x: 0.0,
                    min_y: 0.0,
                    max_x: 0.0,
                    max_y: 0.0,
                },
                cell: 1.0,
                cols: 0,
                rows: 0,
                cells: Vec::new(),
            };
        }

        let bounds = entries
            .iter()
            .map(|(b, _)| *b)
            .reduce(Aabb::union)
            .expect("non-empty");

        let mut extents: Vec<f64> = entries
            .iter()
            .map(|(b, _)| b.width().max(b.height()))
            .filter(|e| e.is_finite() && *e > 0.0)
            .collect();
        extents.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = extents
            .get(extents.len() / 2)
            .copied()
            .unwrap_or(1.0)
            .max(f64::MIN_POSITIVE);

        let mut cell = median;
        let (mut cols, mut rows) = grid_shape(&bounds, cell);
        while cols.saturating_mul(rows) > MAX_CELLS {
            cell *= 2.0;
            let (c, r) = grid_shape(&bounds, cell);
            cols = c;
            rows = r;
        }

        let mut cells = vec![Vec::new(); cols * rows];
        let mut index = Self {
            entries,
            bounds,
            cell,
            cols,
            rows,
            cells: Vec::new(),
        };
        for (i, (b, _)) in index.entries.iter().enumerate() {
            for c in index.covered_cells(b) {
                cells[c].push(i as u32);
            }
        }
        index.cells = cells;
        index
    }

    /// The number of indexed entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The union of every indexed bound.
    pub fn bounds(&self) -> Aabb {
        self.bounds
    }

    /// The grid's cell edge length, in plan millimetres.
    ///
    /// Exposed for testing the SIZING rule rather than for callers to depend
    /// on: a query's ANSWER never changes with the cell size, only its cost,
    /// so a test that checks results cannot tell a well-sized grid from a
    /// badly sized one.
    pub fn cell_size(&self) -> f64 {
        self.cell
    }

    /// Values whose bounds intersect `region` — the culling query.
    ///
    /// Results are in insertion order, deduplicated: an entry spanning several
    /// cells is returned once.
    pub fn query(&self, region: &Aabb) -> Vec<T> {
        self.candidates(region)
            .into_iter()
            .filter(|&i| self.entries[i].0.intersects(region))
            .map(|i| self.entries[i].1)
            .collect()
    }

    /// The same boundary-inclusive, insertion-ordered result as [`Self::query`],
    /// with reusable visit marks for bulk local queries.
    ///
    /// Each query costs O(g + c + k*log(k)), where g is its covered-cell count,
    /// c is its cell-entry visits, and k is its distinct candidates. Bounds
    /// filtering and result allocation are included; this is not a worst-case
    /// subquadratic all-pairs guarantee.
    /// Broad-phase callers must supply conservative bounds for their geometry.
    pub fn query_with_scratch(&self, region: &Aabb, scratch: &mut QueryScratch) -> Vec<T> {
        scratch.begin(self.entries.len());
        let mut candidates = Vec::new();
        if self.cells.is_empty() {
            candidates.extend(0..self.entries.len());
        } else {
            for cell in self.covered_cells(region) {
                for &entry in &self.cells[cell] {
                    let entry = entry as usize;
                    if scratch.first_visit(entry) {
                        candidates.push(entry);
                    }
                }
            }
            candidates.sort_unstable();
        }
        candidates
            .into_iter()
            .filter(|&i| self.entries[i].0.intersects(region))
            .map(|i| self.entries[i].1)
            .collect()
    }

    /// Values whose bounds lie within `radius` of `p` — the proximity query.
    pub fn near(&self, p: Point, radius: f64) -> Vec<T> {
        let region = Aabb::point(p).expanded(radius);
        let r2 = radius * radius;
        self.candidates(&region)
            .into_iter()
            .filter(|&i| self.entries[i].0.distance_squared(p) <= r2)
            .map(|i| self.entries[i].1)
            .collect()
    }

    /// Entry indices whose cells overlap `region`, deduplicated and ascending.
    fn candidates(&self, region: &Aabb) -> Vec<usize> {
        if self.cells.is_empty() {
            return (0..self.entries.len()).collect();
        }
        let mut seen = vec![false; self.entries.len()];
        let mut out = Vec::new();
        for c in self.covered_cells(region) {
            for &i in &self.cells[c] {
                let i = i as usize;
                if !seen[i] {
                    seen[i] = true;
                    out.push(i);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// The cell indices a box covers, clamped to the grid.
    fn covered_cells(&self, b: &Aabb) -> Vec<usize> {
        if self.cols == 0 || self.rows == 0 {
            return Vec::new();
        }
        let col = |x: f64| {
            (((x - self.bounds.min_x) / self.cell).floor() as isize)
                .clamp(0, self.cols as isize - 1) as usize
        };
        let row = |y: f64| {
            (((y - self.bounds.min_y) / self.cell).floor() as isize)
                .clamp(0, self.rows as isize - 1) as usize
        };
        let (c0, c1) = (col(b.min_x), col(b.max_x));
        let (r0, r1) = (row(b.min_y), row(b.max_y));
        let mut out = Vec::with_capacity((c1 - c0 + 1) * (r1 - r0 + 1));
        for r in r0..=r1 {
            for c in c0..=c1 {
                out.push(r * self.cols + c);
            }
        }
        out
    }
}

fn grid_shape(bounds: &Aabb, cell: f64) -> (usize, usize) {
    let cols = ((bounds.width() / cell).ceil() as usize).max(1);
    let rows = ((bounds.height() / cell).ceil() as usize).max(1);
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reusable_query_marks_match_scan_across_queries_indexes_and_wrap() {
        let items = boxes(500);
        let index = SpatialIndex::build(items.clone());
        let mut scratch = QueryScratch::default();
        for &(bounds, _) in &items {
            let expected: Vec<_> = items
                .iter()
                .filter(|(b, _)| b.intersects(&bounds))
                .map(|(_, v)| *v)
                .collect();
            assert_eq!(index.query_with_scratch(&bounds, &mut scratch), expected);
        }
        let all = index.bounds();
        assert_eq!(
            index.query_with_scratch(&all, &mut scratch),
            index.query(&all)
        );
        let retained = scratch.marks.as_ptr();
        let first_mark = scratch.marks[0];
        let last = items.last().unwrap().0;
        index.query_with_scratch(&last, &mut scratch);
        assert_eq!(
            scratch.marks.as_ptr(),
            retained,
            "reuse the visit allocation"
        );
        assert_eq!(
            scratch.marks[0], first_mark,
            "a local query does not clear untouched marks"
        );

        let retained_len = scratch.marks.len();
        let smaller_items = boxes(2);
        let smaller = SpatialIndex::build(smaller_items.clone());
        assert_eq!(
            smaller.query_with_scratch(&smaller.bounds(), &mut scratch),
            smaller.query(&smaller.bounds())
        );
        assert_eq!(
            scratch.marks.as_ptr(),
            retained,
            "smaller index keeps marks"
        );
        assert_eq!(
            scratch.marks.len(),
            retained_len,
            "smaller index keeps length"
        );

        let empty = SpatialIndex::<usize>::build([]);
        assert!(empty.query_with_scratch(&all, &mut scratch).is_empty());
        assert_eq!(scratch.marks.as_ptr(), retained, "empty index keeps marks");
        assert_eq!(
            scratch.marks.len(),
            retained_len,
            "empty index keeps length"
        );

        let larger_items = boxes(400);
        let larger = SpatialIndex::build(larger_items.clone());
        assert_eq!(
            larger.query_with_scratch(&larger.bounds(), &mut scratch),
            larger.query(&larger.bounds())
        );
        assert_eq!(scratch.marks.as_ptr(), retained, "regrowth reuses marks");
        assert_eq!(scratch.marks.len(), retained_len, "regrowth keeps length");

        scratch.epoch = u32::MAX;
        scratch.marks.fill(1);
        assert_eq!(
            index.query_with_scratch(&all, &mut scratch),
            index.query(&all)
        );
        assert_eq!(scratch.epoch, 1);
        let reversed = SpatialIndex::build(items.iter().rev().copied());
        assert_eq!(
            reversed.query_with_scratch(&all, &mut scratch),
            reversed.query(&all)
        );
        assert_eq!(
            index.query_with_scratch(&all, &mut scratch),
            index.query(&all)
        );
    }

    const P: fn(f64, f64) -> Point = Point::new;

    fn boxes(n: usize) -> Vec<(Aabb, usize)> {
        (0..n)
            .map(|i| {
                let x = (i % 10) as f64 * 1000.0;
                let y = (i / 10) as f64 * 1000.0;
                (Aabb::from_corners(P(x, y), P(x + 200.0, y + 200.0)), i)
            })
            .collect()
    }

    #[test]
    fn aabb_intersection_is_boundary_inclusive() {
        // R7.3.1: "tangential or boundary-only contact selects".
        let a = Aabb::from_corners(P(0.0, 0.0), P(10.0, 10.0));
        let touching = Aabb::from_corners(P(10.0, 0.0), P(20.0, 10.0));
        let apart = Aabb::from_corners(P(10.001, 0.0), P(20.0, 10.0));
        assert!(a.intersects(&touching));
        assert!(!a.intersects(&apart));
    }

    #[test]
    fn distance_squared_is_zero_inside_and_exact_outside() {
        let b = Aabb::from_corners(P(0.0, 0.0), P(10.0, 10.0));
        assert_eq!(b.distance_squared(P(5.0, 5.0)), 0.0);
        assert_eq!(b.distance_squared(P(10.0, 10.0)), 0.0, "boundary is inside");
        assert_eq!(b.distance_squared(P(13.0, 14.0)), 9.0 + 16.0);
        assert_eq!(b.distance_squared(P(-3.0, 5.0)), 9.0);
    }

    #[test]
    fn a_query_agrees_with_a_naive_scan() {
        // Issue 0012's first criterion, as a deterministic sweep rather than
        // a single case: every query rectangle over a grid of entries must
        // match the linear filter exactly.
        let items = boxes(100);
        let index = SpatialIndex::build(items.clone());

        for x in (-500..10_000).step_by(700) {
            for y in (-500..10_000).step_by(900) {
                let region = Aabb::from_corners(
                    P(x as f64, y as f64),
                    P(x as f64 + 1500.0, y as f64 + 1100.0),
                );
                let mut got = index.query(&region);
                got.sort_unstable();
                let mut want: Vec<usize> = items
                    .iter()
                    .filter(|(b, _)| b.intersects(&region))
                    .map(|(_, v)| *v)
                    .collect();
                want.sort_unstable();
                assert_eq!(got, want, "region ({x}, {y})");
            }
        }
    }

    #[test]
    fn a_proximity_query_agrees_with_a_naive_scan() {
        let items = boxes(100);
        let index = SpatialIndex::build(items.clone());
        for radius in [0.0, 50.0, 300.0, 2000.0] {
            for i in 0..40 {
                let p = P((i * 271 % 9000) as f64, (i * 733 % 9000) as f64);
                let mut got = index.near(p, radius);
                got.sort_unstable();
                let mut want: Vec<usize> = items
                    .iter()
                    .filter(|(b, _)| b.distance_squared(p) <= radius * radius)
                    .map(|(_, v)| *v)
                    .collect();
                want.sort_unstable();
                assert_eq!(got, want, "p={p:?} r={radius}");
            }
        }
    }

    #[test]
    fn an_entry_spanning_many_cells_is_returned_once() {
        let mut items = boxes(20);
        items.push((
            Aabb::from_corners(P(-1000.0, -1000.0), P(20_000.0, 20_000.0)),
            999,
        ));
        let index = SpatialIndex::build(items);
        let hits = index.query(&Aabb::from_corners(P(0.0, 0.0), P(9000.0, 9000.0)));
        assert_eq!(hits.iter().filter(|&&v| v == 999).count(), 1);
    }

    #[test]
    fn an_empty_index_answers_every_query_with_nothing() {
        let index: SpatialIndex<usize> = SpatialIndex::build(Vec::new());
        assert!(index.is_empty());
        assert!(index
            .query(&Aabb::from_corners(P(-1e9, -1e9), P(1e9, 1e9)))
            .is_empty());
        assert!(index.near(P(0.0, 0.0), 1e9).is_empty());
    }

    #[test]
    fn one_huge_entry_does_not_coarsen_the_grid_for_the_rest() {
        // The reason the cell size is the MEDIAN extent. With a mean, this
        // single entry would set cells ~100x too large and every query would
        // degrade to a scan.
        let mut items = boxes(200);
        items.push((Aabb::from_corners(P(0.0, 0.0), P(1_000_000.0, 10.0)), 9999));
        let index = SpatialIndex::build(items);
        // The property under test is the CELL SIZE, not the answer: query
        // results are filtered by actual intersection either way, so a
        // result-only assertion cannot tell a well-sized grid from a badly
        // sized one. The 200 entries have extent 200; the one outlier has
        // extent 1,000,000, which a mean would drag the cell size to ~5000.
        assert!(
            index.cell_size() < 1000.0,
            "cell size {} was dragged out by one outlier",
            index.cell_size()
        );
        let region = Aabb::from_corners(P(0.0, 0.0), P(300.0, 300.0));
        let hits = index.query(&region);
        assert!(hits.contains(&0));
        assert!(hits.contains(&9999));
    }

    #[test]
    fn a_degenerate_plan_of_coincident_entries_still_answers_correctly() {
        // Zero extent everywhere: the median is filtered out, the cell size
        // falls back, and the grid must not divide by zero or lose entries.
        let items: Vec<(Aabb, usize)> = (0..10).map(|i| (Aabb::point(P(5.0, 5.0)), i)).collect();
        let index = SpatialIndex::build(items);
        assert_eq!(index.query(&Aabb::point(P(5.0, 5.0))).len(), 10);
        assert!(index.query(&Aabb::point(P(6.0, 5.0))).is_empty());
    }
}
