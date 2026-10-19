//! Shared snapping query checkpoint for issue 0058.
//!
//! Uses active-level endpoints, canonical occupied-boundary segments, infinite
//! axes, a bounded grid window, and global/relative angle rays. Queries are
//! read-only with respect to the document.
//!
//! # Exact over the supplied inputs (issue 0012, ED-0112)
//!
//! Every candidate carries a [`Witness`]: its squared distance and closest
//! point as exact rationals over the binary64 inputs — the projection onto
//! an edge, axis or ray is `(a·D + d·S)/D`, never a rounded construction;
//! lattice points are exact products; angle rays are unnormalized exact
//! 45° turns of the previous segment's STORED endpoints. Radius admission,
//! all R6.3.1 key levels and anchor coincidence are decided exactly (float
//! filters first, exact arithmetic only on near-ties). The reported `at` is
//! a binary64 point within the certified enclosure of the exact one.
//!
//! # Maintained, not rebuilt (issue 0012)
//!
//! A [`SnapIndex`] is the snapping view of ONE level and lives across edits.
//! [`SnapIndex::sync`] brings it to a new document revision with one
//! identity diff over the level's features (`O(level)`, cheap per entity;
//! issue 0098 replaces it with change sets) and geometry and index work
//! proportional to what changed:
//!
//! | Feature | Source | Kept current by |
//! |---|---|---|
//! | join nodes, free endpoints | document joins and walls | identity diff of `(entity, point, class)` |
//! | occupied-boundary edges | the level's [`LevelGeometry`] in provenance mode, owned here | the geometry's per-update ring changes |
//! | axes | wall start/end | identity diff; the view-clipped line cache is dropped on change |
//!
//! Feature boxes live in [`AabbTree`]s, so a pointer query visits only
//! subtrees overlapping its search box. Edge candidates carry a [`RingKey`]
//! rather than a canonical ordinal: ordinals shift when an unrelated earlier
//! component appears, so they are resolved for the few reported candidates at
//! query time ([`LevelGeometry::occupied_ordinals`]).
//!
//! [`SnapIndex::build`] is `sync` from empty; it is also the differential
//! oracle for the maintained path (see the tests).
//!
//! Document features are indexed once. Axis bounds are cached for the visible
//! rectangle expanded by the query radius and any outside captured pointer.
//! Panning/zooming can rebuild that line index; ordinary moves inside the same
//! view reuse it. Diagonal line bounds may still admit many candidates.
//!
//! Self-exclusion (R6.1.3) is decided by IDENTITY at the granularity the
//! arrangement actually resolved. Canonicalization deletes collinear vertices,
//! so one canonical edge can span several walls; the index keeps the
//! constant-owner sub-spans underneath it and excludes a span only when every
//! ring contributing it is excluded. One surviving independent ring keeps the
//! span, because that ring pins it in place. A join patch depends on ALL of
//! its members, so any moving member drops it whole — its boundary fragments
//! are inter-member hull edges and belong to no single member.
//!
//! Wall exclusion and join exclusion are separate inputs answering separate
//! questions; see [`SnapQuery::excluded_walls`] and [`SnapQuery::excluded_joins`].
//!
//! Full lifecycle and numerical conformance remain open. Indices are `!Sync`
//! because of the internal cache; serialize queries or give each worker its
//! own index.

use crate::aabb_tree::{AabbTree, KeyedTree, Proxy};
use crate::boolean::BoundaryRun;
use crate::hit::{point_features, EndpointKey};
use crate::incremental::{LevelGeometry, RingKey, SourceKey};
use crate::overlay::FastMap;
use crate::spatial::{Aabb, SpatialIndex};
use crate::walls::level_solids;
use crate::witness::{
    lattice_certainly_outside, lattice_window, Branch, Dir, Extent, LinePrep, Param, Radius, Shape,
    Witness,
};
use domowik_format::{Document, Point};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use uuid::Uuid;

/// R6.1.1's six candidate classes, declared in R6.3.1's inventory/precedence
/// order — the derived [`Ord`] IS that precedence, least first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapClass {
    JoinNode,
    FreeEndpoint,
    Edge,
    Axis,
    Grid,
    Angle,
}

/// The stable, class-specific canonical identity of a candidate — what a
/// caller compares FRAME TO FRAME to decide "the same feature won again"
/// (R6.4.1's guide-lifetime rule) without re-deriving any geometry or key
/// rule of its own. Combined with [`SnapCandidate::entity`] this is a full
/// R6.3.1 level-3/4 identity; [`SnapCandidate::at`] is the point a commit
/// would carry and MOVES between frames even while the feature itself does
/// not (a corner's `edge` candidate slides along the same segment).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SnapFeature {
    /// Identity plus stored point distinguishes a wall's two free endpoints.
    Entity { point: Point },
    /// `edge`: R3.2.4's canonical `(component ordinal, ring ordinal,
    /// vertex-run index)`, then the CONSTANT-OWNER SUB-SPAN index within that
    /// vertex run, then the `edge-start(0) < edge-closest(1) < edge-end(2)`
    /// discriminator.
    ///
    /// The sub-span index is what makes this key an identity rather than a
    /// location: canonicalization can merge several walls' faces into one
    /// vertex run, and two frames whose winner sits on DIFFERENT owners of
    /// that run are not the same feature even though the run is. Ordering is
    /// unaffected — R6.3.1 compares the closest point before any of this — so
    /// carrying it here settles identity once instead of leaving it to a
    /// separate rule (ED-0103 Part 2).
    Edge {
        component: usize,
        ring: usize,
        segment: usize,
        span: usize,
        discriminator: u8,
    },
    /// `axis`: whether the projection parameter fell within `[0, 1]`
    /// (R6.3.1: "on-segment < extension"); the wall is
    /// [`SnapCandidate::entity`].
    Axis { on_segment: bool },
    /// `grid`: the integer lattice indices.
    Grid { i: i64, j: i64 },
    /// Stored ray direction; degrees are a display label only.
    Angle { direction: (f64, f64), degrees: f64 },
}

/// One winning or losing candidate: a class, the entity it names (`None` for
/// `Grid` and `Angle`, which are local constructions rather than document
/// entities per R6.0.5, and for `Edge` — see the module docs), the plan point
/// a commit at this candidate would carry, and the stable [`SnapFeature`]
/// identity a caller tracks across frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapCandidate {
    pub class: SnapClass,
    pub entity: Option<Uuid>,
    pub at: Point,
    pub feature: SnapFeature,
    /// Native guide segment; point targets use `at` instead.
    pub guide: Option<(Point, Point)>,
}

/// An unsupported query is not an ordinary snap miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapError {
    InvalidInput,
    /// A lattice bound cannot fit i64, or enumeration exceeds the work limit.
    GridRange,
}

/// One query against a built [`SnapIndex`].
///
/// `radius_mm` is the R6.0.2 snap radius (T/z), already converted from
/// logical pixels through the caller's view transform — this module has no
/// view transform and must not invent one, matching [`crate::hit::hit_test`].
pub struct SnapQuery<'a> {
    /// The pointer's plan position.
    pub point: Point,
    /// The inclusive admission radius, in plan millimetres (R6.0.2/R6.0.3).
    pub radius_mm: f64,
    /// Finite, non-inverted visible plan rectangle. Axis bounds additionally
    /// include an outside captured pointer and the query radius.
    pub visible_rect_mm: Aabb,
    /// The gesture's fixed anchor (R6.1.3): any candidate whose point
    /// coincides EXACTLY with it is removed across every class, and it is
    /// the ray origin for `angle` candidates (R6.2.2, R6.2.3).
    pub anchor: Option<Point>,
    /// The chain's previous ACCEPTED segment direction as a raw `(dx, dy)`
    /// vector — NOT normalized, NOT converted to degrees, so its exact
    /// rational rotations (R6.2.1's relative family) can be computed without
    /// trigonometry. `None` when the chain has no previous accepted segment,
    /// in which case only the global family applies (R6.2.2).
    pub previous_direction: Option<(f64, f64)>,
    /// The previous accepted segment's stored endpoints `(start, end)`.
    /// Preferred over [`Self::previous_direction`] when present: the
    /// relative family is then exact over the stored coordinates rather
    /// than over a caller's rounded subtraction (ED-0112).
    pub previous_segment: Option<(Point, Point)>,
    /// The pick radius as logical pixels over the view scale `(T, z)`, `z`
    /// in logical pixels per millimetre. When present, admission is exactly
    /// `distance ≤ T/z` — the quotient is never rounded (R6.0.2, ED-0112) —
    /// and [`Self::radius_mm`] is ignored.
    pub screen_radius: Option<(f64, f64)>,
    /// Wall ids excluded by identity (R6.1.3): typically the dragged wall's
    /// own features. Removes those walls' `free-endpoint` and `axis`
    /// candidates, and removes the occupied-boundary SUB-SPANS that only
    /// excluded geometry generates — including join patches, which depend on
    /// every member wall at once and so are dropped by ANY moving member.
    pub excluded_walls: &'a [Uuid],
    /// Join ids whose NODE candidate is excluded by identity — and nothing
    /// else. It does not remove the join's patch from the boundary, because a
    /// node excluded for being the gesture's anchor sits on a patch that is
    /// not moving at all. Patch geometry is derived from `excluded_walls`
    /// instead, so a caller that omits a join id here cannot thereby be handed
    /// a candidate on a surface deforming under its own gesture.
    pub excluded_joins: &'a [Uuid],
    /// Active minor grid spacing in millimetres (section 1.5), or `None` when
    /// unknown/inapplicable.
    pub grid_spacing_mm: Option<f64>,
    /// The R6.4.3 `grid` local preference toggle.
    pub grid_enabled: bool,
    /// The R6.4.3 `object` local preference toggle — governs `join-node`,
    /// `free-endpoint`, `edge`, and `axis` together, the document-derived
    /// classes of R6.0.5.
    pub object_enabled: bool,
    /// The R6.4.3 `angle` local preference toggle.
    pub angle_enabled: bool,
    /// R6.4.2: Alt sampled HELD at gesture start. When true every class is
    /// disabled for the whole gesture regardless of the toggles above.
    pub alt_disabled: bool,
}

/// The R6.2.1 global family: the eight EXACT integer direction vectors for
/// absolute directions at 45° multiples, in `[-180, 180)` order with the
/// `+180`/`-180` pair deduplicated to `-180`. Exact because a 45°-multiple is
/// precisely the direction whose vector is a rational pair (R6.2.1's
/// amendment note); no other multiple of 15° has that property.
const GLOBAL_DIRECTIONS: [(f64, f64); 8] = [
    (-1.0, 0.0),
    (-1.0, -1.0),
    (0.0, -1.0),
    (1.0, -1.0),
    (1.0, 0.0),
    (1.0, 1.0),
    (0.0, 1.0),
    (-1.0, 1.0),
];

/// The most lattice points [`SnapIndex::query`] will ever enumerate for one
/// `grid` request.
///
/// Section 1.5 chooses spacing `s` so that `z * s >= 8` logical px; R6.0.2's
/// largest radius is the touch value `T = 24` px, giving plan radius `T/z`.
/// So for any NORMATIVE combination `radius_mm / grid_spacing_mm <=
/// (T/z) / (8/z) = T/8 = 3`, and the enumerated window is at most `8x8 = 64`
/// points. `4096` (a `64x64` window) is a generous multiple of that bound,
/// not a tuning constant: a caller supplying a spacing/radius pair outside
/// section 1.5's derivation returns `GridRange` rather than allocating an
/// unbounded candidate list.
const MAX_GRID_CANDIDATES: i64 = 4096;

/// Normalizes a direction into the half-open range `[-180, 180)` (R6.2.1).
fn normalize_angle_deg(deg: f64) -> f64 {
    (deg + 180.0).rem_euclid(360.0) - 180.0
}

/// R6.2's targets: the global family, then the previous segment turned by
/// 0°, 45°, 90° and 135° (unnormalized, so exact), each relative ray dropped
/// when it is EXACTLY a global one. Degrees are display labels only;
/// ordering and equality use [`Dir::cmp_angle`] and [`Dir::same_ray`].
fn angle_targets(previous: Option<(Point, Point)>) -> Vec<(Dir, f64)> {
    let label = |dir: &Dir| {
        let (dx, dy) = dir.approx();
        normalize_angle_deg(dy.atan2(dx).to_degrees())
    };
    let mut targets: Vec<(Dir, f64)> = GLOBAL_DIRECTIONS
        .iter()
        .map(|&(x, y)| {
            let dir = Dir::constant(x as i8, y as i8);
            (dir, label(&dir))
        })
        .collect();
    if let Some((from, to)) = previous {
        let globals = targets.len();
        for eighths in 0..4 {
            let dir = Dir::turned(from, to, eighths);
            if dir.is_zero() || targets[..globals].iter().any(|(g, _)| g.same_ray(&dir)) {
                continue;
            }
            targets.push((dir, label(&dir)));
        }
    }
    targets
}

