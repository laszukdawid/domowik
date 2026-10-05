//! Exact sign of the 2x2 orientation determinant, over integers.
//!
//! # Why floating-point expansions are not enough here
//!
//! Shewchuk-style expansions are exact only while every intermediate product
//! stays out of the subnormal range: `two_product`'s error term is itself a
//! `f64`, and a product that underflows has an error term that cannot be
//! represented. The usual repair is to scale the operands into range, and it
//! fails for MIXED magnitudes — scaling by the factor the smallest operand
//! needs overflows the largest, and scaling by what the largest tolerates
//! leaves the smallest underflowing.
//!
//! The witness that forced this module, found by adversarial review (Codex,
//! 2026-09-05): with `η` the smallest positive subnormal,
//!
//! ```text
//! orientation((0, η), (1, 0), (η, η))
//! ```
//!
//! has exact determinant `η² > 0`, so the turn is counter-clockwise. The
//! largest coordinate difference is `1`, so a max-based rescale declines to
//! scale at all, and `two_product(−η, η)` then underflows to zero: the
//! "exact" path returned `Collinear`.
//!
//! # What this does instead
//!
//! Every finite `f64` is exactly `±m · 2^e` with `m` a 53-bit integer, so
//! every product of two of them is exactly `±(m₁m₂) · 2^(e₁+e₂)` — a 106-bit
//! integer at a known power of two. The determinant is a sum of six such
//! products, and the sum of finitely many dyadic rationals is exact in a
//! fixed-point accumulator wide enough to hold every bit position any of them
//! can occupy.
//!
//! No rounding happens anywhere, for ANY finite input, including mixtures of
//! subnormals and near-maximal values. There is no scaling, no filter and no
//! tolerance — this is the fallback the filter escalates to, not a fast path.
//!
//! # Why the STORED coordinates, not their differences
//!
//! `(bx−ax)(cy−ay) − (by−ay)(cx−ax)` expands to six products of the stored
//! coordinates once the `ax·ay` terms cancel. Taking differences first would
//! reintroduce a rounding step — and, for coordinates near the representable
//! maximum, an overflow — before any exact work began. Reading the stored
//! values directly avoids both.
//!
//! No dependency: `u64`/`u128` and a fixed array.

use std::cmp::Ordering;

/// A finite `f64` split into `(negative, mantissa, exponent)` with value
/// `(-1)^negative · mantissa · 2^exponent`.
///
/// Exact by construction — this is the IEEE-754 encoding read back, not an
/// approximation. Zero (of either sign) has mantissa `0`.
#[derive(Clone, Copy)]
struct Split {
    negative: bool,
    mantissa: u64,
    exponent: i32,
}

/// The lowest exponent any single product can carry.
///
/// A subnormal's exponent is `-1074`, so a product of two is `-2148`.
const MIN_EXPONENT: i32 = -2148;

/// The highest exponent any single product can carry.
///
/// `f64::MAX` is `(2^53 − 1) · 2^971`, so its exponent is `971` and a product
/// of two reaches `1942`.
const MAX_EXPONENT: i32 = 1942;

/// Bits in the accumulator.
///
/// The lowest bit a product can set is `MIN_EXPONENT`. The highest is
/// `MAX_EXPONENT` plus the 106 bits of a mantissa product, plus room for the
/// carries of summing six of them. `2^3` covers six terms with margin.
const SPAN_BITS: usize = (MAX_EXPONENT - MIN_EXPONENT) as usize + 106 + 3;

/// 64-bit limbs covering [`SPAN_BITS`], rounded up.
const LIMBS: usize = SPAN_BITS.div_ceil(64);

/// A non-negative fixed-point accumulator over `2^MIN_EXPONENT`.
///
/// Magnitudes only: the caller keeps positive and negative terms in separate
/// accumulators and compares them at the end. That removes borrow handling
/// entirely — there is no subtraction anywhere in this module — and a
/// comparison of two magnitudes is exactly the sign question being asked.
struct Fixed {
    limbs: [u64; LIMBS],
}

impl Fixed {
    fn zero() -> Self {
        Self { limbs: [0; LIMBS] }
    }

