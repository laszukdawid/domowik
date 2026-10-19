//! Component partition and R5.1.9 closure benchmarks.
//!
//! # Why these exist
//!
//! The closure is what decides how much of a level an edit may recompute
//! (BEHAVIOR-SPEC R3.4.8). It is therefore on the path of every edit, and a
//! closure that costs more than the rebuild it is meant to avoid would be
//! worse than not computing one. AGENTS.md requires new geometry to land WITH
//! its benchmark; this is that benchmark.
//!
//! The shapes are chosen to separate the two costs: partitioning walls into
//! components is linear-ish, while LINKING components is quadratic in their
//! number before the bounding-box rejection. `wings` grows the component
//! count deliberately for that reason.
//!
//! Timings are host-dependent and are not a pass/fail gate (ED-0070).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::Point;
use domowik_geometry::locality::{components_of, recomputation_closure, State};
use domowik_geometry::walls::{Endpoint, JoinMember, JoinSpec, WallSolid};
use uuid::Uuid;

const THICKNESS: f64 = 200.0;

/// A closed rectangle of four joined walls, appended to `walls`/`joins`.
fn push_rect(
    walls: &mut Vec<WallSolid>,
    joins: &mut Vec<JoinSpec>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let base = walls.len();
    let c = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    for i in 0..4 {
        let (x0, y0) = c[i];
        let (x1, y1) = c[(i + 1) % 4];
        walls.push(WallSolid::new(
            Point::new(x0, y0),
            Point::new(x1, y1),
            THICKNESS,
        ));
    }
    for i in 0..4 {
        joins.push(JoinSpec {
            members: vec![
                JoinMember {
                    wall: base + i,
                    endpoint: Endpoint::End,
                },
                JoinMember {
                    wall: base + (i + 1) % 4,
                    endpoint: Endpoint::Start,
                },
            ],
        });
    }
}

/// `n` separated rectangles: `n` components, all mutually unlinked.
fn wings(n: usize) -> (Vec<WallSolid>, Vec<JoinSpec>) {
    let mut walls = Vec::new();
    let mut joins = Vec::new();
    for k in 0..n {
        let col = (k % 8) as f64;
        let row = (k / 8) as f64;
        push_rect(
            &mut walls,
            &mut joins,
            col * 40_000.0,
            row * 40_000.0,
            12_000.0,
            9000.0,
        );
    }
    (walls, joins)
}

fn ids(n: usize) -> Vec<Uuid> {
    (0..n as u128).map(|i| Uuid::from_u128(i + 1)).collect()
}

fn partition(c: &mut Criterion) {
    let mut group = c.benchmark_group("components");
    for n in [1usize, 8, 32, 64] {
        let (walls, joins) = wings(n);
        group.bench_function(format!("wings-{n}"), |b| {
            b.iter(|| components_of(black_box(&walls), black_box(&joins)))
        });
    }
    group.finish();
}

fn closure(c: &mut Criterion) {
    let mut group = c.benchmark_group("closure");
    for n in [1usize, 8, 32, 64] {
        let (walls, joins) = wings(n);
        let id = ids(walls.len());
        let state = State {
            walls: &walls,
            ids: &id,
            joins: &joins,
        };
        let touched = [id[0]];
        group.bench_function(format!("wings-{n}"), |b| {
            b.iter(|| {
                recomputation_closure(black_box(&state), black_box(&state), black_box(&touched))
            })
        });
    }
    group.finish();
}

criterion_group!(benches, partition, closure);
criterion_main!(benches);
