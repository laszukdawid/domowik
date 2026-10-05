//! BEHAVIOR-SPEC section 6.7 S-series, executed against the core query.
//!
//! Every fixture states its inputs as RAW DEVICE PIXELS and its view as
//! `{z, ox, oy, DPR d}`. R6.7 defines one conversion chain for them, and this
//! file implements that chain once and drives every input through it. The plan
//! millimetres the specification quotes alongside each raw pair are therefore
//! DERIVED here and then checked against the quoted value, rather than typed in
//! as a second literal that could agree with a transcription error (ED-0089).
//!
//! Screen distances are likewise derived: a squared screen distance in logical
//! pixels is the squared plan distance scaled by z^2, which is what R6.0.1 and
//! R6.3.1 mean by comparing in logical pixels after normalization.

use domowik_format::{Document, JoinMember, Point, Role, Wall, WallJoin};
use domowik_geometry::snap::{SnapClass, SnapIndex, SnapQuery};
use domowik_geometry::spatial::Aabb;
use uuid::Uuid;

/// R6.0.2: 12 logical pixels for mouse and stylus, 24 for touch.
const T_MOUSE_PX: f64 = 12.0;
const T_TOUCH_PX: f64 = 24.0;

/// The R6.7 view: `x = (lx - ox)/z`, `y = (oy - ly)/z`, with logical pixels
/// obtained by dividing raw device pixels by the device pixel ratio.
#[derive(Clone, Copy)]
struct View {
    z: f64,
    ox: f64,
    oy: f64,
    dpr: f64,
}

impl View {
    /// "Unless stated, fixtures use z = 0.1, offsets (ox, oy) = (512, 384),
    /// DPR d = 2."
    fn defaults() -> Self {
        Self {
            z: 0.1,
            ox: 512.0,
            oy: 384.0,
            dpr: 2.0,
        }
    }

    fn logical(&self, raw: (f64, f64)) -> (f64, f64) {
        (raw.0 / self.dpr, raw.1 / self.dpr)
    }

    fn plan(&self, raw: (f64, f64)) -> Point {
        let (lx, ly) = self.logical(raw);
        Point::new((lx - self.ox) / self.z, (self.oy - ly) / self.z)
    }

    /// The R6.0.2 plan-space radius T/z for a screen radius in logical pixels.
    fn radius_mm(&self, t_px: f64) -> f64 {
        t_px / self.z
    }

    /// Squared distance in LOGICAL PIXELS between two plan points. The view
    /// transform is a uniform scale by z plus a translation, so a plan
    /// separation of d millimetres is d*z logical pixels.
    fn squared_px(&self, a: Point, b: Point) -> f64 {
        let (dx, dy) = (a.x - b.x, a.y - b.y);
        (dx * dx + dy * dy) * self.z * self.z
    }

    /// A visible rectangle wide enough that no fixture is bounded by it. R6.1.1
    /// bounds axis search by the visible region; none of these fixtures tests
    /// that bound, so they must not be accidentally clipped by it.
    fn wide_rect(&self) -> Aabb {
        Aabb::from_corners(Point::new(-1e6, -1e6), Point::new(1e6, 1e6))
    }
}