    /// Adds `value · 2^exponent`, exactly.
    ///
    /// `value` is at most 106 bits and `exponent` lies in
    /// `[MIN_EXPONENT, MAX_EXPONENT]`, so the shifted term occupies at most
    /// three limbs and the topmost of those is well inside the array.
    fn add(&mut self, value: u128, exponent: i32) {
        if value == 0 {
            return;
        }
        debug_assert!((MIN_EXPONENT..=MAX_EXPONENT).contains(&exponent));
        let offset = (exponent - MIN_EXPONENT) as usize;
        let index = offset / 64;
        let shift = offset % 64;

        let low = value as u64;
        let high = (value >> 64) as u64;
        // `x << 64` is not `0` for a `u64`: Rust treats a shift at or beyond
        // the width as an overflow, which panics under `debug-assertions` and
        // is masked to `x << 0` otherwise. Either behaviour is wrong here, so
        // the unshifted case is split out rather than written as a shift by
        // `64 - shift`.
        let (w0, w1, w2) = if shift == 0 {
            (low, high, 0)
        } else {
            (
                low << shift,
                (high << shift) | (low >> (64 - shift)),
                high >> (64 - shift),
            )
        };
        self.add_limb(index, w0);
        self.add_limb(index + 1, w1);
        self.add_limb(index + 2, w2);
    }

    /// Adds `value` at limb `index`, propagating the carry upward.
    fn add_limb(&mut self, index: usize, value: u64) {
        if value == 0 {
            return;
        }
        let mut i = index;
        let mut carry = value;
        while carry != 0 {
            // The array is sized so this cannot run past the end: the highest
            // limb any term reaches is `index + 2`, and the top limbs are
            // reserved for carries of the six-term sum.
            debug_assert!(i < LIMBS, "accumulator overflow");
            let (sum, overflowed) = self.limbs[i].overflowing_add(carry);
            self.limbs[i] = sum;
            carry = u64::from(overflowed);
            i += 1;
        }
    }
}

impl PartialEq for Fixed {
    fn eq(&self, other: &Self) -> bool {
        self.limbs == other.limbs
    }
}

impl Eq for Fixed {}

