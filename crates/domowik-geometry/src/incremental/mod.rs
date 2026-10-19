//! Incremental plan geometry: occupied region and rooms of one level, kept
//! up to date edit by edit (BEHAVIOR-SPEC R3.4.8, issue 0013).
//!
//! # Contract
//!
//! After any sequence of [`LevelGeometry::sync`] or [`LevelGeometry::apply`]
//! calls, [`LevelGeometry::occupied`] and [`LevelGeometry::rooms`] equal
//! `walls::plan` of the same inputs EXACTLY — same polygons, same cells,
//! same order. That equality is the test oracle (`tests` below). The work
//! counters in [`EditStats`] are cheap diagnostics; the locality oracle is
//! the opt-in [`EditAudit`] ([`LevelGeometry::set_audit`]), checked against
//! an independently computed permission in `domowik-bench`.
//!
//! Inputs follow the crate's numerical contract: `sync` takes validated
//! level solids (finite, within `DERIVED_COORD_BOUND_MM`, so scaled cell
//! indices fit `i64` with room to spare). A `sync` after an explicit
//! [`LevelGeometry::apply`] reconciles every source once (O(level)), because
//! `apply` can change sources behind the document diff. Keys handed out
//! ([`RingKey`], provenance tags) are valid until the update that removes
//! them; a removed key never names another ring. Malformed internal state is
//! a panic ([`LevelGeometry::validate`] names it), never a silent wrong
//! answer.
//!
//! # Pipeline
//!
//! The full builder (`overlay::Arrangement::build`) runs five stages over
//! everything. This engine keeps each stage's state and updates it from the
//! previous stage's CHANGES only:
//!
//! | Stage | State | Recomputed for |
//! |---|---|---|
//! | [`sources`] | ring per document identity | changed walls and joins |
//! | [`hot`] | reference-counted hot cells | events of added / removed segments |
//! | [`routing`] | segment routes, net edge weights | new segments, and segments meeting a cell whose hot state flipped |
//! | [`dcel`] | half-edges, faces, windings | faces at vertices whose rotation changed; windings inside the changed edges' box |
//! | [`rings`] | occupied / free rings, hole owners | rings through vertices where a boundary changed |
//!
//! A change that reaches far — deleting the wall that closes a large room,
//! or covering an island — does proportionally more work, because the output
//! really changes that much. What never happens is work on geometry the edit
//! cannot affect.
//!
//! # Provenance (opt-in)
//!
//! [`LevelGeometry::with_provenance`] additionally keeps, per directed edge,
//! which sources run along it ([`provenance`]), and the occupied rings carry
//! those tags in their canonical runs — the incremental counterpart of
//! `walls::occupied_region_with_sources`, which snapping needs (R6.1.3,
//! issue 0012). Each update reports which occupied rings were removed, added
//! or re-tagged ([`LevelGeometry::occupied_changes`]), so an index over the
//! boundary can follow along without rescanning it.
//!
//! # Why not rebuild the R5.1.9 closure
//!
//! Codex review of 0013 (2026-10-08) reproduced two failures of
//! closure-scoped rebuilding: a room bounded by four corner-touching walls
//! needs the unchanged three to be built at all, and a wall too small to
//! occupy anything still produces a hot cell that reroutes its neighbour.
//! This engine tracks the dependencies that actually exist — hot-cell
//! producers and incidences — rather than the component relation, and
//! ED-0111 records that R5.1.9's permitted scope includes them.
//!
//! # Debugging
//!
//! [`LevelGeometry::validate`] checks every invariant of every stage and
//! panics naming the first one broken. [`LevelGeometry::last_edit`] says how
//! much each stage did. A mismatch against the full builder is easiest to
//! localize by validating after each `apply` of a failing sequence.

mod dcel;
mod hot;
mod index;
mod provenance;
mod rings;
mod routing;
mod sources;

pub use rings::RingKey;
pub use sources::{SourceChange, SourceKey};

use crate::boolean::BoundaryRun;
use crate::cuts::Aperture;
use crate::overlay::{scale, to_points, FastMap};
use crate::polygon::{canonical_ring, Polygon, Winding};
use crate::rooms::SeedPlacement;
use crate::walls::{LevelSolids, WallSolid};
use dcel::Dcel;
use domowik_format::Point;
use hot::HotCells;
use provenance::Provenance;
use rings::{Located, RingSet, Side};
use routing::{EdgeDelta, Routing, SegId};
use uuid::Uuid;

