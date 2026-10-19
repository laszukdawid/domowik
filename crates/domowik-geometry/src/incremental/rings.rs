//! Boundary rings of the occupied region and of free space, kept per ring.
//!
//! The full builder traces every boundary ring on every call. Here a ring is
//! re-traced only when something along it changed, and hole ownership is
//! repaired only for the rings involved.
//!
//! # The two sides
//!
//! - OCCUPIED rings follow half-edges with an occupied face on the left and
//!   a free face on the right. Positive-area rings are polygon outers,
//!   negative ones their holes (R3.2.1).
//! - FREE rings follow the opposite half-edges. Positive-area free rings are
//!   rooms (R5.1.1); their holes are islands. A negative free ring that no
//!   free outer encloses is the level's exterior, never a room (R5.1.2).
//!
//! The two are not reverses of each other: at a pinch vertex the tracing
//! rule splits occupied rings per component but keeps a room ring whole, as
//! in the full builder.
//!
//! # When a ring is invalid
//!
//! A ring's walk takes, at each vertex, the first boundary half-edge
//! clockwise from the way back. It can change only if one of its half-edges
//! vanished or stopped being boundary, or the choice at one of its vertices
//! changed — which needs a rotation change or a boundary-status change of a
//! half-edge at that vertex. Rings through such vertices are dropped and
//! re-traced; every other ring keeps its id, its canonical form and its hole
//! assignment.
//!
//! # Provenance
//!
//! The occupied side can carry the source tags of every boundary piece
//! (issue 0012, for snapping). A ring's tags can change while its geometry
//! does not — a piece re-owned by an identical source — so such a ring is
//! RE-TAGGED in place: its canonical runs are recomputed and it keeps its
//! key, its polygon and its hole links. Every update records which rings
//! were removed, added and re-tagged in [`RingChanges`], for indexes built
//! on top of the rings.
//!
//! # Hole ownership
//!
//! A hole belongs to the SMALLEST outer that encloses it, the full builder's
//! rule. Kept incrementally: a new or orphaned hole searches outers whose
//! bounds contain it; a new outer adopts holes in its bounds that it encloses
//! and that currently have a larger owner, or none.

use super::dcel::{Dcel, DcelChange, He};
use super::index::{BucketIndex, Cover, COARSE};
use super::provenance::Provenance;
use crate::arrangement::{classify_point, Containment};
use crate::overlay::{
    canonical_cells, cell_bounds, double_area, ring_within, to_points, Cell, CellRing, FastMap,
    CELLS_PER_MM,
};
use crate::polygon::{compare_rings, Polygon};
use domowik_format::Point;
use std::collections::BTreeMap;

/// Where a point lies against one side's output polygons ([`RingSet::locate`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Located {
    /// Strictly inside the polygon whose outer ring this is.
    Inside(RingKey),
    /// On a ring of an output polygon.
    Boundary,
    /// In no output polygon.
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Occupied,
    Free,
}

/// A boundary ring's identity across updates: its slot plus a serial that
/// is never reused, so a key held by an index cannot silently name a
/// different ring that later took the same slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RingKey {
    id: u32,
    serial: u64,
}

/// The rings one update removed, added and re-tagged. A removed key is never
/// valid again; an added or re-tagged key is valid until a later update
/// removes it.
#[derive(Debug, Clone, Default)]
pub(super) struct RingChanges {
    pub(super) removed: Vec<RingKey>,
    pub(super) added: Vec<RingKey>,
    pub(super) retagged: Vec<RingKey>,
}

#[derive(Debug, Clone)]
struct Ring {
    serial: u64,
    hes: Vec<He>,
    /// `None` when the walk collapses to no area; such a ring is kept for
    /// invalidation but never output.
    canon: Option<CellRing>,
    area2: i128,
    bbox: [i64; 4],
    /// The canonical cells as output points, computed once at trace time.
    points: Vec<Point>,
    /// For a hole: its owning outer.
    parent: Option<u32>,
    /// For an outer: its holes.
    children: Vec<u32>,
}

