//! Hit testing and marquee selection.
//!
//! Implements the wall/join hit classes and ordering keys of BEHAVIOR-SPEC
//! R7.1.3 and R7.3.1 marquee queries over [`crate::spatial`]. Distances and
//! projections for regions currently use binary64, not exact-real arithmetic.
//! Endpoint/join radius admission and distance ordering are exact for stored
//! finite coordinates; issue 0012 retains the region and placeholder gaps.
//!
//! # The ordering is total, and that is the whole point
//!
//! R7.2.2 makes repeated clicks CYCLE through the candidates at a point, and
//! R6.3.3 forbids delivery order, map iteration order, or frame history from
//! being observable in the result. A ranking that leaves two candidates
//! comparing equal would cycle in whatever order the input happened to arrive
//! in — so every level of the comparison below is present, including the ones
//! that almost never decide anything.

use crate::polygon::{canonical_ring, compare_rings, Winding};
use crate::spatial::{Aabb, SpatialIndex};
use crate::walls::WallSolid;
use domowik_format::{AssetInstance, Document, Point, DERIVED_COORD_BOUND_MM};
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
    /// A binary64 display value, potentially infinite after squaring. Endpoint
    /// and join ranking does not use this rounded value. Opening and wall
    /// region ranking and projection remain approximate under issue 0012.
    pub distance_squared: f64,
}

