//! Outward-rounded interval enclosures, shared by every filtered predicate.
//!
//! Extracted from `distance.rs` so `arrangement.rs` can certify a constructed
//! crossing with the same machinery rather than growing a second copy of it.
//! The containment argument below is `distance.rs`'s, unchanged, because it is
//! what every caller's soundness rests on.
//!
//! # Outwardness is the whole safety property
//!
//! A filter that merely looks accurate is not a filter. These intervals are
//! widened OUTWARD at every step, so the exact value is guaranteed to lie
//! inside. A caller may therefore treat "the interval decides" as a proof and
//! "the interval does not decide" as a prompt to escalate. An interval that
//! were ever too NARROW would silently certify a wrong answer, and no
//! downstream exact path would run to catch it.

/// An interval that is guaranteed to CONTAIN the exact value it stands for.
///
/// # Why enclosures rather than error-free transformations
///
/// The usual fast filter for a predicate like this is an error-free
/// transformation — compute the rounded value and a correction term with
/// `two_product`/`two_sum`, then bound the residual. That technique is
/// unsound in the subnormal range, because the correction term is itself a
/// binary64 and a product that underflows has an error term that cannot be
/// represented. The same defect once made `orientation`'s "exact" path return
/// `Collinear` for a genuinely counter-clockwise turn, and `exact_det` exists
/// because of it.
///
/// An outward-rounded enclosure has no such precondition. Every IEEE-754
/// operation here is correctly rounded, so WHENEVER ITS ROUNDED RESULT IS
/// FINITE the exact result lies within half an ulp of it, and widening that
/// endpoint by one ulp outward keeps the exact value inside the interval — at
/// any finite magnitude, including subnormal results and results that
/// underflow to zero. Composition preserves containment, so the final
/// interval encloses the exact squared distance.
///
/// That bound says nothing about an operation that OVERFLOWS, and no claim is
/// made for one: an infinite endpoint is not treated as a bound at all.
/// [`separate`] refuses to decide when any endpoint is non-finite, which is
/// what keeps the filter sound in that range rather than the half-ulp
/// argument above.
///
/// This is a filter, not the answer: it is allowed to be too wide to decide,
/// and it can never establish equality. Both are handled by falling through
/// to the exact path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Enclosure {
    pub(crate) lo: f64,
    pub(crate) hi: f64,
}

/// The enclosure of a value that is already exact: itself.
pub(crate) fn exactly(x: f64) -> Enclosure {
    Enclosure { lo: x, hi: x }
}

impl Enclosure {
    /// `a − b`, widened outward by one ulp each way.
    pub(crate) fn difference(a: f64, b: f64) -> Self {
        let d = a - b;
        Self {
            lo: d.next_down(),
            hi: d.next_up(),
        }
    }

    /// The square of every value this encloses.
    ///
    /// Sign-aware, because squaring is not monotone across zero: an interval
    /// straddling zero has an exact lower bound of zero — attained — and an
    /// upper bound at whichever end is larger in magnitude.
    pub(crate) fn square(self) -> Self {
        let (lo, hi) = if self.lo >= 0.0 {
            (self.lo * self.lo, self.hi * self.hi)
        } else if self.hi <= 0.0 {
            (self.hi * self.hi, self.lo * self.lo)
        } else {
            // Zero is attained exactly, so this endpoint needs no widening.
            return Self {
                lo: 0.0,
                hi: (self.lo * self.lo).max(self.hi * self.hi).next_up(),
            };
        };
        Self {
            lo: lo.next_down(),
            hi: hi.next_up(),
        }
    }

    /// The sum of every pair of values the two enclose.
    pub(crate) fn sum(self, other: Self) -> Self {
        Self {
            lo: (self.lo + other.lo).next_down(),
            hi: (self.hi + other.hi).next_up(),
        }
    }

    pub(crate) fn is_finite(&self) -> bool {
        self.lo.is_finite() && self.hi.is_finite()
    }
}

impl Enclosure {
    /// Every product of a value from each interval.
    ///
    /// Sign-agnostic: the extremes of a product of intervals are among the
    /// four corner products, so both ends are taken from those and widened
    /// outward.
    pub(crate) fn product(self, other: Self) -> Self {
        let c = [
            self.lo * other.lo,
            self.lo * other.hi,
            self.hi * other.lo,
            self.hi * other.hi,
        ];
        let mut lo = c[0];
        let mut hi = c[0];
        for v in c.iter().skip(1) {
            if *v < lo {
                lo = *v;
            }
            if *v > hi {
                hi = *v;
            }
        }
        Self {
            lo: lo.next_down(),
            hi: hi.next_up(),
        }
    }

    /// Every difference of a value in `self` from a value in `other`.
    pub(crate) fn minus(self, other: Self) -> Self {
        Self {
            lo: (self.lo - other.hi).next_down(),
            hi: (self.hi - other.lo).next_up(),
        }
    }

    /// Whether the interval is strictly on one side of zero, which is what a
    /// caller needs before dividing by it.
    pub(crate) fn excludes_zero(&self) -> bool {
        self.lo > 0.0 || self.hi < 0.0
    }

    /// The certified sign of every value in this interval, or `None` when the
    /// interval straddles zero and so decides nothing.
    ///
    /// This is what an outward enclosure CAN prove. It cannot prove that a
    /// constructed coordinate is correctly rounded: each widening costs a full
    /// ULP, a composed quotient accumulates several, and correct rounding
    /// needs the interval to sit inside HALF an ULP. Measured over 20,000
    /// random crossings the quotient enclosure was 6 ULPs wide at best and 23
    /// at the median, so it certified correct rounding in none of them. Signs
    /// are a different question and the interval answers them cheaply.
    pub(crate) fn sign(&self) -> Option<std::cmp::Ordering> {
        if !self.is_finite() {
            return None;
        }
        if !self.excludes_zero() {
            return None;
        }
        Some(if self.lo > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        })
    }

    /// Whether every value here is certainly less than every value in `other`.
    pub(crate) fn certainly_below(&self, other: &Self) -> bool {
        self.is_finite() && other.is_finite() && self.hi < other.lo
    }
}
