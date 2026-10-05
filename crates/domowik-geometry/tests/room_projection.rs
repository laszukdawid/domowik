//! Adversarial scrutiny of the room projection, written because the advisory
//! round produced no adversary.
//!
//! The required Codex review (`gpt-5.6-sol`, high effort) started correctly —
//! its banner confirmed the model and effort — and then returned nothing but
//! "Your workspace is out of credits", with exit status 0. INSTRUCTIONS.md
//! says a failed advisory round is not a licence to skip the scrutiny, so
//! these are the tests that review would have demanded: one per attack
//! question it was asked, each answering with a construction rather than an
//! argument.

use domowik_format::{Document, JoinMember, Level, Point, Role, RoomRecord, Wall, WallJoin};
use domowik_geometry::rooms::{self, RoomStatus};
use uuid::Uuid;

/// Adds a closed loop of `t=200` walls through `corners`, with corner joins.
fn loop_of(doc: &mut Document, level: Uuid, corners: &[(f64, f64)]) {
    let n = corners.len();
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let a = corners[i];
        let b = corners[(i + 1) % n];
        let wall = Wall::new(
            level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.0,
            2500.0,
        );
        ids.push(wall.id);
        doc.walls.push(wall);
    }
    for i in 0..n {
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(ids[i], Role::End),
            JoinMember::new(ids[(i + 1) % n], Role::Start),
        ]));
    }
}

fn level_of(doc: &mut Document) -> Uuid {
    let level = Level::new("L1");
    let id = level.id;
    doc.levels.push(level);
    id
}

fn seed(doc: &mut Document, level: Uuid, id: u128, at: (f64, f64)) -> Uuid {
    let record = RoomRecord::new(Uuid::from_u128(id), level, Point::new(at.0, at.1));
    let uuid = record.id;
    doc.room_records.push(record);
    uuid
}

/// A fixture that builds a document and names its level.
type Arrangement = fn() -> (Document, Uuid);

/// BEHAVIOR-SPEC §5.4's base arrangement B4.
fn b4() -> (Document, Uuid) {
    let mut doc = Document::new();
    let level = level_of(&mut doc);
    loop_of(
        &mut doc,
        level,
        &[(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)],
    );
    (doc, level)
}

/// F27's arrangement: an outer loop with an island inside it.
fn courtyard_and_island() -> (Document, Uuid) {
    let mut doc = Document::new();
    let level = level_of(&mut doc);
    loop_of(
        &mut doc,
        level,
        &[(0.0, 0.0), (8000.0, 0.0), (8000.0, 6000.0), (0.0, 6000.0)],
    );
    loop_of(
        &mut doc,
        level,
        &[
            (2500.0, 2000.0),
            (5500.0, 2000.0),
            (5500.0, 4000.0),
            (2500.0, 4000.0),
        ],
    );
    (doc, level)
}

// ---------------------------------------------------------------------------
// 1. Seed classification: can a status be wrong, or can two of them apply?
// ---------------------------------------------------------------------------

