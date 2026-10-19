//! Exact polygon area, and the exact display rounding of BEHAVIOR-SPEC R5.3.2.
//!
//! R5.3.1 states a face's area as its exact real measure in mm², and R5.3.2
//! rounds THAT value to two decimals with ties to even. Doing the rounding on
//! an ordinary `f64` area gets both of the specification's own tie examples
//! wrong, in opposite directions: the nearest double to 30.245 lies above the
//! tie and the nearest double to 10.655 lies below it, so half-even applied to
//! the double answers `30.25` and `10.65` where the rule requires `30.24` and
//! `10.66`. This module removes the question by never forming a float on the
//! path that decides.
//!
//! # Why the exact value is exact
//!
//! A ring's doubled signed area is `Σ (xᵢ·yᵢ₊₁ − xᵢ₊₁·yᵢ)`: a sum of products
//! of two stored coordinates. Every finite binary64 is exactly `±m · 2^e`, so
//! every product is a dyadic rational and so is their sum
//! ([`crate::dyadic`]). Halving moves the binary point. Nothing rounds.
//!
//! # Why the STORED coordinates, not their differences
//!
//! [`crate::polygon::signed_double_area`] translates to the first vertex
//! before summing, which is a real fix for the FLOATING-POINT path — it stops
//! accuracy depending on how far from the origin the plan was drawn. It must
//! not appear on the exact path, because `a.x − origin.x` is itself a rounding
//! step, so translating would compute the exact area of a slightly different
//! polygon. In exact arithmetic the cancellation the translation defends
//! against cannot occur, so the defence is not needed. This is the argument
//! [`crate::exact_det`] already records for reading stored coordinates rather
//! than their differences.
//!
//! # Cost, and the filter that usually avoids it
//!
//! [`ExactArea`] is an EAGER, HEAP-ALLOCATING value: every term allocates
//! limbs and the sum allocates again. It is not free and is not to be used as
//! a general replacement for [`Polygon::area_mm2`].
//!
//! [`display_hundredths_m2`] is the entry point a per-frame or per-projection
//! caller should use. It evaluates the float area with a proven error bound
//! and escalates to the exact path ONLY when the float value lies within that
//! bound of a rounding tie. For ordinary room geometry the bound is on the
//! order of 10⁻⁷ mm² against a tie spacing of 5,000 mm², so the exact path is
//! reached essentially only by inputs that really are on a tie — which is
//! exactly the case the specification puts in its fixtures.
//!
//! # Preconditions, stated because the shoelace does not check them
//!
//! - **Finite coordinates.** A non-finite coordinate has no dyadic form. Every
//!   constructor returns `None` rather than inventing one.
//! - **Simple rings — NOT established by canonicalization.** The shoelace of a
//!   SELF-INTERSECTING ring is the algebraic sum of its signed lobes, not the
//!   area of the point set it traces. [`Polygon::canonicalize`] does not make
//!   a ring simple and does not check that it is: it drops duplicate and
//!   collinear vertices, fixes winding, discards a ring of zero area and sorts
//!   holes. That is NORMALIZATION, not validation. Nothing in this crate
//!   rejects a self-intersecting ring, so a caller that can produce one owns
//!   the consequence; this module computes what it is given.
//! - **Contained, disjoint holes — likewise a caller obligation.**
//!   [`Polygon::exact_area_mm2`] subtracts each hole's absolute area, which is
//!   R5.3.1's "a holed face subtracts hole areas" and is meaningful only for
//!   holes actually inside the exterior ring and disjoint from each other.
//!   `canonicalize` does not establish either property. The union DOES assign
//!   holes by containment, so its output satisfies this; a hand-built
//!   `Polygon` need not, and a hole outside the exterior ring yields a smaller
//!   number rather than an error.
//! - **Degenerate rings.** Fewer than three vertices encloses nothing and
//!   gives exactly zero. So does any ring whose vertices are collinear; that
//!   is the correct area, not a failure.
//!
//! # What this does NOT make exact
//!
//! The area of the ring GIVEN. Constructed vertices — intersection points and
//! `lerp` results — remain ordinary binary64 under ED-0103 N-3, so an exact
//! area over an approximately constructed ring is still an approximation of
//! the true region. Conformance here is about the arithmetic, not about the
//! geometry that fed it.

use crate::dyadic::Dyadic;
use crate::polygon::{signed_double_area, Polygon};
use domowik_format::Point;
use std::collections::HashSet;

/// An exact area in square millimetres.
///
/// Signed, because the difference of two areas is the primitive R5.1.9 needs
/// (ED-0103 N-5); a ring's own area is never negative.
///
/// # Not ordered, because nothing needs it
///
/// There is no `Ord`. What comparison is actually wanted here is the sign of a
/// DIFFERENCE, which [`ExactArea::is_positive`] answers, and no consumer
/// orders two areas against each other. The surface stays minimal until one
/// does.
///
/// This is not a prohibition: an ordering on a numeric type would not by
/// itself prescribe an ordering on faces. Faces are ordered by the R5.1.3
/// canonical ring comparison, and that stays true whatever this type derives —
/// so if a consumer ever needs `Ord`, add it, and keep it away from face
/// ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactArea(Dyadic);

