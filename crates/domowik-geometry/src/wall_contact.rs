//! Pairwise axis contact classification (BEHAVIOR-SPEC R3.1.2).
//!
//! # Why this can be exact
//!
//! R3.1.2's classes are decided entirely by SIGN TESTS and by the order of
//! points along a line. Neither needs a constructed coordinate:
//!
//! - whether a point lies left of, right of, or on an axis is
//!   [`crate::orientation`], which is exact for every finite input;
//! - once four points are known collinear, their order along the line is the
//!   order of one of their raw coordinates, compared directly.
//!
//! So this module has no tolerance, no epsilon and no scaling. The one thing
//! it deliberately does NOT do is produce the crossing point: that is a
//! construction, it rounds, and returning it here would smuggle an inexact
//! value into an otherwise exact answer. [`AxisContact::ProperCrossing`]
//! names the class and leaves the point to whoever needs it.
//!
//! # What is NOT here
//!
//! R3.1.2's last row — occupied rectangles overlapping with positive area but
//! no axis contact — is about the swept BANDS, not the axes, and is not
//! implemented (issue 0055 tracks it). Nothing in this module reports it, and
//! [`AxisContact::Disjoint`] means "the axes do not meet", never "these walls
//! do not overlap".
//!
//! Admission policy is not here either. This module says what the contact IS;
//! which classes reject a batch is R3.1.2's precedence list, applied by the
//! layer that owns admission.

use crate::{orientation, Orientation};
use domowik_format::{Point, COORD_BOUND_MM};

/// Which of the two axes a classification refers to.
///
/// Argument order, not identity: `First` is the pair passed first to
/// [`axis_contact`]. Swapping the arguments swaps these, which is what makes
/// the result argument-order covariant rather than order-dependent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisRole {
    First,
    Second,
}

impl AxisRole {
    fn swapped(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }
}

/// How two wall axes meet (R3.1.2).
///
/// Exactly one class holds for any admissible pair, and the classes are
/// mutually exclusive rather than ranked here — R3.1.2's precedence list is
/// the admission layer's to apply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisContact {
    /// The axes share no point.
    ///
    /// About the AXES only. Two disjoint axes may still have overlapping
    /// occupied rectangles; that is R3.1.2's `wall-area-overlap` row and is
    /// not classified by this module.
    Disjoint,
    /// The axes meet at exactly one point, and it is an endpoint of BOTH.
    ///
    /// R3.1.2's endpoint–endpoint row: auto-join, no split. Reached both by
    /// non-collinear axes sharing an endpoint and by collinear axes touching
    /// end to end.
    EndpointTouch { at: Point },
    /// One axis's endpoint lies strictly inside the other, and the axes are
    /// NOT collinear.
    ///
    /// `split` names the axis whose interior contains `at` — the one R3.1.2
    /// splits. The other keeps its endpoint and is never split (F14b).
    EndpointOnInterior { split: AxisRole, at: Point },
    /// The interiors cross at one point, which is interior to both.
    ///
    /// The point is deliberately absent: computing it is a construction that
    /// rounds, and this type's contract is exactness.
    ProperCrossing,
    /// Collinear, sharing a sub-segment of positive length, but not identical.
    ///
    /// R3.1.2 rejects the batch (`collinear-overlap`).
    CollinearOverlap,
    /// The same axis, in either direction.
    ///
    /// R3.1.2 rejects the batch (`wall-coincidence`), and its precedence puts
    /// this ahead of [`Self::CollinearOverlap`] — which is why identity is
    /// reported as its own class rather than as the overlap it also is.
    Coincident,
}

