//! Occupied-region and room-detection benchmarks.
//!
//! # Why these exist
//!
//! Every edit to a plan recomputes the occupied region of the affected
//! component, and the region is what both the drawing and the floor area are
//! derived from (BEHAVIOR-SPEC R3.2.1, R5.1.1). If this is slow, the editor is
//! slow at the one thing it does. AGENTS.md requires new geometry to land WITH
//! its benchmark, so the cost is known before it becomes a complaint.
//!
//! # What the shapes represent
//!
//! | Benchmark | Shape | Why |
//! |---|---|---|
//! | `single-room` | 4 walls, 4 joins | The smallest complete plan |
//! | `apartment` | 5 rooms sharing walls | A realistic small flat |
//! | `grid-4x4` | 10 crossing walls, 16 rooms | Stress: the arrangement dominates |
//!
//! `occupied-region` and `plan` are both measured because they share almost
//! all their work: rooms are traced from the same boundary the region is, so
//! the difference between the two is the true cost of room detection.
//!
//! `scripts/verify.sh` compiles this on every run so the harness cannot rot.
//! Timings are host-dependent and are not a pass/fail gate (ED-0070).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::Point;
use domowik_geometry::boolean::{union, union_with_sources, SourceRing};
use domowik_geometry::walls::{occupied_region, plan, Endpoint, JoinMember, JoinSpec, WallSolid};

const THICKNESS: f64 = 200.0;

/// Builds walls along a closed polyline and joins each consecutive pair.
fn loop_of(points: &[(f64, f64)], first_index: usize) -> (Vec<WallSolid>, Vec<JoinSpec>) {
    let n = points.len();
    let walls: Vec<WallSolid> = (0..n)
        .map(|i| {
            let (x0, y0) = points[i];
            let (x1, y1) = points[(i + 1) % n];
            WallSolid::new(Point::new(x0, y0), Point::new(x1, y1), THICKNESS)
        })
        .collect();
    let joins: Vec<JoinSpec> = (0..n)
        .map(|i| JoinSpec {
            members: vec![
                JoinMember {
                    wall: first_index + i,
                    endpoint: Endpoint::End,
                },
                JoinMember {
                    wall: first_index + (i + 1) % n,
                    endpoint: Endpoint::Start,
                },
            ],
        })
        .collect();
    (walls, joins)
}

fn single_room() -> (Vec<WallSolid>, Vec<JoinSpec>) {
    loop_of(
        &[(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)],
        0,
    )
}

/// A 5-room flat: an outer shell with four interior partitions. The partitions
/// carry no joins, which is the common state mid-edit — the union still has to
/// close the rooms.
fn apartment() -> (Vec<WallSolid>, Vec<JoinSpec>) {
    let (mut walls, joins) = loop_of(
        &[(0.0, 0.0), (9000.0, 0.0), (9000.0, 7000.0), (0.0, 7000.0)],
        0,
    );
    let partitions = [
        ((4000.0, 0.0), (4000.0, 7000.0)),
        ((0.0, 3000.0), (4000.0, 3000.0)),
        ((4000.0, 4000.0), (9000.0, 4000.0)),
        ((6500.0, 4000.0), (6500.0, 7000.0)),
    ];
    for (a, b) in partitions {
        walls.push(WallSolid::new(
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            THICKNESS,
        ));
    }
    (walls, joins)
}

/// An 11x11 grid of wall lines: 100 rooms, 22 walls, and an arrangement with
/// 121 interior crossings. This is deliberately past any real floor plan.
fn grid(n: usize) -> Vec<WallSolid> {
    let step = 3000.0;
    let extent = step * n as f64;
    let mut walls = Vec::with_capacity(2 * (n + 1));
    for i in 0..=n {
        let v = step * i as f64;
        walls.push(WallSolid::new(
            Point::new(0.0, v),
            Point::new(extent, v),
            THICKNESS,
        ));
        walls.push(WallSolid::new(
            Point::new(v, 0.0),
            Point::new(v, extent),
            THICKNESS,
        ));
    }
    walls
}