impl ExactArea {
    /// Exactly zero.
    ///
    /// A true zero, not a signed one. Summing an empty face list yields this
    /// and displays `0.00`. Folding `f64` areas is not equivalent: `Sum` for
    /// `f64` starts at NEGATIVE zero, which renders as `-0.00` and is
    /// forbidden by R5.3.2.
    pub fn zero() -> Self {
        Self(Dyadic::zero())
    }

    /// Exact sum.
    pub fn add(&self, other: &Self) -> Self {
        Self(self.0.add(&other.0))
    }

    /// Exact difference.
    pub fn sub(&self, other: &Self) -> Self {
        Self(self.0.sub(&other.0))
    }

    /// Exactly zero?
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Strictly greater than zero? This is the R5.1.9 positive-area test.
    pub fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    /// Strictly less than zero?
    pub fn is_negative(&self) -> bool {
        self.0.is_negative()
    }

    /// The R5.3.2 display value, as a count of hundredths of a square metre.
    ///
    /// Converts to m² by exact division by 1,000,000 and rounds to two
    /// decimals, resolving exact halves to the EVEN last digit. The caller
    /// renders the integer, so R5.3.2's arithmetic exists in one place and its
    /// presentation in another.
    ///
    /// Rounding to two decimals of m² is rounding to the nearest integer
    /// number of hundredths, which is `area / 10⁴` — an exact ratio of
    /// integers, decided by comparing twice the remainder against the divisor.
    /// A tie is DETECTED, never approached.
    ///
    /// # Overflow is reported, never silent
    ///
    /// `None` when the value does not fit an `i64` count of hundredths. A
    /// finite coordinate may be as large as `f64::MAX`, so this is reachable
    /// from input that never passed document validation. The bound that
    /// matters for real geometry is `DERIVED_COORD_BOUND_MM`, not
    /// `COORD_BOUND_MM` — a wall band offsets past the stored centre-line
    /// bound — so the widest derived extent is 2.2×10⁷ mm and the largest
    /// derived area about 4.84×10¹⁴ mm², which is 4.84×10¹⁰ hundredths and
    /// nowhere near the limit. Nothing saturates and nothing panics.
    pub fn hundredths_m2_half_even(&self) -> Option<i64> {
        self.0.round_half_even_div_pow10(4)
    }
}

impl std::iter::Sum for ExactArea {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::zero(), |acc, a| acc.add(&a))
    }
}

impl<'a> std::iter::Sum<&'a ExactArea> for ExactArea {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.fold(Self::zero(), |acc, a| acc.add(a))
    }
}

/// Twice the exact signed area of `ring`. `None` if any coordinate is not
/// finite.
fn signed_double_area_exact(ring: &[Point]) -> Option<Dyadic> {
    if ring.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    if ring.len() < 3 {
        return Some(Dyadic::zero());
    }
    let mut total = Dyadic::zero();
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let term = Dyadic::from_f64(a.x)
            .mul(&Dyadic::from_f64(b.y))
            .sub(&Dyadic::from_f64(b.x).mul(&Dyadic::from_f64(a.y)));
        total = total.add(&term);
    }
    Some(total)
}

/// The exact signed area of a simple ring, in mm². Positive counter-clockwise.
///
/// `None` when a coordinate is not finite. See the module preconditions.
pub fn signed_ring_area_mm2(ring: &[Point]) -> Option<ExactArea> {
    signed_double_area_exact(ring).map(|d| ExactArea(d.halve()))
}

/// The exact area of a simple ring, in mm², regardless of winding.
///
/// `None` when a coordinate is not finite. See the module preconditions.
pub fn ring_area_mm2(ring: &[Point]) -> Option<ExactArea> {
    signed_double_area_exact(ring).map(|d| ExactArea(d.halve().abs()))
}

/// A proven upper bound on the error of the FLOAT SHOELACE of one ring, in mm².
///
/// The float path forms `n` translated differences, `2n` products and a sum of
/// `n` terms. Each stage rounds, so the standard forward bound for a sum of
/// products applies (Higham, *Accuracy and Stability of Numerical Algorithms*,
/// 2nd ed., §3.1): the computed value differs from the exact one by at most
/// `γ_k · Σ|terms|` with `γ_k = k·u / (1 − k·u)` and `u = 2⁻⁵³`. Taking
/// `k = n + 8` covers the translation's own rounding with room to spare, and
/// `Σ|terms| ≤ 2·n·M²` where `M` bounds the magnitude of any translated
/// coordinate.
///
/// This covers ONE ring's shoelace and nothing else. The roundings that
/// combine rings — halving, hole subtraction, accumulation, the final division
/// — are bounded separately by [`combination_error_bound`], because they act
/// on different magnitudes and counting them here would be wrong.
///
/// `None` when a coordinate is not finite, or when the bound is not finite.
/// The finiteness test comes FIRST: a ring of fewer than three vertices
/// encloses nothing, but that does not make a `NaN` in it acceptable, and
/// returning a zero bound for one would let the filter certify a value the
/// exact path refuses.
fn float_area_error_bound(ring: &[Point]) -> Option<f64> {
    if ring.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    const U: f64 = f64::EPSILON / 2.0;
    let n = ring.len();
    if n < 3 {
        // Exactly zero area, exactly computed: no rounding happens at all.
        return Some(0.0);
    }
    let origin = ring[0];
    let mut max_extent: f64 = 0.0;
    for p in ring {
        max_extent = max_extent
            .max((p.x - origin.x).abs())
            .max((p.y - origin.y).abs());
    }
    let k = (n as f64) + 8.0;
    let denominator = 1.0 - k * U;
    if denominator <= 0.0 {
        return None;
    }
    let gamma = k * U / denominator;
    // Half the doubled sum, so half the bound; the 1.01 absorbs the rounding
    // of this bound's own arithmetic.
    let bound = gamma * (n as f64) * max_extent * max_extent * 1.01;
    bound.is_finite().then_some(bound)
}