/// A candidate carrying its full ranking data, before being narrowed to the
/// public [`SnapCandidate`].
#[derive(Debug)]
struct Ranked {
    class: SnapClass,
    entity: Option<Uuid>,
    feature: SnapFeature,
    guide: Option<(Point, Point)>,
    /// Index of its exact distance and closest point (issue 0012) in the
    /// query's witness table. Shared by the members of a collinear axis
    /// group, so their exact state is built once per line, not per wall.
    witness: usize,
    /// The exact ray of an `angle` candidate.
    ray: Option<Dir>,
}

/// R6.3.1's level-3 canonical geometry key, evaluated only once level 1
/// (class) and level 2 (exact squared distance) have already tied — so both
/// sides of every match arm below are already the SAME [`SnapFeature`]
/// variant. Closest points compare EXACTLY, through their witnesses.
fn geometry_cmp(a: &Ranked, b: &Ranked, ws: &[Witness]) -> Ordering {
    let point_order = || ws[a.witness].cmp_xy(&ws[b.witness]);
    match (a.feature, b.feature) {
        (SnapFeature::Entity { .. }, SnapFeature::Entity { .. }) => point_order(),
        (
            SnapFeature::Edge {
                component: ca,
                ring: ra,
                segment: sa,
                span: pa,
                discriminator: da,
            },
            SnapFeature::Edge {
                component: cb,
                ring: rb,
                segment: sb,
                span: pb,
                discriminator: db,
            },
        ) => point_order().then_with(|| (ca, ra, sa, pa, da).cmp(&(cb, rb, sb, pb, db))),
        (SnapFeature::Axis { on_segment: oa }, SnapFeature::Axis { on_segment: ob }) => {
            point_order().then_with(|| (!oa).cmp(&!ob))
        }
        (SnapFeature::Grid { i: ia, j: ja }, SnapFeature::Grid { i: ib, j: jb }) => {
            (ia, ja).cmp(&(ib, jb))
        }
        (SnapFeature::Angle { .. }, SnapFeature::Angle { .. }) => match (&a.ray, &b.ray) {
            (Some(x), Some(y)) => x.cmp_angle(y),
            _ => Ordering::Equal,
        },
        _ => unreachable!("class equality (level 1) already implies the same feature variant"),
    }
}

/// Class, EXACT squared distance, geometry key, then identity. Each witness
/// materializes its exact values at most once, however many comparisons the
/// sort makes.
fn rank(a: &Ranked, b: &Ranked, ws: &[Witness]) -> Ordering {
    a.class
        .cmp(&b.class)
        .then_with(|| ws[a.witness].cmp_dist(&ws[b.witness]))
        .then_with(|| geometry_cmp(a, b, ws))
        .then_with(|| a.entity.cmp(&b.entity))
}

/// One end of a floored/ceiled grid index range, checked against the `i64`
/// domain before the `as i64` cast: the cast saturates silently for anything
/// outside `[i64::MIN, i64::MAX]`. `2^63` itself IS exactly representable in
/// `f64`; it is `i64::MAX` (`2^63 - 1`) that is NOT, so `i64::MAX as f64`
/// rounds UP to `2^63` — one past the largest legal `i64`. An inclusive upper
/// comparison against that rounded value would therefore wrongly admit `2^63`,
/// so the upper bound here is checked exclusive.
fn grid_index_bound(rounded: f64) -> Result<i64, SnapError> {
    const LOWER: f64 = -9_223_372_036_854_775_808.0; // -2^63, exactly i64::MIN
    const UPPER: f64 = 9_223_372_036_854_775_808.0; // 2^63, one past i64::MAX
    if !rounded.is_finite() || !(LOWER..UPPER).contains(&rounded) {
        return Err(SnapError::GridRange);
    }
    Ok(rounded as i64)
}

/// A conservative broad-phase box of the line through `start → end` within
/// `box_`, or `None` only for a CERTAIN miss.
///
/// The rounded clip runs against `box_` widened by a relative slack (`16·ε`
/// of the largest magnitude involved plus the smallest normal, which
/// dominates the few roundings of a well-conditioned clip), and its result is
/// widened again. When the clip itself is inconclusive — a parameter
/// overflowed because the direction is tiny or huge relative to the box
/// (Codex GPT-6.1-Sol review: a subnormal-length axis) — the whole box is
/// returned: the line may cross it anywhere, and exact admission decides.
fn axis_box(start: Point, end: Point, box_: Aabb) -> Option<Aabb> {
    let m = [
        start.x, start.y, end.x, end.y, box_.min_x, box_.min_y, box_.max_x, box_.max_y,
    ]
    .iter()
    .fold(0.0f64, |m, v| m.max(v.abs()));
    let slack = 16.0 * f64::EPSILON * m + f64::MIN_POSITIVE;
    let wide = box_.expanded(slack);
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let params = [
        (wide.min_x - start.x) / dx,
        (wide.max_x - start.x) / dx,
        (wide.min_y - start.y) / dy,
        (wide.max_y - start.y) / dy,
    ];
    let conclusive = |d: f64, a: f64, b: f64| d == 0.0 || (a.is_finite() && b.is_finite());
    if !(conclusive(dx, params[0], params[1]) && conclusive(dy, params[2], params[3])) {
        return Some(wide);
    }
    let (a, b) = clip_line_to_box(start, end, wide)?;
    Aabb::of(&[a, b]).map(|b| b.expanded(slack))
}

/// One occupied-boundary segment: a consecutive vertex pair of one ring of
/// one canonical `occupied_region` component (R3.2.4), plus the constant-owner
/// sub-spans that make up its length.
///
/// Canonical geometry is untouched: `a`, `b` and the R6.3.1 key are exactly
/// what the untagged region produces. `run` is additive provenance, and is the
/// only thing that can tell one wall's half of a merged collinear face from
/// its neighbour's.
#[derive(Debug, Clone)]
struct EdgeSegment {
    a: Point,
    b: Point,
    /// `a → b`'s direction enclosures, prepared once.
    prep: LinePrep,
    /// Each interior break's parameter along `a → b`, prepared once; they
    /// increase along the edge, so a span is found by binary search.
    break_params: Vec<Param>,
    /// The boundary ring this is an edge of; its canonical `(component,
    /// ring)` ordinals are resolved at query time.
    ring: RingKey,
    segment: usize,
    /// Sub-spans and their contributing source tags, named by
    /// [`LevelGeometry::source_key`].
    run: BoundaryRun,
}

/// Whether a source's geometry moves with any of `walls` — what it DEPENDS
/// on, which is not the same as what it is named after.
///
/// A wall rectangle depends on exactly its own wall. A join patch is the
/// convex hull of every member's cross-section (R3.2.3), so moving ANY member
/// reshapes the whole hull: its surviving boundary edges are the inter-member
/// hull edges, whose endpoints generally come from OTHER members than the one
/// being dragged. There is therefore no per-member split of a patch, and the
/// only sound rule is that a patch depends on all of its members at once.
fn depends_on_any(key: &SourceKey, walls: &[Uuid]) -> bool {
    match key {
        SourceKey::Wall(id) | SourceKey::WallPiece { wall: id, .. } => walls.contains(id),
        SourceKey::Join { members, .. } => members.iter().any(|(id, _)| walls.contains(id)),
    }
}

/// The view-clipped axis index: its bounds, the lines it was built over
/// (wall, start, end), and a grid over their clipped extents.
type AxisCache = (Aabb, SpatialIndex<usize>);

/// Walls whose axes are the SAME infinite line (exactly: both of a member's
/// endpoints are collinear with the representative's). They share one exact
/// projection and distance per query, so one witness serves them all; only
/// each member's `on-segment` flag differs, decided by comparing the
/// pointer's projection parameter with the member's endpoint parameters
/// along the shared line. Long facades of collinear segments are the
/// ordinary case this serves.
#[derive(Debug, Clone)]
struct AxisGroup {
    key: (u64, u64, u64),
    /// Members leaving in the current sync, as `(id, start, end)`: dropped
    /// together, with one refresh, in [`AxisGroups::finish`].
    leaving: Vec<(Uuid, Point, Point)>,
    /// The representative — the member with the least wall id, so the
    /// witness origin (and so the last bits of a reported point) is a
    /// function of the document, not of edit history.
    rep: Uuid,
    origin: Point,
    end: Point,
    prep: LinePrep,
    members: Vec<(Uuid, Point, Point, Param, Param)>,
}

impl AxisGroup {
    /// Re-picks the representative and recomputes every member's
    /// parameters against it.
    fn refresh(&mut self) {
        let &(rep, origin, end, _, _) = self
            .members
            .iter()
            .min_by_key(|m| m.0)
            .expect("a live group has a member");
        (self.rep, self.origin, self.end) = (rep, origin, end);
        self.prep = LinePrep::of(&Dir::between(origin, end));
        for m in &mut self.members {
            m.3 = Param::of(origin, &self.prep, m.1);
            m.4 = Param::of(origin, &self.prep, m.2);
        }
    }
}

/// The axis groups of a level, maintained wall by wall at sync time, so a
/// view change rebuilds an index over GROUPS (one per distinct line) and an
/// edit regroups only the walls it moved.
#[derive(Debug, Clone, Default)]
struct AxisGroups {
    groups: Vec<Option<AxisGroup>>,
    free: Vec<usize>,
    /// A cheap float key of the line proposes groups; exact orientation
    /// confirms membership, so a rounding-induced key split only costs
    /// sharing, never correctness.
    by_key: FastMap<(u64, u64, u64), Vec<usize>>,
    group_of: FastMap<Uuid, usize>,
    /// Groups touched since the last [`Self::finish`].
    dirty: Vec<usize>,
    /// Group refreshes performed, for tests and diagnostics.
    refreshes: u64,
}

impl AxisGroups {
    /// Direction scaled by its larger component with a canonical sign, and
    /// the line's offset along the normal.
    fn key(a: Point, b: Point) -> Option<(u64, u64, u64)> {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let m = dx.abs().max(dy.abs());
        if !(m > 0.0 && m.is_finite()) {
            return None;
        }
        let (mut ux, mut uy) = (dx / m, dy / m);
        if ux < 0.0 || (ux == 0.0 && uy < 0.0) {
            (ux, uy) = (-ux, -uy);
        }
        let c = uy * a.x - ux * a.y;
        let q = |v: f64| ((v * 1e9).round() as i64) as u64;
        Some((q(ux), q(uy), ((c * 1e3).round() as i64) as u64))
    }

    fn insert(&mut self, id: Uuid, a: Point, b: Point) {
        // A degenerate or non-finite axis gets a group of its own.
        let key = Self::key(a, b).unwrap_or((u64::MAX, id.as_u64_pair().0, id.as_u64_pair().1));
        let found = self.by_key.get(&key).and_then(|slot| {
            slot.iter().copied().find(|&g| {
                let group = self.groups[g].as_ref().expect("keyed group is live");
                crate::orientation(group.origin, group.end, a) == crate::Orientation::Collinear
                    && crate::orientation(group.origin, group.end, b)
                        == crate::Orientation::Collinear
            })
        });
        let g = match found {
            Some(g) => {
                let group = self.groups[g].as_mut().expect("live");
                let (pa, pb) = (
                    Param::of(group.origin, &group.prep, a),
                    Param::of(group.origin, &group.prep, b),
                );
                group.members.push((id, a, b, pa, pb));
                self.dirty.push(g);
                g
            }
            None => {
                let prep = LinePrep::of(&Dir::between(a, b));
                let group = AxisGroup {
                    key,
                    leaving: Vec::new(),
                    rep: id,
                    origin: a,
                    end: b,
                    members: vec![(id, a, b, Param::of(a, &prep, a), Param::of(a, &prep, b))],
                    prep,
                };
                let g = match self.free.pop() {
                    Some(g) => {
                        self.groups[g] = Some(group);
                        g
                    }
                    None => {
                        self.groups.push(Some(group));
                        self.groups.len() - 1
                    }
                };
                self.by_key.entry(key).or_default().push(g);
                g
            }
        };
        self.group_of.insert(id, g);
    }

