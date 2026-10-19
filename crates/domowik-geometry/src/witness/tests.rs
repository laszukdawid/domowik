//! Witnesses against an INDEPENDENT oracle: `i128` rationals over integer
//! fixtures, with rectangle axes on Pythagorean triples so `√D` is an
//! integer and the ideal rectangle is fully rational. Nothing here uses
//! `Enclosure`, `Dyadic` or `Quad`.

use super::*;
use crate::spatial::Aabb;

/// `n / d`, `d > 0`.
#[derive(Debug, Clone, Copy)]
struct Q(i128, i128);

impl Q {
    fn int(n: i128) -> Self {
        Q(n, 1)
    }
    fn new(n: i128, d: i128) -> Self {
        assert!(d != 0);
        if d < 0 {
            Q(-n, -d)
        } else {
            Q(n, d)
        }
    }
    fn add(self, o: Q) -> Q {
        Q::new(self.0 * o.1 + o.0 * self.1, self.1 * o.1).norm()
    }
    fn sub(self, o: Q) -> Q {
        self.add(Q(-o.0, o.1))
    }
    fn mul(self, o: Q) -> Q {
        Q::new(self.0 * o.0, self.1 * o.1).norm()
    }
    fn div(self, o: Q) -> Q {
        Q::new(self.0 * o.1, self.1 * o.0).norm()
    }
    fn cmp(self, o: Q) -> Ordering {
        (self.0 * o.1).cmp(&(o.0 * self.1))
    }
    fn norm(self) -> Q {
        fn gcd(a: i128, b: i128) -> i128 {
            if b == 0 {
                a.abs().max(1)
            } else {
                gcd(b, a % b)
            }
        }
        let g = gcd(self.0, self.1);
        Q(self.0 / g, self.1 / g)
    }
    fn clamp(self, lo: Q, hi: Q) -> Q {
        if self.cmp(lo) == Ordering::Less {
            lo
        } else if self.cmp(hi) == Ordering::Greater {
            hi
        } else {
            self
        }
    }
}

/// Oracle: squared distance and closest point to the closed segment / line /
/// ray `a + t·v`.
fn oracle_linear(p: (i64, i64), a: (i64, i64), v: (i64, i64), extent: u8) -> Option<(Q, Q, Q)> {
    let (vx, vy) = (Q::int(v.0 as i128), Q::int(v.1 as i128));
    let dd = vx.mul(vx).add(vy.mul(vy));
    let (wx, wy) = (Q::int((p.0 - a.0) as i128), Q::int((p.1 - a.1) as i128));
    let s = wx.mul(vx).add(wy.mul(vy));
    let mut t = s.div(dd);
    match extent {
        0 => t = t.clamp(Q::int(0), Q::int(1)),
        2 if t.cmp(Q::int(0)) != Ordering::Greater => return None,
        _ => {}
    }
    let qx = Q::int(a.0 as i128).add(vx.mul(t));
    let qy = Q::int(a.1 as i128).add(vy.mul(t));
    let (dx, dy) = (qx.sub(Q::int(p.0 as i128)), qy.sub(Q::int(p.1 as i128)));
    Some((dx.mul(dx).add(dy.mul(dy)), qx, qy))
}

/// Oracle: the ideal rectangle with a Pythagorean axis `v = n·(3, 4)` (or a
/// permutation/sign of it), so `|v| = 5n`. Closest point by clamping in the
/// rectangle's own frame.
fn oracle_rect(
    p: (i64, i64),
    a: (i64, i64),
    v: (i64, i64),
    len: i64,
    thickness: i64,
    along: Option<(Q, i64)>,
) -> (Q, Q, Q) {
    let l = Q::int(len as i128);
    let (ux, uy) = (
        Q::new(v.0 as i128, len as i128),
        Q::new(v.1 as i128, len as i128),
    );
    let (nx, ny) = (Q(-uy.0, uy.1), ux);
    let (wx, wy) = (Q::int((p.0 - a.0) as i128), Q::int((p.1 - a.1) as i128));
    let s = wx.mul(ux).add(wy.mul(uy)); // along, in mm
    let c = wx.mul(nx).add(wy.mul(ny)); // across
    let h = Q::new(thickness as i128, 2);
    let (lo, hi) = match along {
        None => (Q::int(0), l),
        Some((u, width)) => {
            let centre = u.mul(l);
            let k = Q::new(width as i128, 2);
            (centre.sub(k), centre.add(k))
        }
    };
    let sc = s.clamp(lo, hi);
    let cc = c.clamp(Q(-h.0, h.1), h);
    let qx = Q::int(a.0 as i128).add(ux.mul(sc)).add(nx.mul(cc));
    let qy = Q::int(a.1 as i128).add(uy.mul(sc)).add(ny.mul(cc));
    let (dx, dy) = (s.sub(sc), c.sub(cc));
    (dx.mul(dx).add(dy.mul(dy)), qx, qy)
}

