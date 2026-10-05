//! Positive-area overlap of ideal centered, butt-ended wall rectangles.
//!
//! The separating-axis test compares rectangle support intervals along each
//! axis direction and its normal. All four projection overlaps must be
//! strict. A shared edge or point therefore returns false.
//!
//! No offset vertex is constructed. In particular, this predicate does not
//! confuse the rounded polygon used for rendering with the ideal rectangle
//! specified by the stored endpoints and thickness (BEHAVIOR-SPEC R3.0.2).
//!
//! # Floating filter proof
//!
//! Every input starts as a singleton interval. Addition/subtraction are
//! monotone in their endpoints; multiplication takes the four endpoint
//! products; absolute value and squaring account for an interval crossing
//! zero. Each rounded endpoint is expanded by one representable value in
//! the outward direction. IEEE binary64 basic operations and square root
//! round the exact result, so this encloses it even at subnormal magnitudes.
//! Induction through the expression tree encloses each exact support bound.
//! A denominator whose interval reaches zero, or nonfinite intermediate,
//! yields an undecided interval, never a guessed comparison.
//!
//! Rust documents the square-root rounding guarantee at
//! <https://doc.rust-lang.org/std/primitive.f64.html#method.sqrt>
//! (API contract consulted 2026-09-06; no implementation source adapted).
//! An uncertain strict comparison escalates to independently implemented
//! exact dyadic polynomial comparisons, including exact boundary contact.

use domowik_format::{Point, COORD_BOUND_MM, DIMENSION_CAP_MM};

/// Whether two centered butt rectangles overlap with strictly positive area.
///
/// Coordinates and thicknesses are millimetres. Endpoints must be finite,
/// bounded by `COORD_BOUND_MM`, and distinct within each axis. Thicknesses
/// must be finite and in `(0, DIMENSION_CAP_MM]`. Invalid input returns `None`.
/// No tolerance, clipping grid, minimum length or minimum area is used.
///
/// The result is exact for the ideal rectangles over stored binary64 inputs,
/// invariant under swapping the rectangles or reversing either axis. Level
/// filtering and axis-contact exclusions belong to the caller: this function
/// also accepts coincident or crossing axes. It has no mutation or cache.
/// The floating path allocates nothing; uncertain cases use exact arithmetic.
pub fn rectangles_overlap(
    a0: Point,
    a1: Point,
    ta: f64,
    b0: Point,
    b1: Point,
    tb: f64,
) -> Option<bool> {
    if ![a0, a1, b0, b1].into_iter().all(|p| {
        p.x.is_finite()
            && p.y.is_finite()
            && p.x.abs() <= COORD_BOUND_MM
            && p.y.abs() <= COORD_BOUND_MM
    }) || ![ta, tb]
        .into_iter()
        .all(|t| t.is_finite() && t > 0.0 && t <= DIMENSION_CAP_MM)
        || a0 == a1
        || b0 == b1
    {
        return None;
    }
    Some(
        filtered(a0, a1, ta, b0, b1, tb)
            .unwrap_or_else(|| crate::wall_overlap_exact::overlap(a0, a1, ta, b0, b1, tb)),
    )
}

#[derive(Clone, Copy, Debug)]
struct Interval {
    lo: f64,
    hi: f64,
}

impl Interval {
    const WHOLE: Self = Self {
        lo: f64::NEG_INFINITY,
        hi: f64::INFINITY,
    };

    fn point(x: f64) -> Self {
        Self { lo: x, hi: x }
    }

    fn outward(lo: f64, hi: f64) -> Self {
        if !lo.is_finite() || !hi.is_finite() {
            return Self::WHOLE;
        }
        Self {
            lo: lo.next_down(),
            hi: hi.next_up(),
        }
    }

    fn add(self, rhs: Self) -> Self {
        Self::outward(self.lo + rhs.lo, self.hi + rhs.hi)
    }

    fn sub(self, rhs: Self) -> Self {
        Self::outward(self.lo - rhs.hi, self.hi - rhs.lo)
    }

    fn mul(self, rhs: Self) -> Self {
        let products = [
            self.lo * rhs.lo,
            self.lo * rhs.hi,
            self.hi * rhs.lo,
            self.hi * rhs.hi,
        ];
        if products.iter().any(|p| !p.is_finite()) {
            return Self::WHOLE;
        }
        Self::outward(
            products.into_iter().fold(f64::INFINITY, f64::min),
            products.into_iter().fold(f64::NEG_INFINITY, f64::max),
        )
    }

    fn abs(self) -> Self {
        if self.lo >= 0.0 {
            self
        } else if self.hi <= 0.0 {
            Self {
                lo: -self.hi,
                hi: -self.lo,
            }
        } else {
            Self {
                lo: 0.0,
                hi: (-self.lo).max(self.hi),
            }
        }
    }

