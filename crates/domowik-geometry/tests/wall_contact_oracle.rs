//! Original exhaustive integer-grid oracle for the binary64 classifier.
//! Uses exact integer intersection parameters, not the production orientation
//! branching. All grid integers and intermediate arithmetic fit exactly in i64.

use domowik_format::Point;
use domowik_geometry::wall_contact::{axis_contact, AxisContact, AxisRole};
use domowik_geometry::{orientation, Orientation};

#[test]
fn mixed_unit_and_subnormal_coordinates_keep_the_tiny_nonzero_determinant() {
    let eta = f64::from_bits(1);
    // (1,-eta) cross (eta,0) = eta^2 > 0, even though no binary64 can
    // represent that product. Scaling by only the largest difference fails.
    assert_eq!(
        orientation(
            Point::new(0.0, eta),
            Point::new(1.0, 0.0),
            Point::new(eta, eta)
        ),
        Orientation::CounterClockwise
    );
}

type IPoint = (i64, i64);

fn sub(a: IPoint, b: IPoint) -> IPoint {
    (a.0 - b.0, a.1 - b.1)
}

fn dot(a: IPoint, b: IPoint) -> i64 {
    a.0 * b.0 + a.1 * b.1
}

fn cross(a: IPoint, b: IPoint) -> i64 {
    a.0 * b.1 - a.1 * b.0
}

fn point(a: IPoint) -> Point {
    Point::new(a.0 as f64, a.1 as f64)
}

fn oracle(a: IPoint, b: IPoint, c: IPoint, d: IPoint) -> AxisContact {
    let da = sub(b, a);
    let db = sub(d, c);
    let delta = sub(c, a);
    let det = cross(da, db);
    if det == 0 {
        if cross(delta, da) != 0 {
            return AxisContact::Disjoint;
        }
        let length_squared = dot(da, da);
        let tc = dot(delta, da);
        let td = dot(sub(d, a), da);
        let lo = tc.min(td);
        let hi = tc.max(td);
        if lo == 0 && hi == length_squared {
            return AxisContact::Coincident;
        }
        return match lo.max(0).cmp(&hi.min(length_squared)) {
            std::cmp::Ordering::Less => AxisContact::CollinearOverlap,
            std::cmp::Ordering::Equal => AxisContact::EndpointTouch {
                at: point(if hi == 0 { a } else { b }),
            },
            std::cmp::Ordering::Greater => AxisContact::Disjoint,
        };
    }
    let sign = det.signum();
    let denominator = det.abs();
    let t = cross(delta, db) * sign;
    let u = cross(delta, da) * sign;
    if !(0..=denominator).contains(&t) || !(0..=denominator).contains(&u) {
        return AxisContact::Disjoint;
    }
    let a_interior = 0 < t && t < denominator;
    let b_interior = 0 < u && u < denominator;
    match (a_interior, b_interior) {
        (true, true) => AxisContact::ProperCrossing,
        (true, false) => AxisContact::EndpointOnInterior {
            split: AxisRole::First,
            at: point(if u == 0 { c } else { d }),
        },
        (false, true) => AxisContact::EndpointOnInterior {
            split: AxisRole::Second,
            at: point(if t == 0 { a } else { b }),
        },
        (false, false) => AxisContact::EndpointTouch {
            at: point(if t == 0 { a } else { b }),
        },
    }
}

#[test]
fn every_directed_segment_pair_on_a_five_by_five_grid_matches_integer_parameters() {
    let points: Vec<_> = (-2..=2)
        .flat_map(|x| (-2..=2).map(move |y| (x, y)))
        .collect();
    let mut checked = 0;
    for &a in &points {
        for &b in &points {
            if a == b {
                continue;
            }
            for &c in &points {
                for &d in &points {
                    if c == d {
                        continue;
                    }
                    assert_eq!(
                        axis_contact(point(a), point(b), point(c), point(d)),
                        Some(oracle(a, b, c, d)),
                        "{a:?}->{b:?}, {c:?}->{d:?}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 360_000);
}