/// Every status is reachable, and each seed gets exactly one.
///
/// Exclusivity is by construction here rather than by assertion — the status
/// is a single enum value — so what is worth testing is that the CLASSIFIER
/// agrees with the geometry at each boundary, which is where a
/// nearly-right implementation would report `associated` for a seed on a wall
/// face or `seed-in-exterior` for one inside a hole.
#[test]
fn every_seed_position_gets_the_one_status_the_geometry_says() {
    let (mut doc, level) = courtyard_and_island();
    // The occupied region here is the outer band, x,y in [-100, 8100] minus
    // its interior, plus the island block (2400,1900)-(5600,4100) with
    // chamfered corners.
    let cases: [((f64, f64), RoomStatus, &str); 9] = [
        // Strictly inside the courtyard annulus.
        (
            (1200.0, 3000.0),
            RoomStatus::Associated,
            "courtyard interior",
        ),
        // Strictly inside the island, which is a HOLE of the courtyard face
        // and a face of its own.
        ((4000.0, 3000.0), RoomStatus::Associated, "island interior"),
        // On the inner edge of the outer band: the courtyard face's own ring.
        (
            (1200.0, 100.0),
            RoomStatus::SeedOnBoundary,
            "outer ring edge",
        ),
        // On a VERTEX of that ring.
        (
            (100.0, 100.0),
            RoomStatus::SeedOnBoundary,
            "outer ring corner",
        ),
        // On the island's outward face, which is a hole ring of the courtyard
        // and therefore still the boundary of the occupied region.
        (
            (4000.0, 1900.0),
            RoomStatus::SeedOnBoundary,
            "hole ring edge",
        ),
        // On the island face's OWN ring, the inward side of the same wall.
        (
            (4000.0, 2100.0),
            RoomStatus::SeedOnBoundary,
            "island ring edge",
        ),
        // Between the two: inside the island wall's band.
        (
            (4000.0, 2000.0),
            RoomStatus::SeedInWall,
            "island wall interior",
        ),
        // Inside the outer wall's band.
        ((1200.0, 0.0), RoomStatus::SeedInWall, "outer wall interior"),
        // Beyond everything.
        ((20000.0, 3000.0), RoomStatus::SeedInExterior, "exterior"),
    ];
    for (i, (at, expected, what)) in cases.into_iter().enumerate() {
        let id = seed(&mut doc, level, i as u128 + 1, at);
        let projected = rooms::project_level(&doc, level);
        let record = projected.record(id).expect("projected");
        assert_eq!(record.status, expected, "{what} at {at:?}");
        assert_eq!(
            record.face.is_some(),
            expected.is_candidate(),
            "{what}: a face and candidacy must agree"
        );
        doc.room_records.clear();
    }
}

/// A seed inside a hole belongs to the face the hole encloses, never to the
/// face that carries the hole.
///
/// The specific defect this rules out: testing containment against a face's
/// OUTER ring alone. That reports the island's interior as courtyard, and it
/// would have passed every rectangular-room test in the suite.
#[test]
fn a_seed_inside_a_hole_is_not_a_candidate_of_the_holed_face() {
    let (mut doc, level) = courtyard_and_island();
    let courtyard = seed(&mut doc, level, 1, (1200.0, 3000.0));
    let island = seed(&mut doc, level, 2, (4000.0, 3000.0));
    let projected = rooms::project_level(&doc, level);

    let c = projected
        .record(courtyard)
        .expect("courtyard")
        .face
        .expect("f");
    let i = projected.record(island).expect("island").face.expect("f");
    assert_ne!(c, i, "two faces, not one");
    assert_eq!(
        projected.faces[c].holes.len(),
        1,
        "the courtyard carries the island as a hole"
    );
    assert!(projected.faces[i].holes.is_empty());
    // And each is its own face's winner: neither supersedes the other.
    assert_eq!(projected.effective_record(c), Some(courtyard));
    assert_eq!(projected.effective_record(i), Some(island));
}

