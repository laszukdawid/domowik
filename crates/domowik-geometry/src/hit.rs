//! Hit testing and marquee selection.
//!
//! Implements the wall/join hit classes and ordering keys of BEHAVIOR-SPEC
//! R7.1.3 and R7.3.1 marquee queries. [`HitIndex`] keeps its features in
//! [`crate::aabb_tree`]s and is SYNCED per document revision rather than
//! rebuilt: each wall, opening and point feature carries a stamp of its
//! inputs, and only changed stamps rebuild (issue 0012).
//!
//! Wall and opening regions are the IDEAL rectangles of R3.0.2/R4.0.3 (axis
//! and thickness, never the rounded drawing polygon), and every admission and
//! ranking decision — squared distance, closest point, the canonical ring tie
//! key, marquee intersection and its min-corner key — is exact over the
//! supplied inputs via [`crate::witness`] (ED-0112). Placeholders keep §9.6's
//! binary64 boxes, compared exactly as supplied. `Hit::closest` is a binary64
//! point within the certified enclosure of the exact closest point.
//!
//! # The ordering is total, and that is the whole point
//!
//! R7.2.2 makes repeated clicks CYCLE through the candidates at a point, and
//! R6.3.3 forbids delivery order, map iteration order, or frame history from
//! being observable in the result. A ranking that leaves two candidates
//! comparing equal would cycle in whatever order the input happened to arrive
//! in — so every level of the comparison below is present, including the ones
//! that almost never decide anything.

use crate::aabb_tree::KeyedTree;
use crate::spatial::{Aabb, SpatialIndex};
use crate::walls::WallSolid;
use crate::witness::{
    axis_box, rect_bounds, rect_meets_box, Along, BboxMin, PreparedRect, Radius, Shape, Witness,
};
use domowik_format::{AssetInstance, Document, Point};
use std::cmp::Ordering;
use std::collections::HashMap;
use uuid::Uuid;

/// Hit classes in R7.1.3 precedence order.
///
/// The derived `Ord` IS the precedence: `JoinNode` is declared first and
/// therefore compares least, which is what "top-ranked candidate" means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HitClass {
    /// A join node handle.
    JoinNode,
    /// A wall endpoint belonging to no join.
    FreeEndpoint,
    /// A door or window aperture on a wall.
    Opening,
    /// A wall's OWN occupied rectangle — not the cluster union.
    Region,
    /// A DEGRADED asset instance's deterministic placeholder box (§9.6).
    ///
    /// Ranked last, as R7.1.3 declares it. Candidacy is never inferred here:
    /// the caller supplies the instances a resolver decided are degraded.
    PlaceholderBox,
    /// A RESOLVED asset instance's conservative footprint (issue 0111).
    ///
    /// A MARQUEE candidate class only, not an R7.1.3 point-hit class: resolved
    /// instances are point-hit by the asset store's own footprint query, which
    /// the geometry crate never sees. Declared last so an exact min-corner tie
    /// with a geometry class ranks the geometry class first, matching the
    /// click path, where asset-store hits follow every geometry hit.
    AssetFootprint,
}

/// One hit candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub class: HitClass,
    /// The entity this candidate selects: a join, wall, or opening according
    /// to the hit class.
    pub entity: Uuid,
    /// The point on the candidate closest to the query — the query point
    /// itself when it is inside a region, giving exact distance zero.
    pub closest: Point,
    /// Squared distance from the query point, in plan millimetres squared.
    ///
    /// A binary64 display value: within the certified enclosure of the exact
    /// one, or — when float evaluation under/overflowed — the squared distance
    /// to [`Hit::closest`]. Never negative. Ranking never uses it: every class
    /// ranks by the exact squared distance (ED-0112).
    pub distance_squared: f64,
}

/// The side of the fallback placeholder box, in millimetres (§9.6).
pub const PLACEHOLDER_FALLBACK_MM: f64 = 1000.0;

/// The two §9.6 placeholder branches, in plan millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaceholderGeometry {
    /// Fixed 1000 mm square; ignores instance scale and rotation (ED-0106).
    Fallback(Aabb),
    /// Axis-aligned bounds of the centered, scaled and rotated width/depth
    /// rectangle. Height is vertical and does not affect the plan footprint.
    Declared(Aabb),
}

/// Derive §9.6 interaction bounds from `(width, height, depth)` in mm.
///
/// Declared dimensions must be finite, positive and at most 10^6 mm. The
/// transform must have finite translation/rotation and positive finite scale.
/// Construction uses deterministic binary64 trigonometry, not exact-real
/// arithmetic; returned bounds must fit `DERIVED_COORD_BOUND_MM`. Invalid or
/// overflowing geometry returns `None`. No tolerance or coordinate snapping.
pub fn placeholder_geometry(
    instance: &AssetInstance,
    declared_mm: Option<(f64, f64, f64)>,
) -> Option<PlaceholderGeometry> {
    let Some((width, height, depth)) = declared_mm else {
        return placeholder_box(instance).map(PlaceholderGeometry::Fallback);
    };
    let t = &instance.transform;
    if [width, height, depth]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.0 || *v > 1_000_000.0)
        || !t.scale_uniform.is_finite()
        || t.scale_uniform <= 0.0
        || !t.rotation_deg.is_finite()
        || t.translation_mm.iter().any(|v| !v.is_finite())
    {
        return None;
    }
    let (sin, cos) = t.rotation_deg.rem_euclid(360.0).to_radians().sin_cos();
    let half_x = (cos.abs() * width + sin.abs() * depth) * t.scale_uniform / 2.0;
    let half_y = (sin.abs() * width + cos.abs() * depth) * t.scale_uniform / 2.0;
    let bounds = Aabb {
        min_x: t.translation_mm[0] - half_x,
        min_y: t.translation_mm[1] - half_y,
        max_x: t.translation_mm[0] + half_x,
        max_y: t.translation_mm[1] + half_y,
    };
    [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= domowik_format::DERIVED_COORD_BOUND_MM)
        .then_some(PlaceholderGeometry::Declared(bounds))
}

/// Fixed 1000 mm axis-aligned square centered on the instance translation.
/// Scale and rotation are ignored (ED-0106). Returns `None` for non-finite
/// origins; valid document origins keep the box within derived bounds.
pub fn placeholder_box(instance: &AssetInstance) -> Option<Aabb> {
    let origin = instance.origin();
    if !origin.x.is_finite() || !origin.y.is_finite() {
        return None;
    }
    let half = PLACEHOLDER_FALLBACK_MM / 2.0;
    Some(Aabb {
        min_x: origin.x - half,
        min_y: origin.y - half,
        max_x: origin.x + half,
        max_y: origin.y + half,
    })
}

/// One degraded instance, with the box a resolver's verdict gives it.
///
/// # Why the caller supplies this rather than the query deriving it
///
/// R7.1.3 admits DEGRADED placeholder instances only, and §9.6 decides
/// degradation from blob availability, digest agreement, decodability, media
/// type and retrieval policy. None of that is geometry, and a global rule such
/// as "every instance is degraded while no resolver exists" was refused
/// deliberately: content that is currently missing can yield a degraded result,
/// while a REQUIRED asset may instead force read-only mode, which deletion has
/// to respect. Candidacy is therefore a per-instance verdict, carried in, and
/// `domowik-geometry` gains no dependency on the asset store to obtain it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placeholder {
    /// The instance this box selects.
    pub instance: Uuid,
    /// Its §9.6 bounds, normally from [`placeholder_box`].
    pub bounds: Aabb,
}

/// One RESOLVED asset instance's marquee candidacy (issue 0111).
///
/// The caller supplies candidacy and bounds for the same reason
/// [`Placeholder`] does: resolution is an asset-store verdict, not geometry,
/// and the footprint rectangle is §10.1/§10.4 editor semantics, including the
/// conservative projection a tilt produces. `bounds` are plan millimetres, the
/// axis-aligned envelope of the footprint the asset store point-hits by.
/// Supplied bounds must be finite, ordered and within the caller's supported
/// derived-coordinate range. Intersection is exact over these stored binary64
/// bounds, with no tolerance; constructing the envelope is the caller's job.
///
/// Marquee-only: [`HitIndex::hit_test`] never sees resolved instances, so
/// adding one here grants no point-hit candidacy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetFootprint {
    /// The instance this box selects.
    pub instance: Uuid,
    /// Its conservative footprint bounds, in plan millimetres.
    pub bounds: Aabb,
}

/// Every candidate within `radius_mm` of `point` on `level_id`, ranked.
///
/// The first element is the top-ranked candidate a click selects (R7.2.2).
///
/// The rest are RAW FEATURES, not a click list. One entity may appear more
/// than once — under different classes, or as two endpoints at different
/// points — because R6.3.2 keeps candidates apart whenever their full keys
/// differ. R7.2.2a's first-occurrence projection to an ENTITY list, and
/// therefore what repeated clicks actually cycle through, belongs to the
/// caller and is deliberately not applied here: marquee and snap consume this
/// same feature list and need the features, not the projection.
///
/// Only active-level entities are eligible: R7.1.3 says non-active levels are
/// never hit, cycled, or marquee-selected, so `level_id` is a filter and not a
/// hint.
///
/// `radius_mm` is the logical-pixel radius of R7.1.3 (12 for mouse and stylus,
/// 24 for touch) already converted to plan millimetres through the §1.2
/// inverse transform. That conversion is the viewport's job and is deliberately
/// not done here — this module has no view transform and must not invent one.
pub fn hit_test(doc: &Document, level_id: Uuid, point: Point, radius_mm: f64) -> Vec<Hit> {
    hit_test_with(doc, level_id, point, radius_mm, &[])
}

/// [`hit_test`], plus the degraded instances a resolver decided are
/// `placeholder-box` candidates on this level (R7.1.3, §9.6).
pub fn hit_test_with(
    doc: &Document,
    level_id: Uuid,
    point: Point,
    radius_mm: f64,
    placeholders: &[Placeholder],
) -> Vec<Hit> {
    HitIndex::build_with(doc, level_id, placeholders).hit_test(point, radius_mm)
}

/// A reusable hit-testing index for one level.
///
/// # Committed updates versus preview queries
///
/// Issue 0012 requires these be distinguished, and this type is the
/// distinction. Building the index is the COMMITTED-UPDATE cost, paid once
/// when the document changes. A PREVIEW query — the proximity test a drag runs
/// on every pointer move — reuses it and pays nothing for construction.
///
/// The free [`hit_test`] builds and discards one. Reuse this index when the
/// document is unchanged, for clicks as well as drags; construction is much
/// more expensive than a query. `benches/spatial.rs` measures both separately.
#[derive(Debug, Clone)]
pub struct HitIndex {
    level: Uuid,
    /// Each wall's IDEAL butt-capped rectangle (R3.0.2) — axis and
    /// thickness, never the rounded polygon — stamped by its solid. A wall
    /// whose drawn polygon collapses in binary64 still has its ideal shape
    /// (issue 0012: Codex review counterexample D).
    regions: KeyedTree<Uuid, WallSolid, (Uuid, PreparedRect)>,
    /// Each opening's ideal band over its host's full thickness, stamped by
    /// what it is computed from: its own position and width and its host's
    /// solid.
    openings: KeyedTree<Uuid, OpeningStamp, (Uuid, PreparedRect)>,
    endpoints: KeyedTree<EndpointKey, Point, (Uuid, HitClass, Point)>,
    /// The caller's degraded instances for this level. Empty unless the query
    /// was built with a resolution snapshot. Few, and replaced wholesale.
    placeholders: Vec<Placeholder>,
    placeholder_index: SpatialIndex<usize>,
}

/// The inputs of an opening band: `u`, width, host start, end, thickness.
type OpeningStamp = (f64, f64, Point, Point, f64);

/// A point feature's identity: a join's node, or one end of a wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EndpointKey {
    Join(Uuid),
    /// The wall, and whether this is its END (else its start).
    Wall(Uuid, bool),
}