/// A proven upper bound on the roundings that COMBINE ring areas, in mm².
///
/// [`Polygon::area_mm2`] halves the outer shoelace (1 rounding), halves each of
/// `h` hole shoelaces (`h`), sums them (`h − 1` when `h > 0`), and subtracts
/// (1). [`display_hundredths_m2`] then divides by 10,000 (1). Every one of
/// those acts on an intermediate no larger than `T`, the sum of the rings'
/// absolute areas, so each contributes at most `u·T` and the total is at most
/// `(2h + 2)·u·T`.
///
/// Counting these was missing from the first version of this module, which
/// bounded only the per-ring shoelace. The filter could therefore certify a
/// value whose error came from a step the bound did not describe.
fn combination_error_bound(polygon: &Polygon) -> Option<f64> {
    const U: f64 = f64::EPSILON / 2.0;
    let gross = signed_double_area(&polygon.outer).abs() / 2.0;
    let mut total = gross;
    for hole in &polygon.holes {
        total += signed_double_area(hole).abs() / 2.0;
    }
    let h = polygon.holes.len() as f64;
    let bound = (2.0 * h + 2.0) * U * total * 1.01;
    bound.is_finite().then_some(bound)
}

/// Every error bound for one polygon's float area, or `None` if any is
/// unavailable.
fn polygon_error_bound(polygon: &Polygon) -> Option<f64> {
    let mut bound = float_area_error_bound(&polygon.outer)?;
    for hole in &polygon.holes {
        bound += float_area_error_bound(hole)?;
    }
    bound += combination_error_bound(polygon)?;
    bound.is_finite().then_some(bound)
}

/// The R5.3.2 display value for a polygon, filtered.
///
/// This is the entry point for callers that render areas. It computes the
/// float area first and returns immediately when that value is provably far
/// enough from a rounding tie for the float answer to be the exact one;
/// otherwise it escalates and builds the exact area. No [`ExactArea`] is
/// constructed on the fast path.
///
/// `None` on non-finite coordinates anywhere in the polygon — including in a
/// ring too short to enclose anything, and including a hole — or on a value
/// too large for an `i64` count of hundredths.
pub fn display_hundredths_m2(polygon: &Polygon) -> Option<i64> {
    if let Some(certified) = filtered_hundredths(polygon) {
        return Some(certified);
    }
    polygon.exact_area_mm2()?.hundredths_m2_half_even()
}

/// The filter behind [`display_hundredths_m2`]: `Some` only when the float
/// answer is CERTIFIED correct, by a bound that covers every rounding on the
/// path from ring coordinates to a hundredths value.
///
/// Unlike [`positive_area`], this decision has no ill-conditioned step to
/// defeat it: the only division is by the constant 10,000, and the shoelace
/// and combination bounds account for the rest. That is why a filter is sound
/// here and is not sound there.
///
/// Every way of failing to prove the answer returns `None`, which escalates. A
/// zero bound certifies only when no ring encloses anything, where the float
/// area is exactly zero and exactly right.
fn filtered_hundredths(polygon: &Polygon) -> Option<i64> {
    let bound = polygon_error_bound(polygon)?;
    if !bound.is_finite() || bound < 0.0 {
        return None;
    }
    let encloses_something = std::iter::once(&polygon.outer)
        .chain(polygon.holes.iter())
        .any(|r| r.len() >= 3);
    if bound == 0.0 && encloses_something {
        // A ring with area but a zero bound is a bound that underflowed, not
        // a proof.
        return None;
    }

    let area = polygon.area_mm2();
    if !area.is_finite() {
        return None;
    }
    // Hundredths of a square metre are units of 10,000 mm².
    let hundredths = area / 10_000.0;
    if !hundredths.is_finite() || hundredths.abs() >= (i64::MAX / 2) as f64 {
        return None;
    }
    let nearest = hundredths.round_ties_even();
    // Distance from the value to the tie separating `nearest` from its
    // neighbour, expressed back in mm² so it compares against the bound.
    let distance_to_tie = ((hundredths - nearest).abs() - 0.5).abs() * 10_000.0;
    (distance_to_tie > bound).then_some(nearest as i64)
}

