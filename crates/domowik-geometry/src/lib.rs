//! Geometry: robust predicates and wall polygon generation.
//!
//! # Why the predicates are shaped this way
//!
//! The move to Rust was triggered by two Go predicates missing the 10,000 ns/op
//! bound (12,134 and 20,458 ns/op, 426 and 538 allocations per call) because
//! they evaluated exact arithmetic unconditionally. The same package's
//! adaptively FILTERED path ran at 5.479 ns/op with zero allocations — roughly
//! 2,000x apart, same hardware, same run.
//!
//! The lesson encoded here: **the win is the adaptive filter, not the
//! language.** Every predicate tries a floating-point evaluation with a proven
//! error bound first and escalates only when the bound cannot decide the sign.

#![forbid(unsafe_code)]

mod dyadic;
mod exact_det;
mod wall_overlap_exact;

pub mod arrangement;
pub mod boolean;
pub mod distance;
mod enclosure;
pub mod exact_area;
pub mod hit;
pub mod locality;
pub mod overlay;
pub mod polygon;
pub mod rooms;
pub mod snap;
pub mod spatial;
pub mod wall_contact;
pub mod wall_overlap;
pub mod walls;

use domowik_format::Point;

/// Which side of a directed line a point lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// `c` is left of the directed line `a → b` (counter-clockwise turn).
    CounterClockwise,
    /// `c` is right of `a → b` (clockwise turn).
    Clockwise,
    /// `a`, `b`, `c` are collinear.
    Collinear,
}

/// Error bound constant for the 2x2 determinant filter.
///
/// The determinant `(bx-ax)(cy-ay) - (by-ay)(cx-ax)` involves two products and
/// one subtraction. Each rounds with relative error at most `ε = 2^-53`, and
/// standard forward analysis (Shewchuk 1997, "Adaptive Precision Floating-Point
/// Arithmetic and Fast Robust Geometric Predicates", §2.2) bounds the total
/// error by `3ε + O(ε²)` times the sum of the absolute values of the products.
/// `3.3ε` covers the constant with margin for the second-order term.
const ORIENT_FILTER: f64 = 3.3 * f64::EPSILON / 2.0;