/// A cut as the sync cache compares it: span ends and depth as bits, and
/// the face (`false` = Left).
type CutKey = (u64, u64, bool, u64);

fn cut_key(a: &Aperture) -> CutKey {
    (
        a.s.0.to_bits(),
        a.s.1.to_bits(),
        matches!(a.side, domowik_format::Side::Right),
        a.depth.to_bits(),
    )
}

/// The source key of a wall's `k`-th piece: its own id for the first, so an
/// uncut wall keeps exactly the key the unamended engine gives it.
fn piece_key(wall: Uuid, k: u32) -> SourceKey {
    if k == 0 {
        SourceKey::Wall(wall)
    } else {
        SourceKey::WallPiece { wall, piece: k }
    }
}

/// How much work one edit did, stage by stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditStats {
    /// Sources inserted, replaced or removed.
    pub sources_changed: usize,
    /// Segments withdrawn / inserted.
    pub segments_removed: usize,
    pub segments_added: usize,
    /// Cells that became hot or stopped being hot.
    pub hot_transitions: usize,
    /// Existing segments meeting a flipped cell.
    pub segments_marked: usize,
    /// Segments whose route actually changed.
    pub segments_rerouted: usize,
    /// Integer edges whose net weight changed.
    pub edges_changed: usize,
    /// Faces dropped and re-traced.
    pub faces_dropped: usize,
    pub faces_created: usize,
    /// Kept faces whose winding shifted.
    pub faces_rewound: usize,
    /// New faces located by ray cast rather than from a neighbour.
    pub ray_casts: usize,
    /// Occupied and free rings re-traced.
    pub occupied_rings_retraced: usize,
    pub free_rings_retraced: usize,
    /// Directed edges whose set of contributing sources changed (0 without
    /// provenance).
    pub tag_edges_changed: usize,
    /// Kept occupied rings whose tags were recomputed in place.
    pub occupied_rings_retagged: usize,
}

#[derive(Debug, Clone)]
struct Source {
    /// The ring exactly as supplied, for change detection.
    raw: Vec<Point>,
    segs: Vec<SegId>,
}

/// One level's plan geometry, maintained incrementally.
#[derive(Debug, Clone)]
pub struct LevelGeometry {
    sources: FastMap<SourceKey, (u32, Source)>,
    /// Source index → key, for reading boundary tags.
    keys: FastMap<u32, SourceKey>,
    next_source: u32,
    /// The last synced solid of every wall, with the cuts it was synced with
    /// and how many source pieces that produced, for `sync`'s diff.
    walls: FastMap<Uuid, (WallSolid, Vec<CutKey>, u32)>,
    hot: HotCells,
    routing: Routing,
    dcel: Dcel,
    /// Present only in provenance mode.
    provenance: Option<Provenance>,
    occupied: RingSet,
    free: RingSet,
    last: EditStats,
    /// Whether `walls` still describes the sources, so `sync` may diff
    /// against it. A public [`Self::apply`] can change sources behind the
    /// cache's back; the next `sync` then reconciles every source instead.
    walls_current: bool,
    /// False for an occupied-only engine ([`LevelGeometry::occupied_only`]):
    /// free rings, and with them rooms, are not maintained.
    rooms: bool,
    /// Present while auditing (see [`EditAudit`]).
    audit: Option<EditAudit>,
}

/// What the last `sync` or `apply` recomputed, recorded only while auditing
/// ([`LevelGeometry::set_audit`]) — for locality checks (R5.1.9, ED-0111),
/// never on a product path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditAudit {
    /// Walls whose source ring was inserted, replaced or withdrawn, sorted.
    pub wall_sources: Vec<Uuid>,
    /// Join source changes (insertions, replacements, withdrawals).
    pub join_sources: usize,
    /// Rooms (R5.1.1) rebuilt or patched, by the canonical points of their
    /// outer ring: as they were BEFORE the edit (removed or patched) and as
    /// they are AFTER it (inserted or patched). Sorted.
    pub rooms_before: Vec<Vec<Point>>,
    pub rooms_after: Vec<Vec<Point>>,
    /// The same for occupied-region polygons (for the drawn-region engine,
    /// whose output IS its occupied region).
    pub occupied_before: Vec<Vec<Point>>,
    pub occupied_after: Vec<Vec<Point>>,
}

impl Default for LevelGeometry {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelGeometry {
    /// An empty level.
    pub fn new() -> Self {
        Self::with_tracking(false)
    }