/// Whether `left − right` is positive, when that question can be answered.
///
/// R5.1.9 links components whose occupied regions "intersect with POSITIVE
/// AREA", and the overlap is `area(A) + area(B) − area(A ∪ B)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositiveArea {
    /// Exactly positive, over rings whose every vertex came from the input.
    Yes,
    /// Exactly zero or negative, over rings whose every vertex came from the
    /// input. Regions that merely touch land here, and they land here
    /// EXACTLY: the shoelace sums cancel term for term.
    No,
    /// `right` contains a vertex that is not a vertex of `left`, so it was
    /// CONSTRUCTED, and the displacement of a constructed point has no bound
    /// this function can supply. The arithmetic is certain; the geometry it
    /// was handed is not.
    Uncertain,
}

/// The R5.1.9 positive-area test, exact where it is provable and explicit
/// where it is not.
///
/// # There is no tolerance floor and no epsilon
///
/// Nothing rounds a small overlap down to zero. `OVERLAP_RELATIVE`, the
/// retired `1e-12 × separate` term, did exactly that: at a separate area of
/// 10¹² mm² it discarded any overlap under 1 mm² (ED-0103 N-5).
///
/// # Why the answer is a vertex test and not an error bound
///
/// An earlier version certified `Yes`/`No` whenever the exact difference
/// exceeded `64·n·u·M²`, justified by counting the roundings in an
/// intersection. **That justification was wrong**, and the count is the reason
/// it was wrong: a rounded-operation count bounds each operation's RELATIVE
/// error and says nothing about a division by a near-zero denominator. A cut
/// is `lerp(a, b, t)` with `t = num/denom` and `denom = |r||s|·sin θ`, so the
/// displacement is bounded by `|r|·6u/|sin θ|` and grows without limit as two
/// segments approach parallel (derivation and measurements from issue 0058,
/// measured over 60,000 randomized pairs at controlled crossing angle: worst
/// displacement 1.05×10⁻⁹ mm at `sin θ = 10⁻¹`, 1.18×10⁻⁵ at `10⁻⁵`,
/// 1.10×10⁻³ at `10⁻⁷` — the topology quantum — and 1.14×10⁻¹ mm at `10⁻⁹`). It reaches
/// the quantum around `sin θ = 10⁻⁷` and passes a hundred times it by `10⁻⁹`,
/// so no constant band covers it.
///
/// Nor is the sign safe by argument. "A crossing implies a real overlap, so
/// error perturbs a positive number rather than flipping a zero" is false in
/// the regime that matters: the near-parallel case with the LARGEST
/// construction error is the thin-sliver case with the SMALLEST true overlap,
/// so the two are correlated and the comfort is thinnest exactly where the
/// error is biggest (issue 0058, correcting this module).
///
/// What this function tests instead: whether every vertex of `right` is a
/// vertex of `left`. If one is not, a COORDINATE was constructed, its
/// displacement is unbounded, and no verdict is available.
///
/// # What that test does NOT prove
///
/// It proves only that no coordinate was constructed. It does NOT prove that
/// `right` is the true union of `left`: a boolean routine can assemble rings
/// out of nothing but input vertices and still get the TOPOLOGY wrong —
/// dropping a fragment, mis-classifying one as interior, or tracing a cycle
/// the wrong way. This function would then compute an exact sign over wrong
/// rings and report it as certain.
///
/// So `Yes` and `No` mean: the arithmetic is exact AND no coordinate was
/// invented. They rest on the caller's boolean routine being topologically
/// correct, which is the caller's obligation and is not established here.
/// Closing R5.1.9 exactly needs a predicate that never consults a union at
/// all — see the proposal recorded against issue 0030.
///
/// # What this costs, and why it is still the right answer for R5.1.9
///
/// Two components that merely TOUCH construct nothing, so they return `No`
/// exactly and are correctly not linked. Two that genuinely OVERLAP construct
/// their crossing points, so they return `Uncertain` — and
/// [`crate::locality::link`] links on `Uncertain`, which is the correct
/// outcome for an overlapping pair. The verdict is therefore right in both
/// cases; what `Uncertain` gives up is the ability to say WHY, not the answer.
///
/// `None` when a coordinate is not finite.
pub fn positive_area(left: &[Polygon], right: &[Polygon]) -> Option<PositiveArea> {
    let mut supplied: HashSet<(u64, u64)> = HashSet::new();
    for polygon in left {
        for ring in std::iter::once(&polygon.outer).chain(polygon.holes.iter()) {
            for v in ring {
                if !v.x.is_finite() || !v.y.is_finite() {
                    return None;
                }
                supplied.insert((v.x.to_bits(), v.y.to_bits()));
            }
        }
    }

    let mut constructed = false;
    for polygon in right {
        for ring in std::iter::once(&polygon.outer).chain(polygon.holes.iter()) {
            for v in ring {
                if !v.x.is_finite() || !v.y.is_finite() {
                    return None;
                }
                // Negative zero and positive zero are the same point but not
                // the same bits; compare both spellings before deciding a
                // vertex is new.
                let bits = (v.x.to_bits(), v.y.to_bits());
                let zeroed = ((v.x + 0.0).to_bits(), (v.y + 0.0).to_bits());
                if !supplied.contains(&bits) && !supplied.contains(&zeroed) {
                    constructed = true;
                }
            }
        }
    }

    if constructed {
        return Some(PositiveArea::Uncertain);
    }

    let mut exact = ExactArea::zero();
    for p in left {
        exact = exact.add(&p.exact_area_mm2()?);
    }
    for p in right {
        exact = exact.sub(&p.exact_area_mm2()?);
    }
    Some(if exact.is_positive() {
        PositiveArea::Yes
    } else {
        PositiveArea::No
    })
}

