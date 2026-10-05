//! A minimal exact signed dyadic rational: `±magnitude · 2^exponent`.
//!
//! # Why this exists
//!
//! Every finite `f64` is exactly `±m · 2^e` with `m` a 53-bit integer, so any
//! polynomial in stored coordinates is a sum of signed dyadic products and has
//! an exactly decidable sign. [`crate::exact_det`] does this for the ONE
//! expression it needs — a six-product determinant — with a fixed-width
//! accumulator sized for that degree.
//!
//! The R3.1.2 area predicate reaches TOTAL degree 12, counting a thickness as
//! the length it is: twelve `f64` factors, so ~636 mantissa bits, with
//! per-factor exponents in `[-1074, 971]` summing over twelve factors to
//! `[-12888, 11652]`. That span runs to tens of thousands of bits once terms
//! at opposite ends are added, which is why this is arbitrary width rather
//! than a wider constant.
//!
//! This is that arbitrary width and nothing more: add, subtract, multiply,
//! compare. No division, no square root, no normalization to lowest terms, no
//! dependency. Heap allocation is deliberate and confined to the exact
//! fallback; the filter that decides the overwhelming majority of pairs never
//! constructs one of these.
//!
//! # Representation
//!
//! `limbs` is a little-endian magnitude with no high zero limb, so zero is the
//! empty vector and every value has one representation per exponent. The
//! exponent is NOT canonicalized — `1·2^1` and `2·2^0` are both permitted and
//! compare equal — because canonicalizing costs a scan on every operation to
//! buy nothing the comparison does not already handle.

use std::cmp::Ordering;

/// An exact signed dyadic rational.
#[derive(Clone, Debug)]
pub(crate) struct Dyadic {
    /// Sign of a non-zero value. Always `false` for zero, so the
    /// representation of zero is unique.
    negative: bool,
    /// Little-endian magnitude limbs, no trailing high zero. Empty is zero.
    limbs: Vec<u64>,
    /// Value is `(-1)^negative · magnitude · 2^exponent`.
    exponent: i64,
}

impl Dyadic {
    /// Zero.
    pub(crate) fn zero() -> Self {
        Self {
            negative: false,
            limbs: Vec::new(),
            exponent: 0,
        }
    }

    /// The exact value of a finite `f64`.
    ///
    /// Reads the IEEE-754 encoding, so this is an identity rather than a
    /// conversion. Non-finite input has no dyadic form; the caller's contract
    /// excludes it and a debug build asserts as much.
    pub(crate) fn from_f64(x: f64) -> Self {
        let bits = x.to_bits();
        let negative = bits >> 63 == 1;
        let biased = ((bits >> 52) & 0x7ff) as i64;
        let fraction = bits & 0x000f_ffff_ffff_ffff;
        debug_assert!(biased != 0x7ff, "non-finite input has no dyadic form");
        // Subnormals carry no implicit leading bit and share one exponent.
        let (mantissa, exponent) = if biased == 0 {
            (fraction, -1074)
        } else {
            (fraction | (1 << 52), biased - 1075)
        };
        if mantissa == 0 {
            return Self::zero();
        }
        Self {
            negative,
            limbs: vec![mantissa],
            exponent,
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    pub(crate) fn is_negative(&self) -> bool {
        self.negative
    }

    pub(crate) fn is_positive(&self) -> bool {
        !self.negative && !self.is_zero()
    }

    /// The same magnitude with a positive sign.
    pub(crate) fn abs(&self) -> Self {
        Self {
            negative: false,
            limbs: self.limbs.clone(),
            exponent: self.exponent,
        }
    }

    /// The same magnitude with the opposite sign. Zero stays zero.
    pub(crate) fn neg(&self) -> Self {
        Self {
            negative: !self.negative && !self.is_zero(),
            limbs: self.limbs.clone(),
            exponent: self.exponent,
        }
    }

    pub(crate) fn add(&self, other: &Self) -> Self {
        if self.is_zero() {
            return other.clone();
        }
        if other.is_zero() {
            return self.clone();
        }
        let (a, b, exponent) = align(self, other);
        if self.negative == other.negative {
            Self::new(self.negative, add_magnitudes(&a, &b), exponent)
        } else {
            match compare_magnitudes(&a, &b) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    Self::new(self.negative, subtract_magnitudes(&a, &b), exponent)
                }
                Ordering::Less => Self::new(other.negative, subtract_magnitudes(&b, &a), exponent),
            }
        }
    }