    /// An empty level that also tracks boundary provenance (see the module
    /// documentation). Costs one ledger update per route piece.
    pub fn with_provenance() -> Self {
        Self::with_tracking(true)
    }

    fn with_tracking(provenance: bool) -> Self {
        Self {
            sources: FastMap::default(),
            keys: FastMap::default(),
            next_source: 0,
            walls: FastMap::default(),
            hot: HotCells::new(),
            routing: Routing::new(provenance),
            dcel: Dcel::new(),
            provenance: provenance.then(Provenance::default),
            occupied: RingSet::new(Side::Occupied),
            free: RingSet::new(Side::Free),
            last: EditStats::default(),
            walls_current: true,
            rooms: true,
            audit: None,
        }
    }

    /// An empty level that maintains only the occupied region, for the
    /// drawn region with opening cuts ([`Self::sync_cut`], R4.2.2): the
    /// free-side rings are skipped, so [`Self::rooms`] is always empty.
    pub fn occupied_only() -> Self {
        let mut g = Self::with_tracking(false);
        g.rooms = false;
        g
    }

    /// Builds the geometry of `solids` from scratch.
    pub fn from_solids(solids: &LevelSolids) -> Self {
        let mut g = Self::new();
        g.sync(solids);
        g
    }

    /// Brings the engine to `solids`, diffing by document identity: only
    /// walls whose solid changed, and joins that are new or have a changed
    /// member, are re-processed. The diff itself compares one `WallSolid` per
    /// wall and builds one member key per join; no ring is computed for an
    /// unchanged wall or join.
    ///
    /// After an explicit [`Self::apply`], the first `sync` reconciles every
    /// source against `solids` (comparing rings, so unchanged sources are
    /// not re-routed) and removes sources `solids` does not name: a one-off
    /// O(level) pass that restores the equality with a full build.
    pub fn sync(&mut self, solids: &LevelSolids) -> &EditStats {
        self.sync_cut(solids, &[])
    }