/// A 10x10 grid: 22 walls, well past the index threshold, and the shape the
/// quadratic broad phase used to dominate.
fn grid_large() -> Vec<WallSolid> {
    grid(10)
}

fn occupied(c: &mut Criterion) {
    let mut group = c.benchmark_group("occupied-region");
    let (w, j) = single_room();
    group.bench_function("single-room", |b| {
        b.iter(|| occupied_region(black_box(&w), black_box(&j)))
    });
    let (w, j) = apartment();
    group.bench_function("apartment", |b| {
        b.iter(|| occupied_region(black_box(&w), black_box(&j)))
    });
    let w = grid(4);
    group.bench_function("grid-4x4", |b| {
        b.iter(|| occupied_region(black_box(&w), black_box(&[])))
    });
    let w = grid_large();
    group.bench_function("grid-10x10", |b| {
        b.iter(|| occupied_region(black_box(&w), black_box(&[])))
    });
    group.finish();
}

/// The full derived pass: region AND rooms, which is what every edit
/// recomputes and what `info` and `export-svg` each call once.
fn full_plan(c: &mut Criterion) {
    let mut group = c.benchmark_group("plan");
    let (w, j) = single_room();
    group.bench_function("single-room", |b| {
        b.iter(|| plan(black_box(&w), black_box(&j)))
    });
    let (w, j) = apartment();
    group.bench_function("apartment", |b| {
        b.iter(|| plan(black_box(&w), black_box(&j)))
    });
    let w = grid(4);
    group.bench_function("grid-4x4", |b| {
        b.iter(|| plan(black_box(&w), black_box(&[])))
    });
    group.finish();
}

/// End-to-end public API comparison over the same prebuilt wall rings; fixture
/// construction and geometry-equivalence checking stay outside timed loops.
/// This does not isolate metadata cost: ordinary union also computes complement
/// rings internally, while the tagged path computes only the occupied boundary.
fn sourced_union(c: &mut Criterion) {
    let walls = grid_large();
    let rings: Vec<Vec<Point>> = walls
        .iter()
        .filter_map(WallSolid::rectangle)
        .map(|r| r.to_vec())
        .collect();
    let sourced: Vec<SourceRing> = rings
        .iter()
        .enumerate()
        .map(|(source, points)| SourceRing {
            points: points.clone(),
            source,
        })
        .collect();
    let ordinary = union(&rings);
    let tagged = union_with_sources(&sourced);
    assert!(!ordinary.is_empty());
    assert_eq!(
        tagged.iter().map(|p| p.polygon.clone()).collect::<Vec<_>>(),
        ordinary
    );
    let mut group = c.benchmark_group("occupied-region-sources");
    group.bench_function("ordinary-grid-10x10", |b| {
        b.iter(|| union(black_box(&rings)))
    });
    group.bench_function("tagged-grid-10x10", |b| {
        b.iter(|| union_with_sources(black_box(&sourced)))
    });
    group.finish();
}

fn wall_colors(c: &mut Criterion) {
    let level = domowik_format::Level::new("Colors");
    let mut doc = domowik_format::Document::default();
    for i in 0..100 {
        let y = f64::from(i) * 300.;
        let mut w = domowik_format::Wall::new(
            level.id,
            Point::new(0., y),
            Point::new(4000., y),
            120.,
            2500.,
        );
        w.unknown.insert("color".into(), "#D8CDB7".into());
        doc.walls.push(w);
    }
    doc.levels.push(level.clone());
    c.bench_function("wall-colors/100-walls", |b| {
        b.iter(|| domowik_geometry::cuts::wall_paints(black_box(&doc), level.id))
    });
}

criterion_group!(benches, occupied, full_plan, sourced_union, wall_colors);
criterion_main!(benches);