/// A rectangle fixture: start, axis vector, axis length, thickness, and an
/// optional opening `(u, width)`.
type RectFixture = ((i64, i64), (i64, i64), i64, i64, Option<(Q, i64)>);

struct Rng(u64);

impl Rng {
    fn int(&mut self, m: i64) -> i64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % (2 * m as u64 + 1)) as i64 - m
    }
}

fn pt(p: (i64, i64)) -> Point {
    Point::new(p.0 as f64, p.1 as f64)
}

#[test]
fn linear_witnesses_order_admit_and_locate_exactly_like_the_oracle() {
    let mut rng = Rng(0x51_7cc1_b727_220a);
    for _ in 0..3000 {
        let p = (rng.int(40), rng.int(40));
        let make = |rng: &mut Rng| {
            let a = (rng.int(30), rng.int(30));
            let v = (rng.int(12), rng.int(12));
            let extent = (rng.int(1) + 1) as u8; // 0, 1, 2
            (a, v, extent)
        };
        let (f1, f2) = (make(&mut rng), make(&mut rng));
        let w = |(a, v, e): ((i64, i64), (i64, i64), u8)| {
            let origin = pt(a);
            let end = Point::new(origin.x + v.0 as f64, origin.y + v.1 as f64);
            let extent = match e {
                0 => Extent::Segment { end },
                1 => Extent::Line,
                _ => Extent::Ray,
            };
            Witness::new(
                pt(p),
                Shape::Linear {
                    origin,
                    dir: Dir::between(origin, end),
                    extent,
                },
            )
        };
        let degenerate = |f: ((i64, i64), (i64, i64), u8)| f.1 == (0, 0);
        if degenerate(f1) || degenerate(f2) {
            continue;
        }
        let (w1, w2) = (w(f1), w(f2));
        let (o1, o2) = (
            oracle_linear(p, f1.0, f1.1, f1.2),
            oracle_linear(p, f2.0, f2.1, f2.2),
        );
        assert_eq!(o1.is_none(), w1.branch() == Branch::Rejected, "{f1:?}");
        let (Some(o1), Some(o2)) = (o1, o2) else {
            continue;
        };
        assert_eq!(
            w1.cmp_dist(&w2),
            o1.0.cmp(o2.0),
            "dist {f1:?} {f2:?} at {p:?}"
        );
        assert_eq!(
            w1.cmp_xy(&w2),
            o1.1.cmp(o2.1).then(o1.2.cmp(o2.2)),
            "xy {f1:?} {f2:?} at {p:?}"
        );
        for r in [0, 1, 2, 3, 5, 8] {
            let r2 = Q::int(r * r);
            assert_eq!(
                w1.within(&Radius::mm(r as f64)),
                o1.0.cmp(r2) != Ordering::Greater,
                "radius {r} {f1:?} at {p:?}"
            );
        }
    }
}