    /// [`Self::sync`] with opening cuts (R4.2.2): each wall contributes its
    /// pieces after its own `apertures` ([`crate::cuts::wall_pieces`]) —
    /// several sources per wall where a cut disconnects it — and join
    /// patches are never cut. A wall is re-sourced only when its solid or
    /// its set of cuts changed. Equal to `cuts::drawn_region` of the same
    /// inputs; meant for an [`Self::occupied_only`] engine, since rooms
    /// never see cuts (R4.2.5).
    pub fn sync_cut(&mut self, solids: &LevelSolids, apertures: &[Aperture]) -> &EditStats {
        let resync = !self.walls_current;
        if resync {
            self.walls.clear();
        }
        let mut changes = Vec::new();
        let mut changed: FastMap<Uuid, ()> = FastMap::default();
        let mut present: FastMap<Uuid, ()> = FastMap::default();
        present.reserve(solids.ids.len());
        let mut cuts_of: Vec<Vec<Aperture>> = Vec::new();
        if !apertures.is_empty() {
            cuts_of = vec![Vec::new(); solids.walls.len()];
            for a in apertures {
                if let Some(list) = cuts_of.get_mut(a.wall) {
                    list.push(*a);
                }
            }
        }
        for (i, (wall, &id)) in solids.walls.iter().zip(&solids.ids).enumerate() {
            present.insert(id, ());
            let cuts = cuts_of.get(i).map_or(&[][..], Vec::as_slice);
            let mut key_cuts: Vec<CutKey> = cuts.iter().map(cut_key).collect();
            key_cuts.sort_unstable();
            let old = self.walls.get(&id);
            if old.is_some_and(|(w, c, _)| w == wall && *c == key_cuts) {
                continue;
            }
            let old_pieces = old.map_or(0, |o| o.2);
            let mut pieces = if cuts.is_empty() {
                vec![wall.rectangle().map(|r| r.to_vec()).unwrap_or_default()]
            } else {
                crate::cuts::wall_pieces(wall, cuts)
            };
            if pieces.is_empty() {
                pieces.push(Vec::new());
            }
            let n = pieces.len() as u32;
            self.walls.insert(id, (*wall, key_cuts, n));
            let mut any = false;
            for (k, ring) in pieces.into_iter().enumerate() {
                let key = piece_key(id, k as u32);
                if self.sources.get(&key).map(|(_, s)| &s.raw) == Some(&ring) {
                    continue;
                }
                any = true;
                changes.push(SourceChange::Upsert(key, ring));
            }
            for k in n..old_pieces {
                any = true;
                changes.push(SourceChange::Remove(piece_key(id, k)));
            }
            if any {
                changed.insert(id, ());
            }
        }
        let mut gone = Vec::new();
        self.walls.retain(|id, (_, _, n)| {
            let keep = present.contains_key(id);
            if !keep {
                gone.push((*id, *n));
            }
            keep
        });
        for (id, n) in gone {
            changed.insert(id, ());
            for k in 0..n.max(1) {
                changes.push(SourceChange::Remove(piece_key(id, k)));
            }
        }
        if resync {
            // The cache was cleared: drop every wall source the new state
            // does not produce (absent walls, surplus pieces).
            for key in self.sources.keys() {
                let stale = match key {
                    SourceKey::Wall(id) => !present.contains_key(id),
                    SourceKey::WallPiece { wall, piece } => {
                        self.walls.get(wall).is_none_or(|(_, _, n)| *piece >= *n)
                    }
                    SourceKey::Join { .. } => false,
                };
                if stale {
                    changes.push(SourceChange::Remove(key.clone()));
                }
            }
        }

        // One member key per join, built once: this loop runs over every
        // join on every sync, so it stays allocation-light and uses the fast
        // hasher (it was 90% of a no-op sync of a 10,000-wall joined plan).
        let mut joins_present: FastMap<SourceKey, ()> = FastMap::default();
        joins_present.reserve(solids.joins.len());
        for join in &solids.joins {
            let mut key = sources::join_key(solids, join);
            // A malformed document can repeat a member set; occurrences are
            // numbered in document order, as `sources::join_keys` does.
            while joins_present.contains_key(&key) {
                if let SourceKey::Join { ordinal, .. } = &mut key {
                    *ordinal += 1;
                }
            }
            let touched = match &key {
                SourceKey::Join { members, .. } => {
                    members.iter().any(|(id, _)| changed.contains_key(id))
                }
                SourceKey::Wall(_) | SourceKey::WallPiece { .. } => {
                    unreachable!("join_key returns a join key")
                }
            };
            if resync || touched || !self.sources.contains_key(&key) {
                let patch = crate::walls::join_patch(&solids.walls, join);
                let ring = if patch.len() >= 3 { patch } else { Vec::new() };
                if self.sources.get(&key).map(|(_, s)| &s.raw) != Some(&ring) {
                    changes.push(SourceChange::Upsert(key.clone(), ring));
                }
            }
            joins_present.insert(key, ());
        }
        for key in self.sources.keys() {
            if matches!(key, SourceKey::Join { .. }) && !joins_present.contains_key(key) {
                changes.push(SourceChange::Remove(key.clone()));
            }
        }
        self.walls_current = true;
        self.apply_changes(changes)
    }

    /// Applies explicit source changes as one batch.
    ///
    /// The engine's document cache no longer describes its sources after
    /// this, so the next [`Self::sync`] reconciles every source once.
    pub fn apply(&mut self, changes: Vec<SourceChange>) -> &EditStats {
        self.walls_current = false;
        self.apply_changes(changes)
    }

    /// Starts (or stops) recording an [`EditAudit`] per update. Auditing
    /// keeps a few vectors per update; it is for tests and measurement.
    pub fn set_audit(&mut self, on: bool) {
        self.audit = on.then(EditAudit::default);
        self.free.set_audit(on);
        self.occupied.set_audit(on);
    }

    /// The last update's audit, when auditing.
    pub fn last_audit(&self) -> Option<&EditAudit> {
        self.audit.as_ref()
    }

