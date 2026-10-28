//! BEHAVIOR-SPEC §4.5 fixtures: the occupied region amended by opening cuts
//! (R4.2.1–R4.2.3), against the literal canonical outlines the fixtures
//! state. Every coordinate is an integer millimetre, so the snap-rounded
//! output must equal them exactly.
//!
//! Command behaviour (effects vectors, rejections, remaps) is pinned by the
//! command tests; this file pins the GEOMETRY each fixture's post-state
//! produces, plus the invariances R4.0.6 (opening order) and R4.2.3
//! (host reversal) require.

use domowik_format::{Opening, Point, Side};
use domowik_geometry::cuts::{drawn_region, Aperture};
use domowik_geometry::polygon::{canonical_ring, Polygon, Winding};
use domowik_geometry::walls::{plan, Endpoint, JoinMember, JoinSpec, WallSolid};
use uuid::Uuid;

fn wall(a: (f64, f64), b: (f64, f64), t: f64) -> WallSolid {
    WallSolid::new(Point::new(a.0, a.1), Point::new(b.0, b.1), t)
}

/// An opening on wall index `host` of `walls`: only `u`, width, depth and
/// side matter in plan (R4.2.1).
fn cut(walls: &[WallSolid], host: usize, u: f64, width: f64, depth: f64, side: Side) -> Aperture {
    let mut o = Opening::door(Uuid::nil(), u, width, 2100.0, depth);
    o.side = side;
    Aperture::of(&o, host, &walls[host]).expect("a host with length")
}

fn outline(points: &[(f64, f64)]) -> Vec<Point> {
    let pts: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
    canonical_ring(&pts, Winding::CounterClockwise).expect("a ring")
}

/// The region's components as canonical holeless outer rings, sorted.
fn components(region: &[Polygon]) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = region
        .iter()
        .map(|p| {
            assert!(p.holes.is_empty(), "no holes in these fixtures");
            canonical_ring(&p.outer, Winding::CounterClockwise).expect("a ring")
        })
        .collect();
    out.sort_by(|a, b| a[0].x.total_cmp(&b[0].x).then(a[0].y.total_cmp(&b[0].y)));
    out
}

/// The drawn region, asserted invariant under opening order (R4.0.6) and
/// under reversing every wall with its openings mapped (R4.2.3), and equal
/// to the unamended plan when there is nothing to cut.
fn drawn(walls: &[WallSolid], joins: &[JoinSpec], cuts: &[Aperture]) -> Vec<Vec<Point>> {
    let region = drawn_region(walls, joins, cuts);
    let mut reordered = cuts.to_vec();
    reordered.reverse();
    assert_eq!(
        drawn_region(walls, joins, &reordered),
        region,
        "opening order"
    );
    // R4.3.1: reversing a host maps u → 1 − u and side → opposite.
    let reversed: Vec<WallSolid> = walls
        .iter()
        .map(|w| WallSolid::new(w.end, w.start, w.thickness_mm))
        .collect();
    let flip = |e: Endpoint| match e {
        Endpoint::Start => Endpoint::End,
        Endpoint::End => Endpoint::Start,
    };
    let rjoins: Vec<JoinSpec> = joins
        .iter()
        .map(|j| JoinSpec {
            members: j
                .members
                .iter()
                .map(|m| JoinMember {
                    wall: m.wall,
                    endpoint: flip(m.endpoint),
                })
                .collect(),
        })
        .collect();
    let rcuts: Vec<Aperture> = cuts
        .iter()
        .map(|a| Aperture {
            s: (1.0 - a.s.1, 1.0 - a.s.0),
            side: match a.side {
                Side::Left => Side::Right,
                Side::Right => Side::Left,
            },
            ..*a
        })
        .collect();
    assert_eq!(
        drawn_region(&reversed, &rjoins, &rcuts),
        region,
        "host reversal"
    );
    if cuts.is_empty() {
        assert_eq!(region, plan(walls, joins).0);
    }
    components(&region)
}

fn f1() -> Vec<WallSolid> {
    vec![wall((0.0, 0.0), (4000.0, 0.0), 200.0)]
}

const BAND: [(f64, f64); 4] = [
    (0.0, -100.0),
    (4000.0, -100.0),
    (4000.0, 100.0),
    (0.0, 100.0),
];

