//! Exact positive-area overlap of two ideal butt-ended wall rectangles
//! (BEHAVIOR-SPEC R3.1.2, the `wall-area-overlap` row).
//!
//! # What "ideal" means, and why the drawn polygon is not it
//!
//! A wall's occupied rectangle is defined implicitly by its axis: the points
//! within `t/2` of the axis LINE whose projection lies within the segment.
//! [`crate::wall_polygon`] produces a drawn approximation of that shape — it
//! divides by `hypot`, so its corners are rounded and are not the ideal
//! rectangle's corners. Deciding an exact predicate from those corners would
//! answer a question about a shape the specification never described. Nothing
//! here reads it.
//!
//! # The test
//!
//! Two convex bodies have positive-area intersection exactly when their
//! projections overlap STRICTLY on every candidate separating axis. For two
//! rectangles the candidates are the four edge normals — `dA`, `dA^⊥`, `dB`,
//! `dB^⊥` — since opposite edges share a normal.
//!
//! Writing the tests with UNNORMALIZED axes keeps most of the arithmetic
//! polynomial. Along `dA` the wall's own half-extent is `mA/2` with no radical
//! at all; a radical enters only where a physical thickness must be measured
//! along a unit normal, which is why every one carries a `t/√m`. With
//! `S = b0 + b1 − a0 − a1` (twice the centre difference) and
//! `D = |dA·dB|`, `C = |dA×dB|`, the two tests against A's axes are
//!
//! ```text
//! |dA·S|      <  mA + D + tB·C/√mB
//! |dA×S|      <  tA·√mA + C + tB·D/√mB
//! ```
//!
//! and the remaining two are the same with A and B exchanged. All four strict:
//! rectangles that merely touch share no area, and R3.1.2 warns only on
//! positive area.
//!
//! # Removing the radicals
//!
//! Each is cleared by sign-aware squaring — isolate the term carrying the
//! radical, check the sign of what remains, and square. No factoring.
//!
//! The result is a polynomial of TOTAL degree at most 12, counting a
//! thickness as the length it is: at most four of the twelve factors are
//! thicknesses, and the extremes are `K⁴·mB²` (twelve coordinate factors, no
//! thickness) and `tA⁴·mA²·mB²` (four thicknesses, eight coordinates). It is
//! evaluated exactly over [`crate::dyadic`].
//!
//! Squaring is only valid when both sides are known non-negative, so each step
//! below states which side it has bounded and why. Getting that wrong is the
//! classic way an "exact" predicate becomes silently wrong on one branch.
//!
//! # Contract
//!
//! The caller guarantees finite coordinates within `COORD_BOUND_MM`, positive
//! thicknesses within `DIMENSION_CAP_MM`, and axes of non-zero length. Those
//! are the document's own validation rules; this module does not re-check
//! them, and `mA > 0`, `mB > 0`, `tA > 0`, `tB > 0` are relied on below.

use crate::dyadic::Dyadic;
use domowik_format::Point;

/// A 2-vector of exact values.
struct Vec2 {
    x: Dyadic,
    y: Dyadic,
}

impl Vec2 {
    fn dot(&self, other: &Self) -> Dyadic {
        self.x.mul(&other.x).add(&self.y.mul(&other.y))
    }

    /// The scalar cross product `self.x·other.y − self.y·other.x`.
    fn cross(&self, other: &Self) -> Dyadic {
        self.x.mul(&other.y).sub(&self.y.mul(&other.x))
    }
}

/// `q − p`, exactly.
fn difference(p: Point, q: Point) -> Vec2 {
    Vec2 {
        x: Dyadic::from_f64(q.x).sub(&Dyadic::from_f64(p.x)),
        y: Dyadic::from_f64(q.y).sub(&Dyadic::from_f64(p.y)),
    }
}