#[test]
fn rectangle_witnesses_match_the_rational_oracle() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let axes = [(3, 4), (4, 3), (-3, 4), (4, -3), (0, 5), (5, 0), (-5, 0)];
    let make = |rng: &mut Rng| {
        let (bx, by) = axes[(rng.int(100) + 100) as usize % axes.len()];
        let n = rng.int(3).abs() + 1;
        let a = (rng.int(20), rng.int(20));
        let v = (bx * n, by * n);
        let t = 2 * (rng.int(3).abs() + 1);
        let opening = (rng.int(1) == 0).then(|| (Q::new(1, 2), 2 * n));
        (a, v, 5 * n, t, opening)
    };
    for _ in 0..3000 {
        let p = (rng.int(40), rng.int(40));
        let (f1, f2) = (make(&mut rng), make(&mut rng));
        let w = |(a, v, _, t, op): RectFixture| {
            let along = match op {
                None => Along::Wall,
                Some((u, width)) => Along::Opening {
                    u: u.0 as f64 / u.1 as f64,
                    width: width as f64,
                },
            };
            Witness::new(
                pt(p),
                Shape::Rect {
                    a: pt(a),
                    b: pt((a.0 + v.0, a.1 + v.1)),
                    thickness: t as f64,
                    along,
                },
            )
        };
        let (w1, w2) = (w(f1), w(f2));
        let o = |f: RectFixture| oracle_rect(p, f.0, f.1, f.2, f.3, f.4);
        let (o1, o2) = (o(f1), o(f2));
        assert_eq!(
            w1.cmp_dist(&w2),
            o1.0.cmp(o2.0),
            "dist {f1:?} {f2:?} at {p:?}"
        );
        assert_eq!(
            w1.cmp_xy(&w2),
            o1.1.cmp(o2.1).then(o1.2.cmp(o2.2)),
            "xy {f1:?} {f2:?} at {p:?}"
        );
        for r in [0, 1, 2, 3, 5] {
            assert_eq!(
                w1.within(&Radius::mm(r as f64)),
                o1.0.cmp(Q::int(r * r)) != Ordering::Greater,
                "radius {r} {f1:?} at {p:?}"
            );
        }
        // The exact closest point is reported as the pointer itself inside.
        if o1.0.cmp(Q::int(0)) == Ordering::Equal {
            assert!(w1.is_at(pt(p)));
        }
    }
}

#[test]
fn ideal_corners_order_like_rational_corners() {
    let mut rng = Rng(0x7a3c_99e1_0b55_d24f);
    let corners = |a: (i64, i64), v: (i64, i64), len: i64, t: i64| -> Vec<(Q, Q)> {
        let (ux, uy) = (
            Q::new(v.0 as i128, len as i128),
            Q::new(v.1 as i128, len as i128),
        );
        let (nx, ny) = (Q(-uy.0, uy.1), ux);
        let h = Q::new(t as i128, 2);
        let at = |e: i64, s: i64| {
            let base = (
                Q::int((a.0 + if e > 0 { v.0 } else { 0 }) as i128),
                Q::int((a.1 + if e > 0 { v.1 } else { 0 }) as i128),
            );
            let hs = h.mul(Q::int(s as i128));
            (base.0.add(nx.mul(hs)), base.1.add(ny.mul(hs)))
        };
        let mut ring = vec![at(-1, -1), at(1, -1), at(1, 1), at(-1, 1)];
        let least = (0..4)
            .min_by(|&i, &j| ring[i].0.cmp(ring[j].0).then(ring[i].1.cmp(ring[j].1)))
            .unwrap();
        ring.rotate_left(least);
        ring
    };
    let axes = [(3, 4), (4, 3), (-3, 4), (0, 5), (5, 0), (0, -5)];
    for _ in 0..2000 {
        let mut f = || {
            let (bx, by) = axes[(rng.int(100) + 100) as usize % axes.len()];
            let a = (rng.int(5), rng.int(5));
            (a, (bx, by), 5, 2 * (rng.int(2).abs() + 1))
        };
        let (f1, f2) = (f(), f());
        let shape = |(a, v, _, t): ((i64, i64), (i64, i64), i64, i64)| Shape::Rect {
            a: pt(a),
            b: pt((a.0 + v.0, a.1 + v.1)),
            thickness: t as f64,
            along: Along::Wall,
        };
        let (c1, c2) = (
            corners(f1.0, f1.1, f1.2, f1.3),
            corners(f2.0, f2.1, f2.2, f2.3),
        );
        let want = c1
            .iter()
            .zip(&c2)
            .map(|(p, q)| p.0.cmp(q.0).then(p.1.cmp(q.1)))
            .find(|o| *o != Ordering::Equal)
            .unwrap_or(Ordering::Equal);
        assert_eq!(
            Corners::of(shape(f1)).cmp(&Corners::of(shape(f2))),
            want,
            "{f1:?} {f2:?}"
        );
    }
}