impl Hit {
    /// Class, squared distance, closest point, region tuple, then ID.
    ///
    /// `PlaceholderBox` leaves that chain after the class, because R7.1.3
    /// gives it a key of its own: exact min corner, then stable instance ID,
    /// with NO distance term. Ranking it by distance would be a different
    /// order whenever a nearer box has the larger min corner.
    fn rank(
        &self,
        other: &Self,
        origin: Point,
        region_order: impl FnOnce() -> Ordering,
    ) -> Ordering {
        let class = self.class.cmp(&other.class);
        if class != Ordering::Equal {
            return class;
        }
        if self.class == HitClass::PlaceholderBox {
            return region_order().then_with(|| self.entity.cmp(&other.entity));
        }
        self.class
            .cmp(&other.class)
            .then_with(|| {
                if self.class != HitClass::Region && self.class != HitClass::Opening {
                    // Admission has already checked that these coordinates
                    // are finite. Different classes never reach this closure.
                    // Shared endpoint coordinates imply exactly equal distance;
                    // dense corners need no allocating fallback for this tie.
                    if self.closest == other.closest {
                        return Ordering::Equal;
                    }
                    return crate::distance::compare_squared(origin, self.closest, other.closest)
                        .expect("admitted endpoint coordinates are finite");
                }
                self.distance_squared
                    .partial_cmp(&other.distance_squared)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| {
                (self.closest.x, self.closest.y)
                    .partial_cmp(&(other.closest.x, other.closest.y))
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(region_order)
            .then_with(|| self.entity.cmp(&other.entity))
    }
}

/// The side of the fallback placeholder box, in millimetres (§9.6).
pub const PLACEHOLDER_FALLBACK_MM: f64 = 1000.0;

/// The deterministic placeholder box of BEHAVIOR-SPEC §9.6, for an instance
/// whose descriptor declares no dimensions.
///
/// A [`PLACEHOLDER_FALLBACK_MM`] square, AXIS-ALIGNED IN PLAN SPACE, centred
/// on the instance origin. `scaleUniform` and `rotationDeg` are deliberately
/// IGNORED (ED-0106): §9.6 says a FIXED box and an AXIS-ALIGNED one, and a box
/// scaled or rotated to match content would assert knowledge of content that
/// could not be decoded — which is the one thing a degraded placeholder must
/// not do. Translation is applied, because "centred on the instance origin" is
/// `translationMm`.
///
/// That ignoring is scoped to THIS branch. The descriptor-dimensions branch of
/// §9.6 is owed to asset resolution and is not implemented here; whether
/// declared dimensions scale is a separate question and remains open.
///
/// Returns `None` for a non-finite origin, which the document model already
/// rejects but which this function must not assume, since it is public.
/// The §9.6 geometry for an instance, given what its descriptor declared.
///
/// Two variants because §9.6 has two branches and only one of them is decided.
/// A caller must therefore SEE the undecided case rather than receive a box
/// that quietly guessed, or a `None` that quietly drops the instance out of
/// selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaceholderGeometry {
    /// No declared dimensions: the fixed 1000 mm box of ED-0106.
    Fallback(Aabb),
    /// The descriptor DID declare dimensions, and what to do with them is not
    /// settled. ED-0106 decided only that the FALLBACK ignores `scaleUniform`
    /// and `rotationDeg`, and said in terms that whether declared dimensions
    /// scale is a separate, open question.
    ///
    /// Deliberately not a box. Building one here would answer that question by
    /// implementation, which is the failure mode ED-0106 exists to prevent.
    ///
    /// Carries all THREE declared dimensions, unprojected, because choosing
    /// two of them is part of what is undecided. A descriptor declares width,
    /// height and depth; §9.6's box is a PLAN footprint, and the specification
    /// does not say which axes map to it. The naive reading — width by height
    /// — is very likely wrong for a floor plan, where the footprint is width
    /// by depth and height is the vertical dimension. Projecting here would
    /// bury that choice in geometry code.
    DeclaredUndecided {
        width_mm: f64,
        height_mm: f64,
        depth_mm: f64,
    },
}

/// [`placeholder_box`], for an instance whose descriptor may declare
/// dimensions.
///
/// `declared_mm` is the descriptor's own `placeholderDimensions` as
/// `(width, height, depth)` in millimetres, or `None` when it declared none.
/// Plain `f64`s rather than an asset type: `domowik-geometry` must not depend
/// on the asset store, so the caller copies three numbers across.
pub fn placeholder_geometry(
    instance: &AssetInstance,
    declared_mm: Option<(f64, f64, f64)>,
) -> Option<PlaceholderGeometry> {
    match declared_mm {
        None => placeholder_box(instance).map(PlaceholderGeometry::Fallback),
        Some((width_mm, height_mm, depth_mm)) => Some(PlaceholderGeometry::DeclaredUndecided {
            width_mm,
            height_mm,
            depth_mm,
        }),
    }
}

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
    /// Wall polygons by id, so a query never re-derives them.
    regions: Vec<(Uuid, Vec<Point>)>,
    region_index: SpatialIndex<usize>,
    openings: Vec<(Uuid, Vec<Point>)>,
    opening_index: SpatialIndex<usize>,
    endpoints: SpatialIndex<(Uuid, HitClass, Point)>,
    /// The caller's degraded instances for this level. Empty unless the query
    /// was built with a resolution snapshot.
    placeholders: Vec<Placeholder>,
    placeholder_index: SpatialIndex<usize>,
}

/// The class-specific tail of the R7.1.3 key, kept beside each candidate
/// rather than inside [`Hit`]: a region's is its canonical ring, which is
/// expensive to carry, and a placeholder's is its min corner.
#[derive(Debug, Clone, Copy)]
enum Tail {
    Region(usize),
    Opening(usize),
    Placeholder(Aabb),
    None,
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
        let regions: Vec<(Uuid, Vec<Point>)> = doc
            .walls_on_level(level_id)
            .filter_map(|w| {
                let poly = WallSolid::new(w.start(), w.end(), w.thickness_mm).rectangle()?;
                Some((w.id, canonical_ring(&poly, Winding::CounterClockwise)?))
            })
            .collect();
        let region_index = SpatialIndex::build(
            regions
                .iter()
                .enumerate()
                .filter_map(|(i, (_, poly))| Aabb::of(poly).map(|b| (b, i))),
        );
        // The same aperture rectangle rendered by the bridge: the host axis
        // interval occupied by the opening, at the host's full thickness.
        let host_walls: HashMap<_, _> = if doc.openings.is_empty() {
            HashMap::new()
        } else {
            doc.walls_on_level(level_id)
                .map(|wall| (wall.id, wall))
                .collect()
        };
        let openings: Vec<_> = doc
            .openings
            .iter()
            .filter_map(|opening| {
                let wall = host_walls.get(&opening.host_wall_id)?;
                let (s0, s1) = opening.span(wall.length_mm())?;
                let along = |t: f64| {
                    Point::new(
                        wall.start().x + (wall.end().x - wall.start().x) * t,
                        wall.start().y + (wall.end().y - wall.start().y) * t,
                    )
                };
                let poly = WallSolid::new(along(s0), along(s1), wall.thickness_mm).rectangle()?;
                Some((
                    opening.id,
                    canonical_ring(&poly, Winding::CounterClockwise)?,
                ))
            })
            .collect();
        let opening_index = SpatialIndex::build(
            openings
                .iter()
                .enumerate()
                .filter_map(|(i, (_, poly))| Aabb::of(poly).map(|b| (b, i))),
        );
        let placeholder_index =
            SpatialIndex::build(placeholders.iter().enumerate().map(|(i, p)| (p.bounds, i)));
        Self {
            regions,
            region_index,
            openings,
            opening_index,
            endpoints: endpoint_index(doc, level_id),
            placeholders: placeholders.to_vec(),
            placeholder_index,
        }
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
    /// no hits. Radius boundaries are inclusive; endpoint/join admission is
    /// exact over finite stored coordinates, while regions remain approximate.
    pub fn hit_test(&self, point: Point, radius_mm: f64) -> Vec<Hit> {
        let mut hits = Vec::new();
        if !point.x.is_finite() || !point.y.is_finite() || !radius_mm.is_finite() || radius_mm < 0.0
        {
            return Vec::new();
        }
        let r2 = radius_mm * radius_mm;

        // Endpoints. A joined endpoint contributes to its JOIN NODE and never
        // as its own free endpoint (R6.3.2: "Joined endpoints always
        // deduplicate to their join node, never to per-member endpoint
        // candidates").
        // Do not use `near`: its rounded squared-distance filter can discard
        // a candidate before exact admission. The rectangle includes all
        // representable endpoint coordinates in the real radius bounds:
        // rounding a bound inward cannot cross another representable value.
        for (entity, class, at) in self
            .endpoints
            .query(&Aabb::point(point).expanded(radius_mm))
        {
            let d2 = (at.x - point.x).powi(2) + (at.y - point.y).powi(2);
            if crate::distance::within_radius(point, at, radius_mm) == Some(true) {
                hits.push((
                    Hit {
                        class,
                        entity,
                        closest: at,
                        distance_squared: d2,
                    },
                    Tail::None,
                ));
            }
        }

        // Opening bands rank ahead of their host's wall region. Both use the
        // same broad and narrow phase so a nearby wall remains cycleable.
        for i in self.opening_index.near(point, radius_mm) {
            let (id, poly) = &self.openings[i];
            let (closest, d2) = closest_point_on_polygon(poly, point);
            if d2 <= r2 {
                hits.push((
                    Hit {
                        class: HitClass::Opening,
                        entity: *id,
                        closest,
                        distance_squared: d2,
                    },
                    Tail::Opening(i),
                ));
            }
        }

        // Regions: each wall's own centred butt-capped rectangle (R3.0.2),
        // NOT the cluster union.
        for i in self.region_index.near(point, radius_mm) {
            let (id, poly) = &self.regions[i];
            let (closest, d2) = closest_point_on_polygon(poly, point);
            if d2 <= r2 {
                hits.push((
                    Hit {
                        class: HitClass::Region,
                        entity: *id,
                        closest,
                        distance_squared: d2,
                    },
                    Tail::Region(i),
                ));
            }
        }

        // Placeholder boxes: the §9.6 geometry of instances a resolver called
        // degraded (R7.1.3 class 4). The closest point of an axis-aligned box
        // is a CLAMP, so it is a coordinate of the query or a box bound and
        // never an arithmetic result. Admission is therefore exact over finite
        // inputs, the way endpoint admission is, and unlike the region path.
        for i in self.placeholder_index.near(point, radius_mm) {
            let p = self.placeholders[i];
            let closest = Point::new(
                point.x.clamp(p.bounds.min_x, p.bounds.max_x),
                point.y.clamp(p.bounds.min_y, p.bounds.max_y),
            );
            if crate::distance::within_radius(point, closest, radius_mm) == Some(true) {
                hits.push((
                    Hit {
                        class: HitClass::PlaceholderBox,
                        entity: p.instance,
                        closest,
                        distance_squared: (closest.x - point.x).powi(2)
                            + (closest.y - point.y).powi(2),
                    },
                    Tail::Placeholder(p.bounds),
                ));
            }
        }

        // R6.3.2 merges only candidates equal through EVERY R6.3.1 key level.
        // A wall's two free endpoints share class/entity but have different
        // closest-point keys. Using one comparator for sorting and equality
        // both preserves those features and makes true duplicates adjacent.
        let ranked = |(a, at): &(Hit, Tail), (b, bt): &(Hit, Tail)| {
            a.rank(b, point, || match (at, bt) {
                (Tail::Region(ai), Tail::Region(bi)) => {
                    compare_rings(&self.regions[*ai].1, &self.regions[*bi].1)
                }
                (Tail::Opening(ai), Tail::Opening(bi)) => {
                    compare_rings(&self.openings[*ai].1, &self.openings[*bi].1)
                }
                // R7.1.3's placeholder key: exact min corner, then the ID the
                // caller of `rank` appends.
                (Tail::Placeholder(a), Tail::Placeholder(b)) => (a.min_x, a.min_y)
                    .partial_cmp(&(b.min_x, b.min_y))
                    .unwrap_or(Ordering::Equal),
                _ => Ordering::Equal,
            })
        };
        hits.sort_by(|x, y| ranked(x, y));
        // Full-key equality implies class/entity equality. Reject mismatches
        // cheaply before comparing geometry, but still compare every key for
        // shared identities. The unguarded comparison measurably slows dense
        // queries; see docs/evidence/0050-hit-features/MEASURED.md.
        hits.dedup_by(|x, y| {
            x.0.class == y.0.class
                && x.0.entity == y.0.entity
                && ranked(&*x, &*y) == Ordering::Equal
        });
        hits.into_iter().map(|(hit, _)| hit).collect()
    }

