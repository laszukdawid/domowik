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

/// [`grid`] with every wall end joined at its grid vertex, as the editor's
/// endpoint auto-join (R3.1.2) leaves a drawn plan.
fn joined_grid(n: usize) -> (Document, Uuid) {
    use domowik_format::{JoinMember, Role, WallJoin};
    use std::collections::BTreeMap;
    let (mut doc, level) = grid(n);
    let mut at: BTreeMap<(i64, i64), Vec<JoinMember>> = BTreeMap::new();
    for w in &doc.walls {
        for (p, role) in [(w.start(), Role::Start), (w.end(), Role::End)] {
            at.entry((p.x as i64, p.y as i64))
                .or_default()
                .push(JoinMember::new(w.id, role));
        }
    }
    for members in at.into_values() {
        if members.len() >= 2 {
            doc.joins.push(WallJoin::new(members));
        }
    }
    (doc, level)
}

/// A copy of `doc` with wall `i` moved 50 mm perpendicular to itself.
fn nudged(doc: &Document, i: usize) -> Document {
    let mut moved = doc.clone();
    let (s, e) = (moved.walls[i].start(), moved.walls[i].end());
    let (dx, dy) = if s.y == e.y { (0.0, 50.0) } else { (50.0, 0.0) };
    moved.walls[i].start_mm = Point::new(s.x + dx, s.y + dy).into();
    moved.walls[i].end_mm = Point::new(e.x + dx, e.y + dy).into();
    moved
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
        previous_segment: None,
        screen_radius: None,
        excluded_walls: &[],
        excluded_joins: &[],
        grid_spacing_mm: Some(200.0),
        grid_enabled: true,
        object_enabled: true,
        angle_enabled: false,
        alt_disabled: false,
    }
}

/// A from-scratch build: what every committed edit cost before issue 0012,
/// and still the cold cost of opening a level.
fn build(c: &mut Criterion) {
    let mut group = c.benchmark_group("snap/build");
    group.sample_size(10);
    for n in [4, 10, 20, 70] {
        let (doc, level) = joined_grid(n);
        group.bench_function(format!("joined-grid-{n}x{n}"), |b| {
            b.iter(|| SnapIndex::build(black_box(&doc), level))
        });
    }
    group.finish();
}

/// A committed edit under issue 0012: one wall moved 50 mm perpendicular
/// to itself and back on alternate iterations, then [`SnapIndex::sync`] —
/// identity diff, incremental geometry, ring-change edge updates. Compare
/// with `snap/build` at the same size.
///
/// Fixtures are joined grids (every wall end joined, as drawn). `interior`
/// moves a wall between two rooms; `exterior` one on the outline, which
/// changes the boundary of the single occupied component holding every room
/// as a hole — the most expensive single-wall edit. `then-query` adds the
/// first pointer query after the edit.
fn edit_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("snap/edit-sync");
    group.sample_size(20);
    for n in [4, 10, 20, 70] {
        let (doc, level) = joined_grid(n);
        // Horizontal walls come first, row by row: row n/2, column n/2 is
        // interior; row 0, column n/2 is on the outline.
        let cases = [("interior", (n / 2) * n + n / 2), ("exterior", n / 2)];
        for (case, i) in cases {
            let moved = nudged(&doc, i);
            let mut index = SnapIndex::build(&doc, level);
            let mut flip = false;
            group.bench_function(format!("{case}/grid-{n}x{n}"), |b| {
                b.iter(|| {
                    flip = !flip;
                    index.sync(black_box(if flip { &moved } else { &doc }));
                })
            });
        }
        let moved = nudged(&doc, (n / 2) * n + n / 2);
        let mut index = SnapIndex::build(&doc, level);
        let req = base_query(Point::new(ROOM_W * 2.0, ROOM_H * 2.0));
        let mut flip = false;
        group.bench_function(format!("then-query/grid-{n}x{n}"), |b| {
            b.iter(|| {
                flip = !flip;
                index.sync(black_box(if flip { &moved } else { &doc }));
                index.query(&req).unwrap()
            })
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
    edit_sync,
    query_corner,
    query_mid_span,
    query_with_angle,
    query_with_exclusion
);
criterion_main!(benches);