#[test]
fn ideal_rectangles_meet_boxes_like_a_rational_polygon_test() {
    // Independent: a convex polygon meets a box iff a vertex of either lies
    // in the other or two edges cross (closed sets).
    fn inside_poly(p: (Q, Q), poly: &[(Q, Q)]) -> bool {
        let mut sgn = 0i8;
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let cr =
                b.0.sub(a.0)
                    .mul(p.1.sub(a.1))
                    .sub(b.1.sub(a.1).mul(p.0.sub(a.0)));
            let s = match cr.cmp(Q::int(0)) {
                Ordering::Less => -1,
                Ordering::Greater => 1,
                Ordering::Equal => 0,
            };
            if s != 0 {
                if sgn != 0 && s != sgn {
                    return false;
                }
                sgn = s;
            }
        }
        true
    }
    fn orient(a: (Q, Q), b: (Q, Q), c: (Q, Q)) -> Ordering {
        b.0.sub(a.0)
            .mul(c.1.sub(a.1))
            .sub(b.1.sub(a.1).mul(c.0.sub(a.0)))
            .cmp(Q::int(0))
    }
    fn on(a: (Q, Q), b: (Q, Q), c: (Q, Q)) -> bool {
        let lo = |x: Q, y: Q| if x.cmp(y) == Ordering::Less { x } else { y };
        let hi = |x: Q, y: Q| if x.cmp(y) == Ordering::Greater { x } else { y };
        c.0.cmp(lo(a.0, b.0)) != Ordering::Less
            && c.0.cmp(hi(a.0, b.0)) != Ordering::Greater
            && c.1.cmp(lo(a.1, b.1)) != Ordering::Less
            && c.1.cmp(hi(a.1, b.1)) != Ordering::Greater
    }
    fn cross(a: (Q, Q), b: (Q, Q), c: (Q, Q), d: (Q, Q)) -> bool {
        let (o1, o2, o3, o4) = (
            orient(a, b, c),
            orient(a, b, d),
            orient(c, d, a),
            orient(c, d, b),
        );
        if o1 != o2
            && o3 != o4
            && o1 != Ordering::Equal
            && o2 != Ordering::Equal
            && o3 != Ordering::Equal
            && o4 != Ordering::Equal
        {
            return true;
        }
        (o1 == Ordering::Equal && on(a, b, c))
            || (o2 == Ordering::Equal && on(a, b, d))
            || (o3 == Ordering::Equal && on(c, d, a))
            || (o4 == Ordering::Equal && on(c, d, b))
    }
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let axes = [(3, 4), (4, -3), (0, 5), (5, 0), (-3, -4)];
    for _ in 0..3000 {
        let (bx, by) = axes[(rng.int(100) + 100) as usize % axes.len()];
        let a = (rng.int(6), rng.int(6));
        let t = 2 * (rng.int(2).abs() + 1);
        let (x0, y0) = (rng.int(12), rng.int(12));
        let (x1, y1) = (x0 + rng.int(6).abs(), y0 + rng.int(6).abs());
        let (ux, uy) = (Q::new(bx as i128, 5), Q::new(by as i128, 5));
        let (nx, ny) = (Q(-uy.0, uy.1), ux);
        let h = Q::new(t as i128, 2);
        let c = |e: i64, s: i64| {
            let hs = h.mul(Q::int(s as i128));
            (
                Q::int((a.0 + if e > 0 { bx } else { 0 }) as i128).add(nx.mul(hs)),
                Q::int((a.1 + if e > 0 { by } else { 0 }) as i128).add(ny.mul(hs)),
            )
        };
        let poly = [c(-1, -1), c(1, -1), c(1, 1), c(-1, 1)];
        let q = |v: i64| Q::int(v as i128);
        let boxp = [
            (q(x0), q(y0)),
            (q(x1), q(y0)),
            (q(x1), q(y1)),
            (q(x0), q(y1)),
        ];
        let in_box = |p: (Q, Q)| {
            p.0.cmp(q(x0)) != Ordering::Less
                && p.0.cmp(q(x1)) != Ordering::Greater
                && p.1.cmp(q(y0)) != Ordering::Less
                && p.1.cmp(q(y1)) != Ordering::Greater
        };
        let want = poly.iter().any(|&p| in_box(p))
            || boxp.iter().any(|&p| inside_poly(p, &poly))
            || (0..4).any(|i| {
                (0..4).any(|j| cross(poly[i], poly[(i + 1) % 4], boxp[j], boxp[(j + 1) % 4]))
            });
        let shape = Shape::Rect {
            a: pt(a),
            b: pt((a.0 + bx, a.1 + by)),
            thickness: t as f64,
            along: Along::Wall,
        };
        let rect = Aabb::from_corners(pt((x0, y0)), pt((x1, y1)));
        assert_eq!(
            rect_meets_box(&shape, &rect),
            want,
            "{a:?} {:?} t={t} box {rect:?}",
            (bx, by)
        );
    }
}