    /// The number of indexed wall regions.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// The magnitude a candidate's bounds are compared against when widening the
/// clip limit, from `COORD_BOUND_MM + DIMENSION_CAP_MM` (ED-0100).
///
/// This constant carries NO containment claim about wall geometry. Containment
/// is established per candidate instead, from the bounding box actually
/// computed for its polygon, so the clip needs no error analysis of
/// [`crate::wall_polygon`]'s rounded construction — which would have to
/// account for `thickness_mm / 2.0` underflowing for a subnormal thickness,
/// and for `f64::hypot` carrying no correctly-rounded guarantee across
/// targets. Neither question needs answering for the clip to be exact, so
/// neither is asserted here.
///
/// What the constant does is bound the clipped query's magnitude for ordinary
/// walls: when a candidate's computed bounds fall inside this box, the clipped
/// extents are at most `2 · DERIVED_COORD_BOUND_MM` and the crossing test
/// evaluates them without overflow. A candidate whose computed bounds fall
/// OUTSIDE it remains eligible through the widened limit, but gets no
/// overflow guarantee.
const ENVELOPE: Aabb = Aabb {
    min_x: -DERIVED_COORD_BOUND_MM,
    min_y: -DERIVED_COORD_BOUND_MM,
    max_x: DERIVED_COORD_BOUND_MM,
    max_y: DERIVED_COORD_BOUND_MM,
};

/// `query ∩ limit`, or `None` when the two do not meet.
///
/// Deliberately returns `None` rather than a box with `min > max`: the rest of
/// [`Aabb`] assumes that invariant, and an inverted box silently reports
/// contact it does not have.
fn clip(query: Aabb, limit: Aabb) -> Option<Aabb> {
    let clipped = Aabb {
        min_x: query.min_x.max(limit.min_x),
        min_y: query.min_y.max(limit.min_y),
        max_x: query.max_x.min(limit.max_x),
        max_y: query.max_y.min(limit.max_y),
    };
    (clipped.min_x <= clipped.max_x && clipped.min_y <= clipped.max_y).then_some(clipped)
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
/// Intersection is boundary-inclusive: tangential contact selects.
///
/// # Very large finite corners
///
/// Any FINITE `rect` is accepted and answered, never refused: finiteness is
/// the only validity condition R7.3.2 states. Large corners nonetheless need
/// care, because the narrow-phase crossing test computes its parameters in
/// binary64 ([`crate::arrangement::intersection_param`]) and a query large
/// enough to overflow those products yields `NaN`, which reads as "no
/// crossing". A thin strip through the middle of a wall with a `±1e308`
/// extent then selected NOTHING — no wall vertex was inside the strip and no
/// strip corner inside the wall, so the answer rested on that test alone.
/// Under-selection only: [`Aabb::intersects`] is comparison-only, so the
/// broad phase never wrongly admits a distant rectangle.
///
/// The query is therefore CLIPPED per candidate before the narrow phase, to
/// `L = ENVELOPE ∪ bounds`, where `bounds` is the bounding box actually
/// computed for that candidate's polygon. Since `S ⊆ bounds ⊆ L`,
/// `S ∩ Q = S ∩ (L ∩ Q)` preserves the geometric intersection without an
/// error analysis of how the polygon was constructed. This identity does not
/// make the subsequent rounded intersection arithmetic exact.
///
/// The overflow guarantee is narrower than the containment. When a
/// candidate's computed bounds lie inside [`ENVELOPE`], the clipped extents are at most
/// `2 · DERIVED_COORD_BOUND_MM` and the crossing test evaluates them without
/// overflow. Computed geometry that is itself oversized remains eligible, but such
/// coordinates can overflow the crossing test on their own, whatever the
/// query is.
///
/// Unchanged: the rounded construction of [`crate::wall_polygon`] and the
/// binary64 crossing parameters with their underflow limitations (issue 0012,
/// and the module header).
///
/// # Degenerate rectangles
///
/// The point-hit branch below fires only when BOTH extents are zero, which is
/// what R7.3.2a settled: a LINE rectangle has zero area but two distinct
/// corners, and keeps R7.3.1 intersection semantics (fixture L13).
pub fn marquee(doc: &Document, level_id: Uuid, rect: Aabb) -> Vec<Uuid> {
    marquee_with(doc, level_id, rect, &[])
}

/// [`marquee`], plus the degraded instances a resolver decided are
/// `placeholder-box` candidates on this level (R7.1.3, R7.5.3, §9.6).
///
/// Placeholder boxes join the same keyed list as wall regions, so the R7.3.2
/// marquee key orders the two classes TOGETHER by min corner and only breaks a
/// tie by class rank. Their intersection test is [`Aabb::intersects`], which
/// is comparison-only and therefore boundary-inclusive with no clipping and no
/// overflow exposure — the concern that forced the per-candidate clip for wall
/// polygons does not arise for a box.
pub fn marquee_with(
    doc: &Document,
    level_id: Uuid,
    rect: Aabb,
    placeholders: &[Placeholder],
) -> Vec<Uuid> {
    // Coincident corners degenerate to point-hit semantics at that corner
    // (R7.3.2, as scoped by R7.3.2a).
    //
    // R7.3.1 says a marquee's candidate shapes are wall REGIONS and
    // placeholder boxes, which would make this branch redundant — the general
    // polygon test already answers "which regions contain this point". But
    // point-hit semantics are R7.1.3's, which rank join nodes and free
    // endpoints ABOVE regions and return the JOIN's identity at a node, so
    // the branch changes which entity a drag onto a corner selects.
    // Point-hit semantics select what a CLICK there selects: R7.2.2's
    // top-ranked candidate, one entity, not the whole candidate list. A
    // marquee selects a set, so returning every candidate would be the one
    // reading that makes a degenerate drag select MORE than a click on the
    // same pixel.
    //
    // This classification reads the ORIGINAL rectangle and must stay ahead of
    // the clipping below. Clipping can CREATE a degenerate box — a query
    // spanning `[DERIVED_COORD_BOUND_MM, 1e308]` clips to zero width — and a
    // query that was never a point must not acquire point-hit semantics by
    // being clipped, nor a point be moved onto the envelope.
    if rect.width() == 0.0 && rect.height() == 0.0 {
        let p = Point::new(rect.min_x, rect.min_y);
        return hit_test_with(doc, level_id, p, 0.0, placeholders)
            .into_iter()
            .next()
            .map(|h| h.entity)
            .into_iter()
            .collect();
    }

    let mut keyed: Vec<(f64, f64, u8, Uuid)> = Vec::new();
    for wall in doc.walls_on_level(level_id) {
        let solid = WallSolid::new(wall.start(), wall.end(), wall.thickness_mm);
        let Some(poly) = solid.rectangle() else {
            continue;
        };
        let Some(bounds) = Aabb::of(&poly) else {
            continue;
        };
        if !bounds.intersects(&rect) {
            continue;
        }
        // The broad phase just proved the two boxes meet, and the limit box
        // contains `bounds`, so this intersection is non-empty and the clip
        // cannot yield an inverted box. The guard keeps that invariant local
        // rather than inherited: every other `Aabb` method assumes
        // `min <= max`, and an inverted box would make `intersects` report
        // contact that does not exist.
        let Some(query) = clip(rect, ENVELOPE.union(bounds)) else {
            continue;
        };
        if polygon_intersects_box(&poly, &query) {
            keyed.push((bounds.min_x, bounds.min_y, HitClass::Region as u8, wall.id));
        }
    }

    for p in placeholders {
        if p.bounds.intersects(&rect) {
            keyed.push((
                p.bounds.min_x,
                p.bounds.min_y,
                HitClass::PlaceholderBox as u8,
                p.instance,
            ));
        }
    }

    keyed.sort_by(|a, b| {
        (a.0, a.1)
            .partial_cmp(&(b.0, b.1))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });
    keyed.into_iter().map(|(_, _, _, id)| id).collect()
}

/// Builds the endpoint index: join nodes, then free endpoints.
fn endpoint_index(doc: &Document, level_id: Uuid) -> SpatialIndex<(Uuid, HitClass, Point)> {
    let mut items: Vec<(Aabb, (Uuid, HitClass, Point))> = Vec::new();
    // A set, not a list: membership is asked once per endpoint, and a linear
    // scan there makes the whole build quadratic in the number of joins.
    let mut joined: std::collections::HashSet<(Uuid, domowik_format::Role)> =
        std::collections::HashSet::new();

    for join in doc.joins_on_level(level_id) {
        let mut node: Option<Point> = None;
        for m in &join.members {
            joined.insert((m.wall_id, m.role));
            if node.is_none() {
                node = doc.wall(m.wall_id).map(|w| match m.role {
                    domowik_format::Role::Start => w.start(),
                    domowik_format::Role::End => w.end(),
                });
            }
        }
        if let Some(at) = node {
            items.push((Aabb::point(at), (join.id, HitClass::JoinNode, at)));
        }
    }

    for wall in doc.walls_on_level(level_id) {
        for role in [domowik_format::Role::Start, domowik_format::Role::End] {
            if joined.contains(&(wall.id, role)) {
                continue;
            }
            let at = match role {
                domowik_format::Role::Start => wall.start(),
                domowik_format::Role::End => wall.end(),
            };
            items.push((Aabb::point(at), (wall.id, HitClass::FreeEndpoint, at)));
        }
    }

    SpatialIndex::build(items)
}

/// The closest point of a convex polygon to `p`, with its squared distance.
///
/// Returns `p` itself at distance zero when inside, which R7.1.3 states
/// explicitly for the `region` class: "the closest point is the pointer itself
/// when inside (exact distance zero)".
fn closest_point_on_polygon(poly: &[Point], p: Point) -> (Point, f64) {
    if crate::arrangement::classify_point(p, poly) != crate::arrangement::Containment::Outside {
        return (p, 0.0);
    }
    let mut best = (poly[0], f64::INFINITY);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let c = closest_point_on_segment(a, b, p);
        let d2 = (c.x - p.x).powi(2) + (c.y - p.y).powi(2);
        if d2 < best.1 {
            best = (c, d2);
        }
    }
    best
}

fn closest_point_on_segment(a: Point, b: Point, p: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    Point::new(a.x + dx * t, a.y + dy * t)
}

/// Whether a convex polygon and an axis-aligned box share any point,
/// boundary inclusive.
fn polygon_intersects_box(poly: &[Point], rect: &Aabb) -> bool {
    // Any vertex inside the box, or any box corner inside the polygon, or any
    // polygon edge crossing a box edge. Cheapest test first.
    if poly.iter().any(|p| rect.contains(*p)) {
        return true;
    }
    let corners = [
        Point::new(rect.min_x, rect.min_y),
        Point::new(rect.max_x, rect.min_y),
        Point::new(rect.max_x, rect.max_y),
        Point::new(rect.min_x, rect.max_y),
    ];
    if corners.iter().any(|c| {
        crate::arrangement::classify_point(*c, poly) != crate::arrangement::Containment::Outside
    }) {
        return true;
    }
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        for j in 0..4 {
            let (c, d) = (corners[j], corners[(j + 1) % 4]);
            if crate::arrangement::intersection_param(a, b, c, d).is_some() {
                return true;
            }
        }
    }
    false
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