impl Polygon {
    /// The exact area enclosed by the exterior ring, less every hole, in mm².
    ///
    /// This is R5.3.1's exact measure for a canonical face. `None` when any
    /// coordinate is not finite.
    ///
    /// Unlike [`Polygon::area_mm2`], no step rounds: holes are subtracted
    /// exactly rather than halved and summed in binary64. It ALLOCATES; see
    /// the module docs before reaching for it in a hot path.
    pub fn exact_area_mm2(&self) -> Option<ExactArea> {
        let mut area = ring_area_mm2(&self.outer)?;
        for hole in &self.holes {
            area = area.sub(&ring_area_mm2(hole)?);
        }
        Some(area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::COORD_BOUND_MM;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon {
        Polygon::new(vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ])
    }

    /// Areas in mm² that BEHAVIOR-SPEC R5.3.2 names, with the display it
    /// requires. The last three come from the corrected F27/F39 (ED-0103).
    const SPEC_CASES: [(f64, i64); 7] = [
        (10_640_000.0, 1064),
        (38_200_000.0, 3820),
        (30_245_000.0, 3024), // exact tie, 4 is even
        (10_655_000.0, 1066), // exact tie, 6 is even
        (38_220_000.0, 3822),
        (37_260_000.0, 3726),
        (42_120_000.0, 4212),
    ];

    /// An exact-integer rectangle with the given area, so the fixture value is
    /// the ring's real measure rather than something approached.
    fn rect_of_area(area_mm2: f64) -> Polygon {
        rect(0.0, 0.0, area_mm2 / 1000.0, 1000.0)
    }

    #[test]
    fn spec_display_values_hold_on_the_exact_path() {
        for (area, want) in SPEC_CASES {
            let polygon = rect_of_area(area);
            assert_eq!(polygon.area_mm2(), area, "fixture builds the stated area");
            assert_eq!(
                polygon.exact_area_mm2().unwrap().hundredths_m2_half_even(),
                Some(want),
                "exact display for {area} mm²"
            );
        }
    }

    #[test]
    fn spec_display_values_hold_through_the_filter() {
        for (area, want) in SPEC_CASES {
            assert_eq!(
                display_hundredths_m2(&rect_of_area(area)),
                Some(want),
                "filtered display for {area} mm²"
            );
        }
    }

    /// The defect ED-0103 N-4 records. Rounding the STORED DOUBLE disagrees
    /// with rounding the exact value on both of R5.3.2's ties, in opposite
    /// directions — which is why the fix is not "use round_ties_even".
    #[test]
    fn rounding_the_double_disagrees_with_the_exact_rule_on_both_ties() {
        let via_double = |mm2: f64| -> String { format!("{:.2}", mm2 / 1_000_000.0) };
        assert_eq!(via_double(30_245_000.0), "30.25");
        assert_eq!(via_double(10_655_000.0), "10.65");

        let exact = |mm2: f64| display_hundredths_m2(&rect_of_area(mm2)).unwrap();
        assert_eq!(exact(30_245_000.0), 3024, "R5.3.2 requires 30.24");
        assert_eq!(exact(10_655_000.0), 1066, "R5.3.2 requires 10.66");
    }

    /// ED-0100's second defect: a 2 µm square at the coordinate bound had
    /// exactly zero float area before translation. The exact path never needed
    /// the translation and reports the real measure.
    #[test]
    fn a_two_micrometre_square_at_the_bound_has_its_real_area() {
        let b = COORD_BOUND_MM;
        let side = 0.002;
        let square = rect(b - side, b - side, b, b);
        let exact = square.exact_area_mm2().unwrap();
        assert!(exact.is_positive(), "a real feature has positive area");
        // 0.002 x 0.002 = 4e-6 mm², far below a hundredth of a square metre.
        assert_eq!(exact.hundredths_m2_half_even(), Some(0));
        assert_eq!(display_hundredths_m2(&square), Some(0));
    }

    #[test]
    fn subnormal_coordinates_have_an_exact_positive_area() {
        let eta = f64::from_bits(1);
        let triangle = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(eta, 0.0),
            Point::new(0.0, eta),
        ]);
        let exact = triangle.exact_area_mm2().unwrap();
        assert!(exact.is_positive(), "η²/2 > 0 however small");
        assert!(!exact.is_zero());
    }

    #[test]
    fn the_value_does_not_depend_on_where_the_ring_starts_or_which_way_it_winds() {
        let base = [
            Point::new(3.5, -2.25),
            Point::new(1e6 + 0.5, 7.125),
            Point::new(17.0, 1e6 - 0.375),
            Point::new(-4.75, 900.0),
        ];
        let reference = ring_area_mm2(&base).unwrap();
        for rotation in 1..base.len() {
            let mut rotated = base.to_vec();
            rotated.rotate_left(rotation);
            assert_eq!(ring_area_mm2(&rotated).unwrap(), reference);
        }
        let mut reversed = base.to_vec();
        reversed.reverse();
        assert_eq!(ring_area_mm2(&reversed).unwrap(), reference);
        assert!(
            signed_ring_area_mm2(&reversed)
                .unwrap()
                .add(&signed_ring_area_mm2(&base).unwrap())
                .is_zero(),
            "reversing negates the signed value, so the two cancel exactly"
        );
    }

    #[test]
    fn holes_subtract_exactly() {
        let mut donut = rect(0.0, 0.0, 1000.0, 1000.0);
        donut.holes.push(vec![
            Point::new(100.0, 100.0),
            Point::new(100.0, 200.0),
            Point::new(200.0, 200.0),
            Point::new(200.0, 100.0),
        ]);
        let exact = donut.exact_area_mm2().unwrap();
        let want = ring_area_mm2(&[
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            Point::new(1000.0, 1000.0),
            Point::new(0.0, 1000.0),
        ])
        .unwrap()
        .sub(&ring_area_mm2(&donut.holes[0]).unwrap());
        assert_eq!(exact, want);
        assert_eq!(exact.hundredths_m2_half_even(), Some(99)); // 0.99 m²
    }

    #[test]
    fn non_finite_coordinates_refuse_rather_than_invent_a_value() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let ring = [
                Point::new(0.0, 0.0),
                Point::new(bad, 0.0),
                Point::new(0.0, 1.0),
            ];
            assert_eq!(ring_area_mm2(&ring), None);
            assert_eq!(signed_ring_area_mm2(&ring), None);
            assert_eq!(Polygon::new(ring.to_vec()).exact_area_mm2(), None);
            assert_eq!(display_hundredths_m2(&Polygon::new(ring.to_vec())), None);
        }
    }

    /// Coordinates far outside `COORD_BOUND_MM` are still finite, so the
    /// conversion has to report overflow rather than saturate or panic.
    #[test]
    fn an_area_too_large_for_the_counter_reports_overflow() {
        let huge = rect(0.0, 0.0, 1e300, 1e300);
        let exact = huge.exact_area_mm2().unwrap();
        assert!(exact.is_positive());
        assert_eq!(exact.hundredths_m2_half_even(), None);
        assert_eq!(display_hundredths_m2(&huge), None);
    }

    /// A single `f64` division can round an exact quotient that merely lies
    /// NEAR a tie ONTO the tie, after which ties-to-even walks to the wrong
    /// neighbour. Found by the issue-0060 owner against exact rational
    /// arithmetic; it is the reason the rounding cannot be done by dividing
    /// first, however careful the rounding mode afterwards.
    ///
    /// The exact quotient here is 6,201,590,365,930,937/625, which sits just
    /// BELOW 9,922,544,585,489.5. The float division lands exactly ON that
    /// tie and `round_ties_even` then answers ...490 because it is even. The
    /// exact answer is ...489.
    ///
    /// The value is about 200x beyond `DERIVED_COORD_BOUND_MM`-scale geometry,
    /// so no in-domain document reaches it. It is a regression against the
    /// METHOD, not a claim about the domain — and in-domain exactness here is
    /// established by construction rather than by sampling magnitudes.
    #[test]
    fn a_near_tie_quotient_is_not_rounded_onto_the_tie() {
        let area = f64::from_bits(0x4376_0850_46cd_c5b9);
        assert_eq!(area, 9.922544585489499e16);

        let divided_first = (area / 10_000.0).round_ties_even();
        assert_eq!(divided_first, 9_922_544_585_490.0, "the method that fails");

        let polygon = rect(0.0, 0.0, area, 1.0);
        assert_eq!(
            polygon.exact_area_mm2().unwrap().hundredths_m2_half_even(),
            Some(9_922_544_585_489),
            "exact rounding stays below the tie"
        );
        assert_eq!(display_hundredths_m2(&polygon), Some(9_922_544_585_489));
    }

    #[test]
    fn an_empty_sum_is_a_true_zero_and_never_displays_negative() {
        let total: ExactArea = Vec::<ExactArea>::new().into_iter().sum();
        assert!(total.is_zero());
        assert_eq!(total.hundredths_m2_half_even(), Some(0));
        // The f64 fold this replaces starts at negative zero.
        assert!(Vec::<f64>::new().iter().sum::<f64>().is_sign_negative());
    }

    #[test]
    fn a_level_total_sums_exactly_rather_than_summing_roundings() {
        let faces: Vec<ExactArea> = [10_640_000.0, 30_245_000.0, 5_040_000.0]
            .iter()
            .map(|&a| rect_of_area(a).exact_area_mm2().unwrap())
            .collect();
        let total: ExactArea = faces.iter().sum();
        // 45,925,000 mm² = 45.925 m², an exact tie; 45.92 has the even digit.
        assert_eq!(total.hundredths_m2_half_even(), Some(4592));
    }

    /// The filter may never certify an answer the exact path disagrees with.
    /// Deterministic pseudo-random polygons, including values deliberately
    /// placed on and beside rounding ties.
    #[test]
    fn the_filter_never_certifies_a_different_answer_than_the_exact_path() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut escalations = 0;
        let total = 4000;
        for i in 0..total {
            // Half the cases sit exactly on a tie, where the filter must defer.
            let polygon = if i % 2 == 0 {
                let hundredths = (next() % 1_000_000) as f64;
                rect_of_area(hundredths * 10_000.0 + 5_000.0)
            } else {
                let w = (next() % 10_000_000) as f64 / 64.0;
                let h = (next() % 10_000_000) as f64 / 64.0;
                rect(0.0, 0.0, w, h)
            };
            let exact = polygon
                .exact_area_mm2()
                .and_then(|a| a.hundredths_m2_half_even());
            assert_eq!(
                display_hundredths_m2(&polygon),
                exact,
                "filter disagreed with the exact path on {polygon:?}"
            );
            if filtered_hundredths(&polygon).is_none() {
                escalations += 1;
            }
        }
        // The filter has to actually be exercised in both directions, or this
        // test would pass with a filter that never certifies anything.
        assert!(escalations > total / 4, "ties must escalate: {escalations}");
        assert!(
            escalations < total * 3 / 4,
            "non-ties must not: {escalations}"
        );
    }

    /// R5.1.9 says positive area and means it: no floor may hide an overlap,
    /// however small, and however large the operands.
    ///
    /// Both regions are 1 km squares, which is inside `COORD_BOUND_MM`, and
    /// the overlap is about half a square millimetre. The relative floor ED-0103
    /// N-5 removes was `1e-12 x separate`, which is 2 mm² at this scale, so
    /// the old test reported NO LINK for a real overlap. The assertion below
    /// pins both halves: the overlap is positive, and it is smaller than the
    /// floor that used to hide it.
    #[test]
    fn a_tiny_overlap_between_kilometre_regions_is_still_positive() {
        let side = 1_000_000.0;
        let left = [rect(0.0, 0.0, side, side)];
        let right = [rect(0.0, 0.0, side, side - 5e-7)];

        // `right` carries a vertex `left` does not, so it is treated as
        // constructed and the verdict is honest about that rather than
        // certifying from a band.
        assert_eq!(positive_area(&left, &right), Some(PositiveArea::Uncertain));

        let overlap = left[0]
            .exact_area_mm2()
            .unwrap()
            .sub(&right[0].exact_area_mm2().unwrap());
        assert!(overlap.is_positive());

        let separate = left[0].area_mm2() + right[0].area_mm2();
        let retired_floor = 1e-12 * separate;
        let one_mm2: ExactArea = ring_area_mm2(&[
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ])
        .unwrap();
        assert!(
            retired_floor > 1.0,
            "the retired floor was {retired_floor} mm²"
        );
        assert!(
            overlap.sub(&one_mm2).is_negative(),
            "the overlap is under 1 mm², so the retired floor suppressed it"
        );
    }

    #[test]
    fn regions_that_merely_touch_have_exactly_zero_overlap() {
        let a = rect(0.0, 0.0, 1000.0, 1000.0);
        let b = rect(1000.0, 0.0, 2000.0, 1000.0);
        let merged = rect(0.0, 0.0, 2000.0, 1000.0);
        assert_eq!(
            positive_area(&[a, b], &[merged]),
            Some(PositiveArea::No),
            "touching is not overlapping"
        );
    }

    /// The three outcomes, through the REAL boolean pipeline rather than a
    /// hand-built "merged" polygon, because the whole point of
    /// [`PositiveArea::Uncertain`] is what `union` does to its vertices.
    #[test]
    fn the_three_outcomes_hold_through_the_union_pipeline() {
        use crate::boolean::union;

        let touching = [
            vec![
                Point::new(0.0, 0.0),
                Point::new(1000.0, 0.0),
                Point::new(1000.0, 1000.0),
                Point::new(0.0, 1000.0),
            ],
            vec![
                Point::new(1000.0, 300.0),
                Point::new(2000.0, 300.0),
                Point::new(2000.0, 700.0),
                Point::new(1000.0, 700.0),
            ],
        ];
        let left: Vec<Polygon> = touching.iter().cloned().map(Polygon::new).collect();
        assert_eq!(
            positive_area(&left, &union(&touching)),
            Some(PositiveArea::No),
            "a shared partial edge cancels exactly"
        );

        let overlapping = [
            touching[0].clone(),
            vec![
                Point::new(999.0, 300.0),
                Point::new(2000.0, 300.0),
                Point::new(2000.0, 700.0),
                Point::new(999.0, 700.0),
            ],
        ];
        let left: Vec<Polygon> = overlapping.iter().cloned().map(Polygon::new).collect();
        assert_eq!(
            positive_area(&left, &union(&overlapping)),
            Some(PositiveArea::Uncertain),
            "a real overlap constructs its crossing points, and their              displacement has no bound this function can supply"
        );
    }

    /// The contract root's review required: no non-finite coordinate may be
    /// certified, INCLUDING in a ring too short to enclose anything and
    /// INCLUDING in a hole, and the filtered public entry must agree with the
    /// exact constructors rather than short-circuiting ahead of them.
    #[test]
    fn no_non_finite_coordinate_is_certified_through_the_filtered_entry() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let cases = vec![
                Polygon::new(vec![Point::new(bad, 0.0)]),
                Polygon::new(vec![Point::new(bad, 0.0), Point::new(1.0, 1.0)]),
                Polygon::new(vec![
                    Point::new(bad, 0.0),
                    Point::new(1.0, 0.0),
                    Point::new(1.0, 1.0),
                ]),
            ];
            for polygon in cases {
                assert_eq!(
                    display_hundredths_m2(&polygon),
                    None,
                    "short or long, a non-finite ring has no area: {polygon:?}"
                );
                assert_eq!(filtered_hundredths(&polygon), None, "and must not certify");
            }

            // A sound outer ring with a non-finite hole, at both lengths. The
            // one-vertex hole is the case that previously certified: the bound
            // returned zero for a short ring before it checked finiteness.
            for hole in [
                vec![Point::new(bad, 5.0)],
                vec![
                    Point::new(bad, 5.0),
                    Point::new(6.0, 5.0),
                    Point::new(6.0, 9.0),
                ],
            ] {
                let mut polygon = rect(0.0, 0.0, 1000.0, 1000.0);
                polygon.holes.push(hole);
                assert_eq!(display_hundredths_m2(&polygon), None);
                assert_eq!(filtered_hundredths(&polygon), None);
            }
        }
    }

    /// A finite ring too short to enclose anything is exactly zero, and saying
    /// so is not the same bug as the one above.
    #[test]
    fn a_short_finite_ring_is_exactly_zero() {
        for ring in [
            vec![],
            vec![Point::new(3.0, 4.0)],
            vec![Point::new(3.0, 4.0), Point::new(5.0, 6.0)],
        ] {
            let polygon = Polygon::new(ring);
            assert_eq!(display_hundredths_m2(&polygon), Some(0));
            assert_eq!(
                polygon.exact_area_mm2().unwrap().hundredths_m2_half_even(),
                Some(0)
            );
        }
    }

    /// The discriminator itself: a vertex `right` does not share with `left`
    /// is what makes the answer uncertain, and nothing else does.
    #[test]
    fn only_a_vertex_left_did_not_supply_makes_the_answer_uncertain() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        // Same vertices, rotated and reordered: nothing constructed.
        let rotated = Polygon::new(vec![
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
        ]);
        assert_eq!(
            positive_area(std::slice::from_ref(&a), &[rotated]),
            Some(PositiveArea::No),
            "a permutation of the same vertices constructs nothing"
        );

        // One coordinate moved by a single ulp is a vertex `left` never gave.
        let nudged = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(f64::from_bits(10.0f64.to_bits() + 1), 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]);
        assert_eq!(
            positive_area(&[a], &[nudged]),
            Some(PositiveArea::Uncertain),
            "one ulp is still a point nobody supplied"
        );
    }

    /// How often the union leaves a residue that only a displaced vertex can
    /// explain, and how large it is. This is the measurement ED-0103 N-5's
    /// slack bound is derived against; it fails if the boolean pipeline gets
    /// materially worse, and it fails if a floor creeps back in and starts
    /// reporting touching regions as separate by fiat.
    #[test]
    fn touching_regions_cancel_exactly_or_are_reported_uncertain_never_linked() {
        use crate::boolean::union;

        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (mut exact_zero, mut uncertain) = (0, 0);
        for _ in 0..1000 {
            let ax = (next() % 2_000_000) as f64 / 128.0;
            let ay = (next() % 2_000_000) as f64 / 128.0;
            let y0 = (next() % 1_000_000) as f64 / 128.0;
            let y1 = y0 + (next() % 1_000_000) as f64 / 128.0;
            let rings = [
                vec![
                    Point::new(0.0, 0.0),
                    Point::new(ax, 0.0),
                    Point::new(ax, ay),
                    Point::new(0.0, ay),
                ],
                vec![
                    Point::new(ax, y0),
                    Point::new(ax + 500.0, y0),
                    Point::new(ax + 500.0, y1),
                    Point::new(ax, y1),
                ],
            ];
            let left: Vec<Polygon> = rings.iter().cloned().map(Polygon::new).collect();
            match positive_area(&left, &union(&rings)) {
                Some(PositiveArea::No) => exact_zero += 1,
                Some(PositiveArea::Uncertain) => uncertain += 1,
                other => panic!("touching regions reported {other:?} at ax={ax} y0={y0}"),
            }
        }
        assert_eq!(exact_zero + uncertain, 1000);
        // Every pair now cancels EXACTLY. Before issue 0058's `522ba12` this
        // was 971 and 29: a cut parameter landing on another ring's corner was
        // rebuilt as `a + (b - a)·t` and could miss it by an ulp. That commit
        // gives each welded input vertex a canonical representative and
        // reproduces it, so the merged ring's vertices are input vertices and
        // the shoelace sums cancel term for term.
        //
        // This assertion therefore PINS A CROSS-COMPONENT PAIRING: it holds
        // only with that fix present, which is why the two branches are
        // integrated together.
        assert_eq!(
            uncertain, 0,
            "constructed-vertex residue is back: {uncertain}"
        );
        assert_eq!(exact_zero, 1000);
    }

    #[test]
    fn positive_area_refuses_non_finite_input() {
        let bad = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(f64::NAN, 0.0),
            Point::new(0.0, 1.0),
        ]);
        assert_eq!(positive_area(&[bad], &[]), None);
    }
}