/// Classifies how the axis `(a, b)` meets the axis `(c, d)` (R3.1.2).
///
/// # Exactness
///
/// Exact for every admissible input: no epsilon, no tolerance, no scaling.
/// Every decision is either [`crate::orientation`]'s exact sign or a direct
/// comparison of stored coordinates. Signed zeros compare equal, matching the
/// format's numeric policy, so `-0.0` and `0.0` name the same point.
///
/// # Invariance
///
/// Reversing either axis leaves the result unchanged. Swapping the two axes
/// leaves it unchanged except for [`AxisRole`], which follows the arguments
/// by construction. This is a property of the algorithm, not a normalization
/// step: nothing is sorted or canonicalized before classifying.
///
/// # Domain, and why `None`
///
/// `None` for any input this classifier must not answer for:
///
/// - a coordinate that is infinite or NaN — [`crate::orientation`]'s
///   exactness guarantee does not extend to those;
/// - a coordinate outside `COORD_BOUND_MM` — the predicate would still be
///   exact, but such a coordinate cannot be stored, so an answer would
///   describe a document that cannot exist;
/// - a zero-length axis — FORMAT-SPEC §3 requires `startMm != endMm`, and a
///   degenerate axis has no direction to classify against.
///
/// Every one of these is already a validation error, so `None` is unreachable
/// for walls read out of a validated document. It exists so that a classifier
/// never invents a class for geometry the format forbids. The check runs
/// before any arithmetic, so an out-of-domain pair costs one pass over four
/// points.
///
/// # Cost
///
/// At most four [`crate::orientation`] calls and a handful of comparisons, no
/// allocation. The collinear branch does no orientation work beyond the four
/// it has already paid for. See `benches/predicates.rs`.
pub fn axis_contact(a: Point, b: Point, c: Point, d: Point) -> Option<AxisContact> {
    let (a0, a1, b0, b1) = (a, b, c, d);
    for p in [a0, a1, b0, b1] {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        if p.x.abs() > COORD_BOUND_MM || p.y.abs() > COORD_BOUND_MM {
            return None;
        }
    }
    if same_point(a0, a1) || same_point(b0, b1) {
        return None;
    }

    let b0_vs_a = orientation(a0, a1, b0);
    let b1_vs_a = orientation(a0, a1, b1);
    let a0_vs_b = orientation(b0, b1, a0);
    let a1_vs_b = orientation(b0, b1, a1);

    // All four collinear: the whole question collapses to one dimension.
    if b0_vs_a == Orientation::Collinear
        && b1_vs_a == Orientation::Collinear
        && a0_vs_b == Orientation::Collinear
        && a1_vs_b == Orientation::Collinear
    {
        return Some(collinear_contact(a0, a1, b0, b1));
    }

    // A shared endpoint. Two non-collinear axes meet in at most one point, so
    // finding one here settles the classification outright.
    for (p, q) in [(a0, b0), (a0, b1), (a1, b0), (a1, b1)] {
        if same_point(p, q) {
            return Some(AxisContact::EndpointTouch { at: p });
        }
    }

    // An endpoint strictly inside the other axis. At most one can hold: if
    // an endpoint of each lay strictly inside the other, the single meeting
    // point would have to be an endpoint of both and interior to both at
    // once.
    let axis = Along::of(a0, a1);
    if b0_vs_a == Orientation::Collinear && axis.strictly_between(a0, a1, b0) {
        return Some(AxisContact::EndpointOnInterior {
            split: AxisRole::First,
            at: b0,
        });
    }
    if b1_vs_a == Orientation::Collinear && axis.strictly_between(a0, a1, b1) {
        return Some(AxisContact::EndpointOnInterior {
            split: AxisRole::First,
            at: b1,
        });
    }
    let other = Along::of(b0, b1);
    if a0_vs_b == Orientation::Collinear && other.strictly_between(b0, b1, a0) {
        return Some(AxisContact::EndpointOnInterior {
            split: AxisRole::Second,
            at: a0,
        });
    }
    if a1_vs_b == Orientation::Collinear && other.strictly_between(b0, b1, a1) {
        return Some(AxisContact::EndpointOnInterior {
            split: AxisRole::Second,
            at: a1,
        });
    }

    // Interiors cross exactly when each axis separates the other's endpoints.
    // Any collinear endpoint has already been handled above, so a strict sign
    // difference here means both crossings are interior.
    if b0_vs_a != b1_vs_a && a0_vs_b != a1_vs_b {
        return Some(AxisContact::ProperCrossing);
    }
    Some(AxisContact::Disjoint)
}

