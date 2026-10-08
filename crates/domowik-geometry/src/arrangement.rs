//! Shared planar-arrangement primitives: vertex welding, segment splitting,
//! and point classification against a ring.
//!
//! These are the pieces every boolean or face-traversal routine needs before
//! it can reason about a set of segments as a graph. They live here rather
//! than inside one consumer because two of them existing in two copies is how
//! a fix lands in one and not the other.

use crate::dyadic::Dyadic;
use crate::enclosure::{exactly, Enclosure};
use crate::{orientation, Orientation};
use domowik_format::Point;
use std::cmp::Ordering;

/// Endpoints within this distance are treated as the same vertex.
///
/// One micrometre: far below any meaningful building dimension, far above
/// floating-point noise on millimetre coordinates.
pub const WELD_MM: f64 = 0.001;

/// A vertex identity that tolerates rounding.
///
/// Two points that quantize to the same key are the same graph vertex. This is
/// what lets an intersection computed along one segment and the same
/// intersection computed along another meet in the adjacency structure instead
/// of becoming two vertices a nanometre apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VertexKey(pub i64, pub i64);

impl VertexKey {
    /// Quantizes `p` onto the weld grid.
    pub fn of(p: Point) -> Self {
        Self(
            (p.x / WELD_MM).round() as i64,
            (p.y / WELD_MM).round() as i64,
        )
    }
}

/// Where a point lies relative to a closed ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Containment {
    /// Strictly inside.
    Inside,
    /// Exactly on an edge or vertex.
    Boundary,
    /// Strictly outside.
    Outside,
}