    pub(crate) fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }

    pub(crate) fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        // Exponents ADD and magnitudes multiply, so a product never needs the
        // alignment that makes addition expensive: operand widths stay
        // proportional to the number of factors, not to their exponent spread.
        Self::new(
            self.negative != other.negative,
            multiply_magnitudes(&self.limbs, &other.limbs),
            self.exponent + other.exponent,
        )
    }

    /// `self * self`, which is never negative.
    pub(crate) fn square(&self) -> Self {
        self.mul(self)
    }

    /// `self / 2`, exactly. Halving a dyadic rational only moves the binary
    /// point, so this is the one division that costs nothing and loses nothing.
    pub(crate) fn halve(&self) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        Self {
            negative: self.negative,
            limbs: self.limbs.clone(),
            exponent: self.exponent - 1,
        }
    }

    /// `self / 10^k`, rounded to the nearest integer with ties to even.
    ///
    /// `None` when the quotient does not fit an `i64`, which for the areas
    /// this serves means the caller was handed something outside the
    /// coordinate domain entirely.
    ///
    /// # Why this is exact
    ///
    /// `self` is `±M · 2^e`, so `self / 10^k` is the ratio of two integers:
    /// `M · 2^e` over `10^k` when `e ≥ 0`, and `M` over `10^k · 2^-e` when it
    /// is negative. Integer division gives an exact quotient and remainder,
    /// and the tie test is `2·remainder` against the divisor — a comparison of
    /// integers, not of floats. A tie is therefore DETECTED rather than
    /// approached: `30,245,000 / 10^4` is exactly `3024.5` here, and the
    /// even-digit rule decides it rather than whichever neighbour a binary
    /// division happened to land on.
    ///
    /// Rounding is symmetric about zero, so the magnitude is rounded and the
    /// sign applied afterwards.
    pub(crate) fn round_half_even_div_pow10(&self, k: u32) -> Option<i64> {
        if self.is_zero() {
            return Some(0);
        }

        let mut divisor = vec![1u64];
        for _ in 0..k {
            divisor = multiply_magnitudes(&divisor, &[10]);
        }

        // Shifts are bounded by the dyadic exponent range of products of
        // finite binary64 values; refuse anything wilder rather than allocate
        // for it.
        const MAX_SHIFT_BITS: i64 = 1 << 16;
        if self.exponent.abs() > MAX_SHIFT_BITS {
            return None;
        }
        let (mut remainder, divisor) = if self.exponent >= 0 {
            (shift_left(&self.limbs, self.exponent as usize), divisor)
        } else {
            (
                self.limbs.clone(),
                shift_left(&divisor, (-self.exponent) as usize),
            )
        };

        // Binary long division. The quotient must fit an i64, so at most 63
        // shifted subtractions are possible and a wider gap means overflow.
        let gap = bit_length(&remainder) as i64 - bit_length(&divisor) as i64;
        if gap > 62 {
            return None;
        }
        let mut quotient: u64 = 0;
        for shift in (0..=gap.max(0) as u32).rev() {
            let scaled = shift_left(&divisor, shift as usize);
            if compare_magnitudes(&scaled, &remainder) != Ordering::Greater {
                remainder = subtract_magnitudes(&remainder, &scaled);
                quotient |= 1u64 << shift;
            }
        }

        let twice = shift_left(&remainder, 1);
        let rounded = match compare_magnitudes(&twice, &divisor) {
            Ordering::Less => quotient,
            Ordering::Greater => quotient.checked_add(1)?,
            // An exact half: keep the even neighbour (R5.3.2).
            Ordering::Equal if quotient.is_multiple_of(2) => quotient,
            Ordering::Equal => quotient.checked_add(1)?,
        };

        let magnitude = i64::try_from(rounded).ok()?;
        Some(if self.negative { -magnitude } else { magnitude })
    }

    fn new(negative: bool, mut limbs: Vec<u64>, exponent: i64) -> Self {
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        if limbs.is_empty() {
            return Self::zero();
        }
        Self {
            negative,
            limbs,
            exponent,
        }
    }
}

