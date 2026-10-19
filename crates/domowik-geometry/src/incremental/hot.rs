//! Reference-counted hot cells.
//!
//! A cell is HOT while at least one event holds it: a segment endpoint, or a
//! proper crossing of two segments from different sources (ED-0110; Hobby
//! 1999). Deleting a segment releases its events; the cell goes cold only
//! when its last producer goes. That is the dependency the previous
//! closure-based scope missed: a wall too small to occupy anything still
//! produces a hot cell, and that cell bends whatever passes through it
//! (Codex review of issue 0013, counterexample 2).
//!
//! The set also records which cells changed state since the last
//! [`HotCells::take_transitions`], as NET transitions: a cell released and
//! re-acquired inside one batch did not change and reroutes nothing.

use super::index::{BucketIndex, Cover, FINE};
use crate::overlay::{Cell, FastMap};

#[derive(Debug, Clone)]
pub(super) struct HotCells {
    count: FastMap<Cell, u32>,
    index: BucketIndex<Cell>,
    /// Cells touched this batch, with whether they were hot before it.
    touched: FastMap<Cell, bool>,
}

/// A hot cell's box in scaled units, used both to index it and to find the
/// segments that meet it.
pub(super) fn cell_box(c: Cell) -> [f64; 4] {
    let (x, y) = (c.x as f64, c.y as f64);
    [x - 0.5, y - 0.5, x + 0.5, y + 0.5]
}

impl HotCells {
    pub(super) fn new() -> Self {
        Self {
            count: FastMap::default(),
            index: BucketIndex::new(FINE),
            touched: FastMap::default(),
        }
    }

    pub(super) fn is_hot(&self, c: Cell) -> bool {
        self.count.contains_key(&c)
    }

    /// Takes one reference on `c`.
    pub(super) fn acquire(&mut self, c: Cell) {
        let was = self.is_hot(c);
        self.touched.entry(c).or_insert(was);
        let n = self.count.entry(c).or_insert(0);
        *n += 1;
        if *n == 1 {
            self.index.insert(c, Cover::Box(cell_box(c)));
        }
    }

    /// Releases one reference on `c`, which must be held.
    pub(super) fn release(&mut self, c: Cell) {
        let was = self.is_hot(c);
        self.touched.entry(c).or_insert(was);
        let n = self
            .count
            .get_mut(&c)
            .expect("hot cells: releasing a cell nobody holds");
        *n -= 1;
        if *n == 0 {
            self.count.remove(&c);
            self.index.remove(c, Cover::Box(cell_box(c)));
        }
    }

    /// Cells whose hot state differs from the start of the batch, sorted.
    pub(super) fn take_transitions(&mut self) -> Vec<Cell> {
        let mut out: Vec<Cell> = self
            .touched
            .drain()
            .filter(|&(c, was)| was != self.count.contains_key(&c))
            .map(|(c, _)| c)
            .collect();
        out.sort_unstable();
        out
    }

    /// Hot cells whose box shares a bucket with `cover`; duplicates possible.
    pub(super) fn near(&self, cover: Cover, visit: impl FnMut(Cell)) {
        self.index.query(cover, visit);
    }

    pub(super) fn len(&self) -> usize {
        self.count.len()
    }

    /// Asserts the reference counts equal `expected` exactly.
    pub(super) fn assert_counts(&self, expected: &FastMap<Cell, u32>) {
        assert_eq!(
            self.count.len(),
            expected.len(),
            "hot cells: {} held, {} produced by live events",
            self.count.len(),
            expected.len()
        );
        for (c, n) in expected {
            assert_eq!(
                self.count.get(c),
                Some(n),
                "hot cells: wrong count for {c:?}"
            );
        }
    }
}