    /// Marks wall `id`, last synced as `start → end`, as leaving its group.
    /// Takes effect in [`Self::finish`].
    fn remove(&mut self, id: Uuid, start: Point, end: Point) {
        let g = self
            .group_of
            .remove(&id)
            .expect("axis groups: removing an unknown wall");
        let group = self.groups[g].as_mut().expect("live");
        group.leaving.push((id, start, end));
        self.dirty.push(g);
    }

    /// Applies the sync's pending changes: each touched group drops its
    /// leaving members in ONE pass and refreshes its representative ONCE —
    /// `O(size of the touched groups)` whatever order ids arrive in (Codex
    /// GPT-6.1-Sol review: refreshing per insertion was quadratic for a long
    /// collinear chain with descending ids).
    fn finish(&mut self) {
        let bits = |id: Uuid, a: Point, b: Point| {
            (
                id,
                [a.x.to_bits(), a.y.to_bits(), b.x.to_bits(), b.y.to_bits()],
            )
        };
        let mut dirty = std::mem::take(&mut self.dirty);
        dirty.sort_unstable();
        dirty.dedup();
        for g in dirty {
            let Some(group) = self.groups[g].as_mut() else {
                continue;
            };
            if !group.leaving.is_empty() {
                let leaving: FastMap<(Uuid, [u64; 4]), ()> = group
                    .leaving
                    .drain(..)
                    .map(|(id, a, b)| (bits(id, a, b), ()))
                    .collect();
                group
                    .members
                    .retain(|m| !leaving.contains_key(&bits(m.0, m.1, m.2)));
            }
            if group.members.is_empty() {
                let key = group.key;
                self.groups[g] = None;
                self.free.push(g);
                if let Some(slot) = self.by_key.get_mut(&key) {
                    slot.retain(|&x| x != g);
                    if slot.is_empty() {
                        self.by_key.remove(&key);
                    }
                }
            } else {
                group.refresh();
                self.refreshes += 1;
            }
        }
    }

    fn live(&self) -> impl Iterator<Item = (usize, &AxisGroup)> {
        self.groups
            .iter()
            .enumerate()
            .filter_map(|(g, x)| x.as_ref().map(|x| (g, x)))
    }
}

/// Work done by the last [`SnapIndex::sync`], for tests and diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapSyncStats {
    pub endpoints_changed: usize,
    pub edges_removed: usize,
    pub edges_added: usize,
    pub axes_changed: usize,
}

/// Clips the infinite line through `start`/`end` to `box_`, returning the two
/// endpoints of the clipped segment — `None` when the line never meets the
/// box at all (a wall whose own coordinates are already out of the document
/// bound, which validation flags separately).
fn clip_line_to_box(start: Point, end: Point, box_: Aabb) -> Option<(Point, Point)> {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let (mut t_lo, mut t_hi) = (f64::NEG_INFINITY, f64::INFINITY);
    if dx != 0.0 {
        let (mut t0, mut t1) = ((box_.min_x - start.x) / dx, (box_.max_x - start.x) / dx);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        t_lo = t_lo.max(t0);
        t_hi = t_hi.min(t1);
    } else if start.x < box_.min_x || start.x > box_.max_x {
        return None;
    }
    if dy != 0.0 {
        let (mut t0, mut t1) = ((box_.min_y - start.y) / dy, (box_.max_y - start.y) / dy);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        t_lo = t_lo.max(t0);
        t_hi = t_hi.min(t1);
    } else if start.y < box_.min_y || start.y > box_.max_y {
        return None;
    }
    if !t_lo.is_finite() || !t_hi.is_finite() || t_lo > t_hi {
        return None;
    }
    let at = |t: f64| Point::new(start.x + dx * t, start.y + dy * t);
    Some((at(t_lo), at(t_hi)))
}

/// A reusable snapping index for one level: the committed-update cost of
/// BEHAVIOR-SPEC section 6, paid once per document change so
/// [`SnapIndex::query`] can be called on every pointer move against prebuilt
/// [`SpatialIndex`] broad-phases instead of rescanning the document.
#[derive(Debug, Clone)]
pub struct SnapIndex {
    level: Uuid,
    /// The level's occupied region with provenance. Owned: its per-update
    /// ring changes must be consumed after EVERY update, which only the
    /// owner can guarantee.
    geometry: LevelGeometry,
    endpoints: KeyedTree<EndpointKey, Point, (Uuid, SnapClass, Point)>,
    edges: AabbTree<EdgeSegment>,
    edges_of: HashMap<RingKey, Vec<Proxy>>,
    /// Each wall's UNCLAMPED axis endpoints, for the actual projection, with
    /// the epoch of the sync that last saw the wall. Order is not semantic:
    /// candidates are ranked by a total key, and each exact distance is
    /// evaluated at most once per query whatever the order
    /// ([`SquaredDistance`]).
    axis_lines: FastMap<Uuid, (Point, Point, u64)>,
    epoch: u64,
    /// `axis_lines` grouped by infinite line, kept in step with it.
    axis_groups: AxisGroups,
    /// View-specific line bounds over `axis_lines` (as a vector, for stable
    /// indices). Interior caching makes this index `!Sync`; serialize
    /// queries or give each worker its own index.
    axis_cache: RefCell<Option<AxisCache>>,
    last: SnapSyncStats,
}

impl SnapIndex {
    /// Builds the index for `level_id` from scratch: [`Self::sync`] from an
    /// empty level. Only active-level walls and joins are included (R6.0.5:
    /// non-active levels are never snappable).
    pub fn build(doc: &Document, level_id: Uuid) -> Self {
        let mut index = Self::new(level_id);
        index.sync(doc);
        index
    }

    /// An empty index for `level_id`.
    pub fn new(level_id: Uuid) -> Self {
        Self {
            level: level_id,
            geometry: LevelGeometry::with_provenance(),
            endpoints: KeyedTree::default(),
            edges: AabbTree::new(),
            edges_of: HashMap::new(),
            axis_lines: FastMap::default(),
            epoch: 0,
            axis_groups: AxisGroups::default(),
            axis_cache: RefCell::new(None),
            last: SnapSyncStats::default(),
        }
    }

    /// The level this index covers.
    pub fn level(&self) -> Uuid {
        self.level
    }

    /// The level's occupied region and rooms, maintained by the same syncs.
    /// Its [`LevelGeometry::plan`] equals `walls::plan` of the synced
    /// document's level.
    pub fn geometry(&self) -> &LevelGeometry {
        &self.geometry
    }

    /// Work done by the last [`Self::sync`].
    pub fn last_sync(&self) -> &SnapSyncStats {
        &self.last
    }

    /// Brings the index to `doc`. Afterwards every query answers exactly
    /// as [`Self::build`] of `doc` would.
    ///
    /// Cost: one identity diff over the level's walls and joins (no geometry
    /// for unchanged ones), plus geometry and index work proportional to
    /// what changed.
    pub fn sync(&mut self, doc: &Document) -> &SnapSyncStats {
        let mut stats = SnapSyncStats::default();
        self.sync_endpoints(doc, &mut stats);

        let solids = level_solids(doc, self.level);
        self.geometry.sync(&solids);
        self.sync_edges(&mut stats);

        self.epoch += 1;
        let epoch = self.epoch;
        for (wall, &id) in solids.walls.iter().zip(&solids.ids) {
            let line = (wall.start, wall.end, epoch);
            match self.axis_lines.get_mut(&id) {
                Some(old) if (old.0, old.1) == (wall.start, wall.end) => old.2 = epoch,
                old => {
                    if let Some(&mut (a, b, _)) = old {
                        self.axis_groups.remove(id, a, b);
                    }
                    self.axis_lines.insert(id, line);
                    self.axis_groups.insert(id, wall.start, wall.end);
                    stats.axes_changed += 1;
                }
            }
        }
        if self.axis_lines.len() != solids.ids.len() {
            let groups = &mut self.axis_groups;
            self.axis_lines.retain(|&id, l| {
                let keep = l.2 == epoch;
                if !keep {
                    groups.remove(id, l.0, l.1);
                    stats.axes_changed += 1;
                }
                keep
            });
        }
        self.axis_groups.finish();
        if stats.axes_changed > 0 {
            *self.axis_cache.borrow_mut() = None;
        }
        self.last = stats;
        &self.last
    }

    /// Checks that the indexes hold exactly the current features: every
    /// indexed ring is live with one leaf per canonical edge, no leaf is
    /// orphaned, both trees are structurally sound, and the geometry's own
    /// invariants hold. Panics naming the first violation. Proportional to
    /// the level — for tests and debugging.
    pub fn validate(&self) {
        self.geometry.validate();
        self.endpoints.validate();
        self.edges.validate();
        let mut leaves = 0;
        for (key, proxies) in &self.edges_of {
            let ring = self
                .geometry
                .occupied_ring(*key)
                .unwrap_or_else(|| panic!("snap: edges indexed for dead ring {key:?}"));
            assert_eq!(
                proxies.len(),
                ring.points.len(),
                "snap: ring {key:?} has a stale edge count"
            );
            for &proxy in proxies {
                assert_eq!(self.edges.get(proxy).ring, *key, "snap: leaf in wrong ring");
            }
            leaves += proxies.len();
        }
        assert_eq!(leaves, self.edges.len(), "snap: orphaned edge leaves");
        assert!(
            self.axis_groups.dirty.is_empty(),
            "snap: axis groups left dirty after a sync"
        );
        let mut grouped = 0;
        for (g, group) in self.axis_groups.live() {
            assert!(
                group.leaving.is_empty(),
                "snap: axis group {g} has unapplied removals"
            );
            assert!(!group.members.is_empty(), "snap: empty axis group {g}");
            let least = group.members.iter().map(|m| m.0).min().expect("non-empty");
            assert_eq!(
                group.rep, least,
                "snap: axis group {g} has a stale representative"
            );
            for &(id, a, b, _, _) in &group.members {
                assert_eq!(
                    self.axis_lines.get(&id).map(|l| (l.0, l.1)),
                    Some((a, b)),
                    "snap: axis {id} stale in its group"
                );
                assert_eq!(
                    self.axis_groups.group_of.get(&id),
                    Some(&g),
                    "snap: axis {id} group link"
                );
                for q in [a, b] {
                    assert_eq!(
                        crate::orientation(group.origin, group.end, q),
                        crate::Orientation::Collinear,
                        "snap: axis {id} is not on its group's line"
                    );
                }
                grouped += 1;
            }
        }
        assert_eq!(
            grouped,
            self.axis_lines.len(),
            "snap: axes missing from groups"
        );
    }

    fn sync_endpoints(&mut self, doc: &Document, stats: &mut SnapSyncStats) {
        stats.endpoints_changed =
            self.endpoints
                .sync(point_features(doc, self.level), |&key, &at| {
                    let class = match key {
                        EndpointKey::Join(_) => SnapClass::JoinNode,
                        EndpointKey::Wall(..) => SnapClass::FreeEndpoint,
                    };
                    Some((Aabb::point(at), (key.entity(), class, at)))
                });
    }

    /// Applies the geometry's last ring changes to the edge tree.
    fn sync_edges(&mut self, stats: &mut SnapSyncStats) {
        let changes = self.geometry.occupied_changes();
        for key in changes
            .removed
            .iter()
            .chain(changes.retagged)
            .chain(changes.added)
        {
            for proxy in self.edges_of.remove(key).unwrap_or_default() {
                self.edges.remove(proxy);
                stats.edges_removed += 1;
            }
        }
        // The first sync bulk-builds the tree (median split); later ones
        // insert the few changed rings' edges one by one.
        let bulk = self.edges.is_empty();
        let mut fresh: Vec<(Aabb, EdgeSegment)> = Vec::new();
        for &key in changes.added.iter().chain(changes.retagged) {
            let Some(ring) = self.geometry.occupied_ring(key) else {
                continue;
            };
            let points = ring.points;
            let n = points.len();
            for (segment, run) in ring.runs().enumerate() {
                let (a, b) = (points[segment], points[(segment + 1) % n]);
                if let Some(bounds) = Aabb::of(&[a, b]) {
                    let prep = LinePrep::of(&Dir::between(a, b));
                    let break_params = run.breaks.iter().map(|&q| Param::of(a, &prep, q)).collect();
                    let edge = EdgeSegment {
                        a,
                        b,
                        prep,
                        break_params,
                        ring: key,
                        segment,
                        run,
                    };
                    fresh.push((bounds, edge));
                }
            }
        }
        stats.edges_added = fresh.len();
        let rings: Vec<RingKey> = fresh.iter().map(|(_, e)| e.ring).collect();
        let proxies: Vec<Proxy> = if bulk {
            self.edges.extend_empty(fresh)
        } else {
            fresh
                .into_iter()
                .map(|(b, e)| self.edges.insert(b, e))
                .collect()
        };
        for (key, proxy) in rings.into_iter().zip(proxies) {
            self.edges_of.entry(key).or_default().push(proxy);
        }
    }

