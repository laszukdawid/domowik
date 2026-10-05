//! The Part A size corpus, measured (ASSET-STRATEGY §10.9, issue 0035).
//!
//! ROADMAP Phase 1 exits on, among other things, "size targets are measured
//! and published". This builds both Part A scenarios to their normative
//! construction, writes real packages, and asserts the published budgets.
//!
//! # The scenario is now built to its full 1,000 entities
//!
//! `unfurnished-1000-v1` specifies 4 levels, 480 walls, 320 joins, 156
//! openings, and **40 room-metadata records**. The records were the missing
//! forty: BEHAVIOR-SPEC §5.2 was unimplemented, so rooms were derived geometry
//! with nothing persisted, and this measured 960 entities and published a
//! LOWER BOUND rather than a measurement (issue 0060).
//!
//! They exist now, so this builds and measures the whole scenario. The
//! shortfall assertion is kept, inverted: the count is pinned at 1,000 so a
//! regression that dropped the records would fail here rather than quietly
//! restore the old under-measurement.

use domowik_container::{read_package, write_package, Package};
use domowik_format::{
    Document, JoinMember, Level, Opening, Point, Role, RoomRecord, Wall, WallJoin,
};
use uuid::Uuid;

const RING_W: f64 = 12_000.0;
const RING_H: f64 = 9_000.0;
const PITCH_X: f64 = 15_000.0;
const PITCH_Y: f64 = 12_000.0;

/// `empty-v1`: exactly one level, zero entities of every other type.
fn empty_v1() -> Document {
    let mut doc = Document::new();
    doc.levels.push(Level::new("Level 1"));
    doc
}

/// Every record associates with the face it seeds, which is what makes the
/// forty records a measurement of the SPECIFIED scenario rather than forty
/// arbitrary rows of the right size.
#[test]
fn every_corpus_record_seeds_the_ring_it_belongs_to() {
    use domowik_geometry::rooms::{self, RoomStatus};
    let doc = unfurnished_1000_v1();
    let mut associated = 0usize;
    for level in &doc.levels {
        let projected = rooms::project_level(&doc, level.id);
        assert_eq!(projected.faces.len(), 10, "ten rings per level");
        for record in &projected.records {
            assert_eq!(
                record.status,
                RoomStatus::Associated,
                "a ring centre is strictly inside its own ring, and no two \
                 records share a face"
            );
            associated += 1;
        }
    }
    assert_eq!(associated, 40);
}