/// Whether the two ideal rectangles share positive area.
///
/// `false` for rectangles that merely touch along an edge or at a corner:
/// R3.1.2's row is about positive area, and every inequality below is strict.
///
/// See the module documentation for the contract the caller must satisfy.
pub(crate) fn overlap(a0: Point, a1: Point, ta: f64, b0: Point, b1: Point, tb: f64) -> bool {
    let da = difference(a0, a1);
    let db = difference(b0, b1);
    // S = (b0 + b1) − (a0 + a1), twice the difference of the two centres.
    // Formed as a single exact expression rather than as centres, which would
    // need a division by two — exact for a dyadic, but pointless work.
    let s = Vec2 {
        x: Dyadic::from_f64(b0.x)
            .add(&Dyadic::from_f64(b1.x))
            .sub(&Dyadic::from_f64(a0.x))
            .sub(&Dyadic::from_f64(a1.x)),
        y: Dyadic::from_f64(b0.y)
            .add(&Dyadic::from_f64(b1.y))
            .sub(&Dyadic::from_f64(a0.y))
            .sub(&Dyadic::from_f64(a1.y)),
    };

    let ma = da.dot(&da);
    let mb = db.dot(&db);
    // `D` and `C` are magnitudes: the tests are symmetric in the sign of the
    // dot and cross products, since a rectangle extends the same distance
    // either way along each of its own axes.
    let d = da.dot(&db).abs();
    let c = da.cross(&db).abs();
    let ta = Dyadic::from_f64(ta);
    let tb = Dyadic::from_f64(tb);

    along_axis(&da.dot(&s).abs(), &ma, &d, &c, &mb, &tb)
        && normal_axis(&da.cross(&s).abs(), &ma, &d, &c, &mb, &ta, &tb)
        // The same two tests against B's axes. `C` is unchanged — the cross
        // product's magnitude is symmetric — and `D` likewise; only the roles
        // of the two walls swap.
        && along_axis(&db.dot(&s).abs(), &mb, &d, &c, &ma, &ta)
        && normal_axis(&db.cross(&s).abs(), &mb, &d, &c, &ma, &tb, &ta)
}

/// `|dA·S| < mA + D + tB·C/√mB`, exactly.
///
/// One radical, so one squaring. `K = |dA·S| − mA − D` is the whole left side
/// once the radical-free terms move across; the right side `tB·C/√mB` is
/// non-negative because `tB > 0`, `C ≥ 0` and `mB > 0`.
///
/// - `K < 0` ⇒ true, since the right side cannot be negative.
/// - otherwise both sides are non-negative, so the inequality survives
///   squaring: `K·√mB < tB·C` ⟺ `K²·mB < tB²·C²`.
///
/// `K = 0` needs no separate branch: it reduces to `0 < tB²C²`, which is true
/// exactly when `C > 0` — the axes are not parallel — and that is the right
/// answer, because a parallel pair touching end-on shares no area.
fn along_axis(
    lhs: &Dyadic,
    m_self: &Dyadic,
    d: &Dyadic,
    c: &Dyadic,
    m_other: &Dyadic,
    t_other: &Dyadic,
) -> bool {
    let k = lhs.sub(m_self).sub(d);
    if k.is_negative() {
        return true;
    }
    k.square().mul(m_other) < t_other.square().mul(&c.square())
}