/// Codex GPT-6-Astra 0012 review, counterexample A: the exact projection is
/// inside the radius; the rounded construction was not.
#[test]
fn a_projection_exactly_inside_the_radius_is_admitted() {
    let (a, b, p) = (
        Point::new(0.0, 0.0),
        Point::new(3.0, 1.0),
        Point::new(1.0, 0.0),
    );
    // 0x1.43d136248490fp-2
    let r = f64::from_bits(0x3FD4_3D13_6248_490F);
    let w = Witness::new(
        p,
        Shape::Linear {
            origin: a,
            dir: Dir::between(a, b),
            extent: Extent::Segment { end: b },
        },
    );
    assert!(w.within(&Radius::mm(r)));
}

/// Counterexample B: two collinear axes with the same exact projection
/// compare EQUAL in distance (so R6.3.1's on-segment key decides), where the
/// rounded projections did not.
#[test]
fn collinear_axes_tie_exactly() {
    let p = Point::new(1.0, 0.0);
    let line = |a: Point, b: Point| {
        Witness::new(
            p,
            Shape::Linear {
                origin: a,
                dir: Dir::between(a, b),
                extent: Extent::Line,
            },
        )
    };
    let (wa, wb) = (
        line(Point::new(0.0, 0.0), Point::new(3.0, 1.0)),
        line(Point::new(3.0, 1.0), Point::new(6.0, 2.0)),
    );
    assert_eq!(wa.cmp_dist(&wb), Ordering::Equal);
    assert_eq!(wa.cmp_xy(&wb), Ordering::Equal);
}

/// Counterexample C: a rounded wall corner lies OUTSIDE the ideal wall.
#[test]
fn a_rounded_corner_is_outside_the_ideal_wall() {
    let shape = Shape::Rect {
        a: Point::new(0.0, 0.0),
        b: Point::new(10.0, 10.0),
        thickness: 2.0,
        along: Along::Wall,
    };
    let corner = Point::new(9.292893218813452, 10.707106781186548);
    let w = Witness::new(corner, shape);
    assert_ne!(w.branch(), Branch::Fixed);
    assert!(!w.within(&Radius::mm(0.0)));
}

/// Counterexample D: a 2⁻⁶⁰ mm wall still contains a point on its axis.
#[test]
fn a_hair_thin_wall_still_contains_its_axis() {
    let shape = Shape::Rect {
        a: Point::new(0.0, 1.0),
        b: Point::new(1.0, 1.0),
        thickness: 2f64.powi(-60),
        along: Along::Wall,
    };
    let w = Witness::new(Point::new(0.5, 1.0), shape);
    assert_eq!(w.branch(), Branch::Fixed);
    assert!(w.within(&Radius::mm(0.0)));
}

/// Counterexample E: a subnormal-length axis is still a line.
#[test]
fn a_subnormal_axis_is_still_a_line() {
    let eta = f64::from_bits(1);
    let (a, b) = (Point::new(0.0, 0.0), Point::new(eta, 0.0));
    let w = Witness::new(
        Point::new(eta, 0.0),
        Shape::Linear {
            origin: a,
            dir: Dir::between(a, b),
            extent: Extent::Line,
        },
    );
    assert_eq!(w.branch(), Branch::Interior);
    assert!(w.within(&Radius::mm(0.0)));
}