impl Ord for Fixed {
    fn cmp(&self, other: &Self) -> Ordering {
        for i in (0..LIMBS).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => continue,
                decided => return decided,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for Fixed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Splits a finite `f64` into its exact integer components.
///
/// Infinities and NaN have no such form and must not reach here; the caller
/// keeps them out.
fn split(x: f64) -> Split {
    let bits = x.to_bits();
    let negative = bits >> 63 == 1;
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    debug_assert!(biased != 0x7ff, "orientation excludes non-finite inputs");
    if biased == 0 {
        // Subnormal or zero: no implicit leading bit, and a fixed exponent.
        Split {
            negative,
            mantissa: fraction,
            exponent: -1074,
        }
    } else {
        Split {
            negative,
            mantissa: fraction | (1 << 52),
            // `biased - 1023` is the value exponent; the mantissa carries 52
            // extra fractional bits, hence the further `- 52`.
            exponent: biased - 1075,
        }
    }
}

/// The exact sign of `ax·by − ax·cy + bx·cy − bx·ay + cx·ay − cx·by`.
///
/// That expression is `(bx−ax)(cy−ay) − (by−ay)(cx−ax)` with the `ax·ay`
/// terms cancelled algebraically — the same determinant, evaluated without
/// ever forming a difference.
///
/// `Greater` means counter-clockwise, `Less` clockwise, `Equal` collinear.
///
/// # Exactness
///
/// Exact for every finite input, with no precondition on magnitude and no
/// mixed-scale caveat. Each of the six products is an integer at a known
/// power of two, and each is accumulated into a fixed-point register wide
/// enough for the entire representable exponent range.
pub(crate) fn orientation_sign(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> Ordering {
    let mut positive = Fixed::zero();
    let mut negative = Fixed::zero();

    let terms = [
        (ax, by, false),
        (ax, cy, true),
        (bx, cy, false),
        (bx, ay, true),
        (cx, ay, false),
        (cx, by, true),
    ];
    for (u, v, negate) in terms {
        let (u, v) = (split(u), split(v));
        if u.mantissa == 0 || v.mantissa == 0 {
            continue;
        }
        let product = u128::from(u.mantissa) * u128::from(v.mantissa);
        let exponent = u.exponent + v.exponent;
        if (u.negative ^ v.negative) ^ negate {
            negative.add(product, exponent);
        } else {
            positive.add(product, exponent);
        }
    }
    positive.cmp(&negative)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The witness that motivated this module. The determinant is exactly
    /// `η²`, far below anything `f64` can represent, and its SIGN is still
    /// decidable.
    #[test]
    fn the_subnormal_witness_is_counter_clockwise() {
        let eta = f64::from_bits(1);
        assert_eq!(
            orientation_sign(0.0, eta, 1.0, 0.0, eta, eta),
            Ordering::Greater
        );
        // Mirrored: swapping two points reverses the turn.
        assert_eq!(
            orientation_sign(1.0, 0.0, 0.0, eta, eta, eta),
            Ordering::Less
        );
    }

    #[test]
    fn a_product_of_two_smallest_subnormals_still_carries_a_sign() {
        let eta = f64::from_bits(1);
        // Determinant η·η − 0 = η² > 0.
        assert_eq!(
            orientation_sign(0.0, 0.0, eta, 0.0, 0.0, eta),
            Ordering::Greater
        );
        assert_eq!(
            orientation_sign(0.0, 0.0, 0.0, eta, eta, 0.0),
            Ordering::Less
        );
    }

    #[test]
    fn the_extreme_exponents_do_not_overflow_the_accumulator() {
        let big = f64::MAX;
        let eta = f64::from_bits(1);
        // Largest and smallest magnitudes in one determinant.
        assert_eq!(
            orientation_sign(0.0, 0.0, big, 0.0, 0.0, eta),
            Ordering::Greater
        );
        assert_eq!(
            orientation_sign(0.0, 0.0, eta, 0.0, 0.0, big),
            Ordering::Greater
        );
        // Both products at the top of the range, differing by one ulp.
        let next = f64::from_bits(big.to_bits() - 1);
        assert_eq!(
            orientation_sign(0.0, 0.0, big, next, next, big),
            Ordering::Greater
        );
    }

    #[test]
    fn collinear_and_signed_zero_inputs_report_exactly_zero() {
        assert_eq!(
            orientation_sign(0.0, 0.0, 1.0, 1.0, 2.0, 2.0),
            Ordering::Equal
        );
        assert_eq!(
            orientation_sign(-0.0, 0.0, 1.0, -0.0, 2.0, 0.0),
            Ordering::Equal,
            "signed zeros are zero"
        );
        // A degenerate triple with two identical points.
        assert_eq!(
            orientation_sign(3.0, 4.0, 3.0, 4.0, 9.0, 1.0),
            Ordering::Equal
        );
    }

    #[test]
    fn an_integer_grid_agrees_with_i128_arithmetic() {
        // Small integers are exact in both worlds, so `i128` is an
        // independent oracle over a dense sweep.
        for ax in -3..=3i64 {
            for ay in -3..=3i64 {
                for bx in -3..=3i64 {
                    for by in -3..=3i64 {
                        for cx in -3..=3i64 {
                            for cy in -3..=3i64 {
                                let exact = i128::from(bx - ax) * i128::from(cy - ay)
                                    - i128::from(by - ay) * i128::from(cx - ax);
                                let got = orientation_sign(
                                    ax as f64, ay as f64, bx as f64, by as f64, cx as f64,
                                    cy as f64,
                                );
                                assert_eq!(got, exact.cmp(&0), "{ax} {ay} {bx} {by} {cx} {cy}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn scaling_every_coordinate_by_a_power_of_two_preserves_the_sign() {
        // The determinant is homogeneous of degree two, so this must hold —
        // and it exercises the accumulator across the whole exponent range
        // rather than only near 1.
        let base = [0.25f64, -0.5, 3.0, 1.5, -2.0, 7.0];
        let reference = orientation_sign(base[0], base[1], base[2], base[3], base[4], base[5]);
        // det = (3 − 0.25)(7 + 0.5) − (1.5 + 0.5)(−2 − 0.25) = 25.125 > 0.
        assert_eq!(
            reference,
            Ordering::Greater,
            "fixture must not be degenerate"
        );
        for exponent in [-1060i32, -900, -500, 0, 500, 900, 1000] {
            let scale = if exponent >= -1022 {
                f64::from_bits(((exponent + 1023) as u64) << 52)
            } else {
                // Reach the subnormal range in two exact steps.
                f64::from_bits(((exponent + 1023 + 600) as u64) << 52)
                    * f64::from_bits(((1023 - 600) as u64) << 52)
            };
            let s: Vec<f64> = base.iter().map(|v| v * scale).collect();
            assert_eq!(
                orientation_sign(s[0], s[1], s[2], s[3], s[4], s[5]),
                reference,
                "scaling by 2^{exponent} changed the sign"
            );
        }
    }
}
