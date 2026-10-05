//! Shared snapping query checkpoint for issue 0058.
//!
//! Uses active-level endpoints, canonical occupied-boundary segments, infinite
//! axes, a bounded grid window, and global/relative angle rays. Queries are
//! read-only with respect to the document. Point-distance comparison is exact
//! for the stored candidate coordinates; derived boundaries, projections,
//! rotation sums, and lattice multiplication remain rounded under issue 0012.
//! IEEE-754 division is correctly rounded for its operands, but that does not
//! make a composed geometric projection correctly rounded or exact. No error
//! bound or full R6 conformance is claimed for those constructions.
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

use crate::boolean::BoundaryRun;
use crate::distance::{compare_squared, within_radius};
use crate::spatial::{Aabb, SpatialIndex};
use crate::walls::{level_solids, occupied_region_with_sources};
use domowik_format::{Document, Point, Role};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashSet;
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

/// Exact ray equality for finite stored vectors, using filtered orientation
/// followed by a sign comparison on a nonzero component.
fn same_ray(u: (f64, f64), v: (f64, f64)) -> bool {
    if [u.0, u.1, v.0, v.1].iter().any(|x| !x.is_finite()) || u == (0.0, 0.0) || v == (0.0, 0.0) {
        return false;
    }
    crate::orientation(
        Point::new(0.0, 0.0),
        Point::new(u.0, u.1),
        Point::new(v.0, v.1),
    ) == crate::Orientation::Collinear
        && if u.0 != 0.0 {
            u.0.is_sign_positive() == v.0.is_sign_positive()
        } else {
            u.1.is_sign_positive() == v.1.is_sign_positive()
        }
}

/// Global rays plus rounded rational-vector rotations of the previous
/// direction. Degrees are display labels, not equality or ordering keys.
/// Orders finite nonzero stored rays in [-180, 180) without atan2 rounding.
fn ray_order(a: (f64, f64), b: (f64, f64)) -> Ordering {
    let half = |v: (f64, f64)| {
        if v.1 < 0.0 || (v.1 == 0.0 && v.0 < 0.0) {
            0
        } else {
            1
        }
    };
    half(a).cmp(&half(b)).then_with(|| {
        match crate::orientation(
            Point::new(0.0, 0.0),
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
        ) {
            crate::Orientation::CounterClockwise => Ordering::Less,
            crate::Orientation::Clockwise => Ordering::Greater,
            crate::Orientation::Collinear => Ordering::Equal,
        }
    })
}

fn angle_targets(previous_direction: Option<(f64, f64)>) -> Vec<((f64, f64), f64)> {
    let label = |dx: f64, dy: f64| normalize_angle_deg(dy.atan2(dx).to_degrees());
    let mut targets: Vec<((f64, f64), f64)> = GLOBAL_DIRECTIONS
        .iter()
        .map(|&(dx, dy)| ((dx, dy), label(dx, dy)))
        .collect();
    if let Some((dx, dy)) = previous_direction {
        if dx.is_finite() && dy.is_finite() && (dx != 0.0 || dy != 0.0) {
            let rotations = [(dx, dy), (dx - dy, dx + dy), (-dy, dx), (-dx - dy, dx - dy)];
            for (rx, ry) in rotations {
                if !rx.is_finite() || !ry.is_finite() || (rx == 0.0 && ry == 0.0) {
                    continue;
                }
                if targets.iter().any(|&(g, _)| same_ray(g, (rx, ry))) {
                    continue;
                }
                targets.push(((rx, ry), label(rx, ry)));
            }
        }
    }
    targets
}

/// A candidate carrying its full ranking data, before being narrowed to the
/// public [`SnapCandidate`].
#[derive(Debug, Clone, Copy)]
struct Ranked {
    class: SnapClass,
    entity: Option<Uuid>,
    at: Point,
    feature: SnapFeature,
    guide: Option<(Point, Point)>,
}