/// `unfurnished-1000-v1`, to the §10.9 construction.
///
/// Rings r in 0..10 at origin `((r mod 5)*15000, (r div 5)*12000)`, each
/// 12000 x 9000 mm. Bottom and top edges split at 4000 and 8000; sides at 3000
/// and 6000. Three sub-walls per edge, twelve per ring, 120 per level, 480
/// across four levels. A join at every interior split point: eight per ring,
/// 320 in total. Doors on the middle sub-wall of each edge for the first 39 of
/// 40 ring instances, in level-major then ring-major order: 156. One
/// room-metadata record per ring instance, seeded at the ring centre
/// `(15000·(r mod 5) + 6000, 12000·(r div 5) + 4500)` with the optional name
/// and both style references omitted: 40.
fn unfurnished_1000_v1() -> Document {
    let mut doc = Document::new();
    let mut instance = 0usize;

    for level_index in 1..=4u32 {
        let mut level = Level::new(format!("Level {level_index}"));
        level.elevation_mm = 3000.0 * f64::from(level_index - 1);
        let level_id = level.id;
        doc.levels.push(level);

        for ring in 0..10usize {
            let ox = (ring % 5) as f64 * PITCH_X;
            let oy = (ring / 5) as f64 * PITCH_Y;

            let along = |a: Point, b: Point, cuts: [f64; 2]| -> [(Point, Point); 3] {
                let span = (b.x - a.x).hypot(b.y - a.y);
                let at = |d: f64| {
                    let t = d / span;
                    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
                };
                [
                    (a, at(cuts[0])),
                    (at(cuts[0]), at(cuts[1])),
                    (at(cuts[1]), b),
                ]
            };

            let bl = Point::new(ox, oy);
            let br = Point::new(ox + RING_W, oy);
            let tr = Point::new(ox + RING_W, oy + RING_H);
            let tl = Point::new(ox, oy + RING_H);

            // Bottom, right, top, left — the order the openings rule names.
            let edges = [
                along(bl, br, [4000.0, 8000.0]),
                along(br, tr, [3000.0, 6000.0]),
                along(tr, tl, [4000.0, 8000.0]),
                along(tl, bl, [3000.0, 6000.0]),
            ];

            let mut middles = Vec::with_capacity(4);
            for edge in edges {
                let mut ids = Vec::with_capacity(3);
                for (a, b) in edge {
                    let wall = Wall::new(level_id, a, b, 200.0, 2500.0);
                    ids.push(wall.id);
                    doc.walls.push(wall);
                }
                middles.push(ids[1]);
                doc.joins.push(WallJoin::new(vec![
                    JoinMember::new(ids[0], Role::End),
                    JoinMember::new(ids[1], Role::Start),
                ]));
                doc.joins.push(WallJoin::new(vec![
                    JoinMember::new(ids[1], Role::End),
                    JoinMember::new(ids[2], Role::Start),
                ]));
            }

            // §10.9: one record per ring instance, seeded at the ring
            // centre, `{levelId, seedMm}` only — the optional display name and
            // both style references stay absent. The centre is strictly inside
            // the ring's face (the 200 mm bands leave x ∈ (ox+100, ox+11900)
            // and y ∈ (oy+100, oy+8900) free), so every record associates.
            doc.room_records.push(RoomRecord::new(
                Uuid::now_v7(),
                level_id,
                Point::new(ox + RING_W / 2.0, oy + RING_H / 2.0),
            ));

            // Instances 0..=38 receive four doors; instance 39 receives none.
            if instance < 39 {
                for middle in middles {
                    doc.openings
                        .push(Opening::door(middle, 0.5, 900.0, 2100.0, 200.0));
                }
            }
            instance += 1;
        }
    }
    doc
}

fn measure(name: &str, doc: Document) -> (usize, usize) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(format!("{name}.domo"));
    write_package(&path, &Package::new(doc)).expect("the scenario must be writable");
    let bytes = std::fs::metadata(&path).expect("written").len() as usize;
    let pkg = read_package(&path).expect("reopens");
    let d = &pkg.snapshot;
    let entities = d.entity_count();
    println!(
        "  {name}: {bytes} bytes ({:.1} KiB), {entities} entities",
        bytes as f64 / 1024.0
    );
    (bytes, entities)
}

#[test]
fn empty_v1_is_built_to_its_specification_and_fits_its_budget() {
    let doc = empty_v1();
    assert_eq!(doc.levels.len(), 1);
    assert_eq!(doc.levels[0].name, "Level 1");
    assert_eq!(doc.levels[0].elevation_mm, 0.0);
    assert_eq!(doc.levels[0].floor_thickness_mm, 200.0);
    assert_eq!(doc.levels[0].default_wall_height_mm, 2500.0);
    assert_eq!(doc.levels[0].default_ceiling_height_mm, 2500.0);
    assert!(doc.walls.is_empty() && doc.openings.is_empty() && doc.joins.is_empty());

    let (bytes, entities) = measure("empty-v1", doc);
    assert_eq!(entities, 1);
    assert!(
        bytes < 16 * 1024,
        "§10.9 target is < 16 KiB; measured {bytes} bytes"
    );
}

