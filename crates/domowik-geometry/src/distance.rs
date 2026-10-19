//! Exact point-to-point distance ordering and radius admission.
//!
//! Two predicates over PLAN MILLIMETRES, both exact for every finite input:
//!
//! - [`compare_squared`] orders two candidate points by their distance from an
//!   origin — the comparison R6.3.1 and R7.1.3 rank hit and snap candidates by.
//! - [`within_radius`] answers the INCLUSIVE membership question "is this point
//!   within `radius_mm` of the origin", the R7.1.3 pick-radius admission test.
//!
//! # What "exact" means here, and what it does not
//!
//! Both compare SQUARED distances, so no square root is taken and no rounding
//! is introduced by one. The squared distances themselves are compared exactly:
//! `(px − qx)² + (py − qy)²` is a polynomial of degree two in values that are
//! already binary64, so it is a dyadic rational and is evaluated without
//! rounding on the fallback path. An answer of [`Ordering::Equal`], and an
//! inclusive boundary hit, are therefore facts about the real numbers the
//! inputs denote — not artefacts of the arithmetic.
//!
//! This is a claim about POINTS ONLY. It says nothing about distance to a
//! segment, a line, or a region: those need a projection, the projection is not
//! generally a dyadic rational of the inputs, and nothing here supplies it.
//! Region and projected-distance arithmetic remain an explicit gap, recorded
//! against issue 0012.
//!
//! # Domain
//!
//! Every coordinate must be finite, and `radius_mm` must be finite and
//! non-negative. Outside that domain both functions return `None` rather than
//! guessing: a non-finite coordinate has no dyadic form, and a negative radius
//! is not a pick radius. `-0.0` is accepted as a radius and behaves as `+0.0`.
//! Callers at the FFI boundary already reject non-finite input before it
//! reaches the core; `None` is the second line of that defence, not a
//! substitute for it.
//!
//! # How it stays fast
//!
//! A conservative floating-point filter runs first and decides the ordinary
//! case; the exact path runs only when the filter cannot separate the two
//! quantities. The filter is an OUTWARD-ROUNDED ENCLOSURE rather than a
//! running error bound, and the difference matters — see [`Enclosure`].

use crate::enclosure::{exactly, Enclosure};
use std::cmp::Ordering;

use crate::dyadic::Dyadic;
use domowik_format::Point;

/// Orders `a` and `b` by their distance from `origin`, exactly.
///
/// Compares squared distances, so the result is the distance ordering without
/// a square root. `Ordering::Equal` means the two points are EXACTLY
/// equidistant from `origin` in the reals, which is why callers may use it as
/// a tie signal and fall through to a further key (R6.3.1 does exactly that).
///
/// `None` when any coordinate of any argument is not finite.
///
/// Coordinates are plan millimetres. Determinism: the answer depends only on
/// the three argument values, never on evaluation order or history.
pub fn compare_squared(origin: Point, a: Point, b: Point) -> Option<Ordering> {
    if !finite(origin) || !finite(a) || !finite(b) {
        return None;
    }
    let (sa, sb) = (squared(origin, a), squared(origin, b));
    Some(match separate(sa, sb) {
        Some(order) => order,
        None => exact_squared(origin, a).cmp(&exact_squared(origin, b)),
    })
}

/// One point's squared distance from an origin, prepared for MANY exact
/// comparisons — the shape a ranking sort needs.
///
/// [`compare_squared`] recomputes both enclosures on every call and, on a
/// near-tie, both exact values. A sort makes `O(n log n)` comparisons, and
/// candidates that genuinely tie (axis projections of collinear walls onto
/// the same pointer, a few ulps apart) send most of them down the exact path;
/// how many depended on the input order, which made a snap query's cost
/// depend on the order the index happened to return candidates in. Here
/// each candidate's enclosure is computed once, and its exact value at most
/// once, on first need, so a sort costs at most `n` exact evaluations
/// whatever the order. The answers are identical to [`compare_squared`].
#[derive(Debug, Clone)]
pub struct SquaredDistance {
    origin: Point,
    at: Point,
    enclosure: Enclosure,
    exact: std::cell::OnceCell<Dyadic>,
}

