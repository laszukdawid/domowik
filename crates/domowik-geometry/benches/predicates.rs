//! Predicate benchmarks.
//!
//! The rewrite was justified on measured numbers, so the numbers keep being
//! measured. `scripts/verify.sh` compiles this harness on every run so the
//! measurement path cannot rot; the timings are not a pass/fail gate because
//! they are host-dependent (ED-0070).
//!
//! # Budgets these defend
//!
//! | Path | Budget | Source |
//! |------|--------|--------|
//! | Filtered, non-adversarial | < 50 ns/op, 0 allocs | Go filtered path: 5.479 ns/op |
//! | Any predicate, adversarial | < 10,000 ns/op | Phase 0 criterion, issue 0030 |
//!
//! The filtered budget is a REGRESSION ALARM, not a tuning target.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::{Document, Level, Point};
use domowik_geometry::wall_contact::{axis_contact, AxisContact, AxisRole};
use domowik_geometry::wall_overlap::rectangles_overlap;
use domowik_geometry::{orientation, wall_polygon, Orientation};
use uuid::Uuid;

/// The common case: a clearly non-degenerate triple. This is the path that must
/// stay in single-digit nanoseconds — it is what every hit test and every room
/// trace calls, millions of times.
fn orientation_filtered(c: &mut Criterion) {
    let a = Point::new(0.0, 0.0);
    let b = Point::new(4000.0, 0.0);
    let p = Point::new(1500.0, 2500.0);
    c.bench_function("orientation/filtered", |bench| {
        bench.iter(|| orientation(black_box(a), black_box(b), black_box(p)))
    });
}

/// The adversarial case: collinear input where the floating-point filter cannot
/// decide and the exact fallback runs. This is the path the Go implementation
/// took UNCONDITIONALLY, at 12,134 ns/op.
fn orientation_exact_fallback(c: &mut Criterion) {
    let a = Point::new(0.5, 0.5);
    let b = Point::new(12.0, 12.0);
    let p = Point::new(24.0, 24.0);
    c.bench_function("orientation/exact-fallback", |bench| {
        bench.iter(|| orientation(black_box(a), black_box(b), black_box(p)))
    });
}

/// The mixed-magnitude witness: a subnormal against a normal coordinate, so
/// the product underflows and the filter must escalate. Cost of the integer
/// fallback on the shape that motivated it.
fn orientation_mixed_subnormal(c: &mut Criterion) {
    let eta = f64::from_bits(1);
    let (a, b, p) = (
        Point::new(0.0, eta),
        Point::new(1.0, 0.0),
        Point::new(eta, eta),
    );
    assert_eq!(
        orientation(a, b, p),
        Orientation::CounterClockwise,
        "the exact determinant is eta^2 > 0"
    );
    c.bench_function("orientation/mixed-subnormal", |bench| {
        bench.iter(|| orientation(black_box(a), black_box(b), black_box(p)))
    });
}

/// Coordinates large enough that both products overflow to infinity, so the
/// filter cannot be trusted and the integer path answers instead.
fn orientation_overflow(c: &mut Criterion) {
    let big = 1e300;
    let (a, b, p) = (
        Point::new(0.0, 0.0),
        Point::new(big, big),
        Point::new(big, 2.0 * big),
    );
    assert_eq!(
        orientation(a, b, p),
        Orientation::CounterClockwise,
        "the exact determinant is 1e600 > 0"
    );
    c.bench_function("orientation/overflow", |bench| {
        bench.iter(|| orientation(black_box(a), black_box(b), black_box(p)))
    });
}

fn wall_polygon_bench(c: &mut Criterion) {
    let s = Point::new(0.0, 0.0);
    let e = Point::new(4000.0, 0.0);
    c.bench_function("wall_polygon", |bench| {
        bench.iter(|| wall_polygon(black_box(s), black_box(e), black_box(120.0)))
    });
}

