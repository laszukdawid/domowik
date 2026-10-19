//! Criterion benchmarks for the shared edit BUILDER (issue 0055).
//!
//! `interaction` times a gesture's commit and derived rebuild, with its batch
//! built beforehand. This times the step before that: turning a gesture into
//! a batch, which is where R3.1.2 contact classification, crossing
//! construction, splitting and joining happen. The builder reads `&Document`
//! and writes nothing, so no setup is excluded and nothing is cloned per
//! iteration except what the builder itself clones.
//!
//! Gestures are placed relative to the corpus's top edge, so every corpus
//! with a horizontal opening-free wall supplies them. One with no such wall
//! is skipped rather than given a substitute target.
//!
//! `scripts/verify.sh` compiles this on every run. Timings are host-dependent
//! and are not a pass/fail gate (ED-0070).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_bench::corpus;
use domowik_cli::{add_wall_operations, apply_preview, set_wall_operations};
use domowik_format::{Document, Point, Wall};
use uuid::Uuid;

/// The horizontal opening-free wall with the greatest midpoint `y`, as in the
/// `interaction` gestures: its far side is empty space.
fn top_edge(doc: &Document, level: Uuid) -> Option<&Wall> {
    doc.walls_on_level(level)
        .filter(|w| w.start().y == w.end().y && w.start().x != w.end().x)
        .filter(|w| !doc.openings.iter().any(|o| o.host_wall_id == w.id))
        .max_by(|a, b| {
            (a.start().y, a.id)
                .partial_cmp(&(b.start().y, b.id))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn wall(level: Uuid, a: Point, b: Point) -> Wall {
    Wall::new(level, a, b, 120.0, 2500.0)
}

fn builder(c: &mut Criterion) {
    for corpus in corpus::all() {
        let level = corpus.primary_level();
        let doc = &corpus.doc;
        let Some(host) = top_edge(doc, level) else {
            continue;
        };
        let host_id = host.id;
        let (s, e) = (host.start(), host.end());
        // A quarter along the host, so a mid-span split lands away from any
        // node and the crossing gestures cross the host alone.
        let m = Point::new(s.x + (e.x - s.x) / 4.0, s.y);
        let far = Point::new(m.x + 1.0e5, m.y + 1.0e5);

        let gestures: Vec<(&str, Wall)> = vec![
            (
                "add-free",
                wall(level, far, Point::new(far.x + 3000.0, far.y)),
            ),
            ("add-joined", wall(level, e, Point::new(e.x, e.y + 3000.0))),
            ("add-split", wall(level, m, Point::new(m.x, m.y + 3000.0))),
            (
                "add-cross-axis",
                wall(
                    level,
                    Point::new(m.x + 137.0, m.y - 400.0),
                    Point::new(m.x + 137.0, m.y + 1500.0),
                ),
            ),
            // The exact crossing is not a binary64 value: this is the row the
            // exact construction pays for.
            (
                "add-cross-diagonal",
                wall(
                    level,
                    Point::new(m.x - 913.0, m.y - 411.0),
                    Point::new(m.x + 1307.0, m.y + 1209.0),
                ),
            ),
        ];

        let mut group = c.benchmark_group(format!("builder/{}", corpus.name));
        for (name, w) in gestures {
            assert!(
                add_wall_operations(doc, level, w.clone()).is_ok(),
                "{}: {name} must be an accepted gesture",
                corpus.name
            );
            group.bench_function(name, |b| {
                b.iter(|| add_wall_operations(black_box(doc), level, black_box(w.clone())))
            });
        }
        let drag_to = Point::new(e.x + 250.0, e.y + 250.0);
        group.bench_function("set-endpoint", |b| {
            b.iter(|| set_wall_operations(black_box(doc), host_id, None, Some(drag_to), None, None))
        });
        group.finish();
    }
}

/// A plan of 140 random long walls drawn through the builder: every wall
/// crosses many others, so it holds about 4,300 split walls and 2,100 joins.
/// Far denser in crossings than any house; it measures how one gesture's cost
/// grows with the plan when the gesture splits dozens of walls at once.
///
/// Before issue 0055 joined each touched node by rescanning every wall and
/// join, a gesture here cost about 0.6 s; the join stage is now one pass.
fn dense_crossings(c: &mut Criterion) {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for _ in 0..140 {
        let mut coord = || {
            let base = (next(9) * 1000) as f64;
            if next(3) == 0 {
                base + (next(1000) as f64) / 7.0
            } else {
                base
            }
        };
        let (a, b) = (Point::new(coord(), coord()), Point::new(coord(), coord()));
        if a == b {
            continue;
        }
        if let Ok(edit) = add_wall_operations(&doc, level, wall(level, a, b)) {
            for op in &edit.operations {
                apply_preview(&mut doc, op);
            }
        }
    }
    // A diagonal across the whole plan, crossing walls at non-binary64 points.
    let across = wall(level, Point::new(-500.3, 133.7), Point::new(8700.9, 7911.1));
    let edit = add_wall_operations(&doc, level, across.clone()).expect("an accepted gesture");
    assert!(edit.splits > 20, "the gesture must split many walls");
    let mut group = c.benchmark_group("builder/dense-crossings");
    group.sample_size(20);
    group.bench_function("add-across", |b| {
        b.iter(|| add_wall_operations(black_box(&doc), level, black_box(across.clone())))
    });
    group.finish();
}

criterion_group!(benches, builder, dense_crossings);
criterion_main!(benches);