impl SquaredDistance {
    /// `None` when a coordinate is not finite.
    pub fn new(origin: Point, at: Point) -> Option<Self> {
        if !finite(origin) || !finite(at) {
            return None;
        }
        Some(Self {
            origin,
            at,
            enclosure: squared(origin, at),
            exact: std::cell::OnceCell::new(),
        })
    }

    fn exact(&self) -> &Dyadic {
        self.exact
            .get_or_init(|| exact_squared(self.origin, self.at))
    }

    /// Exact ordering of the two squared distances. Both must share an
    /// origin (checked in debug builds); otherwise the answer is meaningless.
    pub fn compare(&self, other: &Self) -> Ordering {
        debug_assert_eq!(
            self.origin, other.origin,
            "distances from different origins"
        );
        if self.at == other.at {
            return Ordering::Equal;
        }
        separate(self.enclosure, other.enclosure).unwrap_or_else(|| self.exact().cmp(other.exact()))
    }
}

/// Whether `at` lies within `radius_mm` of `origin`, inclusive of the
/// boundary, exactly.
///
/// Inclusive: a point at exactly `radius_mm` is admitted, which is what
/// R7.1.3's pick radius requires. Because the comparison is exact, "exactly
/// `radius_mm`" is a statement about the reals and not about whichever side
/// rounding happened to prefer.
///
/// `None` when a coordinate is not finite, or when `radius_mm` is not finite
/// or is negative. `radius_mm` is plan millimetres — already converted from
/// logical pixels by the caller's view transform, since this module has no
/// view transform and must not invent one.
pub fn within_radius(origin: Point, at: Point, radius_mm: f64) -> Option<bool> {
    if !finite(origin) || !finite(at) || !radius_mm.is_finite() || radius_mm < 0.0 {
        return None;
    }
    let (distance, radius) = (squared(origin, at), Enclosure::square(exactly(radius_mm)));
    Some(match separate(distance, radius) {
        Some(order) => order != Ordering::Greater,
        None => exact_squared(origin, at) <= Dyadic::from_f64(radius_mm).square(),
    })
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// An enclosure of the squared distance between two points.
fn squared(p: Point, q: Point) -> Enclosure {
    let dx = Enclosure::difference(p.x, q.x).square();
    let dy = Enclosure::difference(p.y, q.y).square();
    dx.sum(dy)
}

/// The ordering of two enclosed quantities when their intervals are disjoint,
/// or `None` when they overlap and only exact arithmetic can decide.
///
/// Non-finite endpoints are treated as undecidable rather than reasoned about.
/// A coordinate difference can overflow to infinity for legitimately finite
/// inputs (`1e308 − −1e308`), and a squared difference overflows far sooner,
/// so this is a reachable case and not a formality. It costs only speed: the
/// exact path has no magnitude precondition, since a squared difference of
/// binary64 values occupies dyadic exponents within `[-2148, 1942]`.
fn separate(a: Enclosure, b: Enclosure) -> Option<Ordering> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    if a.hi < b.lo {
        Some(Ordering::Less)
    } else if a.lo > b.hi {
        Some(Ordering::Greater)
    } else {
        None
    }
}

