//! Exact point-distance predicate benchmarks (issue 0012).
//!
//! # What is measured, and why both paths separately
//!
//! `distance::compare_squared` and `distance::within_radius` run a
//! conservative floating-point enclosure first and fall back to exact dyadic
//! arithmetic only when the enclosures overlap. Those two paths differ by
//! orders of magnitude — AGENTS.md records a measured ~2,000x gap for the
//! orientation predicate's filter versus its exact fallback — so a single
//! blended number would describe neither. Each group below therefore pins the
//! path it exercises:
//!
//! | Benchmark | Path |
//! |---|---|
//! | `ordinary` | Filter decides; the case every pointer move actually hits |
//! | `near-tie` | Filter decides, but the quantities are one ulp apart |
//! | `exact-boundary` | Exact fallback: genuinely equal, so no filter can separate |
//! | `exact-underflow` | Exact fallback: squared differences underflow to zero |
//! | `exact-overflow` | Exact fallback: squared differences leave binary64's range |
//!
//! Timings are host-dependent and are NOT a pass/fail gate (ED-0070);
//! `scripts/verify.sh` compiles this on every run so the harness cannot rot.
//! The number that matters for a caller is not any single row but the ratio
//! between `ordinary` and the `exact-*` rows, together with how often real
//! input lands in each — a predicate whose filter misses often is a different
//! predicate from one whose filter hits.
//!
//! Scope: POINT primitives only. Nothing here measures distance to a segment,
//! a line or a region, and no such predicate exists yet.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::Point;
use domowik_geometry::distance::{compare_squared, within_radius};

const P: fn(f64, f64) -> Point = Point::new;

/// Ordering two candidates by distance from a pointer — the R6.3.1/R7.1.3
/// comparison, once per candidate pair.
fn compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("distance/compare");
    let origin = P(1234.5, -678.25);

    // Ordinary: millimetre-scale plan coordinates, clearly separated.
    let (a, b) = (P(1300.0, -600.0), P(1500.0, -400.0));
    group.bench_function("ordinary", |g| {
        g.iter(|| compare_squared(black_box(origin), black_box(a), black_box(b)))
    });

    // Near-tie: separated by one ulp of the coordinate, so the enclosures are
    // close but still disjoint.
    let (a, b) = (P(1300.0, -600.0), P(1300.0f64.next_up(), -600.0));
    group.bench_function("near-tie", |g| {
        g.iter(|| compare_squared(black_box(origin), black_box(a), black_box(b)))
    });

    // Exact fallback, equality: 3-4-5 rotations are exactly equidistant, and
    // no enclosure can ever separate equal quantities.
    let (a, b) = (P(3.0, 4.0), P(-4.0, 3.0));
    group.bench_function("exact-boundary", |g| {
        g.iter(|| compare_squared(black_box(P(0.0, 0.0)), black_box(a), black_box(b)))
    });

    // Exact fallback, underflow: the squared differences round to zero.
    let (a, b) = (P(f64::from_bits(1), 0.0), P(f64::from_bits(2), 0.0));
    group.bench_function("exact-underflow", |g| {
        g.iter(|| compare_squared(black_box(P(0.0, 0.0)), black_box(a), black_box(b)))
    });

    // Exact fallback, overflow: the squared differences leave binary64.
    let (a, b) = (P(1e308, 1e308), P(1e308, 1.0000001e308));
    group.bench_function("exact-overflow", |g| {
        g.iter(|| compare_squared(black_box(P(-1e308, -1e308)), black_box(a), black_box(b)))
    });

    group.finish();
}

/// Inclusive radius admission — the R7.1.3 pick-radius test, once per
/// candidate.
fn radius(c: &mut Criterion) {
    let mut group = c.benchmark_group("distance/within-radius");
    let origin = P(1234.5, -678.25);

    // Ordinary: a 240 mm pick radius (12 logical px at 20 mm/px), candidate
    // well inside it.
    group.bench_function("ordinary", |g| {
        g.iter(|| {
            within_radius(
                black_box(origin),
                black_box(P(1300.0, -700.0)),
                black_box(240.0),
            )
        })
    });

    // Exact fallback: exactly on the boundary, which is the case the whole
    // predicate exists for and the one no filter can decide.
    group.bench_function("exact-boundary", |g| {
        g.iter(|| {
            within_radius(
                black_box(P(0.0, 0.0)),
                black_box(P(3.0, 4.0)),
                black_box(5.0),
            )
        })
    });

    // Exact fallback: a subnormal separation against a zero radius.
    group.bench_function("exact-underflow", |g| {
        g.iter(|| {
            within_radius(
                black_box(P(0.0, 0.0)),
                black_box(P(f64::from_bits(1), 0.0)),
                black_box(0.0),
            )
        })
    });

    // Exact fallback: squares beyond binary64's range on both sides.
    group.bench_function("exact-overflow", |g| {
        g.iter(|| {
            within_radius(
                black_box(P(0.0, 0.0)),
                black_box(P(1e200, 1e200)),
                black_box(1e300),
            )
        })
    });

    // Refusal: a non-finite coordinate never reaches either path.
    group.bench_function("refused", |g| {
        g.iter(|| {
            within_radius(
                black_box(origin),
                black_box(P(f64::NAN, 0.0)),
                black_box(240.0),
            )
        })
    });

    group.finish();
}

criterion_group!(benches, compare, radius);
criterion_main!(benches);