    fn square(self) -> Self {
        let absolute = self.abs();
        let mut result = Self::outward(absolute.lo * absolute.lo, absolute.hi * absolute.hi);
        result.lo = result.lo.max(0.0);
        result
    }

    fn sqrt(self) -> Self {
        if self.lo < 0.0 {
            return Self::WHOLE;
        }
        let mut result = Self::outward(self.lo.sqrt(), self.hi.sqrt());
        result.lo = result.lo.max(0.0);
        result
    }

    fn div_positive(self, rhs: Self) -> Self {
        if rhs.lo <= 0.0 {
            return Self::WHOLE;
        }
        self.mul(Self::outward(1.0 / rhs.hi, 1.0 / rhs.lo))
    }

    fn strictly_less(self, rhs: Self) -> Option<bool> {
        if self.hi < rhs.lo {
            Some(true)
        } else if self.lo >= rhs.hi {
            Some(false)
        } else {
            None
        }
    }
}

type Vector = [Interval; 2];

fn vector(a: Point, b: Point) -> Vector {
    [
        Interval::point(b.x).sub(Interval::point(a.x)),
        Interval::point(b.y).sub(Interval::point(a.y)),
    ]
}

fn dot(a: Vector, b: Vector) -> Interval {
    a[0].mul(b[0]).add(a[1].mul(b[1]))
}

fn cross(a: Vector, b: Vector) -> Interval {
    a[0].mul(b[1]).sub(a[1].mul(b[0]))
}

fn norm_squared(a: Vector) -> Interval {
    let mut result = a[0].square().add(a[1].square());
    result.lo = result.lo.max(0.0);
    result
}

