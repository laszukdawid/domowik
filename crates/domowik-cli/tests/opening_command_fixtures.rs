//! BEHAVIOR-SPEC §4.5 fixtures through the product: openings created,
//! edited and deleted by committed batches (admission runs document
//! validation), hosts edited by the wall commands, and at every step the
//! literal amended outline (R4.2.2) of the core's drawn region, from the
//! one-shot builder and the retained cut engine alike. Refusals are matched
//! by validation VARIANT; the spec's diagnostic strings are mapped to them in
//! issue 0013's coverage table.

use domowik_document::operation::Operation;
use domowik_document::session::{CommandError, Session};
use domowik_edit::{add_wall_operations, reverse_wall_operations, set_wall_operations, EditError};
use domowik_format::{
    Document, Entity, Hinge, Level, Opening, OpeningKind, Point, Side, ValidationError, Wall,
};
use domowik_geometry::cuts::{level_apertures, level_drawn_region};
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::polygon::{canonical_ring, Winding};
use domowik_geometry::walls;
use std::collections::BTreeMap;
use uuid::Uuid;

struct Plan {
    s: Session,
    level: Uuid,
}

fn plan_with_levels(n: usize) -> (Plan, Vec<Uuid>) {
    let mut doc = Document::new();
    let ids: Vec<Uuid> = (0..n)
        .map(|i| {
            let l = Level::new(format!("L{}", i + 1));
            let id = l.id;
            doc.levels.push(l);
            id
        })
        .collect();
    let s = Session::open_snapshot(Uuid::now_v7(), doc, &BTreeMap::new(), &[], Vec::new())
        .expect("opens");
    (Plan { s, level: ids[0] }, ids)
}

impl Plan {
    fn doc(&self) -> &Document {
        self.s.document()
    }

    fn add_wall(&mut self, level: Uuid, a: (f64, f64), b: (f64, f64), t: f64) -> Uuid {
        let w = Wall::new(level, Point::new(a.0, a.1), Point::new(b.0, b.1), t, 2500.0);
        let id = w.id;
        let edit = add_wall_operations(self.doc(), level, w).expect("classifies");
        self.s.commit(edit.operations).expect("commits");
        id
    }

    fn commit(&mut self, ops: Vec<Operation>) -> Result<Uuid, CommandError> {
        self.s.commit(ops)
    }

    /// The level's drawn-region components as canonical rings, sorted; the
    /// retained cut engine must agree with the one-shot builder.
    fn drawn(&self, level: Uuid) -> Vec<Vec<Point>> {
        let region = level_drawn_region(self.doc(), level);
        let solids = walls::level_solids(self.doc(), level);
        let mut engine = LevelGeometry::occupied_only();
        engine.sync_cut(&solids, &level_apertures(self.doc(), &solids));
        assert_eq!(engine.occupied(), region, "retained cut engine == one-shot");
        let mut out: Vec<Vec<Point>> = region
            .iter()
            .map(|p| {
                assert!(p.holes.is_empty());
                canonical_ring(&p.outer, Winding::CounterClockwise).expect("ring")
            })
            .collect();
        out.sort_by(|a, b| a[0].x.total_cmp(&b[0].x));
        out
    }
}

fn ring(points: &[(f64, f64)]) -> Vec<Point> {
    let pts: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
    canonical_ring(&pts, Winding::CounterClockwise).expect("ring")
}

#[allow(clippy::too_many_arguments)]
fn opening(
    host: Uuid,
    kind: OpeningKind,
    u: f64,
    w: f64,
    h: f64,
    sill: f64,
    depth: f64,
    side: Side,
) -> Opening {
    let mut o = Opening::door(host, u, w, h, depth);
    o.kind = kind;
    o.sill_mm = sill;
    o.side = side;
    o
}

fn create(o: &Opening) -> Vec<Operation> {
    vec![Operation::create(Entity::Opening(o.clone()))]
}

fn set(id: Uuid, field: &str, value: serde_json::Value) -> Vec<Operation> {
    vec![Operation::set_fields(
        id,
        serde_json::Map::from_iter([(field.to_owned(), value)]),
    )]
}

/// A refused commit with exactly the validation variant `want`, leaving the
/// document unchanged.
fn refused(p: &mut Plan, ops: Vec<Operation>, want: fn(&ValidationError) -> bool) {
    let before = p.doc().clone();
    match p.commit(ops) {
        Err(CommandError::Invalid(errors)) => assert!(errors.iter().any(want), "{errors:?}"),
        other => panic!("expected a validation refusal, got {other:?}"),
    }
    assert_eq!(p.doc(), &before, "a refused batch changes nothing");
}

const BAND: [(f64, f64); 4] = [
    (0.0, -100.0),
    (4000.0, -100.0),
    (4000.0, 100.0),
    (0.0, 100.0),
];
const F26: [(f64, f64); 8] = [
    (0.0, -100.0),
    (4000.0, -100.0),
    (4000.0, 100.0),
    (2600.0, 100.0),
    (2600.0, 0.0),
    (1400.0, 0.0),
    (1400.0, 100.0),
    (0.0, 100.0),
];

/// F1 state: W1 (0,0)→(4000,0), t = 200.
fn f1() -> (Plan, Uuid) {
    let (mut p, _) = plan_with_levels(1);
    let level = p.level;
    let w1 = p.add_wall(level, (0.0, 0.0), (4000.0, 0.0), 200.0);
    (p, w1)
}