impl EndpointKey {
    pub(crate) fn entity(self) -> Uuid {
        match self {
            EndpointKey::Join(id) | EndpointKey::Wall(id, _) => id,
        }
    }
}

/// Every point feature of `level_id`: one per join, at its node (the first
/// member whose wall resolves), and one per wall end no join claims
/// (R6.3.2: joined endpoints deduplicate to the join node, enforced here at
/// generation, not by a later dedupe pass). Shared by hit testing and
/// snapping, which rank the same features under different classes.
pub(crate) fn point_features(doc: &Document, level_id: Uuid) -> Vec<(EndpointKey, Point)> {
    let mut out = Vec::new();
    // A set, not a list: membership is asked once per endpoint, and a linear
    // scan there makes the whole build quadratic in the number of joins.
    let mut joined: crate::overlay::FastMap<(Uuid, domowik_format::Role), ()> = Default::default();
    let joins = doc.joins_on_level(level_id);
    // Members resolve through a map for the same reason: `Document::wall`
    // is a linear scan.
    let walls: crate::overlay::FastMap<Uuid, &domowik_format::Wall> = if joins.is_empty() {
        Default::default()
    } else {
        doc.walls_on_level(level_id).map(|w| (w.id, w)).collect()
    };
    for join in joins {
        // ED-0112: the node is the centre of the topology cell its members'
        // endpoints share (validation requires one cell, R3.3.4/ED-0110), so
        // it does not depend on member order. A malformed join spanning
        // cells takes the least of them, for the same reason.
        let mut node: Option<crate::overlay::Cell> = None;
        for m in &join.members {
            joined.insert((m.wall_id, m.role), ());
            if let Some(w) = walls.get(&m.wall_id) {
                let at = match m.role {
                    domowik_format::Role::Start => w.start(),
                    domowik_format::Role::End => w.end(),
                };
                let cell = crate::overlay::Cell::of(at);
                node = Some(node.map_or(cell, |n| n.min(cell)));
            }
        }
        if let Some(cell) = node {
            out.push((EndpointKey::Join(join.id), cell.center()));
        }
    }
    for wall in doc.walls_on_level(level_id) {
        for role in [domowik_format::Role::Start, domowik_format::Role::End] {
            if joined.contains_key(&(wall.id, role)) {
                continue;
            }
            let at = match role {
                domowik_format::Role::Start => wall.start(),
                domowik_format::Role::End => wall.end(),
            };
            out.push((
                EndpointKey::Wall(wall.id, role == domowik_format::Role::End),
                at,
            ));
        }
    }
    out
}

/// The class-specific tail of the R7.1.3 key, kept beside each candidate
/// rather than inside [`Hit`]: a region's is its canonical ring, which is
/// expensive to carry, and a placeholder's is its min corner.
#[derive(Debug, Clone, Copy)]
enum Tail<'a> {
    Region(&'a PreparedRect),
    Opening(&'a PreparedRect),
    Placeholder(Aabb),
    None,
}

/// A candidate during ranking: the public [`Hit`], its exact witness (none
/// for a placeholder, whose R7.1.3 key has no distance term), and its
/// class-specific tail key.
struct Cand<'a> {
    hit: Hit,
    witness: Option<Witness>,
    /// [`Witness::exact_key`], cached: ranking compares these floats first.
    key: Option<[f64; 3]>,
    tail: Tail<'a>,
}

/// R7.1.3's total order: class; then for placeholders min corner and id;
/// for every other class EXACT squared distance, EXACT closest `(x, y)`, the
/// ideal ring tuple (regions and openings), then id.
fn rank(a: &Cand, b: &Cand) -> Ordering {
    let class = a.hit.class.cmp(&b.hit.class);
    if class != Ordering::Equal {
        return class;
    }
    if let (Tail::Placeholder(x), Tail::Placeholder(y)) = (&a.tail, &b.tail) {
        return (x.min_x, x.min_y)
            .partial_cmp(&(y.min_x, y.min_y))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.hit.entity.cmp(&b.hit.entity));
    }
    let distance_then_point = match (&a.key, &b.key) {
        // Exact singletons: float order IS the exact order.
        (Some(x), Some(y)) => x
            .iter()
            .zip(y)
            .map(|(p, q)| p.partial_cmp(q).unwrap_or(Ordering::Equal))
            .find(|o| *o != Ordering::Equal)
            .unwrap_or(Ordering::Equal),
        _ => {
            let (wa, wb) = (
                a.witness.as_ref().expect("non-placeholder has a witness"),
                b.witness.as_ref().expect("non-placeholder has a witness"),
            );
            wa.cmp_dist(wb).then_with(|| wa.cmp_xy(wb))
        }
    };
    distance_then_point
        .then_with(|| match (&a.tail, &b.tail) {
            (Tail::Region(x), Tail::Region(y)) | (Tail::Opening(x), Tail::Opening(y)) => {
                x.cmp_ring(y)
            }
            _ => Ordering::Equal,
        })
        .then_with(|| a.hit.entity.cmp(&b.hit.entity))
}

impl HitIndex {
    /// Builds the index for `level_id`.
    ///
    /// Only active-level entities are included: R7.1.3 says non-active levels
    /// are never hit, cycled, or marquee-selected, so this is a filter and not
    /// a hint.
    pub fn build(doc: &Document, level_id: Uuid) -> Self {
        Self::build_with(doc, level_id, &[])
    }

    /// Builds the index for `level_id`, including the degraded instances a
    /// resolver decided are `placeholder-box` candidates (R7.1.3, §9.6).
    ///
    /// `placeholders` is the caller's own resolution snapshot for THIS level.
    /// Candidacy is never inferred here: see [`Placeholder`].
    pub fn build_with(doc: &Document, level_id: Uuid, placeholders: &[Placeholder]) -> Self {
        let mut index = Self::new(level_id);
        index.sync(doc, placeholders);
        index
    }

    /// An empty index for `level_id`.
    pub fn new(level_id: Uuid) -> Self {
        Self {
            level: level_id,
            regions: KeyedTree::default(),
            openings: KeyedTree::default(),
            endpoints: KeyedTree::default(),
            placeholders: Vec::new(),
            placeholder_index: SpatialIndex::build(Vec::new()),
        }
    }

    /// The level this index covers.
    pub fn level(&self) -> Uuid {
        self.level
    }