fn filtered(a0: Point, a1: Point, ta: f64, b0: Point, b1: Point, tb: f64) -> Option<bool> {
    let da = vector(a0, a1);
    let db = vector(b0, b1);
    // Twice the difference of rectangle centres; no division by two and no
    // rounded centre construction precedes the interval arithmetic.
    let s0 = vector(a0, b0);
    let s1 = vector(a1, b1);
    let s = [s0[0].add(s1[0]), s0[1].add(s1[1])];
    let ma = norm_squared(da);
    let mb = norm_squared(db);
    let lengths = [ma.sqrt(), mb.sqrt()];
    let d = dot(da, db).abs();
    let c = cross(da, db).abs();
    let mut uncertain = false;
    for (axis, own_m, own_length, other_length, own_t, other_t) in [
        (da, ma, lengths[0], lengths[1], ta, tb),
        (db, mb, lengths[1], lengths[0], tb, ta),
    ] {
        let own_t = Interval::point(own_t);
        let other_t = Interval::point(other_t);
        let along_support = own_m.add(d).add(other_t.mul(c).div_positive(other_length));
        let normal_support = own_t
            .mul(own_length)
            .add(c)
            .add(other_t.mul(d).div_positive(other_length));
        for decision in [
            dot(axis, s).abs().strictly_less(along_support),
            cross(axis, s).abs().strictly_less(normal_support),
        ] {
            match decision {
                Some(false) => return Some(false),
                Some(true) => {}
                None => uncertain = true,
            }
        }
    }
    (!uncertain).then_some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_separated_and_overlapping_pairs_are_filtered() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(2000.0, 0.0);
        assert_eq!(
            filtered(
                a,
                b,
                200.0,
                Point::new(0.0, 500.0),
                Point::new(2000.0, 500.0),
                200.0
            ),
            Some(false)
        );
        assert_eq!(
            filtered(
                a,
                b,
                200.0,
                Point::new(0.0, 150.0),
                Point::new(2000.0, 150.0),
                200.0
            ),
            Some(true)
        );
        assert_eq!(
            filtered(
                a,
                b,
                200.0,
                Point::new(850.0, 900.0),
                Point::new(3250.0, -900.0),
                200.0
            ),
            Some(true)
        );
    }

    #[test]
    fn exact_touching_is_not_guessed_by_the_filter() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(2000.0, 0.0);
        let c = Point::new(0.0, 200.0);
        let d = Point::new(2000.0, 200.0);
        assert_eq!(filtered(a, b, 200.0, c, d, 200.0), None);
        assert_eq!(rectangles_overlap(a, b, 200.0, c, d, 200.0), Some(false));
    }

    #[test]
    fn invalid_geometry_is_not_a_disjoint_result() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(1.0, 0.0);
        for t in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            DIMENSION_CAP_MM.next_up(),
        ] {
            assert_eq!(rectangles_overlap(a, b, t, a, b, 1.0), None);
        }
        assert_eq!(rectangles_overlap(a, a, 1.0, a, b, 1.0), None);
        for x in [f64::NAN, f64::INFINITY, COORD_BOUND_MM.next_up()] {
            assert_eq!(
                rectangles_overlap(Point::new(x, 0.0), b, 1.0, a, b, 1.0),
                None
            );
        }
    }

    #[test]
    fn interval_extremes_escalate_or_enclose_without_nan_decisions() {
        let eta = f64::from_bits(1);
        let product = Interval::point(eta).mul(Interval::point(eta));
        assert!(product.lo <= 0.0 && product.hi >= eta);
        assert_eq!(
            Interval::WHOLE
                .mul(Interval::point(0.0))
                .strictly_less(Interval::point(0.0)),
            None
        );
        assert_eq!(
            Interval::point(1.0)
                .div_positive(Interval::point(0.0))
                .strictly_less(Interval::point(0.0)),
            None
        );
        assert_eq!(
            Interval::point(f64::MAX)
                .square()
                .strictly_less(Interval::point(1.0)),
            None
        );
    }

    // ---- Benchmark-fixture alignment ----
    //
    // The benchmark labels a case "filtered" or "exact" to say which path it
    // is timing. These assert that the filter really takes the path the label
    // claims, so a label cannot quietly become a lie about what was measured.
    // If one of these fails, the FILTER is right and the label is wrong.

    /// The spine every fixture below is measured against.
    const SPINE: (Point, Point, f64) = (Point::new(0.0, 0.0), Point::new(2000.0, 0.0), 200.0);

    fn filter(b0: Point, b1: Point, tb: f64) -> Option<bool> {
        filtered(SPINE.0, SPINE.1, SPINE.2, b0, b1, tb)
    }

    fn public(b0: Point, b1: Point, tb: f64) -> Option<bool> {
        rectangles_overlap(SPINE.0, SPINE.1, SPINE.2, b0, b1, tb)
    }

    #[test]
    fn clearly_separated_and_clearly_overlapping_pairs_are_decided_by_the_filter() {
        // 900 mm of clearance: no interval can straddle a separating axis.
        assert_eq!(
            filter(Point::new(0.0, 1000.0), Point::new(2000.0, 1000.0), 200.0),
            Some(false),
            "y = 1000 is clear"
        );
        // Half the thickness of interpenetration.
        assert_eq!(
            filter(Point::new(0.0, 100.0), Point::new(2000.0, 100.0), 200.0),
            Some(true),
            "y = 100 overlaps over a 100 mm band"
        );
        // R3.1.2's own non-parallel illustration: axes meeting only outside
        // both segments, bands overlapping near the cap.
        assert_eq!(
            filter(Point::new(850.0, 900.0), Point::new(3250.0, -900.0), 200.0),
            Some(true),
            "the specification's band-overlap example"
        );
    }

    #[test]
    fn boundary_pairs_escalate_rather_than_being_guessed() {
        // Exactly touching at y = 100. The separating axis is achieved with
        // equality, which no outward-rounded interval can resolve.
        assert_eq!(
            filter(Point::new(0.0, 200.0), Point::new(2000.0, 200.0), 200.0),
            None,
            "edge-to-edge contact is a tie the filter must not decide"
        );
        assert_eq!(
            public(Point::new(0.0, 200.0), Point::new(2000.0, 200.0), 200.0),
            Some(false),
            "and the exact path says touching is not overlap"
        );

        // One ulp of clearance: still inside the interval's own widening.
        let apart = f64::from_bits(200f64.to_bits() + 1);
        assert_eq!(
            filter(Point::new(0.0, apart), Point::new(2000.0, apart), 200.0),
            None,
            "one ulp of clearance is below the filter's resolution"
        );
        assert_eq!(
            public(Point::new(0.0, apart), Point::new(2000.0, apart), 200.0),
            Some(false)
        );
    }

    #[test]
    fn a_mixed_subnormal_pair_escalates_and_the_exact_path_answers() {
        // Both axes are a unit long but rise or fall by one subnormal, and
        // both thicknesses are that same subnormal. The axes cross at the
        // midpoint, so the rectangles certainly share area — but every
        // quantity the filter forms underflows.
        let eta = f64::from_bits(1);
        let a0 = Point::new(0.0, 0.0);
        let a1 = Point::new(1.0, eta);
        let b0 = Point::new(0.0, eta);
        let b1 = Point::new(1.0, 0.0);
        assert_eq!(
            filtered(a0, a1, eta, b0, b1, eta),
            None,
            "subnormal denominators leave the filter no resolution"
        );
        assert_eq!(
            rectangles_overlap(a0, a1, eta, b0, b1, eta),
            Some(true),
            "the axes cross, so the rectangles share area"
        );
    }
}