    /// Whether a contributing ring's GEOMETRY is excluded by identity
    /// (R6.1.3), never by distance.
    ///
    /// Derived inside the core from [`SnapQuery::excluded_walls`] alone, and
    /// deliberately NOT from [`SnapQuery::excluded_joins`]. The two are
    /// different questions and neither implies the other:
    ///
    /// - dragging a wall's FAR end reshapes its cross-section at the near
    ///   join, so that patch deforms even though the near node does not move.
    ///   A caller that excluded only the node would be handed candidates on a
    ///   surface moving under its own gesture, so this cannot wait to be told;
    /// - excluding a node because it is the gesture's ANCHOR says nothing
    ///   about the patch, which in that case is static and remains a legal
    ///   target. Folding the node exclusion in here would delete it.
    fn ring_excluded(&self, tag: usize, req: &SnapQuery) -> bool {
        self.geometry
            .source_key(tag)
            .is_some_and(|key| depends_on_any(key, req.excluded_walls))
    }

    /// The witness of `seg` itself (origin `a`, direction `b − a`), for
    /// parameter comparisons along it.
    fn edge_line(seg: &EdgeSegment, p: Point, extent: Extent) -> Witness {
        Witness::prepared(
            p,
            Shape::Linear {
                origin: seg.a,
                dir: Dir::between(seg.a, seg.b),
                extent,
            },
            Some(&seg.prep),
        )
    }

    /// Which sub-span of `seg` contains the closest point of `w` (a witness
    /// on `seg`'s own line), by EXACT position along the edge: a point on an
    /// interior break belongs to the span that ENDS there, so the answer is
    /// total and the two spans meeting at a break never both claim it.
    fn span_of(seg: &EdgeSegment, w: &Witness) -> usize {
        // The first break the projection does not pass: breaks are strictly
        // increasing along the edge, so "projection ≤ break" is monotone.
        seg.break_params
            .partition_point(|b| w.cmp_param(b) == Ordering::Greater)
    }

    /// The closest point of the part of `seg` this gesture may still snap
    /// to, as a witness with its sub-span index — `None` when every sub-span
    /// is excluded.
    ///
    /// A canonical edge can span several walls once collinear vertices are
    /// gone, so excluding it whole would delete a static neighbour's face and
    /// keeping it whole would offer the dragged wall its own. Neither is
    /// R6.1.3. The allowed set is the retained sub-spans, and the candidate is
    /// the closest point OF THAT SET — so a retained neighbour span still
    /// contributes its own nearest point instead of vanishing behind an
    /// excluded one. Spans are compared exactly: distance, then `(x, y)`.
    fn allowed_closest_point(
        &self,
        seg: &EdgeSegment,
        point: Point,
        req: &SnapQuery,
    ) -> Option<(Witness, usize)> {
        let excluded: Vec<bool> = seg
            .run
            .sources
            .iter()
            .map(|span| {
                // Retain the span when ANY contributing ring survives: one
                // independent non-excluded ring is enough to pin it in place.
                !span.is_empty() && span.iter().all(|tag| self.ring_excluded(*tag, req))
            })
            .collect();
        if !excluded.iter().any(|e| *e) {
            let w = Self::edge_line(seg, point, Extent::Segment { end: seg.b });
            let span = Self::span_of(seg, &w);
            return Some((w, span));
        }
        let mut best: Option<(Witness, usize)> = None;
        let mut from = seg.a;
        for (i, drop) in excluded.iter().enumerate() {
            let to = seg.run.breaks.get(i).copied().unwrap_or(seg.b);
            if !drop {
                let w = Witness::new(
                    point,
                    Shape::Linear {
                        origin: from,
                        dir: Dir::between(from, to),
                        extent: Extent::Segment { end: to },
                    },
                );
                // Equidistant retained spans break by R6.3.1's own level-3
                // key, the smaller exact `(x, y)`, never by span order (which
                // would depend on the ring's traversal direction).
                let better = best
                    .as_ref()
                    .is_none_or(|(b, _)| w.cmp_dist(b).then_with(|| w.cmp_xy(b)) == Ordering::Less);
                if better {
                    best = Some((w, i));
                }
            }
            from = to;
        }
        best
    }

    /// Ranked candidates, or an explicit unsupported/invalid-input error.
    ///
    /// Radius admission is inclusive and EXACT, as are all three ranking
    /// levels (class, squared distance, geometry key): every candidate is
    /// compared through its [`Witness`] — the exact projection onto the
    /// edge, axis or ray, never a rounded one. The reported `at` is a
    /// binary64 point within the witness's certified enclosure (ED-0112).
    /// Alt disable returns an empty result. Invalid inputs and oversized grid
    /// ranges are errors, not misses.
    pub fn query(&self, req: &SnapQuery) -> Result<Vec<SnapCandidate>, SnapError> {
        if req.alt_disabled {
            return Ok(Vec::new());
        }
        let point = req.point;
        let radius = match req.screen_radius {
            Some((px, zoom)) => {
                if !px.is_finite() || px < 0.0 || !zoom.is_finite() || zoom <= 0.0 {
                    return Err(SnapError::InvalidInput);
                }
                Radius::screen(px, zoom)
            }
            None => {
                if !req.radius_mm.is_finite() || req.radius_mm < 0.0 {
                    return Err(SnapError::InvalidInput);
                }
                Radius::mm(req.radius_mm)
            }
        };
        let radius_mm = radius.search_mm();
        if !point.x.is_finite() || !point.y.is_finite() || !radius_mm.is_finite() {
            return Err(SnapError::InvalidInput);
        }
        let view = req.visible_rect_mm;
        let bounds = view.union(Aabb::point(point)).expanded(radius_mm);
        let finite = |p: Point| p.x.is_finite() && p.y.is_finite();
        if [
            view.min_x,
            view.min_y,
            view.max_x,
            view.max_y,
            bounds.min_x,
            bounds.min_y,
            bounds.max_x,
            bounds.max_y,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || view.min_x > view.max_x
            || view.min_y > view.max_y
            || req.anchor.is_some_and(|a| !finite(a))
            || req
                .previous_segment
                .is_some_and(|(a, b)| !finite(a) || !finite(b))
        {
            return Err(SnapError::InvalidInput);
        }
        let search = Aabb::point(point).expanded(radius_mm);
        let mut raw: Vec<Ranked> = Vec::with_capacity(32);
        // Admission runs before a candidate is stored, so the many rejected
        // ones (most grid points, distant axes) are never copied around.
        let mut ws: Vec<Witness> = Vec::with_capacity(32);
        let excluded = |class: SnapClass, entity: Option<Uuid>| match (class, entity) {
            // `join-node` candidates are keyed by JOIN id, every other
            // entity-bearing class by WALL id; they are different identity
            // spaces and must not be tested against the same list. Node
            // exclusion stays the caller's to state, because only the gesture
            // knows whether the node moves (R3.4.2) or is merely its anchor.
            (SnapClass::JoinNode, Some(id)) => req.excluded_joins.contains(&id),
            (SnapClass::JoinNode, None) => false,
            (_, Some(id)) => req.excluded_walls.contains(&id),
            _ => false,
        };
        // R6.1.3: anchor coincidence filters EVERY class, never merely
        // deprioritized: "every acceptance that would produce a zero-length
        // segment". Both the exact point and the reported binary64 point are
        // checked, because the commit carries the latter (ED-0112).
        let admissible = |w: &Witness| {
            w.within(&radius)
                && req
                    .anchor
                    .is_none_or(|anchor| !w.is_at(anchor) && w.output() != anchor)
        };
        let push = |raw: &mut Vec<Ranked>, ws: &mut Vec<Witness>, mut c: Ranked, w: Witness| {
            if !excluded(c.class, c.entity) && admissible(&w) {
                c.witness = ws.len();
                ws.push(w);
                raw.push(c);
            }
        };

        if req.object_enabled {
            self.endpoints
                .tree()
                .query(&search, |_, _, &(id, class, at)| {
                    push(
                        &mut raw,
                        &mut ws,
                        Ranked {
                            class,
                            entity: Some(id),
                            feature: SnapFeature::Entity { point: at },
                            guide: None,
                            witness: 0,
                            ray: None,
                        },
                        Witness::new(point, Shape::Point(at)),
                    );
                });

            self.edges.query(&search, |_, _, seg| {
                // A hole no outer owns is not part of the region (as in the
                // full builder), so it has no ordinals and no candidates.
                let Some((component, ring)) = self.geometry.occupied_ordinals(seg.ring) else {
                    return;
                };
                let Some((witness, span)) = self.allowed_closest_point(seg, point, req) else {
                    return;
                };
                let discriminator = if witness.is_at(seg.a) {
                    0
                } else if witness.is_at(seg.b) {
                    2
                } else {
                    1
                };
                push(
                    &mut raw,
                    &mut ws,
                    Ranked {
                        class: SnapClass::Edge,
                        entity: None,
                        feature: SnapFeature::Edge {
                            component,
                            ring,
                            segment: seg.segment,
                            span,
                            discriminator,
                        },
                        guide: Some((seg.a, seg.b)),
                        witness: 0,
                        ray: None,
                    },
                    witness,
                );
            });

            // The camera can pan beyond the document's coordinate domain.
            // Include an outside captured pointer as well as the visible view.
            let mut cache = self.axis_cache.borrow_mut();
            if cache.as_ref().is_none_or(|(old, _)| *old != bounds) {
                let index =
                    SpatialIndex::build(self.axis_groups.live().filter_map(|(g, group)| {
                        axis_box(group.origin, group.end, bounds).map(|b| (b, g))
                    }));
                *cache = Some((bounds, index));
            }
            let (_, index) = cache.as_ref().expect("cache initialized");
            for g in index.query(&search) {
                let group = self.axis_groups.groups[g]
                    .as_ref()
                    .expect("indexed group is live");
                let witness = Witness::prepared(
                    point,
                    Shape::Linear {
                        origin: group.origin,
                        dir: Dir::between(group.origin, group.end),
                        extent: Extent::Line,
                    },
                    Some(&group.prep),
                );
                // Admission depends only on the shared witness: decide it
                // once, store the witness once, and let every member refer
                // to it (its exact state is then built at most once).
                if witness.branch() == Branch::Rejected || !admissible(&witness) {
                    continue;
                }
                let shared = ws.len();
                ws.push(witness);
                for (id, start, end, pa, pb) in &group.members {
                    if excluded(SnapClass::Axis, Some(*id)) {
                        continue;
                    }
                    // On the member's own segment iff the projection
                    // parameter is not strictly beyond both of its ends.
                    let w = &ws[shared];
                    let (x, y) = (w.cmp_param(pa), w.cmp_param(pb));
                    let on_segment = x != y || x == Ordering::Equal;
                    raw.push(Ranked {
                        class: SnapClass::Axis,
                        entity: Some(*id),
                        feature: SnapFeature::Axis { on_segment },
                        guide: clip_line_to_box(*start, *end, bounds),
                        witness: shared,
                        ray: None,
                    });
                }
            }
        }

        if req.grid_enabled {
            if let Some(s) = req.grid_spacing_mm {
                if s.is_finite() && s > 0.0 {
                    // A certified window (exact admission below decides).
                    let (Some(wx), Some(wy)) = (
                        lattice_window(point.x, radius_mm, s),
                        lattice_window(point.y, radius_mm, s),
                    ) else {
                        return Err(SnapError::GridRange);
                    };
                    let i0 = grid_index_bound(wx.0);
                    let i1 = grid_index_bound(wx.1);
                    let j0 = grid_index_bound(wy.0);
                    let j1 = grid_index_bound(wy.1);
                    let (Ok(i0), Ok(i1), Ok(j0), Ok(j1)) = (i0, i1, j0, j1) else {
                        return Err(SnapError::GridRange);
                    };
                    let (ni, nj) = (
                        i1.saturating_sub(i0).saturating_add(1),
                        j1.saturating_sub(j0).saturating_add(1),
                    );
                    if ni > 0 && nj > 0 && ni.saturating_mul(nj) <= MAX_GRID_CANDIDATES {
                        for i in i0..=i1 {
                            for j in j0..=j1 {
                                if lattice_certainly_outside(point, radius_mm, i, j, s) {
                                    continue;
                                }
                                push(
                                    &mut raw,
                                    &mut ws,
                                    Ranked {
                                        class: SnapClass::Grid,
                                        entity: None,
                                        feature: SnapFeature::Grid { i, j },
                                        guide: None,
                                        witness: 0,
                                        ray: None,
                                    },
                                    Witness::new(point, Shape::Lattice { i, j, s }),
                                );
                            }
                        }
                    } else {
                        return Err(SnapError::GridRange);
                    }
                } else {
                    return Err(SnapError::InvalidInput);
                }
            }
        }

        if req.angle_enabled {
            if let Some(anchor) = req.anchor {
                let previous = req.previous_segment.or_else(|| {
                    req.previous_direction
                        .filter(|d| d.0.is_finite() && d.1.is_finite())
                        .map(|(dx, dy)| (Point::new(0.0, 0.0), Point::new(dx, dy)))
                });
                for (dir, degrees) in angle_targets(previous) {
                    // R6.2.2: strictly ahead of the anchor, never at or behind
                    // it — decided exactly by the witness's `S > 0`.
                    let witness = Witness::new(
                        point,
                        Shape::Linear {
                            origin: anchor,
                            dir,
                            extent: Extent::Ray,
                        },
                    );
                    if witness.branch() == Branch::Rejected {
                        continue;
                    }
                    let direction = dir.approx();
                    push(
                        &mut raw,
                        &mut ws,
                        Ranked {
                            class: SnapClass::Angle,
                            entity: None,
                            feature: SnapFeature::Angle { direction, degrees },
                            guide: clip_line_to_box(
                                anchor,
                                Point::new(anchor.x + direction.0, anchor.y + direction.1),
                                bounds,
                            ),
                            witness: 0,
                            ray: Some(dir),
                        },
                        witness,
                    );
                }
            }
        }

        // The sort and dedupe work on a permutation of indices instead of
        // moving candidates.
        // The index is the last key, so the order is total and the unstable
        // (allocation-free) sort is deterministic.
        let mut kept: Vec<u32> = (0..raw.len() as u32).collect();
        kept.sort_unstable_by(|&a, &b| {
            rank(&raw[a as usize], &raw[b as usize], &ws).then(a.cmp(&b))
        });
        kept.dedup_by(|a, b| {
            let (a, b) = (&raw[*a as usize], &raw[*b as usize]);
            a.class == b.class && a.entity == b.entity && rank(a, b, &ws) == Ordering::Equal
        });

        Ok(kept
            .into_iter()
            .map(|i| &raw[i as usize])
            .map(|r| SnapCandidate {
                class: r.class,
                entity: r.entity,
                at: ws[r.witness].output(),
                feature: r.feature,
                guide: r.guide,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{JoinMember, Role, Wall, WallJoin};

    const P: fn(f64, f64) -> Point = Point::new;

    #[test]
    fn ray_equality_and_order_do_not_underflow_or_round_angle_labels() {
        let tiny = f64::from_bits(1);
        let dir = |x: f64, y: f64| Dir::between(P(0.0, 0.0), P(x, y));
        assert!(dir(tiny, tiny).same_ray(&dir(tiny * 2.0, tiny * 2.0)));
        assert!(!dir(tiny, tiny).same_ray(&dir(tiny, tiny * 2.0)));
        assert!(!dir(tiny, tiny).same_ray(&dir(-tiny, -tiny)));
        assert_eq!(
            dir(tiny, tiny).cmp_angle(&dir(tiny, tiny * 2.0)),
            Ordering::Less
        );
        for pair in GLOBAL_DIRECTIONS.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert_eq!(
                Dir::constant(a.0 as i8, a.1 as i8).cmp_angle(&Dir::constant(b.0 as i8, b.1 as i8)),
                Ordering::Less
            );
        }
        // A 45° turn of (3, 1) is exactly the ray (2, 4), not a rounded one.
        assert!(Dir::turned(P(0.0, 0.0), P(3.0, 1.0), 1).same_ray(&dir(1.0, 2.0)));
    }

    /// Codex GPT-6.1-Sol review (exact kernel): a long collinear chain whose
    /// ids arrive in DESCENDING order refreshes its group once per sync, not
    /// once per member (which was quadratic).
    #[test]
    fn a_long_collinear_chain_refreshes_its_group_once() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let n = 2000u128;
        for i in 0..n {
            let x = i as f64 * 3.0;
            let mut w = Wall::new(level, P(x, x / 3.0), P(x + 3.0, x / 3.0 + 1.0), 1.0, 2500.0);
            w.id = Uuid::from_u128(n - i);
            doc.walls.push(w);
        }
        let index = SnapIndex::build(&doc, level);
        index.validate();
        assert_eq!(index.axis_groups.live().count(), 1);
        assert_eq!(index.axis_groups.refreshes, 1);
    }

    /// Codex GPT-6.1-Sol review (exact kernel): a subnormal-length axis is a
    /// line like any other — its rounded clip overflows, and the index must
    /// keep it for exact admission instead of dropping it.
    #[test]
    fn a_subnormal_length_axis_still_snaps() {
        let eta = f64::from_bits(1);
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(eta, 0.0), 1.0, 2500.0));
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(1.0, 0.0), 0.0);
        req.visible_rect_mm = Aabb::from_corners(P(-2.0, -2.0), P(2.0, 2.0));
        let hits = index.query(&req).unwrap();
        assert!(
            hits.iter()
                .any(|c| c.class == SnapClass::Axis && c.at == P(1.0, 0.0)),
            "{hits:?}"
        );
    }