    fn apply_changes(&mut self, changes: Vec<SourceChange>) -> &EditStats {
        if let Some(audit) = &mut self.audit {
            *audit = EditAudit::default();
            for change in &changes {
                let (SourceChange::Upsert(key, _) | SourceChange::Remove(key)) = change;
                match key {
                    SourceKey::Wall(id) | SourceKey::WallPiece { wall: id, .. } => {
                        audit.wall_sources.push(*id)
                    }
                    SourceKey::Join { .. } => audit.join_sources += 1,
                }
            }
            audit.wall_sources.sort_unstable();
            audit.wall_sources.dedup();
        }
        let mut stats = EditStats {
            sources_changed: changes.len(),
            ..EditStats::default()
        };
        let mut delta: EdgeDelta = FastMap::default();

        // 1. Withdraw replaced and removed sources.
        let mut upserts = Vec::new();
        for change in changes {
            let (key, ring) = match change {
                SourceChange::Upsert(k, r) => (k, Some(r)),
                SourceChange::Remove(k) => (k, None),
            };
            if let Some((index, old)) = self.sources.remove(&key) {
                self.keys.remove(&index);
                for id in old.segs {
                    self.routing.remove(id, &mut self.hot, &mut delta);
                    stats.segments_removed += 1;
                }
            }
            if let Some(r) = ring {
                upserts.push((key, r));
            }
        }

        // 2. Insert new rings. Canonicalized exactly as the full builder does,
        //    so a degenerate ring contributes nothing in both.
        for (key, raw) in upserts {
            let index = self.next_source;
            self.next_source += 1;
            let mut segs = Vec::new();
            if let Some(pts) = canonical_ring(&raw, Winding::CounterClockwise) {
                for i in 0..pts.len() {
                    let (a, b) = (scale(pts[i]), scale(pts[(i + 1) % pts.len()]));
                    segs.push(self.routing.add(a, b, index, &mut self.hot));
                    stats.segments_added += 1;
                }
            }
            self.keys.insert(index, key.clone());
            self.sources.insert(key, (index, Source { raw, segs }));
        }

        // 3. Reroute whatever the settled hot set affects.
        let flipped = self.hot.take_transitions();
        stats.hot_transitions = flipped.len();
        stats.segments_marked = self.routing.mark_meeting(&flipped);
        stats.segments_rerouted = self.routing.route_pending(&self.hot, &mut delta);

        // 4. Ownership, faces and windings, then rings.
        let retag = match (&mut self.provenance, self.routing.take_tags()) {
            (Some(prov), Some(mut tags)) => prov.apply(&mut tags),
            _ => Vec::new(),
        };
        stats.tag_edges_changed = retag.len();
        let change = self.dcel.apply(&delta);
        stats.edges_changed = change.changed_edges;
        stats.faces_dropped = change.dropped_faces;
        stats.faces_created = change.new_faces.len();
        stats.faces_rewound = change.rewound.len();
        stats.ray_casts = change.ray_casts;
        stats.occupied_rings_retraced =
            self.occupied
                .update(&self.dcel, &change, self.provenance.as_ref(), &retag);
        stats.occupied_rings_retagged = self.occupied.changes().retagged.len();
        if self.rooms {
            stats.free_rings_retraced = self.free.update(&self.dcel, &change, None, &[]);
        }
        if let (Some(audit), Some(rooms)) = (&mut self.audit, self.free.audit()) {
            if self.rooms {
                audit.rooms_before = rooms.before.clone();
                audit.rooms_after = rooms.after.clone();
            }
        }
        if let (Some(audit), Some(occupied)) = (&mut self.audit, self.occupied.audit()) {
            audit.occupied_before = occupied.before.clone();
            audit.occupied_after = occupied.after.clone();
        }

        self.last = stats;
        &self.last
    }

    /// The occupied region (R3.2.1), canonical, in `walls::plan(..).0`'s
    /// order. Borrowed: iterating costs nothing beyond the polygons read.
    pub fn occupied_iter(&self) -> impl ExactSizeIterator<Item = &Polygon> {
        self.occupied.polygons()
    }

    /// The rooms (R5.1.1), canonical, in `walls::plan(..).1`'s order.
    pub fn rooms_iter(&self) -> impl ExactSizeIterator<Item = &Polygon> {
        self.free.polygons()
    }

    /// [`Self::occupied_iter`], cloned.
    pub fn occupied(&self) -> Vec<Polygon> {
        self.occupied_iter().cloned().collect()
    }

    /// [`Self::rooms_iter`], cloned.
    pub fn rooms(&self) -> Vec<Polygon> {
        self.rooms_iter().cloned().collect()
    }

    /// Both, cloned, as `walls::plan` returns them.
    pub fn plan(&self) -> (Vec<Polygon>, Vec<Polygon>) {
        (self.occupied(), self.rooms())
    }