/// `|dA×S| < tA·√mA + C + tB·D/√mB`, exactly.
///
/// Two radicals, so two squarings, and the second needs the first's residue to
/// have a known sign.
///
/// With `K = |dA×S| − C`:
///
/// - `K ≤ 0` ⇒ true. The right side exceeds `C` strictly, because `tA > 0`
///   and `mA > 0` make `tA·√mA` strictly positive.
/// - otherwise `K > 0`, and squaring `K < tA√mA + tB·D/√mB` then multiplying
///   by `mB > 0` gives
///   `E = K²·mB − tA²·mA·mB − tB²·D² < 2·tA·tB·D·√(mA·mB)`.
///   The right side is non-negative (`tA, tB > 0`, `D ≥ 0`, `mA, mB > 0`), so:
///   - `E < 0` ⇒ true;
///   - `E = 0` ⇒ true exactly when the right side is strictly positive, which
///     with `tA, tB > 0` and `mA, mB > 0` means `D > 0`;
///   - `E > 0` ⇒ both sides non-negative, square again:
///     `E² < 4·tA²·tB²·mA·mB·D²`.
fn normal_axis(
    lhs: &Dyadic,
    m_self: &Dyadic,
    d: &Dyadic,
    c: &Dyadic,
    m_other: &Dyadic,
    t_self: &Dyadic,
    t_other: &Dyadic,
) -> bool {
    let k = lhs.sub(c);
    if k.is_negative() || k.is_zero() {
        return true;
    }
    let e = k
        .square()
        .mul(m_other)
        .sub(&t_self.square().mul(m_self).mul(m_other))
        .sub(&t_other.square().mul(&d.square()));
    if e.is_negative() {
        return true;
    }
    if e.is_zero() {
        return d.is_positive();
    }
    let four = Dyadic::from_f64(4.0);
    e.square()
        < four
            .mul(&t_self.square())
            .mul(&t_other.square())
            .mul(m_self)
            .mul(m_other)
            .mul(&d.square())
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    /// Every reading of one pair: each axis reversed, and the walls swapped.
    /// The predicate is about two undirected rectangles, so all eight must
    /// agree.
    #[track_caller]
    fn assert_overlap(a: (Point, Point, f64), b: (Point, Point, f64), expected: bool, why: &str) {
        for (a0, a1) in [(a.0, a.1), (a.1, a.0)] {
            for (b0, b1) in [(b.0, b.1), (b.1, b.0)] {
                assert_eq!(overlap(a0, a1, a.2, b0, b1, b.2), expected, "{why}");
                assert_eq!(
                    overlap(b0, b1, b.2, a0, a1, a.2),
                    expected,
                    "{why} (swapped)"
                );
            }
        }
    }

    #[test]
    fn parallel_strips_touching_edge_to_edge_share_no_area() {
        // y in [-100, 100] against y in [100, 300]: they meet exactly on the
        // line y = 100. Touching is not positive area.
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(0.0, 200.0), P(1000.0, 200.0), 200.0),
            false,
            "edge-to-edge contact is not overlap",
        );
    }

    #[test]
    fn one_ulp_of_interpenetration_is_positive_area() {
        // The same pair with the upper strip lowered by a single ulp. The
        // shared region is 1000 mm long and one ulp high — vanishingly thin,
        // and unambiguously positive.
        let just_below = f64::from_bits(200f64.to_bits() - 1);
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(0.0, just_below), P(1000.0, just_below), 200.0),
            true,
            "one ulp of interpenetration is still area",
        );
        // And one ulp the other way is still a clean miss.
        let just_above = f64::from_bits(200f64.to_bits() + 1);
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(0.0, just_above), P(1000.0, just_above), 200.0),
            false,
            "one ulp of clearance is not overlap",
        );
    }

    #[test]
    fn collinear_strips_touching_end_to_end_share_no_area() {
        // Butt ends: R3.0.2's caps are square, so these meet on the segment
        // x = 1000 and share no area.
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(1000.0, 0.0), P(2000.0, 0.0), 200.0),
            false,
            "butt caps touching end to end",
        );
        // Pulled one ulp closer, they interpenetrate.
        let closer = f64::from_bits(1000f64.to_bits() - 1);
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(closer, 0.0), P(2000.0, 0.0), 200.0),
            true,
            "one ulp of collinear interpenetration",
        );
    }

    #[test]
    fn a_corner_touching_a_corner_shares_no_area() {
        // Perpendicular strips whose corners meet exactly at (1000, 100).
        assert_overlap(
            (P(0.0, 0.0), P(900.0, 0.0), 200.0),
            (P(1000.0, 200.0), P(1000.0, 1000.0), 200.0),
            false,
            "corner-to-corner contact",
        );
    }

    #[test]
    fn the_specifications_own_band_overlap_example_is_positive() {
        // R3.1.2's `wall-area-overlap` illustration: the axes meet only at
        // (2050, 0), outside W1's segment, so there is NO axis contact — and
        // the swept bands still overlap near W1's right cap.
        assert_overlap(
            (P(0.0, 0.0), P(2000.0, 0.0), 200.0),
            (P(850.0, 900.0), P(3250.0, -900.0), 200.0),
            true,
            "disjoint axes, overlapping bands",
        );
    }

    #[test]
    fn crossing_walls_and_clearly_separated_walls_answer_as_expected() {
        // A proper crossing certainly shares area.
        assert_overlap(
            (P(0.0, 0.0), P(4000.0, 0.0), 200.0),
            (P(2000.0, -1000.0), P(2000.0, 1000.0), 200.0),
            true,
            "crossing walls overlap",
        );
        // Far apart in both axes.
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(0.0, 5000.0), P(1000.0, 5000.0), 200.0),
            false,
            "separated walls do not overlap",
        );
        // Parallel, offset by exactly one thickness plus a millimetre.
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(0.0, 201.0), P(1000.0, 201.0), 200.0),
            false,
            "a millimetre of clearance",
        );
    }

    #[test]
    fn a_thin_wall_inside_a_thick_one_overlaps() {
        // Containment is overlap: every separating axis fails.
        assert_overlap(
            (P(0.0, 0.0), P(4000.0, 0.0), 1000.0),
            (P(1000.0, 100.0), P(3000.0, 100.0), 20.0),
            true,
            "a thin wall inside a thick one",
        );
    }

    #[test]
    fn subnormal_geometry_is_decided_rather_than_flushed_to_zero() {
        // Every coordinate and both thicknesses are subnormal, so every
        // product underflows f64 entirely. The shapes are the touching and
        // one-ulp cases above, scaled to where floating point has nothing
        // left.
        let eta = f64::from_bits(1);
        let touching = (P(0.0, 0.0), P(eta * 8.0, 0.0), eta * 2.0);
        assert_overlap(
            touching,
            (P(0.0, eta * 2.0), P(eta * 8.0, eta * 2.0), eta * 2.0),
            false,
            "subnormal strips touching exactly",
        );
        assert_overlap(
            touching,
            (P(0.0, eta), P(eta * 8.0, eta), eta * 2.0),
            true,
            "subnormal strips interpenetrating by one eta",
        );
    }

    #[test]
    fn mixed_scales_do_not_collapse_the_decision() {
        // A wall at the coordinate bound against a subnormally thin one: the
        // products span the whole exponent range in a single expression.
        let eta = f64::from_bits(1);
        assert_overlap(
            (P(-1.0e7, 0.0), P(1.0e7, 0.0), 1.0e6),
            (P(0.0, 0.0), P(1.0e7, 0.0), eta),
            true,
            "a subnormally thin wall lying inside a vast one",
        );
        // The same thin wall placed just outside the thick one's face. The
        // thick wall spans y in [-500000, 500000]; put the thin one above it.
        assert_overlap(
            (P(-1.0e7, 0.0), P(1.0e7, 0.0), 1.0e6),
            (P(0.0, 500001.0), P(1.0e7, 500001.0), eta),
            false,
            "and just clear of its face",
        );
    }

    #[test]
    fn near_parallel_walls_at_a_shallow_angle_are_decided_exactly() {
        // C is tiny but non-zero, so the parallel shortcut does not apply and
        // both squarings run on nearly-cancelling quantities.
        let tilt = f64::from_bits(1000f64.to_bits() + 1) - 1000.0;
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(0.0, 200.0), P(1000.0, 200.0 + tilt), 200.0),
            false,
            "a one-ulp tilt does not create overlap where touching held",
        );
        assert_overlap(
            (P(0.0, 0.0), P(1000.0, 0.0), 200.0),
            (P(0.0, 200.0 - tilt), P(1000.0, 200.0), 200.0),
            true,
            "tilting the near end down does",
        );
    }
}