/// The F26 window: u = 1/2, 1200 wide, 100 deep from the Left (+y) face.
fn f26(w: &[WallSolid]) -> Aperture {
    cut(w, 0, 0.5, 1200.0, 100.0, Side::Left)
}

const F26_OUTLINE: [(f64, f64); 8] = [
    (0.0, -100.0),
    (4000.0, -100.0),
    (4000.0, 100.0),
    (2600.0, 100.0),
    (2600.0, 0.0),
    (1400.0, 0.0),
    (1400.0, 100.0),
    (0.0, 100.0),
];

#[test]
fn f25_a_full_thickness_door_disconnects_the_band() {
    let w = f1();
    let door = cut(&w, 0, 0.5, 900.0, 200.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[door]),
        vec![
            outline(&[
                (0.0, -100.0),
                (1550.0, -100.0),
                (1550.0, 100.0),
                (0.0, 100.0)
            ]),
            outline(&[
                (2450.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (2450.0, 100.0)
            ]),
        ]
    );
}

#[test]
fn f26_a_partial_depth_window_notches_the_band() {
    let w = f1();
    assert_eq!(drawn(&w, &[], &[f26(&w)]), vec![outline(&F26_OUTLINE)]);
}

#[test]
fn f27_touching_openings_and_a_transom() {
    let w = f1();
    let door = cut(&w, 0, 0.25, 800.0, 200.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[door]),
        vec![
            outline(&[(0.0, -100.0), (600.0, -100.0), (600.0, 100.0), (0.0, 100.0)]),
            outline(&[
                (1400.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (1400.0, 100.0)
            ]),
        ]
    );
    let window = cut(&w, 0, 9.0 / 20.0, 800.0, 100.0, Side::Left);
    let step2 = vec![
        outline(&[(0.0, -100.0), (600.0, -100.0), (600.0, 100.0), (0.0, 100.0)]),
        outline(&[
            (1400.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (2200.0, 100.0),
            (2200.0, 0.0),
            (1400.0, 0.0),
        ]),
    ];
    assert_eq!(drawn(&w, &[], &[door, window]), step2);
    // Step 3: a transom inside the door's footprint changes nothing.
    let transom = cut(&w, 0, 0.25, 800.0, 100.0, Side::Left);
    assert_eq!(drawn(&w, &[], &[door, window, transom]), step2);
}

#[test]
fn f28_b_and_c_cuts_beside_the_window() {
    let w = f1();
    let ob = cut(&w, 0, 3.0 / 8.0, 800.0, 200.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[f26(&w), ob]),
        vec![
            outline(&[
                (0.0, -100.0),
                (1100.0, -100.0),
                (1100.0, 100.0),
                (0.0, 100.0)
            ]),
            outline(&[
                (1900.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (2600.0, 100.0),
                (2600.0, 0.0),
                (1900.0, 0.0),
            ]),
        ]
    );
    let oc = cut(&w, 0, 0.75, 800.0, 200.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[f26(&w), oc]),
        vec![
            outline(&[
                (0.0, -100.0),
                (2600.0, -100.0),
                (2600.0, 0.0),
                (1400.0, 0.0),
                (1400.0, 100.0),
                (0.0, 100.0),
            ]),
            outline(&[
                (3400.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (3400.0, 100.0)
            ]),
        ]
    );
}

/// F29 (the reversal itself is exercised by `drawn`) and F30: u = 3/10 on
/// the 4000 wall, then the wall shortened to 3600 keeping u.
#[test]
fn f29_and_f30_window_at_three_tenths() {
    let w = f1();
    let ow = cut(&w, 0, 0.3, 1200.0, 100.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[ow]),
        vec![outline(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (1800.0, 100.0),
            (1800.0, 0.0),
            (600.0, 0.0),
            (600.0, 100.0),
            (0.0, 100.0),
        ])]
    );
    let short = vec![wall((0.0, 0.0), (3600.0, 0.0), 200.0)];
    let ow = cut(&short, 0, 0.3, 1200.0, 100.0, Side::Left);
    assert_eq!(
        drawn(&short, &[], &[ow]),
        vec![outline(&[
            (0.0, -100.0),
            (3600.0, -100.0),
            (3600.0, 100.0),
            (1680.0, 100.0),
            (1680.0, 0.0),
            (480.0, 0.0),
            (480.0, 100.0),
            (0.0, 100.0),
        ])]
    );
}