#[test]
fn a_screen_radius_is_never_rounded() {
    // 12 px at 3 px/mm is exactly 4 mm; a point at 4 mm is admitted, one
    // ulp further is not.
    let p = Point::new(0.0, 0.0);
    let at = |x: f64| Witness::new(p, Shape::Point(Point::new(x, 0.0)));
    let r = Radius::screen(12.0, 3.0);
    assert!(at(4.0).within(&r));
    assert!(!at(4f64.next_up()).within(&r));
    // 1 px at 3 px/mm is 1/3 mm, not a binary64. The binary64 nearest it
    // lies just below (…3148), its successor just above (…3703).
    let r = Radius::screen(1.0, 3.0);
    let below = 1.0 / 3.0;
    assert!(at(below).within(&r));
    assert!(!at(below.next_up()).within(&r));
}

/// The direct bounding-box minimum agrees with the minimum over the four
/// ideal corners, exactly, on arbitrary (irrational-length) axes.
#[test]
fn the_direct_bbox_minimum_agrees_with_the_corners() {
    let mut rng = Rng(0x0dd_ba11_cafe_f00d);
    for _ in 0..3000 {
        let mut f = || {
            let a = (rng.int(50), rng.int(50));
            let b = (a.0 + rng.int(40), a.1 + rng.int(40));
            let t = rng.int(9).abs() + 1;
            (a, b, t)
        };
        let (f1, f2) = (f(), f());
        if f1.0 == f1.1 || f2.0 == f2.1 {
            continue;
        }
        let shape = |(a, b, t): ((i64, i64), (i64, i64), i64)| Shape::Rect {
            a: pt(a),
            b: pt(b),
            thickness: t as f64,
            along: Along::Wall,
        };
        let (s1, s2) = (shape(f1), shape(f2));
        assert_eq!(
            BboxMin::of(s1).cmp(&BboxMin::of(s2)),
            Corners::of(s1).cmp_bbox_min(&Corners::of(s2)),
            "{f1:?} {f2:?}"
        );
        let (x, y) = (rng.int(60) as f64, rng.int(60) as f64);
        assert_eq!(
            BboxMin::of(s1).cmp_point(x, y),
            Corners::of(s1).cmp_bbox_min_point(x, y),
            "{f1:?} vs ({x}, {y})"
        );
    }
}

/// The float fast path never certifies a point the rational oracle puts
/// outside — sampled on and next to every boundary, where a wrong error
/// margin would show — and it does certify ordinary interior points.
#[test]
fn certainly_inside_is_never_wrong() {
    let mut rng = Rng(0x5eed_1e55_c0ff_ee00);
    let axes = [(3, 4), (4, -3), (0, 5), (5, 0), (-3, -4)];
    let mut certified = 0;
    for _ in 0..40_000 {
        let (bx, by) = axes[(rng.int(100) + 100) as usize % axes.len()];
        let n = rng.int(3).abs() + 1;
        let a = (rng.int(20), rng.int(20));
        let v = (bx * n, by * n);
        let t = 2 * (rng.int(3).abs() + 1);
        let opening = (rng.int(1) == 0).then(|| (Q::new(1, 2), 2 * n));
        // Pointers on a fine grid near the rectangle, plus one-ulp nudges.
        let base = (a.0 + rng.int(25), a.1 + rng.int(25));
        let nudge = rng.int(2);
        let p = Point::new(
            base.0 as f64 + nudge as f64 * f64::EPSILON * 64.0,
            base.1 as f64,
        );
        let shape = Shape::Rect {
            a: pt(a),
            b: pt((a.0 + v.0, a.1 + v.1)),
            thickness: t as f64,
            along: match opening {
                None => Along::Wall,
                Some((u, w)) => Along::Opening {
                    u: u.0 as f64 / u.1 as f64,
                    width: w as f64,
                },
            },
        };
        if certainly_inside(p, &shape) {
            certified += 1;
            let w = Witness::new(p, shape);
            assert!(w.within(&Radius::mm(0.0)), "{shape:?} at {p:?}");
            if nudge == 0 {
                let o = oracle_rect(base, a, v, 5 * n, t, opening);
                assert_eq!(o.0.cmp(Q::int(0)), Ordering::Equal, "{shape:?} at {p:?}");
            }
        }
    }
    assert!(
        certified > 500,
        "the fast path should fire on interior points ({certified})"
    );
}
