//! Exact real numbers of the form `(x + y·√d) / z`, and their signs.
//!
//! # Why this one shape
//!
//! Every quantity the exact hit and snap kernels compare is of this form
//! (issue 0012, Codex GPT-6-Astra review 2026-10-09):
//!
//! - squared distance from a point to a point, segment, line or ray is a
//!   rational `N / D` (`y = 0`);
//! - squared distance to an IDEAL wall or opening rectangle outside its band
//!   is `(A − B·√D) / D`, `D` the squared axis length, because the side lies
//!   `h·√D / D` from the axis;
//! - coordinates of the closest point, and of an ideal rectangle's corners,
//!   are `(X + Y·√D) / Z`.
//!
//! All coefficients are dyadic rationals ([`Dyadic`]) built from the binary64
//! inputs, so nothing here rounds. Comparing two such numbers reduces to the
//! sign of `P + Q·√d + R·√e` ([`sign2`]), decided by sign-aware squaring
//! ([`sign1`]): no square root, no factorization, no precision loop, and a
//! fixed number of big-integer products.
//!
//! This is the EXACT path. Callers try a floating enclosure first and come
//! here only when it cannot decide (see `witness.rs`).

use crate::dyadic::Dyadic;
use std::cmp::Ordering;

fn sign(x: &Dyadic) -> Ordering {
    if x.is_zero() {
        Ordering::Equal
    } else if x.is_negative() {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

fn times(s: Ordering, t: Ordering) -> Ordering {
    match (s, t) {
        (Ordering::Equal, _) | (_, Ordering::Equal) => Ordering::Equal,
        (a, b) if a == b => Ordering::Greater,
        _ => Ordering::Less,
    }
}

/// The sign of `x + y·√d`, for `d ≥ 0`.
///
/// Same-sign terms decide themselves. Opposite-sign terms compare their
/// non-negative magnitudes by squaring: `|x| > |y|·√d ⇔ x² > y²·d`, and the
/// larger magnitude carries its sign. A zero difference is exact
/// cancellation, not a fuzzy tie.
pub(crate) fn sign1(x: &Dyadic, y: &Dyadic, d: &Dyadic) -> Ordering {
    let (sx, sy) = (
        sign(x),
        if d.is_zero() {
            Ordering::Equal
        } else {
            sign(y)
        },
    );
    if sy == Ordering::Equal {
        return sx;
    }
    if sx == Ordering::Equal || sx == sy {
        return sy;
    }
    times(sx, sign(&x.square().sub(&y.square().mul(d))))
}

/// The sign of `p + q·√d + r·√e`, for `d, e ≥ 0`.
///
/// With `s = sign(p + q·√d)`: if `r·√e` is zero or shares `s`, the sum has
/// sign `s` (or `r`'s, when `s` is zero). Otherwise the two parts have
/// opposite signs and the larger magnitude wins:
/// `(p + q·√d)² − r²·e = (p² + q²·d − r²·e) + 2pq·√d`, whose sign `sign1`
/// decides. Squaring is legitimate only because the signs were established
/// first — `1 − √2` squared would lose its negative sign.
pub(crate) fn sign2(p: &Dyadic, q: &Dyadic, d: &Dyadic, r: &Dyadic, e: &Dyadic) -> Ordering {
    let s = sign1(p, q, d);
    let sr = if e.is_zero() {
        Ordering::Equal
    } else {
        sign(r)
    };
    if sr == Ordering::Equal {
        return s;
    }
    if s == Ordering::Equal || s == sr {
        return sr;
    }
    let x = p.square().add(&q.square().mul(d)).sub(&r.square().mul(e));
    let y = p.mul(q).add(&p.mul(q));
    times(s, sign1(&x, &y, d))
}

/// The exact real `(x + y·√d) / z`, with `d ≥ 0` and `z > 0`.
#[derive(Debug, Clone)]
pub(crate) struct Quad {
    pub(crate) x: Dyadic,
    pub(crate) y: Dyadic,
    pub(crate) d: Dyadic,
    pub(crate) z: Dyadic,
}

impl Quad {
    /// `x / z`, `z > 0`.
    pub(crate) fn ratio(x: Dyadic, z: Dyadic) -> Self {
        debug_assert!(z.is_positive(), "quad: non-positive denominator");
        Self {
            x,
            y: Dyadic::zero(),
            d: Dyadic::zero(),
            z,
        }
    }

    /// `(x + y·√d) / z`, `d ≥ 0`, `z > 0`.
    pub(crate) fn new(x: Dyadic, y: Dyadic, d: Dyadic, z: Dyadic) -> Self {
        debug_assert!(z.is_positive(), "quad: non-positive denominator");
        debug_assert!(!d.is_negative(), "quad: negative radicand");
        Self { x, y, d, z }
    }

    /// The exact ordering of `self` and `other`: the sign of
    /// `(x₁z₂ − x₂z₁) + y₁z₂·√d₁ − y₂z₁·√d₂` (denominators are positive).
    pub(crate) fn cmp(&self, other: &Self) -> Ordering {
        let p = self.x.mul(&other.z).sub(&other.x.mul(&self.z));
        let q = self.y.mul(&other.z);
        let r = other.y.mul(&self.z).neg();
        sign2(&p, &q, &self.d, &r, &other.d)
    }

    /// A binary64 approximation of the value, evaluated in scaled
    /// arithmetic (mantissa and a separate exponent), so no intermediate
    /// overflows or underflows however extreme the coefficients: a handful
    /// of roundings of relative size 2⁻⁵³, plus whatever cancellation
    /// `x + y·√d` itself has. Used to REPORT a point when a witness's
    /// enclosure was lost; never to decide anything.
    pub(crate) fn approx(&self) -> f64 {
        let Some((mz, ez)) = self.z.to_scaled() else {
            return f64::NAN;
        };
        let (mx, ex) = self.x.to_scaled().unwrap_or((0.0, i64::MIN / 4));
        let term = match (self.y.to_scaled(), self.d.to_scaled()) {
            (Some((my, ey)), Some((md, ed))) => {
                // √(md·2^ed): make the exponent even first.
                let (md, ed) = if ed % 2 != 0 {
                    (md * 2.0, ed - 1)
                } else {
                    (md, ed)
                };
                Some((my * md.sqrt(), ey + ed / 2))
            }
            _ => None,
        };
        let (m, e) = match term {
            None => (mx, ex),
            Some((mt, et)) => {
                let e = ex.max(et);
                let at = |m: f64, k: i64| scale2(m, k - e);
                (at(mx, ex) + at(mt, et), e)
            }
        };
        scale2(m / mz, e - ez)
    }

    /// The sign of the value.
    pub(crate) fn sign(&self) -> Ordering {
        sign1(&self.x, &self.y, &self.d)
    }
}

/// `m · 2^k` for any `k`, without an intermediate overflow or underflow
/// beyond what the final value itself has.
fn scale2(m: f64, k: i64) -> f64 {
    let k = k.clamp(-3000, 3000) as i32;
    let (a, b) = (k / 3, k - 2 * (k / 3));
    m * 2f64.powi(a) * 2f64.powi(a) * 2f64.powi(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dy(v: i64) -> Dyadic {
        Dyadic::from_f64(v as f64)
    }

    fn isqrt(n: u128) -> u128 {
        if n < 2 {
            return n;
        }
        let mut x = (n as f64).sqrt() as u128;
        while x * x > n {
            x -= 1;
        }
        while (x + 1) * (x + 1) <= n {
            x += 1;
        }
        x
    }

    /// An INDEPENDENT oracle for `sign(x + y√d)` on small integers: bracket
    /// `√d` between `⌊√(d·4^k)⌋/2^k` and the next step, widen `k` until the
    /// bracket excludes zero; a perfect square resolves exactly.
    fn oracle1(x: i64, y: i64, d: i64) -> Ordering {
        let r = isqrt(d as u128);
        if r * r == d as u128 {
            return (x + y * r as i64).cmp(&0);
        }
        for k in [16u32, 24, 30] {
            let s = isqrt((d as u128) << (2 * k)) as i128;
            let scale = 1i128 << k;
            let (lo, hi) = if y >= 0 {
                (y as i128 * s, y as i128 * (s + 1))
            } else {
                (y as i128 * (s + 1), y as i128 * s)
            };
            let base = x as i128 * scale;
            if base + lo > 0 {
                return Ordering::Greater;
            }
            if base + hi < 0 {
                return Ordering::Less;
            }
        }
        panic!("oracle could not separate {x} + {y}√{d}");
    }

    struct Rng(u64);

    impl Rng {
        fn int(&mut self, m: i64) -> i64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % (2 * m as u64 + 1)) as i64 - m
        }
    }

    #[test]
    fn sign1_agrees_with_an_independent_bracketing_oracle() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..20_000 {
            let (x, y, d) = (rng.int(5000), rng.int(300), rng.int(400).abs());
            assert_eq!(
                sign1(&dy(x), &dy(y), &dy(d)),
                oracle1(x, y, d),
                "{x} + {y}√{d}"
            );
        }
    }

    #[test]
    fn exact_cancellations_are_zero_and_near_misses_are_not() {
        // 3 − √9, 2√2 − √8, 7 − 5√2 (≈ −0.071), 99 − 70√2 (≈ 0.0051).
        assert_eq!(sign1(&dy(3), &dy(-1), &dy(9)), Ordering::Equal);
        assert_eq!(
            sign2(&dy(0), &dy(2), &dy(2), &dy(-1), &dy(8)),
            Ordering::Equal
        );
        assert_eq!(sign1(&dy(7), &dy(-5), &dy(2)), Ordering::Less);
        assert_eq!(sign1(&dy(99), &dy(-70), &dy(2)), Ordering::Greater);
        // 1 − √2 keeps its sign: the case naive squaring gets wrong.
        assert_eq!(sign1(&dy(1), &dy(-1), &dy(2)), Ordering::Less);
    }

    #[test]
    fn sign2_agrees_with_the_bracketing_oracle_on_two_radicals() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for _ in 0..20_000 {
            let (p, q, d, r, e) = (
                rng.int(3000),
                rng.int(60),
                rng.int(300).abs(),
                rng.int(60),
                rng.int(300).abs(),
            );
            // Oracle: bracket both radicals at 2^-30.
            let k = 30u32;
            let (sd, se) = (
                isqrt((d as u128) << (2 * k)) as i128,
                isqrt((e as u128) << (2 * k)) as i128,
            );
            let span = |c: i64, s: i128| {
                if c >= 0 {
                    (c as i128 * s, c as i128 * (s + 1))
                } else {
                    (c as i128 * (s + 1), c as i128 * s)
                }
            };
            let (a, b) = (span(q, sd), span(r, se));
            let base = (p as i128) << k;
            let (lo, hi) = (base + a.0 + b.0, base + a.1 + b.1);
            let got = sign2(&dy(p), &dy(q), &dy(d), &dy(r), &dy(e));
            if lo > 0 {
                assert_eq!(got, Ordering::Greater, "{p}+{q}√{d}+{r}√{e}");
            } else if hi < 0 {
                assert_eq!(got, Ordering::Less, "{p}+{q}√{d}+{r}√{e}");
            }
        }
    }

    #[test]
    fn quads_order_like_their_values() {
        // (1 + √2)/2 ≈ 1.207 > 6/5; √8/2 = √2 = (0 + 1·√2)/1.
        let a = Quad::new(dy(1), dy(1), dy(2), dy(2));
        let b = Quad::ratio(dy(6), dy(5));
        assert_eq!(a.cmp(&b), Ordering::Greater);
        assert_eq!(b.cmp(&a), Ordering::Less);
        let c = Quad::new(dy(0), dy(1), dy(8), dy(2));
        let d = Quad::new(dy(0), dy(1), dy(2), dy(1));
        assert_eq!(c.cmp(&d), Ordering::Equal);
        assert_eq!(
            Quad::new(dy(1), dy(-1), dy(2), dy(3)).sign(),
            Ordering::Less
        );
    }
}