    /// Brings the index to `doc` and `placeholders` (issue 0012): afterwards
    /// every query answers exactly as [`Self::build_with`] of the same
    /// inputs would. Returns how many entries changed.
    ///
    /// Cost: one stamp comparison per wall, opening and point feature of the
    /// level; geometry and tree work only for what changed.
    pub fn sync(&mut self, doc: &Document, placeholders: &[Placeholder]) -> usize {
        let level = self.level;
        let mut changed = self.regions.sync(
            doc.walls_on_level(level)
                .map(|w| (w.id, WallSolid::new(w.start(), w.end(), w.thickness_mm))),
            |&id, solid| {
                // A wall needs a direction and a positive thickness.
                let (dx, dy) = (solid.end.x - solid.start.x, solid.end.y - solid.start.y);
                let t = solid.thickness_mm;
                if !(dx.is_finite() && dy.is_finite() && (dx != 0.0 || dy != 0.0))
                    || !(t.is_finite() && t > 0.0)
                {
                    return None;
                }
                let shape = Shape::Rect {
                    a: solid.start,
                    b: solid.end,
                    thickness: solid.thickness_mm,
                    along: Along::Wall,
                };
                Some((rect_bounds(&shape)?, (id, PreparedRect::of(shape))))
            },
        );

        // The same aperture rectangle rendered by the bridge: the host axis
        // interval occupied by the opening, at the host's full thickness.
        let host_walls: HashMap<_, _> = if doc.openings.is_empty() {
            HashMap::new()
        } else {
            doc.walls_on_level(level)
                .map(|wall| (wall.id, wall))
                .collect()
        };
        changed += self.openings.sync(
            doc.openings.iter().filter_map(|opening| {
                let wall = host_walls.get(&opening.host_wall_id)?;
                Some((
                    opening.id,
                    (
                        opening.u,
                        opening.width_mm,
                        wall.start(),
                        wall.end(),
                        wall.thickness_mm,
                    ),
                ))
            }),
            |&id, &(u, width_mm, start, end, thickness)| {
                // The band `width` wide centred at `u` along the host axis,
                // over the host's full thickness (R4.0.3, R7.1.3), built from
                // the stored inputs — not from rounded aperture endpoints.
                let valid = |v: f64| v.is_finite() && v > 0.0;
                if start == end || !valid(width_mm) || !valid(thickness) || !u.is_finite() {
                    return None;
                }
                let shape = Shape::Rect {
                    a: start,
                    b: end,
                    thickness,
                    along: Along::Opening { u, width: width_mm },
                };
                Some((rect_bounds(&shape)?, (id, PreparedRect::of(shape))))
            },
        );

        changed += self
            .endpoints
            .sync(point_features(doc, level), |&key, &at| {
                let class = match key {
                    EndpointKey::Join(_) => HitClass::JoinNode,
                    EndpointKey::Wall(..) => HitClass::FreeEndpoint,
                };
                Some((Aabb::point(at), (key.entity(), class, at)))
            });

        if self.placeholders != placeholders {
            changed += self.placeholders.len() + placeholders.len();
            self.placeholders = placeholders.to_vec();
            self.placeholder_index = SpatialIndex::build(
                self.placeholders
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (p.bounds, i)),
            );
        }
        changed
    }

    /// Checks both trees and their key maps; panics on violation. For tests
    /// and debugging.
    pub fn validate(&self) {
        self.regions.validate();
        self.openings.validate();
        self.endpoints.validate();
    }

    /// Every candidate within `radius_mm` of `point`, ranked.
    ///
    /// The first element is the top-ranked candidate a click selects (R7.2.2).
    /// The rest are RAW FEATURES: one entity may appear more than once, and
    /// R7.2.2a's first-occurrence projection to an entity list — what repeated
    /// clicks cycle through — is the caller's, not this method's. See
    /// [`hit_test`].
    ///
    /// `radius_mm` is the logical-pixel radius of R7.1.3 (12 for mouse and
    /// stylus, 24 for touch) already converted to plan millimetres through the
    /// §1.2 inverse transform. That conversion is the viewport's job and is
    /// deliberately not done here — this module has no view transform and must
    /// not invent one.
    ///
    /// Non-finite query coordinates, non-finite radii and negative radii return
    /// no hits. Radius boundaries are inclusive, and admission and ranking of
    /// every class are exact over the supplied inputs (ED-0112).
    pub fn hit_test(&self, point: Point, radius_mm: f64) -> Vec<Hit> {
        if !radius_mm.is_finite() || radius_mm < 0.0 {
            return Vec::new();
        }
        self.hit_test_radius(point, &Radius::mm(radius_mm))
    }

    /// [`Self::hit_test`] with the radius as logical pixels `px` over the
    /// view scale `zoom` (logical pixels per millimetre): admission is
    /// exactly `distance ≤ px / zoom`, never a rounded quotient (ED-0112).
    /// Invalid radii return no hits.
    pub fn hit_test_screen(&self, point: Point, px: f64, zoom: f64) -> Vec<Hit> {
        if !px.is_finite() || px < 0.0 || !zoom.is_finite() || zoom <= 0.0 {
            return Vec::new();
        }
        self.hit_test_radius(point, &Radius::screen(px, zoom))
    }

    fn hit_test_radius(&self, point: Point, radius: &Radius) -> Vec<Hit> {
        let radius_mm = radius.search_mm();
        if !point.x.is_finite() || !point.y.is_finite() || !radius_mm.is_finite() {
            return Vec::new();
        }
        let mut cands: Vec<Cand> = Vec::with_capacity(16);
        fn admit<'a>(
            cands: &mut Vec<Cand<'a>>,
            radius: &Radius,
            witness: Witness,
            class: HitClass,
            entity: Uuid,
            tail: Tail<'a>,
        ) {
            if witness.within(radius) {
                cands.push(Cand {
                    hit: Hit {
                        class,
                        entity,
                        closest: witness.output(),
                        distance_squared: witness.approx_dist2(),
                    },
                    key: witness.exact_key(),
                    witness: Some(witness),
                    tail,
                });
            }
        }
        // The search box is rounded outward (`Aabb::expanded`) and feature
        // boxes are conservative, so the broad phase never drops a candidate;
        // every admission below is exact.
        let search = Aabb::point(point).expanded(radius_mm);

        // Endpoints. A joined endpoint contributes to its JOIN NODE and never
        // as its own free endpoint (R6.3.2).
        self.endpoints
            .tree()
            .query(&search, |_, _, &(entity, class, at)| {
                admit(
                    &mut cands,
                    radius,
                    Witness::new(point, Shape::Point(at)),
                    class,
                    entity,
                    Tail::None,
                );
            });
        // Opening bands rank ahead of their host's wall region; both are the
        // IDEAL rectangles, so a nearby wall remains cycleable.
        self.openings.tree().query(&search, |_, _, (id, shape)| {
            admit(
                &mut cands,
                radius,
                shape.witness(point),
                HitClass::Opening,
                *id,
                Tail::Opening(shape),
            );
        });
        // Regions: each wall's own ideal butt-capped rectangle (R3.0.2), NOT
        // the cluster union.
        self.regions.tree().query(&search, |_, _, (id, shape)| {
            admit(
                &mut cands,
                radius,
                shape.witness(point),
                HitClass::Region,
                *id,
                Tail::Region(shape),
            );
        });

        // Placeholder boxes (R7.1.3 class 4): the closest point of an
        // axis-aligned box is a CLAMP — query or bound coordinates — so its
        // exact admission needs no construction.
        for i in self.placeholder_index.near(point, radius_mm) {
            let p = self.placeholders[i];
            let closest = Point::new(
                point.x.clamp(p.bounds.min_x, p.bounds.max_x),
                point.y.clamp(p.bounds.min_y, p.bounds.max_y),
            );
            let witness = Witness::new(point, Shape::Point(closest));
            if witness.within(radius) {
                cands.push(Cand {
                    hit: Hit {
                        class: HitClass::PlaceholderBox,
                        entity: p.instance,
                        closest,
                        distance_squared: witness.approx_dist2(),
                    },
                    witness: None,
                    key: None,
                    tail: Tail::Placeholder(p.bounds),
                });
            }
        }

        // R6.3.2 merges only candidates equal through EVERY key level. Sort a
        // permutation (candidates carry witnesses); the index is the last
        // key, so the unstable sort is total and deterministic.
        let mut order: Vec<u32> = (0..cands.len() as u32).collect();
        order.sort_unstable_by(|&a, &b| {
            rank(&cands[a as usize], &cands[b as usize]).then(a.cmp(&b))
        });
        order.dedup_by(|x, y| {
            let (x, y) = (&cands[*x as usize], &cands[*y as usize]);
            x.hit.class == y.hit.class
                && x.hit.entity == y.hit.entity
                && rank(x, y) == Ordering::Equal
        });
        order
            .into_iter()
            .map(|i| cands[i as usize].hit.clone())
            .collect()
    }

    /// The number of indexed wall regions (collapsed ones included).
    pub fn len(&self) -> usize {
        self.regions.tree().len()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.tree().is_empty()
    }

    /// [`marquee_with_assets`]: R7.3.1 boundary-inclusive intersection of
    /// `rect` with each wall's IDEAL rectangle (exact separating axes), each
    /// placeholder box, and each resolved asset footprint (issue 0111),
    /// ordered by R7.3.2's key — the interaction shape's exact bounding-box
    /// min corner, class rank, then id.
    pub fn marquee(&self, rect: Aabb) -> Vec<Uuid> {
        self.marquee_with_assets(rect, &[])
    }

    /// [`marquee`], plus the resolved instances the caller's asset store
    /// decided are marquee candidates on this level (issue 0111).
    ///
    /// Asset footprints join the same keyed list as wall regions and
    /// placeholder boxes, ranked by the same min-corner key, so the R7.3.2
    /// order stays total and delivery-order-independent across all three
    /// classes. Coincident corners keep point-hit semantics and answer from
    /// the geometry classes alone: resolved instances are point-hit by the
    /// asset store's own query, which a degenerate rectangle never merges.
    pub fn marquee_with_assets(&self, rect: Aabb, assets: &[AssetFootprint]) -> Vec<Uuid> {
        // Coincident corners degenerate to point-hit semantics (R7.3.2a): the
        // entity a CLICK there selects.
        if rect.width() == 0.0 && rect.height() == 0.0 {
            let p = Point::new(rect.min_x, rect.min_y);
            return self
                .hit_test(p, 0.0)
                .into_iter()
                .next()
                .map(|h| h.entity)
                .into_iter()
                .collect();
        }
        let mut keyed = Keyed::default();
        self.regions.tree().query(&rect, |_, _, (id, prepared)| {
            if let Some(exact) = prepared.axis {
                if exact.intersects(&rect) {
                    keyed.push_exact(exact.min_x, exact.min_y, HitClass::Region, *id);
                }
            } else if rect_meets_box(&prepared.shape, &rect) {
                keyed.push_rect(prepared.bbox_min(), HitClass::Region, *id);
            }
        });
        for p in &self.placeholders {
            if p.bounds.intersects(&rect) {
                keyed.push_exact(
                    p.bounds.min_x,
                    p.bounds.min_y,
                    HitClass::PlaceholderBox,
                    p.instance,
                );
            }
        }
        for a in assets {
            if a.bounds.intersects(&rect) {
                keyed.push_exact(
                    a.bounds.min_x,
                    a.bounds.min_y,
                    HitClass::AssetFootprint,
                    a.instance,
                );
            }
        }
        keyed.into_ids()
    }
}

/// Entity ids whose interaction shape intersects `rect` on `level_id`
/// (R7.3.1), in ascending MARQUEE KEY order.
///
/// The marquee key is `(min-corner x, min-corner y of the interaction shape's
/// bounding box, class rank, stable entity id)`, and R7.3.2 requires it
/// precisely so that "most recent addition" is total and
/// delivery-order-independent. Each entity appears at most once even when
/// several of its features intersect.
///
/// Intersection is boundary-inclusive (tangential contact selects) and
/// EXACT against each wall's ideal rectangle: separating axes with `√D` kept
/// symbolic, so any finite `rect` is answered without overflow or rounded
/// intersection parameters (issue 0012). The key's min corner is the ideal
/// bounding box's, compared exactly.
///
/// # Degenerate rectangles
///
/// Coincident corners take point-hit semantics (R7.3.2, as scoped by
/// R7.3.2a): the entity a click there selects. A LINE rectangle has zero area
/// but two distinct corners and keeps R7.3.1 intersection (fixture L13).
pub fn marquee(doc: &Document, level_id: Uuid, rect: Aabb) -> Vec<Uuid> {
    marquee_with(doc, level_id, rect, &[])
}

/// [`marquee`], plus the degraded instances a resolver decided are
/// `placeholder-box` candidates on this level (R7.1.3, R7.5.3, §9.6).
///
/// Placeholder boxes join the same keyed list as wall regions, so the R7.3.2
/// marquee key orders the two classes TOGETHER by min corner and only breaks a
/// tie by class rank. One-shot: builds the level's [`HitIndex`]; reuse an
/// index ([`HitIndex::marquee_with_assets`]) across a gesture.
pub fn marquee_with(
    doc: &Document,
    level_id: Uuid,
    rect: Aabb,
    placeholders: &[Placeholder],
) -> Vec<Uuid> {
    marquee_all(doc, level_id, rect, placeholders, &[])
}

/// [`marquee_with`], plus the resolved instances the caller's asset store
/// decided are marquee candidates on this level (issue 0111).
///
/// Asset footprints join the same keyed list as wall regions and placeholder
/// boxes under R7.3.2's key; an exact min-corner tie between classes is broken
/// by class rank, ranking every geometry class ahead of `AssetFootprint`,
/// which matches the click path, where asset-store hits follow every geometry
/// hit. One-shot: builds the level's [`HitIndex`]; reuse an index
/// ([`HitIndex::marquee_with_assets`]) across a gesture.
pub fn marquee_all(
    doc: &Document,
    level_id: Uuid,
    rect: Aabb,
    placeholders: &[Placeholder],
    assets: &[AssetFootprint],
) -> Vec<Uuid> {
    // A direct scan — the one-shot path, and an independent check on the
    // indexed [`HitIndex::marquee_with_assets`] (same exact predicates, no
    // tree). Coincident corners take point-hit semantics from the geometry
    // classes alone (R7.3.2a): resolved instances are point-hit by the asset
    // store's own query.
    if rect.width() == 0.0 && rect.height() == 0.0 {
        return HitIndex::build_with(doc, level_id, placeholders).marquee_with_assets(rect, assets);
    }
    let mut keyed = Keyed::default();
    for wall in doc.walls_on_level(level_id) {
        let solid = WallSolid::new(wall.start(), wall.end(), wall.thickness_mm);
        let (dx, dy) = (solid.end.x - solid.start.x, solid.end.y - solid.start.y);
        let t = solid.thickness_mm;
        if !(dx.is_finite() && dy.is_finite() && (dx != 0.0 || dy != 0.0))
            || !(t.is_finite() && t > 0.0)
        {
            continue;
        }
        let shape = Shape::Rect {
            a: solid.start,
            b: solid.end,
            thickness: solid.thickness_mm,
            along: Along::Wall,
        };
        if let Some(exact) = axis_box(&shape) {
            // Its box IS the ideal shape: exact intersection and key.
            if exact.intersects(&rect) {
                keyed.push_exact(exact.min_x, exact.min_y, HitClass::Region, wall.id);
            }
        } else if rect_bounds(&shape).is_some_and(|b| b.intersects(&rect))
            && rect_meets_box(&shape, &rect)
        {
            keyed.push_rect(BboxMin::of(shape), HitClass::Region, wall.id);
        }
    }
    for p in placeholders {
        if p.bounds.intersects(&rect) {
            keyed.push_exact(
                p.bounds.min_x,
                p.bounds.min_y,
                HitClass::PlaceholderBox,
                p.instance,
            );
        }
    }
    for a in assets {
        if a.bounds.intersects(&rect) {
            keyed.push_exact(
                a.bounds.min_x,
                a.bounds.min_y,
                HitClass::AssetFootprint,
                a.instance,
            );
        }
    }
    keyed.into_ids()
}

/// A marquee candidate's R7.3.2 min-corner key: an ideal rectangle's exact
/// bounding-box minimum, or a placeholder box's stored one.
enum MinKey {
    /// Boxed: the exact key is large and rarely needed past its enclosure.
    Rect(Box<BboxMin>),
    Boxed(f64, f64),
}

/// Marquee candidates under R7.3.2's key. Keys that are exact binary64
/// values (axis-aligned walls, placeholder boxes) go in a plain float list
/// — the common case sorts exactly like floats, because it IS floats; only a
/// selected rotated wall needs the exact key machinery.
#[derive(Default)]
struct Keyed {
    exact: Vec<(f64, f64, u8, Uuid)>,
    general: Vec<(MinKey, u8, Uuid)>,
}

impl Keyed {
    fn push_exact(&mut self, x: f64, y: f64, class: HitClass, id: Uuid) {
        self.exact.push((x, y, class as u8, id));
    }

    fn push_rect(&mut self, key: BboxMin, class: HitClass, id: Uuid) {
        match key.exact_key() {
            Some((x, y)) => self.push_exact(x, y, class, id),
            None => self
                .general
                .push((MinKey::Rect(Box::new(key)), class as u8, id)),
        }
    }