/// The exact squared distance, as a dyadic rational.
///
/// Every step is exact: `Dyadic::from_f64` reads the IEEE-754 encoding
/// (including subnormals, and mapping both signed zeros to one canonical
/// zero), and dyadic subtraction, squaring and addition introduce no rounding.
fn exact_squared(p: Point, q: Point) -> Dyadic {
    let dx = Dyadic::from_f64(p.x).sub(&Dyadic::from_f64(q.x));
    let dy = Dyadic::from_f64(p.y).sub(&Dyadic::from_f64(q.y));
    dx.square().add(&dy.square())
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;
    const ORIGIN: Point = Point { x: 0.0, y: 0.0 };

    /// An INDEPENDENT oracle: the same question answered in integer
    /// arithmetic, with no dyadic rational anywhere near it.
    ///
    /// Restricted to integral coordinates of magnitude at most 2^31, where
    /// every difference fits an `i64` and every square and their sum fit an
    /// `i128` exactly — so this is not an approximation of the answer, it IS
    /// the answer, computed a completely different way. Two exact
    /// implementations disagreeing is a real signal; that is the same
    /// discipline `exact_det` and `dyadic` already provide for orientation.
    fn integer_squared(p: Point, q: Point) -> i128 {
        for v in [p.x, p.y, q.x, q.y] {
            assert!(
                v.fract() == 0.0 && v.abs() <= 2f64.powi(31),
                "oracle domain"
            );
        }
        let dx = (p.x as i128) - (q.x as i128);
        let dy = (p.y as i128) - (q.y as i128);
        dx * dx + dy * dy
    }

    /// Origins the oracle sweeps use. A zero origin makes every coordinate
    /// difference `px − 0`, which is exact and therefore never exercises the
    /// subtraction at all, so the sweeps run against off-origin points too.
    /// Rounding in the subtraction itself cannot be reached from inside this
    /// oracle's integral domain — differences of integers this size are exact
    /// in binary64 — so that case is covered separately by
    /// [`the_filter_never_disagrees_with_exact_arithmetic`] and by
    /// [`a_fractional_origin_rounds_the_subtraction`].
    const ORACLE_ORIGINS: [Point; 3] = [
        Point { x: 0.0, y: 0.0 },
        Point {
            x: 137.0,
            y: -239.0,
        },
        Point {
            x: -1_048_576.0,
            y: 65_537.0,
        },
    ];

    #[test]
    fn a_prepared_distance_orders_exactly_as_compare_squared() {
        let o = P(0.1, -0.3);
        let pts = [
            P(1.0, 0.0),
            P(0.0, 1.0),
            P(1.0 + f64::EPSILON, 0.0),
            P(0.1, 0.7),
            P(0.1, -1.3),
            P(1e300, 0.0),
            P(-0.0, 0.0),
            P(0.1 + 1e-17, -0.3),
        ];
        for &a in &pts {
            for &b in &pts {
                let (da, db) = (
                    SquaredDistance::new(o, a).unwrap(),
                    SquaredDistance::new(o, b).unwrap(),
                );
                assert_eq!(
                    Some(da.compare(&db)),
                    compare_squared(o, a, b),
                    "{a:?} {b:?}"
                );
            }
        }
        assert!(SquaredDistance::new(o, P(f64::NAN, 0.0)).is_none());
    }

    #[test]
    fn the_integer_oracle_agrees_over_a_sweep() {
        let mut checked = 0;
        for origin in ORACLE_ORIGINS {
            for ax in (-400..=400).step_by(83) {
                for ay in (-400..=400).step_by(97) {
                    for bx in (-400..=400).step_by(101) {
                        for by in (-400..=400).step_by(89) {
                            let (a, b) = (P(ax as f64, ay as f64), P(bx as f64, by as f64));
                            let want = integer_squared(origin, a).cmp(&integer_squared(origin, b));
                            assert_eq!(
                                compare_squared(origin, a, b),
                                Some(want),
                                "origin={origin:?} a={a:?} b={b:?}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 3000, "the sweep must actually cover something");
    }

    #[test]
    fn the_integer_oracle_agrees_on_radius_admission() {
        for origin in ORACLE_ORIGINS {
            for x in (-30..=30).step_by(7) {
                for y in (-30..=30).step_by(9) {
                    for r in 0..=42 {
                        let at = P(origin.x + x as f64, origin.y + y as f64);
                        let want = integer_squared(origin, at) <= (r as i128) * (r as i128);
                        assert_eq!(
                            within_radius(origin, at, r as f64),
                            Some(want),
                            "origin={origin:?} at={at:?} r={r}"
                        );
                    }
                }
            }
        }
    }

    /// The case the integral oracle cannot reach: an origin whose subtraction
    /// genuinely rounds. Checked against the exact path rather than against
    /// integers, since the exact answer is no longer an integer.
    #[test]
    fn a_fractional_origin_rounds_the_subtraction() {
        let origin = P(0.1, -0.3);
        let mut rounded = 0;
        for i in -20..=20 {
            for j in -20..=20 {
                let a = P(i as f64 * 0.7, j as f64 * 1.3);
                let b = P(j as f64 * 0.9, i as f64 * 1.1);
                // "Did this subtraction round?" asked exactly: compare the
                // binary64 difference against the dyadic one. A round-trip
                // through `+ origin.x` would not answer it — that can come
                // back exact even when the subtraction lost a bit.
                let exact = Dyadic::from_f64(a.x).sub(&Dyadic::from_f64(origin.x));
                if Dyadic::from_f64(a.x - origin.x).cmp(&exact) != Ordering::Equal {
                    rounded += 1;
                }
                let want = exact_squared(origin, a).cmp(&exact_squared(origin, b));
                assert_eq!(
                    compare_squared(origin, a, b),
                    Some(want),
                    "origin={origin:?} a={a:?} b={b:?}"
                );
            }
        }
        assert!(
            rounded > 0,
            "the fixture must actually round some subtractions"
        );
    }

    // ---- exact boundaries ----

    #[test]
    fn a_point_exactly_on_the_radius_is_admitted() {
        // 3-4-5 scaled by powers of two keeps the distance exactly
        // representable, so "exactly on the boundary" is a real case and not
        // an approximation of one.
        for k in [-60i32, -20, 0, 20, 60] {
            let s = 2f64.powi(k);
            let at = P(3.0 * s, 4.0 * s);
            assert_eq!(within_radius(ORIGIN, at, 5.0 * s), Some(true), "k={k}");
            // One ulp of radius less, and it is outside — the boundary is
            // decided exactly, not by whichever side rounding preferred.
            assert_eq!(
                within_radius(ORIGIN, at, (5.0 * s).next_down()),
                Some(false),
                "k={k}"
            );
            assert_eq!(within_radius(ORIGIN, at, (5.0 * s).next_up()), Some(true));
        }
    }

    #[test]
    fn exactly_equidistant_points_compare_equal() {
        let a = P(3.0, 4.0);
        for b in [P(-4.0, 3.0), P(4.0, -3.0), P(-3.0, -4.0), P(5.0, 0.0)] {
            assert_eq!(
                compare_squared(ORIGIN, a, b),
                Some(Ordering::Equal),
                "b={b:?}"
            );
        }
    }

    #[test]
    fn a_one_ulp_difference_is_ordered_not_tied() {
        // The near-tie the floating filter must NOT resolve by accident.
        let a = P(1.0, 0.0);
        let b = P(1.0f64.next_up(), 0.0);
        assert_eq!(compare_squared(ORIGIN, a, b), Some(Ordering::Less));
        assert_eq!(compare_squared(ORIGIN, b, a), Some(Ordering::Greater));
        // 1 + 1 ulp squared rounds back to 1 + 1 ulp's square only inexactly;
        // the exact path is what makes this strict.
        assert_ne!(compare_squared(ORIGIN, a, b), Some(Ordering::Equal));
    }

    // ---- signed zero ----

    #[test]
    fn signed_zero_coordinates_are_the_same_point() {
        for p in [P(0.0, 0.0), P(-0.0, 0.0), P(0.0, -0.0), P(-0.0, -0.0)] {
            assert_eq!(within_radius(p, P(-0.0, -0.0), 0.0), Some(true), "p={p:?}");
            assert_eq!(
                compare_squared(p, P(0.0, -0.0), P(-0.0, 0.0)),
                Some(Ordering::Equal),
                "p={p:?}"
            );
        }
    }

    #[test]
    fn a_negative_zero_radius_is_a_zero_radius() {
        assert_eq!(within_radius(ORIGIN, ORIGIN, -0.0), Some(true));
        assert_eq!(within_radius(ORIGIN, P(1.0, 0.0), -0.0), Some(false));
    }

    // ---- subnormals ----

    #[test]
    fn a_subnormal_separation_is_not_mistaken_for_zero() {
        // The filter's squares UNDERFLOW to zero here, which is exactly the
        // case an error-free transformation cannot bound. The point is a
        // hair away from the origin, so a zero radius must exclude it.
        let tiny = f64::MIN_POSITIVE * f64::EPSILON; // a subnormal
        assert!(tiny.is_subnormal() && tiny > 0.0);
        let at = P(tiny, 0.0);
        assert_eq!(within_radius(ORIGIN, at, 0.0), Some(false));
        assert_eq!(within_radius(ORIGIN, at, tiny), Some(true));
        // And it is strictly further out than the origin itself.
        assert_eq!(compare_squared(ORIGIN, ORIGIN, at), Some(Ordering::Less));
    }

    #[test]
    fn two_subnormal_separations_are_ordered_against_each_other() {
        let one = P(f64::from_bits(1), 0.0);
        let two = P(f64::from_bits(2), 0.0);
        assert_eq!(compare_squared(ORIGIN, one, two), Some(Ordering::Less));
        assert_eq!(compare_squared(ORIGIN, two, one), Some(Ordering::Greater));
        assert_eq!(compare_squared(ORIGIN, one, one), Some(Ordering::Equal));
    }

    // ---- overflow ----

    #[test]
    fn coordinates_that_overflow_the_filter_are_still_ordered() {
        // Both the difference and its square leave binary64's range, so the
        // filter cannot decide and the exact path must.
        let far = P(1e308, 1e308);
        let further = P(1e308, 1.0000001e308);
        assert_eq!(
            compare_squared(P(-1e308, -1e308), far, further),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_squared(P(-1e308, -1e308), far, far),
            Some(Ordering::Equal)
        );
        // A separation of 2e308 exceeds EVERY finite radius, and saying so is
        // the whole point: computed in binary64 the difference overflows to
        // infinity, its square is infinity, `f64::MAX`'s square is infinity
        // too, and `inf <= inf` admits the point. The exact path does not.
        assert_eq!(
            within_radius(P(-1e308, 0.0), P(1e308, 0.0), 1e308),
            Some(false)
        );
        assert_eq!(
            within_radius(P(-1e308, 0.0), P(1e308, 0.0), f64::MAX),
            Some(false),
            "no finite radius reaches 2e308"
        );
        // An inside case whose squares also overflow, so the exact path is
        // what admits it: 1e200·√2 is comfortably under 1e300.
        assert_eq!(within_radius(ORIGIN, P(1e200, 1e200), 1e300), Some(true));
        assert_eq!(within_radius(ORIGIN, P(1e200, 1e200), 1e200), Some(false));
    }

    // ---- domain rejection ----

    #[test]
    fn non_finite_input_is_refused_rather_than_guessed() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(compare_squared(P(bad, 0.0), ORIGIN, ORIGIN), None);
            assert_eq!(compare_squared(ORIGIN, P(0.0, bad), ORIGIN), None);
            assert_eq!(compare_squared(ORIGIN, ORIGIN, P(bad, bad)), None);
            assert_eq!(within_radius(P(0.0, bad), ORIGIN, 1.0), None);
            assert_eq!(within_radius(ORIGIN, P(bad, 0.0), 1.0), None);
            assert_eq!(within_radius(ORIGIN, ORIGIN, bad), None);
        }
    }

    #[test]
    fn a_negative_radius_is_refused() {
        assert_eq!(within_radius(ORIGIN, ORIGIN, -1.0), None);
        assert_eq!(within_radius(ORIGIN, ORIGIN, -f64::MIN_POSITIVE), None);
        assert_eq!(within_radius(ORIGIN, ORIGIN, 0.0), Some(true));
    }

    // ---- the ordering is an ordering ----

    #[test]
    fn the_comparison_is_antisymmetric_and_transitive_over_a_sweep() {
        let points: Vec<Point> = [
            (0.0, 0.0),
            (1.0, 0.0),
            (-1.0, 0.0),
            (3.0, 4.0),
            (-4.0, 3.0),
            (1e-300, 1e-300),
            (f64::MIN_POSITIVE, 0.0),
            (1e300, -1e300),
            (5.0, 0.0),
        ]
        .iter()
        .map(|&(x, y)| P(x, y))
        .collect();
        let origin = P(0.25, -0.5);
        for a in &points {
            for b in &points {
                let ab = compare_squared(origin, *a, *b).expect("finite");
                assert_eq!(compare_squared(origin, *b, *a), Some(ab.reverse()));
                for c in &points {
                    let bc = compare_squared(origin, *b, *c).expect("finite");
                    if ab == bc && ab != Ordering::Equal {
                        assert_eq!(
                            compare_squared(origin, *a, *c),
                            Some(ab),
                            "{a:?} {b:?} {c:?}"
                        );
                    }
                }
            }
        }
    }

    /// Which path each `benches/distance.rs` fixture actually takes.
    ///
    /// The benchmark's whole value is that its rows name the path they
    /// measure, and that is a claim about `separate` — so it is asserted here
    /// rather than believed. If a fixture ever migrates from one path to the
    /// other, this fails and the benchmark's prose gets corrected with it.
    #[test]
    fn the_benchmark_fixtures_take_the_paths_the_benchmark_claims() {
        let decided = |origin: Point, a: Point, b: Point| {
            separate(squared(origin, a), squared(origin, b)).is_some()
        };

        // `compare/ordinary` and `compare/near-tie`: the filter decides.
        let pointer = P(1234.5, -678.25);
        assert!(
            decided(pointer, P(1300.0, -600.0), P(1500.0, -400.0)),
            "compare/ordinary must be decided by the filter"
        );
        assert!(
            decided(pointer, P(1300.0, -600.0), P(1300.0f64.next_up(), -600.0)),
            "compare/near-tie must be decided by the filter"
        );

        // The `exact-*` fixtures: the filter cannot decide any of them.
        assert!(
            !decided(ORIGIN, P(3.0, 4.0), P(-4.0, 3.0)),
            "compare/exact-boundary is an exact tie; no enclosure can separate it"
        );
        assert!(
            !decided(ORIGIN, P(f64::from_bits(1), 0.0), P(f64::from_bits(2), 0.0)),
            "compare/exact-underflow must reach the exact path"
        );
        assert!(
            !decided(P(-1e308, -1e308), P(1e308, 1e308), P(1e308, 1.0000001e308)),
            "compare/exact-overflow must reach the exact path"
        );

        // The radius fixtures, against the radius enclosure rather than a
        // second point.
        let radius_decided = |origin: Point, at: Point, r: f64| {
            separate(squared(origin, at), Enclosure::square(exactly(r))).is_some()
        };
        assert!(
            radius_decided(pointer, P(1300.0, -700.0), 240.0),
            "within-radius/ordinary must be decided by the filter"
        );
        assert!(
            !radius_decided(ORIGIN, P(3.0, 4.0), 5.0),
            "within-radius/exact-boundary must reach the exact path"
        );
        assert!(
            !radius_decided(ORIGIN, P(f64::from_bits(1), 0.0), 0.0),
            "within-radius/exact-underflow must reach the exact path"
        );
        assert!(
            !radius_decided(ORIGIN, P(1e200, 1e200), 1e300),
            "within-radius/exact-overflow must reach the exact path"
        );
    }

    /// The filter must never decide differently from the exact path. Sweeping
    /// magnitudes that straddle the underflow and overflow thresholds is where
    /// a filter that looked fine on ordinary numbers goes wrong.
    #[test]
    fn the_filter_never_disagrees_with_exact_arithmetic() {
        let scales = [1e-320, 1e-200, 1e-8, 1.0, 1e8, 1e200, 1e300];
        for s in scales {
            for t in scales {
                let (a, b) = (P(3.0 * s, 4.0 * s), P(5.0 * t, 0.0));
                let want = exact_squared(ORIGIN, a).cmp(&exact_squared(ORIGIN, b));
                assert_eq!(compare_squared(ORIGIN, a, b), Some(want), "s={s:e} t={t:e}");
                let radius = 5.0 * t;
                let want = exact_squared(ORIGIN, a) <= Dyadic::from_f64(radius).square();
                assert_eq!(
                    within_radius(ORIGIN, a, radius),
                    Some(want),
                    "s={s:e} r={radius:e}"
                );
            }
        }
    }
}
