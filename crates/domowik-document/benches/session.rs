//! Session commit and replay cost against history length (issue 0038).
//!
//! # Why this benchmark exists
//!
//! The Go spike's finding F1 recorded that repeated `Session.Execute` slowed
//! "superlinearly with executed-command count (~quadratic in this workload)"
//! and left the cause undiagnosed. The Rust rewrite inherited it. Nothing in
//! the test suite could see it, because every test uses a handful of
//! operations and a quadratic path is indistinguishable from a linear one at
//! n = 5.
//!
//! # What is measured
//!
//! | Benchmark | What it is |
//! |---|---|
//! | `commit/<n>` | One more commit onto a session that already holds n operations — the cost a user pays per keystroke |
//! | `open/<n>` | Full replay of an n-operation log, the cost of opening a stored plan |
//!
//! `commit` is the one that matters for interactivity, and it is measured as
//! the marginal cost of ONE commit rather than the cumulative cost of n, so a
//! per-commit cost that grows with history is visible directly rather than
//! inferred from a curve.
//!
//! # Budgets
//!
//! REGRESSION ALARMS, not tuning targets. An interactive frame is 16 ms.
//!
//! | Path | Budget | Why |
//! |---|---|---|
//! | `commit` at n = 2000 | < 1 ms | A commit happens per user edit; 16 would fit in a frame |
//! | `open` at n = 2000 | < 200 ms | Opening a plan is once per document, and a user will wait |
//!
//! The point of the budgets is the SHAPE: `commit/100` and `commit/2000`
//! should be within a small factor of each other. A ratio that tracks n is
//! the defect this benchmark exists to expose.
//!
//! Timings are host-dependent and are NOT a pass/fail gate (ED-0070);
//! `scripts/verify.sh` compiles this on every run so the harness cannot rot.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use domowik_document::operation::Operation;
use domowik_document::session::Session;
use domowik_document::wall_contact::wall_contact_refusals;
use domowik_document::warnings::validation_warnings;
use domowik_format::{Document, Entity, Level, Point, Wall};
use domowik_geometry::wall_contact::{axis_contact, AxisContact};
use uuid::Uuid;

/// A session holding `n` wall-creation operations, one per batch.
///
/// One operation per batch is the interactive shape: a user draws a wall, the
/// application commits it. Batching many operations into one commit is the
/// case the Go spike measured separately and found much cheaper per operation;
/// the expensive case is the honest one to benchmark.
fn session_with(n: usize) -> (Session, Uuid) {
    let mut s = Session::new();
    let level = Level::new("Level 1");
    let level_id = level.id;
    s.commit(vec![Operation::create(Entity::Level(level))])
        .expect("level");
    for i in 0..n {
        s.commit(vec![Operation::create(Entity::Wall(nth_wall(level_id, i)))])
            .expect("wall");
    }
    (s, level_id)
}

/// Wall `i` of the synthetic plan: a short segment on its own, so no two walls
/// touch and the commit cost is not dominated by join or room derivation.
fn nth_wall(level: Uuid, i: usize) -> Wall {
    let x = (i as f64) * 10.0;
    Wall::new(
        level,
        Point::new(x, 0.0),
        Point::new(x + 5.0, 0.0),
        200.0,
        2500.0,
    )
}

fn commit(c: &mut Criterion) {
    let mut group = c.benchmark_group("commit");
    for n in [100usize, 500, 1000, 2000] {
        let (session, level) = session_with(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || session.clone(),
                |mut s| {
                    s.commit(vec![Operation::create(Entity::Wall(nth_wall(
                        level,
                        n + 1,
                    )))])
                    .expect("commit");
                    black_box(s.document().walls.len())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn open(c: &mut Criterion) {
    let mut group = c.benchmark_group("open");
    for n in [100usize, 500, 1000, 2000] {
        let (session, _) = session_with(n);
        let batches: Vec<_> = session.log().batches().to_vec();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let s = Session::open_snapshot(
                    Uuid::now_v7(),
                    Default::default(),
                    &Default::default(),
                    &[],
                    batches.clone(),
                )
                .expect("open");
                black_box(s.document().walls.len())
            });
        });
    }
    group.finish();
}