    fn endpoint_only_index(points: &[Point]) -> HitIndex {
        HitIndex {
            regions: Vec::new(),
            region_index: SpatialIndex::build(Vec::<(Aabb, usize)>::new()),
            openings: Vec::new(),
            opening_index: SpatialIndex::build(Vec::<(Aabb, usize)>::new()),
            placeholders: Vec::new(),
            placeholder_index: SpatialIndex::build(Vec::<(Aabb, usize)>::new()),
            endpoints: SpatialIndex::build(points.iter().enumerate().map(|(i, &at)| {
                (
                    Aabb::point(at),
                    (Uuid::from_u128(i as u128 + 1), HitClass::FreeEndpoint, at),
                )
            })),
        }
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
        let rectangle = WallSolid::new(P(-100.0, 0.0), P(100.0, 0.0), 20.0)
            .rectangle()
            .expect("a rectangle");
        let regions = vec![(
            wall,
            canonical_ring(&rectangle, Winding::CounterClockwise).expect("a ring"),
        )];
        let region_index = SpatialIndex::build(
            regions
                .iter()
                .enumerate()
                .filter_map(|(i, (_, poly))| Aabb::of(poly).map(|b| (b, i))),
        );
        let index = HitIndex {
            regions,
            region_index,
            openings: Vec::new(),
            opening_index: SpatialIndex::build(Vec::<(Aabb, usize)>::new()),
            endpoints: SpatialIndex::build(vec![
                // Equal at EVERY level: these two are one candidate.
                (Aabb::point(origin), (wall, HitClass::FreeEndpoint, origin)),
                (Aabb::point(origin), (wall, HitClass::FreeEndpoint, origin)),
                // Same entity and class, different point: level 3 separates.
                (Aabb::point(offset), (wall, HitClass::FreeEndpoint, offset)),
                // Same entity and point, different class: level 1 separates.
                (Aabb::point(origin), (wall, HitClass::JoinNode, origin)),
            ]),
            placeholders: Vec::new(),
            placeholder_index: SpatialIndex::build(Vec::<(Aabb, usize)>::new()),
        };

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
                    let solid = WallSolid::new(w.start(), w.end(), w.thickness_mm);
                    solid
                        .rectangle()
                        .map(|poly| closest_point_on_polygon(&poly, p).1 <= r * r)
                        .unwrap_or(false)
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

    /// §9.6 has two geometry branches and only the fallback is decided, so
    /// the undecided one must be VISIBLE rather than guessed or dropped.
    #[test]
    fn a_declared_extent_is_reported_undecided_rather_than_boxed() {
        let (doc, _, _) = ph1();
        let instance = &doc.asset_instances[0];

        // No declared dimensions: the settled branch, a plain fallback box.
        match placeholder_geometry(instance, None) {
            Some(PlaceholderGeometry::Fallback(b)) => {
                assert_eq!(b, placeholder_box(instance).unwrap())
            }
            other => panic!("expected the fallback box, got {other:?}"),
        }

        // Declared dimensions: NOT a box. ED-0106 settled the fallback only,
        // and building a box here would answer its open question by
        // implementation.
        match placeholder_geometry(instance, Some((600.0, 2100.0, 400.0))) {
            Some(PlaceholderGeometry::DeclaredUndecided {
                width_mm,
                height_mm,
                depth_mm,
            }) => {
                // All three survive unprojected: a door-like 600 x 2100 x 400
                // has a PLAN footprint of 600 by 400, and picking that pair is
                // exactly what is not yet decided.
                assert_eq!((width_mm, height_mm, depth_mm), (600.0, 2100.0, 400.0));
            }
            other => panic!("expected the undecided branch, got {other:?}"),
        }
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

    /// Clipping really can produce a zero-width box, so the ordering of the
    /// classification is load-bearing rather than theoretical. Asserted on
    /// `clip` directly: an answer-level test cannot show this, because a
    /// query out at the envelope edge selects nothing either way.
    #[test]
    fn clipping_can_produce_a_zero_width_box() {
        let beyond = Aabb::from_corners(P(DERIVED_COORD_BOUND_MM, 0.0), P(1e308, 3000.0));
        assert!(beyond.width() > 0.0, "the query itself is not degenerate");
        let clipped = clip(beyond, ENVELOPE).expect("it meets the envelope");
        assert_eq!(clipped.width(), 0.0, "but its clip is");
        assert_eq!(clipped.height(), 3000.0);
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
        let far = 5.0 * DERIVED_COORD_BOUND_MM;
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