fn window(w1: Uuid) -> Opening {
    let mut o = opening(
        w1,
        OpeningKind::Window,
        0.5,
        1200.0,
        1300.0,
        900.0,
        100.0,
        Side::Left,
    );
    o.hinge = Hinge::NearEnd;
    o.opens_toward = Side::Left;
    o.swing_deg = 45.0;
    o
}

#[test]
fn f25_and_f26_create_cut_and_undo() {
    let (mut p, w1) = f1();
    let level = p.level;
    let door = opening(
        w1,
        OpeningKind::Door,
        0.5,
        900.0,
        2100.0,
        0.0,
        200.0,
        Side::Left,
    );
    p.commit(create(&door)).expect("F25 commits");
    assert_eq!(
        p.doc().openings,
        vec![door.clone()],
        "exactly the commanded fields"
    );
    assert_eq!(
        p.drawn(level),
        vec![
            ring(&[
                (0.0, -100.0),
                (1550.0, -100.0),
                (1550.0, 100.0),
                (0.0, 100.0)
            ]),
            ring(&[
                (2450.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (2450.0, 100.0)
            ]),
        ]
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.drawn(level), vec![ring(&BAND)], "F25 undo restores F1");

    let ow = window(w1);
    p.commit(create(&ow)).expect("F26 commits");
    assert_eq!(p.drawn(level), vec![ring(&F26)]);
    p.s.undo().expect("undo").expect("entry");
    assert!(p.doc().openings.is_empty());
}

#[test]
fn f27_touching_openings_in_either_order_and_a_transom() {
    let step2 = vec![
        ring(&[(0.0, -100.0), (600.0, -100.0), (600.0, 100.0), (0.0, 100.0)]),
        ring(&[
            (1400.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (2200.0, 100.0),
            (2200.0, 0.0),
            (1400.0, 0.0),
        ]),
    ];
    for reverse in [false, true] {
        let (mut p, w1) = f1();
        let level = p.level;
        let od = opening(
            w1,
            OpeningKind::Door,
            0.25,
            800.0,
            2100.0,
            0.0,
            200.0,
            Side::Left,
        );
        let on = opening(
            w1,
            OpeningKind::Window,
            9.0 / 20.0,
            800.0,
            1300.0,
            900.0,
            100.0,
            Side::Left,
        );
        let order = if reverse { [&on, &od] } else { [&od, &on] };
        for o in order {
            p.commit(create(o))
                .expect("touching openings commit (R4.1.4)");
        }
        assert_eq!(p.drawn(level), step2, "order invariance (R4.0.6)");
        let ot = opening(
            w1,
            OpeningKind::Window,
            0.25,
            800.0,
            380.0,
            2100.0,
            100.0,
            Side::Left,
        );
        p.commit(create(&ot))
            .expect("the transom touches the door vertically");
        assert_eq!(p.drawn(level), step2, "the transom changes no plan");
        p.s.undo().expect("undo").expect("entry");
        assert_eq!(p.drawn(level), step2);
    }
}

#[test]
fn f28_overlap_matrix() {
    let (mut p, w1) = f1();
    let level = p.level;
    p.commit(create(&window(w1))).expect("F26");
    // (a) positive overlap in both directions: refused.
    let oa = opening(
        w1,
        OpeningKind::Door,
        3.0 / 8.0,
        800.0,
        2100.0,
        0.0,
        200.0,
        Side::Left,
    );
    refused(&mut p, create(&oa), |e| {
        matches!(e, ValidationError::OverlappingOpenings { .. })
    });
    // (b) vertically disjoint: commits and cuts left of the notch.
    let ob = opening(
        w1,
        OpeningKind::Door,
        3.0 / 8.0,
        800.0,
        150.0,
        2300.0,
        200.0,
        Side::Left,
    );
    p.commit(create(&ob)).expect("(b) commits");
    assert_eq!(
        p.drawn(level),
        vec![
            ring(&[
                (0.0, -100.0),
                (1100.0, -100.0),
                (1100.0, 100.0),
                (0.0, 100.0)
            ]),
            ring(&[
                (1900.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (2600.0, 100.0),
                (2600.0, 0.0),
                (1900.0, 0.0)
            ]),
        ]
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.drawn(level), vec![ring(&F26)]);
    // (c) touching at 2600: commits.
    let oc = opening(
        w1,
        OpeningKind::Door,
        0.75,
        800.0,
        2100.0,
        0.0,
        200.0,
        Side::Left,
    );
    p.commit(create(&oc)).expect("(c) commits");
    assert_eq!(
        p.drawn(level),
        vec![
            ring(&[
                (0.0, -100.0),
                (2600.0, -100.0),
                (2600.0, 0.0),
                (1400.0, 0.0),
                (1400.0, 100.0),
                (0.0, 100.0)
            ]),
            ring(&[
                (3400.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (3400.0, 100.0)
            ]),
        ]
    );
}

/// F29's window: u = 3/10, 1200 wide, Left, hinge nearStart.
fn f29() -> (Plan, Uuid, Uuid) {
    let (mut p, w1) = f1();
    let mut ow = window(w1);
    ow.u = 0.3;
    ow.hinge = Hinge::NearStart;
    ow.opens_toward = Side::Right;
    ow.swing_deg = 60.0;
    let id = ow.id;
    p.commit(create(&ow)).expect("commits");
    (p, w1, id)
}

const F29_OUTLINE: [(f64, f64); 8] = [
    (0.0, -100.0),
    (4000.0, -100.0),
    (4000.0, 100.0),
    (1800.0, 100.0),
    (1800.0, 0.0),
    (600.0, 0.0),
    (600.0, 100.0),
    (0.0, 100.0),
];

#[test]
fn f29_reversal_maps_the_window_and_keeps_the_outline() {
    let (mut p, w1, ow) = f29();
    let level = p.level;
    assert_eq!(p.drawn(level), vec![ring(&F29_OUTLINE)]);
    let edit = reverse_wall_operations(p.doc(), w1);
    p.commit(edit.operations).expect("reverses");
    let o = p
        .doc()
        .openings
        .iter()
        .find(|o| o.id == ow)
        .unwrap()
        .clone();
    assert!((o.u - 0.7).abs() < 1e-12, "u = 7/10, got {}", o.u);
    assert_eq!(
        (o.side, o.hinge, o.opens_toward),
        (Side::Right, Hinge::NearEnd, Side::Left)
    );
    assert_eq!(o.swing_deg, 60.0);
    assert_eq!(
        p.drawn(level),
        vec![ring(&F29_OUTLINE)],
        "world intent invariant"
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.doc().openings.iter().find(|o| o.id == ow).unwrap().u, 0.3);
}

#[test]
fn f30_shorten_keeping_u_and_f31_shortening_that_breaks_fit() {
    let (mut p, w1, ow) = f29();
    let level = p.level;
    // F31 first, each against the F30 before-state: refused.
    for end in [1700.0, 2000.0] {
        let before = p.doc().clone();
        let refused =
            match set_wall_operations(p.doc(), w1, None, Some(Point::new(end, 0.0)), None, None) {
                Err(_) => true,
                Ok(edit) => p.commit(edit.operations).is_err(),
            };
        assert!(refused, "end {end} must reject aperture-not-strict-fit");
        assert_eq!(p.doc(), &before);
    }
    let edit = set_wall_operations(p.doc(), w1, None, Some(Point::new(3600.0, 0.0)), None, None)
        .expect("classifies");
    p.commit(edit.operations).expect("F30 commits");
    assert_eq!(
        p.doc().openings.iter().find(|o| o.id == ow).unwrap().u,
        0.3,
        "u kept"
    );
    assert_eq!(
        p.drawn(level),
        vec![ring(&[
            (0.0, -100.0),
            (3600.0, -100.0),
            (3600.0, 100.0),
            (1680.0, 100.0),
            (1680.0, 0.0),
            (480.0, 0.0),
            (480.0, 100.0),
            (0.0, 100.0)
        ])]
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.drawn(level), vec![ring(&F29_OUTLINE)]);
}

#[test]
fn f32_f32b_and_f33_splits_through_a_hosted_window() {
    // F32: a t = 300 crossing at x = 3000 keeps the window on W1 with u = 2/5.
    let (mut p, w1, ow) = f29();
    let level = p.level;
    p.add_wall(level, (3000.0, -1500.0), (3000.0, 1500.0), 300.0);
    let o = p.doc().openings.iter().find(|o| o.id == ow).unwrap();
    assert_eq!(o.host_wall_id, w1);
    assert!((o.u - 0.4).abs() < 1e-12, "u = 2/5, got {}", o.u);
    assert_eq!(
        p.drawn(level),
        vec![ring(&[
            (0.0, -100.0),
            (2850.0, -100.0),
            (2850.0, -1500.0),
            (3150.0, -1500.0),
            (3150.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (3150.0, 100.0),
            (3150.0, 1500.0),
            (2850.0, 1500.0),
            (2850.0, 100.0),
            (1800.0, 100.0),
            (1800.0, 0.0),
            (600.0, 0.0),
            (600.0, 100.0),
            (0.0, 100.0),
        ])]
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(
        p.doc().openings.iter().find(|o| o.id == ow).unwrap().u,
        0.3,
        "undo restores u"
    );

    // F32b: a crossing at x = 500 remaps the window to the NEW child, u = 1/5.
    p.add_wall(level, (500.0, -1000.0), (500.0, 1000.0), 200.0);
    let o = p.doc().openings.iter().find(|o| o.id == ow).unwrap();
    let host = p.doc().wall(o.host_wall_id).unwrap();
    assert_ne!(host.id, w1, "remapped to the new child");
    assert_eq!(
        (host.start(), host.end()),
        (Point::new(500.0, 0.0), Point::new(4000.0, 0.0))
    );
    assert!((o.u - 0.2).abs() < 1e-12, "u = 1/5, got {}", o.u);
    assert_eq!(
        p.drawn(level),
        vec![ring(&[
            (0.0, -100.0),
            (400.0, -100.0),
            (400.0, -1000.0),
            (600.0, -1000.0),
            (600.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (1800.0, 100.0),
            (1800.0, 0.0),
            (600.0, 0.0),
            (600.0, 1000.0),
            (400.0, 1000.0),
            (400.0, 100.0),
            (0.0, 100.0),
        ])]
    );
    p.s.undo().expect("undo").expect("entry");

    // F33: crossings strictly inside, and exactly on the edge of, the
    // aperture reject the whole batch.
    for x in [1200.0, 600.0] {
        let before = p.doc().clone();
        let w = Wall::new(
            level,
            Point::new(x, -1000.0),
            Point::new(x, 1000.0),
            200.0,
            2500.0,
        );
        assert!(
            matches!(
                add_wall_operations(p.doc(), level, w),
                Err(EditError::OpeningSplitAmbiguous(_))
            ),
            "x = {x}"
        );
        assert_eq!(p.doc(), &before);
    }
}

/// The primary §4.4 code of a refused batch (R4.4 precedence), the
/// document unchanged.
fn primary(p: &mut Plan, ops: Vec<Operation>) -> &'static str {
    let before = p.doc().clone();
    let err = p.commit(ops).expect_err("refused");
    assert_eq!(p.doc(), &before, "a refused batch changes nothing");
    err.primary_opening_code()
        .unwrap_or_else(|| panic!("no §4.4 code in {err}"))
        .as_str()
}

/// §4 F34 in full: every creation reject, the raw edit rejects, both
/// combined-invalid precedence vectors, and the accepted boundaries. Enum
/// values a typed opening cannot hold are written as raw `SetFields` (the
/// native bridge refuses them on creation; see its tests).
#[test]
fn f34_validation_matrix() {
    let (mut p, levels) = plan_with_levels(2);
    let w1 = p.add_wall(levels[0], (0.0, 0.0), (4000.0, 0.0), 200.0);
    let base = || {
        opening(
            w1,
            OpeningKind::Door,
            0.5,
            900.0,
            2100.0,
            0.0,
            200.0,
            Side::Left,
        )
    };
    let with = |f: &dyn Fn(&mut Opening)| {
        let mut o = base();
        f(&mut o);
        create(&o)
    };
    let rows: Vec<(&str, Vec<Operation>)> = vec![
        ("nonpositive-width", with(&|o| o.width_mm = 0.0)),
        ("nonpositive-height", with(&|o| o.height_mm = 0.0)),
        ("nonpositive-depth", with(&|o| o.depth_mm = 0.0)),
        (
            "depth-exceeds-host-thickness",
            with(&|o| o.depth_mm = 201.0),
        ),
        ("invalid-opening-center", with(&|o| o.u = 1.25)),
        ("nonfinite-value", with(&|o| o.u = f64::NAN)),
        ("negative-sill", with(&|o| o.sill_mm = -100.0)),
        ("invalid-swing-angle", with(&|o| o.swing_deg = -5.0)),
        ("invalid-swing-angle", with(&|o| o.swing_deg = 181.0)),
        ("nonfinite-value", with(&|o| o.swing_deg = f64::NAN)),
        (
            "missing-style",
            with(&|o| o.style_refs = vec![String::new()]),
        ),
        ("unknown-host", with(&|o| o.host_wall_id = Uuid::now_v7())),
        ("entity-id-collision", with(&|o| o.id = w1)),
        (
            "aperture-exceeds-host-height",
            with(&|o| {
                o.sill_mm = 2101.0;
                o.height_mm = 400.0;
            }),
        ),
    ];
    for (code, ops) in rows {
        assert_eq!(primary(&mut p, ops), code);
    }

    // Edit rejects against the F26 aftermath.
    let ow = window(w1);
    let id = ow.id;
    p.commit(create(&ow)).expect("F26");
    let w2 = p.add_wall(levels[0], (0.0, 3000.0), (4000.0, 3000.0), 200.0);
    let edits: Vec<(&str, Vec<Operation>)> = vec![
        (
            "invalid-opening-kind",
            set(id, "kind", serde_json::json!("sliding")),
        ),
        ("invalid-side", set(id, "side", serde_json::json!("Inward"))),
        (
            "invalid-hinge",
            set(id, "hinge", serde_json::json!("middle")),
        ),
        (
            "invalid-opens-toward",
            set(id, "opensToward", serde_json::json!("Inward")),
        ),
        (
            "opening-direct-rehost",
            set(id, "hostWallId", serde_json::json!(w2)),
        ),
        (
            "opening-level-immutable",
            set(id, "levelId", serde_json::json!(levels[1])),
        ),
        (
            "wall-level-immutable",
            set(w1, "levelId", serde_json::json!(levels[1])),
        ),
        // Any write is refused, even of the current value, and a write
        // restored later in the same batch.
        (
            "wall-level-immutable",
            set(w1, "levelId", serde_json::json!(levels[0])),
        ),
        (
            "wall-level-immutable",
            [
                set(w1, "levelId", serde_json::json!(levels[1])),
                set(w1, "levelId", serde_json::json!(levels[0])),
            ]
            .concat(),
        ),
    ];
    for (code, ops) in edits {
        assert_eq!(primary(&mut p, ops), code);
    }

    // Combined-invalid vectors (R4.4): kind outranks u, width and sill;
    // width outranks height and depth; an identity failure outranks an enum.
    let mut combined = base();
    combined.u = 1.25;
    combined.width_mm = 0.0;
    combined.sill_mm = -5.0;
    let cid = combined.id;
    assert_eq!(
        primary(
            &mut p,
            [
                create(&combined),
                set(cid, "kind", serde_json::json!("sliding"))
            ]
            .concat()
        ),
        "invalid-opening-kind"
    );
    let mut dims = base();
    dims.width_mm = 0.0;
    dims.height_mm = 0.0;
    dims.depth_mm = 0.0;
    assert_eq!(primary(&mut p, create(&dims)), "nonpositive-width");
    let mut orphan = base();
    orphan.host_wall_id = Uuid::now_v7();
    assert_eq!(
        primary(
            &mut p,
            [
                set(id, "kind", serde_json::json!("sliding")),
                create(&orphan)
            ]
            .concat()
        ),
        "unknown-host"
    );

    // A fake "split": a new wall elsewhere plus a host change is a direct
    // rehost, not a remap.
    let elsewhere = Wall::new(
        levels[0],
        Point::new(0.0, 6000.0),
        Point::new(4000.0, 6000.0),
        200.0,
        2500.0,
    );
    let elsewhere_id = elsewhere.id;
    assert_eq!(
        primary(
            &mut p,
            [
                vec![Operation::create(Entity::Wall(elsewhere))],
                set(id, "hostWallId", serde_json::json!(elsewhere_id))
            ]
            .concat()
        ),
        "opening-direct-rehost"
    );

    // Accepted boundaries.
    p.commit(set(id, "depthMm", serde_json::json!(200.0)))
        .expect("depth == thickness commits");
    let mut top = opening(
        w2,
        OpeningKind::Window,
        0.25,
        600.0,
        400.0,
        2100.0,
        100.0,
        Side::Left,
    );
    top.style_refs.clear();
    p.commit(create(&top))
        .expect("sill + height == host height commits");
    let floor = opening(
        w2,
        OpeningKind::Door,
        0.75,
        600.0,
        2100.0,
        0.0,
        200.0,
        Side::Left,
    );
    p.commit(create(&floor))
        .expect("sill 0, height 2100 commits");
}

#[test]
fn f35_a_door_near_a_join_keeps_the_patch() {
    let (mut p, _) = plan_with_levels(1);
    let level = p.level;
    let w1 = p.add_wall(level, (0.0, 0.0), (3000.0, 0.0), 200.0);
    p.add_wall(level, (3000.0, 0.0), (3000.0, 2500.0), 300.0);
    assert_eq!(p.doc().joins.len(), 1);
    let door = opening(
        w1,
        OpeningKind::Door,
        5.0 / 6.0,
        600.0,
        2100.0,
        0.0,
        200.0,
        Side::Left,
    );
    p.commit(create(&door)).expect("commits");
    assert_eq!(
        p.drawn(level),
        vec![
            ring(&[
                (0.0, -100.0),
                (2200.0, -100.0),
                (2200.0, 100.0),
                (0.0, 100.0)
            ]),
            ring(&[
                (2800.0, -100.0),
                (3000.0, -100.0),
                (3150.0, 0.0),
                (3150.0, 2500.0),
                (2850.0, 2500.0),
                (2850.0, 100.0),
                (2800.0, 100.0)
            ]),
        ]
    );
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.drawn(level).len(), 1, "the pre-opening region");
}

#[test]
fn f36_delete_and_f37_updates() {
    let (mut p, w1) = f1();
    let level = p.level;
    let ow = window(w1);
    let id = ow.id;
    p.commit(create(&ow)).expect("F26");
    // F36: delete, then undo.
    p.commit(vec![Operation::delete(id)]).expect("deletes");
    assert_eq!(p.drawn(level), vec![ring(&BAND)]);
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.drawn(level), vec![ring(&F26)]);
    // F37: each update is its own batch; geometric ones move the notch,
    // the rest leave the outline unchanged; undo restores F26 each time.
    type Case<'a> = (&'a str, serde_json::Value, Option<Vec<Vec<Point>>>);
    let cases: Vec<Case> = vec![
        ("kind", serde_json::json!("door"), None),
        (
            "side",
            serde_json::json!("Right"),
            Some(vec![ring(&[
                (0.0, -100.0),
                (1400.0, -100.0),
                (1400.0, 0.0),
                (2600.0, 0.0),
                (2600.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (0.0, 100.0),
            ])]),
        ),
        ("hinge", serde_json::json!("nearStart"), None),
        ("opensToward", serde_json::json!("Right"), None),
        ("sillMm", serde_json::json!(600.0), None),
        (
            "u",
            serde_json::json!(0.625),
            Some(vec![ring(&[
                (0.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (3100.0, 100.0),
                (3100.0, 0.0),
                (1900.0, 0.0),
                (1900.0, 100.0),
                (0.0, 100.0),
            ])]),
        ),
        (
            "widthMm",
            serde_json::json!(800.0),
            Some(vec![ring(&[
                (0.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (2400.0, 100.0),
                (2400.0, 0.0),
                (1600.0, 0.0),
                (1600.0, 100.0),
                (0.0, 100.0),
            ])]),
        ),
        ("heightMm", serde_json::json!(1500.0), None),
        (
            "depthMm",
            serde_json::json!(200.0),
            Some(vec![
                ring(&[
                    (0.0, -100.0),
                    (1400.0, -100.0),
                    (1400.0, 100.0),
                    (0.0, 100.0),
                ]),
                ring(&[
                    (2600.0, -100.0),
                    (4000.0, -100.0),
                    (4000.0, 100.0),
                    (2600.0, 100.0),
                ]),
            ]),
        ),
        ("swingDeg", serde_json::json!(90.0), None),
    ];
    for (field, value, outline) in cases {
        let before = p.doc().openings[0].clone();
        p.commit(set(id, field, value))
            .unwrap_or_else(|e| panic!("{field}: {e}"));
        assert_ne!(p.doc().openings[0], before, "{field} changed");
        assert_eq!(
            p.drawn(level),
            outline.unwrap_or_else(|| vec![ring(&F26)]),
            "{field}"
        );
        p.s.undo().expect("undo").expect("entry");
        assert_eq!(p.doc().openings[0], before, "{field} undo");
        assert_eq!(p.drawn(level), vec![ring(&F26)], "{field} undo outline");
    }
}

#[test]
fn f38_cross_level_isolation() {
    let (mut p, levels) = plan_with_levels(2);
    let w1 = p.add_wall(levels[0], (0.0, 0.0), (4000.0, 0.0), 200.0);
    let w2 = {
        let w = Wall::new(
            levels[1],
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2700.0,
        );
        let id = w.id;
        let edit = add_wall_operations(p.doc(), levels[1], w).expect("classifies");
        p.commit(edit.operations).expect("commits");
        id
    };
    let split = vec![
        ring(&[
            (0.0, -100.0),
            (1550.0, -100.0),
            (1550.0, 100.0),
            (0.0, 100.0),
        ]),
        ring(&[
            (2450.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (2450.0, 100.0),
        ]),
    ];
    let door = |host| {
        opening(
            host,
            OpeningKind::Door,
            0.5,
            900.0,
            2100.0,
            0.0,
            200.0,
            Side::Left,
        )
    };
    p.commit(create(&door(w1))).expect("step 1");
    assert_eq!(p.drawn(levels[0]), split);
    assert_eq!(p.drawn(levels[1]), vec![ring(&BAND)]);
    p.commit(create(&door(w2)))
        .expect("step 2: identical footprint on another level");
    assert_eq!(p.drawn(levels[0]), split);
    assert_eq!(p.drawn(levels[1]), split);
    p.s.undo().expect("undo").expect("entry");
    assert_eq!(p.drawn(levels[0]), split, "undoing step 2 leaves L1");
    assert_eq!(p.drawn(levels[1]), vec![ring(&BAND)]);
}

/// R4.3.5 holds for EXTERNAL admission too: another device's raw batch
/// rehosting an opening is refused with its code; and a verified split
/// remap survives undo AND redo (admission re-verifies each compensation).
#[test]
fn external_rehost_is_refused_and_split_remaps_survive_undo_and_redo() {
    use domowik_document::operation::Batch;
    let (mut p, w1, ow) = f29();
    let level = p.level;
    let w2 = p.add_wall(level, (0.0, 3000.0), (4000.0, 3000.0), 200.0);
    let before = p.doc().clone();
    let other = Uuid::now_v7();
    let batch = Batch::new(
        other,
        1,
        p.s.log().frontier(),
        set(ow, "hostWallId", serde_json::json!(w2)),
    );
    let err = p.s.accept_external(batch).expect_err("refused");
    assert_eq!(
        err.primary_opening_code().map(|c| c.as_str()),
        Some("opening-direct-rehost")
    );
    assert_eq!(p.doc(), &before);

    // F32b's remap to a new child, undone and redone.
    p.add_wall(level, (500.0, -1000.0), (500.0, 1000.0), 200.0);
    let remapped = p.doc().clone();
    assert_ne!(
        remapped
            .openings
            .iter()
            .find(|o| o.id == ow)
            .unwrap()
            .host_wall_id,
        w1
    );
    p.s.undo().expect("undo is admitted").expect("entry");
    assert_eq!(
        p.doc()
            .openings
            .iter()
            .find(|o| o.id == ow)
            .unwrap()
            .host_wall_id,
        w1
    );
    p.s.redo().expect("redo is admitted").expect("entry");
    let host = |d: &Document| d.openings.iter().find(|o| o.id == ow).unwrap().host_wall_id;
    assert_eq!(
        host(p.doc()),
        host(&remapped),
        "redo remaps to the same child"
    );
}

/// Codex GPT-6.1-Sol's admission counterexamples (R4.4 increment review):
/// attempted host writes that change nothing, a "split" that leaves a gap
/// or shifts the child off axis, a host deleted and recreated on another
/// level, and `levelId` smuggled into a created opening — all refused.
#[test]
fn admission_bypasses_found_in_review_are_refused() {
    let (mut p, levels) = plan_with_levels(2);
    let a = p.add_wall(levels[0], (0.0, 0.0), (4000.0, 0.0), 200.0);
    let b = p.add_wall(levels[0], (0.0, 3000.0), (4000.0, 3000.0), 200.0);
    let o = opening(
        a,
        OpeningKind::Window,
        0.75,
        500.0,
        500.0,
        1000.0,
        100.0,
        Side::Left,
    );
    let oid = o.id;
    p.commit(create(&o)).expect("O");
    let host = |id: Uuid| serde_json::json!(id);

    // 1. Attempted host writes: same value; B then back to A; B then delete.
    for ops in [
        set(oid, "hostWallId", host(a)),
        [
            set(oid, "hostWallId", host(b)),
            set(oid, "hostWallId", host(a)),
        ]
        .concat(),
        [
            set(oid, "hostWallId", host(b)),
            vec![Operation::delete(oid)],
        ]
        .concat(),
    ] {
        assert_eq!(primary(&mut p, ops), "opening-direct-rehost");
    }

    // 2. A child (2000,0)→(4000,0) while A shrinks to (0,0)→(1000,0): a gap,
    //    not a partition.
    let a_wall = p.doc().wall(a).unwrap().clone();
    let mut child = a_wall.clone();
    child.id = Uuid::now_v7();
    child.start_mm = [2000.0, 0.0];
    let child_id = child.id;
    assert_eq!(
        primary(
            &mut p,
            [
                vec![Operation::create(Entity::Wall(child))],
                set(a, "endMm", serde_json::json!([1000.0, 0.0])),
                set(oid, "hostWallId", host(child_id)),
                set(oid, "u", serde_json::json!(0.5)),
            ]
            .concat()
        ),
        "opening-direct-rehost"
    );

    // 3. Deleting and recreating the host on L2 in one batch.
    let mut moved = a_wall.clone();
    moved.level_id = levels[1];
    assert_eq!(
        primary(
            &mut p,
            vec![Operation::delete(a), Operation::create(Entity::Wall(moved))]
        ),
        "entity-id-collision"
    );

    // 4. A created opening carrying `levelId`.
    let mut smuggled = opening(
        b,
        OpeningKind::Door,
        0.5,
        800.0,
        2100.0,
        0.0,
        200.0,
        Side::Left,
    );
    smuggled
        .unknown
        .insert("levelId".to_owned(), serde_json::json!(levels[1]));
    assert_eq!(
        primary(&mut p, create(&smuggled)),
        "opening-level-immutable"
    );
}

/// Sol's off-axis witness: on a 1,000,000 mm host, a "child" shifted
/// 0.0005 mm off the axis is not a split child (the bound is the binary64
/// construction error of a split point, not an input tolerance).
#[test]
fn a_child_shifted_off_axis_is_not_a_split() {
    let (mut p, levels) = plan_with_levels(1);
    let a = p.add_wall(levels[0], (0.0, 0.0), (1_000_000.0, 0.0), 200.0);
    let o = opening(
        a,
        OpeningKind::Window,
        0.75,
        500.0,
        500.0,
        1000.0,
        100.0,
        Side::Left,
    );
    let oid = o.id;
    p.commit(create(&o)).expect("O");
    let mut child = p.doc().wall(a).unwrap().clone();
    child.id = Uuid::now_v7();
    child.start_mm = [500_000.0, 0.0005];
    child.end_mm = [1_000_000.0, 0.0005];
    let child_id = child.id;
    assert_eq!(
        primary(
            &mut p,
            [
                vec![Operation::create(Entity::Wall(child))],
                set(a, "endMm", serde_json::json!([500_000.0, 0.0])),
                set(oid, "hostWallId", serde_json::json!(child_id)),
                set(oid, "u", serde_json::json!(0.5)),
            ]
            .concat()
        ),
        "opening-direct-rehost"
    );
}

/// Sol's compatibility witness: compensating this actor's historical
/// same-id replacement (LWW4) still restores the previous wall — same-kind
/// live creates stay legal at admission.
#[test]
fn compensating_a_historical_replacement_restores_the_wall() {
    use domowik_document::operation::Batch;
    let mut doc = Document::new();
    let level = Level::new("L1");
    let level_id = level.id;
    doc.levels.push(level);
    let wall = Wall::new(
        level_id,
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2500.0,
    );
    doc.walls.push(wall.clone());
    let actor = Uuid::now_v7();
    let mut session = Session::open_snapshot(actor, doc.clone(), &BTreeMap::new(), &[], Vec::new())
        .expect("opens");
    let mut thicker = wall.clone();
    thicker.thickness_mm = 300.0;
    let replacement = Batch::new(
        actor,
        1,
        BTreeMap::new(),
        vec![Operation::create(Entity::Wall(thicker))],
    );
    session
        .open_with(vec![replacement.clone()])
        .expect("historical replay");
    assert_eq!(session.document().walls[0].thickness_mm, 300.0);
    session
        .compensate(&replacement)
        .expect("compensation is admitted");
    assert_eq!(session.document().walls[0].thickness_mm, 200.0);
}

/// Sol's verification witness: a same-id REPLACEMENT (LWW4) may not do
/// what a SetFields may not — move a host wall to another level under its
/// surviving opening, or rehost an opening.
#[test]
fn a_same_id_replacement_cannot_change_level_or_host() {
    let (mut p, levels) = plan_with_levels(2);
    let a = p.add_wall(levels[0], (0.0, 0.0), (4000.0, 0.0), 200.0);
    let b = p.add_wall(levels[0], (0.0, 3000.0), (4000.0, 3000.0), 200.0);
    let o = opening(
        a,
        OpeningKind::Window,
        0.5,
        500.0,
        500.0,
        1000.0,
        100.0,
        Side::Left,
    );
    p.commit(create(&o)).expect("O");
    let mut moved = p.doc().wall(a).unwrap().clone();
    moved.level_id = levels[1];
    let other = Uuid::now_v7();
    let external = |p: &mut Plan, op: Operation| {
        let before = p.doc().clone();
        let batch =
            domowik_document::operation::Batch::new(other, 1, p.s.log().frontier(), vec![op]);
        let err = p.s.accept_external(batch).expect_err("refused");
        assert_eq!(p.doc(), &before);
        err.primary_opening_code().map(|c| c.as_str())
    };
    assert_eq!(
        external(&mut p, Operation::create(Entity::Wall(moved))),
        Some("wall-level-immutable")
    );
    let mut rehosted = o.clone();
    rehosted.host_wall_id = b;
    assert_eq!(
        external(&mut p, Operation::create(Entity::Opening(rehosted))),
        Some("opening-direct-rehost")
    );
}

/// Sol's second verification round: an ordinary local same-id create is a
/// collision; a split may name the retained host explicitly, but must keep
/// the parent id on its START child; unrelated collinear walls and a second
/// split in the same batch do not disturb a split; `hostWallId` on a wall or
/// level is just an unknown field.
#[test]
fn split_verification_second_round() {
    use domowik_edit::split_operations;
    let (mut p, levels) = plan_with_levels(2);
    let a = p.add_wall(levels[0], (0.0, 0.0), (4000.0, 0.0), 200.0);
    let o = opening(
        a,
        OpeningKind::Window,
        0.25,
        500.0,
        500.0,
        1000.0,
        100.0,
        Side::Left,
    );
    let oid = o.id;
    p.commit(create(&o)).expect("O");

    // 1. A local re-create of a live id.
    assert_eq!(primary(&mut p, create(&o)), "entity-id-collision");

    // 2a. A legitimate split with an explicit, unchanged host write.
    let mut ops = split_operations(p.doc(), a, &[Point::new(2000.0, 0.0)]).expect("split");
    ops.extend(set(oid, "hostWallId", serde_json::json!(a)));
    p.commit(ops)
        .expect("an explicit retained-host write within a split is admitted");
    assert_eq!(p.doc().openings[0].host_wall_id, a);
    p.s.undo().expect("undo").expect("entry");

    // 2b. A "split" handing the parent id to the END child.
    let a_wall = p.doc().wall(a).unwrap().clone();
    let mut start_child = a_wall.clone();
    start_child.id = Uuid::now_v7();
    start_child.end_mm = [2000.0, 0.0];
    let c = start_child.id;
    assert_eq!(
        primary(
            &mut p,
            [
                vec![Operation::create(Entity::Wall(start_child))],
                set(a, "startMm", serde_json::json!([2000.0, 0.0])),
                set(oid, "hostWallId", serde_json::json!(c)),
                set(oid, "u", serde_json::json!(0.5)),
            ]
            .concat()
        ),
        "opening-direct-rehost"
    );

    // 3. A real split of A (window moves to the end child) alongside an
    //    unrelated collinear wall on L1 and one on L2.
    let mut o2 = opening(
        a,
        OpeningKind::Window,
        0.75,
        500.0,
        500.0,
        1000.0,
        100.0,
        Side::Left,
    );
    o2.id = Uuid::now_v7();
    p.commit(create(&o2)).expect("O2");
    let mut ops = split_operations(p.doc(), a, &[Point::new(2000.0, 0.0)]).expect("split");
    let far = Wall::new(
        levels[0],
        Point::new(6000.0, 0.0),
        Point::new(10_000.0, 0.0),
        200.0,
        2500.0,
    );
    let upstairs = Wall::new(
        levels[1],
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2500.0,
    );
    ops.push(Operation::create(Entity::Wall(far)));
    ops.push(Operation::create(Entity::Wall(upstairs)));
    p.commit(ops)
        .expect("unrelated collinear walls do not disturb the split");
    assert_ne!(
        p.doc()
            .openings
            .iter()
            .find(|x| x.id == o2.id)
            .unwrap()
            .host_wall_id,
        a
    );

    // 4. `hostWallId` on a wall or a level is an unknown field, not a rehost.
    p.commit(set(a, "hostWallId", serde_json::json!("future metadata")))
        .expect("wall");
    p.commit(set(
        levels[0],
        "hostWallId",
        serde_json::json!("future metadata"),
    ))
    .expect("level");
}

/// Sol's third round: one command creating a fresh id twice is a
/// collision, and a local collision is RANKED (a non-finite width outranks
/// it, as on external admission).
#[test]
fn local_collisions_are_ranked_and_cover_the_same_command() {
    let (mut p, levels) = plan_with_levels(1);
    let a = p.add_wall(levels[0], (0.0, 0.0), (4000.0, 0.0), 200.0);
    let f = opening(
        a,
        OpeningKind::Window,
        0.25,
        500.0,
        500.0,
        1000.0,
        100.0,
        Side::Left,
    );
    assert_eq!(
        primary(&mut p, [create(&f), create(&f)].concat()),
        "entity-id-collision"
    );
    let mut nan = opening(
        a,
        OpeningKind::Window,
        0.75,
        500.0,
        500.0,
        1000.0,
        100.0,
        Side::Left,
    );
    nan.width_mm = f64::NAN;
    let mut clash = Wall::new(
        levels[0],
        Point::new(0.0, 5000.0),
        Point::new(4000.0, 5000.0),
        200.0,
        2500.0,
    );
    clash.id = levels[0];
    assert_eq!(
        primary(
            &mut p,
            [create(&nan), vec![Operation::create(Entity::Wall(clash))]].concat()
        ),
        "nonfinite-value"
    );
    // The colliding create itself carries the nonfinite value (Sol round 3).
    let mut live = f.clone();
    p.commit(create(&live)).expect("creates");
    live.width_mm = f64::NAN;
    assert_eq!(primary(&mut p, create(&live)), "nonfinite-value");
}