impl AxisContact {
    /// The class this would be with the two axes passed the other way round.
    ///
    /// Only [`AxisContact::EndpointOnInterior`] differs, and only in which
    /// axis it names. Exposed so a caller that normalizes a pair — by UUID,
    /// say — can restore the role without reclassifying.
    pub fn with_axes_swapped(self) -> Self {
        match self {
            Self::EndpointOnInterior { split, at } => Self::EndpointOnInterior {
                split: split.swapped(),
                at,
            },
            other => other,
        }
    }
}

/// Exact point identity. `==` on `f64` treats `-0.0` and `0.0` as equal,
/// which is what the format's numeric policy requires.
fn same_point(p: Point, q: Point) -> bool {
    p.x == q.x && p.y == q.y
}

/// Which raw coordinate orders points along a known-collinear line.
///
/// Projecting onto the direction would need a dot product, which rounds. For
/// points already known to be collinear, one of their own coordinates is a
/// faithful monotone parameter along the line — so ordering costs a
/// comparison and stays exact.
#[derive(Debug, Clone, Copy)]
enum Along {
    /// The line is not vertical; `x` increases or decreases monotonically.
    X,
    /// The line is vertical, so every point shares an `x` and `y` orders them.
    Y,
}

impl Along {
    fn of(p: Point, q: Point) -> Self {
        if p.x == q.x {
            Self::Y
        } else {
            Self::X
        }
    }

    fn key(self, p: Point) -> f64 {
        match self {
            Self::X => p.x,
            Self::Y => p.y,
        }
    }

