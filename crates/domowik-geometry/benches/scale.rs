//! Plan-scale benchmarks (issue 0096).
//!
//! # Why these exist
//!
//! The `regions` fixtures top out at 22 walls, which says nothing about how
//! `walls::plan` grows. These run 100, 1,000 and 10,000 walls in the shapes
//! that stress different terms of the cost:
//!
//! | Family | Shape | Stresses |
//! |---|---|---|
//! | `isolated-N` | N short walls, none touching | Broad phase: must not be quadratic |
//! | `tiled-N` | k×k rooms of unjoined walls overlapping at nodes | Ordinary plans, many faces |
//! | `bundle-N` | N long walls, flush, each a few µrad off parallel | Near-collinear edges within a cell |
//! | `fan-N` | N long walls all crossing in one narrow strip | Snap rounding's worst case: hot-cell incidences |
//! | `lattice-N` | N long walls at ±37°, crossing pairwise | Long diagonals (bucket walk) and non-axis crossings (interval filter) |
//!
//! Snap rounding's routing work is the number of (segment, hot cell)
//! incidences, which Hobby (1999) §2.3 warns can approach `N·(N + K)` for
//! nearly collinear input. `fan-N` is built to approach it; no real floor plan
//! does. Real houses rarely hold more than ~100 parallel walls.
//!
//! Timings are host-dependent and are not a pass/fail gate (ED-0070).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use domowik_format::Point;
use domowik_geometry::walls::{plan, WallSolid};

const THICKNESS: f64 = 200.0;

fn wall(x0: f64, y0: f64, x1: f64, y1: f64) -> WallSolid {
    WallSolid::new(Point::new(x0, y0), Point::new(x1, y1), THICKNESS)
}

/// `n` 1 m walls on a 3 m lattice, none touching.
fn isolated(n: usize) -> Vec<WallSolid> {
    let side = (n as f64).sqrt().ceil() as usize;
    (0..n)
        .map(|i| {
            let (x, y) = ((i % side) as f64 * 3000.0, (i / side) as f64 * 3000.0);
            wall(x, y, x + 1000.0, y)
        })
        .collect()
}

/// `k×k` rooms of 3 m, one wall per room side, overlapping at every node:
/// `2k(k+1)` walls.
fn tiled(k: usize) -> Vec<WallSolid> {
    let s = 3000.0;
    let mut walls = Vec::with_capacity(2 * k * (k + 1));
    for i in 0..=k {
        for j in 0..k {
            let (a, b, c) = (i as f64 * s, j as f64 * s, (j + 1) as f64 * s);
            walls.push(wall(b, a, c, a));
            walls.push(wall(a, b, a, c));
        }
    }
    walls
}

/// `n` 20 m walls stacked flush (spacing = thickness), wall `i` rotated
/// `2·i` µrad, so neighbouring faces start coincident and drift apart by
/// about one topology cell per half metre.
fn bundle(n: usize) -> Vec<WallSolid> {
    let len = 20_000.0;
    (0..n)
        .map(|i| {
            let y = i as f64 * THICKNESS;
            let slope = 2e-6 * i as f64;
            wall(0.0, y, len, y + slope * len)
        })
        .collect()
}

/// `n` 20 m walls whose centrelines all cross pairwise inside a strip a few
/// millimetres wide: wall `i` runs from height `i·δ` to `(n − i)·δ`.
fn fan(n: usize) -> Vec<WallSolid> {
    let (len, delta) = (20_000.0, 0.01);
    (0..n)
        .map(|i| wall(0.0, i as f64 * delta, len, (n - i) as f64 * delta))
        .collect()
}

/// `n` 30 m walls, half rising and half falling at a 3:4 slope, spaced 2 m
/// apart, so every rising wall crosses every falling one at a non-axis angle.
fn lattice(n: usize) -> Vec<WallSolid> {
    let (dx, dy) = (24_000.0, 18_000.0);
    (0..n)
        .map(|i| {
            let off = (i / 2) as f64 * 2000.0;
            if i % 2 == 0 {
                wall(off, 0.0, off + dx, dy)
            } else {
                wall(off, dy, off + dx, 0.0)
            }
        })
        .collect()
}

fn run(c: &mut Criterion, name: &str, cases: &[(usize, Vec<WallSolid>)]) {
    let mut group = c.benchmark_group(format!("scale/{name}"));
    group.sample_size(10);
    for (n, walls) in cases {
        group.bench_with_input(BenchmarkId::from_parameter(n), walls, |b, w| {
            b.iter(|| plan(black_box(w), &[]))
        });
    }
    group.finish();
}

fn scale(c: &mut Criterion) {
    run(
        c,
        "isolated",
        &[100, 1000, 10_000].map(|n| (n, isolated(n))),
    );
    run(
        c,
        "tiled",
        &[7, 22, 70].map(|k| (2 * k * (k + 1), tiled(k))),
    );
    run(c, "bundle", &[10, 100, 1000].map(|n| (n, bundle(n))));
    run(c, "fan", &[10, 30, 100].map(|n| (n, fan(n))));
    run(c, "lattice", &[10, 30, 100].map(|n| (n, lattice(n))));
}

criterion_group!(benches, scale);
criterion_main!(benches);
