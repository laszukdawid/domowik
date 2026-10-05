//! Spatial-index and hit-testing benchmarks (issue 0012).
//!
//! # Dataset distribution
//!
//! A `grid(n)` plan: an n x n arrangement of 4 m x 3 m rooms sharing walls,
//! which is the shape a real floor plan has — walls short relative to the
//! plan, spread evenly, meeting at corners. `grid(4)` is 40 walls (a large
//! flat), `grid(10)` is 220 walls (a small office floor), `grid(20)` is 840
//! walls (past anything a v0.1 user is expected to draw).
//!
//! # Query mix, measured SEPARATELY as issue 0012 requires
//!
//! | Benchmark | What it is |
//! |---|---|
//! | `build` | A committed update: the index is rebuilt per edit |
//! | `cull` | Visible-region culling: a viewport-sized rectangle query |
//! | `point-hit/one-shot` | A click: build the index, query, discard |
//! | `point-hit/indexed` | A drag: query an index a committed update built |
//! | `preview` | A transient drag query: proximity at a wider radius |
//!
//! # Budgets
//!
//! These are REGRESSION ALARMS, not tuning targets, and they are generous on
//! purpose. An interactive frame is 16 ms; a query that costs 100 µs can run
//! a hundred times a frame and still leave 40% of the budget.
//!
//! | Path | Budget | Why |
//! |---|---|---|
//! | `cull`, `point-hit`, `preview` | < 100 µs at `grid(10)` | 160 queries per frame |
//! | `build` | < 1 ms at `grid(10)` | Once per committed edit, not per frame |
//!
//! Timings are host-dependent and are NOT a pass/fail gate (ED-0070);
//! `scripts/verify.sh` compiles this on every run so the harness cannot rot.
//! Allocation counts are not measured here — Rust has no allocation counter in
//! the standard library, and adding a counting allocator to the workspace to
//! satisfy a criterion written against Go's `testing.B` would be the wrong
//! trade. Recorded as a known gap rather than quietly dropped.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::{Document, Point, Wall};
use domowik_geometry::hit::{hit_test, marquee, HitIndex};
use domowik_geometry::spatial::{Aabb, QueryScratch, SpatialIndex};
use domowik_geometry::walls::WallSolid;
use uuid::Uuid;

const ROOM_W: f64 = 4000.0;
const ROOM_H: f64 = 3000.0;
const THICKNESS: f64 = 200.0;

/// An n x n grid of rooms sharing walls.
fn grid(n: usize) -> (Document, Uuid) {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    for i in 0..=n {
        let y = i as f64 * ROOM_H;
        for j in 0..n {
            let x = j as f64 * ROOM_W;
            doc.walls.push(Wall::new(
                level,
                Point::new(x, y),
                Point::new(x + ROOM_W, y),
                THICKNESS,
                2500.0,
            ));
        }
    }
    for j in 0..=n {
        let x = j as f64 * ROOM_W;
        for i in 0..n {
            let y = i as f64 * ROOM_H;
            doc.walls.push(Wall::new(
                level,
                Point::new(x, y),
                Point::new(x, y + ROOM_H),
                THICKNESS,
                2500.0,
            ));
        }
    }
    (doc, level)
}

fn wall_boxes(doc: &Document, level: Uuid) -> Vec<(Aabb, Uuid)> {
    doc.walls_on_level(level)
        .filter_map(|w| {
            let poly = WallSolid::new(w.start(), w.end(), w.thickness_mm).rectangle()?;
            Some((Aabb::of(&poly)?, w.id))
        })
        .collect()
}

/// A committed update: the index is rebuilt, because derived geometry is
/// rebuilt on every edit anyway (R3.0.7).
fn build(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/build");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let items = wall_boxes(&doc, level);
        group.bench_function(format!("grid-{n}x{n}-{}-walls", items.len()), |b| {
            b.iter(|| SpatialIndex::build(black_box(items.clone())))
        });
    }
    group.finish();
}

/// Visible-region culling: one viewport-sized rectangle.
fn cull(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/cull");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let index = SpatialIndex::build(wall_boxes(&doc, level));
        // A viewport showing roughly four rooms.
        let view = Aabb::from_corners(
            Point::new(ROOM_W, ROOM_H),
            Point::new(ROOM_W * 3.0, ROOM_H * 3.0),
        );
        group.bench_function(format!("grid-{n}x{n}"), |b| {
            b.iter(|| index.query(black_box(&view)))
        });
    }
    group.finish();
}