#[test]
fn unfurnished_1000_v1_is_built_to_its_specification() {
    let doc = unfurnished_1000_v1();
    assert_eq!(doc.levels.len(), 4, "4 levels");
    assert_eq!(doc.walls.len(), 480, "480 walls");
    assert_eq!(doc.joins.len(), 320, "320 joins");
    assert_eq!(doc.openings.len(), 156, "156 openings");
    assert_eq!(doc.room_records.len(), 40, "40 room-metadata records");
    assert_eq!(
        doc.entity_count(),
        1000,
        "the scenario is 1,000 entities; it measured 960 until §5.2 existed"
    );
    assert_eq!(doc.validate(), vec![], "the scenario must be a valid plan");
}

#[test]
fn unfurnished_1000_v1_fits_its_budget() {
    let (bytes, entities) = measure("unfurnished-1000-v1", unfurnished_1000_v1());
    assert_eq!(entities, 1000);
    assert!(
        bytes < 256 * 1024,
        "§10.9 target is < 256 KiB; measured {bytes} bytes"
    );
}

#[test]
fn the_measured_compression_factor_is_recorded() {
    // The §10.9 budget table assumes 150-250 B/entity uncompressed and a
    // 2.0-2.5x compression factor. The measurement comes in far under the
    // estimate, so the ASSUMPTIONS are what need reporting, not just the
    // total — a number that beats its budget for a reason nobody wrote down
    // is not evidence, it is luck.
    let doc = unfurnished_1000_v1();
    let entities = doc.entity_count();
    let uncompressed = serde_json::to_vec(&doc).expect("serialises").len();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("u.domo");
    write_package(&path, &Package::new(doc)).expect("write");
    let packed = std::fs::metadata(&path).expect("written").len() as usize;

    println!("\n  unfurnished-1000-v1: {entities} entities");
    println!(
        "    uncompressed document.json: {uncompressed} B ({:.0} B/entity)",
        uncompressed as f64 / entities as f64
    );
    println!(
        "    packaged: {packed} B ({:.1} KiB, {:.0} B/entity)",
        packed as f64 / 1024.0,
        packed as f64 / entities as f64
    );
    println!(
        "    compression factor: {:.1}x",
        uncompressed as f64 / packed as f64
    );

    // The assumed per-entity range is what the estimate was built on; the
    // measurement lands inside it, so the estimate's error is the compression
    // factor, not the payload size.
    let per_entity = uncompressed as f64 / entities as f64;
    assert!(
        (150.0..=250.0).contains(&per_entity),
        "uncompressed payload is {per_entity:.0} B/entity, outside the \
         assumed 150-250 B; the budget's OTHER assumption would then also \
         need re-deriving"
    );
    assert!(
        uncompressed as f64 / packed as f64 > 2.5,
        "the estimate assumed at most 2.5x; if compression no longer beats \
         that, the measured total will move toward the estimate"
    );
}

#[test]
fn the_corpus_scenarios_reopen_to_what_was_written() {
    // A size measurement of a package that cannot be reopened measures
    // nothing.
    for (name, doc) in [
        ("empty-v1", empty_v1()),
        ("unfurnished-1000-v1", unfurnished_1000_v1()),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("{name}.domo"));
        let expected = doc.clone();
        write_package(&path, &Package::new(doc)).expect("write");
        let back = read_package(&path).expect("read");
        assert_eq!(back.snapshot, expected, "{name} must round-trip");
    }
}

#[test]
fn the_scenario_generator_is_deterministic_in_shape() {
    // Entity ids are UUIDv7 and differ between runs, so the SHAPE is what must
    // be stable — otherwise the size measurement is not repeatable.
    let a = unfurnished_1000_v1();
    let b = unfurnished_1000_v1();
    let shape = |d: &Document| -> Vec<[f64; 4]> {
        let mut v: Vec<[f64; 4]> = d
            .walls
            .iter()
            .map(|w| [w.start_mm[0], w.start_mm[1], w.end_mm[0], w.end_mm[1]])
            .collect();
        v.sort_by(|x, y| x.partial_cmp(y).expect("finite"));
        v
    };
    assert_eq!(shape(&a), shape(&b));
}