impl Ring {
    fn is_outer(&self) -> bool {
        self.canon.is_some() && self.area2 > 0
    }

    fn is_hole(&self) -> bool {
        self.canon.is_some() && self.area2 < 0
    }

    fn cells(&self) -> &[Cell] {
        &self
            .canon
            .as_ref()
            .expect("only canonical rings are compared")
            .cells
    }

    fn cover(&self) -> Cover {
        let b = self.bbox;
        Cover::Box([b[0] as f64, b[1] as f64, b[2] as f64, b[3] as f64])
    }
}

fn contains(outer: &[i64; 4], inner: &[i64; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

#[derive(Debug, Clone)]
pub(super) struct RingSet {
    side: Side,
    ring_of: FastMap<He, u32>,
    rings: Vec<Option<Ring>>,
    free: Vec<u32>,
    outers: BucketIndex<u32>,
    holes: BucketIndex<u32>,
    /// Output polygons keyed by their outer ring's canonical cells. Two
    /// outers of one arrangement never share cells, so this key alone gives
    /// the full builder's polygon order (`overlay::polygon_cmp`).
    polygons: BTreeMap<Vec<Cell>, Polygon>,
    /// Outer ring id → its polygon's position in `polygons`, rebuilt lazily
    /// after the SET of polygons changed (`None` = stale). Hole patches do
    /// not move polygons, so a typical edit inside one component keeps it.
    ranks: std::cell::RefCell<Option<FastMap<u32, usize>>>,
    next_serial: u64,
    changes: RingChanges,
    /// Present while auditing: the output polygons the last update rebuilt
    /// or patched (see [`PolygonAudit`]).
    audit: Option<PolygonAudit>,
}

/// The output polygons one update touched, by the canonical points of their
/// outer ring — a value identity, valid within the state it names.
/// `before`: polygons of the previous state that were removed or patched.
/// `after`: polygons of the new state that were inserted or patched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PolygonAudit {
    pub before: Vec<Vec<Point>>,
    pub after: Vec<Vec<Point>>,
    /// Keys of patched polygons, resolved into `after` at the end of the
    /// update (a patched polygon may still be removed later in it).
    patched: Vec<Vec<Cell>>,
}

impl RingSet {
    pub(super) fn new(side: Side) -> Self {
        Self {
            next_serial: 0,
            changes: RingChanges::default(),
            audit: None,
            side,
            ring_of: FastMap::default(),
            rings: Vec::new(),
            free: Vec::new(),
            outers: BucketIndex::new(COARSE),
            holes: BucketIndex::new(COARSE),
            polygons: BTreeMap::new(),
            ranks: std::cell::RefCell::new(None),
        }
    }

    fn key(&self, r: u32) -> RingKey {
        RingKey {
            id: r,
            serial: self.ring(r).serial,
        }
    }

    /// Every live ring's key, in slot order (for tests and debugging).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn keys(&self) -> impl Iterator<Item = RingKey> + '_ {
        self.rings.iter().enumerate().filter_map(|(r, ring)| {
            ring.as_ref().map(|ring| RingKey {
                id: r as u32,
                serial: ring.serial,
            })
        })
    }

    /// The rings the last update removed, added and re-tagged.
    pub(super) fn changes(&self) -> &RingChanges {
        &self.changes
    }

    fn live(&self, key: RingKey) -> Option<&Ring> {
        self.rings
            .get(key.id as usize)?
            .as_ref()
            .filter(|r| r.serial == key.serial)
    }

    /// A live canonical ring's output points and per-edge runs (interior
    /// break cells, then one tag list per piece). `None` for a stale key or
    /// a ring that collapsed to no area.
    #[allow(clippy::type_complexity)]
    pub(super) fn view(&self, key: RingKey) -> Option<(&[Point], &[(Vec<Cell>, Vec<Vec<usize>>)])> {
        let ring = self.live(key)?;
        let canon = ring.canon.as_ref()?;
        Some((&ring.points, &canon.runs))
    }

    /// The ring's place in the output: `(polygon ordinal, ring ordinal)`,
    /// ring 0 being the outer and `1 + i` the polygon's `i`-th hole. `None`
    /// for a stale key, a collapsed ring, or a hole that no outer owns
    /// (dropped from the output, as in the full builder).
    ///
    /// The polygon ordinal comes from a rank table rebuilt (`O(k log k)` for
    /// `k` polygons) on the first call after the set of polygons changed,
    /// then `O(1)`. Counting preceding polygons per call instead made every
    /// edge candidate of a late component cost `O(k)` (Codex GPT-6.1-Sol
    /// review: 8 µs per query at 5,000 components).
    pub(super) fn ordinals(&self, key: RingKey) -> Option<(usize, usize)> {
        let ring = self.live(key)?;
        let (outer, ordinal) = if ring.is_outer() {
            (ring, 0)
        } else if ring.is_hole() {
            let outer = self.ring(ring.parent?);
            let polygon = self.polygons.get(outer.cells())?;
            let i = polygon
                .holes
                .binary_search_by(|h| compare_rings(h, &ring.points))
                .ok()?;
            (outer, 1 + i)
        } else {
            return None;
        };
        let outer_id = ring.parent.filter(|_| ordinal > 0).unwrap_or(key.id);
        debug_assert!(std::ptr::eq(self.ring(outer_id), outer));
        let mut ranks = self.ranks.borrow_mut();
        let ranks = ranks.get_or_insert_with(|| {
            let mut outers: Vec<(&[Cell], u32)> = self
                .rings
                .iter()
                .enumerate()
                .filter_map(|(r, ring)| {
                    let ring = ring.as_ref()?;
                    ring.is_outer().then(|| (ring.cells(), r as u32))
                })
                .collect();
            outers.sort_unstable_by(|a, b| a.0.cmp(b.0));
            outers
                .iter()
                .enumerate()
                .map(|(i, &(_, r))| (r, i))
                .collect()
        });
        Some((*ranks.get(&outer_id)?, ordinal))
    }

    fn ring(&self, r: u32) -> &Ring {
        self.rings[r as usize].as_ref().expect("rings: dead ring")
    }

    fn is_boundary(&self, dcel: &Dcel, he: He) -> bool {
        let occ = |h: He| dcel.winding_of(h) > 0;
        match self.side {
            Side::Occupied => occ(he) && !occ(he ^ 1),
            Side::Free => !occ(he) && occ(he ^ 1),
        }
    }

    /// The ring successor of boundary half-edge `he`.
    fn next_boundary(&self, dcel: &Dcel, he: He) -> He {
        let list = dcel.outgoing(dcel.dest(he));
        let i = dcel.position(he ^ 1);
        for k in 1..=list.len() {
            let c = list[(i + list.len() - k) % list.len()];
            if self.is_boundary(dcel, c) {
                return c;
            }
        }
        panic!(
            "rings: boundary half-edge {he} ({:?} -> {:?}) has no boundary continuation",
            dcel.origin(he),
            dcel.dest(he)
        );
    }

    /// Repairs the rings after a DCEL update. `prov` is the tag ledger when
    /// this side carries tags, and `retag` the directed edges whose sources
    /// changed in this batch. Returns the number of rings re-traced.
    pub(super) fn update(
        &mut self,
        dcel: &Dcel,
        change: &DcelChange,
        prov: Option<&Provenance>,
        retag: &[(Cell, Cell)],
    ) -> usize {
        self.changes = RingChanges::default();
        if let Some(audit) = &mut self.audit {
            *audit = PolygonAudit::default();
        }
        let retraced = self.repair(dcel, change, prov);
        if let Some(prov) = prov {
            self.retag(dcel, prov, retag);
        }
        retraced
    }

    /// Recomputes the runs of kept rings through an edge whose sources
    /// changed. Rings traced in this update already read the new ledger.
    fn retag(&mut self, dcel: &Dcel, prov: &Provenance, edges: &[(Cell, Cell)]) {
        let mut rings: Vec<u32> = edges
            .iter()
            .filter_map(|&(from, to)| dcel.half_edge(from, to))
            .filter_map(|he| self.ring_of.get(&he).copied())
            .collect();
        rings.sort_unstable();
        rings.dedup();
        for r in rings {
            let key = self.key(r);
            if self.changes.added.contains(&key) {
                continue;
            }
            let ring = self.rings[r as usize]
                .as_mut()
                .expect("ring of a live edge");
            if ring.canon.is_none() {
                continue;
            }
            let cells: Vec<Cell> = ring.hes.iter().map(|&h| dcel.origin(h)).collect();
            let tags = ring
                .hes
                .iter()
                .map(|&h| prov.sources(dcel.origin(h), dcel.dest(h)))
                .collect();
            ring.canon = canonical_cells(cells, tags, ring.area2 > 0);
            self.changes.retagged.push(key);
        }
    }

    fn repair(&mut self, dcel: &Dcel, change: &DcelChange, prov: Option<&Provenance>) -> usize {
        // Half-edges whose boundary status may have changed.
        let mut candidates: Vec<He> = Vec::new();
        for &f in change.new_faces.iter().chain(&change.rewound) {
            for &he in &dcel.face(f).hes {
                candidates.push(he);
                candidates.push(he ^ 1);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();

        let mut invalid: Vec<u32> = Vec::new();
        let mut vertices: Vec<Cell> = change.rot_vertices.clone();
        let mut entering: Vec<He> = Vec::new();
        for &he in &change.removed_hes {
            if let Some(&r) = self.ring_of.get(&he) {
                invalid.push(r);
            }
        }
        for &he in &candidates {
            if !dcel.alive(he) {
                continue;
            }
            let now = self.is_boundary(dcel, he);
            let before = self.ring_of.get(&he).copied();
            if now != before.is_some() {
                vertices.push(dcel.origin(he));
                vertices.push(dcel.dest(he));
                if let Some(r) = before {
                    invalid.push(r);
                }
                if now {
                    entering.push(he);
                }
            }
        }
        vertices.sort_unstable();
        vertices.dedup();
        for v in &vertices {
            for &he in dcel.outgoing(*v) {
                if let Some(&r) = self.ring_of.get(&(he ^ 1)) {
                    invalid.push(r);
                }
            }
        }
        invalid.sort_unstable();
        invalid.dedup();
        if invalid.is_empty() && entering.is_empty() {
            return 0;
        }

        // Drop invalid rings; their surviving half-edges seed the re-trace.
        let mut seeds = entering;
        let mut orphans: Vec<u32> = Vec::new();
        for &r in &invalid {
            self.changes.removed.push(self.key(r));
            let ring = self.rings[r as usize].take().expect("invalid ring is live");
            for &he in &ring.hes {
                self.ring_of.remove(&he);
            }
            if ring.is_outer() {
                self.outers.remove(r, ring.cover());
                if let Some(audit) = &mut self.audit {
                    audit.before.push(ring.points.clone());
                }
                self.polygons.remove(ring.cells());
                *self.ranks.get_mut() = None;
                orphans.extend(ring.children.iter().copied());
            }
            if ring.is_hole() {
                self.holes.remove(r, ring.cover());
                if let Some(p) = ring.parent {
                    self.unlink(r, &ring.points, p);
                }
            }
            seeds.extend(ring.hes);
        }
        let freed = invalid.clone();

        // Re-trace.
        let mut fresh: Vec<u32> = Vec::new();
        seeds.sort_unstable();
        seeds.dedup();
        for s in seeds {
            if dcel.alive(s) && !self.ring_of.contains_key(&s) && self.is_boundary(dcel, s) {
                let r = self.trace(dcel, s, prov);
                self.changes.added.push(self.key(r));
                fresh.push(r);
            }
        }

        // Hole ownership.
        for &r in &orphans {
            if let Some(ring) = self.rings[r as usize].as_mut() {
                ring.parent = None;
            }
        }
        // New and orphaned holes search every outer, new ones included, so
        // they land on the smallest. Then each new outer may take a KEPT
        // hole whose current owner is larger; new holes are already optimal.
        let mut searched = FastMap::<u32, ()>::default();
        for &r in fresh.iter().chain(&orphans) {
            if matches!(&self.rings[r as usize], Some(ring) if ring.is_hole()) {
                self.assign(r);
                searched.insert(r, ());
            }
        }
        for &r in &fresh {
            if self.ring(r).is_outer() {
                self.adopt(r, &searched);
            }
        }

        // A new outer's polygon is built once, with whatever holes it holds
        // now; later hole changes patch it in place.
        for &r in &fresh {
            let ring = self.ring(r);
            if ring.is_outer() {
                let polygon = self.polygon_of(ring);
                let key = ring.cells().to_vec();
                if let Some(audit) = &mut self.audit {
                    audit.after.push(polygon.outer.clone());
                }
                self.polygons.insert(key, polygon);
                *self.ranks.get_mut() = None;
            }
        }

        self.free.extend(freed);
        if let Some(audit) = &mut self.audit {
            for key in std::mem::take(&mut audit.patched) {
                if let Some(polygon) = self.polygons.get(&key) {
                    audit.after.push(polygon.outer.clone());
                }
            }
            // Polygons are inserted only at the end of an update, so every
            // patched or removed one existed before it: `before` holds
            // previous-state identities only.
            for list in [&mut audit.before, &mut audit.after] {
                list.sort_by(|a, b| compare_rings(a, b));
                list.dedup();
            }
        }
        fresh.len()
    }

    /// Starts or stops auditing (see [`PolygonAudit`]).
    pub(super) fn set_audit(&mut self, on: bool) {
        self.audit = on.then(PolygonAudit::default);
    }

    /// The last update's audit, when auditing.
    pub(super) fn audit(&self) -> Option<&PolygonAudit> {
        self.audit.as_ref()
    }

    fn trace(&mut self, dcel: &Dcel, start: He, prov: Option<&Provenance>) -> u32 {
        let id = match self.free.pop() {
            Some(id) => id,
            None => {
                self.rings.push(None);
                (self.rings.len() - 1) as u32
            }
        };
        let mut hes = Vec::new();
        let mut cells = Vec::new();
        let mut tags = Vec::new();
        let mut h = start;
        loop {
            let prev = self.ring_of.insert(h, id);
            assert!(
                prev.is_none(),
                "rings: walk from {start} reached half-edge {h}, already in ring {prev:?}"
            );
            hes.push(h);
            cells.push(dcel.origin(h));
            if let Some(prov) = prov {
                tags.push(prov.sources(dcel.origin(h), dcel.dest(h)));
            }
            h = self.next_boundary(dcel, h);
            if h == start {
                break;
            }
        }
        let area2 = double_area(&cells);
        let n = cells.len();
        if prov.is_none() {
            tags = vec![Vec::new(); n];
        }
        let canon = if area2 == 0 {
            None
        } else {
            canonical_cells(cells, tags, area2 > 0)
        };
        let bbox = canon
            .as_ref()
            .map(|c| cell_bounds(&c.cells))
            .unwrap_or([0, 0, 0, 0]);
        let points = canon
            .as_ref()
            .map(|c| to_points(&c.cells))
            .unwrap_or_default();
        self.next_serial += 1;
        let ring = Ring {
            serial: self.next_serial,
            hes,
            canon,
            area2,
            bbox,
            points,
            parent: None,
            children: Vec::new(),
        };
        if ring.is_outer() {
            self.outers.insert(id, ring.cover());
        }
        if ring.is_hole() {
            self.holes.insert(id, ring.cover());
        }
        self.rings[id as usize] = Some(ring);
        id
    }

    /// Gives hole `h` to the smallest outer enclosing it, if any.
    fn assign(&mut self, h: u32) {
        let hb = self.ring(h).bbox;
        let mut candidates = Vec::new();
        self.outers
            .query(self.ring(h).cover(), |o| candidates.push(o));
        candidates.sort_unstable();
        candidates.dedup();
        candidates.retain(|&o| contains(&self.ring(o).bbox, &hb));
        candidates.sort_by_key(|&o| self.ring(o).area2);
        for o in candidates {
            if ring_within(self.ring(h).cells(), self.ring(o).cells()) {
                self.link(h, o);
                return;
            }
        }
    }

    /// Lets the new outer `o` take every hole it encloses that has a larger
    /// owner or none.
    fn adopt(&mut self, o: u32, skip: &FastMap<u32, ()>) {
        let ob = self.ring(o).bbox;
        let area = self.ring(o).area2;
        let mut candidates = Vec::new();
        self.holes
            .query(self.ring(o).cover(), |h| candidates.push(h));
        candidates.sort_unstable();
        candidates.dedup();
        for h in candidates {
            if skip.contains_key(&h) {
                continue;
            }
            let hole = self.ring(h);
            if !contains(&ob, &hole.bbox) {
                continue;
            }
            let better = hole.parent.is_none_or(|p| area < self.ring(p).area2);
            if better && ring_within(hole.cells(), self.ring(o).cells()) {
                if let Some(p) = hole.parent {
                    let points = hole.points.clone();
                    self.unlink(h, &points, p);
                }
                self.link(h, o);
            }
        }
    }

    /// Makes `o` the owner of hole `h`, patching `o`'s output polygon in
    /// place if it already has one (a new outer gets its polygon built after
    /// assignment instead).
    fn link(&mut self, h: u32, o: u32) {
        self.rings[h as usize].as_mut().expect("hole live").parent = Some(o);
        let outer = self.rings[o as usize].as_mut().expect("outer live");
        outer.children.push(h);
        let key = outer
            .canon
            .as_ref()
            .expect("outer is canonical")
            .cells
            .clone();
        let points = self.ring(h).points.clone();
        if let Some(polygon) = self.polygons.get_mut(&key) {
            if let Some(audit) = &mut self.audit {
                audit.before.push(polygon.outer.clone());
                audit.patched.push(key.clone());
            }
            let at = polygon
                .holes
                .partition_point(|x| compare_rings(x, &points).is_lt());
            polygon.holes.insert(at, points);
        }
    }

    /// Detaches hole `h` (whose output points are `points`) from owner `p`.
    fn unlink(&mut self, h: u32, points: &[Point], p: u32) {
        let Some(outer) = self.rings[p as usize].as_mut() else {
            return;
        };
        outer.children.retain(|&c| c != h);
        let key = outer
            .canon
            .as_ref()
            .expect("outer is canonical")
            .cells
            .clone();
        if let Some(polygon) = self.polygons.get_mut(&key) {
            if let Some(audit) = &mut self.audit {
                audit.before.push(polygon.outer.clone());
                audit.patched.push(key.clone());
            }
            if let Ok(i) = polygon.holes.binary_search_by(|x| compare_rings(x, points)) {
                polygon.holes.remove(i);
            }
        }
    }

    /// One outer ring and its holes as an output polygon, holes in canonical
    /// order.
    fn polygon_of(&self, ring: &Ring) -> Polygon {
        let mut holes: Vec<Vec<Point>> = ring
            .children
            .iter()
            .map(|&h| self.ring(h).points.clone())
            .collect();
        holes.sort_by(|a, b| compare_rings(a, b));
        Polygon {
            outer: ring.points.clone(),
            holes,
        }
    }

    /// The canonical polygons of this side, in the full builder's order.
    /// Maintained per ring, so reading them costs nothing beyond iteration.
    pub(super) fn polygons(&self) -> impl ExactSizeIterator<Item = &Polygon> {
        self.polygons.values()
    }

    /// Where `p` (millimetres, as given — never snapped to a cell) lies
    /// against this side's output polygons, holes included. The answer is
    /// the one testing every output polygon with `classify_point` gives, but
    /// only rings whose bucket and closed bounds hold `p` are walked.
    ///
    /// A point inside an owned hole is outside that hole's polygon only; an
    /// island polygon inside the hole is tested on its own. Unowned holes
    /// (not output) and collapsed rings are ignored. The output polygons have
    /// disjoint interiors and no boundary inside another polygon, so the
    /// first boundary or interior found is the answer.
    pub(super) fn locate(&self, p: Point) -> Located {
        let (x, y) = (p.x * CELLS_PER_MM, p.y * CELLS_PER_MM);
        let at = Cover::Box([x, y, x, y]);
        let holds = |b: &[i64; 4]| {
            b[0] as f64 <= x && x <= b[2] as f64 && b[1] as f64 <= y && y <= b[3] as f64
        };
        // Outers whose owned hole strictly contains `p`: usually one or two,
        // but one per level of nesting for concentric loops, so it is sorted
        // once and searched — a linear `contains` per outer made a seed at
        // the centre of N nested loops cost N² (Codex GPT-6.1-Sol review).
        let mut excluded: Vec<u32> = Vec::new();
        let mut found = None;
        self.holes.query(at, |h| {
            if found.is_some() {
                return;
            }
            let ring = self.ring(h);
            let Some(parent) = ring.parent.filter(|_| ring.is_hole()) else {
                return;
            };
            if !holds(&ring.bbox) {
                return;
            }
            match classify_point(p, &ring.points) {
                Containment::Boundary => found = Some(Located::Boundary),
                Containment::Inside => excluded.push(parent),
                Containment::Outside => {}
            }
        });
        if let Some(found) = found {
            return found;
        }
        excluded.sort_unstable();
        self.outers.query(at, |r| {
            if found.is_some() {
                return;
            }
            let ring = self.ring(r);
            if !ring.is_outer() || !holds(&ring.bbox) {
                return;
            }
            match classify_point(p, &ring.points) {
                Containment::Boundary => found = Some(Located::Boundary),
                Containment::Inside if excluded.binary_search(&r).is_err() => {
                    found = Some(Located::Inside(self.key(r)))
                }
                _ => {}
            }
        });
        found.unwrap_or(Located::Outside)
    }

    /// Checks ring membership and closure against the DCEL, and every ring's
    /// tags against `prov` when this side carries them. Panics with a
    /// description on the first violation.
    pub(super) fn validate(&self, dcel: &Dcel, prov: Option<&Provenance>) {
        if let Some(prov) = prov {
            for (r, ring) in self.rings.iter().enumerate() {
                let Some(ring) = ring else { continue };
                let Some(canon) = &ring.canon else { continue };
                let cells: Vec<Cell> = ring.hes.iter().map(|&h| dcel.origin(h)).collect();
                let tags = ring
                    .hes
                    .iter()
                    .map(|&h| prov.sources(dcel.origin(h), dcel.dest(h)))
                    .collect();
                let fresh = canonical_cells(cells, tags, ring.area2 > 0).expect("still canonical");
                assert_eq!(
                    fresh.runs, canon.runs,
                    "rings ({:?}): ring {r} carries stale tags",
                    self.side
                );
            }
        }
        for he in dcel.half_edges() {
            let boundary = self.is_boundary(dcel, he);
            assert_eq!(
                boundary,
                self.ring_of.contains_key(&he),
                "rings ({:?}): half-edge {he} boundary status disagrees with ring membership",
                self.side
            );
        }
        let outers = self.rings.iter().flatten().filter(|r| r.is_outer()).count();
        assert_eq!(
            self.polygons.len(),
            outers,
            "rings ({:?}): output map is stale",
            self.side
        );
        for ring in self.rings.iter().flatten().filter(|r| r.is_outer()) {
            assert_eq!(
                self.polygons.get(ring.cells()),
                Some(&self.polygon_of(ring)),
                "rings ({:?}): output polygon is stale",
                self.side
            );
        }
        for (r, ring) in self.rings.iter().enumerate() {
            let Some(ring) = ring else { continue };
            for (i, &he) in ring.hes.iter().enumerate() {
                assert_eq!(self.ring_of.get(&he), Some(&(r as u32)));
                let next = ring.hes[(i + 1) % ring.hes.len()];
                assert_eq!(
                    self.next_boundary(dcel, he),
                    next,
                    "rings ({:?}): ring {r} is not a closed boundary walk",
                    self.side
                );
            }
        }
    }
}
