//! Original integer-vertex oracle for ideal wall rectangles.
//!
//! Directions have integer lengths 1, 5 or 13. Multiplying all vertices by
//! 130 therefore makes every half-thickness offset integral, even for odd
//! thicknesses. Projection extrema of these exact vertices provide a separate
//! oracle from the production centre/support radical inequalities.

use domowik_format::Point;
use domowik_geometry::wall_overlap::rectangles_overlap;

type IPoint = (i64, i64);

#[derive(Clone, Debug)]
struct Rectangle {
    start: IPoint,
    end: IPoint,
    thickness: i64,
    vertices: [IPoint; 4],
}

fn rectangle(start: IPoint, direction: IPoint, length: i64, thickness: i64) -> Rectangle {
    assert_eq!(
        direction.0 * direction.0 + direction.1 * direction.1,
        length * length
    );
    let end = (start.0 + direction.0, start.1 + direction.1);
    let offset = (
        -direction.1 * thickness * (65 / length),
        direction.0 * thickness * (65 / length),
    );
    let a = (start.0 * 130, start.1 * 130);
    let b = (end.0 * 130, end.1 * 130);
    Rectangle {
        start,
        end,
        thickness,
        vertices: [
            (a.0 + offset.0, a.1 + offset.1),
            (b.0 + offset.0, b.1 + offset.1),
            (b.0 - offset.0, b.1 - offset.1),
            (a.0 - offset.0, a.1 - offset.1),
        ],
    }
}

fn oracle(a: &Rectangle, b: &Rectangle) -> bool {
    for vertices in [&a.vertices, &b.vertices] {
        for i in 0..4 {
            let p = vertices[i];
            let q = vertices[(i + 1) % 4];
            let normal = (p.1 - q.1, q.0 - p.0);
            let project = |vertices: &[IPoint; 4]| {
                let values = vertices.map(|p| p.0 * normal.0 + p.1 * normal.1);
                (*values.iter().min().unwrap(), *values.iter().max().unwrap())
            };
            let pa = project(&a.vertices);
            let pb = project(&b.vertices);
            if pa.0.max(pb.0) >= pa.1.min(pb.1) {
                return false;
            }
        }
    }
    true
}

fn point(p: IPoint, scale: f64) -> Point {
    Point::new(p.0 as f64 * scale, p.1 as f64 * scale)
}

fn actual(a: &Rectangle, b: &Rectangle, scale: f64) -> Option<bool> {
    rectangles_overlap(
        point(a.start, scale),
        point(a.end, scale),
        a.thickness as f64 * scale,
        point(b.start, scale),
        point(b.end, scale),
        b.thickness as f64 * scale,
    )
}

#[test]
fn rational_rectangle_vertex_oracle_checks_all_pairs_and_reversal() {
    let mut rectangles = Vec::new();
    for x in [-10, 0, 10] {
        for y in [-10, 0, 10] {
            for (direction, length) in [
                ((1, 0), 1),
                ((0, 1), 1),
                ((3, 4), 5),
                ((5, 12), 13),
                ((-4, 3), 5),
                ((12, -5), 13),
            ] {
                for thickness in [1, 2, 5, 10] {
                    rectangles.push(rectangle((x, y), direction, length, thickness));
                }
            }
        }
    }
    let mut count = 0;
    for a in &rectangles {
        for b in &rectangles {
            let expected = oracle(a, b);
            assert_eq!(actual(a, b, 1.0), Some(expected), "{a:?} vs {b:?}");
            let reversed = Rectangle {
                start: a.end,
                end: a.start,
                ..a.clone()
            };
            assert_eq!(
                actual(&reversed, b, 1.0),
                Some(expected),
                "reversed {a:?} vs {b:?}"
            );
            count += 1;
        }
    }
    assert_eq!(count, 46_656);
}