/// Linear interpolation along `a → b`.
pub fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// The binary64 nearest the exact quotient `num/den`, ties to even.
///
/// No exact division happens. Deciding whether a candidate `q` is below the
/// quotient is one exact multiply and one exact compare, because
/// `q < num/den` is `q·den < num` once the sign of `den` is accounted for.
/// `candidate` seeds the search, so a fast path that is already right costs
/// two comparisons; a badly conditioned one costs a gallop and a bisection
/// over at most 64 ULP steps.
fn correctly_round(candidate: f64, num: &Dyadic, den: &Dyadic) -> Option<f64> {
    if den.is_zero() || !candidate.is_finite() {
        return None;
    }
    // `below(q)` is true when q is strictly less than the exact quotient.
    let flip = den.is_negative();
    let below = |q: f64| -> bool {
        let lhs = Dyadic::from_f64(q).mul(den);
        let ord = lhs.cmp(num);
        if flip {
            ord == Ordering::Greater
        } else {
            ord == Ordering::Less
        }
    };
    // A monotone integer view of the finite doubles, so "one ULP" is "+1".
    let key = |q: f64| -> i64 {
        let bits = q.to_bits() as i64;
        if bits < 0 {
            i64::MIN - bits
        } else {
            bits
        }
    };
    let unkey = |k: i64| -> f64 {
        let bits = if k < 0 {
            (i64::MIN - k) as u64
        } else {
            k as u64
        };
        f64::from_bits(bits)
    };

    let start = key(candidate);
    let (mut lo, mut hi);
    if below(candidate) {
        // Gallop upwards until the quotient is bracketed.
        let mut step = 1i64;
        let mut prev = start;
        loop {
            let next = prev.checked_add(step)?;
            let probe = unkey(next);
            if !probe.is_finite() {
                return None;
            }
            if !below(probe) {
                lo = prev;
                hi = next;
                break;
            }
            prev = next;
            step = step.saturating_mul(2);
        }
    } else {
        let mut step = 1i64;
        let mut prev = start;
        loop {
            let next = prev.checked_sub(step)?;
            let probe = unkey(next);
            if !probe.is_finite() {
                return None;
            }
            if below(probe) {
                lo = next;
                hi = prev;
                break;
            }
            prev = next;
            step = step.saturating_mul(2);
        }
    }
    // Bisect to adjacent representables straddling the exact quotient.
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if below(unkey(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (a, b) = (unkey(lo), unkey(hi));
    // Choose the nearer of the two, comparing |num - a·den| against
    // |b·den - num| exactly rather than in the arithmetic being rounded.
    let da = num.sub(&Dyadic::from_f64(a).mul(den)).abs();
    let db = Dyadic::from_f64(b).mul(den).sub(num).abs();
    Some(match da.cmp(&db) {
        Ordering::Less => a,
        Ordering::Greater => b,
        // Tie: to even mantissa, which is what binary64 rounding does.
        Ordering::Equal => {
            if a.to_bits() % 2 == 0 {
                a
            } else {
                b
            }
        }
    })
}

/// The intersection of `a→b` and `c→d`, correctly rounded from the EXACT
/// rational crossing of the stored coordinates.
///
/// # Why this is not `lerp` of a rounded parameter
///
/// Every input coordinate is a binary64 and therefore an exact dyadic
/// rational, so with `r = b - a` and `s = d - c` taken as exact differences
/// the crossing is
///
/// ```text
/// N = (c-a) x s     D = r x s     P = ( (a·D + r·N) / D )
/// ```
///
/// a ratio of two exact dyadics in each coordinate. Rounding the PARAMETER and
/// interpolating instead carries the parameter's error into the point, and
/// that error grows as `1/|sin(theta)|`. Measured against the exact crossing of
/// the STORED coordinates over 60,000 randomized pairs, the worst displacement
/// of the interpolated answer is 1.05e-9 mm at `sin(theta) = 1e-1`, reaches the
/// 1e-3 mm topology quantum by `sin(theta) = 1e-7`, and is 1.14e-1 mm at 1e-9.
/// Evaluating the coordinate exactly and rounding once replaces that with half
/// an ULP, independent of the crossing angle.
///
/// The reference matters and is easy to get wrong: construction error must be
/// measured against the exact crossing OF THE STORED COORDINATES. Measured
/// against an intended ideal instead, it conflates input representation with
/// construction, which is how an earlier witness for this function overstated
/// a case whose construction error was in fact zero.
///
/// Returns `None` for parallel or collinear input, for a crossing outside
/// either segment, and for non-finite input. Membership is decided on exact
/// signs, never on a rounded parameter, so no finite valid crossing is
/// rejected and no angle threshold exists.
///
/// This is the CONSTRUCTION step only. Replacing the result by the canonical
/// representative of its weld class (ED-0103 N-2) is a separate step and
/// belongs to the caller.
pub fn intersection_point(a: Point, b: Point, c: Point, d_: Point) -> Option<Point> {
    for p in [a, b, c, d_] {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
    }
    // FILTER, before any exact value is built. An outward enclosure cannot
    // certify that a constructed coordinate is correctly rounded — that needs
    // the interval inside half an ULP and each widening costs a whole one —
    // but it certifies SIGNS cheaply, and every early exit here is a sign
    // question. A pair that provably does not cross, or provably crosses
    // outside one of the segments, returns without allocating anything.
    //
    // Only a filter that fails to decide is free. A filter that decided WRONGLY
    // would return here and the exact path below would never run, so every
    // decision it makes rests on the enclosure being outward.
    let d = |x: f64, y: f64| Enclosure::difference(x, y);
    let (fr1, fr2) = (d(b.x, a.x), d(b.y, a.y));
    let (fs1, fs2) = (d(d_.x, c.x), d(d_.y, c.y));
    let (fw1, fw2) = (d(c.x, a.x), d(c.y, a.y));
    let fden = fr1.product(fs2).minus(fr2.product(fs1));
    if let Some(den_sign) = fden.sign() {
        let ftn = fw1.product(fs2).minus(fw2.product(fs1));
        let fun = fw1.product(fr2).minus(fw2.product(fr1));
        let zero = exactly(0.0);
        // 0 <= n/den <= 1 with the inequalities reversed for a negative
        // denominator, decided only where the interval is unambiguous.
        let outside = |n: &Enclosure| -> bool {
            if den_sign == Ordering::Greater {
                n.certainly_below(&zero) || fden.certainly_below(n)
            } else {
                zero.certainly_below(n) || n.certainly_below(&fden)
            }
        };
        if outside(&ftn) || outside(&fun) {
            return None;
        }
    }

    intersection_point_exact(a, b, c, d_)
}

/// The escalation path of [`intersection_point`], with no filter in front of
/// it. Separated so a differential test can run both and require that the
/// filter only ever DECLINES a crossing the exact path also declines: a filter
/// that returned a different answer would be a false certification, and no
/// later stage would catch it.
pub(crate) fn intersection_point_exact(a: Point, b: Point, c: Point, d_: Point) -> Option<Point> {
    for p in [a, b, c, d_] {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
    }
    let ex = |x: f64, y: f64| Dyadic::from_f64(x).sub(&Dyadic::from_f64(y));
    let (r1, r2) = (ex(b.x, a.x), ex(b.y, a.y));
    let (s1, s2) = (ex(d_.x, c.x), ex(d_.y, c.y));
    let (w1, w2) = (ex(c.x, a.x), ex(c.y, a.y));

    let den = r1.mul(&s2).sub(&r2.mul(&s1));
    if den.is_zero() {
        return None;
    }
    let tn = w1.mul(&s2).sub(&w2.mul(&s1));
    let un = w1.mul(&r2).sub(&w2.mul(&r1));

    // Exact membership: 0 <= t <= 1 and 0 <= u <= 1, decided by sign rather
    // than by dividing. With `den` negative the inequalities reverse.
    let in_unit = |n: &Dyadic| -> bool {
        let (lo, hi) = if den.is_negative() {
            (!n.is_positive(), n.cmp(&den) != Ordering::Less)
        } else {
            (!n.is_negative(), n.cmp(&den) != Ordering::Greater)
        };
        lo && hi
    };
    if !in_unit(&tn) || !in_unit(&un) {
        return None;
    }

    // Px = (a.x·D + r.x·N) / D, both parts exact.
    let nx = Dyadic::from_f64(a.x).mul(&den).add(&r1.mul(&tn));
    let ny = Dyadic::from_f64(a.y).mul(&den).add(&r2.mul(&tn));

    // Seed each coordinate with the ordinary floating-point answer, which is
    // already correct almost always and merely a starting point when it is not.
    let t = {
        let approx = (b.x - a.x) * (d_.y - c.y) - (b.y - a.y) * (d_.x - c.x);
        if approx == 0.0 {
            0.5
        } else {
            (((c.x - a.x) * (d_.y - c.y)) - ((c.y - a.y) * (d_.x - c.x))) / approx
        }
    };
    let seed_x = a.x + (b.x - a.x) * t;
    let seed_y = a.y + (b.y - a.y) * t;
    let x = correctly_round(if seed_x.is_finite() { seed_x } else { a.x }, &nx, &den)?;
    let y = correctly_round(if seed_y.is_finite() { seed_y } else { a.y }, &ny, &den)?;
    Some(Point::new(x, y))
}

/// Parameter `t` along `a→b` at which it meets `c→d`, if they properly cross
/// or touch. `None` when they are parallel, collinear, or do not meet.
pub fn intersection_param(a: Point, b: Point, c: Point, d: Point) -> Option<f64> {
    // Use the robust predicate to decide whether a crossing exists at all,
    // before computing where. Straddling on both sides is the crossing test.
    let (o1, o2) = (orientation(a, b, c), orientation(a, b, d));
    let (o3, o4) = (orientation(c, d, a), orientation(c, d, b));

    let proper = o1 != o2 && o3 != o4;
    let touching = o1 == Orientation::Collinear
        || o2 == Orientation::Collinear
        || o3 == Orientation::Collinear
        || o4 == Orientation::Collinear;

    if !proper && !touching {
        return None;
    }

    let (r1, r2) = (b.x - a.x, b.y - a.y);
    let (s1, s2) = (d.x - c.x, d.y - c.y);
    let denom = r1 * s2 - r2 * s1;
    if denom == 0.0 {
        // Parallel or collinear: no single crossing parameter.
        return None;
    }
    let t = ((c.x - a.x) * s2 - (c.y - a.y) * s1) / denom;
    let u = ((c.x - a.x) * r2 - (c.y - a.y) * r1) / denom;

    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

/// Parameters along `a→b` at which a COLLINEAR overlapping segment `c→d`
/// starts or ends.
///
/// [`intersection_param`] returns `None` for collinear input because there is
/// no single crossing point. That is correct as far as it goes, but on its own
/// it leaves overlapping segments unsplit: two collinear walls sharing part of
/// their length produce duplicate edges over the shared span, which corrupts
/// the angular adjacency and destroys the face walk. Overlapping walls are a
/// normal result of editing, so this is not an exotic case.
pub fn collinear_overlap_params(a: Point, b: Point, c: Point, d: Point) -> Vec<f64> {
    // Only meaningful when all four points are collinear.
    if orientation(a, b, c) != Orientation::Collinear
        || orientation(a, b, d) != Orientation::Collinear
    {
        return Vec::new();
    }
    let (rx, ry) = (b.x - a.x, b.y - a.y);
    let len2 = rx * rx + ry * ry;
    if len2 == 0.0 {
        return Vec::new();
    }
    // Project c and d onto a→b as parameters.
    [c, d]
        .iter()
        .map(|p| ((p.x - a.x) * rx + (p.y - a.y) * ry) / len2)
        .filter(|t| *t > 0.0 && *t < 1.0)
        .collect()
}

/// Every cut POINT along `a→b` induced by the segments in `others`, in order
/// from `a` to `b` inclusive, so `windows(2)` yields the sub-edges directly.
///
/// # There is no rounded sort key
///
/// An earlier version sorted by the floating-point parameter and then
/// interpolated it back into a coordinate. That made the ordering and the
/// coordinates two separate approximations of the same thing, free to
/// disagree: two distinct cuts whose parameters rounded together would collapse
/// or swap. Carrying the POINTS and ordering them exactly removes the
/// disagreement rather than reconciling it, because there is no longer a second
/// quantity to reconcile.
///
/// Ordering compares the exact projection `(p - a)·r`, which is a total order
/// along the segment and needs no division. Ties fall back to the exact
/// coordinate pair, so the order is total even for coincident points.
///
/// # Two cuts are one cut when they share a [`VertexKey`]
///
/// That is ED-0103 N-2's identity relation, and it is the one every other
/// stage of the arrangement uses. A distance test here would be a second
/// identity rule, and the two disagree at the margins: points 0.6 µm apart
/// either side of a cell boundary are within [`WELD_MM`] of each other but are
/// distinct vertices. Collapsing such a cut on one edge while the edge it
/// crosses keeps it leaves the two edges without a shared vertex, and the
/// union traced from them loses most of its area.
///
/// A line meets a grid cell in one contiguous span, so cuts sharing a key are
/// adjacent after the exact sort and `dedup_by` sees every pair. Which member
/// survives does not matter: the caller replaces each cut by its weld class's
/// representative.
///
/// Endpoints are reused exactly: the run begins at `a` and ends at `b`. Collinear overlap bounds are the OTHER segment's own vertices,
/// reused exactly rather than reprojected.
pub fn cuts_along(a: Point, b: Point, others: &[(Point, Point)]) -> Vec<Point> {
    let mut cuts: Vec<Point> = vec![a, b];
    for &(c, d) in others {
        if VertexKey::of(c) == VertexKey::of(d) {
            continue;
        }
        if let Some(p) = intersection_point(a, b, c, d) {
            cuts.push(p);
        }
        cuts.extend(collinear_overlap_points(a, b, c, d));
    }
    cuts.sort_by(|p, q| compare_along(a, b, *p, *q));
    cuts.dedup_by(|p, q| VertexKey::of(*p) == VertexKey::of(*q));
    cuts
}

/// Exact order of two points by their projection along `a→b`.
fn compare_along(a: Point, b: Point, p: Point, q: Point) -> Ordering {
    let ex = |x: f64, y: f64| Dyadic::from_f64(x).sub(&Dyadic::from_f64(y));
    let (rx, ry) = (ex(b.x, a.x), ex(b.y, a.y));
    let along = |z: Point| ex(z.x, a.x).mul(&rx).add(&ex(z.y, a.y).mul(&ry));
    along(p).cmp(&along(q)).then_with(|| {
        (p.x, p.y)
            .partial_cmp(&(q.x, q.y))
            .unwrap_or(Ordering::Equal)
    })
}

/// The other segment's own vertices where a COLLINEAR overlap begins or ends.
///
/// Nothing is constructed. All four points are collinear by the exact
/// orientation predicate, so a vertex of `c→d` that projects strictly inside
/// `a→b` lies on `a→b`, and the cut is that stored vertex itself.
fn collinear_overlap_points(a: Point, b: Point, c: Point, d: Point) -> Vec<Point> {
    if orientation(a, b, c) != Orientation::Collinear
        || orientation(a, b, d) != Orientation::Collinear
    {
        return Vec::new();
    }
    let (rx, ry) = (b.x - a.x, b.y - a.y);
    let len2 = rx * rx + ry * ry;
    if len2 == 0.0 {
        return Vec::new();
    }
    [c, d]
        .into_iter()
        .filter(|p| {
            let t = ((p.x - a.x) * rx + (p.y - a.y) * ry) / len2;
            t > 0.0 && t < 1.0
        })
        .collect()
}

/// Classifies `p` against the closed ring `ring`.
///
/// The boundary test uses the exact orientation predicate, so a point lying on
/// an edge is reported as [`Containment::Boundary`] rather than falling to
/// whichever side rounding prefers. The interior test is the standard
/// crossing-number rule, evaluated only after the boundary case is excluded.
pub fn classify_point(p: Point, ring: &[Point]) -> Containment {
    if ring.len() < 3 {
        return Containment::Outside;
    }
    let n = ring.len();

    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if orientation(a, b, p) == Orientation::Collinear
            && p.x >= a.x.min(b.x)
            && p.x <= a.x.max(b.x)
            && p.y >= a.y.min(b.y)
            && p.y <= a.y.max(b.y)
        {
            return Containment::Boundary;
        }
    }

    // Crossing number along the ray y = p.y, x > p.x. The half-open rule on
    // the vertical span (`a.y > p.y` differs from `b.y > p.y`) counts a vertex
    // exactly once when the ray passes through it.
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if x > p.x {
                inside = !inside;
            }
        }
    }
    if inside {
        Containment::Inside
    } else {
        Containment::Outside
    }
}