/// R6.3.1's level-3 canonical geometry key, evaluated only once level 1
/// (class) and level 2 (exact squared distance) have already tied — so both
/// sides of every match arm below are already the SAME [`SnapFeature`]
/// variant.
fn geometry_cmp(a: &Ranked, b: &Ranked) -> Ordering {
    let point_order = || {
        (a.at.x, a.at.y)
            .partial_cmp(&(b.at.x, b.at.y))
            .unwrap_or(Ordering::Equal)
    };
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
        (SnapFeature::Angle { direction: a, .. }, SnapFeature::Angle { direction: b, .. }) => {
            ray_order(a, b)
        }
        _ => unreachable!("class equality (level 1) already implies the same feature variant"),
    }
}

/// Class, exact distance to the stored candidate, geometry key, then identity.
/// Projected coordinates remain approximate as documented above.
fn rank(a: &Ranked, b: &Ranked, origin: Point) -> Ordering {
    a.class
        .cmp(&b.class)
        .then_with(|| {
            if a.at == b.at {
                Ordering::Equal
            } else {
                compare_squared(origin, a.at, b.at).expect("admission checked finite points")
            }
        })
        .then_with(|| geometry_cmp(a, b))
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

fn closest_point_on_segment(a: Point, b: Point, p: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return a;
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    // A clamped parameter means the answer IS an endpoint, so return the
    // stored one. Reconstructing it as `a + (b - a) * 1` rounds: for
    // `a.x = -1000`, `b.x = 0.3` the reconstruction lands on
    // 0.2999999999999545, which is a different point from the one the ring
    // actually stores. That drift is not cosmetic. It pushed an endpoint at
    // exactly the admission radius outside it, and — because the error
    // depends on the direction the edge is traversed — it decided ties
    // between two equidistant candidates before the R6.3.1 coordinate key
    // could, making the winner depend on ring orientation.
    if t.partial_cmp(&0.0) != Some(Ordering::Greater) {
        return a;
    }
    if t.partial_cmp(&1.0) != Some(Ordering::Less) {
        return b;
    }
    Point::new(a.x + dx * t, a.y + dy * t)
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
    component: usize,
    ring: usize,
    segment: usize,
    /// Sub-spans and their contributing ring tags, indexed into
    /// [`SnapIndex::rings`].
    run: BoundaryRun,
}

/// A ring that contributed occupied boundary, and what its geometry DEPENDS
/// on — which is not the same as what it is named after.
///
/// A wall rectangle depends on exactly its own wall. A join patch is the
/// convex hull of every member's cross-section (R3.2.3), so moving ANY member
/// reshapes the whole hull: its surviving boundary edges are the inter-member
/// hull edges, whose endpoints generally come from OTHER members than the one
/// being dragged. There is therefore no per-member split of a patch, and the
/// only sound rule is that a patch depends on all of its members at once.
#[derive(Debug, Clone)]
struct ContributingRing {
    /// Walls whose movement changes this ring's geometry.
    depends_on: Vec<Uuid>,
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

/// The point at parameter `t` along the ray from `origin` in `direction`.
///
/// Shared by the `axis` class (R6.1.1's perpendicular projection onto a wall's
/// axis line) and the `angle` class (R6.2.3's projection onto a chain target
/// ray), because they are the same construction and so carry the same error.
///
/// # Error bound
///
/// This is a CONSTRUCTION, not a predicate, so filter-then-escalate cannot
/// make it exact: the result must be a `Point`, and the exact projection of a
/// binary64 point onto a binary64 line is not generally a binary64 point. What
/// can be stated is how far the returned point lies from the EXACT axis line,
/// measured perpendicular to it.
///
/// Write `u = 2^-53`, `A = origin`, `D` for the exact direction the caller
/// meant, `d` for the rounded form actually passed in, and `q` for the
/// returned point.
/// Assume no overflow or underflow, which holds inside the ED-0100 bounds:
/// coordinates are at most `derivedCoordBoundMm = 1.1e7`, so every product
/// stays far inside the binary64 range, and a difference small enough to
/// underflow is exact by Sterbenz.
///
/// Two things move `q` off the exact line. An error in `t` is not one of them:
/// it slides `q` ALONG the direction and changes no perpendicular distance.
///
/// 1. **The direction is rounded**, by at most one rounding per component in
///    every caller here: an `axis` direction is `end - start`, and a relative
///    `angle` direction is one of `(dx-dy, dx+dy)`, `(-dy, dx)`,
///    `(-dx-dy, dx-dy)`. The global angle family is EXACT, being integer
///    pairs (R6.2.1), so it only improves the bound. Thus
///    `dx = Dx(1+e1)`, `dy = Dy(1+e2)` with `|e1|, |e2| <= u`, so
///    `cross(D, d) = Dx·Dy·(e2 - e1)` and
///    `|cross(D, d)| <= 2u·|Dx·Dy| <= u·|D|^2`, because `2|Dx·Dy| <= |D|^2`.
///    Scaled by `t` and divided by `|D|`, that contributes `|t|·u·|D|`.
/// 2. **The reconstruction rounds twice per coordinate.** `fl(A + fl(d·t))`
///    has componentwise error at most `u(|d·t| + |q|)`, and a perpendicular
///    deviation is at most the sum of the two componentwise errors, hence at
///    most `u·sqrt(2)·(|t|·|d| + |q|)`.
///
/// So the perpendicular deviation of `q` from the exact axis line is at most
///
/// ```text
/// u · ( |t|·|D|·(1 + sqrt(2)(1+u)) + sqrt(2)·|q| )
/// ```
///
/// At the coordinate bound, with `|t|·|D| <= 2C` and `|q| <= C` for
/// `C = 1.1e7 mm`, that is under `8e-9 mm` — about five orders of magnitude
/// BELOW the ratified topology quantum `WELD_MM = 1e-3 mm` (ED-0100,
/// ED-0103 N-2). The construction therefore cannot move a point across a
/// topology-grid cell, which is what the arrangement's identity relation
/// depends on. It is NOT exact, and R6.0.3's exact-real comparison still
/// applies to the STORED candidate coordinates rather than to this result.
///
/// The remaining derived class, `grid`, is simpler and bounded the same way: a
/// lattice point is `(i as f64) * s`, at most two roundings, so its absolute
/// error is at most `2u·|i·s| <= 2u·C`, again far under the quantum.
fn project_onto_ray(origin: Point, direction: (f64, f64), t: f64) -> Point {
    let (dx, dy) = direction;
    Point::new(origin.x + dx * t, origin.y + dy * t)
}

/// A reusable snapping index for one level: the committed-update cost of
/// BEHAVIOR-SPEC section 6, paid once per document change so
/// [`SnapIndex::query`] can be called on every pointer move against prebuilt
/// [`SpatialIndex`] broad-phases instead of rescanning the document.
#[derive(Debug, Clone)]
pub struct SnapIndex {
    endpoints: SpatialIndex<(Uuid, SnapClass, Point)>,
    edge_segments: Vec<EdgeSegment>,
    edge_index: SpatialIndex<usize>,
    /// Contributing rings, indexed by the boundary tag carried in
    /// [`EdgeSegment::run`]: wall rectangles first, then join patches.
    rings: Vec<ContributingRing>,
    /// Each wall's UNCLAMPED axis endpoints, for the actual projection.
    axis_lines: Vec<(Uuid, Point, Point)>,
    /// View-specific line bounds. Interior caching makes this index `!Sync`;
    /// serialize queries or give each worker its own index.
    axis_cache: RefCell<Option<(Aabb, SpatialIndex<usize>)>>,
}

impl SnapIndex {
    /// Builds the index for `level_id`. Only active-level walls and joins are
    /// included (R6.0.5: non-active levels are never snappable).
    pub fn build(doc: &Document, level_id: Uuid) -> Self {
        let mut joined: HashSet<(Uuid, Role)> = HashSet::new();
        let mut endpoint_items: Vec<(Aabb, (Uuid, SnapClass, Point))> = Vec::new();
        for join in doc.joins_on_level(level_id) {
            let mut node: Option<Point> = None;
            for m in &join.members {
                joined.insert((m.wall_id, m.role));
                if node.is_none() {
                    node = doc.wall(m.wall_id).map(|w| match m.role {
                        Role::Start => w.start(),
                        Role::End => w.end(),
                    });
                }
            }
            // R6.3.2: joined endpoints deduplicate to ONE join-node candidate,
            // never per-member endpoint candidates — enforced here at
            // generation, not by a later dedupe pass.
            if let Some(at) = node {
                endpoint_items.push((Aabb::point(at), (join.id, SnapClass::JoinNode, at)));
            }
        }
        for wall in doc.walls_on_level(level_id) {
            for role in [Role::Start, Role::End] {
                if joined.contains(&(wall.id, role)) {
                    continue;
                }
                let at = match role {
                    Role::Start => wall.start(),
                    Role::End => wall.end(),
                };
                endpoint_items.push((Aabb::point(at), (wall.id, SnapClass::FreeEndpoint, at)));
            }
        }
        let endpoints = SpatialIndex::build(endpoint_items);

        let solids = level_solids(doc, level_id);

        // Ring dependencies, in the tag order `occupied_region_with_sources`
        // assigns: one entry per wall, then one per join.
        let mut rings: Vec<ContributingRing> = solids
            .ids
            .iter()
            .map(|id| ContributingRing {
                depends_on: vec![*id],
            })
            .collect();
        for spec in &solids.joins {
            let mut depends_on: Vec<Uuid> = spec
                .members
                .iter()
                .filter_map(|m| solids.ids.get(m.wall).copied())
                .collect();
            depends_on.sort_unstable();
            depends_on.dedup();
            rings.push(ContributingRing { depends_on });
        }

        let region = occupied_region_with_sources(&solids.walls, &solids.joins);
        let mut edge_segments: Vec<EdgeSegment> = Vec::new();
        for (component, sourced) in region.components.iter().enumerate() {
            let poly = &sourced.polygon;
            let rings_of = std::iter::once((&poly.outer, &sourced.outer_runs))
                .chain(poly.holes.iter().zip(sourced.hole_runs.iter()));
            for (ring, (points, runs)) in rings_of.enumerate() {
                let n = points.len();
                for segment in 0..n {
                    edge_segments.push(EdgeSegment {
                        a: points[segment],
                        b: points[(segment + 1) % n],
                        component,
                        ring,
                        segment,
                        run: runs[segment].clone(),
                    });
                }
            }
        }
        let edge_index = SpatialIndex::build(
            edge_segments
                .iter()
                .enumerate()
                .filter_map(|(i, s)| Aabb::of(&[s.a, s.b]).map(|b| (b, i))),
        );

        let mut axis_lines: Vec<(Uuid, Point, Point)> = Vec::new();
        for (i, wall) in solids.walls.iter().enumerate() {
            let id = solids.ids[i];
            axis_lines.push((id, wall.start, wall.end));
        }

        Self {
            endpoints,
            edge_segments,
            edge_index,
            rings,
            axis_lines,
            axis_cache: RefCell::new(None),
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
        self.rings.get(tag).is_some_and(|ring| {
            ring.depends_on
                .iter()
                .any(|wall| req.excluded_walls.contains(wall))
        })
    }

    /// The closest point of the part of `seg` this gesture may still snap to,
    /// or `None` when every sub-span of it is excluded.
    ///
    /// A canonical edge can span several walls once collinear vertices are
    /// gone, so excluding it whole would delete a static neighbour's face and
    /// keeping it whole would offer the dragged wall its own. Neither is
    /// R6.1.3. The allowed set is the retained sub-spans, and the candidate is
    /// the closest point OF THAT SET — so a retained neighbour span still
    /// contributes its own nearest point instead of vanishing behind an
    /// excluded one.
    /// Which sub-span of `seg` contains `at`, by position ALONG the edge.
    ///
    /// Ordering, not proximity: a point sitting exactly on an interior break
    /// belongs to the span that ENDS there, so the answer is total and the two
    /// spans meeting at a break never both claim it.
    fn span_of(seg: &EdgeSegment, at: Point) -> usize {
        let (dx, dy) = (seg.b.x - seg.a.x, seg.b.y - seg.a.y);
        let dd = dx * dx + dy * dy;
        if !dd.is_finite() || dd.partial_cmp(&0.0) != Some(Ordering::Greater) {
            return 0;
        }
        let along = |p: Point| ((p.x - seg.a.x) * dx + (p.y - seg.a.y) * dy) / dd;
        let t = along(at);
        for (i, brk) in seg.run.breaks.iter().enumerate() {
            if t <= along(*brk) {
                return i;
            }
        }
        seg.run.breaks.len()
    }

    fn allowed_closest_point(
        &self,
        seg: &EdgeSegment,
        point: Point,
        req: &SnapQuery,
    ) -> Option<(Point, usize)> {
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
            // Nothing on this edge is excluded, so the arithmetic stays
            // bit-for-bit what an untagged index would have produced.
            let at = closest_point_on_segment(seg.a, seg.b, point);
            return Some((at, Self::span_of(seg, at)));
        }
        let mut best: Option<(Point, usize)> = None;
        let mut from = seg.a;
        for (i, drop) in excluded.iter().enumerate() {
            let to = seg.run.breaks.get(i).copied().unwrap_or(seg.b);
            if !drop {
                let at = closest_point_on_segment(from, to, point);
                let better = match best {
                    None => true,
                    Some((current, _)) => match compare_squared(point, at, current) {
                        Some(Ordering::Less) => true,
                        // Excluding a middle span can leave two retained spans
                        // exactly equidistant. Breaking that by span order
                        // would make the winner depend on which way the
                        // canonical ring happens to run, so it is broken by
                        // R6.3.1's own level-3 key instead: the smaller exact
                        // `(x, y)` of the closest point.
                        Some(Ordering::Equal) => (at.x, at.y) < (current.x, current.y),
                        _ => false,
                    },
                };
                if better {
                    best = Some((at, i));
                }
            }
            from = to;
        }
        best
    }

    /// Ranked candidates, or an explicit unsupported/invalid-input error.
    ///
    /// Radius boundaries are inclusive over stored candidate coordinates.
    /// Alt disable returns an empty result. Invalid inputs and oversized grid
    /// ranges are errors, not misses.
    pub fn query(&self, req: &SnapQuery) -> Result<Vec<SnapCandidate>, SnapError> {
        if req.alt_disabled {
            return Ok(Vec::new());
        }
        let (point, radius_mm) = (req.point, req.radius_mm);
        if !point.x.is_finite() || !point.y.is_finite() || !radius_mm.is_finite() || radius_mm < 0.0
        {
            return Err(SnapError::InvalidInput);
        }
        let view = req.visible_rect_mm;
        let bounds = view.union(Aabb::point(point)).expanded(radius_mm);
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
            || req
                .anchor
                .is_some_and(|a| !a.x.is_finite() || !a.y.is_finite())
        {
            return Err(SnapError::InvalidInput);
        }
        let mut raw: Vec<Ranked> = Vec::new();

        if req.object_enabled {
            for (id, class, at) in self
                .endpoints
                .query(&Aabb::point(point).expanded(radius_mm))
            {
                raw.push(Ranked {
                    class,
                    entity: Some(id),
                    at,
                    feature: SnapFeature::Entity { point: at },
                    guide: None,
                });
            }

            for i in self
                .edge_index
                .query(&Aabb::point(point).expanded(radius_mm))
            {
                let seg = &self.edge_segments[i];
                let Some((at, span)) = self.allowed_closest_point(seg, point, req) else {
                    continue;
                };
                let discriminator = if at == seg.a {
                    0
                } else if at == seg.b {
                    2
                } else {
                    1
                };
                raw.push(Ranked {
                    class: SnapClass::Edge,
                    entity: None,
                    at,
                    feature: SnapFeature::Edge {
                        component: seg.component,
                        ring: seg.ring,
                        segment: seg.segment,
                        span,
                        discriminator,
                    },
                    guide: Some((seg.a, seg.b)),
                });
            }

            // The camera can pan beyond the document's coordinate domain.
            // Include an outside captured pointer as well as the visible view.
            let mut cache = self.axis_cache.borrow_mut();
            if cache.as_ref().is_none_or(|(old, _)| *old != bounds) {
                let index = SpatialIndex::build(self.axis_lines.iter().enumerate().filter_map(
                    |(i, (_, start, end))| {
                        clip_line_to_box(*start, *end, bounds)
                            .and_then(|(a, b)| Aabb::of(&[a, b]))
                            .map(|b| (b, i))
                    },
                ));
                *cache = Some((bounds, index));
            }
            let candidates = cache
                .as_ref()
                .expect("cache initialized")
                .1
                .query(&Aabb::point(point).expanded(radius_mm));
            drop(cache);
            for i in candidates {
                let (id, start, end) = self.axis_lines[i];
                let (dx, dy) = (end.x - start.x, end.y - start.y);
                let dd = dx * dx + dy * dy;
                if dd == 0.0 || !dd.is_finite() {
                    continue;
                }
                let (vx, vy) = (point.x - start.x, point.y - start.y);
                let t = (vx * dx + vy * dy) / dd;
                if !t.is_finite() {
                    continue;
                }
                let at = project_onto_ray(start, (dx, dy), t);
                raw.push(Ranked {
                    class: SnapClass::Axis,
                    entity: Some(id),
                    at,
                    feature: SnapFeature::Axis {
                        on_segment: (0.0..=1.0).contains(&t),
                    },
                    guide: clip_line_to_box(start, end, bounds),
                });
            }
        }

        if req.grid_enabled {
            if let Some(s) = req.grid_spacing_mm {
                if s.is_finite() && s > 0.0 {
                    let i0 = grid_index_bound(((point.x - radius_mm) / s).floor());
                    let i1 = grid_index_bound(((point.x + radius_mm) / s).ceil());
                    let j0 = grid_index_bound(((point.y - radius_mm) / s).floor());
                    let j1 = grid_index_bound(((point.y + radius_mm) / s).ceil());
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
                                let at = Point::new(i as f64 * s, j as f64 * s);
                                raw.push(Ranked {
                                    class: SnapClass::Grid,
                                    entity: None,
                                    at,
                                    feature: SnapFeature::Grid { i, j },
                                    guide: None,
                                });
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
                for ((dx, dy), degrees) in angle_targets(req.previous_direction) {
                    let (vx, vy) = (point.x - anchor.x, point.y - anchor.y);
                    let num = vx * dx + vy * dy;
                    // R6.2.2: strictly ahead of the anchor, never at or behind it.
                    if num.partial_cmp(&0.0) != Some(Ordering::Greater) {
                        continue;
                    }
                    let dd = dx * dx + dy * dy;
                    let t = num / dd;
                    if !t.is_finite() {
                        continue;
                    }
                    let at = project_onto_ray(anchor, (dx, dy), t);
                    raw.push(Ranked {
                        class: SnapClass::Angle,
                        entity: None,
                        at,
                        feature: SnapFeature::Angle {
                            direction: (dx, dy),
                            degrees,
                        },
                        guide: clip_line_to_box(
                            anchor,
                            Point::new(anchor.x + dx, anchor.y + dy),
                            bounds,
                        ),
                    });
                }
            }
        }

        let mut kept: Vec<Ranked> = raw
            .into_iter()
            .filter(|c| {
                match (c.class, c.entity) {
                    // `join-node` candidates are keyed by JOIN id, every other
                    // entity-bearing class by WALL id; they are different
                    // identity spaces and must not be tested against the same
                    // list. Node exclusion stays the caller's to state,
                    // because only the gesture knows whether the node moves
                    // (R3.4.2) or is merely its anchor.
                    (SnapClass::JoinNode, Some(id)) if req.excluded_joins.contains(&id) => {
                        return false
                    }
                    (SnapClass::JoinNode, _) => {}
                    (_, Some(id)) if req.excluded_walls.contains(&id) => return false,
                    _ => {}
                }
                // R6.1.3: anchor coincidence filters EVERY class, never
                // merely deprioritized. This also covers "every acceptance
                // that would produce a zero-length segment" — a candidate at
                // the anchor is exactly that acceptance.
                if let Some(anchor) = req.anchor {
                    if c.at == anchor {
                        return false;
                    }
                }
                matches!(within_radius(point, c.at, radius_mm), Some(true))
            })
            .collect();

        kept.sort_by(|a, b| rank(a, b, point));
        kept.dedup_by(|a, b| {
            a.class == b.class && a.entity == b.entity && rank(a, b, point) == Ordering::Equal
        });

        Ok(kept
            .into_iter()
            .map(|r| SnapCandidate {
                class: r.class,
                entity: r.entity,
                at: r.at,
                feature: r.feature,
                guide: r.guide,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{JoinMember, Wall, WallJoin};

    const P: fn(f64, f64) -> Point = Point::new;

    #[test]
    fn ray_equality_and_order_do_not_underflow_or_round_angle_labels() {
        let tiny = f64::from_bits(1);
        assert!(same_ray((tiny, tiny), (tiny * 2.0, tiny * 2.0)));
        assert!(!same_ray((tiny, tiny), (tiny, tiny * 2.0)));
        assert_eq!(ray_order((tiny, tiny), (tiny, tiny * 2.0)), Ordering::Less);
        for pair in GLOBAL_DIRECTIONS.windows(2) {
            assert_eq!(ray_order(pair[0], pair[1]), Ordering::Less);
        }
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

    fn empty_query(point: Point, radius_mm: f64) -> SnapQuery<'static> {
        SnapQuery {
            point,
            radius_mm,
            visible_rect_mm: Aabb::from_corners(P(-1e7, -1e7), P(1e7, 1e7)),
            anchor: None,
            previous_direction: None,
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

    /// Codex (gpt-5.6-sol) adversarial round, 2026-09-12.
    ///
    /// The round reported that a sub-span's closest point was RECONSTRUCTED as
    /// `a + (b - a) * 1` rather than returned as the stored `b`. That is real:
    /// over segments inside the ED-0100 coordinate bound the reconstruction
    /// differs from the stored endpoint for about one in six random segments.
    /// Querying exactly AT a boundary vertex must therefore answer with that
    /// vertex and not with a rebuilt neighbour of it.
    /// The [`project_onto_ray`] error bound, checked EXACTLY rather than in the
    /// arithmetic it bounds.
    ///
    /// The perpendicular deviation of `q` from the exact line through `A` and
    /// `B` is `|cross(B-A, q-A)| / |B-A|`. Both the cross product and the
    /// squared length are exact in `Dyadic`, and comparing `cross^2` against
    /// `bound^2 · |B-A|^2` removes the division and the square root, so
    /// nothing in the check itself rounds.
    #[test]
    fn the_ray_projection_stays_within_its_derived_perpendicular_bound() {
        use crate::dyadic::Dyadic;

        const U: f64 = f64::EPSILON / 2.0;
        /// ED-0100's `derivedCoordBoundMm`.
        const C: f64 = 1.1e7;
        /// Shrinks the bound, so the check can be shown capable of failing.
        const SHRINK: f64 = 1.0;

        let exact = Dyadic::from_f64;
        let sub = |x: f64, y: f64| exact(x).sub(&exact(y));

        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 11) as f64 / (1u64 << 53) as f64) * 2.0 * C - C
        };

        let mut checked = 0usize;
        for case in 0..20_000 {
            let (ax, ay) = (next(), next());
            // Axis-aligned, diagonal, and short-relative-to-offset shapes,
            // which is where a projection behaves worst.
            let (bx, by) = match case % 4 {
                0 => (next(), next()),
                1 => (next(), ay),
                2 => (ax, next()),
                _ => (ax + next() * 1e-9, ay + next() * 1e-9),
            };
            let (px, py) = (next(), next());
            let start = P(ax, ay);

            let (dx, dy) = (bx - ax, by - ay);
            let dd = dx * dx + dy * dy;
            if dd == 0.0 || !dd.is_finite() {
                continue;
            }
            let t = ((px - ax) * dx + (py - ay) * dy) / dd;
            if !t.is_finite() {
                continue;
            }
            let q = project_onto_ray(start, (dx, dy), t);
            if !q.x.is_finite() || !q.y.is_finite() {
                continue;
            }

            // The derived bound for this case. `hypot` is within an ulp of the
            // exact length, and the result is inflated generously so rounding
            // INSIDE the bound cannot let a real violation past.
            let bound = U
                * (t.abs() * dx.hypot(dy) * (1.0 + std::f64::consts::SQRT_2 * (1.0 + U))
                    + std::f64::consts::SQRT_2 * q.x.hypot(q.y))
                * (1.0 + 1024.0 * U)
                / SHRINK;

            let cross = sub(bx, ax)
                .mul(&sub(q.y, ay))
                .sub(&sub(by, ay).mul(&sub(q.x, ax)));
            let len2 = sub(bx, ax).square().add(&sub(by, ay).square());
            assert!(
                cross.square() <= exact(bound).square().mul(&len2),
                "case {case}: projection left its bound for \
                 a=({ax},{ay}) b=({bx},{by}) p=({px},{py})"
            );
            checked += 1;
        }
        // A filter that quietly skipped everything would pass vacuously.
        assert!(checked > 15_000, "only {checked} cases reached the bound");
    }

    /// Codex adversarial round finding 2, which could NOT be fixed when it was
    /// first reported: the bottom and top faces of one rectangle resolved an
    /// equidistant pair to OPPOSITE sides, because the arrangement stored
    /// `1.9` on one face and `1.8999999999999773` on the other.
    ///
    /// The cause was a constructed vertex, not the comparator, and it is now
    /// fixed at its source — a cut that lands on an input corner reproduces
    /// that corner. With both faces carrying the same stored coordinates the
    /// pair is a genuine tie again and R6.3.1's key decides it, the same way
    /// whichever direction the ring runs.
    #[test]
    fn equidistant_retained_spans_ignore_the_faces_traversal_direction() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        for (x0, x1) in [(-1000.0, 1.1), (1.1, 1.9), (1.9, 1000.0)] {
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
        assert_eq!(winner_x(-1.0), 1.1, "bottom face, traversed left to right");
        assert_eq!(winner_x(1.0), 1.1, "top face, traversed right to left");
    }

    /// The same defect seen from the admission side: a retained span's own
    /// endpoint sitting exactly at the radius must be admitted, which needs
    /// the stored break to BE the input corner rather than an ulp below it.
    #[test]
    fn a_retained_spans_endpoint_is_admitted_at_exactly_the_radius() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(-1000.0, 0.0), P(0.3, 0.0), 2.0, 2500.0));
        doc.walls
            .push(Wall::new(level, P(0.3, 0.0), P(1000.0, 0.0), 2.0, 2500.0));
        let dragged = doc.walls[1].id;
        assert_eq!(doc.validate(), vec![]);
        let index = SnapIndex::build(&doc, level);

        let point = P(0.4, -1.0);
        let corner = P(0.3, -1.0);
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