/// The admission guard in isolation, including changed-axis discovery and
/// allocation/drop of diagnostics. Fixtures contain n disjoint short axes;
/// only one wall changes. This measures O(n) transition work, not an all-pairs
/// validation report, geometry derivation, file save, FFI or a Flutter frame.
fn wall_contact_admission(c: &mut Criterion) {
    let mut group = c.benchmark_group("wall_contact_admission");
    for n in [100usize, 500, 2000] {
        let mut before = Document::with_ground_floor();
        let level = before.levels[0].id;
        before.walls = (0..n).map(|i| nth_wall(level, i)).collect();
        for kind in ["height_only", "move_clear", "overlap_refused"] {
            let mut after = before.clone();
            match kind {
                "height_only" => after.walls[0].height_mm = 3000.0,
                "move_clear" => {
                    after.walls[0].start_mm[1] = 500.0;
                    after.walls[0].end_mm[1] = 500.0;
                }
                "overlap_refused" => after.walls[0].end_mm[0] = 12.0,
                _ => unreachable!(),
            }
            assert_eq!(
                wall_contact_refusals(&before, &after).len(),
                usize::from(kind == "overlap_refused")
            );
            group.bench_function(BenchmarkId::new(kind, n), |b| {
                b.iter(|| wall_contact_refusals(black_box(&before), black_box(&after)))
            });
        }
    }
    group.finish();
}

/// Whole-document validation warning traversal, including index construction,
/// candidate queries, exact filtering and diagnostic allocation. The fixtures
/// are built before timing and contain independent wall pairs, so the expected
/// warning count is stable and does not grow accidentally through cross-pair
/// interactions.
fn validation_warning_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation-warnings/walls");
    for count in [100usize, 500, 2000] {
        let clean = warning_fixture(count, false);
        let warned = warning_fixture(count, true);
        assert!(validation_warnings(&clean).is_empty());
        assert_eq!(validation_warnings(&warned).len(), count / 2);
        group.bench_function(BenchmarkId::new("clean-spaced-axes", count), |b| {
            b.iter(|| validation_warnings(black_box(&clean)).len())
        });
        group.bench_function(BenchmarkId::new("independent-warnings", count), |b| {
            b.iter(|| validation_warnings(black_box(&warned)).len())
        });
    }
    group.finish();
}

fn warning_fixture(count: usize, warned: bool) -> Document {
    let mut doc = Document::new();
    let level = Level::new("Level 1");
    let level_id = level.id;
    doc.levels.push(level);
    for i in 0..count {
        let pair = i / 2;
        let x = pair as f64 * 10_000.0;
        let (start, end) = if warned && i % 2 == 1 {
            (Point::new(x + 850.0, 900.0), Point::new(x + 3250.0, -900.0))
        } else if !warned && i % 2 == 1 {
            (Point::new(x, 1000.0), Point::new(x + 2000.0, 1000.0))
        } else {
            (Point::new(x, 0.0), Point::new(x + 2000.0, 0.0))
        };
        let mut wall = Wall::new(level_id, start, end, 200.0, 2500.0);
        wall.id = Uuid::from_u128(i as u128 + 1);
        doc.walls.push(wall);
    }
    assert!(doc.validate().is_empty(), "benchmark fixture must validate");
    for pair in (0..count).step_by(2) {
        assert!(
            matches!(
                axis_contact(
                    doc.walls[pair].start(),
                    doc.walls[pair].end(),
                    doc.walls[pair + 1].start(),
                    doc.walls[pair + 1].end(),
                ),
                Some(AxisContact::Disjoint)
            ),
            "fixture pair {pair} must not contain a refusing axis contact"
        );
    }
    doc
}

criterion_group!(
    benches,
    commit,
    open,
    wall_contact_admission,
    validation_warning_traversal
);
criterion_main!(benches);