    /// Where a room seed lies (R5.2.4): exactly what classifying it against
    /// every polygon of [`Self::occupied`] and then of [`Self::rooms`] gives,
    /// boundary settled against the occupied region first, the room as its
    /// ordinal in [`Self::rooms`]. Walks only the rings whose bounds hold
    /// the seed, using the retained ring buckets and hole ownership — not a
    /// logarithmic bound: rings sharing a ≈16 m bucket are all visited.
    pub fn locate_seed(&self, seed: Point) -> SeedPlacement {
        match self.occupied.locate(seed) {
            Located::Boundary => return SeedPlacement::OnBoundary,
            Located::Inside(_) => return SeedPlacement::InWall,
            Located::Outside => {}
        }
        match self.free.locate(seed) {
            Located::Boundary => SeedPlacement::OnBoundary,
            Located::Inside(key) => match self.free.ordinals(key) {
                Some((room, 0)) => SeedPlacement::InFace(room),
                _ => unreachable!("a located room outer is an output polygon"),
            },
            Located::Outside => SeedPlacement::InExterior,
        }
    }

    /// The occupied rings the most recent `sync` or `apply` removed, added
    /// and re-tagged. An index over the boundary that applies these after
    /// EVERY update stays equal to one built from scratch.
    pub fn occupied_changes(&self) -> OccupiedChanges<'_> {
        let c = self.occupied.changes();
        OccupiedChanges {
            removed: &c.removed,
            added: &c.added,
            retagged: &c.retagged,
        }
    }

    /// A live occupied ring: its canonical points and, in provenance mode,
    /// the sources of each piece. `None` for a stale key or a ring that
    /// collapsed to no area.
    pub fn occupied_ring(&self, key: RingKey) -> Option<OccupiedRing<'_>> {
        let (points, runs) = self.occupied.view(key)?;
        Some(OccupiedRing { points, runs })
    }

    /// `(component ordinal, ring ordinal)` of an occupied ring in
    /// [`Self::occupied`]'s output — ring 0 the outer, `1 + i` hole `i` — or
    /// `None` when the ring is not part of the output. `O(component
    /// ordinal)`; meant for the few rings a query reports.
    pub fn occupied_ordinals(&self, key: RingKey) -> Option<(usize, usize)> {
        self.occupied.ordinals(key)
    }

    /// The source a boundary tag names. Tags are only issued in provenance
    /// mode and are valid while their source is.
    pub fn source_key(&self, tag: usize) -> Option<&SourceKey> {
        u32::try_from(tag).ok().and_then(|t| self.keys.get(&t))
    }

    /// Work done by the most recent `sync` or `apply`.
    pub fn last_edit(&self) -> &EditStats {
        &self.last
    }

    /// Sizes of the maintained state, for diagnostics.
    pub fn size(&self) -> (usize, usize, usize) {
        (self.routing.len(), self.hot.len(), self.dcel.edge_count())
    }

    /// Checks every invariant of every stage; panics naming the first one
    /// broken. Proportional to the whole level — for tests and debugging.
    pub fn validate(&self) {
        self.routing.validate(&self.hot);
        if let Some(prov) = &self.provenance {
            prov.validate(self.routing.routes());
        }
        self.dcel.validate();
        self.occupied.validate(&self.dcel, self.provenance.as_ref());
        if self.rooms {
            self.free.validate(&self.dcel, None);
        }
    }
}

/// See [`LevelGeometry::occupied_changes`].
#[derive(Debug, Clone, Copy)]
pub struct OccupiedChanges<'a> {
    pub removed: &'a [RingKey],
    pub added: &'a [RingKey],
    pub retagged: &'a [RingKey],
}

/// See [`LevelGeometry::occupied_ring`].
#[derive(Debug, Clone, Copy)]
pub struct OccupiedRing<'a> {
    /// Canonical vertices, as in the output polygon (R3.2.4).
    pub points: &'a [Point],
    runs: &'a [(Vec<crate::overlay::Cell>, Vec<Vec<usize>>)],
}

impl OccupiedRing<'_> {
    /// One run per canonical edge (edge `i` runs `points[i] → points[i+1]`):
    /// the collinear vertices canonicalization removed, and the sorted tags
    /// of each piece between them — exactly `walls::occupied_region_with_
    /// sources`'s runs, with tags naming sources by [`LevelGeometry::
    /// source_key`] instead of by position. Empty tag lists without
    /// provenance.
    pub fn runs(&self) -> impl ExactSizeIterator<Item = BoundaryRun> + '_ {
        self.runs.iter().map(|(breaks, pieces)| BoundaryRun {
            breaks: to_points(breaks),
            sources: pieces
                .iter()
                .map(|p| {
                    let mut t = p.clone();
                    t.sort_unstable();
                    t.dedup();
                    t
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