/// F35's point-pinch: two lobes touching at one occupied point stay distinct,
/// and a seed at the pinch itself is classified once and the same way twice.
#[test]
fn a_pinch_between_two_occupied_regions_classifies_once_and_deterministically() {
    let mut doc = Document::new();
    let level = level_of(&mut doc);
    loop_of(
        &mut doc,
        level,
        &[(0.0, 0.0), (2000.0, 0.0), (2000.0, 2000.0), (0.0, 2000.0)],
    );
    loop_of(
        &mut doc,
        level,
        &[
            (2000.0, 2000.0),
            (4000.0, 2000.0),
            (4000.0, 4000.0),
            (2000.0, 4000.0),
        ],
    );
    let a = seed(&mut doc, level, 1, (1000.0, 1000.0));
    let b = seed(&mut doc, level, 2, (3000.0, 3000.0));
    let pinch = seed(&mut doc, level, 3, (2000.0, 2000.0));

    let first = rooms::project_level(&doc, level);
    assert_eq!(first.faces.len(), 2, "two lobes, no figure-eight face");
    let fa = first.record(a).expect("a").face.expect("f");
    let fb = first.record(b).expect("b").face.expect("f");
    assert_ne!(fa, fb, "the lobes share no free-space point");
    assert_eq!(first.record(a).expect("a").status, RoomStatus::Associated);
    assert_eq!(first.record(b).expect("b").status, RoomStatus::Associated);

    // The pinch is occupied, so it is no lobe's candidate — and specifically
    // it is INSIDE the occupied region rather than on its boundary.
    //
    // That distinction is the regression. `walls::plan` merges the touching
    // loops into ONE canonical component, so no ring passes through the pinch
    // and the strict-interior branch is reached. Were the union ever to emit
    // the lobes as two members sharing that point, this would report
    // `seed-on-boundary`, which is the failure `project_against`'s
    // precondition describes.
    let at_pinch = first.record(pinch).expect("pinch");
    assert_eq!(
        at_pinch.status,
        RoomStatus::SeedInWall,
        "all four quadrants around the pinch are occupied, so the point is \
         interior to the union, not on its boundary"
    );
    assert_eq!(at_pinch.face, None);
    assert_eq!(
        first.occupied.len(),
        1,
        "the union must merge the touching loops"
    );

    // Projection is a pure function: the same document twice, the same answer.
    let second = rooms::project_level(&doc, level);
    assert_eq!(first.records, second.records);
}

// ---------------------------------------------------------------------------
// 2. Is "first after sorting" the same as "lowest stable UUID of that face"?
// ---------------------------------------------------------------------------

/// Per-face winners are chosen within each face, not from the global order.
///
/// The construction is the point: face A's candidates are 20 and 40, face B's
/// are 10 and 30. A global-minimum bug would give face A no winner, or give
/// it 10, which is not even its candidate. Records are pushed in DESCENDING
/// order so document order cannot accidentally be right.
#[test]
fn each_face_gets_its_own_lowest_uuid_not_the_documents() {
    let mut doc = Document::new();
    let level = level_of(&mut doc);
    loop_of(
        &mut doc,
        level,
        &[(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)],
    );
    loop_of(
        &mut doc,
        level,
        &[
            (10000.0, 0.0),
            (14000.0, 0.0),
            (14000.0, 3000.0),
            (10000.0, 3000.0),
        ],
    );
    // Descending ids, and interleaved between the two rooms.
    let r40 = seed(&mut doc, level, 40, (2500.0, 1500.0));
    let r30 = seed(&mut doc, level, 30, (12500.0, 1500.0));
    let r20 = seed(&mut doc, level, 20, (1500.0, 1500.0));
    let r10 = seed(&mut doc, level, 10, (11500.0, 1500.0));

    let p = rooms::project_level(&doc, level);
    assert_eq!(p.faces.len(), 2);
    let status = |id| p.record(id).expect("projected").status;
    assert_eq!(
        status(r20),
        RoomStatus::Associated,
        "20 beats 40 in its face"
    );
    assert_eq!(status(r40), RoomStatus::Superseded);
    assert_eq!(
        status(r10),
        RoomStatus::Associated,
        "10 beats 30 in its face"
    );
    assert_eq!(status(r30), RoomStatus::Superseded);

    // Exactly one winner per face, and each winner is a candidate of the face
    // it won.
    for face in 0..p.faces.len() {
        let winner = p.effective_record(face).expect("every face has candidates");
        assert_eq!(p.record(winner).expect("w").face, Some(face));
        assert_eq!(p.record(winner).expect("w").status, RoomStatus::Associated);
    }
    assert_eq!(
        p.records
            .iter()
            .filter(|r| r.status == RoomStatus::Associated)
            .count(),
        2,
        "one winner per face, no more"
    );
}