    fn into_ids(mut self) -> Vec<Uuid> {
        if self.general.is_empty() {
            self.exact.sort_by(|a, b| {
                (a.0, a.1)
                    .partial_cmp(&(b.0, b.1))
                    .unwrap_or(Ordering::Equal)
                    .then(a.2.cmp(&b.2))
                    .then(a.3.cmp(&b.3))
            });
            return self.exact.into_iter().map(|k| k.3).collect();
        }
        let mut all = self.general;
        all.extend(
            self.exact
                .into_iter()
                .map(|(x, y, c, id)| (MinKey::Boxed(x, y), c, id)),
        );
        sort_keyed(&mut all);
        all.into_iter().map(|(_, _, id)| id).collect()
    }
}

/// R7.3.2's order — min corner (exact), class rank, entity id — for a list
/// holding at least one non-binary64 key.
fn sort_keyed(keyed: &mut [(MinKey, u8, Uuid)]) {
    let cmp_key = |a: &MinKey, b: &MinKey| match (a, b) {
        (MinKey::Rect(x), MinKey::Rect(y)) => x.cmp(y),
        (MinKey::Rect(x), MinKey::Boxed(bx, by)) => x.cmp_point(*bx, *by),
        (MinKey::Boxed(ax, ay), MinKey::Rect(y)) => y.cmp_point(*ax, *ay).reverse(),
        (MinKey::Boxed(ax, ay), MinKey::Boxed(bx, by)) => {
            (ax, ay).partial_cmp(&(bx, by)).unwrap_or(Ordering::Equal)
        }
    };
    keyed.sort_by(|a, b| cmp_key(&a.0, &b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{
        AssetInstance, InstanceTransform, JoinMember, Level, Role, Wall, WallJoin,
    };

    const P: fn(f64, f64) -> Point = Point::new;

    /// An L of two walls joined at (4000, 0).
    fn l_shape() -> (Document, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(4000.0, 0.0), 200.0, 2500.0));
        doc.walls.push(Wall::new(
            level,
            P(4000.0, 0.0),
            P(4000.0, 3000.0),
            200.0,
            2500.0,
        ));
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(doc.walls[0].id, Role::End),
            JoinMember::new(doc.walls[1].id, Role::Start),
        ]));
        assert_eq!(doc.validate(), vec![]);
        (doc, level)
    }

    #[test]
    fn opening_is_selected_before_its_host_and_only_on_its_level() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let wall = Wall::new(level, P(0.0, 0.0), P(4000.0, 0.0), 200.0, 2500.0);
        let wall_id = wall.id;
        let opening = domowik_format::Opening::window(wall_id, 0.5, 1200.0, 1400.0, 900.0, 200.0);
        let opening_id = opening.id;
        doc.walls.push(wall);
        doc.openings.push(opening);
        let hits = hit_test(&doc, level, P(2000.0, 0.0), 0.0);
        assert_eq!(hits[0].class, HitClass::Opening);
        assert_eq!(hits[0].entity, opening_id);
        assert_eq!(hits[1].entity, wall_id);
        assert!(hit_test(&doc, Uuid::nil(), P(2000.0, 0.0), 0.0).is_empty());
        assert_eq!(hit_test(&doc, level, P(500.0, 0.0), 0.0)[0].entity, wall_id);
    }

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
                + (self.next() - 0.5) * 40.0 * (self.below(3) == 0) as u8 as f64
        }
    }

    /// Issue 0012: a MAINTAINED index answers point hits and marquees
    /// exactly as one built from scratch (and as the scanning
    /// [`marquee_with`]) after every edit of a random stream of walls,
    /// joins, openings and placeholder snapshots.
    #[test]
    fn a_synced_index_answers_exactly_as_a_fresh_build_after_every_edit() {
        use domowik_format::Opening;
        for seed in 1..=6u64 {
            let mut rng = Rng(0x7a3c_99e1_0b55_d24f ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let mut doc = Document::with_ground_floor();
            let level = doc.levels[0].id;
            let mut index = HitIndex::new(level);
            let mut placeholders: Vec<Placeholder> = Vec::new();
            for step in 0..60 {
                match rng.below(12) {
                    0..=3 => {
                        let a = P(rng.coord(), rng.coord());
                        let b = P(rng.coord(), rng.coord());
                        if a != b {
                            doc.walls.push(Wall::new(level, a, b, 200.0, 2500.0));
                        }
                    }
                    4 if !doc.walls.is_empty() => {
                        let id = doc.walls.remove(rng.below(doc.walls.len())).id;
                        doc.joins
                            .retain(|j| j.members.iter().all(|m| m.wall_id != id));
                        doc.openings.retain(|o| o.host_wall_id != id);
                    }
                    5..=6 if !doc.walls.is_empty() => {
                        let i = rng.below(doc.walls.len());
                        let end = P(rng.coord(), rng.coord());
                        if end != doc.walls[i].start() {
                            doc.walls[i].end_mm = end.into();
                        }
                    }
                    7 if doc.walls.len() >= 2 => {
                        let (wa, wb) = (doc.walls[0].id, doc.walls[doc.walls.len() - 1].id);
                        let free = |w: Uuid, r: Role| {
                            doc.joins
                                .iter()
                                .all(|j| j.members.iter().all(|m| m.wall_id != w || m.role != r))
                        };
                        if wa != wb && free(wa, Role::End) && free(wb, Role::Start) {
                            doc.joins.push(WallJoin::new(vec![
                                JoinMember::new(wa, Role::End),
                                JoinMember::new(wb, Role::Start),
                            ]));
                        }
                    }
                    8..=9 if !doc.walls.is_empty() => {
                        let host = doc.walls[rng.below(doc.walls.len())].id;
                        let u = 0.2 + 0.6 * rng.next();
                        doc.openings
                            .push(Opening::window(host, u, 600.0, 1400.0, 900.0, 200.0));
                    }
                    10 if !doc.openings.is_empty() => {
                        let i = rng.below(doc.openings.len());
                        doc.openings[i].u = 0.2 + 0.6 * rng.next();
                    }
                    _ => {
                        let at = P(rng.coord(), rng.coord());
                        placeholders.push(Placeholder {
                            instance: Uuid::now_v7(),
                            bounds: Aabb::from_corners(at, P(at.x + 300.0, at.y + 200.0)),
                        });
                        if placeholders.len() > 3 {
                            placeholders.remove(0);
                        }
                    }
                }
                index.sync(&doc, &placeholders);
                index.validate();
                let fresh = HitIndex::build_with(&doc, level, &placeholders);
                for _ in 0..12 {
                    let p = P(rng.coord(), rng.coord());
                    assert_eq!(
                        index.hit_test(p, 150.0),
                        fresh.hit_test(p, 150.0),
                        "seed {seed} step {step} at {p:?}"
                    );
                    let q = P(rng.coord(), rng.coord());
                    let rect = Aabb::from_corners(p, q);
                    assert_eq!(
                        index.marquee(rect),
                        marquee_with(&doc, level, rect, &placeholders),
                        "seed {seed} step {step} marquee {rect:?}"
                    );
                }
            }
        }
    }

    /// Codex GPT-6.1-Sol review: a valid wall whose rectangle collapses in
    /// binary64 is still selected by the marquee, as the scan selects it.
    #[test]
    fn the_indexed_marquee_keeps_a_wall_whose_rectangle_collapses() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 1.0), P(1000.0, 1.0), 1e-17, 2500.0));
        assert_eq!(doc.validate(), vec![]);
        let rect = Aabb::from_corners(P(-1.0, 0.0), P(1001.0, 2.0));
        let scanned = marquee_with(&doc, level, rect, &[]);
        assert_eq!(scanned, vec![doc.walls[0].id]);
        assert_eq!(HitIndex::build(&doc, level).marquee(rect), scanned);
        let mut thick = doc.clone();
        thick.walls[0].thickness_mm = 200.0;
        let mut index = HitIndex::build(&thick, level);
        index.sync(&doc, &[]);
        assert_eq!(index.marquee(rect), scanned);
    }

    /// Codex GPT-6.1-Sol review (exact kernel): a wall whose float
    /// enclosures underflow still reports its TRUE closest point and
    /// distance when the pointer is outside it — not the pointer at 0.
    #[test]
    fn a_tiny_wall_reports_its_true_closest_point_from_outside() {
        let t = 2f64.powi(-600);
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(3.0 * t, t), 1.0, 2500.0));
        let p = P(1.0, 0.4);
        let hits = hit_test(&doc, level, p, 2.0);
        let region = hits
            .iter()
            .find(|h| h.class == HitClass::Region)
            .expect("the wall is within 2 mm");
        assert_ne!(region.closest, p);
        // Exact squared distance ≈ 1.156 (Sol's Fraction computation).
        assert!((region.distance_squared - 1.156).abs() < 1e-3, "{region:?}");
        assert!(
            (region.closest.x + 0.02).abs() < 1e-3 && (region.closest.y - 0.06).abs() < 1e-3,
            "{region:?}"
        );
    }

    /// Codex GPT-6.1-Sol fix verification (FV1): with the distance enclosure
    /// lost, the reported squared distance must not go negative through
    /// cancellation in the expanded radical form.
    #[test]
    fn a_lost_enclosure_never_reports_a_negative_distance() {
        let t = 2f64.powi(-600);
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(t, t), 2.0, 2500.0));
        let p = P(0.7071067811865475, -0.7071067811865477);
        let hits = hit_test(&doc, level, p, 1e-15);
        let region = hits
            .iter()
            .find(|h| h.class == HitClass::Region)
            .expect("the true distance is ≈ 1.7e-16 mm");
        assert!(region.distance_squared >= 0.0, "{region:?}");
        assert!(region.distance_squared < 1e-30, "{region:?}");
    }

    /// Issue 0012 (Codex review C): a point on the ROUNDED polygon's corner
    /// lies outside the IDEAL wall, so a zero-radius hit there finds nothing.
    #[test]
    fn a_rounded_corner_is_not_a_hit_on_the_ideal_wall() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(10.0, 10.0), 2.0, 2500.0));
        let corner = P(9.292893218813452, 10.707106781186548);
        assert!(hit_test(&doc, level, corner, 0.0)
            .iter()
            .all(|h| h.class != HitClass::Region));
    }

    /// Issue 0012 (Codex review D): a valid 2⁻⁶⁰ mm wall, whose drawn polygon
    /// collapses in binary64, still contains a point on its axis.
    #[test]
    fn a_hair_thin_wall_is_still_hit() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            P(0.0, 1.0),
            P(1.0, 1.0),
            2f64.powi(-60),
            2500.0,
        ));
        let hits = hit_test(&doc, level, P(0.5, 1.0), 0.0);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            (hits[0].class, hits[0].closest),
            (HitClass::Region, P(0.5, 1.0))
        );
    }

    /// ED-0112: a join node is the centre of its members' topology cell, so
    /// members a fraction of a cell apart, listed in either order, give one
    /// node at one point.
    #[test]
    fn a_join_node_is_its_cell_centre_whatever_the_member_order() {
        let q = 1.0 / 1024.0;
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            P(0.0, 0.0),
            P(1000.0 + 0.2 * q, 0.0),
            200.0,
            2500.0,
        ));
        doc.walls.push(Wall::new(
            level,
            P(1000.0 - 0.3 * q, 0.0),
            P(1000.0, 800.0),
            200.0,
            2500.0,
        ));
        let (a, b) = (doc.walls[0].id, doc.walls[1].id);
        let node = |members: Vec<JoinMember>| {
            let mut d = doc.clone();
            d.joins.push(WallJoin::new(members));
            let hits = hit_test(&d, level, P(1000.0, 0.0), 1.0);
            hits.into_iter()
                .find(|h| h.class == HitClass::JoinNode)
                .expect("a join node")
                .closest
        };
        let forward = node(vec![
            JoinMember::new(a, Role::End),
            JoinMember::new(b, Role::Start),
        ]);
        let reverse = node(vec![
            JoinMember::new(b, Role::Start),
            JoinMember::new(a, Role::End),
        ]);
        assert_eq!(forward, reverse);
        assert_eq!(forward, P(1000.0, 0.0), "the cell centre");
    }

    /// An endpoint tree holding exactly `items`, under synthetic keys so
    /// that identical items can coexist.
    fn endpoint_tree(
        items: &[(Uuid, HitClass, Point)],
    ) -> KeyedTree<EndpointKey, Point, (Uuid, HitClass, Point)> {
        let key = |i: usize| EndpointKey::Join(Uuid::from_u128(1000 + i as u128));
        let mut tree = KeyedTree::default();
        tree.sync(
            items.iter().enumerate().map(|(i, item)| (key(i), item.2)),
            |k, _| {
                let i = (k.entity().as_u128() - 1000) as usize;
                Some((Aabb::point(items[i].2), items[i]))
            },
        );
        tree
    }

    fn index_of(regions: Vec<(Uuid, Shape)>, endpoints: &[(Uuid, HitClass, Point)]) -> HitIndex {
        let mut index = HitIndex::new(Uuid::nil());
        let stamp = WallSolid::new(P(0.0, 0.0), P(1.0, 0.0), 1.0);
        index
            .regions
            .sync(regions.iter().map(|(id, _)| (*id, stamp)), |id, _| {
                let shape = regions.iter().find(|(r, _)| r == id)?.1;
                Some((rect_bounds(&shape)?, (*id, PreparedRect::of(shape))))
            });
        index.endpoints = endpoint_tree(endpoints);
        index
    }

    fn endpoint_only_index(points: &[Point]) -> HitIndex {
        let items: Vec<_> = points
            .iter()
            .enumerate()
            .map(|(i, &at)| (Uuid::from_u128(i as u128 + 1), HitClass::FreeEndpoint, at))
            .collect();
        index_of(Vec::new(), &items)
    }

    #[test]
    fn endpoint_radius_excludes_a_point_beyond_a_rounded_tie() {
        let at = P(1.0, 2.0_f64.powi(-27));
        assert!(endpoint_only_index(&[at])
            .hit_test(P(0.0, 0.0), 1.0)
            .is_empty());
    }

    #[test]
    fn endpoint_order_uses_exact_distance_before_coordinate_ties() {
        let farther = P(-1.0, 2.0_f64.powi(-27));
        let nearer = P(1.0, 0.0);
        for points in [[farther, nearer], [nearer, farther]] {
            let hits = endpoint_only_index(&points).hit_test(P(0.0, 0.0), 2.0);
            assert_eq!(hits.len(), 2);
            assert_eq!(hits[0].closest, nearer);
        }
    }

    #[test]
    fn endpoint_radius_handles_squared_underflow_and_overflow() {
        for radius in [f64::from_bits(1), 1e308] {
            let index = endpoint_only_index(&[P(radius, radius), P(radius, 0.0)]);
            let hits = index.hit_test(P(0.0, 0.0), radius);
            assert_eq!(hits.len(), 1, "radius {radius}");
            assert_eq!(hits[0].closest, P(radius, 0.0));
        }
    }

    #[test]
    fn endpoint_radius_includes_exact_boundary_and_rejects_invalid_queries() {
        let index = endpoint_only_index(&[P(3.0, 4.0)]);
        assert_eq!(index.hit_test(P(0.0, 0.0), 5.0).len(), 1);
        for radius in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(index.hit_test(P(0.0, 0.0), radius).is_empty());
        }
        assert!(index.hit_test(P(f64::NAN, 0.0), 5.0).is_empty());
    }

    #[test]
    fn a_click_inside_a_wall_hits_its_region_at_exact_distance_zero() {
        let (doc, level) = l_shape();
        let hits = hit_test(&doc, level, P(2000.0, 0.0), 100.0);
        assert_eq!(hits[0].class, HitClass::Region);
        assert_eq!(hits[0].entity, doc.walls[0].id);
        assert_eq!(hits[0].distance_squared, 0.0);
        assert_eq!(hits[0].closest, P(2000.0, 0.0), "the pointer itself");
    }

    #[test]
    fn a_join_node_outranks_the_regions_that_meet_there() {
        // R7.1.3 precedence: join-node beats free-endpoint beats region, and
        // the classes decide BEFORE distance does.
        let (doc, level) = l_shape();
        let hits = hit_test(&doc, level, P(4000.0, 0.0), 200.0);
        assert_eq!(hits[0].class, HitClass::JoinNode);
        assert_eq!(hits[0].entity, doc.joins[0].id);
        // Both walls are also hit, as regions, behind it.
        assert!(hits.iter().any(|h| h.class == HitClass::Region));
    }

    #[test]
    fn a_joined_endpoint_never_appears_as_a_free_endpoint() {
        // R6.3.2: joined endpoints always deduplicate to their join node.
        let (doc, level) = l_shape();
        let hits = hit_test(&doc, level, P(4000.0, 0.0), 200.0);
        assert!(
            !hits.iter().any(|h| h.class == HitClass::FreeEndpoint),
            "got {hits:#?}"
        );
    }

    #[test]
    fn an_unjoined_endpoint_is_a_free_endpoint_candidate() {
        let (doc, level) = l_shape();
        let hits = hit_test(&doc, level, P(0.0, 0.0), 200.0);
        assert_eq!(hits[0].class, HitClass::FreeEndpoint);
        assert_eq!(hits[0].entity, doc.walls[0].id);
    }

    #[test]
    fn a_click_outside_the_radius_hits_nothing() {
        let (doc, level) = l_shape();
        assert!(hit_test(&doc, level, P(2000.0, 5000.0), 100.0).is_empty());
    }

    #[test]
    fn the_radius_boundary_is_inclusive() {
        // R7.1.3: "inclusive boundaries".
        let (doc, level) = l_shape();
        // 100 mm from the centre line is the wall face; exactly on it hits.
        assert!(!hit_test(&doc, level, P(2000.0, 100.0), 0.0).is_empty());
        assert!(hit_test(&doc, level, P(2000.0, 100.001), 0.0).is_empty());
    }

    #[test]
    fn walls_on_another_level_are_never_hit() {
        // R7.1.3: only active-level entities are eligible.
        let (mut doc, level) = l_shape();
        let upper = Level::new("Level 2");
        let upper_id = upper.id;
        doc.levels.push(upper);
        doc.walls.push(Wall::new(
            upper_id,
            P(2000.0, 0.0),
            P(2000.0, 1000.0),
            200.0,
            2500.0,
        ));
        let hits = hit_test(&doc, level, P(2000.0, 0.0), 100.0);
        assert!(hits.iter().all(|h| h.entity != doc.walls[2].id));

        // On its own level the same point hits it twice: its free endpoint
        // sits there, and so does its region.
        let upper_hits = hit_test(&doc, upper_id, P(2000.0, 0.0), 100.0);
        assert_eq!(upper_hits.len(), 2, "got {upper_hits:#?}");
        assert!(upper_hits.iter().all(|h| h.entity == doc.walls[2].id));
        assert_eq!(upper_hits[0].class, HitClass::FreeEndpoint);
    }

    #[test]
    fn ranking_is_total_so_repeated_clicks_cycle_deterministically() {
        // R6.3.3: delivery order, map iteration order and insertion order are
        // never observable. Two identical walls at one point must still rank
        // in a fixed order, decided by the stable entity id.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        for _ in 0..4 {
            doc.walls
                .push(Wall::new(level, P(0.0, 0.0), P(1000.0, 0.0), 200.0, 2500.0));
        }
        let hits = hit_test(&doc, level, P(500.0, 0.0), 50.0);
        let regions: Vec<Uuid> = hits
            .iter()
            .filter(|h| h.class == HitClass::Region)
            .map(|h| h.entity)
            .collect();
        assert_eq!(regions.len(), 4);
        let mut sorted = regions.clone();
        sorted.sort();
        assert_eq!(regions, sorted, "ties break by ascending entity id");

        // And the answer does not depend on document order.
        let mut reversed = doc.clone();
        reversed.walls.reverse();
        let again: Vec<Uuid> = hit_test(&reversed, level, P(500.0, 0.0), 50.0)
            .iter()
            .filter(|h| h.class == HitClass::Region)
            .map(|h| h.entity)
            .collect();
        assert_eq!(regions, again);
    }

    #[test]
    fn the_nearer_of_two_regions_ranks_first() {
        let (doc, level) = l_shape();
        // Just outside wall 0's face, far from wall 1.
        let hits = hit_test(&doc, level, P(1000.0, 150.0), 3000.0);
        let regions: Vec<&Hit> = hits
            .iter()
            .filter(|h| h.class == HitClass::Region)
            .collect();
        assert_eq!(regions[0].entity, doc.walls[0].id);
        assert!(regions[0].distance_squared < regions[1].distance_squared);
    }

    #[test]
    fn equal_distance_regions_compare_geometry_before_identity() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let mut horizontal = Wall::new(level, P(-1000.0, 0.0), P(1000.0, 0.0), 200.0, 2500.0);
        let mut vertical = Wall::new(level, P(0.0, -1000.0), P(0.0, 1000.0), 200.0, 2500.0);
        horizontal.id = Uuid::from_u128(2);
        vertical.id = Uuid::from_u128(1);
        doc.walls = vec![vertical, horizontal];
        for reverse_document in [false, true] {
            for reverse_endpoints in [false, true] {
                let mut variant = doc.clone();
                if reverse_document {
                    variant.walls.reverse();
                }
                if reverse_endpoints {
                    for wall in &mut variant.walls {
                        std::mem::swap(&mut wall.start_mm, &mut wall.end_mm);
                    }
                }
                let hits = hit_test(&variant, level, P(0.0, 0.0), 12.0);
                assert_eq!(hits.len(), 2);
                assert!(hits
                    .iter()
                    .all(|hit| hit.class == HitClass::Region && hit.distance_squared == 0.0));
                assert_eq!(
                    hits[0].entity,
                    Uuid::from_u128(2),
                    "canonical min corner (-1000,-100) precedes (-100,-1000), despite the IDs"
                );
            }
        }
    }

    /// A wall whose two FREE endpoints both fall inside the radius.
    fn stub(level: Uuid, doc: &mut Document) -> Uuid {
        doc.walls.push(Wall::new(
            level,
            P(-300.0, 0.0),
            P(300.0, 0.0),
            200.0,
            2500.0,
        ));
        doc.walls[doc.walls.len() - 1].id
    }

    /// R6.3.2 merges two candidates only when they are equal through the
    /// FINAL key level. Two free endpoints of ONE wall share a class and an
    /// entity but differ at level 3, so they are two candidates — and being
    /// equidistant they sort ADJACENTLY, which is exactly where a
    /// `(class, entity)` predicate silently dropped one of them.
    #[test]
    fn both_free_endpoints_of_one_wall_survive_when_they_sort_adjacently() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let wall = stub(level, &mut doc);
        assert_eq!(doc.validate(), vec![]);

        // Equidistant from both endpoints (d² = 300² + 500² = 340000), so
        // nothing can sort between them.
        let hits = hit_test(&doc, level, P(0.0, 500.0), 600.0);
        let ends: Vec<&Hit> = hits
            .iter()
            .filter(|h| h.class == HitClass::FreeEndpoint)
            .collect();
        assert_eq!(ends.len(), 2, "one wall, two distinct endpoints: {hits:#?}");
        assert!(ends.iter().all(|h| h.entity == wall));
        assert_eq!(ends[0].closest, P(-300.0, 0.0));
        assert_eq!(ends[1].closest, P(300.0, 0.0));
        assert_eq!(ends[0].distance_squared, ends[1].distance_squared);
        // The region is the same entity again at a DIFFERENT class, and
        // classes never merge (R6.3.2, resolved at level 1).
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[2].class, HitClass::Region);
        assert_eq!(hits[2].entity, wall);
    }

    /// The dedupe collapses ONLY exact full-key duplicates.
    ///
    /// Built against the private index rather than from a `Document`, because
    /// a VALID document cannot produce two byte-identical endpoint features —
    /// and fabricating an invalid one would test the fixture, not the
    /// predicate. This asserts the predicate directly: identical keys
    /// collapse, an entity shared across classes does not, and one entity's
    /// two endpoints at different points do not.
    #[test]
    fn only_identical_full_keys_collapse() {
        let wall = Uuid::from_u128(7);
        let origin = P(0.0, 0.0);
        let offset = P(50.0, 0.0);
        let regions = vec![(
            wall,
            Shape::Rect {
                a: P(-100.0, 0.0),
                b: P(100.0, 0.0),
                thickness: 20.0,
                along: Along::Wall,
            },
        )];
        let index = index_of(
            regions,
            &[
                // Equal at EVERY level: these two are one candidate.
                (wall, HitClass::FreeEndpoint, origin),
                (wall, HitClass::FreeEndpoint, origin),
                // Same entity and class, different point: level 3 separates.
                (wall, HitClass::FreeEndpoint, offset),
                // Same entity and point, different class: level 1 separates.
                (wall, HitClass::JoinNode, origin),
            ],
        );

        let hits = index.hit_test(origin, 100.0);
        assert_eq!(
            hits.iter()
                .map(|h| (h.class, h.closest))
                .collect::<Vec<_>>(),
            [
                (HitClass::JoinNode, origin),
                (HitClass::FreeEndpoint, origin),
                (HitClass::FreeEndpoint, offset),
                (HitClass::Region, origin),
            ],
            "only the identical pair collapses: {hits:#?}"
        );
        assert!(hits.iter().all(|h| h.entity == wall));
    }

    /// One wall with both free endpoints in reach, plus a second wall whose
    /// own endpoint ranks BETWEEN them.
    fn interleaved() -> (Document, Uuid, Uuid, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let mut near = Wall::new(level, P(-200.0, 0.0), P(600.0, 0.0), 200.0, 2500.0);
        let mut other = Wall::new(level, P(0.0, 400.0), P(0.0, 1400.0), 200.0, 2500.0);
        near.id = Uuid::from_u128(1);
        other.id = Uuid::from_u128(2);
        doc.walls = vec![near, other];
        (doc, level, Uuid::from_u128(1), Uuid::from_u128(2))
    }

    /// The same defect with a foreign candidate ranking between the two.
    ///
    /// This case PASSED against the old predicate while the adjacent one
    /// failed, and the pair is what localised the defect: `dedup_by` removes
    /// only consecutive duplicates, so one wall's two endpoints survived or
    /// did not according to whether another feature ranked between them. The
    /// variable is the SCENE, not delivery or insertion order.
    #[test]
    fn both_free_endpoints_of_one_wall_survive_when_another_sorts_between() {
        let (doc, level, near, other) = interleaved();
        // d² from the origin: near.start 40000, other.start 160000,
        // near.end 360000 — the foreign endpoint lands in the middle.
        let hits = hit_test(&doc, level, P(0.0, 0.0), 1000.0);
        let ends: Vec<&Hit> = hits
            .iter()
            .filter(|h| h.class == HitClass::FreeEndpoint)
            .collect();
        assert_eq!(
            ends.iter().map(|h| h.entity).collect::<Vec<_>>(),
            [near, other, near],
            "{hits:#?}"
        );
        assert_eq!(ends[0].closest, P(-200.0, 0.0));
        assert_eq!(ends[1].closest, P(0.0, 400.0));
        assert_eq!(ends[2].closest, P(600.0, 0.0));
        assert_eq!(hits.len(), 5, "three endpoints and two regions: {hits:#?}");
    }

    /// R6.3.3: neither document order nor endpoint direction is observable.
    ///
    /// A reversed wall has the same rectangle and the same endpoint SET, so
    /// every variant below must produce a byte-identical candidate list.
    ///
    /// A STANDING GUARD, not evidence of a repaired regression: this passed
    /// before the R6.3.2 dedupe fix as well. The total sort already made
    /// insertion order unobservable, and the old dedupe's fault lay elsewhere.
    #[test]
    fn feature_dedupe_is_independent_of_document_and_endpoint_order() {
        let (doc, level, _, _) = interleaved();
        let expected = hit_test(&doc, level, P(0.0, 0.0), 1000.0);
        for reverse_document in [false, true] {
            for reverse_endpoints in [false, true] {
                let mut variant = doc.clone();
                if reverse_document {
                    variant.walls.reverse();
                }
                if reverse_endpoints {
                    for wall in &mut variant.walls {
                        std::mem::swap(&mut wall.start_mm, &mut wall.end_mm);
                    }
                }
                assert_eq!(
                    hit_test(&variant, level, P(0.0, 0.0), 1000.0),
                    expected,
                    "document={reverse_document} endpoints={reverse_endpoints}"
                );
            }
        }
    }

    #[test]
    fn hit_test_agrees_with_a_naive_scan_over_a_sweep_of_points() {
        // Issue 0012's first criterion: index results match naive queries.
        let (doc, level) = l_shape();
        let naive = |p: Point, r: f64| -> Vec<Uuid> {
            let mut out: Vec<Uuid> = doc
                .walls_on_level(level)
                .filter(|w| {
                    // The L's walls are axis-aligned, so their ideal
                    // rectangles are exact boxes and the closest point is a
                    // clamp: an oracle independent of the witness kernel.
                    let solid = WallSolid::new(w.start(), w.end(), w.thickness_mm);
                    let b = Aabb::of(&solid.rectangle().expect("valid wall")).expect("finite");
                    let q = P(p.x.clamp(b.min_x, b.max_x), p.y.clamp(b.min_y, b.max_y));
                    crate::distance::within_radius(p, q, r) == Some(true)
                })
                .map(|w| w.id)
                .collect();
            out.sort();
            out
        };
        for x in (-500..5000).step_by(311) {
            for y in (-500..3500).step_by(407) {
                let p = P(x as f64, y as f64);
                for r in [0.0, 50.0, 250.0] {
                    let mut got: Vec<Uuid> = hit_test(&doc, level, p, r)
                        .into_iter()
                        .filter(|h| h.class == HitClass::Region)
                        .map(|h| h.entity)
                        .collect();
                    got.sort();
                    assert_eq!(got, naive(p, r), "p={p:?} r={r}");
                }
            }
        }
    }

    // ---- placeholder boxes (R7.5.3, §9.6, PH1) ----

    /// PH1's setup: an optional instance with no declared dimensions at origin
    /// (2000,0), on a level with NO wall features, so every candidate in the
    /// fixture has to come from the placeholder.
    fn ph1() -> (Document, Uuid, Placeholder) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let instance = AssetInstance {
            id: Uuid::from_u128(0x9),
            level_id: level,
            digest: format!("sha256:{}", "9a".repeat(32)),
            transform: InstanceTransform {
                translation_mm: [2000.0, 0.0],
                ..Default::default()
            },
            display_name: None,
            unknown: Default::default(),
        };
        let bounds = placeholder_box(&instance).expect("finite origin");
        // PH1 fixes the right face at 2500 through its snap probe, so the box
        // is x in [1500,2500], y in [-500,500].
        assert_eq!((bounds.min_x, bounds.max_x), (1500.0, 2500.0));
        assert_eq!((bounds.min_y, bounds.max_y), (-500.0, 500.0));
        doc.asset_instances.push(instance);
        let id = doc.asset_instances[0].id;
        (
            doc,
            level,
            Placeholder {
                instance: id,
                bounds,
            },
        )
    }

    /// PH1's marquee step, plus the boundary case R7.3.1 applies "identically
    /// to its §9.6 bounds".
    #[test]
    fn a_marquee_containing_a_placeholder_box_selects_the_instance() {
        let (doc, level, ph) = ph1();
        let containing = Aabb::from_corners(P(1400.0, -600.0), P(2600.0, 600.0));
        assert_eq!(
            marquee_with(&doc, level, containing, &[ph]),
            vec![ph.instance]
        );

        // Boundary-inclusive: a rectangle whose right edge lies exactly on the
        // box's left face still selects.
        let tangent = Aabb::from_corners(P(1000.0, -100.0), P(1500.0, 100.0));
        assert_eq!(marquee_with(&doc, level, tangent, &[ph]), vec![ph.instance]);

        // And one that stops a millimetre short does not, so the assertion
        // above is not passing because everything matches.
        let short = Aabb::from_corners(P(1000.0, -100.0), P(1499.0, 100.0));
        assert!(marquee_with(&doc, level, short, &[ph]).is_empty());
    }

    /// Issue 0111: resolved asset footprints are marquee candidates that join
    /// the R7.3.2 keyed order, not an appendix to it.
    #[test]
    fn resolved_asset_footprints_join_the_marquee_keyed_order() {
        let (doc, level) = l_shape();
        // The first wall's ideal rectangle spans x ∈ [0,4000], y ∈ [-100,100],
        // so its exact min corner is (0, -100).
        let wall = doc.walls[0].id;
        // Same min corner as the wall: the tie must fall to CLASS RANK, which
        // puts every geometry class ahead of an asset footprint — the same
        // precedence the click path gives asset-store hits.
        let tied = AssetFootprint {
            instance: Uuid::now_v7(),
            bounds: Aabb::from_corners(P(0.0, -100.0), P(600.0, 500.0)),
        };
        // Clear of the rectangle entirely.
        let away = AssetFootprint {
            instance: Uuid::now_v7(),
            bounds: Aabb::from_corners(P(9000.0, 9000.0), P(9600.0, 9600.0)),
        };
        // Its left face lies exactly on the rectangle's right edge: boundary
        // contact selects, like every other marquee class.
        let tangent = AssetFootprint {
            instance: Uuid::now_v7(),
            bounds: Aabb::from_corners(P(5000.0, -100.0), P(5600.0, 500.0)),
        };
        let rect = Aabb::from_corners(P(-1000.0, -1000.0), P(5000.0, 1000.0));

        // Delivery order must not matter, so try both asset permutations.
        // The L's second wall (min corner (3900, -100)) sorts between the
        // tied asset (0, -100) and the tangent one (5000, -100).
        for assets in [&[tied, away, tangent], &[tangent, away, tied]] {
            assert_eq!(
                marquee_all(&doc, level, rect, &[], assets),
                vec![wall, tied.instance, doc.walls[1].id, tangent.instance]
            );
            // The indexed path agrees with the one-shot scan, assets included.
            assert_eq!(
                HitIndex::build_with(&doc, level, &[]).marquee_with_assets(rect, assets),
                marquee_all(&doc, level, rect, &[], assets)
            );
        }

        // A rectangle that stops short of the tangent asset's face drops it,
        // so the boundary assertion above is not passing by accident.
        let short = Aabb::from_corners(P(-1000.0, -1000.0), P(4999.0, 1000.0));
        assert_eq!(
            marquee_all(&doc, level, short, &[], &[tangent]),
            vec![wall, doc.walls[1].id]
        );

        // Coincident corners keep point-hit semantics from the geometry
        // classes alone (R7.3.2a): an asset footprint is not a point hit.
        let degenerate = Aabb::from_corners(P(300.0, 300.0), P(300.0, 300.0));
        assert!(marquee_all(&doc, level, degenerate, &[], &[tied]).is_empty());
    }

    /// PH1's hit step.
    #[test]
    fn a_click_inside_a_placeholder_box_selects_the_instance() {
        let (doc, level, ph) = ph1();
        let hits = hit_test_with(&doc, level, P(2000.0, 0.0), 0.0, &[ph]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].class, HitClass::PlaceholderBox);
        assert_eq!(hits[0].entity, ph.instance);
        // Inside the box, so the closest point is the query itself.
        assert_eq!(hits[0].closest, P(2000.0, 0.0));
        assert_eq!(hits[0].distance_squared, 0.0);
    }

    #[test]
    fn declared_placeholders_use_scaled_rotated_width_and_depth() {
        let (mut doc, _, _) = ph1();
        let instance = &mut doc.asset_instances[0];
        assert_eq!(
            placeholder_geometry(instance, None),
            placeholder_box(instance).map(PlaceholderGeometry::Fallback)
        );
        let declared = Some((600.0, 2100.0, 400.0));
        assert_eq!(
            placeholder_geometry(instance, declared),
            Some(PlaceholderGeometry::Declared(Aabb::from_corners(
                P(1700.0, -200.0),
                P(2300.0, 200.0)
            )))
        );
        instance.transform.scale_uniform = 2.0;
        instance.transform.rotation_deg = 90.0;
        let Some(PlaceholderGeometry::Declared(b)) = placeholder_geometry(instance, declared)
        else {
            panic!("declared box")
        };
        assert!((b.min_x - 1600.0).abs() < 1e-9);
        assert!((b.max_x - 2400.0).abs() < 1e-9);
        assert!((b.min_y + 600.0).abs() < 1e-9);
        assert!((b.max_y - 600.0).abs() < 1e-9);
        instance.transform.rotation_deg = 45.0;
        let Some(PlaceholderGeometry::Declared(b)) = placeholder_geometry(instance, declared)
        else {
            panic!("declared box")
        };
        let half = 1000.0 / 2.0_f64.sqrt();
        assert!((b.min_x - (2000.0 - half)).abs() < 1e-9);
        assert!((b.max_y - half).abs() < 1e-9);
        for bad in [
            Some((0.0, 1.0, 1.0)),
            Some((f64::NAN, 1.0, 1.0)),
            Some((1.0, 1.0, 1_000_001.0)),
        ] {
            assert!(placeholder_geometry(instance, bad).is_none());
        }
        instance.transform.scale_uniform = f64::MAX;
        assert!(placeholder_geometry(instance, declared).is_none());
    }

    /// ED-0106's discriminating fixture. A non-identity transform must leave
    /// the fallback box EXACTLY where an identity transform puts it: the
    /// 1000 mm square is fixed and axis-aligned in plan space, and
    /// `scaleUniform` and `rotationDeg` are ignored for this branch.
    ///
    /// PH1 alone cannot show this, because its instance has an identity
    /// transform and both readings agree there.
    #[test]
    fn a_non_identity_transform_does_not_resize_or_rotate_the_fallback_box() {
        let (_, level, identity) = ph1();
        let transformed = AssetInstance {
            id: Uuid::from_u128(0xA),
            level_id: level,
            digest: format!("sha256:{}", "9a".repeat(32)),
            transform: InstanceTransform {
                translation_mm: [2000.0, 0.0],
                scale_uniform: 3.0,
                rotation_deg: 45.0,
                ..Default::default()
            },
            display_name: None,
            unknown: Default::default(),
        };
        let scaled_and_rotated = placeholder_box(&transformed).expect("finite origin");
        assert_eq!(
            scaled_and_rotated, identity.bounds,
            "scaleUniform and rotationDeg do not reach the fallback box"
        );
        // Named explicitly so the failure says which reading broke: scaling by
        // 3 would give a 3000 mm span, and rotating a 1000 mm square by 45
        // degrees would give a bounding box about 1414 mm across.
        assert_eq!(scaled_and_rotated.width(), PLACEHOLDER_FALLBACK_MM);
        assert_eq!(scaled_and_rotated.height(), PLACEHOLDER_FALLBACK_MM);
    }

    /// Root's decision, made checkable: candidacy comes from the caller's
    /// per-instance resolution result, never from the mere existence of an
    /// instance in the document. An instance the resolver did not report is
    /// invisible to both queries.
    #[test]
    fn an_instance_absent_from_the_snapshot_is_not_a_candidate() {
        let (doc, level, ph) = ph1();
        assert_eq!(
            doc.asset_instances.len(),
            1,
            "the instance IS in the document"
        );
        let over_it = Aabb::from_corners(P(1400.0, -600.0), P(2600.0, 600.0));
        assert!(
            marquee_with(&doc, level, over_it, &[]).is_empty(),
            "an empty resolution snapshot yields no placeholder candidates"
        );
        assert!(hit_test_with(&doc, level, P(2000.0, 0.0), 0.0, &[]).is_empty());
        // The same document with the verdict supplied does select it, so this
        // is not passing because the fixture never matches.
        assert_eq!(marquee_with(&doc, level, over_it, &[ph]), vec![ph.instance]);
    }

    /// R7.3.2's marquee key spans classes: min corner first, class rank only
    /// as a tie-break. Its own fixture, since PH1 deliberately has no walls.
    #[test]
    fn placeholder_and_region_candidates_share_one_marquee_key_order() {
        let (mut doc, level, ph) = ph1();
        // Box min corners (0,-100) and (3000,-100), either side of the
        // placeholder's (1500,-500) in x.
        doc.walls
            .push(Wall::new(level, P(0.0, 0.0), P(500.0, 0.0), 200.0, 2500.0));
        doc.walls.push(Wall::new(
            level,
            P(3000.0, 0.0),
            P(3500.0, 0.0),
            200.0,
            2500.0,
        ));
        let all = Aabb::from_corners(P(-500.0, -1000.0), P(4000.0, 1000.0));
        assert_eq!(
            marquee_with(&doc, level, all, &[ph]),
            vec![doc.walls[0].id, ph.instance, doc.walls[1].id],
            "ascending min corner interleaves the classes, never groups them"
        );
    }

    /// Promised to the issue-0026 worker: their delivery order is not
    /// observable in either answer (R6.3.3), so they are free to change it.
    #[test]
    fn snapshot_order_is_not_observable_in_either_query() {
        let (doc, level, _) = ph1();
        let a = Placeholder {
            instance: Uuid::from_u128(0x1),
            bounds: Aabb::from_corners(P(0.0, 0.0), P(100.0, 100.0)),
        };
        let b = Placeholder {
            instance: Uuid::from_u128(0x2),
            bounds: Aabb::from_corners(P(50.0, 0.0), P(150.0, 100.0)),
        };
        let rect = Aabb::from_corners(P(-10.0, -10.0), P(200.0, 200.0));
        assert_eq!(
            marquee_with(&doc, level, rect, &[a, b]),
            marquee_with(&doc, level, rect, &[b, a])
        );
        let at = P(75.0, 50.0);
        assert_eq!(
            hit_test_with(&doc, level, at, 10.0, &[a, b]),
            hit_test_with(&doc, level, at, 10.0, &[b, a])
        );
    }

    /// R7.1.3 ranks `placeholder-box` LAST, so a wall region at the same point
    /// outranks it and a click there selects the wall.
    #[test]
    fn a_region_outranks_a_placeholder_box_at_the_same_point() {
        let (mut doc, level, ph) = ph1();
        doc.walls.push(Wall::new(
            level,
            P(1500.0, 0.0),
            P(2500.0, 0.0),
            200.0,
            2500.0,
        ));
        let hits = hit_test_with(&doc, level, P(2000.0, 0.0), 0.0, &[ph]);
        assert_eq!(hits[0].class, HitClass::Region);
        assert_eq!(hits[0].entity, doc.walls[0].id);
        assert_eq!(hits[1].class, HitClass::PlaceholderBox);
    }

    /// Two boxes order by min corner, NOT by distance to the query, which is
    /// the whole reason `rank` gives this class its own key. Here the nearer
    /// box has the larger min corner, so a distance-first order would swap
    /// them.
    #[test]
    fn placeholder_boxes_order_by_min_corner_not_by_distance() {
        let (doc, level, _) = ph1();
        let far_left = Placeholder {
            instance: Uuid::from_u128(0x1),
            bounds: Aabb::from_corners(P(0.0, 0.0), P(100.0, 100.0)),
        };
        let near_right = Placeholder {
            instance: Uuid::from_u128(0x2),
            bounds: Aabb::from_corners(P(150.0, 0.0), P(250.0, 100.0)),
        };
        // Query sits just right of the second box: it is nearer to it.
        let at = P(260.0, 50.0);
        let hits = hit_test_with(&doc, level, at, 300.0, &[far_left, near_right]);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0].entity, far_left.instance,
            "the smaller min corner ranks first even though it is further away"
        );
        assert!(hits[0].distance_squared > hits[1].distance_squared);
    }

    /// Radius admission is EXACT at the boundary, which the clamp buys: the
    /// closest point of an axis-aligned box is a coordinate of the query or a
    /// box bound, never an arithmetic result, so this class does not inherit
    /// the region path's rounded projection.
    ///
    /// Level filtering is not tested here because it is not this layer's: the
    /// snapshot is per level, so an instance on another level is one the
    /// CALLER must not pass.
    #[test]
    fn a_placeholder_box_admission_is_exact_at_the_radius_boundary() {
        let (doc, level, ph) = ph1();
        // 100 mm left of the box's left face at y = 0.
        let at = P(1400.0, 0.0);
        assert_eq!(hit_test_with(&doc, level, at, 100.0, &[ph]).len(), 1);
        assert!(hit_test_with(&doc, level, at, 99.9, &[ph]).is_empty());
    }

    // ---- marquee (R7.3) ----

    #[test]
    fn a_reused_index_answers_exactly_as_the_one_shot_call_does() {
        // The preview path and the click path must never disagree; if they
        // could, a drag would highlight one thing and the click select another.
        let (doc, level) = l_shape();
        let index = HitIndex::build(&doc, level);
        for x in (-500..5000).step_by(311) {
            for y in (-500..3500).step_by(407) {
                let p = P(x as f64, y as f64);
                assert_eq!(
                    index.hit_test(p, 250.0),
                    hit_test(&doc, level, p, 250.0),
                    "p={p:?}"
                );
            }
        }
    }

    #[test]
    fn a_marquee_selects_every_intersecting_wall() {
        let (doc, level) = l_shape();
        let all = marquee(
            &doc,
            level,
            Aabb::from_corners(P(-500.0, -500.0), P(5000.0, 4000.0)),
        );
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn a_marquee_touching_only_a_boundary_still_selects() {
        // R7.3.1: intersection is boundary-inclusive under exact-real
        // semantics; tangential contact selects.
        let (doc, level) = l_shape();
        // The rectangle's right edge lies exactly on wall 0's left face.
        let touching = marquee(
            &doc,
            level,
            Aabb::from_corners(P(-1000.0, -100.0), P(0.0, 100.0)),
        );
        assert_eq!(touching, vec![doc.walls[0].id]);
    }

    /// R7.5.4 and R7.1.3: selection spans the ACTIVE level only. Fixture L4
    /// puts the rectangle over both levels' walls deliberately, so that
    /// exclusion is what the answer rests on rather than a lucky miss.
    #[test]
    fn a_marquee_never_reaches_another_level() {
        let (mut doc, level) = l_shape();
        let upper = Level::new("Level 2");
        let upper_id = upper.id;
        doc.levels.push(upper);
        // Identical XY to wall 0, one level up (fixture F24's shape).
        doc.walls.push(Wall::new(
            upper_id,
            P(0.0, 0.0),
            P(4000.0, 0.0),
            200.0,
            2500.0,
        ));
        let over_everything = Aabb::from_corners(P(-500.0, -500.0), P(5000.0, 4000.0));

        let active = marquee(&doc, level, over_everything);
        assert_eq!(active, vec![doc.walls[0].id, doc.walls[1].id]);
        assert!(
            !active.contains(&doc.walls[2].id),
            "an inactive level's wall is invisible to the marquee, not merely missed"
        );
        // The same rectangle answered against the upper level returns that
        // wall and nothing from L1, so the filter is a level filter and not a
        // fixture that happens to select two walls.
        assert_eq!(
            marquee(&doc, upper_id, over_everything),
            vec![doc.walls[2].id]
        );
    }

    #[test]
    fn a_marquee_that_misses_selects_nothing() {
        let (doc, level) = l_shape();
        assert!(marquee(
            &doc,
            level,
            Aabb::from_corners(P(0.0, 1000.0), P(1000.0, 2000.0))
        )
        .is_empty());
    }

    #[test]
    fn a_marquee_inside_a_diagonal_walls_bounding_box_but_off_the_wall_misses() {
        // The case a bounding-box test alone gets WRONG. A diagonal wall's box
        // covers most of the square it spans, but the wall itself is a thin
        // band through the middle — a rectangle in the corner of that box
        // touches the box and not the wall.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            P(0.0, 0.0),
            P(5000.0, 5000.0),
            100.0,
            2500.0,
        ));
        let corner = Aabb::from_corners(P(3500.0, 500.0), P(4500.0, 1500.0));
        assert!(
            marquee(&doc, level, corner).is_empty(),
            "the wall's bounding box covers this, the wall does not"
        );
        // And the same rectangle moved onto the wall DOES select it, so the
        // test is not passing merely because nothing ever matches.
        let on_it = Aabb::from_corners(P(3500.0, 3500.0), P(4500.0, 4500.0));
        assert_eq!(marquee(&doc, level, on_it), vec![doc.walls[0].id]);
    }

    #[test]
    fn marquee_order_is_by_marquee_key_not_document_order() {
        // R7.3.2 makes PRIMARY the LAST entity of this order, so the order is
        // observable and must not depend on how the document happens to be
        // arranged.
        let (doc, level) = l_shape();
        let rect = Aabb::from_corners(P(-500.0, -500.0), P(5000.0, 4000.0));
        let forward = marquee(&doc, level, rect);
        let mut reversed = doc.clone();
        reversed.walls.reverse();
        assert_eq!(forward, marquee(&reversed, level, rect));
        // Wall 0's box starts at x = 0, wall 1's at x = 3900.
        assert_eq!(forward, vec![doc.walls[0].id, doc.walls[1].id]);
    }

    #[test]
    fn a_zero_area_marquee_degenerates_to_a_point_hit() {
        // R7.3.2. Falling through to box intersection would select every wall
        // whose BOUNDING BOX contains the point, which is a different answer:
        // the diagonal wall below has a box covering the origin but does not
        // itself reach it.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            P(500.0, 500.0),
            P(5000.0, 5000.0),
            100.0,
            2500.0,
        ));
        let at_origin = Aabb::from_corners(P(600.0, 4000.0), P(600.0, 4000.0));
        assert!(marquee(&doc, level, at_origin).is_empty());
        let on_the_wall = Aabb::from_corners(P(1000.0, 1000.0), P(1000.0, 1000.0));
        assert_eq!(marquee(&doc, level, on_the_wall), vec![doc.walls[0].id]);
    }

    #[test]
    fn a_zero_area_marquee_at_a_join_node_selects_the_join() {
        // The case that DISCRIMINATES R7.3.2's point-hit reading from the
        // general intersection path. Both would select something here; only
        // point-hit semantics return the join's identity rather than the two
        // walls whose rectangles cover the node.
        let (doc, level) = l_shape();
        let at_node = Aabb::from_corners(P(4000.0, 0.0), P(4000.0, 0.0));
        assert_eq!(marquee(&doc, level, at_node), vec![doc.joins[0].id]);
    }

    // ---- very large finite marquee corners (R7.3.2 admits them) ----

    /// The witness: a thin VERTICAL strip through the middle of a wall whose y
    /// extent is `±1e308`. Neither short-circuit stage can decide it — no wall
    /// vertex is inside the strip, no strip corner is inside the wall — so it
    /// rests on the crossing test, which overflowed to `NaN` and reported no
    /// crossing. Before the clip this returned nothing.
    #[test]
    fn a_huge_vertical_band_through_a_wall_still_selects_it() {
        let (doc, level) = l_shape();
        let strip = Aabb::from_corners(P(2000.0, -1e308), P(2001.0, 1e308));
        assert_eq!(marquee(&doc, level, strip), vec![doc.walls[0].id]);
        // The bounded query it must agree with.
        let bounded = Aabb::from_corners(P(2000.0, -1.1e7), P(2001.0, 1.1e7));
        assert_eq!(marquee(&doc, level, strip), marquee(&doc, level, bounded));
    }

    /// The transpose of the witness: a thin HORIZONTAL band through the middle
    /// of the VERTICAL wall, y ∈ [1000, 1001] against wall 1's
    /// x ∈ [3900, 4100], y ∈ [0, 3000]. Its vertices sit at y = 0 and
    /// y = 3000, so none is inside the band, and the band's corners are at
    /// x = ±1e308, so none is inside the wall — the same overflowing crossing
    /// test decides it, on the other axis. Wall 0 lies below the band
    /// entirely, which keeps the expected answer a single wall.
    #[test]
    fn a_huge_horizontal_band_through_a_wall_still_selects_it() {
        let (doc, level) = l_shape();
        let band = Aabb::from_corners(P(-1e308, 1000.0), P(1e308, 1001.0));
        assert_eq!(marquee(&doc, level, band), vec![doc.walls[1].id]);
        let bounded = Aabb::from_corners(P(-1.1e7, 1000.0), P(1.1e7, 1001.0));
        assert_eq!(marquee(&doc, level, band), marquee(&doc, level, bounded));
    }

    /// Clipping must not manufacture hits. This band is enormous on x and
    /// nowhere near the plan on y.
    #[test]
    fn a_huge_band_that_misses_everything_still_selects_nothing() {
        let (doc, level) = l_shape();
        assert!(
            marquee(
                &doc,
                level,
                Aabb::from_corners(P(-1e308, 1e300), P(1e308, 1e308))
            )
            .is_empty(),
            "a band a googol millimetres above the plan selects nothing"
        );
    }

    /// A query enclosing the whole plan selects everything, in key order.
    #[test]
    fn a_huge_enclosing_marquee_selects_every_wall_in_key_order() {
        let (doc, level) = l_shape();
        let huge = Aabb::from_corners(P(-1e308, -1e308), P(1e308, 1e308));
        assert_eq!(
            marquee(&doc, level, huge),
            vec![doc.walls[0].id, doc.walls[1].id]
        );
    }

    /// R7.3.1's boundary-inclusive rule survives the clip: this band's upper
    /// edge lies exactly on wall 0's bottom face, y = −100, and touching
    /// selects.
    #[test]
    fn a_huge_band_tangent_to_a_wall_face_selects_it() {
        let (doc, level) = l_shape();
        let tangent = Aabb::from_corners(P(-1e308, -1e308), P(1e308, -100.0));
        assert_eq!(marquee(&doc, level, tangent), vec![doc.walls[0].id]);
        // One millimetre lower is a genuine miss, so the assertion above is
        // not passing merely because everything matches.
        let below = Aabb::from_corners(P(-1e308, -1e308), P(1e308, -101.0));
        assert!(marquee(&doc, level, below).is_empty());
    }

    /// R7.3.1 is direction-independent, and `Aabb::from_corners` normalizes:
    /// the answer cannot depend on which opposite corners the caller names.
    #[test]
    fn huge_corners_answer_the_same_in_either_order() {
        let (doc, level) = l_shape();
        let forward = marquee(
            &doc,
            level,
            Aabb::from_corners(P(2000.0, -1e308), P(2001.0, 1e308)),
        );
        let reversed = marquee(
            &doc,
            level,
            Aabb::from_corners(P(2001.0, 1e308), P(2000.0, -1e308)),
        );
        assert_eq!(forward, reversed);
        assert_eq!(forward, vec![doc.walls[0].id]);
    }

    /// The marquee key comes from the CANDIDATE's bounds, never from the
    /// query, so clipping cannot reorder the result — and R7.3.2 derives
    /// PRIMARY from that order, which makes it observable.
    #[test]
    fn clipping_leaves_marquee_order_unchanged() {
        let (doc, level) = l_shape();
        let huge = Aabb::from_corners(P(-1e308, -1e308), P(1e308, 1e308));
        let bounded = Aabb::from_corners(P(-1.1e7, -1.1e7), P(1.1e7, 1.1e7));
        let ordinary = Aabb::from_corners(P(-500.0, -500.0), P(5000.0, 4000.0));
        assert_eq!(marquee(&doc, level, huge), marquee(&doc, level, bounded));
        assert_eq!(marquee(&doc, level, huge), marquee(&doc, level, ordinary));
        let mut reversed = doc.clone();
        reversed.walls.reverse();
        assert_eq!(
            marquee(&doc, level, huge),
            marquee(&reversed, level, huge),
            "document order must not leak into the answer"
        );
    }

    /// The R7.3.2 degenerate branch is classified on the ORIGINAL rectangle.
    /// A huge point selects nothing — correctly, nothing is out there.
    #[test]
    fn a_huge_degenerate_point_selects_nothing() {
        let (doc, level) = l_shape();
        let huge_point = Aabb::from_corners(P(1e308, 1e308), P(1e308, 1e308));
        assert!(marquee(&doc, level, huge_point).is_empty());
    }

    /// The classification guard with teeth: a LINE rectangle that crosses both
    /// walls keeps R7.3.1 rectangle semantics and returns both of them, while
    /// point-hit semantics at the same min corner return ONE entity — the join
    /// that outranks both regions there. Reinterpreting a line rectangle as a
    /// point, whether by clipping or by rereading R7.3.2, changes this answer.
    ///
    /// R7.3.2a settles the reading this asserts (fixture L13): mathematically
    /// a line rectangle has zero area, but "at its corner" is single-valued
    /// only when the two corners coincide, so the degeneracy that takes
    /// point-hit semantics is COINCIDENT CORNERS and nothing wider.
    #[test]
    fn a_line_rectangle_keeps_rectangle_semantics() {
        // Fixture L13 in full: `l_shape` is F2, and the two inputs below are
        // L13's Result B and Result A, which share the corner (4000, 0).
        let (doc, level) = l_shape();
        let line = Aabb::from_corners(P(4000.0, 0.0), P(4000.0, 500.0));
        assert_eq!(line.width(), 0.0);
        assert!(line.height() > 0.0);
        assert_eq!(
            marquee(&doc, level, line),
            vec![doc.walls[0].id, doc.walls[1].id],
            "a line rectangle selects every region it meets"
        );
        // The same corner under point-hit semantics answers differently, which
        // is what makes the assertion above a real constraint.
        let at_corner = hit_test(&doc, level, P(4000.0, 0.0), 0.0);
        assert_eq!(at_corner[0].entity, doc.joins[0].id);
        assert_eq!(
            marquee(
                &doc,
                level,
                Aabb::from_corners(P(4000.0, 0.0), P(4000.0, 0.0))
            ),
            vec![doc.joins[0].id],
            "L13 Result A: a true point still degenerates to point-hit semantics"
        );
    }

    /// A document whose coordinates are OUT of bounds is not something this
    /// function may quietly re-answer: the limit box is widened by the
    /// candidate's own bounds precisely so a wall beyond the envelope is still
    /// selected by a query that reaches it.
    #[test]
    fn a_wall_beyond_the_envelope_is_still_selected() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let far = 5.0 * domowik_format::DERIVED_COORD_BOUND_MM;
        doc.walls.push(Wall::new(
            level,
            P(far, 0.0),
            P(far + 4000.0, 0.0),
            200.0,
            2500.0,
        ));
        assert!(
            !doc.validate().is_empty(),
            "the fixture is deliberately out of bounds"
        );
        let over_it = Aabb::from_corners(P(far + 1000.0, -500.0), P(far + 2000.0, 500.0));
        assert_eq!(marquee(&doc, level, over_it), vec![doc.walls[0].id]);
    }

    #[test]
    fn a_marquee_wholly_inside_one_wall_selects_it() {
        let (doc, level) = l_shape();
        let inside = marquee(
            &doc,
            level,
            Aabb::from_corners(P(1000.0, -50.0), P(1200.0, 50.0)),
        );
        assert_eq!(inside, vec![doc.walls[0].id]);
    }

    #[test]
    fn a_marquee_crossing_a_wall_without_containing_a_vertex_selects_it() {
        // The case a vertex-containment test alone misses: a thin rectangle
        // straddling the wall, with no corner of either inside the other.
        let (doc, level) = l_shape();
        let crossing = marquee(
            &doc,
            level,
            Aabb::from_corners(P(2000.0, -500.0), P(2100.0, 500.0)),
        );
        assert_eq!(crossing, vec![doc.walls[0].id]);
    }
}
