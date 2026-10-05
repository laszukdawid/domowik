//! Snapping benchmarks (issue 0058 checkpoint).
//!
//! # Dataset
//!
//! The same `grid(n)` plan as `benches/spatial.rs`: an n x n arrangement of
//! 4 m x 3 m rooms sharing walls. `grid(4)` is 40 walls, `grid(10)` is 220
//! walls, `grid(20)` is 840 walls — the same three scales issue 0058's
//! acceptance criteria name.
//!
//! # What is measured
//!
//! | Benchmark | What it is |
//! |---|---|
//! | `build` | The committed-update cost: rebuilding `SnapIndex` after an edit |
//! | `query/corner` | A pointer move at a four-wall corner: the densest candidate set |
//! | `query/mid-span` | A pointer move over an isolated span: only edge/axis/grid candidates |
//! | `query/with-angle` | The same corner query with an active wall-chain anchor, exercising `angle` |
//!
//! Timings are host-dependent and are NOT a pass/fail gate (ED-0070);
//! `scripts/verify.sh` compiles this on every run so the harness cannot rot.
//! Each `query/*` row is repeated evaluation against ONE built index, which
//! is the real per-pointer-move cost R6.0.4/issue 0058 care about — `build`
//! is paid once per committed document edit, not once per frame.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::{Document, Point, Wall};
use domowik_geometry::snap::{SnapIndex, SnapQuery};
use domowik_geometry::spatial::Aabb;
use uuid::Uuid;

const ROOM_W: f64 = 4000.0;
const ROOM_H: f64 = 3000.0;
const THICKNESS: f64 = 200.0;

/// An n x n grid of rooms sharing walls — identical to `benches/spatial.rs`.
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

fn base_query(point: Point) -> SnapQuery<'static> {
    SnapQuery {
        point,
        radius_mm: 240.0, // 12 logical px at a typical 20 mm/px zoom (R6.0.2)
        visible_rect_mm: Aabb::from_corners(
            Point::new(-ROOM_W, -ROOM_H),
            Point::new(ROOM_W * 30.0, ROOM_H * 30.0),
        ),
        anchor: None,
        previous_direction: None,
        excluded_walls: &[],
        excluded_joins: &[],
        grid_spacing_mm: Some(200.0),
        grid_enabled: true,
        object_enabled: true,
        angle_enabled: false,
        alt_disabled: false,
    }
}

/// A committed update: the index rebuilt after every edit (R3.0.7 kind of
/// rebuild-on-every-change discipline, mirrored here for snapping).
fn build(c: &mut Criterion) {
    let mut group = c.benchmark_group("snap/build");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        group.bench_function(format!("grid-{n}x{n}"), |b| {
            b.iter(|| SnapIndex::build(black_box(&doc), level))
        });
    }
    group.finish();
}

/// Repeated queries at a four-wall corner — the densest candidate set a
/// pointer move can land on.
fn query_corner(c: &mut Criterion) {
    let mut group = c.benchmark_group("snap/query-corner");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let index = SnapIndex::build(&doc, level);
        let point = Point::new(ROOM_W * 2.0, ROOM_H * 2.0);
        let req = base_query(point);
        assert!(!index.query(&req).unwrap().is_empty());
        group.bench_function(format!("grid-{n}x{n}"), |b| {
            b.iter(|| index.query(black_box(&req)).unwrap())
        });
    }
    group.finish();
}

/// Repeated queries over an isolated span, away from any join.
fn query_mid_span(c: &mut Criterion) {
    let mut group = c.benchmark_group("snap/query-mid-span");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let index = SnapIndex::build(&doc, level);
        let point = Point::new(ROOM_W * 2.0 + ROOM_W / 2.0, ROOM_H * 2.0 + 30.0);
        let req = base_query(point);
        assert!(!index.query(&req).unwrap().is_empty());
        group.bench_function(format!("grid-{n}x{n}"), |b| {
            b.iter(|| index.query(black_box(&req)).unwrap())
        });
    }
    group.finish();
}

/// The same corner query with an active chain anchor, so `angle` candidates
/// are evaluated too (R6.2).
fn query_with_angle(c: &mut Criterion) {
    let mut group = c.benchmark_group("snap/query-with-angle");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let index = SnapIndex::build(&doc, level);
        let point = Point::new(ROOM_W * 2.0, ROOM_H * 2.0);
        let mut req = base_query(point);
        req.anchor = Some(Point::new(ROOM_W * 2.0 - 500.0, ROOM_H * 2.0));
        req.previous_direction = Some((1.0, 0.0));
        req.angle_enabled = true;
        assert!(!index.query(&req).unwrap().is_empty());
        group.bench_function(format!("grid-{n}x{n}"), |b| {
            b.iter(|| index.query(black_box(&req)).unwrap())
        });
    }
    group.finish();
}

/// The same corner query with the gesture excluding a wall it is moving, so
/// the sub-span ownership path runs (R6.1.3).
///
/// The exclusion set is one wall, not the document: the point of measuring it
/// is that a gesture pays for the candidates its own broad phase returned, not
/// for the size of the plan.
fn query_with_exclusion(c: &mut Criterion) {
    let mut group = c.benchmark_group("snap/query-with-exclusion");
    for n in [4, 10, 20] {
        let (doc, level) = grid(n);
        let index = SnapIndex::build(&doc, level);
        let point = Point::new(ROOM_W * 2.0, ROOM_H * 2.0);
        let excluded = [doc.walls_on_level(level).next().expect("grid has walls").id];
        let mut req = base_query(point);
        req.excluded_walls = &excluded;
        assert!(!index.query(&req).unwrap().is_empty());
        group.bench_function(format!("grid-{n}x{n}"), |b| {
            b.iter(|| index.query(black_box(&req)).unwrap())
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    build,
    query_corner,
    query_mid_span,
    query_with_angle,
    query_with_exclusion
);
criterion_main!(benches);