/// Returns the orientation of the triple `(a, b, c)`.
///
/// # Accuracy
///
/// Exact for every finite input, with NO magnitude precondition — subnormals,
/// near-maximal values and mixtures of the two all included.
///
/// The fast path is a filtered floating-point determinant. It is trusted only
/// when its own arithmetic is trustworthy, which needs three conditions, each
/// checked:
///
/// 1. the computed determinant is provably larger than its error bound;
/// 2. neither product UNDERFLOWED — a product below `f64::MIN_POSITIVE` whose
///    operands were both non-zero has lost bits the filter cannot see, and
///    its error bound is meaningless because the bound itself underflows;
/// 3. nothing OVERFLOWED — two large coordinates can push a product, or the
///    difference of two products, to infinity.
///
/// Otherwise the sign is recomputed by [`exact_det::orientation_sign`], which
/// is integer arithmetic over the stored coordinates and has no failure mode.
///
/// Inputs containing an infinity or NaN are outside the domain
/// (`COORD_BOUND_MM` rejects them at validation) and are not covered by this
/// guarantee.
///
/// # Performance
///
/// A handful of floating-point operations and no allocation. There are
/// several branches — the two sign shortcuts, the underflow and overflow
/// guards, and the final bound comparison — all predictable and all taken the
/// same way for the overwhelming majority of non-degenerate inputs, which is
/// what the filter decides without escalating. See `benches/predicates.rs`.
#[inline]
pub fn orientation(a: Point, b: Point, c: Point) -> Orientation {
    let (dxb, dyb) = (b.x - a.x, b.y - a.y);
    let (dxc, dyc) = (c.x - a.x, c.y - a.y);
    let detleft = dxb * dyc;
    let detright = dyb * dxc;
    let det = detleft - detright;

    // THE FILTER'S OWN ARITHMETIC MUST BE SOUND BEFORE ITS BOUND MEANS
    // ANYTHING.
    //
    // Two failures are invisible to a relative-error bound:
    //
    // - UNDERFLOW. A product of two non-zero doubles can land in the
    //   subnormal range or round to zero outright, losing bits; and the error
    //   bound, being a fraction of that product, underflows with it. The
    //   witness is `orientation((0, η), (1, 0), (η, η))` with η the smallest
    //   subnormal: the true determinant is η² > 0, but `(−η)·η` rounds to −0
    //   and every downstream test sees a clean zero. Found by adversarial
    //   review (Codex, 2026-09-05).
    // - OVERFLOW. Large coordinates push a product, or `det` itself, to
    //   infinity, and `classify` would then report a turn from a NaN or an
    //   infinity.
    //
    // A GENUINE zero product — an axis-aligned wall, where one difference is
    // exactly zero — is not underflow and keeps the fast path, so the test is
    // "zero-or-subnormal from non-zero operands", never "small".
    let underflowed = |p: f64, u: f64, v: f64| p.abs() < f64::MIN_POSITIVE && u != 0.0 && v != 0.0;
    if !det.is_finite() || underflowed(detleft, dxb, dyc) || underflowed(detright, dyb, dxc) {
        return exact(a, b, c);
    }

    // Error bound scaled to the magnitude of the operands. When the two
    // products have opposite signs the subtraction cannot cancel, so the sign
    // is trustworthy immediately.
    let detsum = if detleft > 0.0 {
        if detright <= 0.0 {
            return classify(det);
        }
        detleft + detright
    } else if detleft < 0.0 {
        if detright >= 0.0 {
            return classify(det);
        }
        -detleft - detright
    } else {
        // `detleft` is a GENUINE zero: the underflow guard above already sent
        // every rounded-to-zero product to the exact path, so reaching here
        // means `dxb` or `dyc` really is zero — an axis-aligned wall, the
        // commonest shape in a plan. `det` is then exactly `-detright`, whose
        // sign is exact.
        return classify(det);
    };

    if det.abs() > ORIENT_FILTER * detsum {
        return classify(det);
    }

    // The filter could not decide. Fall back to an exact evaluation.
    exact(a, b, c)
}

/// The exact sign, as an [`Orientation`].
///
/// Integer arithmetic over the STORED coordinates — no differences, no
/// scaling, no floating-point expansion — so it has no magnitude
/// precondition and no failure mode for any finite input. See
/// [`exact_det`].
#[inline]
fn exact(a: Point, b: Point, c: Point) -> Orientation {
    match exact_det::orientation_sign(a.x, a.y, b.x, b.y, c.x, c.y) {
        std::cmp::Ordering::Greater => Orientation::CounterClockwise,
        std::cmp::Ordering::Less => Orientation::Clockwise,
        std::cmp::Ordering::Equal => Orientation::Collinear,
    }
}

#[inline]
fn classify(det: f64) -> Orientation {
    if det > 0.0 {
        Orientation::CounterClockwise
    } else if det < 0.0 {
        Orientation::Clockwise
    } else {
        Orientation::Collinear
    }
}