    /// Codex GPT-6.1-Sol review (exact kernel): a 2⁻⁶⁰⁰-scale previous
    /// segment loses its float enclosures; the angle candidate must still
    /// report a finite point near the exact projection, not infinity.
    #[test]
    fn a_tiny_previous_segment_reports_a_finite_projection() {
        let t = 2f64.powi(-600);
        let doc = Document::with_ground_floor();
        let index = SnapIndex::build(&doc, doc.levels[0].id);
        let mut req = empty_query(P(1.0, 0.4), 1.0);
        req.visible_rect_mm = Aabb::from_corners(P(-2.0, -2.0), P(2.0, 2.0));
        req.object_enabled = false;
        req.angle_enabled = true;
        req.anchor = Some(P(0.0, 0.0));
        req.previous_segment = Some((P(0.0, 0.0), P(3.0 * t, t)));
        let hits = index.query(&req).unwrap();
        let top = hits.first().expect("a candidate");
        assert_eq!(top.class, SnapClass::Angle);
        // Exact projection of (1, 0.4) on the ray (3, 1): (1.02, 0.34).
        assert!(
            (top.at.x - 1.02).abs() < 1e-12 && (top.at.y - 0.34).abs() < 1e-12,
            "{top:?}"
        );
    }

    #[test]
    fn a_finite_query_at_the_range_limit_is_answered() {
        let doc = Document::with_ground_floor();
        let index = SnapIndex::build(&doc, doc.levels[0].id);
        let at = P(f64::MAX, 0.0);
        let mut req = empty_query(at, 0.0);
        req.visible_rect_mm = Aabb::point(at);
        assert_eq!(index.query(&req), Ok(Vec::new()));
    }

    #[test]
    fn axis_extensions_follow_views_beyond_document_coordinate_bounds() {
        let (doc, level, _) = f1();
        let index = SnapIndex::build(&doc, level);
        for x in [20_000_000.0, -20_000_000.0] {
            let mut req = empty_query(P(x, 0.0), 120.0);
            req.visible_rect_mm = Aabb::from_corners(P(x - 1000.0, -1000.0), P(x + 1000.0, 1000.0));
            let hits = index.query(&req).unwrap();
            assert_eq!(hits[0].class, SnapClass::Axis);
            assert_eq!(hits[0].at, req.point);
            assert!(hits[0].guide.is_some());
            let key = index.axis_cache.borrow().as_ref().unwrap().0;
            req.point.x += 10.0;
            assert!(!index.query(&req).unwrap().is_empty());
            assert_eq!(index.axis_cache.borrow().as_ref().unwrap().0, key);
        }
    }