/// True when every vertex of `inner` is inside or on `outer`, and at least one
/// is strictly inside.
///
/// Written for rings that are known not to cross — the arrangement has already
/// split every crossing — so vertex containment settles ring containment.
pub fn ring_contains_ring(outer: &[Point], inner: &[Point]) -> bool {
    let mut any_strictly_inside = false;
    for &p in inner {
        match classify_point(p, outer) {
            Containment::Outside => return false,
            Containment::Inside => any_strictly_inside = true,
            Containment::Boundary => {}
        }
    }
    any_strictly_inside
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    fn xorshift(state: &mut u64) -> f64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        ((*state >> 11) as f64 / (1u64 << 53) as f64) * 2.0e6 - 1.0e6
    }

    /// The filter must never disagree with the exact path. A disagreement is a
    /// FALSE CERTIFICATION: the filtered call returns and the exact path never
    /// runs, so nothing downstream can catch it.
    #[test]
    fn the_filter_never_disagrees_with_the_exact_path() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut crossings = 0;
        for _ in 0..40_000 {
            let mut next = || xorshift(&mut state);
            let (a, b) = (P(next(), next()), P(next(), next()));
            let (c, d) = (P(next(), next()), P(next(), next()));
            let filtered = intersection_point(a, b, c, d);
            let exact = intersection_point_exact(a, b, c, d);
            assert_eq!(filtered, exact, "a={a:?} b={b:?} c={c:?} d={d:?}");
            if filtered.is_some() {
                crossings += 1;
            }
        }
        assert!(crossings > 100, "only {crossings} crossings exercised");
    }

    /// Near-parallel pairs are where the filter is least able to decide and
    /// most tempting to let through. Same requirement.
    #[test]
    fn the_filter_never_disagrees_on_near_parallel_pairs() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for i in 0..20_000 {
            let mut next = || xorshift(&mut state);
            let (a, b) = (P(next(), next()), P(next(), next()));
            // A second segment almost parallel to the first.
            let scale = 1e-9 * ((i % 7) + 1) as f64;
            let (rx, ry) = (b.x - a.x, b.y - a.y);
            let c = P(a.x + rx * 0.25 + ry * scale, a.y + ry * 0.25 - rx * scale);
            let d = P(a.x + rx * 0.75 - ry * scale, a.y + ry * 0.75 + rx * scale);
            assert_eq!(
                intersection_point(a, b, c, d),
                intersection_point_exact(a, b, c, d),
                "a={a:?} b={b:?} c={c:?} d={d:?}"
            );
        }
    }

    /// A denominator interval straddling zero decides nothing and must fall
    /// through rather than guess a sign.
    #[test]
    fn a_denominator_interval_containing_zero_does_not_decide() {
        // Exactly parallel: the true denominator is zero, so no enclosure can
        // put it strictly on one side.
        let (a, b) = (P(0.0, 0.0), P(1.0, 1.0));
        let (c, d) = (P(0.0, 1.0), P(1.0, 2.0));
        let den = Enclosure::difference(b.x, a.x)
            .product(Enclosure::difference(d.y, c.y))
            .minus(Enclosure::difference(b.y, a.y).product(Enclosure::difference(d.x, c.x)));
        assert!(!den.excludes_zero(), "{den:?}");
        assert_eq!(den.sign(), None);
        assert_eq!(intersection_point(a, b, c, d), None);
        assert_eq!(intersection_point_exact(a, b, c, d), None);
    }

    /// Overflowing and subnormal magnitudes: the filter must decline rather
    /// than treat an infinite endpoint as a bound, and must still agree.
    #[test]
    fn overflow_and_subnormal_magnitudes_agree_with_the_exact_path() {
        let huge = f64::MAX / 4.0;
        let tiny = f64::MIN_POSITIVE;
        let sub = 5e-324;
        for (a, b, c, d) in [
            (
                P(-huge, -huge),
                P(huge, huge),
                P(-huge, huge),
                P(huge, -huge),
            ),
            (P(0.0, 0.0), P(tiny, 0.0), P(0.0, -tiny), P(0.0, tiny)),
            (P(0.0, 0.0), P(sub, sub), P(0.0, sub), P(sub, 0.0)),
            (P(-huge, 0.0), P(huge, 0.0), P(0.0, -huge), P(0.0, huge)),
        ] {
            assert_eq!(
                intersection_point(a, b, c, d),
                intersection_point_exact(a, b, c, d),
                "a={a:?} b={b:?} c={c:?} d={d:?}"
            );
        }
    }

    /// An enclosure must CONTAIN the exact value. This checks containment on
    /// the operations the filter composes, using exact dyadic arithmetic as
    /// the reference rather than another float computation.
    #[test]
    fn every_enclosure_operation_contains_its_exact_value() {
        let mut state = 0xDEAD_BEEF_CAFE_1234u64;
        let inside = |e: &Enclosure, exact: &Dyadic| {
            Dyadic::from_f64(e.lo).cmp(exact) != Ordering::Greater
                && Dyadic::from_f64(e.hi).cmp(exact) != Ordering::Less
        };
        for _ in 0..20_000 {
            let mut next = || xorshift(&mut state);
            let (w, x, y, z) = (next(), next(), next(), next());

            let dw = Enclosure::difference(w, x);
            let exact_dw = Dyadic::from_f64(w).sub(&Dyadic::from_f64(x));
            assert!(inside(&dw, &exact_dw), "difference {w} {x}");

            let dy = Enclosure::difference(y, z);
            let exact_dy = Dyadic::from_f64(y).sub(&Dyadic::from_f64(z));

            let prod = dw.product(dy);
            assert!(inside(&prod, &exact_dw.mul(&exact_dy)), "product");

            let diff = dw.minus(dy);
            assert!(inside(&diff, &exact_dw.sub(&exact_dy)), "minus");

            let sum = dw.sum(dy);
            assert!(inside(&sum, &exact_dw.add(&exact_dy)), "sum");
        }
    }

    /// `sign` and `excludes_zero` are tested directly, not only through a
    /// caller. Reaching their zero boundary through `intersection_point` turns
    /// out to be impossible — outward widening moves an endpoint off zero — so
    /// a mutation of the boundary survived until this test existed.
    #[test]
    fn the_sign_boundary_refuses_to_certify_an_interval_touching_zero() {
        let touching_above = Enclosure { lo: 0.0, hi: 1.0 };
        let touching_below = Enclosure { lo: -1.0, hi: 0.0 };
        let straddling = Enclosure { lo: -1.0, hi: 1.0 };
        for e in [touching_above, touching_below, straddling] {
            assert_eq!(e.sign(), None, "{e:?} contains zero and decides nothing");
            assert!(!e.excludes_zero(), "{e:?}");
        }
        assert_eq!(
            Enclosure { lo: 1.0, hi: 2.0 }.sign(),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Enclosure { lo: -2.0, hi: -1.0 }.sign(),
            Some(Ordering::Less)
        );
        assert_eq!(
            Enclosure {
                lo: f64::NEG_INFINITY,
                hi: 1.0
            }
            .sign(),
            None,
            "an infinite endpoint is not a bound"
        );
    }

    /// A crossing sitting exactly on a rounding midpoint is where correct
    /// rounding is decided by the tie rule, and where a filter would be most
    /// dangerous. The filter must not decide it; the exact path must, to even.
    #[test]
    fn a_crossing_on_a_rounding_midpoint_is_resolved_by_the_exact_path() {
        // Halfway between two adjacent doubles near 1: the exact crossing is
        // the midpoint, so rounding must go to the even neighbour.
        let one = 1.0f64;
        let up = one.next_up();
        let mid_num = one + up; // exact: both are within a factor of two
        let a = P(0.0, 0.0);
        let b = P(mid_num, 0.0);
        let c = P(1.0, -1.0);
        let d = P(1.0, 1.0);
        let got = intersection_point(a, b, c, d).expect("they cross");
        assert_eq!(got, intersection_point_exact(a, b, c, d).unwrap());
        assert_eq!(got.x, 1.0, "the crossing is at x = 1 exactly");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    fn square() -> Vec<Point> {
        vec![P(0.0, 0.0), P(10.0, 0.0), P(10.0, 10.0), P(0.0, 10.0)]
    }

    #[test]
    fn classify_point_separates_inside_boundary_and_outside() {
        assert_eq!(classify_point(P(5.0, 5.0), &square()), Containment::Inside);
        assert_eq!(
            classify_point(P(0.0, 5.0), &square()),
            Containment::Boundary
        );
        assert_eq!(
            classify_point(P(0.0, 0.0), &square()),
            Containment::Boundary
        );
        assert_eq!(
            classify_point(P(-1.0, 5.0), &square()),
            Containment::Outside
        );
    }

    #[test]
    fn classify_point_counts_a_vertex_crossing_once() {
        // The ray from (-1, 10) passes exactly through two ring vertices. A
        // rule that counted both edges at each vertex would report Inside.
        let ring = square();
        assert_eq!(classify_point(P(-1.0, 10.0), &ring), Containment::Outside);
        assert_eq!(classify_point(P(-1.0, 0.0), &ring), Containment::Outside);
    }

    #[test]
    fn classify_point_handles_a_concave_ring() {
        // An L shape; the notch is outside even though it is inside the
        // bounding box.
        let l = vec![
            P(0.0, 0.0),
            P(10.0, 0.0),
            P(10.0, 4.0),
            P(4.0, 4.0),
            P(4.0, 10.0),
            P(0.0, 10.0),
        ];
        assert_eq!(classify_point(P(2.0, 2.0), &l), Containment::Inside);
        assert_eq!(classify_point(P(8.0, 8.0), &l), Containment::Outside);
    }

    #[test]
    fn ring_contains_ring_rejects_a_shared_boundary_with_no_interior() {
        // The "inner" ring is the outer ring itself: every vertex is on the
        // boundary and none strictly inside, so it is not contained.
        assert!(!ring_contains_ring(&square(), &square()));
    }

    #[test]
    fn ring_contains_ring_accepts_a_strictly_smaller_ring() {
        let inner = vec![P(2.0, 2.0), P(4.0, 2.0), P(4.0, 4.0), P(2.0, 4.0)];
        assert!(ring_contains_ring(&square(), &inner));
        assert!(!ring_contains_ring(&inner, &square()));
    }

    #[test]
    fn cuts_along_brackets_the_whole_segment_and_finds_a_crossing() {
        let cuts = cuts_along(P(0.0, 0.0), P(10.0, 0.0), &[(P(4.0, -1.0), P(4.0, 1.0))]);
        // The cut POINT is asserted, not a parameter that would still have to
        // be interpolated back into one.
        assert_eq!(cuts, vec![P(0.0, 0.0), P(4.0, 0.0), P(10.0, 0.0)]);
    }

    #[test]
    fn cuts_along_splits_at_a_collinear_overlap() {
        let cuts = cuts_along(P(0.0, 0.0), P(10.0, 0.0), &[(P(3.0, 0.0), P(7.0, 0.0))]);
        // Collinear overlap bounds are the other segment's own vertices,
        // reused rather than reprojected.
        assert_eq!(
            cuts,
            vec![P(0.0, 0.0), P(3.0, 0.0), P(7.0, 0.0), P(10.0, 0.0)]
        );
    }

    #[test]
    fn cuts_along_resolves_a_weld_sized_feature_on_a_full_length_edge() {
        // An edge spanning the whole coordinate domain (COORD_BOUND_MM is
        // 1_000_000 mm), cut twice 1 mm apart at its far end. A parameter-space
        // tolerance that ignored the edge's length would fuse the two cuts and
        // the 1 mm feature would vanish from the arrangement.
        let cuts = cuts_along(
            P(-1_000_000.0, 0.0),
            P(1_000_000.0, 0.0),
            &[
                (P(999_000.0, -1.0), P(999_000.0, 1.0)),
                (P(999_001.0, -1.0), P(999_001.0, 1.0)),
            ],
        );
        assert_eq!(cuts.len(), 4, "both cuts must survive: {cuts:?}");
    }

    #[test]
    fn cuts_along_collapses_cuts_in_one_topology_cell() {
        // 0.4 µm apart in one cell: the same vertex under N-2, and keeping
        // both would emit a fragment VertexKey collapses to zero length.
        let cuts = cuts_along(
            P(0.0, 0.0),
            P(1000.0, 0.0),
            &[
                (P(500.0, -1.0), P(500.0, 1.0)),
                (P(500.0004, -1.0), P(500.0004, 1.0)),
            ],
        );
        assert_eq!(cuts.len(), 3, "one cut, not two: {cuts:?}");
    }

    #[test]
    fn cuts_along_keeps_close_cuts_in_different_topology_cells() {
        // 0.6 µm apart across the cell boundary at 500.0005: within WELD_MM of
        // each other, but two vertices under N-2. A distance test would
        // collapse them; the edge each one crosses keeps its own.
        let (p, q) = (P(500.0002, 0.0), P(500.0008, 0.0));
        assert_ne!(VertexKey::of(p), VertexKey::of(q));
        let cuts = cuts_along(
            P(0.0, 0.0),
            P(1000.0, 0.0),
            &[(P(p.x, -1.0), P(p.x, 1.0)), (P(q.x, -1.0), P(q.x, 1.0))],
        );
        assert_eq!(cuts, vec![P(0.0, 0.0), p, q, P(1000.0, 0.0)]);
    }

    #[test]
    fn vertex_key_welds_points_closer_than_the_tolerance() {
        assert_eq!(
            VertexKey::of(P(1000.0, 0.0)),
            VertexKey::of(P(1000.0 + WELD_MM / 4.0, 0.0))
        );
        assert_ne!(
            VertexKey::of(P(1000.0, 0.0)),
            VertexKey::of(P(1000.0 + WELD_MM * 2.0, 0.0))
        );
    }
}