/// The four corners of a wall's rectangular footprint, counter-clockwise.
///
/// The wall is extruded symmetrically about its centre line by half its
/// thickness. Join geometry between adjacent walls is NOT applied — that is
/// room reconciliation, and it is not implemented yet.
pub fn wall_polygon(start: Point, end: Point, thickness_mm: f64) -> Option<[Point; 4]> {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let len = dx.hypot(dy);
    if !len.is_finite() || len == 0.0 {
        return None;
    }
    // Unit normal, rotated +90° from the wall direction.
    let half = thickness_mm / 2.0;
    let (nx, ny) = (-dy / len * half, dx / len * half);
    Some([
        Point::new(start.x - nx, start.y - ny),
        Point::new(end.x - nx, end.y - ny),
        Point::new(end.x + nx, end.y + ny),
        Point::new(start.x + nx, start.y + ny),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    #[test]
    fn orientation_classifies_the_three_obvious_cases() {
        let (a, b) = (P(0.0, 0.0), P(1.0, 0.0));
        assert_eq!(
            orientation(a, b, P(0.0, 1.0)),
            Orientation::CounterClockwise
        );
        assert_eq!(orientation(a, b, P(0.0, -1.0)), Orientation::Clockwise);
        assert_eq!(orientation(a, b, P(2.0, 0.0)), Orientation::Collinear);
    }

    #[test]
    fn orientation_is_antisymmetric_under_swapping_two_points() {
        let (a, b, c) = (P(0.0, 0.0), P(4000.0, 100.0), P(1500.0, 2500.0));
        assert_eq!(orientation(a, b, c), Orientation::CounterClockwise);
        assert_eq!(orientation(b, a, c), Orientation::Clockwise);
    }

    /// The case the filter exists for. These points are collinear, but the
    /// naive determinant evaluates to a non-zero value because the products
    /// cancel catastrophically. A predicate without an exact fallback reports
    /// a turn where there is none.
    #[test]
    fn nearly_degenerate_collinear_points_are_exactly_collinear() {
        // Chosen so (b-a) and (c-a) are exact multiples: c lies on the line
        // through a and b, but the intermediate products are not representable.
        let a = P(0.5, 0.5);
        let b = P(12.0, 12.0);
        let c = P(24.0, 24.0);
        assert_eq!(orientation(a, b, c), Orientation::Collinear);

        // A point one ULP off the line must NOT report collinear.
        let off = P(24.0, f64::from_bits(24.0f64.to_bits() + 1));
        assert_eq!(
            orientation(a, b, off),
            Orientation::CounterClockwise,
            "a point one ULP above the line is not collinear"
        );
    }

    /// Guards the fallback specifically: the filter and the exact path must
    /// agree wherever the filter is willing to decide.
    #[test]
    fn the_exact_fallback_agrees_with_the_filter_on_decidable_input() {
        let cases = [
            (P(0.0, 0.0), P(1000.0, 0.0), P(500.0, 1.0)),
            (P(0.0, 0.0), P(1000.0, 0.0), P(500.0, -1.0)),
            (P(-3.0, -7.0), P(11.0, 13.0), P(29.0, 41.0)),
        ];
        for (a, b, c) in cases {
            assert_eq!(
                orientation(a, b, c),
                exact(a, b, c),
                "filter and exact path disagree on {a:?} {b:?} {c:?}"
            );
        }
    }

    /// The witness that forced the integer fallback. Its determinant is `η²`,
    /// which no `f64` can hold — but its SIGN is not in doubt.
    #[test]
    fn a_subnormal_determinant_is_not_reported_as_collinear() {
        let eta = f64::from_bits(1);
        assert_eq!(
            orientation(P(0.0, eta), P(1.0, 0.0), P(eta, eta)),
            Orientation::CounterClockwise,
            "exact determinant is eta^2 > 0"
        );
        assert_eq!(
            orientation(P(1.0, 0.0), P(0.0, eta), P(eta, eta)),
            Orientation::Clockwise,
            "and it reverses with the triple"
        );
        // Both products underflowing, not just one.
        assert_eq!(
            orientation(P(0.0, 0.0), P(eta, 0.0), P(0.0, eta)),
            Orientation::CounterClockwise
        );
    }

    /// Mixed magnitudes are the case a uniform rescale cannot serve: the
    /// factor the subnormal needs overflows the large coordinate, and the
    /// factor the large one tolerates leaves the subnormal underflowing.
    #[test]
    fn mixed_subnormal_and_large_coordinates_are_exact() {
        let eta = f64::from_bits(1);
        for scale in [1.0f64, 1e100, 1e200, f64::MAX / 4.0] {
            assert_eq!(
                orientation(P(0.0, 0.0), P(scale, 0.0), P(0.0, eta)),
                Orientation::CounterClockwise,
                "scale {scale}"
            );
            assert_eq!(
                orientation(P(0.0, 0.0), P(0.0, scale), P(eta, 0.0)),
                Orientation::Clockwise,
                "scale {scale}"
            );
        }
    }

    /// A product that OVERFLOWS must not be classified from an infinity.
    #[test]
    fn overflowing_products_fall_back_rather_than_reporting_an_infinity() {
        let big = 1e300;
        // Each product overflows to infinity; the true determinant is
        // 1e300*2e300 - 1e300*1e300 = 1e600 > 0.
        assert_eq!(
            orientation(P(0.0, 0.0), P(big, big), P(big, 2.0 * big)),
            Orientation::CounterClockwise
        );
        assert_eq!(
            orientation(P(0.0, 0.0), P(big, 2.0 * big), P(big, big)),
            Orientation::Clockwise
        );
    }

    #[test]
    fn wall_polygon_is_symmetric_about_the_centre_line() {
        let poly = wall_polygon(P(0.0, 0.0), P(1000.0, 0.0), 200.0).expect("non-degenerate");
        assert_eq!(poly[0], P(0.0, -100.0));
        assert_eq!(poly[1], P(1000.0, -100.0));
        assert_eq!(poly[2], P(1000.0, 100.0));
        assert_eq!(poly[3], P(0.0, 100.0));
    }

    #[test]
    fn wall_polygon_is_counter_clockwise() {
        // The union in `boolean` requires interior-on-the-left rings, and
        // R3.2.4 canonicalizes exterior rings counter-clockwise. This test
        // previously asserted Clockwise under this same name, so the ring the
        // rest of the engine consumed wound the wrong way against its label.
        let poly = wall_polygon(P(0.0, 0.0), P(1000.0, 0.0), 200.0).expect("non-degenerate");
        assert_eq!(
            orientation(poly[0], poly[1], poly[2]),
            Orientation::CounterClockwise,
            "corners must wind counter-clockwise"
        );
    }

    #[test]
    fn a_zero_length_wall_has_no_polygon() {
        assert!(wall_polygon(P(5.0, 5.0), P(5.0, 5.0), 100.0).is_none());
    }

    /// REGRESSION (ED-0100). Both products underflow to zero, `det` is zero,
    /// and the sign shortcut used to return Collinear for a triangle whose
    /// real turn is counter-clockwise. The doc comment claimed exactness for
    /// every input while this input existed.
    #[test]
    fn a_triangle_of_subnormal_size_is_not_collinear() {
        let eta = f64::from_bits(1);
        assert_eq!(
            orientation(P(0.0, 0.0), P(eta, 0.0), P(0.0, eta)),
            Orientation::CounterClockwise
        );
        // Mirrored, so a fix that merely forced one branch would still fail.
        assert_eq!(
            orientation(P(0.0, 0.0), P(0.0, eta), P(eta, 0.0)),
            Orientation::Clockwise
        );
    }

    /// A genuine zero product — an axis-aligned wall — must NOT be diverted to
    /// the exact path. This is the common case and the whole point of the
    /// filter; the underflow guard tests the operands, not the product alone.
    #[test]
    fn an_axis_aligned_triple_still_takes_the_fast_path() {
        assert_eq!(
            orientation(P(0.0, 0.0), P(4000.0, 0.0), P(4000.0, 3000.0)),
            Orientation::CounterClockwise
        );
        assert_eq!(
            orientation(P(0.0, 0.0), P(4000.0, 0.0), P(8000.0, 0.0)),
            Orientation::Collinear
        );
    }

    #[test]
    fn the_exact_fallback_is_exact_where_rounded_subtraction_was_not() {
        // Adversarial review (Codex, gpt-5.6-sol, 2026-09-01) observed that the
        // fallback subtracted coordinates with ordinary rounding and
        // compensated only the PRODUCTS, so the subtraction's error was never
        // recovered — while the documentation claimed exactness for every
        // input.
        //
        // These two triples are well inside COORD_BOUND_MM and the predicate
        // returned the OPPOSITE sign from the true one. Both expectations were
        // computed with exact rational arithmetic, not by running this code.
        let cases = [
            (
                P(910936.0478429426, 204558.37792401644),
                P(-51697.073536482174, -769292.9677823646),
                P(523981.0953440073, -186906.0261667562),
                Orientation::CounterClockwise,
            ),
            (
                P(599287.4896993204, -830443.0270992398),
                P(321171.30040978827, 819554.2751034459),
                P(490776.6418678869, -186673.93678138123),
                Orientation::Clockwise,
            ),
        ];
        for (a, b, c, want) in cases {
            assert_eq!(orientation(a, b, c), want, "a={a:?} b={b:?} c={c:?}");
        }
    }

    #[test]
    fn orientation_agrees_with_an_exact_integer_oracle() {
        // Integer coordinates make an i128 determinant EXACT, so it is a true
        // oracle rather than another floating-point opinion. Most cases here
        // are collinear or one unit off it, which is where a filtered
        // predicate must fall back and be right.
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as i64 % 2_000_001 - 1_000_000
        };
        for _ in 0..20_000 {
            let (ax, ay) = (next(), next());
            let (bx, by) = (next(), next());
            let (dx, dy) = (bx - ax, by - ay);
            let t = next() % 5;
            let (mut cx, mut cy) = (ax + dx * t, ay + dy * t);
            cx += next() % 3;
            cy += next() % 3;

            let exact = (i128::from(bx - ax) * i128::from(cy - ay))
                - (i128::from(by - ay) * i128::from(cx - ax));
            let want = match exact.cmp(&0) {
                std::cmp::Ordering::Greater => Orientation::CounterClockwise,
                std::cmp::Ordering::Less => Orientation::Clockwise,
                std::cmp::Ordering::Equal => Orientation::Collinear,
            };
            let got = orientation(
                P(ax as f64, ay as f64),
                P(bx as f64, by as f64),
                P(cx as f64, cy as f64),
            );
            assert_eq!(
                got, want,
                "a=({ax},{ay}) b=({bx},{by}) c=({cx},{cy}) exact={exact}"
            );
        }
    }

    /// A deterministic xorshift, so a failure is reproducible from the seed
    /// alone and no dependency is added.
    fn next(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    /// A finite `f64` with a uniformly chosen biased exponent, so the sample
    /// spans subnormals, the whole normal range and the top of it — not just
    /// the neighbourhood of 1 that a naive `rand::<f64>()` would give.
    fn any_finite(state: &mut u64) -> f64 {
        let bits = next(state);
        let sign = bits & (1 << 63);
        let fraction = bits & 0x000f_ffff_ffff_ffff;
        // 0 is subnormal/zero; 0x7ff would be inf/NaN and is excluded.
        let biased = (next(state) % 0x7ff) << 52;
        f64::from_bits(sign | biased | fraction)
    }

    /// THE differential property: wherever the filter answers, it answers what
    /// the exact evaluation would.
    ///
    /// Not a fallback test — `orientation` is compared against
    /// `exact_det::orientation_sign` on every sample, so a filter that decided
    /// wrongly is caught whether or not it escalated. The sample spans the
    /// full binary64 exponent range on all six coordinates independently, so
    /// mixed subnormal-and-huge triples, overflowing products and total
    /// cancellation all occur.
    #[test]
    fn the_filter_never_disagrees_with_the_exact_sign() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for case in 0..50_000 {
            let (ax, ay) = (any_finite(&mut state), any_finite(&mut state));
            let (bx, by) = (any_finite(&mut state), any_finite(&mut state));
            let (cx, cy) = (any_finite(&mut state), any_finite(&mut state));
            let (a, b, c) = (P(ax, ay), P(bx, by), P(cx, cy));
            assert_eq!(
                orientation(a, b, c),
                exact(a, b, c),
                "case {case}: {a:?} {b:?} {c:?}"
            );
        }
    }

    /// The same, restricted to triples engineered to CANCEL: `c` is placed on
    /// or beside the line through `a` and `b`, so the two products are equal
    /// or nearly so and the filter is at its least decisive.
    #[test]
    fn the_filter_never_disagrees_under_deliberate_cancellation() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for case in 0..20_000 {
            let a = P(any_finite(&mut state), any_finite(&mut state));
            let b = P(any_finite(&mut state), any_finite(&mut state));
            // Exactly collinear when the arithmetic is exact, then perturbed
            // by one ulp in one coordinate a third of the time.
            let t = (next(&mut state) % 5) as f64;
            let mut c = P(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            match next(&mut state) % 3 {
                0 => {}
                1 => c = P(f64::from_bits(c.x.to_bits() ^ 1), c.y),
                _ => c = P(c.x, f64::from_bits(c.y.to_bits() ^ 1)),
            }
            if ![a.x, a.y, b.x, b.y, c.x, c.y].iter().all(|v| v.is_finite()) {
                continue;
            }
            assert_eq!(
                orientation(a, b, c),
                exact(a, b, c),
                "case {case}: {a:?} {b:?} {c:?}"
            );
        }
    }

    /// Swapping two points of the triple reverses the turn, exactly — for
    /// every input, including the ones the filter refuses to decide.
    #[test]
    fn the_sign_is_antisymmetric_across_the_whole_range() {
        let mut state = 0xD1B5_4A32_D192_ED03u64;
        for _ in 0..20_000 {
            let a = P(any_finite(&mut state), any_finite(&mut state));
            let b = P(any_finite(&mut state), any_finite(&mut state));
            let c = P(any_finite(&mut state), any_finite(&mut state));
            let forward = orientation(a, b, c);
            let reversed = match orientation(a, c, b) {
                Orientation::CounterClockwise => Orientation::Clockwise,
                Orientation::Clockwise => Orientation::CounterClockwise,
                Orientation::Collinear => Orientation::Collinear,
            };
            assert_eq!(forward, reversed, "{a:?} {b:?} {c:?}");
            // And a cyclic rotation leaves it unchanged.
            assert_eq!(forward, orientation(b, c, a));
            assert_eq!(forward, orientation(c, a, b));
        }
    }

    /// Catastrophic cancellation across the whole exponent range: the leading
    /// products cancel exactly and the answer lives entirely in the tail.
    ///
    /// A left-to-right floating-point sum gets the SIGN backwards here, which
    /// is the one error a geometric predicate must never make.
    #[test]
    fn the_sign_survives_catastrophic_cancellation() {
        // det = (1e300 - 0)(1 - 0) - (1 - 0)(1e300 - 0) + tail. Built so the
        // huge terms annihilate and a subnormal decides the outcome.
        let eta = f64::from_bits(1);
        assert_eq!(
            orientation(P(0.0, 0.0), P(1e300, 1e300), P(1e300, 1e300 + eta)),
            orientation(P(0.0, 0.0), P(1e300, 1e300), P(1e300, 1e300)),
            "adding a subnormal below the ulp of 1e300 changes nothing at all"
        );
        // Whereas one ulp of 1e300 IS representable and does change it.
        let up = f64::from_bits(1e300f64.to_bits() + 1);
        assert_eq!(
            orientation(P(0.0, 0.0), P(1e300, 1e300), P(1e300, up)),
            Orientation::CounterClockwise
        );
    }
}