/// The four-member crossing join of F32 / F32b.
fn crossing(walls: &[WallSolid]) -> Vec<JoinSpec> {
    vec![JoinSpec {
        members: vec![
            JoinMember {
                wall: 0,
                endpoint: Endpoint::End,
            },
            JoinMember {
                wall: 1,
                endpoint: Endpoint::Start,
            },
            JoinMember {
                wall: 2,
                endpoint: Endpoint::End,
            },
            JoinMember {
                wall: 3,
                endpoint: Endpoint::Start,
            },
        ],
    }]
    .into_iter()
    .filter(|_| walls.len() == 4)
    .collect()
}

#[test]
fn f32_a_split_keeps_the_window_and_the_patch() {
    let w = vec![
        wall((0.0, 0.0), (3000.0, 0.0), 200.0),
        wall((3000.0, 0.0), (4000.0, 0.0), 200.0),
        wall((3000.0, -1500.0), (3000.0, 0.0), 300.0),
        wall((3000.0, 0.0), (3000.0, 1500.0), 300.0),
    ];
    let ow = cut(&w, 0, 0.4, 1200.0, 100.0, Side::Left);
    assert_eq!(
        drawn(&w, &crossing(&w), &[ow]),
        vec![outline(&[
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
}

#[test]
fn f32b_the_window_remapped_to_a_new_child() {
    let w = vec![
        wall((0.0, 0.0), (500.0, 0.0), 200.0),
        wall((500.0, 0.0), (4000.0, 0.0), 200.0),
        wall((500.0, -1000.0), (500.0, 0.0), 200.0),
        wall((500.0, 0.0), (500.0, 1000.0), 200.0),
    ];
    let ow = cut(&w, 1, 0.2, 1200.0, 100.0, Side::Left);
    assert_eq!(
        drawn(&w, &crossing(&w), &[ow]),
        vec![outline(&[
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
}

/// F35: a door 200 mm from a nondegenerate join; the patch is never cut.
#[test]
fn f35_the_join_patch_is_never_subtracted() {
    let w = vec![
        wall((0.0, 0.0), (3000.0, 0.0), 200.0),
        wall((3000.0, 0.0), (3000.0, 2500.0), 300.0),
    ];
    let joins = vec![JoinSpec {
        members: vec![
            JoinMember {
                wall: 0,
                endpoint: Endpoint::End,
            },
            JoinMember {
                wall: 1,
                endpoint: Endpoint::Start,
            },
        ],
    }];
    let door = cut(&w, 0, 5.0 / 6.0, 600.0, 200.0, Side::Left);
    assert_eq!(
        drawn(&w, &joins, &[door]),
        vec![
            outline(&[
                (0.0, -100.0),
                (2200.0, -100.0),
                (2200.0, 100.0),
                (0.0, 100.0)
            ]),
            outline(&[
                (2800.0, -100.0),
                (3000.0, -100.0),
                (3150.0, 0.0),
                (3150.0, 2500.0),
                (2850.0, 2500.0),
                (2850.0, 100.0),
                (2800.0, 100.0),
            ]),
        ]
    );
}

/// F36 (no opening: the bare band) and F37's geometric updates.
#[test]
fn f36_and_f37_the_window_moved_resized_flipped_and_deepened() {
    let w = f1();
    assert_eq!(drawn(&w, &[], &[]), vec![outline(&BAND)]);
    let flipped = cut(&w, 0, 0.5, 1200.0, 100.0, Side::Right);
    assert_eq!(
        drawn(&w, &[], &[flipped]),
        vec![outline(&[
            (0.0, -100.0),
            (1400.0, -100.0),
            (1400.0, 0.0),
            (2600.0, 0.0),
            (2600.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (0.0, 100.0),
        ])],
        "(b) side"
    );
    let moved = cut(&w, 0, 5.0 / 8.0, 1200.0, 100.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[moved]),
        vec![outline(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (3100.0, 100.0),
            (3100.0, 0.0),
            (1900.0, 0.0),
            (1900.0, 100.0),
            (0.0, 100.0),
        ])],
        "(g) u"
    );
    let narrow = cut(&w, 0, 0.5, 800.0, 100.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[narrow]),
        vec![outline(&[
            (0.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (2400.0, 100.0),
            (2400.0, 0.0),
            (1600.0, 0.0),
            (1600.0, 100.0),
            (0.0, 100.0),
        ])],
        "(h) width"
    );
    let deep = cut(&w, 0, 0.5, 1200.0, 200.0, Side::Left);
    assert_eq!(
        drawn(&w, &[], &[deep]),
        vec![
            outline(&[
                (0.0, -100.0),
                (1400.0, -100.0),
                (1400.0, 100.0),
                (0.0, 100.0)
            ]),
            outline(&[
                (2600.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (2600.0, 100.0)
            ]),
        ],
        "(j) depth = thickness"
    );
}

/// The drawn region never changes rooms (R4.2.5): a full-thickness door in a
/// closed room leaves `plan`'s room untouched while the drawn region opens.
#[test]
fn cuts_never_change_rooms() {
    let c = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
    let w: Vec<WallSolid> = (0..4).map(|i| wall(c[i], c[(i + 1) % 4], 200.0)).collect();
    let door = cut(&w, 0, 0.5, 900.0, 200.0, Side::Left);
    let (_, rooms) = plan(&w, &[]);
    assert_eq!(rooms.len(), 1, "the unamended plan still has its room");
    let region = drawn_region(&w, &[], &[door]);
    assert_ne!(region, plan(&w, &[]).0, "the drawn region is cut");
}

/// F38: identical walls on two levels, a door on each in turn; every
/// level's drawn region is its own (`level_drawn_region`).
#[test]
fn f38_cuts_stay_on_their_own_level() {
    use domowik_format::{Document, Level, Wall};
    use domowik_geometry::cuts::level_drawn_region;
    let mut doc = Document::new();
    let (l1, l2) = (Level::new("L1"), Level::new("L2"));
    let (l1, l2) = (
        {
            let id = l1.id;
            doc.levels.push(l1);
            id
        },
        {
            let id = l2.id;
            doc.levels.push(l2);
            id
        },
    );
    let w1 = Wall::new(
        l1,
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2500.0,
    );
    let w2 = Wall::new(
        l2,
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2700.0,
    );
    let (w1_id, w2_id) = (w1.id, w2.id);
    doc.walls.extend([w1, w2]);
    let level = |doc: &Document, l| components(&level_drawn_region(doc, l));
    let split = vec![
        outline(&[
            (0.0, -100.0),
            (1550.0, -100.0),
            (1550.0, 100.0),
            (0.0, 100.0),
        ]),
        outline(&[
            (2450.0, -100.0),
            (4000.0, -100.0),
            (4000.0, 100.0),
            (2450.0, 100.0),
        ]),
    ];
    doc.openings
        .push(Opening::door(w1_id, 0.5, 900.0, 2100.0, 200.0));
    assert_eq!(level(&doc, l1), split);
    assert_eq!(level(&doc, l2), vec![outline(&BAND)]);
    doc.openings
        .push(Opening::door(w2_id, 0.5, 900.0, 2100.0, 200.0));
    assert_eq!(level(&doc, l1), split);
    assert_eq!(level(&doc, l2), split);
}

/// Astra's patch-preservation probe: a full-depth cut on W1 reaching INTO
/// the F35 join's diamond. The diamond is never cut, and W1's cut never
/// erases W2: a subtract-after-union implementation fails both points.
#[test]
fn a_cut_reaching_into_a_join_patch_spares_the_patch_and_the_neighbour() {
    use domowik_geometry::arrangement::{classify_point, Containment};
    let w = vec![
        wall((0.0, 0.0), (3000.0, 0.0), 200.0),
        wall((3000.0, 0.0), (3000.0, 2500.0), 300.0),
    ];
    let joins = vec![JoinSpec {
        members: vec![
            JoinMember {
                wall: 0,
                endpoint: Endpoint::End,
            },
            JoinMember {
                wall: 1,
                endpoint: Endpoint::Start,
            },
        ],
    }];
    // Span [2700, 2950] of the 3000 mm host.
    let door = Aperture {
        wall: 0,
        s: (2700.0 / 3000.0, 2950.0 / 3000.0),
        side: Side::Left,
        depth: 200.0,
    };
    let region = drawn_region(&w, &joins, &[door]);
    let inside = |x: f64, y: f64| {
        region.iter().any(|p| {
            classify_point(Point::new(x, y), &p.outer) != Containment::Outside
                && p.holes
                    .iter()
                    .all(|h| classify_point(Point::new(x, y), h) != Containment::Inside)
        })
    };
    assert!(
        inside(2900.0, -20.0),
        "inside the diamond, outside W2: kept"
    );
    assert!(inside(2900.0, 50.0), "inside W2, outside the diamond: kept");
    assert!(
        !inside(2750.0, 0.0),
        "inside the cut, outside everything else: removed"
    );
}

/// Cuts from opposite faces whose depths add up to the thickness remove it
/// together; a cut covered by a deeper one changes nothing; a host's pieces
/// are seam-free single contours.
#[test]
fn opposite_and_redundant_cuts_and_seam_free_pieces() {
    use domowik_geometry::cuts::wall_pieces;
    let w = f1();
    let left = Aperture {
        wall: 0,
        s: (0.25, 0.5),
        side: Side::Left,
        depth: 120.0,
    };
    let right = Aperture {
        wall: 0,
        s: (0.375, 0.625),
        side: Side::Right,
        depth: 80.0,
    };
    // Left removes η ∈ [−20, 100] over [1000, 2000]; Right removes
    // [−100, −20] over [1500, 2500]; together the full thickness over
    // [1500, 2000].
    assert_eq!(
        drawn(&w, &[], &[left, right]),
        vec![
            outline(&[
                (0.0, -100.0),
                (1500.0, -100.0),
                (1500.0, -20.0),
                (1000.0, -20.0),
                (1000.0, 100.0),
                (0.0, 100.0)
            ]),
            outline(&[
                (2000.0, -20.0),
                (2500.0, -20.0),
                (2500.0, -100.0),
                (4000.0, -100.0),
                (4000.0, 100.0),
                (2000.0, 100.0)
            ]),
        ]
    );
    // A shallower cut inside a deeper one is redundant.
    let deep = Aperture {
        wall: 0,
        s: (0.25, 0.75),
        side: Side::Left,
        depth: 150.0,
    };
    let shallow = Aperture {
        wall: 0,
        s: (0.375, 0.5),
        side: Side::Left,
        depth: 50.0,
    };
    assert_eq!(
        wall_pieces(&w[0], &[deep, shallow]),
        wall_pieces(&w[0], &[deep])
    );
    // F26's host: ONE contour with eight vertices — no tile seams.
    let pieces = wall_pieces(&w[0], &[f26(&w)]);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].len(), 8);
    // No effective cut: exactly the wall rectangle.
    assert_eq!(
        wall_pieces(&w[0], &[]),
        vec![w[0].rectangle().unwrap().to_vec()]
    );
}

#[test]
fn side_pieces_split_a_cut_wall_at_its_axis_and_tile_its_pieces() {
    use domowik_geometry::cuts::{wall_pieces, wall_side_pieces};
    let walls = [wall((0.0, 0.0), (4000.0, 0.0), 200.0)];
    let area = |rings: Vec<Vec<Point>>| -> f64 {
        rings.into_iter().map(|r| Polygon::new(r).area_mm2()).sum()
    };
    // A full-depth door and a shallow Left niche: the Right half survives
    // behind the niche, the Left half does not exist at the door.
    let cuts = [
        cut(&walls, 0, 0.25, 900.0, 200.0, Side::Right),
        cut(&walls, 0, 0.75, 600.0, 80.0, Side::Left),
    ];
    let whole = area(wall_pieces(&walls[0], &cuts));
    let left = wall_side_pieces(&walls[0], &cuts, Side::Left);
    let right = wall_side_pieces(&walls[0], &cuts, Side::Right);
    assert!(left.iter().flatten().all(|p| p.y >= 0.0));
    assert!(right.iter().flatten().all(|p| p.y <= 0.0));
    assert_eq!(right.len(), 2, "the door separates the Right half");
    assert_eq!(area(right.clone()), (4000.0 - 900.0) * 100.0);
    assert_eq!(area(left) + area(right), whole);
    // Uncut walls split into two exact half rectangles.
    let uncut = wall_side_pieces(&walls[0], &[], Side::Left);
    assert_eq!(area(uncut), 4000.0 * 100.0);
}