/// A click: hit testing at the mouse radius (12 logical px; at a typical zoom
/// of 20 mm per px that is 240 mm of plan).
fn point_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/point-hit");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        // A corner where four walls meet: the densest place to click.
        let p = Point::new(ROOM_W * 2.0, ROOM_H * 2.0);
        // A click: build and discard, which is what a one-shot query costs.
        group.bench_function(format!("one-shot/grid-{n}x{n}"), |b| {
            b.iter(|| hit_test(black_box(&doc), level, black_box(p), 240.0))
        });
        // A drag: the index was built by the last committed update.
        let index = HitIndex::build(&doc, level);
        group.bench_function(format!("indexed/grid-{n}x{n}"), |b| {
            b.iter(|| index.hit_test(black_box(p), 240.0))
        });
    }
    group.finish();
}

/// Deliberately overlapping regions: every result has distance zero and the
/// same closest point, so the canonical polygon key resolves the ordering.
/// This is a stress distribution, not a representative residential plan.
fn tied_regions(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/tied-regions");
    for count in [32, 220] {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        for i in 0..count {
            let half_length = 1000.0 + i as f64;
            let mut wall = Wall::new(
                level,
                Point::new(-half_length, 0.0),
                Point::new(half_length, 0.0),
                THICKNESS,
                2500.0,
            );
            // IDs oppose the polygon ordering, and remain reproducible.
            wall.id = Uuid::from_u128(i as u128 + 1);
            doc.walls.push(wall);
        }
        let point = Point::new(0.0, 0.0);
        group.bench_function(format!("build/{count}"), |b| {
            b.iter(|| HitIndex::build(black_box(&doc), level))
        });
        group.bench_function(format!("one-shot/{count}"), |b| {
            b.iter(|| hit_test(black_box(&doc), level, black_box(point), 12.0))
        });
        let index = HitIndex::build(&doc, level);
        group.bench_function(format!("indexed/{count}"), |b| {
            b.iter(|| index.hit_test(black_box(point), 12.0))
        });
    }
    group.finish();
}

/// Endpoint-heavy: many FREE endpoints inside one radius.
///
/// The region-dominated distributions above barely exercise the R6.3.2
/// feature dedupe, because one wall contributes exactly one region and the
/// full-key comparison then decides on the first level it looks at. This is
/// the distribution where the dedupe costs something: a fan of unjoined walls
/// whose inner endpoints all sit inside the radius, so every wall contributes
/// BOTH an endpoint and a region and the endpoint candidates are numerous and
/// close together. A stress distribution, not a residential plan.
fn dense_endpoints(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/dense-endpoints");
    for count in [32, 220] {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        for i in 0..count {
            let angle = i as f64 / count as f64 * std::f64::consts::TAU;
            let (sin, cos) = angle.sin_cos();
            let mut wall = Wall::new(
                level,
                Point::new(cos * 100.0, sin * 100.0),
                Point::new(cos * 5000.0, sin * 5000.0),
                THICKNESS,
                2500.0,
            );
            // IDs oppose the geometric ordering, and remain reproducible.
            wall.id = Uuid::from_u128((count - i) as u128);
            doc.walls.push(wall);
        }
        // 24 logical px at the default 0.1 px/mm zoom: the touch radius.
        let point = Point::new(0.0, 0.0);
        group.bench_function(format!("build/{count}"), |b| {
            b.iter(|| HitIndex::build(black_box(&doc), level))
        });
        group.bench_function(format!("one-shot/{count}"), |b| {
            b.iter(|| hit_test(black_box(&doc), level, black_box(point), 240.0))
        });
        let index = HitIndex::build(&doc, level);
        group.bench_function(format!("indexed/{count}"), |b| {
            b.iter(|| index.hit_test(black_box(point), 240.0))
        });
    }
    group.finish();
}

/// A transient drag-preview query: proximity at a wider radius, which is what
/// a snap evaluation asks for on every pointer move.
fn preview(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/preview");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let index = SpatialIndex::build(wall_boxes(&doc, level));
        let p = Point::new(ROOM_W * 2.5, ROOM_H * 2.5);
        group.bench_function(format!("grid-{n}x{n}"), |b| {
            b.iter(|| index.near(black_box(p), 1000.0))
        });
    }
    group.finish();
}