/// The interval-filter paths for the three ordinary rectangle cases from
/// R3.0.2/R3.1.2. Assertions are deliberately outside the timed closures:
/// changing the expected classification must fail the benchmark immediately.
fn wall_overlap_filtered(c: &mut Criterion) {
    let mut group = c.benchmark_group("wall-overlap/filtered");
    let cases = [
        (
            "parallel-clear",
            Point::new(0.0, 0.0),
            Point::new(2000.0, 0.0),
            200.0,
            Point::new(0.0, 1000.0),
            Point::new(2000.0, 1000.0),
            200.0,
            false,
        ),
        (
            "parallel-overlap",
            Point::new(0.0, 0.0),
            Point::new(2000.0, 0.0),
            200.0,
            Point::new(0.0, 100.0),
            Point::new(2000.0, 100.0),
            200.0,
            true,
        ),
        (
            "nonparallel-specfixture",
            Point::new(0.0, 0.0),
            Point::new(2000.0, 0.0),
            200.0,
            Point::new(850.0, 900.0),
            Point::new(3250.0, -900.0),
            200.0,
            true,
        ),
    ];
    for (name, a0, a1, ta, b0, b1, tb, expected) in cases {
        assert_eq!(
            rectangles_overlap(a0, a1, ta, b0, b1, tb),
            Some(expected),
            "{name} must retain its specification classification"
        );
        group.bench_function(name, |bench| {
            bench.iter(|| {
                rectangles_overlap(
                    black_box(a0),
                    black_box(a1),
                    black_box(ta),
                    black_box(b0),
                    black_box(b1),
                    black_box(tb),
                )
            })
        });
    }
    group.finish();
}

/// Boundary and magnitude witnesses for the exact rectangle predicate.
fn wall_overlap_exact(c: &mut Criterion) {
    let mut group = c.benchmark_group("wall-overlap/exact");
    let ulp = 200.0f64.next_up();
    let eta = f64::from_bits(1);
    let cases = [
        (
            "touching",
            Point::new(0.0, 0.0),
            Point::new(2000.0, 0.0),
            200.0,
            Point::new(0.0, 200.0),
            Point::new(2000.0, 200.0),
            200.0,
            false,
        ),
        (
            "one-ulp-separated",
            Point::new(0.0, 0.0),
            Point::new(2000.0, 0.0),
            200.0,
            Point::new(0.0, ulp),
            Point::new(2000.0, ulp),
            200.0,
            false,
        ),
        (
            "mixed-subnormal",
            Point::new(0.0, 0.0),
            Point::new(1.0, eta),
            eta,
            Point::new(0.0, eta),
            Point::new(1.0, 0.0),
            eta,
            true,
        ),
    ];
    for (name, a0, a1, ta, b0, b1, tb, expected) in cases {
        assert_eq!(
            rectangles_overlap(a0, a1, ta, b0, b1, tb),
            Some(expected),
            "{name} must retain its exact classification"
        );
        group.bench_function(name, |bench| {
            bench.iter(|| {
                rectangles_overlap(
                    black_box(a0),
                    black_box(a1),
                    black_box(ta),
                    black_box(b0),
                    black_box(b1),
                    black_box(tb),
                )
            })
        });
    }
    group.finish();
}

/// R3.1.2 axis contact classification, per class.
///
/// The classes cost different amounts and the difference is structural, so
/// they are measured apart rather than averaged. `disjoint` is the common
/// case a pairwise scan pays on nearly every pair; `collinear-overlap` is the
/// branch that pays four orientation calls and then does interval work; and
/// `near-collinear` is the adversarial one, where the filtered determinant
/// cannot decide and `orientation` escalates to exact arithmetic.
///
/// Not a budget. This is one pair; the cost of classifying a DOCUMENT depends
/// on how many pairs the caller forms, which is the admission layer's design
/// and is not measured here.
fn axis_contact_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("wall-contact");
    let cases: [(&str, [Point; 4], AxisContact); 7] = [
        (
            // Diagonal and collinear, so all four orientation calls land on
            // exactly collinear input — the case where the filter cannot
            // decide any of them and each escalates.
            "diagonal-collinear-disjoint",
            [
                Point::new(0.5, 0.5),
                Point::new(12.0, 12.0),
                Point::new(24.0, 24.0),
                Point::new(36.0, 36.0),
            ],
            AxisContact::Disjoint,
        ),
        (
            "disjoint",
            [
                Point::new(0.0, 0.0),
                Point::new(4000.0, 0.0),
                Point::new(0.0, 500.0),
                Point::new(4000.0, 500.0),
            ],
            AxisContact::Disjoint,
        ),
        (
            "proper-crossing",
            [
                Point::new(0.0, 0.0),
                Point::new(4000.0, 0.0),
                Point::new(2000.0, -1000.0),
                Point::new(2000.0, 1000.0),
            ],
            AxisContact::ProperCrossing,
        ),
        (
            "endpoint-touch",
            [
                Point::new(0.0, 0.0),
                Point::new(4000.0, 0.0),
                Point::new(4000.0, 0.0),
                Point::new(4000.0, 3000.0),
            ],
            AxisContact::EndpointTouch {
                at: Point::new(4000.0, 0.0),
            },
        ),
        (
            "endpoint-on-interior",
            [
                Point::new(0.0, 0.0),
                Point::new(4000.0, 0.0),
                Point::new(2000.0, 0.0),
                Point::new(2000.0, 3000.0),
            ],
            AxisContact::EndpointOnInterior {
                split: AxisRole::First,
                at: Point::new(2000.0, 0.0),
            },
        ),
        (
            "collinear-overlap",
            [
                Point::new(0.0, 0.0),
                Point::new(4000.0, 0.0),
                Point::new(2000.0, 0.0),
                Point::new(6000.0, 0.0),
            ],
            AxisContact::CollinearOverlap,
        ),
        (
            // Both products enormous and nearly equal, one ulp off the axis:
            // the filter cannot decide and the exact fallback runs.
            "near-collinear-exact-fallback",
            [
                Point::new(-10_000_000.0, -10_000_000.0),
                Point::new(10_000_000.0, 10_000_000.0),
                Point::new(5_000_000.0, f64::from_bits(5_000_000f64.to_bits() + 1)),
                Point::new(5_000_000.0, 6_000_000.0),
            ],
            AxisContact::Disjoint,
        ),
    ];
    for (name, [a, b, cc, d], expected) in cases {
        assert_eq!(
            axis_contact(a, b, cc, d),
            Some(expected),
            "{name} must classify as the class it is named for"
        );
        group.bench_function(name, |bench| {
            bench.iter(|| axis_contact(black_box(a), black_box(b), black_box(cc), black_box(d)))
        });
    }
    group.finish();
}