    /// Whether collinear `p` lies strictly inside the segment `(s, e)`.
    fn strictly_between(self, s: Point, e: Point, p: Point) -> bool {
        let (lo, hi) = ordered(self.key(s), self.key(e));
        let k = self.key(p);
        lo < k && k < hi
    }
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// The four-points-collinear case, decided by interval arithmetic on one
/// coordinate.
fn collinear_contact(a0: Point, a1: Point, b0: Point, b1: Point) -> AxisContact {
    let along = Along::of(a0, a1);
    let (alo, ahi) = ordered(along.key(a0), along.key(a1));
    let (blo, bhi) = ordered(along.key(b0), along.key(b1));

    // Identity first: R3.1.2's precedence puts full coincidence ahead of
    // positive-length overlap, and the two are otherwise indistinguishable
    // from the overlap's length alone.
    if alo == blo && ahi == bhi {
        return AxisContact::Coincident;
    }

    let lo = if alo > blo { alo } else { blo };
    let hi = if ahi < bhi { ahi } else { bhi };
    if lo < hi {
        // A shared sub-segment of positive length.
        AxisContact::CollinearOverlap
    } else if lo == hi {
        // Exactly one shared point on collinear axes, which can only be an
        // endpoint of both: a collinear endpoint-to-INTERIOR touch would
        // force positive-length overlap, so it cannot arrive here.
        AxisContact::EndpointTouch {
            at: if along.key(a0) == lo { a0 } else { a1 },
        }
    } else {
        AxisContact::Disjoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    fn classify(a: (Point, Point), b: (Point, Point)) -> AxisContact {
        axis_contact(a.0, a.1, b.0, b.1).expect("admissible")
    }

    /// Every reversal and swap of one pair. R3.1.2 classifies an UNDIRECTED
    /// pair of undirected axes, so all eight readings must agree.
    fn every_reading(a: (Point, Point), b: (Point, Point)) -> Vec<AxisContact> {
        let mut out = Vec::new();
        for (a0, a1) in [(a.0, a.1), (a.1, a.0)] {
            for (b0, b1) in [(b.0, b.1), (b.1, b.0)] {
                out.push(classify((a0, a1), (b0, b1)));
                // The swapped reading, mapped back through the role.
                out.push(classify((b0, b1), (a0, a1)).with_axes_swapped());
            }
        }
        out
    }

    #[track_caller]
    fn assert_invariant(a: (Point, Point), b: (Point, Point), expected: AxisContact) {
        for (i, got) in every_reading(a, b).into_iter().enumerate() {
            assert_eq!(got, expected, "reading {i} disagreed");
        }
    }

    #[test]
    fn the_six_classes_are_reached_and_are_reading_invariant() {
        // Disjoint, non-collinear.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(0.0, 500.0), P(4000.0, 500.0)),
            AxisContact::Disjoint,
        );
        // Endpoint touch, non-collinear.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(4000.0, 0.0), P(4000.0, 3000.0)),
            AxisContact::EndpointTouch { at: P(4000.0, 0.0) },
        );
        // Proper crossing.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(2000.0, -1000.0), P(2000.0, 1000.0)),
            AxisContact::ProperCrossing,
        );
        // Coincident, and the reversed reading is the same class.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(4000.0, 0.0), P(0.0, 0.0)),
            AxisContact::Coincident,
        );
        // Collinear overlap: issue 0055's reproduction.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(2000.0, 0.0), P(6000.0, 0.0)),
            AxisContact::CollinearOverlap,
        );
        // Collinear containment is still overlap, not coincidence.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(1000.0, 0.0), P(3000.0, 0.0)),
            AxisContact::CollinearOverlap,
        );
        // Collinear, end to end: one shared endpoint, no overlap.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(4000.0, 0.0), P(9000.0, 0.0)),
            AxisContact::EndpointTouch { at: P(4000.0, 0.0) },
        );
        // Collinear and separated.
        assert_invariant(
            (P(0.0, 0.0), P(4000.0, 0.0)),
            (P(5000.0, 0.0), P(9000.0, 0.0)),
            AxisContact::Disjoint,
        );
    }

    #[test]
    fn endpoint_on_interior_names_the_axis_that_gets_split() {
        let spine = (P(0.0, 0.0), P(4000.0, 0.0));
        let branch = (P(2000.0, 0.0), P(2000.0, 3000.0));
        assert_eq!(
            classify(spine, branch),
            AxisContact::EndpointOnInterior {
                split: AxisRole::First,
                at: P(2000.0, 0.0),
            }
        );
        assert_eq!(
            classify(branch, spine),
            AxisContact::EndpointOnInterior {
                split: AxisRole::Second,
                at: P(2000.0, 0.0),
            },
            "the role follows the arguments, and only the role"
        );
        assert_invariant(
            spine,
            branch,
            AxisContact::EndpointOnInterior {
                split: AxisRole::First,
                at: P(2000.0, 0.0),
            },
        );
    }

    #[test]
    fn a_vertical_axis_orders_by_y_rather_than_x() {
        // `Along::X` would compare four identical values and call every
        // collinear vertical pair coincident.
        assert_invariant(
            (P(3000.0, 0.0), P(3000.0, 4000.0)),
            (P(3000.0, 2000.0), P(3000.0, 6000.0)),
            AxisContact::CollinearOverlap,
        );
        assert_invariant(
            (P(3000.0, 0.0), P(3000.0, 4000.0)),
            (P(3000.0, 4000.0), P(3000.0, 6000.0)),
            AxisContact::EndpointTouch {
                at: P(3000.0, 4000.0),
            },
        );
        assert_invariant(
            (P(3000.0, 0.0), P(3000.0, 4000.0)),
            (P(3000.0, 5000.0), P(3000.0, 6000.0)),
            AxisContact::Disjoint,
        );
    }

    #[test]
    fn one_ulp_off_the_axis_is_not_on_it() {
        // The exactness that matters: a point a single ulp above the axis is
        // NOT collinear, so no split is induced. A tolerant classifier would
        // manufacture one — and R3.1.2 has no tolerance to appeal to.
        let spine = (P(0.0, 0.0), P(24.0, 24.0));
        let exact = P(12.0, 12.0);
        let nudged = P(12.0, f64::from_bits(12f64.to_bits() + 1));
        assert_eq!(
            classify(spine, (exact, P(12.0, 20.0))),
            AxisContact::EndpointOnInterior {
                split: AxisRole::First,
                at: exact,
            }
        );
        assert_eq!(
            classify(spine, (nudged, P(12.0, 20.0))),
            AxisContact::Disjoint,
            "one ulp above the axis misses it"
        );

        // The same at the coordinate bound, where a naive determinant's two
        // products are enormous and nearly equal.
        let long = (
            P(-COORD_BOUND_MM, -COORD_BOUND_MM),
            P(COORD_BOUND_MM, COORD_BOUND_MM),
        );
        let mid = P(5_000_000.0, 5_000_000.0);
        let mid_up = P(5_000_000.0, f64::from_bits(5_000_000f64.to_bits() + 1));
        assert_eq!(
            classify(long, (mid, P(5_000_000.0, 6_000_000.0))),
            AxisContact::EndpointOnInterior {
                split: AxisRole::First,
                at: mid,
            }
        );
        assert_eq!(
            classify(long, (mid_up, P(5_000_000.0, 6_000_000.0))),
            AxisContact::Disjoint
        );
    }

    #[test]
    fn signed_zero_names_the_same_point_as_zero() {
        // The format maps −0 to +0, so an axis ending at −0 touches one
        // starting at +0 and is coincident with its own mirror.
        assert_invariant(
            (P(-4000.0, 0.0), P(-0.0, -0.0)),
            (P(0.0, 0.0), P(4000.0, 0.0)),
            AxisContact::EndpointTouch { at: P(-0.0, -0.0) },
        );
        assert_eq!(
            classify(
                (P(0.0, 0.0), P(4000.0, 0.0)),
                (P(-0.0, -0.0), P(4000.0, 0.0))
            ),
            AxisContact::Coincident
        );
    }

    #[test]
    fn subnormal_coordinates_are_classified_without_collapsing() {
        // Subnormals are in the domain — `COORD_BOUND_MM` bounds magnitude
        // from above, never from below — and must not be flushed to a shared
        // zero, which would report a spurious coincidence or touch.
        let tiny = f64::from_bits(1);
        let a = (P(0.0, 0.0), P(tiny * 4.0, 0.0));
        assert_eq!(
            classify(a, (P(tiny * 2.0, 0.0), P(tiny * 8.0, 0.0))),
            AxisContact::CollinearOverlap
        );
        assert_eq!(
            classify(a, (P(tiny * 4.0, 0.0), P(tiny * 8.0, 0.0))),
            AxisContact::EndpointTouch {
                at: P(tiny * 4.0, 0.0)
            }
        );
        assert_eq!(
            classify(a, (P(tiny * 6.0, 0.0), P(tiny * 8.0, 0.0))),
            AxisContact::Disjoint,
            "distinct subnormals stay distinct"
        );
    }

    #[test]
    fn the_domain_is_refused_rather_than_answered() {
        let ok = (P(0.0, 0.0), P(4000.0, 0.0));
        // Exactly ON the bound is admissible.
        assert!(axis_contact(
            P(-COORD_BOUND_MM, 0.0),
            P(COORD_BOUND_MM, 0.0),
            P(0.0, -COORD_BOUND_MM),
            P(0.0, COORD_BOUND_MM),
        )
        .is_some());
        // Beyond it is not, on any of the four points or either coordinate.
        let beyond = f64::from_bits(COORD_BOUND_MM.to_bits() + 1);
        assert_eq!(
            axis_contact(P(beyond, 0.0), P(4000.0, 0.0), ok.0, ok.1),
            None
        );
        assert_eq!(axis_contact(ok.0, ok.1, P(0.0, 0.0), P(0.0, beyond)), None);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(axis_contact(P(bad, 0.0), P(4000.0, 0.0), ok.0, ok.1), None);
            assert_eq!(axis_contact(ok.0, ok.1, P(0.0, 0.0), P(0.0, bad)), None);
        }
        // Zero length, including a −0/+0 pair that names one point.
        assert_eq!(axis_contact(P(1.0, 1.0), P(1.0, 1.0), ok.0, ok.1), None);
        assert_eq!(
            axis_contact(P(0.0, -0.0), P(-0.0, 0.0), ok.0, ok.1),
            None,
            "signed zeros are the same point, so this axis is degenerate"
        );
    }

    #[test]
    fn a_crossing_outside_a_segment_is_disjoint_not_a_crossing() {
        // The spec's own `wall-area-overlap` example: the axes' infinite lines
        // meet at (2050, 0), beyond W1's end. No axis contact exists, and this
        // module reports exactly that — the band overlap is not its business.
        assert_invariant(
            (P(0.0, 0.0), P(2000.0, 0.0)),
            (P(850.0, 900.0), P(3250.0, -900.0)),
            AxisContact::Disjoint,
        );
    }
}
