//! Which sources run along each directed integer edge.
//!
//! The full builder tags every half-edge with the sources whose route pieces
//! run in its direction (`overlay::Arrangement::build` with `track_tags`);
//! snapping reads those tags off the occupied boundary to tell a dragged
//! wall's own face from a static neighbour's (R6.1.3).
//!
//! # Why a separate ledger
//!
//! The DCEL's edge weights are NET windings: a piece of source A replaced by
//! an identical piece of source B leaves every weight, face and ring
//! unchanged while changing every exclusion decision along it. Tags therefore
//! cannot be recovered from [`super::routing::EdgeDelta`]; they are counted
//! here, from the same route contributions, independently of the geometry.
//!
//! # What is counted
//!
//! For every DIRECTED cell pair `(from, to)` — one route piece's direction —
//! a count per source of the live route pieces running that way. Counts, not
//! sets: one source can run twice along the same piece (two collinear edges
//! of a degenerate ring), and withdrawing one must not erase the other. The
//! ledger also covers edges whose weights cancel (interior or back-to-back),
//! so a boundary exposed later already carries its tags.

use crate::overlay::{Cell, FastMap};

/// Per-batch tag changes: `((from, to), source, ±1)` per route piece.
pub(super) type TagDelta = Vec<((Cell, Cell), u32, i32)>;

#[derive(Debug, Clone, Default)]
pub(super) struct Provenance {
    /// `(from, to)` → `(source, count)`, sorted by source, counts positive.
    owners: FastMap<(Cell, Cell), Vec<(u32, u32)>>,
}

impl Provenance {
    /// Applies a batch's contributions. Returns the directed edges whose
    /// SET of sources changed, which is what can change a boundary run's
    /// tags. A piece withdrawn and re-added within the batch nets to nothing
    /// and is not reported.
    pub(super) fn apply(&mut self, delta: &mut TagDelta) -> Vec<(Cell, Cell)> {
        delta.sort_unstable_by_key(|&(k, s, _)| (k, s));
        let mut changed = Vec::new();
        let mut i = 0;
        while i < delta.len() {
            let (key, source, _) = delta[i];
            let mut net = 0i64;
            while i < delta.len() && delta[i].0 == key && delta[i].1 == source {
                net += delta[i].2 as i64;
                i += 1;
            }
            if net == 0 {
                continue;
            }
            let list = self.owners.entry(key).or_default();
            let at = list.binary_search_by_key(&source, |&(s, _)| s);
            let before = at.map(|j| list[j].1).unwrap_or(0) as i64;
            let after = before + net;
            assert!(
                after >= 0,
                "provenance: source {source} withdrawn from {key:?} more often than added"
            );
            match (at, after) {
                (Ok(j), 0) => {
                    list.remove(j);
                }
                (Ok(j), n) => list[j].1 = n as u32,
                (Err(j), n) => list.insert(j, (source, n as u32)),
            }
            if list.is_empty() {
                self.owners.remove(&key);
            }
            if (before == 0) != (after == 0) && changed.last() != Some(&key) {
                changed.push(key);
            }
        }
        changed
    }

    /// The sources running `from → to`, ascending.
    pub(super) fn sources(&self, from: Cell, to: Cell) -> Vec<usize> {
        self.owners
            .get(&(from, to))
            .map(|l| l.iter().map(|&(s, _)| s as usize).collect())
            .unwrap_or_default()
    }

    /// Checks the ledger against a recount from every live route.
    pub(super) fn validate<'a>(&self, routes: impl Iterator<Item = (u32, &'a [Cell])>) {
        let mut expected: FastMap<(Cell, Cell), Vec<(u32, u32)>> = FastMap::default();
        for (source, route) in routes {
            for w in route.windows(2) {
                let list = expected.entry((w[0], w[1])).or_default();
                match list.binary_search_by_key(&source, |&(s, _)| s) {
                    Ok(j) => list[j].1 += 1,
                    Err(j) => list.insert(j, (source, 1)),
                }
            }
        }
        assert_eq!(
            expected.len(),
            self.owners.len(),
            "provenance: ledger covers a different set of directed edges"
        );
        for (key, list) in &expected {
            assert_eq!(
                self.owners.get(key),
                Some(list),
                "provenance: owners of {key:?} disagree with the routes"
            );
        }
    }
}