/// §2.1's level advisories over a whole document.
///
/// The traversal sorts by elevation and UUID, then walks adjacent pairs. It
/// is measured WHOLE: sorting, the per-pair slab tests and the result vector's
/// allocation and drop are all inside the timed region, and nothing here
/// isolates their individual costs or attributes the total to any one of them.
///
/// The per-pair slab test is one correctly-rounded subtraction, escalating to
/// an error-free residual only when the rounded difference lands exactly on
/// the plane below. `clean/*` is the ordinary path, where that escalation
/// does not happen; `exact-tie/512` is where it does at every pair.
///
/// Not a budget. Warnings are a read-only DIAGNOSTIC channel — they never
/// gate a command, a save or an exit status — but they are recomputed on the
/// editor's refresh after an accepted edit, so the traversal is not a
/// rare-path cost and is worth watching.
fn level_warnings(c: &mut Criterion) {
    let mut group = c.benchmark_group("warnings/levels");

    /// `count` levels, inserted in DESCENDING elevation so the sort has real
    /// work rather than an already-ordered input.
    ///
    /// Every elevation is DISTINCT, so the UUID tie-break is never reached:
    /// these fixtures do not exercise it, and no conclusion about tied
    /// elevations follows from them.
    fn stack(count: usize, thickness: f64) -> Document {
        let mut doc = Document::new();
        for i in (0..count).rev() {
            let mut level = Level::new("L");
            level.id = Uuid::from_u128(count as u128 - i as u128);
            level.elevation_mm = i as f64 * 3000.0;
            level.floor_thickness_mm = thickness;
            doc.levels.push(level);
        }
        doc
    }

    for count in [8usize, 64, 512] {
        let doc = stack(count, 200.0);
        assert!(doc.warnings().is_empty());
        group.bench_function(format!("clean/{count}"), |bench| {
            bench.iter(|| black_box(&doc).warnings().len())
        });
    }

    // Every adjacent pair warns: each slab reaches a metre below the level
    // under it, so the result vector grows with the document.
    let warned = stack(512, 4000.0);
    assert_eq!(warned.warnings().len(), 511);
    group.bench_function("every-pair-warns/512", |bench| {
        bench.iter(|| black_box(&warned).warnings().len())
    });

    // The EXACT-TIE path: every slab's underside lands exactly on the plane
    // below, so the rounded comparison is undecided at every pair and the
    // error-free residual runs 511 times. This is the escalation's cost, and
    // the clean case above is what it is measured against.
    let mut tied = Document::new();
    for i in (0..512usize).rev() {
        let mut level = Level::new("L");
        level.id = Uuid::from_u128(512 - i as u128);
        level.elevation_mm = i as f64 * 3000.0;
        // Exactly the gap to the level below: touching, never intruding.
        level.floor_thickness_mm = if i == 0 { 0.0 } else { 3000.0 };
        tied.levels.push(level);
    }
    assert!(tied.warnings().is_empty(), "touching does not warn");
    group.bench_function("exact-tie/512", |bench| {
        bench.iter(|| black_box(&tied).warnings().len())
    });
    group.finish();
}

criterion_group!(
    benches,
    orientation_filtered,
    orientation_exact_fallback,
    orientation_mixed_subnormal,
    orientation_overflow,
    wall_polygon_bench,
    wall_overlap_filtered,
    wall_overlap_exact,
    axis_contact_bench,
    level_warnings
);
criterion_main!(benches);