#[test]
fn exact_power_of_two_scaling_preserves_integer_oracle_answers() {
    let mut rectangles = Vec::new();
    for start in [(0, 0), (1, 2), (-3, 5)] {
        for (direction, length) in [((1, 0), 1), ((3, 4), 5), ((5, 12), 13)] {
            for thickness in [1, 2, 5] {
                rectangles.push(rectangle(start, direction, length, thickness));
            }
        }
    }
    // Powers of two constructed from exponent bits; all scaled inputs remain
    // exact and within the coordinate/dimension domain. Tiny scales stress
    // intermediate underflow, not a changed underlying geometry.
    for exponent in [-1000, -530, -500, -100, 0, 15] {
        let scale = f64::from_bits(((exponent + 1023) as u64) << 52);
        for a in &rectangles {
            for b in &rectangles {
                assert_eq!(
                    actual(a, b, scale),
                    Some(oracle(a, b)),
                    "scale 2^{exponent}: {a:?} vs {b:?}"
                );
            }
        }
    }
}

#[test]
fn subnormal_thickness_and_one_ulp_clearances_remain_distinct() {
    let eta = f64::from_bits(1);
    for width in [eta, f64::MIN_POSITIVE, 1.0, 200.0] {
        for (gap, expected) in [
            (width.next_down(), true),
            (width, false),
            (width.next_up(), false),
        ] {
            assert_eq!(
                rectangles_overlap(
                    Point::new(0.0, 0.0),
                    Point::new(1.0, 0.0),
                    width,
                    Point::new(0.0, gap),
                    Point::new(1.0, gap),
                    width
                ),
                Some(expected),
                "width {width:e}, gap {gap:e}"
            );
        }
    }
}

#[test]
fn public_predicate_preserves_all_subnormal_touching_and_asymmetric_overlap() {
    // These inputs make every coordinate, non-zero axis component and
    // thickness subnormal.  The interval filter cannot safely decide their
    // squared support expressions, so this is deliberately a public-path
    // regression for the exact dyadic fallback rather than a direct fallback
    // unit test.
    let eta = f64::from_bits(1);
    let assert_all_readings = |a0, a1, ta, b0, b1, tb, expected, why| {
        for (left0, left1) in [(a0, a1), (a1, a0)] {
            for (right0, right1) in [(b0, b1), (b1, b0)] {
                assert_eq!(
                    rectangles_overlap(left0, left1, ta, right0, right1, tb),
                    Some(expected),
                    "{why}: reversed axes"
                );
                assert_eq!(
                    rectangles_overlap(right0, right1, tb, left0, left1, ta),
                    Some(expected),
                    "{why}: swapped walls"
                );
            }
        }
    };

    // Both horizontal bands have thickness 2η.  Centres 2η apart put their
    // faces at y = η: a shared edge is not positive area.  Moving the second
    // centre down exactly one η makes a 1η-high shared band, which is area.
    assert_all_readings(
        Point::new(0.0, 0.0),
        Point::new(eta * 8.0, 0.0),
        eta * 2.0,
        Point::new(0.0, eta * 2.0),
        Point::new(eta * 8.0, eta * 2.0),
        eta * 2.0,
        false,
        "all-subnormal strips sharing one edge",
    );
    assert_all_readings(
        Point::new(0.0, 0.0),
        Point::new(eta * 8.0, 0.0),
        eta * 2.0,
        Point::new(0.0, eta),
        Point::new(eta * 8.0, eta),
        eta * 2.0,
        true,
        "all-subnormal strips overlapping by one eta",
    );

    // Scale an asymmetric horizontal/vertical corner construction while it
    // remains subnormal.  A has thickness 2s; B has thickness 4s.  At the
    // first position their only common point is (8s, s).  At the second, B
    // moves one s left and one s down, giving the open rectangle
    // (7s, 8s) × (0, s) in common.
    for scale in [eta, eta * 8.0, eta * 128.0] {
        assert_all_readings(
            Point::new(0.0, 0.0),
            Point::new(scale * 8.0, 0.0),
            scale * 2.0,
            Point::new(scale * 10.0, scale),
            Point::new(scale * 10.0, scale * 9.0),
            scale * 4.0,
            false,
            "scaled asymmetric all-subnormal corner touch",
        );
        assert_all_readings(
            Point::new(0.0, 0.0),
            Point::new(scale * 8.0, 0.0),
            scale * 2.0,
            Point::new(scale * 9.0, 0.0),
            Point::new(scale * 9.0, scale * 8.0),
            scale * 4.0,
            true,
            "scaled asymmetric all-subnormal positive overlap",
        );
    }
}