fn query<'a>(view: &View, point: Point, t_px: f64) -> SnapQuery<'a> {
    SnapQuery {
        point,
        radius_mm: view.radius_mm(t_px),
        visible_rect_mm: view.wide_rect(),
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

/// F1: one isolated wall `W1` (0,0)->(4000,0), t=200, h=2500.
fn f1() -> (Document, Uuid, Uuid) {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.walls.push(Wall::new(
        level,
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2500.0,
    ));
    let w1 = doc.walls[0].id;
    assert_eq!(doc.validate(), vec![]);
    (doc, level, w1)
}

/// F2: F1 plus `W2` (4000,0)->(4000,3000), auto-joined at (4000,0).
fn f2() -> (Document, Uuid, Uuid, Uuid, Uuid) {
    let (mut doc, level, w1) = f1();
    doc.walls.push(Wall::new(
        level,
        Point::new(4000.0, 0.0),
        Point::new(4000.0, 3000.0),
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

// ---------------------------------------------------------------------------
// S1 — inclusive boundary hit; axis extension outranks grid
// ---------------------------------------------------------------------------

#[test]
fn s1_free_endpoint_is_admitted_at_exactly_the_radius() {
    let view = View::defaults();
    let (doc, level, w1) = f1();
    let index = SnapIndex::build(&doc, level);
    let end = doc.wall(w1).unwrap().end();

    let point = view.plan((1800.0, 768.0));
    assert_eq!(view.logical((1800.0, 768.0)), (900.0, 384.0));
    assert_eq!(point, Point::new(3880.0, 0.0));
    // "squared screen distance to W1.End exactly T^2 = 144 px^2"
    assert_eq!(view.squared_px(point, end), T_MOUSE_PX * T_MOUSE_PX);

    // Inclusive at exact equality, and class order outranks grid regardless,
    // so the grid is switched ON here to prove the class rule rather than
    // assume it.
    let mut req = query(&view, point, T_MOUSE_PX);
    req.grid_enabled = true;
    req.grid_spacing_mm = Some(100.0);
    let winner = &index.query(&req).unwrap()[0];
    assert_eq!(winner.class, SnapClass::FreeEndpoint);
    assert_eq!(winner.at, end);
}

#[test]
fn s1_axis_extension_wins_beyond_the_walls_own_end() {
    let view = View::defaults();
    let (doc, level, w1) = f1();
    let index = SnapIndex::build(&doc, level);

    let point = view.plan((2000.0, 768.0));
    assert_eq!(point, Point::new(4880.0, 0.0));

    let mut req = query(&view, point, T_MOUSE_PX);
    req.grid_enabled = true;
    req.grid_spacing_mm = Some(100.0);
    let candidates = index.query(&req).unwrap();
    let winner = &candidates[0];
    assert_eq!(winner.class, SnapClass::Axis);
    assert_eq!(
        winner.at, point,
        "the projection of an on-axis pointer is itself"
    );
    assert_eq!(view.squared_px(point, winner.at), 0.0);
    assert_eq!(winner.entity, Some(w1));

    // The two neighbouring lattice points quoted by the fixture lose by class
    // even though one of them is nearer than 4 px^2 would suggest.
    for lattice in [Point::new(4900.0, 0.0), Point::new(4800.0, 0.0)] {
        let grid = candidates
            .iter()
            .find(|c| c.class == SnapClass::Grid && c.at == lattice)
            .expect("both lattice neighbours qualify");
        assert!(view.squared_px(point, grid.at) > 0.0);
    }
}

// ---------------------------------------------------------------------------
// S2 — equal distance broken by tuple, then by identity
// ---------------------------------------------------------------------------

#[test]
fn s2_an_exact_tie_breaks_by_coordinate_tuple() {
    let view = View::defaults();
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.walls.push(Wall::new(
        level,
        Point::new(-600.0, 0.0),
        Point::new(940.0, 0.0),
        200.0,
        2500.0,
    ));
    doc.walls.push(Wall::new(
        level,
        Point::new(1060.0, 0.0),
        Point::new(2600.0, 0.0),
        200.0,
        2500.0,
    ));
    // The fixture states the two walls stand cleanly: a positive 120 mm gap,
    // no contact, no area overlap, no warnings.
    assert_eq!(doc.validate(), vec![]);
    assert!(doc.warnings().is_empty(), "{:?}", doc.warnings());
    let (a_end, b_start) = (doc.walls[0].end(), doc.walls[1].start());
    let index = SnapIndex::build(&doc, level);

    let point = view.plan((1224.0, 768.0));
    assert_eq!(view.logical((1224.0, 768.0)), (612.0, 384.0));
    assert_eq!(point, Point::new(1000.0, 0.0));
    // Both endpoints sit at 60^2 * z^2 = 36 px^2.
    assert_eq!(
        view.squared_px(point, a_end),
        view.squared_px(point, b_start)
    );
    assert_eq!(view.squared_px(point, a_end), 36.0);

    let winner = &index.query(&query(&view, point, T_MOUSE_PX)).unwrap()[0];
    assert_eq!(winner.class, SnapClass::FreeEndpoint);
    assert!((a_end.x, a_end.y) < (b_start.x, b_start.y));
    assert_eq!(winner.at, a_end);
}

#[test]
fn s2_coincident_free_endpoints_need_the_identity_level() {
    // The contrast case: two free endpoints whose keys agree through level 3,
    // so only the ascending stable id separates them. Dedupe does not apply.
    let view = View::defaults();
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.walls.push(Wall::new(
        level,
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2500.0,
    ));
    doc.walls.push(Wall::new(
        level,
        Point::new(4000.0, 0.0),
        Point::new(4000.0, 3000.0),
        200.0,
        2500.0,
    ));
    let (w1, w2) = (doc.walls[0].id, doc.walls[1].id);
    let index = SnapIndex::build(&doc, level);
    let shared = doc.walls[0].end();
    assert_eq!(shared, doc.walls[1].start());

    let candidates = index.query(&query(&view, shared, T_MOUSE_PX)).unwrap();
    let endpoints: Vec<_> = candidates
        .iter()
        .filter(|c| c.class == SnapClass::FreeEndpoint && c.at == shared)
        .collect();
    assert_eq!(
        endpoints.len(),
        2,
        "no join exists, so neither dedupes away"
    );
    let first = endpoints[0].entity.unwrap();
    assert_eq!(first, w1.min(w2), "ascending stable id picks the lower");
}

// ---------------------------------------------------------------------------
// S3 — joined endpoints deduplicate to one node candidate
// ---------------------------------------------------------------------------

#[test]
fn s3_joined_endpoints_yield_exactly_one_node_candidate() {
    let view = View::defaults();
    let (doc, level, _w1, _w2, join) = f2();
    let index = SnapIndex::build(&doc, level);

    let point = view.plan((1826.0, 770.0));
    assert_eq!(view.logical((1826.0, 770.0)), (913.0, 385.0));
    assert_eq!(point, Point::new(4010.0, -10.0));

    let candidates = index.query(&query(&view, point, T_MOUSE_PX)).unwrap();
    let nodes: Vec<_> = candidates
        .iter()
        .filter(|c| c.class == SnapClass::JoinNode)
        .collect();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].entity, Some(join));
    assert_eq!(nodes[0].at, Point::new(4000.0, 0.0));
    assert!(
        !candidates
            .iter()
            .any(|c| c.class == SnapClass::FreeEndpoint),
        "member endpoints produce NO separate free-endpoint candidates"
    );
}

// ---------------------------------------------------------------------------
// S7 — touch versus mouse radii
// ---------------------------------------------------------------------------

#[test]
fn s7_the_device_decides_the_radius_for_identical_coordinates() {
    let view = View::defaults();
    let (doc, level, w1) = f1();
    let index = SnapIndex::build(&doc, level);
    let end = doc.wall(w1).unwrap().end();

    let point = view.plan((1854.0, 738.0));
    assert_eq!(view.logical((1854.0, 738.0)), (927.0, 369.0));
    assert_eq!(point, Point::new(4150.0, 150.0));
    let to_end = view.squared_px(point, end);
    assert_eq!(to_end, 450.0);
    assert!(to_end > T_MOUSE_PX * T_MOUSE_PX);

    // Grid OFF exactly as the fixture states, or the always-present lattice
    // would make a true mouse miss impossible.
    let mouse = index.query(&query(&view, point, T_MOUSE_PX)).unwrap();
    assert_eq!(mouse, vec![], "450 > 144: a TRUE MISS");

    let touch = index.query(&query(&view, point, T_TOUCH_PX)).unwrap();
    assert!(to_end <= T_TOUCH_PX * T_TOUCH_PX);
    assert_eq!(touch[0].class, SnapClass::FreeEndpoint);
    assert_eq!(
        touch[0].at, end,
        "class order picks the endpoint over the farther axis and edge"
    );
    assert!(touch
        .iter()
        .any(|c| c.class == SnapClass::Axis || c.class == SnapClass::Edge));
}

// ---------------------------------------------------------------------------
// S4 — self-snap exclusion; snapped winners resolve legally
// ---------------------------------------------------------------------------

/// S4's scene: one accepted chain segment, isolated.
fn s4_seg1() -> (Document, Uuid) {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.walls.push(Wall::new(
        level,
        Point::new(0.0, 0.0),
        Point::new(3000.0, 0.0),
        200.0,
        2500.0,
    ));
    assert_eq!(doc.validate(), vec![]);
    (doc, level)
}

/// S4's chain query: grid s = 100 mm, object snapping on, anchor at the
/// previous accepted endpoint.
fn s4_query<'a>(view: &View, point: Point, anchor: Point) -> SnapQuery<'a> {
    let mut req = query(view, point, T_MOUSE_PX);
    req.anchor = Some(anchor);
    req.grid_enabled = true;
    req.grid_spacing_mm = Some(100.0);
    req
}

#[test]
fn s4a_the_axis_extension_wins_over_grid_with_the_anchor_out_of_range() {
    let view = View::defaults();
    let (doc, level) = s4_seg1();
    let index = SnapIndex::build(&doc, level);
    let anchor = doc.walls[0].end();

    let point = view.plan((1654.0, 766.0));
    assert_eq!(view.logical((1654.0, 766.0)), (827.0, 383.0));
    assert_eq!(point, Point::new(3150.0, 10.0));

    // The fixture's own comparator trace, recomputed rather than restated.
    let to_anchor = view.squared_px(point, anchor);
    assert_eq!(to_anchor, 226.0);
    assert!(
        to_anchor > T_MOUSE_PX * T_MOUSE_PX,
        "the anchor is out of range anyway"
    );
    assert_eq!(view.squared_px(point, Point::new(3000.0, 10.0)), 225.0);
    assert_eq!(view.squared_px(point, Point::new(3000.0, 100.0)), 306.0);
    assert_eq!(view.squared_px(point, Point::new(3150.0, 0.0)), 1.0);
    for lattice in [Point::new(3100.0, 0.0), Point::new(3200.0, 0.0)] {
        assert_eq!(view.squared_px(point, lattice), 26.0);
    }

    let candidates = index.query(&s4_query(&view, point, anchor)).unwrap();
    let winner = &candidates[0];
    assert_eq!(winner.class, SnapClass::Axis);
    assert_eq!(winner.at, Point::new(3150.0, 0.0));
    assert!(
        !candidates.iter().any(|c| matches!(
            c.class,
            SnapClass::JoinNode | SnapClass::FreeEndpoint | SnapClass::Edge
        )),
        "no node, endpoint or edge is within the radius: {candidates:#?}"
    );
}

#[test]
fn s4a_anchor_coincident_candidates_are_dropped_while_neighbours_survive() {
    let view = View::defaults();
    let (doc, level) = s4_seg1();
    let index = SnapIndex::build(&doc, level);
    let anchor = doc.walls[0].end();

    // First probe: the cap-face edge point and the axis extension survive,
    // while the anchor-coincident endpoint and every zero-length acceptance
    // are gone.
    let point = Point::new(3005.0, 3.0);
    let candidates = index.query(&s4_query(&view, point, anchor)).unwrap();
    assert!(candidates.iter().all(|c| c.at != anchor), "{candidates:#?}");
    assert!(candidates
        .iter()
        .any(|c| c.class == SnapClass::Edge && c.at == Point::new(3000.0, 3.0)));
    assert!(candidates
        .iter()
        .any(|c| c.class == SnapClass::Axis && c.at == Point::new(3005.0, 0.0)));

    // Second probe: the two boundary points at the cap's corners tie at
    // 425/4 px^2 and win by CLASS over an axis extension sitting at zero.
    let on_axis = view.plan((1629.0, 768.0));
    assert_eq!(view.logical((1629.0, 768.0)), (814.5, 384.0));
    assert_eq!(on_axis, Point::new(3025.0, 0.0));
    for corner in [Point::new(3000.0, -100.0), Point::new(3000.0, 100.0)] {
        assert_eq!(view.squared_px(on_axis, corner), 425.0 / 4.0);
    }
    let candidates = index.query(&s4_query(&view, on_axis, anchor)).unwrap();
    assert_eq!(candidates[0].class, SnapClass::Edge);
    assert_eq!(
        candidates[0].at,
        Point::new(3000.0, -100.0),
        "(3000,-100) < (3000,100)"
    );
    assert_eq!(view.squared_px(on_axis, candidates[0].at), 425.0 / 4.0);
    let axis = candidates
        .iter()
        .find(|c| c.class == SnapClass::Axis)
        .expect("the axis extension still qualifies, it just loses by class");
    assert_eq!(view.squared_px(on_axis, axis.at), 0.0);
    assert!(candidates.iter().all(|c| c.at != anchor));
}

#[test]
fn s4b_an_axis_extension_beyond_a_third_walls_endpoint_wins_at_zero() {
    let view = View::defaults();
    let (mut doc, level) = s4_seg1();
    let seg1 = doc.walls[0].id;
    doc.walls.push(Wall::new(
        level,
        Point::new(3000.0, 0.0),
        Point::new(3150.0, 0.0),
        200.0,
        2500.0,
    ));
    let seg2 = doc.walls[1].id;
    doc.joins.push(WallJoin::new(vec![
        JoinMember::new(seg1, Role::End),
        JoinMember::new(seg2, Role::Start),
    ]));
    doc.walls.push(Wall::new(
        level,
        Point::new(3600.0, -500.0),
        Point::new(3600.0, 1500.0),
        200.0,
        2500.0,
    ));
    let wv = doc.walls[2].id;
    let index = SnapIndex::build(&doc, level);

    // The chain anchor is the ACTUAL PRIOR ENDPOINT seg2.End.
    let anchor = doc.wall(seg2).unwrap().end();
    assert_eq!(anchor, Point::new(3150.0, 0.0));

    let point = view.plan((1744.0, 428.0));
    assert_eq!(view.logical((1744.0, 428.0)), (872.0, 214.0));
    assert_eq!(point, Point::new(3600.0, 1700.0));

    let top = doc.wall(wv).unwrap().end();
    let to_top = view.squared_px(point, top);
    assert_eq!(to_top, 400.0);
    assert!(
        to_top > T_MOUSE_PX * T_MOUSE_PX,
        "the endpoint is OUTSIDE the radius"
    );

    let candidates = index.query(&s4_query(&view, point, anchor)).unwrap();
    let winner = &candidates[0];
    assert_eq!(winner.class, SnapClass::Axis);
    assert_eq!(
        winner.at, point,
        "exactly on Wv's axis line, 200 mm past its end"
    );
    assert_eq!(view.squared_px(point, winner.at), 0.0);
    assert_eq!(winner.entity, Some(wv));
    let grid = candidates
        .iter()
        .find(|c| c.class == SnapClass::Grid)
        .expect("the lattice point ties at zero and loses by class");
    assert_eq!(grid.at, point);
}

// ---------------------------------------------------------------------------
// S5 — angle families normalize and deduplicate; ahead-of-anchor check
// ---------------------------------------------------------------------------

#[test]
fn s5_relative_and_global_families_deduplicate_to_one_45_degree_winner() {
    let view = View::defaults();
    let (doc, level) = s4_seg1();
    let index = SnapIndex::build(&doc, level);
    let anchor = doc.walls[0].end();
    let previous = (
        doc.walls[0].end().x - doc.walls[0].start().x,
        doc.walls[0].end().y - doc.walls[0].start().y,
    );

    let point = view.plan((1824.0, 568.0));
    assert_eq!(view.logical((1824.0, 568.0)), (912.0, 284.0));
    assert_eq!(point, Point::new(4000.0, 1000.0));

    // Grid toggle OFF, object snap ON, exactly as the fixture states.
    let mut req = query(&view, point, T_MOUSE_PX);
    req.anchor = Some(anchor);
    req.previous_direction = Some(previous);
    req.angle_enabled = true;
    let candidates = index.query(&req).unwrap();

    let winner = &candidates[0];
    assert_eq!(winner.class, SnapClass::Angle);
    assert_eq!(
        winner.at, point,
        "the projection of an on-ray pointer is itself"
    );
    // One target, not four: the relative sums all coincide with global
    // 45-degree multiples.
    assert_eq!(
        candidates
            .iter()
            .filter(|c| c.class == SnapClass::Angle)
            .count(),
        1
    );
}

// ---------------------------------------------------------------------------
// S6 — disable modifier held across a gesture
// ---------------------------------------------------------------------------

#[test]
fn s6_alt_sampled_at_gesture_start_suppresses_every_default_enabled_class() {
    let view = View::defaults();
    // "Before: empty document, level L1" — nothing to snap to except the
    // local constructions, which are what Alt has to suppress.
    let doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let index = SnapIndex::build(&doc, level);

    for raw in [(1271.4, 650.2), (1471.4, 450.2)] {
        let point = view.plan(raw);
        let mut req = query(&view, point, T_MOUSE_PX);
        req.grid_enabled = true;
        req.grid_spacing_mm = Some(100.0);
        req.angle_enabled = true;
        req.anchor = Some(Point::new(0.0, 0.0));

        // With the toggles at their R6.4.3 defaults the lattice always
        // answers, so the empty result below is Alt and not an empty scene.
        assert!(!index.query(&req).unwrap().is_empty());

        req.alt_disabled = true;
        assert_eq!(
            index.query(&req).unwrap(),
            vec![],
            "raw coordinates commit unsnapped"
        );
    }
}

// ---------------------------------------------------------------------------
// G1 — guide lifetime follows consecutive wins and gesture end
// ---------------------------------------------------------------------------

#[test]
fn g1_the_winning_feature_persists_across_frames_and_then_changes() {
    let view = View::defaults();
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.walls.push(Wall::new(
        level,
        Point::new(0.0, 0.0),
        Point::new(1000.0, 0.0),
        200.0,
        2500.0,
    ));
    doc.walls.push(Wall::new(
        level,
        Point::new(2000.0, -400.0),
        Point::new(2400.0, -400.0),
        200.0,
        2500.0,
    ));
    assert_eq!(doc.validate(), vec![]);
    let index = SnapIndex::build(&doc, level);
    // The continuation anchor is the PRIOR ACCEPTED ENDPOINT; only it is
    // excluded, so Wa's and seg1's edges and axes stay legal candidates.
    let anchor = doc.walls[1].end();
    assert_eq!(anchor, Point::new(2400.0, -400.0));

    let frame = |raw: (f64, f64)| {
        let point = view.plan(raw);
        let candidates = index.query(&s4_query(&view, point, anchor)).unwrap();
        (point, candidates)
    };

    let (p1, f1) = frame((1128.0, 744.0));
    assert_eq!(view.logical((1128.0, 744.0)), (564.0, 372.0));
    assert_eq!(p1, Point::new(520.0, 120.0));
    assert_eq!(f1[0].class, SnapClass::Edge);
    assert_eq!(f1[0].at, Point::new(520.0, 100.0));
    assert_eq!(view.squared_px(p1, f1[0].at), 4.0);
    assert!(f1[0].guide.is_some(), "the edge alignment guide renders");

    let (p2, f2) = frame((1130.0, 742.0));
    assert_eq!(p2, Point::new(530.0, 130.0));
    assert_eq!(f2[0].class, SnapClass::Edge);
    assert_eq!(f2[0].at, Point::new(530.0, 100.0));
    assert_eq!(view.squared_px(p2, f2[0].at), 9.0);
    // Same-class consecutive win: the SAME feature, so the guide persists
    // unchanged even though its closest point slid along the face.
    assert_eq!(f2[0].feature, f1[0].feature);
    assert_ne!(f2[0].at, f1[0].at);

    let (p3, f3) = frame((1364.0, 708.0));
    assert_eq!(view.logical((1364.0, 708.0)), (682.0, 354.0));
    assert_eq!(p3, Point::new(1700.0, 300.0));
    assert_eq!(f3[0].class, SnapClass::Grid);
    assert_eq!(f3[0].at, p3);
    assert_eq!(view.squared_px(p3, f3[0].at), 0.0);
    // The winner changed, so the edge guide vanishes.
    assert_ne!(f3[0].feature, f2[0].feature);
    // Every object candidate the fixture names is outside the radius here.
    for outside in [
        Point::new(1000.0, 100.0),
        Point::new(2000.0, -300.0),
        Point::new(1700.0, 0.0),
        Point::new(1700.0, -400.0),
    ] {
        assert!(view.squared_px(p3, outside) > T_MOUSE_PX * T_MOUSE_PX);
    }
}