/// The order is over the whole 128-bit identifier, not a prefix or a hash.
#[test]
fn the_tie_break_uses_the_full_128_bit_identifier() {
    let (mut doc, level) = b4();
    // Two ids differing only in the LAST bit. A comparison on the high half
    // would call these equal and pick whichever it met first.
    let high = seed(&mut doc, level, (1u128 << 64) | 2, (2000.0, 1500.0));
    let low = seed(&mut doc, level, (1u128 << 64) | 1, (2500.0, 1500.0));
    let p = rooms::project_level(&doc, level);
    assert_eq!(p.effective_record(0), Some(low));
    assert_eq!(p.record(high).expect("high").status, RoomStatus::Superseded);
}

// ---------------------------------------------------------------------------
// 3. Is the display rounding R5.3.2, or only R5.3.2 on the worked examples?
// ---------------------------------------------------------------------------

/// The rule, implemented independently in exact integer arithmetic.
///
/// Deliberately NOT the same shape as the implementation: this divides with a
/// remainder and compares `2 * r` against the divisor, so a tie is recognised
/// exactly and no floating-point value is involved anywhere.
fn reference(area_mm2: u64) -> String {
    let (q, r) = (area_mm2 / 10_000, area_mm2 % 10_000);
    let hundredths = match (r * 2).cmp(&10_000) {
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Less => q,
        // An exact half goes to the even neighbour.
        std::cmp::Ordering::Equal if q % 2 == 0 => q,
        std::cmp::Ordering::Equal => q + 1,
    };
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// A differential sweep, weighted towards the ties.
///
/// The four worked examples prove almost nothing on their own: an
/// implementation that truncated would pass two of them. This walks every
/// remainder in a neighbourhood of the tie for many magnitudes, which is where
/// a wrong rounding mode shows and where a division that lost the tie would.
///
/// It is a SAMPLE, not a proof of equivalence, and the implementation is known
/// not to be equivalent in general — see `display_area_m2`'s own doc for the
/// counterexample. Establishing a bound belongs with the exact kernel (issue
/// 0030), and so do regressions for it.
#[test]
fn the_display_rule_matches_an_exact_integer_reference() {
    let mut checked = 0usize;
    for base in [
        0u64,
        10_000,
        1_000_000,
        10_640_000,
        30_240_000,
        1_000_000_000,
        400_000_000_000_000, // the widest plan the coordinate bound admits
    ] {
        for step in 0..40u64 {
            // Both sides of every tie, and the ties themselves.
            for offset in [0u64, 4_998, 4_999, 5_000, 5_001, 5_002, 9_999] {
                let area = base + step * 10_000 + offset;
                assert_eq!(
                    rooms::display_area_m2(area as f64),
                    reference(area),
                    "area {area} mm²"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 1_000, "the sweep must actually sweep");
}

/// The four examples R5.3.2 spells out, including both tie directions, and the
/// zero boundary — where the rule's prose and its tie rule agree rather than
/// compete.
///
/// 0.005 m² is 5,000 mm², an exact tie, and ties go to the even last digit —
/// 0. So "any value below 0.005 m² displays as 0.00" and the tie rule give the
/// same answer at exactly 0.005, and 5,001 mm² is the first value that shows
/// anything (issue 0030, 2026-09-12).
#[test]
fn the_worked_examples_and_the_zero_boundary() {
    assert_eq!(rooms::display_area_m2(10_640_000.0), "10.64");
    assert_eq!(rooms::display_area_m2(38_200_000.0), "38.20");
    assert_eq!(rooms::display_area_m2(30_245_000.0), "30.24");
    assert_eq!(rooms::display_area_m2(10_655_000.0), "10.66");
    assert_eq!(rooms::display_area_m2(4_999.0), "0.00");
    assert_eq!(rooms::display_area_m2(5_000.0), "0.00", "the tie, to even");
    assert_eq!(rooms::display_area_m2(5_001.0), "0.01");
}

/// Nothing renders as negative zero, whatever arrives.
#[test]
fn no_input_produces_negative_zero_or_a_leading_minus() {
    for input in [
        0.0,
        -0.0,
        -1.0,
        -1e15,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
    ] {
        let shown = rooms::display_area_m2(input);
        assert!(!shown.starts_with('-'), "input {input} rendered {shown}");
        assert!(shown.contains('.'), "always two decimals: {shown}");
    }
}

// ---------------------------------------------------------------------------
// 4. Can interior_point miss?
// ---------------------------------------------------------------------------

/// Every face of every arrangement here yields a point that actually seeds a
/// record into that face.
///
/// Asserted through the PROJECTION rather than through a containment helper:
/// the property that matters is not "the point classifies inside" but "a
/// record seeded there associates with that face", which is what a caller
/// does with it.
#[test]
fn an_interior_point_seeds_the_face_it_came_from() {
    let arrangements: Vec<(&str, Arrangement)> = vec![
        ("rectangle", b4 as Arrangement),
        ("courtyard and island", courtyard_and_island),
        ("L", || {
            let mut doc = Document::new();
            let level = level_of(&mut doc);
            loop_of(
                &mut doc,
                level,
                &[
                    (0.0, 0.0),
                    (8000.0, 0.0),
                    (8000.0, 3000.0),
                    (3000.0, 3000.0),
                    (3000.0, 6000.0),
                    (0.0, 6000.0),
                ],
            );
            (doc, level)
        }),
        ("U", || {
            let mut doc = Document::new();
            let level = level_of(&mut doc);
            loop_of(
                &mut doc,
                level,
                &[
                    (0.0, 0.0),
                    (9000.0, 0.0),
                    (9000.0, 6000.0),
                    (6000.0, 6000.0),
                    (6000.0, 2000.0),
                    (3000.0, 2000.0),
                    (3000.0, 6000.0),
                    (0.0, 6000.0),
                ],
            );
            (doc, level)
        }),
        ("thin corridor", || {
            let mut doc = Document::new();
            let level = level_of(&mut doc);
            loop_of(
                &mut doc,
                level,
                &[(0.0, 0.0), (12000.0, 0.0), (12000.0, 900.0), (0.0, 900.0)],
            );
            (doc, level)
        }),
    ];

    for (what, build) in arrangements {
        let (mut doc, level) = build();
        let faces = rooms::project_level(&doc, level).faces;
        assert!(!faces.is_empty(), "{what}: the fixture must enclose a face");
        for (index, face) in faces.iter().enumerate() {
            let point = rooms::interior_point(face)
                .unwrap_or_else(|| panic!("{what}: face {index} yielded no interior point"));
            doc.room_records.clear();
            let id = seed(&mut doc, level, 1, (point.x, point.y));
            let projected = rooms::project_level(&doc, level);
            let record = projected.record(id).expect("projected");
            assert_eq!(
                record.status,
                RoomStatus::Associated,
                "{what}: a record seeded at face {index}'s own interior point \
                 must associate, not land in a wall"
            );
            assert_eq!(
                projected.faces[record.face.expect("a face")].area_mm2(),
                face.area_mm2(),
                "{what}: and it must associate with THAT face"
            );
        }
    }
}

/// A ring with fewer than three vertices has no interior, and says so rather
/// than returning a point.
#[test]
fn a_degenerate_ring_yields_no_interior_point() {
    use domowik_geometry::polygon::Polygon;
    for outer in [
        vec![],
        vec![Point::new(0.0, 0.0)],
        vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0)],
    ] {
        let polygon = Polygon {
            outer,
            holes: Vec::new(),
        };
        assert!(rooms::interior_point(&polygon).is_none());
    }
}
