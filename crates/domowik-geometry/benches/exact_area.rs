//! Exact-area and display-rounding benchmarks.
//!
//! # Why these exist
//!
//! [`domowik_geometry::exact_area`] is the R5.3.1/R5.3.2 path, and it is the
//! one place in the crate where an ordinary operation may escalate to
//! arbitrary-precision arithmetic. AGENTS.md requires new geometry to land
//! WITH its benchmark and requires a floating-point filter to be tried before
//! exact arithmetic; these measure both sides of that filter, so the cost of
//! the escalation is a number rather than an assumption.
//!
//! The pairs matter more than the absolute figures. `display/filtered` is an
//! ordinary room area, where the filter certifies the float answer and no
//! `ExactArea` is built. `display/on-a-tie` is the same shape whose area lands
//! exactly on a rounding tie, which is the case the filter must refuse — the
//! ratio between them is the price of exactness, paid only when it buys
//! something.
//!
//! Timings are host-dependent and are not a pass/fail gate (ED-0070).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::Point;
use domowik_geometry::boolean::union;
use domowik_geometry::exact_area::{display_hundredths_m2, positive_area, ring_area_mm2};
use domowik_geometry::polygon::Polygon;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
    vec![
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ]
}

/// A rectangle whose area is exactly `area_mm2`.
fn rect_of_area(area_mm2: f64) -> Polygon {
    Polygon::new(rect(0.0, 0.0, area_mm2 / 1000.0, 1000.0))
}

/// A ring with `sides` vertices on a circle, so the sum has real length.
fn polygon_with(sides: usize, radius: f64) -> Polygon {
    let points = (0..sides)
        .map(|i| {
            let t = (i as f64) / (sides as f64) * std::f64::consts::TAU;
            Point::new(radius * t.cos(), radius * t.sin())
        })
        .collect();
    Polygon::new(points)
}

fn benches(c: &mut Criterion) {
    // 3.8 x 2.8 m, the F26-scale room. Not on a tie, so the filter certifies.
    let ordinary = rect_of_area(10_640_000.0);
    // 30.245 m² — an exact tie, so the filter must refuse and the exact path
    // decides. This is F26's own area.
    let on_a_tie = rect_of_area(30_245_000.0);

    c.bench_function("exact-area/display/filtered", |b| {
        b.iter(|| display_hundredths_m2(black_box(&ordinary)))
    });
    c.bench_function("exact-area/display/on-a-tie", |b| {
        b.iter(|| display_hundredths_m2(black_box(&on_a_tie)))
    });

    for sides in [4usize, 64, 512] {
        let polygon = polygon_with(sides, 5000.0);
        c.bench_function(&format!("exact-area/ring/{sides}-vertices"), |b| {
            b.iter(|| ring_area_mm2(black_box(&polygon.outer)))
        });
        c.bench_function(&format!("exact-area/float-ring/{sides}-vertices"), |b| {
            b.iter(|| black_box(&polygon).area_mm2())
        });
    }

    // R5.1.9's link test, on regions that TOUCH — the case that reaches the
    // exact path, since the float difference cannot clear the slack bound.
    let touching = [
        rect(0.0, 0.0, 1000.0, 1000.0),
        rect(1000.0, 300.0, 2000.0, 700.0),
    ];
    let touching_left: Vec<Polygon> = touching.iter().cloned().map(Polygon::new).collect();
    let touching_union = union(&touching);
    c.bench_function("exact-area/positive-area/touching", |b| {
        b.iter(|| positive_area(black_box(&touching_left), black_box(&touching_union)))
    });

    // Regions that overlap clearly, where the filter answers without escalating.
    let overlapping = [
        rect(0.0, 0.0, 1000.0, 1000.0),
        rect(500.0, 300.0, 2000.0, 700.0),
    ];
    let overlapping_left: Vec<Polygon> = overlapping.iter().cloned().map(Polygon::new).collect();
    let overlapping_union = union(&overlapping);
    c.bench_function("exact-area/positive-area/overlapping", |b| {
        b.iter(|| positive_area(black_box(&overlapping_left), black_box(&overlapping_union)))
    });
}

criterion_group!(exact_area_benches, benches);
criterion_main!(exact_area_benches);