impl PartialEq for Dyadic {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Dyadic {}

impl Ord for Dyadic {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_zero(), other.is_zero()) {
            (true, true) => return Ordering::Equal,
            (true, false) => {
                return if other.negative {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (false, true) => {
                return if self.negative {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            (false, false) => {}
        }
        match (self.negative, other.negative) {
            (false, true) => return Ordering::Greater,
            (true, false) => return Ordering::Less,
            _ => {}
        }
        let (a, b, _) = align(self, other);
        let magnitude = compare_magnitudes(&a, &b);
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for Dyadic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Both magnitudes at the lower of the two exponents.
///
/// The operand with the higher exponent is shifted UP, never the other one
/// down: shifting down would discard bits, and every bit here is load-bearing.
fn align(a: &Dyadic, b: &Dyadic) -> (Vec<u64>, Vec<u64>, i64) {
    match a.exponent.cmp(&b.exponent) {
        Ordering::Equal => (a.limbs.clone(), b.limbs.clone(), a.exponent),
        Ordering::Greater => (
            shift_left(&a.limbs, (a.exponent - b.exponent) as usize),
            b.limbs.clone(),
            b.exponent,
        ),
        Ordering::Less => (
            a.limbs.clone(),
            shift_left(&b.limbs, (b.exponent - a.exponent) as usize),
            a.exponent,
        ),
    }
}

fn shift_left(limbs: &[u64], bits: usize) -> Vec<u64> {
    if limbs.is_empty() {
        return Vec::new();
    }
    let whole = bits / 64;
    let part = bits % 64;
    let mut out = vec![0u64; whole];
    if part == 0 {
        // `x >> 64` is an overflow for `u64` — a panic under
        // `debug-assertions`, a mask otherwise — so the aligned case is split
        // out rather than written as a shift by `64 - part`.
        out.extend_from_slice(limbs);
        return out;
    }
    let mut carry = 0u64;
    for &limb in limbs {
        out.push((limb << part) | carry);
        carry = limb >> (64 - part);
    }
    if carry != 0 {
        out.push(carry);
    }
    out
}

fn add_magnitudes(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u128;
    for i in 0..a.len().max(b.len()) {
        let sum = u128::from(*a.get(i).unwrap_or(&0)) + u128::from(*b.get(i).unwrap_or(&0)) + carry;
        out.push(sum as u64);
        carry = sum >> 64;
    }
    if carry != 0 {
        out.push(carry as u64);
    }
    out
}

/// `a - b`, requiring `a >= b` as magnitudes.
fn subtract_magnitudes(a: &[u64], b: &[u64]) -> Vec<u64> {
    debug_assert!(compare_magnitudes(a, b) != Ordering::Less);
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i128;
    for (i, &limb) in a.iter().enumerate() {
        let diff = i128::from(limb) - i128::from(*b.get(i).unwrap_or(&0)) - borrow;
        if diff < 0 {
            out.push((diff + (1i128 << 64)) as u64);
            borrow = 1;
        } else {
            out.push(diff as u64);
            borrow = 0;
        }
    }
    debug_assert_eq!(borrow, 0, "subtract_magnitudes needs a >= b");
    out
}

fn multiply_magnitudes(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = vec![0u64; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u128;
        for (j, &y) in b.iter().enumerate() {
            let slot = &mut out[i + j];
            let total = u128::from(*slot) + u128::from(x) * u128::from(y) + carry;
            *slot = total as u64;
            carry = total >> 64;
        }
        // The carry cannot run past the end: `out` is exactly wide enough for
        // the product of an `a.len()`-limb and a `b.len()`-limb magnitude.
        let mut k = i + b.len();
        while carry != 0 {
            let total = u128::from(out[k]) + carry;
            out[k] = total as u64;
            carry = total >> 64;
            k += 1;
        }
    }
    out
}

/// Compares two magnitudes already at a common exponent.
/// Position of the highest set bit, counting from one. Zero has length zero.
fn bit_length(limbs: &[u64]) -> usize {
    match limbs.iter().rposition(|&l| l != 0) {
        None => 0,
        Some(i) => i * 64 + (64 - limbs[i].leading_zeros() as usize),
    }
}

fn compare_magnitudes(a: &[u64], b: &[u64]) -> Ordering {
    let top = a.len().max(b.len());
    for i in (0..top).rev() {
        match a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)) {
            Ordering::Equal => continue,
            decided => return decided,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(x: f64) -> Dyadic {
        Dyadic::from_f64(x)
    }

    #[test]
    fn small_integers_round_trip_through_arithmetic() {
        for a in -8i64..=8 {
            for b in -8i64..=8 {
                let (x, y) = (d(a as f64), d(b as f64));
                assert_eq!(x.cmp(&y), a.cmp(&b), "{a} vs {b}");
                assert_eq!(x.add(&y), d((a + b) as f64), "{a} + {b}");
                assert_eq!(x.sub(&y), d((a - b) as f64), "{a} - {b}");
                assert_eq!(x.mul(&y), d((a * b) as f64), "{a} * {b}");
            }
        }
    }

    #[test]
    fn zero_has_one_representation_and_orders_correctly() {
        assert!(d(0.0).is_zero());
        assert!(d(-0.0).is_zero(), "signed zero is zero");
        assert_eq!(d(0.0), d(-0.0));
        assert_eq!(d(0.0), Dyadic::zero());
        assert!(!d(-0.0).is_negative(), "zero is never negative");
        assert!(d(-1.0) < d(0.0) && d(0.0) < d(1.0));
        // A difference that cancels exactly must produce canonical zero, not a
        // vector of zero limbs that compares wrongly.
        let cancelled = d(1e300).sub(&d(1e300));
        assert!(cancelled.is_zero() && !cancelled.is_negative());
        assert_eq!(cancelled, Dyadic::zero());
    }

    #[test]
    fn carries_propagate_across_limb_boundaries() {
        // Repeated doubling walks a set bit up through several limbs, and
        // subtracting one then turns every bit below it on — so adding it back
        // must carry the whole way.
        let one = d(1.0);
        let mut value = d(1.0);
        for _ in 0..300 {
            value = value.add(&value);
        }
        // 2^300, so exactly one bit set five limbs up when exponent is folded.
        assert_eq!(value, d(2f64.powi(300)));
        assert_eq!(value.sub(&one).add(&one), value);
        // (2^300 - 1) + 1 must carry all the way back to 2^300.
        let ones = value.sub(&one);
        assert!(ones < value);
        assert_eq!(ones.add(&one), value);
    }

    #[test]
    fn subtraction_borrows_across_limb_boundaries() {
        let big = d(2f64.powi(192));
        let one = d(1.0);
        let borrowed = big.sub(&one);
        assert!(borrowed.is_positive());
        assert!(borrowed < big);
        assert_eq!(borrowed.add(&one), big);
        // And the reverse direction takes the sign with it.
        let negative = one.sub(&big);
        assert!(negative.is_negative());
        assert_eq!(negative.neg(), borrowed);
    }

    #[test]
    fn mixed_scales_add_and_compare_exactly() {
        // The case that defeats floating point: a subnormal added to a huge
        // value must not vanish, and must still order correctly.
        let eta = d(f64::from_bits(1));
        let huge = d(1e300);
        let sum = huge.add(&eta);
        assert!(sum > huge, "the subnormal survived the addition");
        assert_eq!(sum.sub(&huge), eta);
        assert_eq!(
            huge.add(&eta).sub(&eta),
            huge,
            "and it comes back out unchanged"
        );
        // In floating point this addition is the identity.
        assert_eq!(1e300 + f64::from_bits(1), 1e300);
    }

    #[test]
    fn products_of_subnormals_are_exact_rather_than_zero() {
        let eta = d(f64::from_bits(1));
        let square = eta.square();
        assert!(square.is_positive(), "eta^2 underflows f64 but is not zero");
        assert!(square < eta, "and it is smaller than eta");
        // f64 cannot hold it at all.
        assert_eq!(f64::from_bits(1) * f64::from_bits(1), 0.0);
    }

    #[test]
    fn cancellation_across_wildly_different_exponents_keeps_the_sign() {
        // (huge + tiny) - huge is exactly tiny; a rounded evaluation gives 0.
        let huge = d(f64::MAX);
        let tiny = d(f64::from_bits(1));
        assert_eq!(huge.add(&tiny).sub(&huge), tiny);
        // And the sign survives when the tail decides the answer.
        let left = huge.add(&tiny);
        let right = huge.add(&tiny).add(&tiny);
        assert!(left < right);
        assert!(right.sub(&left) == tiny);
    }

    #[test]
    fn negation_and_absolute_value_are_involutions_on_the_magnitude() {
        for v in [1.0, -1.0, 1e-300, -1e300, f64::from_bits(1)] {
            let x = d(v);
            assert_eq!(x.neg().neg(), x);
            assert_eq!(x.abs(), if x.is_negative() { x.neg() } else { x.clone() });
            assert!(!x.abs().is_negative());
        }
    }

    #[test]
    fn multiplication_is_commutative_and_signs_compose() {
        let values = [1.5f64, -3.25, 1e100, -1e-100, f64::from_bits(1), 0.0];
        for &u in &values {
            for &v in &values {
                let (x, y) = (d(u), d(v));
                assert_eq!(x.mul(&y), y.mul(&x), "{u} * {v}");
                let product = x.mul(&y);
                if u == 0.0 || v == 0.0 {
                    assert!(product.is_zero());
                } else {
                    assert_eq!(product.is_negative(), (u < 0.0) != (v < 0.0), "{u} * {v}");
                }
            }
        }
    }
}