    /// A deterministic xorshift, so failures reproduce.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() * n as f64) as usize % n.max(1)
        }

        fn coord(&mut self) -> f64 {
            self.below(9) as f64 * 500.0
                + if self.below(4) == 0 {
                    (self.next() - 0.5) * 300.0
                } else {
                    0.0
                }
        }
    }

    /// Issue 0012: a MAINTAINED index answers every query exactly as one
    /// built from scratch, after every edit of a random stream — walls
    /// added, moved and deleted, joins made and dropped, a wall on another
    /// level — and under random wall exclusions.
    #[test]
    fn a_synced_index_answers_exactly_as_a_fresh_build_after_every_edit() {
        for seed in 1..=6u64 {
            let mut rng = Rng(0x51_7cc1_b727_220a ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let mut doc = Document::with_ground_floor();
            let level = doc.levels[0].id;
            let other = Uuid::now_v7();
            let mut index = SnapIndex::new(level);
            for step in 0..50 {
                match rng.below(10) {
                    0..=3 => {
                        let a = P(rng.coord(), rng.coord());
                        let b = if rng.below(2) == 0 {
                            P(a.x, rng.coord())
                        } else {
                            P(rng.coord(), a.y)
                        };
                        if a != b {
                            let on = if rng.below(8) == 0 { other } else { level };
                            doc.walls.push(Wall::new(on, a, b, 200.0, 2500.0));
                        }
                    }
                    4..=5 if !doc.walls.is_empty() => {
                        let id = doc.walls.remove(rng.below(doc.walls.len())).id;
                        doc.joins
                            .retain(|j| j.members.iter().all(|m| m.wall_id != id));
                    }
                    6..=7 if !doc.walls.is_empty() => {
                        let i = rng.below(doc.walls.len());
                        let end = P(rng.coord(), rng.coord());
                        if end != doc.walls[i].start() {
                            doc.walls[i].end_mm = end.into();
                        }
                    }
                    8 if doc.walls.len() >= 2 => {
                        let (a, b) = (rng.below(doc.walls.len()), rng.below(doc.walls.len()));
                        let (wa, wb) = (doc.walls[a].id, doc.walls[b].id);
                        let free = |w: Uuid, r: Role| {
                            doc.joins
                                .iter()
                                .all(|j| j.members.iter().all(|m| m.wall_id != w || m.role != r))
                        };
                        if a != b && free(wa, Role::End) && free(wb, Role::Start) {
                            doc.joins.push(WallJoin::new(vec![
                                JoinMember::new(wa, Role::End),
                                JoinMember::new(wb, Role::Start),
                            ]));
                        }
                    }
                    _ if !doc.joins.is_empty() => {
                        doc.joins.remove(rng.below(doc.joins.len()));
                    }
                    _ => {}
                }
                index.sync(&doc);
                index.validate();
                let fresh = SnapIndex::build(&doc, level);
                assert_eq!(
                    index.geometry().plan(),
                    crate::walls::plan(
                        &level_solids(&doc, level).walls,
                        &level_solids(&doc, level).joins
                    ),
                    "seed {seed} step {step}: geometry"
                );
                let excluded: Vec<Uuid> = doc
                    .walls
                    .iter()
                    .filter(|_| rng.below(5) == 0)
                    .map(|w| w.id)
                    .collect();
                for _ in 0..12 {
                    let mut req = empty_query(P(rng.coord(), rng.coord()), 260.0);
                    if rng.below(2) == 0 {
                        req.excluded_walls = &excluded;
                    }
                    assert_eq!(
                        index.query(&req),
                        fresh.query(&req),
                        "seed {seed} step {step} at {:?}",
                        req.point
                    );
                }
            }
        }
    }

    /// Moving one wall in a plan changes a bounded part of the index.
    #[test]
    fn a_local_edit_resyncs_a_bounded_part_of_the_index() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let s = 3000.0;
        for i in 0..=8 {
            for j in 0..8 {
                let (a, b, c) = (i as f64 * s, j as f64 * s, (j + 1) as f64 * s);
                doc.walls
                    .push(Wall::new(level, P(b, a), P(c, a), 200.0, 2500.0));
                doc.walls
                    .push(Wall::new(level, P(a, b), P(a, c), 200.0, 2500.0));
            }
        }
        let mut index = SnapIndex::build(&doc, level);
        let i = doc.walls.len() / 2;
        let (start, end) = (doc.walls[i].start(), doc.walls[i].end());
        doc.walls[i].start_mm = P(start.x + 50.0, start.y).into();
        doc.walls[i].end_mm = P(end.x + 50.0, end.y).into();
        let stats = index.sync(&doc).clone();
        assert_eq!(stats.axes_changed, 1, "{stats:?}");
        assert_eq!(stats.endpoints_changed, 2, "{stats:?}");
        assert!(stats.edges_added <= 64, "{stats:?}");
    }

    fn empty_query(point: Point, radius_mm: f64) -> SnapQuery<'static> {
        SnapQuery {
            point,
            radius_mm,
            visible_rect_mm: Aabb::from_corners(P(-1e7, -1e7), P(1e7, 1e7)),
            anchor: None,
            previous_direction: None,
            previous_segment: None,
            screen_radius: None,
            excluded_walls: &[],
            excluded_joins: &[],
            grid_spacing_mm: None,
            grid_enabled: false,
            object_enabled: true,
            angle_enabled: false,
            alt_disabled: false,
        }
    }

    /// F1-shaped fixture: one isolated wall `W1` (0,0)->(4000,0).
    fn f1() -> (Document, Uuid, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(4000.0, 0.0), 200.0, 2500.0));
        let wall = doc.walls[0].id;
        assert_eq!(doc.validate(), vec![]);
        (doc, level, wall)
    }

    /// F2-shaped fixture: two walls joined end-to-end at (4000,0).
    fn f2() -> (Document, Uuid, Uuid, Uuid, Uuid) {
        let (mut doc, level, w1) = f1();
        doc.walls.push(Wall::new(
            level,
            P(4000.0, 0.0),
            P(4000.0, 3000.0),
            200.0,
            2500.0,
        ));
        let w2 = doc.walls[1].id;
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(w1, Role::End),
            JoinMember::new(w2, Role::Start),
        ]));
        let join = doc.joins[0].id;
        assert_eq!(doc.validate(), vec![]);
        (doc, level, w1, w2, join)
    }

    // ---- S1: inclusive boundary; axis outranks grid ----

    #[test]
    fn s1_free_endpoint_wins_at_exact_boundary_and_class_order_beats_grid() {
        let (doc, level, w1) = f1();
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(3880.0, 0.0), 120.0);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        let hits = index.query(&req).unwrap();
        assert_eq!(hits[0].class, SnapClass::FreeEndpoint);
        assert_eq!(hits[0].entity, Some(w1));
        assert_eq!(hits[0].at, P(4000.0, 0.0));
    }

    #[test]
    fn s1_axis_extension_wins_beyond_the_walls_own_endpoint() {
        // R6.1.1: the axis candidate is NOT pruned by the wall's own segment
        // bounds; the winning projection here is 880 mm past `W1.End`.
        let (doc, level, w1) = f1();
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(4880.0, 0.0), 120.0);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        let hits = index.query(&req).unwrap();
        assert_eq!(hits[0].class, SnapClass::Axis);
        assert_eq!(hits[0].entity, Some(w1));
        assert_eq!(hits[0].at, P(4880.0, 0.0));
        assert!(
            hits[0].at.x > 4000.0 + 200.0,
            "the winning projection extends past the wall's own endpoint"
        );
    }

    // ---- S2: equal-distance tuple tie, then UUID tie ----

    #[test]
    fn s2_equal_distance_free_endpoints_break_by_tuple_then_by_uuid() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let mut wa = Wall::new(level, P(-600.0, 0.0), P(940.0, 0.0), 200.0, 2500.0);
        let mut wb = Wall::new(level, P(1060.0, 0.0), P(2600.0, 0.0), 200.0, 2500.0);
        wa.id = Uuid::from_u128(2);
        wb.id = Uuid::from_u128(1);
        doc.walls = vec![wa.clone(), wb.clone()];
        assert_eq!(doc.validate(), vec![]);
        let index = SnapIndex::build(&doc, level);

        // Equidistant from (940,0) and (1060,0): tuple order picks the
        // smaller x regardless of which UUID is smaller.
        let hits = index.query(&empty_query(P(1000.0, 0.0), 200.0)).unwrap();
        assert_eq!(hits[0].at, P(940.0, 0.0));
        assert_eq!(hits[0].entity, Some(wa.id));

        // Two endpoints at the SAME point differ only by wall id.
        let mut doc2 = Document::with_ground_floor();
        doc2.walls.push(Wall::new(
            level,
            P(4000.0, 0.0),
            P(6000.0, 0.0),
            200.0,
            2500.0,
        ));
        doc2.walls.push(Wall::new(
            level,
            P(2000.0, 0.0),
            P(4000.0, 0.0),
            200.0,
            2500.0,
        ));
        doc2.walls[0].id = Uuid::from_u128(20);
        doc2.walls[1].id = Uuid::from_u128(10);
        let index2 = SnapIndex::build(&doc2, level);
        let hits2 = index2.query(&empty_query(P(4000.0, 0.0), 10.0)).unwrap();
        let endpoints: Vec<_> = hits2
            .iter()
            .filter(|h| h.class == SnapClass::FreeEndpoint)
            .collect();
        assert_eq!(endpoints.len(), 2, "{endpoints:#?}");
        assert_eq!(
            endpoints[0].entity,
            Some(Uuid::from_u128(10)),
            "ascending UUID wins the full-key tie"
        );
    }

    // ---- S3: joined endpoints deduplicate to one join-node candidate ----

    #[test]
    fn s3_joined_endpoints_produce_exactly_one_join_node_candidate() {
        let (doc, level, _, _, join) = f2();
        let index = SnapIndex::build(&doc, level);
        let hits = index.query(&empty_query(P(4010.0, -10.0), 200.0)).unwrap();
        let join_nodes: Vec<_> = hits
            .iter()
            .filter(|h| h.class == SnapClass::JoinNode)
            .collect();
        assert_eq!(join_nodes.len(), 1, "{hits:#?}");
        assert_eq!(join_nodes[0].entity, Some(join));
        assert_eq!(join_nodes[0].at, P(4000.0, 0.0));
        assert!(
            !hits.iter().any(|h| h.class == SnapClass::FreeEndpoint),
            "member endpoints must not also appear as free endpoints: {hits:#?}"
        );
    }

    // ---- S4: self-snap exclusion and legal winners ----

    #[test]
    fn s4_anchor_coincident_candidates_are_removed_across_every_class() {
        let (mut doc, level, w1) = f1();
        // Second segment (3000,0)->accepted nowhere yet); anchor at seg1's end.
        doc.walls[0] = Wall::new(level, P(0.0, 0.0), P(3000.0, 0.0), 200.0, 2500.0);
        let seg1 = doc.walls[0].id;
        assert_eq!(w1, w1); // silence unused binding warning path clarity
        assert_eq!(doc.validate(), vec![]);
        let index = SnapIndex::build(&doc, level);

        let mut req = empty_query(P(3150.0, 10.0), 120.0);
        req.anchor = Some(P(3000.0, 0.0));
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        let hits = index.query(&req).unwrap();

        assert!(
            hits.iter().all(|h| h.at != P(3000.0, 0.0)),
            "the anchor-coincident free endpoint, edge, axis and grid \
             candidates must all be excluded: {hits:#?}"
        );
        // The axis extension 150 mm past seg1's end wins (S4 part a).
        assert_eq!(hits[0].class, SnapClass::Axis);
        assert_eq!(hits[0].entity, Some(seg1));
        assert_eq!(hits[0].at, P(3150.0, 0.0));
    }

    #[test]
    fn s4_edge_beats_axis_when_both_qualify_and_the_anchor_is_still_excluded() {
        let (mut doc, level, _) = f1();
        doc.walls[0] = Wall::new(level, P(0.0, 0.0), P(3000.0, 0.0), 200.0, 2500.0);
        assert_eq!(doc.validate(), vec![]);
        let index = SnapIndex::build(&doc, level);

        let mut req = empty_query(P(3025.0, 0.0), 120.0);
        req.anchor = Some(P(3000.0, 0.0));
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        let hits = index.query(&req).unwrap();
        assert_eq!(hits[0].class, SnapClass::Edge, "{hits:#?}");
        assert!(hits.iter().all(|h| h.at != P(3000.0, 0.0)));
    }

    // ---- S5: angle families normalize and dedupe; ahead-of-anchor check ----

    #[test]
    fn s5_relative_and_global_45_degree_families_dedupe_to_one_winner() {
        let doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(4000.0, 1000.0), 200.0);
        req.anchor = Some(P(3000.0, 0.0));
        req.previous_direction = Some((3000.0, 0.0));
        req.angle_enabled = true;
        let hits = index.query(&req).unwrap();
        assert_eq!(
            hits.len(),
            1,
            "global and relative 45 degree dedupe: {hits:#?}"
        );
        assert_eq!(hits[0].class, SnapClass::Angle);
        assert_eq!(
            hits[0].at,
            P(4000.0, 1000.0),
            "on-ray pointer projects to itself"
        );
    }

    #[test]
    fn angle_candidate_off_every_ray_or_behind_the_anchor_is_excluded() {
        let doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let index = SnapIndex::build(&doc, level);
        // Chosen off every one of the eight global rays and, on the ones it
        // is ahead of, well outside a 50 mm perpendicular radius (see the
        // derivation in the review that added this fixture).
        let mut req = empty_query(P(2500.0, 137.0), 50.0);
        req.anchor = Some(P(3000.0, 0.0));
        req.angle_enabled = true;
        let hits = index.query(&req).unwrap();
        assert!(
            hits.iter().all(|h| h.class != SnapClass::Angle),
            "{hits:#?}"
        );
    }

    // ---- S6: Alt disables every class ----

    #[test]
    fn s6_alt_disabled_returns_no_candidates_regardless_of_toggles() {
        let (doc, level, _) = f1();
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(0.0, 0.0), 1_000_000.0);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        req.alt_disabled = true;
        assert!(index.query(&req).unwrap().is_empty());
    }

    // ---- S7: touch versus mouse radii ----

    #[test]
    fn s7_the_same_query_misses_at_mouse_radius_and_hits_at_touch_radius() {
        let (doc, level, w1) = f1();
        let index = SnapIndex::build(&doc, level);
        let point = P(4150.0, 150.0);
        assert!(index
            .query(&empty_query(point, 12.0 * 10.0))
            .unwrap()
            .is_empty());
        let hits = index.query(&empty_query(point, 24.0 * 10.0)).unwrap();
        assert_eq!(hits[0].class, SnapClass::FreeEndpoint);
        assert_eq!(hits[0].entity, Some(w1));
    }

    // ---- toggles ----

    #[test]
    fn object_toggle_off_hides_join_node_free_endpoint_edge_and_axis() {
        let (doc, level, _) = f1();
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(2000.0, 0.0), 200.0);
        req.object_enabled = false;
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        let hits = index.query(&req).unwrap();
        assert!(hits.iter().all(|h| h.class == SnapClass::Grid), "{hits:#?}");
    }

    #[test]
    fn grid_toggle_off_hides_grid_candidates() {
        let doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(100.0, 0.0), 50.0);
        req.grid_enabled = false;
        req.grid_spacing_mm = Some(100.0);
        assert!(index.query(&req).unwrap().is_empty());
        req.grid_enabled = true;
        assert!(!index.query(&req).unwrap().is_empty());
    }

    #[test]
    fn an_unsupported_grid_range_is_an_explicit_error() {
        let doc = Document::with_ground_floor();
        let index = SnapIndex::build(&doc, doc.levels[0].id);
        for radius in [1e6, 1e308] {
            let mut req = empty_query(P(0.0, 0.0), radius);
            req.grid_enabled = true;
            req.grid_spacing_mm = Some(100.0);
            assert_eq!(index.query(&req), Err(SnapError::GridRange));
        }
    }

    // ---- grid index bound: overflow, boundary, and adjacent-representable
    // adversarial cases (issue 0058 saturation fix) ----

    #[test]
    fn grid_index_bound_rejects_positive_and_negative_i64_overflow() {
        assert_eq!(
            grid_index_bound(1e25),
            Err(SnapError::GridRange),
            "must not silently saturate to i64::MAX"
        );
        assert_eq!(
            grid_index_bound(-1e25),
            Err(SnapError::GridRange),
            "must not silently saturate to i64::MIN"
        );
    }

    #[test]
    fn grid_index_bound_allows_exact_i64_min_but_rejects_i64_max_rounded_up() {
        // i64::MIN is exactly representable as f64: the inclusive lower bound
        // is legal.
        assert_eq!(grid_index_bound(i64::MIN as f64), Ok(i64::MIN));
        // i64::MAX (2^63 - 1) is NOT exactly representable; it rounds UP to
        // 2^63 in f64, one past the largest legal i64. An inclusive upper
        // comparison against that rounded value would wrongly admit it.
        assert_eq!(grid_index_bound(i64::MAX as f64), Err(SnapError::GridRange));
    }

    #[test]
    fn grid_index_bound_handles_values_immediately_adjacent_to_the_boundary() {
        const LOWER: f64 = -9_223_372_036_854_775_808.0; // -2^63, exactly i64::MIN
        const UPPER: f64 = 9_223_372_036_854_775_808.0; // 2^63, one past i64::MAX
        assert_eq!(grid_index_bound(LOWER), Ok(i64::MIN));

        // The next representable f64 below LOWER (more negative) must be
        // rejected; the exact boundary above is legal.
        let just_below_lower = f64::from_bits(LOWER.to_bits() + 1);
        assert!(just_below_lower < LOWER);
        assert_eq!(
            grid_index_bound(just_below_lower),
            Err(SnapError::GridRange)
        );

        // The next representable f64 below UPPER is strictly less than 2^63
        // and must be accepted; UPPER itself and anything at/above it must not.
        let just_below_upper = f64::from_bits(UPPER.to_bits() - 1);
        assert!(just_below_upper < UPPER);
        assert!(grid_index_bound(just_below_upper).is_ok());
        assert_eq!(grid_index_bound(UPPER), Err(SnapError::GridRange));
    }

    #[test]
    fn grid_index_bound_rejects_non_finite_values() {
        assert_eq!(grid_index_bound(f64::NAN), Err(SnapError::GridRange));
        assert_eq!(grid_index_bound(f64::INFINITY), Err(SnapError::GridRange));
        assert_eq!(
            grid_index_bound(f64::NEG_INFINITY),
            Err(SnapError::GridRange)
        );
    }

    #[test]
    fn grid_query_rejects_a_point_whose_index_overflows_i64_instead_of_saturating() {
        let doc = Document::with_ground_floor();
        let index = SnapIndex::build(&doc, doc.levels[0].id);
        let mut req = empty_query(P(1e25, 0.0), 10.0);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        assert_eq!(
            index.query(&req),
            Err(SnapError::GridRange),
            "a point far outside the i64 lattice domain must error, not \
             silently clamp to an i64::MAX/MIN index"
        );

        let mut req_y = empty_query(P(0.0, -1e25), 10.0);
        req_y.grid_enabled = true;
        req_y.grid_spacing_mm = Some(100.0);
        assert_eq!(index.query(&req_y), Err(SnapError::GridRange));
    }

    #[test]
    fn grid_query_rejects_overflow_from_tiny_spacing_even_at_an_ordinary_point() {
        let doc = Document::with_ground_floor();
        let index = SnapIndex::build(&doc, doc.levels[0].id);
        let mut req = empty_query(P(0.0, 0.0), 10.0);
        req.grid_enabled = true;
        // `1e-300` is too coarse: `10.0 / 1e-300 == 1e301`, still finite.
        // The smallest positive subnormal genuinely divides out to infinity,
        // which is the actual quotient-overflow case (not_finite, not just
        // out-of-i64-range).
        req.grid_spacing_mm = Some(f64::from_bits(1));
        assert_eq!(index.query(&req), Err(SnapError::GridRange));
    }

    #[test]
    fn alt_disabled_bypasses_grid_overflow_that_would_otherwise_error() {
        let doc = Document::with_ground_floor();
        let index = SnapIndex::build(&doc, doc.levels[0].id);
        let mut req = empty_query(P(1e25, 0.0), 10.0);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        req.alt_disabled = true;
        assert_eq!(index.query(&req), Ok(Vec::new()));
    }

    #[test]
    fn grid_disabled_bypasses_grid_overflow_that_would_otherwise_error() {
        let doc = Document::with_ground_floor();
        let index = SnapIndex::build(&doc, doc.levels[0].id);
        let mut req = empty_query(P(1e25, 0.0), 10.0);
        req.grid_enabled = false;
        req.grid_spacing_mm = Some(100.0);
        assert!(index.query(&req).unwrap().is_empty());
    }

    #[test]
    fn grid_query_still_returns_ordinary_lattice_points_unchanged() {
        let doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(100.0, 0.0), 50.0);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        let hits = index.query(&req).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.class == SnapClass::Grid
                    && h.feature == (SnapFeature::Grid { i: 1, j: 0 }))
        );
    }

    // ---- determinism (R6.3.3) ----

    #[test]
    fn winner_does_not_depend_on_document_or_wall_order() {
        let (doc, level, _, _, _) = f2();
        let index = SnapIndex::build(&doc, level);
        let point = P(4010.0, -10.0);
        let forward = index.query(&empty_query(point, 200.0)).unwrap();

        let mut reversed = doc.clone();
        reversed.walls.reverse();
        reversed.joins.reverse();
        let index2 = SnapIndex::build(&reversed, level);
        let backward = index2.query(&empty_query(point, 200.0)).unwrap();

        assert_eq!(forward, backward);
    }

    // ---- adversarial: identity exclusion versus anchor exclusion ----

    /// Numerics counterexample 1 (issue-0030 ownership review, 2026-09-12):
    /// two collinear abutting walls of equal thickness merge into ONE
    /// rectangle, so each long face is a single canonical edge spanning both.
    fn collinear_pair() -> (Document, Uuid, Uuid, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(1000.0, 0.0), 100.0, 2500.0));
        doc.walls.push(Wall::new(
            level,
            P(1000.0, 0.0),
            P(3000.0, 0.0),
            100.0,
            2500.0,
        ));
        let (static_wall, dragged) = (doc.walls[0].id, doc.walls[1].id);
        assert_eq!(doc.validate(), vec![]);
        (doc, level, static_wall, dragged)
    }

    #[test]
    fn a_merged_collinear_face_excludes_only_the_dragged_walls_own_span() {
        let (doc, level, static_wall, dragged) = collinear_pair();
        let index = SnapIndex::build(&doc, level);
        let excluded = [dragged];

        // The whole bottom face is ONE canonical edge from (0,-50) to
        // (3000,-50): the merged tag set names both walls and can distinguish
        // neither, which is why the decision is made per sub-span.
        let mut over_dragged = empty_query(P(2500.0, -52.0), 120.0);
        over_dragged.excluded_walls = &excluded;
        let candidates = index.query(&over_dragged).unwrap();
        assert!(
            !candidates.iter().any(|c| c.class == SnapClass::Edge),
            "a point on the dragged wall's own face is a self-snap: {candidates:#?}"
        );

        // Excluding the edge WHOLE would be the opposite error, so the static
        // wall's own half of the same canonical edge must survive intact.
        let mut over_static = empty_query(P(500.0, -52.0), 120.0);
        over_static.excluded_walls = &excluded;
        let kept = index.query(&over_static).unwrap();
        let edge = kept
            .iter()
            .find(|c| c.class == SnapClass::Edge)
            .expect("the static wall's face is not the dragged wall's");
        assert_eq!(edge.at, P(500.0, -50.0));

        // And with nothing excluded the same pointer still reaches the
        // dragged wall's span, so the difference above is exclusion and not a
        // geometry change.
        let plain = index.query(&empty_query(P(2500.0, -52.0), 120.0)).unwrap();
        assert!(plain
            .iter()
            .any(|c| c.class == SnapClass::Edge && c.at == P(2500.0, -50.0)));
        let _ = static_wall;
    }

    #[test]
    fn two_equidistant_retained_spans_resolve_by_the_comparators_own_key() {
        // Excluding the MIDDLE of three collinear walls leaves one canonical
        // edge with two disjoint retained spans. A pointer on the excluded
        // span's perpendicular bisector is exactly equidistant from both, so
        // the choice must come from R6.3.1's level-3 key rather than from
        // iteration order.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        for (x0, x1) in [(0.0, 1000.0), (1000.0, 2000.0), (2000.0, 3000.0)] {
            doc.walls
                .push(Wall::new(level, P(x0, 0.0), P(x1, 0.0), 100.0, 2500.0));
        }
        let middle = doc.walls[1].id;
        assert_eq!(doc.validate(), vec![]);
        let index = SnapIndex::build(&doc, level);

        // Probe the TOP face deliberately: the canonical outer ring runs
        // counter-clockwise, so that edge is traversed in -x and its FIRST
        // retained span is the one with the LARGER x. Iteration order and the
        // comparator key therefore disagree here, which is the only way this
        // assertion can tell them apart.
        let excluded = [middle];
        let mut req = empty_query(P(1500.0, 52.0), 520.0);
        req.excluded_walls = &excluded;
        let candidates = index.query(&req).unwrap();
        let edge = candidates
            .iter()
            .find(|c| c.class == SnapClass::Edge && c.at.y == 50.0)
            .expect("both retained spans are within radius");
        // (1000,50) and (2000,50) are equidistant; the smaller (x, y) wins.
        assert_eq!(edge.at, P(1000.0, 50.0));

        // And the excluded middle is genuinely unreachable, not merely
        // outranked.
        let mut over_middle = empty_query(P(1500.0, 52.0), 120.0);
        over_middle.excluded_walls = &excluded;
        assert!(!index
            .query(&over_middle)
            .unwrap()
            .iter()
            .any(|c| c.class == SnapClass::Edge));
    }

    #[test]
    fn one_vertex_run_shared_by_two_walls_yields_two_distinct_edge_features() {
        // Without the sub-span index these two frames would report the SAME
        // feature, so a guide would persist across a change of owner and a
        // caller tracking identity frame to frame could not tell them apart.
        let (doc, level, _static_wall, _dragged) = collinear_pair();
        let index = SnapIndex::build(&doc, level);

        let over = |x: f64| {
            index
                .query(&empty_query(P(x, -52.0), 120.0))
                .unwrap()
                .into_iter()
                .find(|c| c.class == SnapClass::Edge)
                .expect("the shared bottom face is within radius")
        };
        let (left, right) = (over(500.0), over(2500.0));

        let key = |f: SnapFeature| match f {
            SnapFeature::Edge {
                component,
                ring,
                segment,
                span,
                discriminator,
            } => (component, ring, segment, span, discriminator),
            other => panic!("expected an edge feature, got {other:?}"),
        };
        let (lc, lr, ls, lspan, _) = key(left.feature);
        let (rc, rr, rs, rspan, _) = key(right.feature);
        assert_eq!(
            (lc, lr, ls),
            (rc, rr, rs),
            "one canonical vertex run, so the R3.2.4 key is shared"
        );
        assert_ne!(lspan, rspan, "different owners, so different features");
    }

    /// ED-0112's output contract: a projected candidate's reported point is
    /// within a few ulps of the largest magnitude involved of the EXACT
    /// projection — checked here in exact arithmetic over random lines up to
    /// ED-0100's coordinate bound.
    #[test]
    fn a_projected_output_is_within_its_bound_of_the_exact_projection() {
        use crate::dyadic::Dyadic;
        const C: f64 = 1.1e7;
        let ex = Dyadic::from_f64;
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 11) as f64 / (1u64 << 53) as f64) * 2.0 * C - C
        };
        for case in 0..20_000 {
            let (ax, ay) = (next(), next());
            let (bx, by) = match case % 4 {
                0 => (next(), next()),
                1 => (next(), ay),
                2 => (ax, next()),
                _ => (ax + next() * 1e-9, ay + next() * 1e-9),
            };
            let (a, b, p) = (P(ax, ay), P(bx, by), P(next(), next()));
            if a == b {
                continue;
            }
            let w = Witness::new(
                p,
                Shape::Linear {
                    origin: a,
                    dir: Dir::between(a, b),
                    extent: Extent::Line,
                },
            );
            let q = w.output();
            // Exact projection X = (a·D + v·S)/D; check |q·D − (a·D + v·S)| ≤ e·D.
            let (vx, vy) = (ex(bx).sub(&ex(ax)), ex(by).sub(&ex(ay)));
            let dd = vx.square().add(&vy.square());
            let s = ex(p.x)
                .sub(&ex(ax))
                .mul(&vx)
                .add(&ex(p.y).sub(&ex(ay)).mul(&vy));
            let m = [ax, ay, bx, by, p.x, p.y]
                .iter()
                .fold(1.0f64, |m, v| m.max(v.abs()));
            let e = ex(64.0 * f64::EPSILON * m);
            for (qc, ac, vc) in [(q.x, ax, &vx), (q.y, ay, &vy)] {
                let err = ex(qc).mul(&dd).sub(&ex(ac).mul(&dd).add(&vc.mul(&s)));
                assert!(
                    err.abs() <= e.mul(&dd),
                    "case {case}: {a:?} {b:?} {p:?} -> {q:?}"
                );
            }
        }
    }

    #[test]
    fn equidistant_retained_spans_ignore_the_faces_traversal_direction() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        // Grid-representable (multiples of 2^-10 mm), so derived boundary
        // coordinates are the stored ones and the tie is exact.
        for (x0, x1) in [(-1000.0, 1.125), (1.125, 1.875), (1.875, 1000.0)] {
            doc.walls
                .push(Wall::new(level, P(x0, 0.0), P(x1, 0.0), 2.0, 2500.0));
        }
        let middle = doc.walls[1].id;
        assert_eq!(doc.validate(), vec![]);
        let index = SnapIndex::build(&doc, level);
        let excluded = [middle];

        let winner_x = |y: f64| {
            let mut req = empty_query(P(1.5, y), 0.5);
            req.excluded_walls = &excluded;
            index
                .query(&req)
                .unwrap()
                .into_iter()
                .find(|c| c.class == SnapClass::Edge)
                .expect("a retained span is within radius on both faces")
                .at
                .x
        };
        assert_eq!(
            winner_x(-1.0),
            1.125,
            "bottom face, traversed left to right"
        );
        assert_eq!(winner_x(1.0), 1.125, "top face, traversed right to left");
    }

    /// The same defect seen from the admission side: a retained span's own
    /// endpoint sitting exactly at the radius must be admitted, which needs
    /// the stored break to BE the input corner rather than an ulp below it.
    #[test]
    fn a_retained_spans_endpoint_is_admitted_at_exactly_the_radius() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(-1000.0, 0.0), P(0.25, 0.0), 2.0, 2500.0));
        doc.walls
            .push(Wall::new(level, P(0.25, 0.0), P(1000.0, 0.0), 2.0, 2500.0));
        let dragged = doc.walls[1].id;
        assert_eq!(doc.validate(), vec![]);
        let index = SnapIndex::build(&doc, level);

        let point = P(0.375, -1.0);
        let corner = P(0.25, -1.0);
        let excluded = [dragged];
        let mut req = empty_query(point, point.x - corner.x);
        req.excluded_walls = &excluded;

        let candidates = index.query(&req).unwrap();
        let edge = candidates
            .iter()
            .find(|c| c.class == SnapClass::Edge)
            .expect("the retained span reaches the pointer at exactly the radius");
        assert_eq!(edge.at, corner);
    }

    #[test]
    fn a_query_at_a_boundary_vertex_returns_that_stored_vertex_exactly() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        // Deliberately awkward, non-dyadic, and far from the origin, which is
        // where the reconstruction and the stored value part company.
        doc.walls.push(Wall::new(
            level,
            P(-1509616.2171497215, 6537042.49344076),
            P(-7523960.777007088, -5535220.707859709),
            237.0,
            2500.0,
        ));
        assert_eq!(doc.validate(), vec![]);
        let solids = crate::walls::level_solids(&doc, level);
        let region = crate::walls::occupied_region_with_sources(&solids.walls, &solids.joins);
        let index = SnapIndex::build(&doc, level);

        let ring = &region.components[0].polygon.outer;
        for i in 0..ring.len() {
            // Edge `i` ENDS at this vertex, so its projection parameter
            // clamps high — the case that used to be rebuilt. The edge
            // STARTING here clamps low and returns its own stored `a`
            // unchanged either way, which is why the check must name the
            // segment rather than accept any candidate at the point.
            let vertex = ring[(i + 1) % ring.len()];
            let candidates = index.query(&empty_query(vertex, 1.0)).unwrap();
            let ending = candidates
                .iter()
                .find(|c| matches!(c.feature, SnapFeature::Edge { segment, .. } if segment == i))
                .unwrap_or_else(|| panic!("edge {i} is within a millimetre of its own end"));
            assert_eq!(
                ending.at, vertex,
                "edge {i} answered with a rebuilt endpoint rather than its stored one"
            );
        }
    }

    #[test]
    fn a_retained_span_supplies_its_own_nearest_point_past_an_excluded_one() {
        // The excluded span is nearer than the retained one. Dropping the
        // candidate at the edge's closest point would silently delete the
        // static wall's face; the allowed set is the retained spans, so the
        // candidate is the closest point OF THOSE.
        let (doc, level, _static_wall, dragged) = collinear_pair();
        let index = SnapIndex::build(&doc, level);
        let excluded = [dragged];
        let mut req = empty_query(P(1050.0, -52.0), 120.0);
        req.excluded_walls = &excluded;
        let candidates = index.query(&req).unwrap();
        let edge = candidates
            .iter()
            .find(|c| c.class == SnapClass::Edge)
            .expect("the static span is within radius");
        assert_eq!(edge.at, P(1000.0, -50.0));
    }

    /// Numerics counterexample 2: a join patch is the convex hull of every
    /// member's cross-section (R3.2.3), so the fragments of it that reach the
    /// occupied boundary run BETWEEN members and carry no single member's
    /// identity. In F2 that fragment is the edge (4000,-100) -> (4100,0),
    /// whose two endpoints come from the two different walls' chords.
    ///
    /// Radius 40 from this probe reaches the fragment and nothing else: the
    /// nearest wall faces, the axis projections and the join node all sit at
    /// 50^2 + 50^2 mm^2. Radius 120 additionally reaches the node and both
    /// axes, which is what lets one fixture separate the two exclusion sets.
    fn patch_fragment_probe() -> Point {
        P(4050.0, -50.0)
    }

    #[test]
    fn a_moving_member_excludes_the_patch_it_deforms_without_being_told_the_join() {
        let (doc, level, w1, _w2, join) = f2();
        let index = SnapIndex::build(&doc, level);

        let before = index
            .query(&empty_query(patch_fragment_probe(), 40.0))
            .unwrap();
        assert_eq!(before.len(), 1, "{before:#?}");
        assert_eq!(before[0].class, SnapClass::Edge);
        assert_eq!(before[0].at, patch_fragment_probe());

        // Excluding the WALL alone must take the patch with it: the hull is a
        // joint function of every member, so it deforms whichever end of `w1`
        // the gesture is holding.
        let excluded = [w1];
        let mut req = empty_query(patch_fragment_probe(), 40.0);
        req.excluded_walls = &excluded;
        assert!(req.excluded_joins.is_empty());
        assert_eq!(index.query(&req).unwrap(), vec![]);

        // The NODE is a separate question and this gesture has not answered
        // it: dragging `w1`'s far end deforms this patch while leaving this
        // node exactly where it was, so the node candidate must survive.
        let mut wide = empty_query(patch_fragment_probe(), 120.0);
        wide.excluded_walls = &excluded;
        let candidates = index.query(&wide).unwrap();
        assert!(
            candidates
                .iter()
                .any(|c| c.class == SnapClass::JoinNode && c.entity == Some(join)),
            "{candidates:#?}"
        );
    }

    #[test]
    fn an_excluded_node_does_not_take_the_static_patch_it_anchors_with_it() {
        // The mirror case: a node excluded for being the gesture's anchor sits
        // on a patch that nothing is moving. Deleting the patch here would
        // erode a legal target rather than prevent a self-snap.
        let (doc, level, _w1, w2, join) = f2();
        let index = SnapIndex::build(&doc, level);
        let joins = [join];

        let mut narrow = empty_query(patch_fragment_probe(), 40.0);
        narrow.excluded_joins = &joins;
        let patch = index.query(&narrow).unwrap();
        assert_eq!(patch.len(), 1, "{patch:#?}");
        assert_eq!(patch[0].class, SnapClass::Edge);

        let mut wide = empty_query(patch_fragment_probe(), 120.0);
        wide.excluded_joins = &joins;
        let candidates = index.query(&wide).unwrap();
        assert!(!candidates.iter().any(|c| c.class == SnapClass::JoinNode));
        // Neither wall was named, so both keep their own axes.
        assert!(candidates
            .iter()
            .any(|c| c.class == SnapClass::Axis && c.entity == Some(w2)));
    }

    #[test]
    fn the_wall_and_join_exclusion_sets_stay_independent() {
        let (doc, level, w1, _w2, join) = f2();
        let index = SnapIndex::build(&doc, level);
        let walls = [w1];
        let joins = [join];

        let mut wall_only = empty_query(patch_fragment_probe(), 120.0);
        wall_only.excluded_walls = &walls;
        let mut join_only = empty_query(patch_fragment_probe(), 120.0);
        join_only.excluded_joins = &joins;
        let mut both = empty_query(patch_fragment_probe(), 120.0);
        both.excluded_walls = &walls;
        both.excluded_joins = &joins;

        let (a, b, c) = (
            index.query(&wall_only).unwrap(),
            index.query(&join_only).unwrap(),
            index.query(&both).unwrap(),
        );
        // Neither set implies the other, so neither single query equals the
        // combined one, and the combined one is exactly their intersection.
        assert_ne!(a, c);
        assert_ne!(b, c);
        let expected: Vec<_> = a.iter().filter(|x| b.contains(x)).cloned().collect();
        assert_eq!(c, expected);
    }

    #[test]
    fn a_body_dragged_wall_loses_its_own_edge_and_axis_candidates() {
        // R6.1.3: "a body-dragged wall excludes its own edges and axis". W1 is
        // the only wall, so every boundary span belongs to it and NO edge
        // survives — this is identity, not a radius change.
        let (doc, level, wall) = f1();
        let index = SnapIndex::build(&doc, level);
        // Radius 150 admits the top face at (2000,100) AND the axis-line
        // projection at (2000,0), so both classes are present to lose.
        let mut req = empty_query(P(2000.0, 100.0), 150.0);
        let included = index.query(&req).unwrap();
        assert!(included.iter().any(|h| h.class == SnapClass::Edge));
        assert!(included.iter().any(|h| h.class == SnapClass::Axis));

        let excluded = [wall];
        req.excluded_walls = &excluded;
        let after = index.query(&req).unwrap();
        assert!(
            !after
                .iter()
                .any(|h| matches!(h.class, SnapClass::Edge | SnapClass::Axis)),
            "{after:#?}"
        );
    }

    #[test]
    fn a_non_finite_or_negative_radius_query_returns_nothing() {
        let (doc, level, _) = f1();
        let index = SnapIndex::build(&doc, level);
        for radius in [-1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                index.query(&empty_query(P(0.0, 0.0), radius)),
                Err(SnapError::InvalidInput)
            );
        }
        assert_eq!(
            index.query(&empty_query(P(f64::NAN, 0.0), 100.0)),
            Err(SnapError::InvalidInput)
        );
    }

    // ---- class-major ordering: a far join-node still beats a near grid hit ----

    #[test]
    fn class_order_outranks_distance() {
        let (doc, level, _, _, join) = f2();
        let index = SnapIndex::build(&doc, level);
        let mut req = empty_query(P(3999.0, 0.0), 250.0);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        let hits = index.query(&req).unwrap();
        assert_eq!(hits[0].class, SnapClass::JoinNode);
        assert_eq!(hits[0].entity, Some(join));
        assert!(
            hits.iter().any(|h| h.class == SnapClass::Grid),
            "a nearer grid point exists but does not win: {hits:#?}"
        );
    }

    // ---- R6.1.1 bound: on-viewport axis projections stay within the
    // visible rect expanded by the radius ----

    #[test]
    fn on_viewport_axis_projections_stay_within_the_expanded_visible_rect() {
        let (mut doc, level, _) = f1();
        doc.walls[0] = Wall::new(level, P(0.0, 0.0), P(3000.0, 0.0), 200.0, 2500.0);
        let index = SnapIndex::build(&doc, level);
        let visible = Aabb::from_corners(P(0.0, -1000.0), P(3500.0, 1000.0));
        let point = P(3150.0, 10.0);
        assert!(visible.contains(point));
        let mut req = empty_query(point, 120.0);
        req.visible_rect_mm = visible;
        let hits = index.query(&req).unwrap();
        let bound = visible.expanded(req.radius_mm);
        for hit in hits.iter().filter(|h| h.class == SnapClass::Axis) {
            assert!(bound.contains(hit.at), "R6.1.1 bound violated: {hit:?}");
        }
    }

    // ---- stable feature identity survives the closest point moving ----

    #[test]
    fn the_same_edge_feature_keeps_its_key_while_the_closest_point_slides() {
        let (doc, level, _) = f1();
        let index = SnapIndex::build(&doc, level);
        let a = index.query(&empty_query(P(1000.0, 100.0), 10.0)).unwrap();
        let b = index.query(&empty_query(P(1500.0, 100.0), 10.0)).unwrap();
        let (edge_a, edge_b) = (
            a.iter().find(|h| h.class == SnapClass::Edge).expect("a"),
            b.iter().find(|h| h.class == SnapClass::Edge).expect("b"),
        );
        assert_eq!(edge_a.feature, edge_b.feature, "same segment, same key");
        assert_ne!(edge_a.at, edge_b.at, "but the closest point moved");
    }
}