/// Marquee selection over a large rectangle: the worst realistic case, since
/// every candidate has to be tested against the box rather than culled.
///
/// Four measurements per size, in two pairs. Every variant runs the SAME
/// per-candidate clipping, so no pair isolates the clip's cost against an
/// unclipped implementation — establishing that needs a run of this benchmark
/// from before the change. What the pairs do measure is sensitivity to query
/// MAGNITUDE at equal work:
///
/// - `grid-NxN` / `grid-NxN-huge`: an enclosing query, ordinary corners
///   versus `±1e308`. Both short-circuit on vertex containment, so neither
///   exercises the repaired edge-crossing path.
/// - `grid-NxN-band` / `grid-NxN-band-huge`: a one-millimetre horizontal band
///   at mid-room height. No wall vertex lies inside it and no band corner
///   lies inside a wall, so every candidate is decided by the crossing test
///   itself — the path the clip repairs.
///
/// Each pair's answers are asserted equal, as ordered id lists, OUTSIDE the
/// timed section: two variants that select different walls would be measuring
/// two different amounts of work.
fn marquee_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/marquee");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let enclosing = Aabb::from_corners(
            Point::new(-1000.0, -1000.0),
            Point::new(ROOM_W * n as f64, ROOM_H * n as f64),
        );
        let enclosing_huge =
            Aabb::from_corners(Point::new(-1e308, -1e308), Point::new(1e308, 1e308));
        // Mid-room height: clear of every horizontal wall band, and crossing
        // the vertical walls of the bottom row between their end vertices.
        let band = Aabb::from_corners(Point::new(-1.1e7, 1500.0), Point::new(1.1e7, 1501.0));
        let band_huge = Aabb::from_corners(Point::new(-1e308, 1500.0), Point::new(1e308, 1501.0));

        for (name, bounded, huge) in [("", enclosing, enclosing_huge), ("-band", band, band_huge)] {
            let expected = marquee(&doc, level, bounded);
            assert!(!expected.is_empty(), "grid-{n}{name} must select something");
            assert_eq!(
                marquee(&doc, level, huge),
                expected,
                "grid-{n}{name}: the huge query must select the same walls, \
                 in the same order, as the bounded one"
            );
            // The selected count is in the NAME, as in `build` above: a
            // marquee's cost is driven by how many candidates survive, so a
            // timing without it cannot be compared against anything later.
            let hits = expected.len();
            let walls = doc.walls_on_level(level).count();
            group.bench_function(format!("grid-{n}x{n}{name}-{hits}-of-{walls}"), |b| {
                b.iter(|| marquee(black_box(&doc), level, black_box(bounded)))
            });
            group.bench_function(format!("grid-{n}x{n}{name}-{hits}-of-{walls}-huge"), |b| {
                b.iter(|| marquee(black_box(&doc), level, black_box(huge)))
            });
        }
    }
    group.finish();
}

/// Bulk local queries over the same axis-aligned boxes, comparing the
/// ordinary query's per-call candidate storage with a caller-owned scratch
/// buffer. Fixtures and expected answers are prepared before timing.
fn bulk_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/bulk-query");
    for count in [100usize, 500, 2000] {
        let entries: Vec<_> = (0..count)
            .map(|i| {
                let x = (i % 50) as f64 * 1000.0;
                let y = (i / 50) as f64 * 1000.0;
                (
                    Aabb::from_corners(Point::new(x, y), Point::new(x + 200.0, y + 200.0)),
                    i,
                )
            })
            .collect();
        let index = SpatialIndex::build(entries);
        let queries: Vec<_> = (0..count)
            .map(|i| {
                let x = (i % 50) as f64 * 1000.0;
                let y = (i / 50) as f64 * 1000.0;
                Aabb::from_corners(Point::new(x, y), Point::new(x + 200.0, y + 200.0))
            })
            .collect();
        let expected: Vec<Vec<usize>> = queries.iter().map(|q| index.query(q)).collect();
        assert_eq!(expected.iter().map(Vec::len).sum::<usize>(), count);
        let scratch_results: Vec<Vec<usize>> = {
            let mut scratch = QueryScratch::default();
            queries
                .iter()
                .map(|q| index.query_with_scratch(q, &mut scratch))
                .collect()
        };
        assert_eq!(scratch_results, expected);

        group.bench_function(format!("ordinary/{count}"), |bench| {
            bench.iter(|| {
                let total: usize = queries
                    .iter()
                    .map(|q| index.query(black_box(q)).len())
                    .sum();
                black_box(total)
            })
        });
        let mut scratch = QueryScratch::default();
        group.bench_function(format!("reused-scratch/{count}"), |bench| {
            bench.iter(|| {
                let total: usize = queries
                    .iter()
                    .map(|q| index.query_with_scratch(black_box(q), &mut scratch).len())
                    .sum();
                black_box(total)
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    build,
    cull,
    point_hit,
    tied_regions,
    dense_endpoints,
    preview,
    marquee_bench,
    bulk_query
);
criterion_main!(benches);
