//! Exact distance witnesses for hit testing and snapping (issue 0012).
//!
//! # What a witness is
//!
//! A [`Witness`] is one candidate's answer to "how far is the pointer `p`
//! from this feature, and where is the closest point" — held so that every
//! comparison R6.3.1 and R7.1.3 make on it is EXACT over the supplied
//! binary64 inputs, without materializing exact arithmetic in the ordinary
//! case:
//!
//! | Feature | Shape | Squared distance | Closest point |
//! |---|---|---|---|
//! | stored point (endpoint, join node, cell centre) | [`Shape::Point`] | `|p − q|²` | `q` |
//! | grid lattice point `(i·s, j·s)` | [`Shape::Lattice`] | rational | exact products |
//! | segment / line / ray from `a` along `d` | [`Shape::Linear`] | `C²/D` interior, else endpoint | `(a·D + d·S)/D` |
//! | ideal wall or opening rectangle | [`Shape::Rect`] | `(A − B·√D)/D` | `(X + Y·√D)/D` |
//!
//! with `w = p − a`, `D = |d|²`, `S = w·d`, `C = d × w` — no division, no
//! rounded projection, no rounded wall corner. The rectangle is the IDEAL
//! one of R3.0.2 (axis endpoints and thickness), not the rounded polygon
//! `wall_polygon` draws (Codex GPT-6-Astra review: a rounded corner can sit
//! outside the ideal wall, and a 2⁻⁶⁰ mm wall's polygon collapses).
//!
//! # Filter, then exact
//!
//! Construction computes certified [`Enclosure`]s of the squared distance and
//! of both closest coordinates (outward rounding at every step, and any
//! non-finite intermediate makes the enclosure absent rather than wrong).
//! Every decision — the branch (inside, endpoint, interior), radius
//! admission, ordering, coordinate ties, anchor coincidence — tries the
//! enclosures first. Only an overlap builds the exact [`Quad`]s, once per
//! witness ([`OnceCell`]), from [`Dyadic`]s of the original inputs.
//!
//! # The reported point is approximate, by decision
//!
//! [`Witness::output`] is a binary64 point within the certified coordinate
//! enclosure (its midpoint), so its error is at most half the enclosure's
//! width — a few ulps for ordinary geometry. It is EXACT when the enclosure
//! is a single value (stored points, provably exact computations) and for a
//! pointer lying on the feature; otherwise it may differ from an exactly
//! representable result by an ulp. When an enclosure was lost to under- or
//! overflow, the point is converted from the exact coordinates in scaled
//! arithmetic. Ranking never uses it (owner decision 2026-10-09, ED-0112:
//! exact ranking and admission, bounded output).

use crate::dyadic::Dyadic;
use crate::enclosure::{exactly, Enclosure};
use crate::quad::Quad;
use domowik_format::Point;
use std::cell::OnceCell;
use std::cmp::Ordering;

type E = Option<Enclosure>;

// --- exactness-preserving enclosure arithmetic ------------------------------
//
// Each operation widens its result outward by one ulp, EXCEPT when both
// operands are exact singletons and the rounded result is provably exact,
// in which case the result stays a singleton. Exactness is proven without
// rounding: a sum by its TwoSum error term (exact for every finite input,
// subnormals included — Knuth), a product by its FMA residual (exact when the
// product does not underflow; guarded by a magnitude threshold), a quotient
// by multiplying back. This is what lets the FILTER decide exact ties —
// collinear axes through the pointer, grid-aligned walls — instead of every
// such tie escalating to big-integer arithmetic.

/// Below this magnitude a product's FMA residual may not be representable;
/// such products are widened instead (2^-900, far above the underflow
/// threshold of any residual of a product this large).
const FMA_SAFE: f64 = 1.0e-270;

fn single(e: &Enclosure) -> Option<f64> {
    (e.lo == e.hi).then_some(e.lo)
}
fn ex(x: f64) -> E {
    Some(exactly(x))
}
fn widen(v: f64) -> E {
    Enclosure {
        lo: v.next_down(),
        hi: v.next_up(),
    }
    .finite()
}
fn exact_sum(a: f64, b: f64) -> E {
    let s = a + b;
    if !s.is_finite() {
        return None;
    }
    let bb = s - a;
    let err = (a - (s - bb)) + (b - bb);
    if err == 0.0 {
        ex(s)
    } else {
        widen(s)
    }
}
fn exact_product(a: f64, b: f64) -> E {
    let p = a * b;
    if !p.is_finite() {
        return None;
    }
    if a == 0.0 || b == 0.0 {
        return ex(0.0);
    }
    if p.abs() >= FMA_SAFE && a.mul_add(b, -p) == 0.0 {
        ex(p)
    } else {
        widen(p)
    }
}
fn dif(a: f64, b: f64) -> E {
    exact_sum(a, -b)
}
/// Rejects a NaN endpoint. Infinite endpoints are sound bounds and stay;
/// NaN can only arise from an infinite operand, which [`mul`] and [`div`]
/// refuse before their corner min/max could swallow it.
fn ordered(e: Enclosure) -> E {
    (e.lo <= e.hi).then_some(e)
}
fn add(a: E, b: E) -> E {
    let (a, b) = (a?, b?);
    match (single(&a), single(&b)) {
        (Some(x), Some(y)) => exact_sum(x, y),
        _ => ordered(a.sum(b)),
    }
}
fn sub(a: E, b: E) -> E {
    add(a, neg(b))
}
fn mul(a: E, b: E) -> E {
    let (a, b) = (a?, b?);
    match (single(&a), single(&b)) {
        (Some(x), Some(y)) => exact_product(x, y),
        // An exact zero annihilates any finite enclosure exactly.
        (Some(z), _) | (_, Some(z)) if z == 0.0 && a.is_finite() && b.is_finite() => ex(0.0),
        _ if a.is_finite() && b.is_finite() => ordered(a.product(b)),
        _ => None,
    }
}
fn sq(a: E) -> E {
    let a = a?;
    match single(&a) {
        Some(x) => exact_product(x, x),
        None if a.is_finite() => ordered(a.square()),
        None => None,
    }
}
fn div(a: E, b: E) -> E {
    let (a, b) = (a?, b?);
    if let (Some(x), Some(y)) = (single(&a), single(&b)) {
        if y != 0.0 {
            let q = x / y;
            if q.is_finite()
                && (x == 0.0
                    || (x.abs() >= FMA_SAFE && q.abs() >= FMA_SAFE && q.mul_add(y, -x) == 0.0))
            {
                return ex(q);
            }
        }
    }
    if !a.is_finite() {
        return None;
    }
    a.quotient(b).and_then(ordered)
}
fn neg(a: E) -> E {
    a.map(|e| Enclosure {
        lo: -e.hi,
        hi: -e.lo,
    })
}
fn abs(a: E) -> E {
    let e = a?;
    Some(if e.lo >= 0.0 {
        e
    } else if e.hi <= 0.0 {
        Enclosure {
            lo: -e.hi,
            hi: -e.lo,
        }
    } else {
        Enclosure {
            lo: 0.0,
            hi: e.hi.max(-e.lo),
        }
    })
}
fn root(a: E) -> E {
    let e = a?;
    // An exact square root of an exact value stays exact — `√D` of an
    // axis-aligned or Pythagorean axis — proven by a zero FMA residual (the
    // residual is exact when `x` is far from underflow).
    if let Some(x) = single(&e) {
        if x == 0.0 {
            return ex(0.0);
        }
        if x >= FMA_SAFE && x.is_finite() {
            let r = x.sqrt();
            if r.mul_add(r, -x) == 0.0 {
                return ex(r);
            }
        }
    }
    // A certified non-negative quantity may round to a slightly negative
    // lower bound; clamp it, since the true value is ≥ 0.
    Enclosure {
        lo: e.lo.max(0.0),
        hi: e.hi,
    }
    .sqrt()
}
fn scale(a: E, k: i8) -> E {
    match k {
        0 => ex(0.0),
        1 => a,
        -1 => neg(a),
        _ => mul(a, ex(k as f64)),
    }
}
/// An enclosure of `t / 2`: exact unless `t` is subnormal and odd.
fn half(t: f64) -> E {
    let h = t * 0.5;
    if h * 2.0 == t {
        ex(h)
    } else {
        Some(Enclosure {
            lo: h.next_down(),
            hi: h.next_up(),
        })
    }
}

/// The certified sign of `a`, or `None` when it straddles zero (or is
/// absent). A degenerate `[0, 0]` is exactly zero.
fn sign_of(a: E) -> Option<Ordering> {
    let e = a?;
    if e.lo == 0.0 && e.hi == 0.0 {
        return Some(Ordering::Equal);
    }
    e.sign()
}

/// Orders two enclosed values when the enclosures decide, else `None`.
fn order(a: E, b: E) -> Option<Ordering> {
    let (a, b) = (a?, b?);
    if a.hi < b.lo {
        Some(Ordering::Less)
    } else if a.lo > b.hi {
        Some(Ordering::Greater)
    } else if a.lo == a.hi && b.lo == b.hi && a.lo == b.lo {
        // Both exact singletons: equal values.
        Some(Ordering::Equal)
    } else {
        None
    }
}

fn dsign(x: &Dyadic) -> Ordering {
    if x.is_zero() {
        Ordering::Equal
    } else if x.is_negative() {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

fn d(x: f64) -> Dyadic {
    Dyadic::from_f64(x)
}

fn one() -> Dyadic {
    d(1.0)
}

/// A direction `(m₀·uₓ + m₁·u_y, m₂·uₓ + m₃·u_y)` with `u = to − from`.
///
/// Covers a segment (`u` itself), the global 45° family (`u = (1, 0)` and
/// integer `m`), and 45° multiples of a previous segment, all WITHOUT
/// normalization: `(uₓ − u_y, uₓ + u_y)` is `u` turned 45° and scaled by
/// `√2`, and positive scaling changes neither a ray nor a projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dir {
    from: Point,
    to: Point,
    m: [i8; 4],
}

impl Dir {
    /// `to − from`.
    pub(crate) fn between(from: Point, to: Point) -> Self {
        Self {
            from,
            to,
            m: [1, 0, 0, 1],
        }
    }

    /// The integer direction `(x, y)`.
    pub(crate) fn constant(x: i8, y: i8) -> Self {
        Self {
            from: Point::new(0.0, 0.0),
            to: Point::new(1.0, 0.0),
            m: [x, 0, y, 0],
        }
    }

    /// `to − from` turned by `eighths · 45°` counter-clockwise, unnormalized.
    pub(crate) fn turned(from: Point, to: Point, eighths: i8) -> Self {
        let m = match eighths.rem_euclid(8) {
            0 => [1, 0, 0, 1],
            1 => [1, -1, 1, 1],
            2 => [0, -1, 1, 0],
            3 => [-1, -1, 1, -1],
            4 => [-1, 0, 0, -1],
            5 => [-1, 1, -1, -1],
            6 => [0, 1, -1, 0],
            _ => [1, 1, -1, 1],
        };
        Self { from, to, m }
    }

    fn enclose(&self) -> (E, E) {
        let (ux, uy) = (dif(self.to.x, self.from.x), dif(self.to.y, self.from.y));
        if self.m == [1, 0, 0, 1] {
            return (ux, uy);
        }
        let [a, b, c, e] = self.m;
        (
            add(scale(ux, a), scale(uy, b)),
            add(scale(ux, c), scale(uy, e)),
        )
    }

    fn exact(&self) -> (Dyadic, Dyadic) {
        let ux = d(self.to.x).sub(&d(self.from.x));
        let uy = d(self.to.y).sub(&d(self.from.y));
        let k = |v: &Dyadic, m: i8| match m {
            0 => Dyadic::zero(),
            1 => v.clone(),
            -1 => v.neg(),
            _ => v.mul(&d(m as f64)),
        };
        let [a, b, c, e] = self.m;
        (k(&ux, a).add(&k(&uy, b)), k(&ux, c).add(&k(&uy, e)))
    }

    /// Certified signs of both components.
    fn signs(&self) -> (Ordering, Ordering) {
        let (x, y) = self.enclose();
        match (sign_of(x), sign_of(y)) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                let (x, y) = self.exact();
                (dsign(&x), dsign(&y))
            }
        }
    }

    /// The sign of `self × other` (positive: `other` is counter-clockwise).
    fn cross(&self, o: &Self) -> Ordering {
        let ((ax, ay), (bx, by)) = (self.enclose(), o.enclose());
        sign_of(sub(mul(ax, by), mul(ay, bx))).unwrap_or_else(|| {
            let ((ax, ay), (bx, by)) = (self.exact(), o.exact());
            dsign(&ax.mul(&by).sub(&ay.mul(&bx)))
        })
    }

    /// Whether this is the zero vector.
    pub(crate) fn is_zero(&self) -> bool {
        self.signs() == (Ordering::Equal, Ordering::Equal)
    }

    /// R6.2.1's angular order over `[−180°, 180°)`, exactly: lower half-plane
    /// (including `−x`) first, then counter-clockwise.
    pub(crate) fn cmp_angle(&self, o: &Self) -> Ordering {
        let half = |d: &Self| {
            let (sx, sy) = d.signs();
            u8::from(!(sy == Ordering::Less || (sy == Ordering::Equal && sx == Ordering::Less)))
        };
        half(self)
            .cmp(&half(o))
            .then_with(|| self.cross(o).reverse())
    }

    /// Whether both are the same ray (parallel, same sense), exactly.
    pub(crate) fn same_ray(&self, o: &Self) -> bool {
        if self.is_zero() || o.is_zero() || self.cross(o) != Ordering::Equal {
            return false;
        }
        let ((ax, ay), (bx, by)) = (self.enclose(), o.enclose());
        let dot = sign_of(add(mul(ax, bx), mul(ay, by))).unwrap_or_else(|| {
            let ((ax, ay), (bx, by)) = (self.exact(), o.exact());
            dsign(&ax.mul(&bx).add(&ay.mul(&by)))
        });
        dot == Ordering::Greater
    }

    /// An approximate direction, for display and guides.
    pub(crate) fn approx(&self) -> (f64, f64) {
        let (x, y) = self.enclose();
        (
            x.map_or(f64::NAN, |e| e.mid()),
            y.map_or(f64::NAN, |e| e.mid()),
        )
    }
}

/// A linear feature's direction enclosures `(vₓ, v_y, D)`, prepared once
/// per feature (at index sync) instead of once per query — the part of a
/// witness that does not depend on the pointer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LinePrep([E; 3]);

impl LinePrep {
    pub(crate) fn of(dir: &Dir) -> Self {
        let (vx, vy) = dir.enclose();
        Self([vx, vy, add(sq(vx), sq(vy))])
    }
}

/// The pointer-independent quantities of an ideal rectangle, enclosed:
/// `h²·D` and `h·√D` (`h` the half thickness), and for openings `k²·D`,
/// `k·√D` (`k` the half width) and `u·v`. Prepared once per rectangle at index sync
/// ([`PreparedRect`]); computed on the fly otherwise.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RectConsts {
    h2d: E,
    hrd: E,
    k2d: E,
    krd: E,
    uvx: E,
    uvy: E,
}

impl RectConsts {
    fn of(shape: &Shape, line: &LinePrep) -> Self {
        let Shape::Rect {
            thickness, along, ..
        } = *shape
        else {
            unreachable!("RectConsts of a rectangle")
        };
        let [vx, vy, dd] = line.0;
        let rd = root(dd);
        let h = half(thickness);
        let (k, uvx, uvy) = match along {
            Along::Wall => (ex(0.0), ex(0.0), ex(0.0)),
            Along::Opening { u, width } => (half(width), mul(ex(u), vx), mul(ex(u), vy)),
        };
        Self {
            h2d: mul(sq(h), dd),
            hrd: mul(h, rd),
            k2d: mul(sq(k), dd),
            krd: mul(k, rd),
            uvx,
            uvy,
        }
    }
}

/// A point's parameter `(q − origin)·v` along a prepared line, enclosed
/// once (at index build) so per-query comparisons against it are a pair of
/// float comparisons; [`Witness::cmp_param`] falls back to exact arithmetic.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Param {
    q: Point,
    enc: E,
}

impl Param {
    pub(crate) fn of(origin: Point, prep: &LinePrep, q: Point) -> Self {
        let [vx, vy, _] = prep.0;
        Self {
            q,
            enc: add(mul(dif(q.x, origin.x), vx), mul(dif(q.y, origin.y), vy)),
        }
    }
}

/// How far a linear feature extends from its origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Extent {
    /// The closed segment from the origin to `origin + d` (`end` is that
    /// point, stored).
    Segment { end: Point },
    /// The whole line.
    Line,
    /// The open ray `origin + t·d`, `t > 0` (R6.2.2 excludes `t ≤ 0`).
    Ray,
}

/// What an ideal rectangle spans along its axis `a → b`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Along {
    /// The whole axis: a wall's butt-capped rectangle (R3.0.2).
    Wall,
    /// `width` centred at parameter `u`: an opening's band (R4.0.3), over the
    /// host's full thickness.
    Opening { u: f64, width: f64 },
}

/// The feature a witness measures to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    Point(Point),
    Lattice {
        i: i64,
        j: i64,
        s: f64,
    },
    Linear {
        origin: Point,
        dir: Dir,
        extent: Extent,
    },
    Rect {
        a: Point,
        b: Point,
        thickness: f64,
        along: Along,
    },
}

/// Where on its feature a witness's closest point lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Branch {
    /// A point feature, or the pointer inside a rectangle.
    Fixed,
    /// A linear feature's origin (`S ≤ 0` on a segment).
    Start,
    /// A segment's end (`S ≥ D`).
    End,
    /// Strictly inside a segment, or anywhere on a line or ray.
    Interior,
    /// A rectangle's boundary, outside it: `lon`/`lat` say which clamps are
    /// active, `-1`/`+1` for the low/high side, `0` for none.
    Outside { lon: i8, lat: i8 },
    /// A ray behind (or at) its origin, which R6.2.2 excludes.
    Rejected,
}

/// Exact intermediates of a linear or rectangle witness.
#[derive(Debug, Clone)]
struct Core {
    /// The direction (or axis), `D`, `S`, `C`.
    vx: Dyadic,
    vy: Dyadic,
    dd: Dyadic,
    s: Dyadic,
    c: Dyadic,
    /// Rectangles: half thickness, and for openings `u`, half width and
    /// `M = S − u·D`.
    h: Dyadic,
    u: Dyadic,
    k: Dyadic,
    m: Dyadic,
}

/// A squared pick radius, held exactly.
#[derive(Debug)]
pub(crate) struct Radius {
    enc: E,
    exact: OnceCell<Quad>,
    px: f64,
    zoom: f64,
}

impl Radius {
    /// A radius in plan millimetres, already converted by the caller.
    pub(crate) fn mm(r: f64) -> Self {
        Self::screen(r, 1.0)
    }

    /// `px / zoom` millimetres, kept as the quotient so `T/z` is never
    /// rounded (R7.1.3's logical-pixel radius over the view scale).
    /// `zoom` is logical pixels per millimetre and must be positive.
    pub(crate) fn screen(px: f64, zoom: f64) -> Self {
        Self {
            enc: div(sq(ex(px)), sq(ex(zoom))),
            exact: OnceCell::new(),
            px,
            zoom,
        }
    }

    fn exact(&self) -> &Quad {
        self.exact
            .get_or_init(|| Quad::ratio(d(self.px).square(), d(self.zoom).square()))
    }

    /// A binary64 search radius at least the exact one, for broad phases.
    pub(crate) fn search_mm(&self) -> f64 {
        if self.zoom == 1.0 {
            self.px
        } else {
            (self.px / self.zoom).next_up()
        }
    }
}

/// One candidate's exact-on-demand distance and closest point. See the
/// module documentation.
#[derive(Debug, Clone)]
pub(crate) struct Witness {
    p: Point,
    shape: Shape,
    branch: Branch,
    dist: E,
    x: E,
    y: E,
    /// `(S, D)` enclosures of a linear or rectangle shape, kept for later
    /// parameter comparisons (the rest of the construction-time intervals
    /// are not stored: a witness is moved around and should stay small).
    sd: [E; 2],
    /// Everything exact, allocated on the first undecided comparison only:
    /// an ordinary witness is a few dozen bytes and never touches the heap.
    lazy: OnceCell<Box<Lazy>>,
}

/// The exact state of a witness.
#[derive(Debug, Clone)]
struct Lazy {
    core: Option<Core>,
    dist: OnceCell<Quad>,
    x: OnceCell<Quad>,
    y: OnceCell<Quad>,
}

impl Witness {
    /// The witness of `shape` for pointer `p`. Coordinates must be finite.
    pub(crate) fn new(p: Point, shape: Shape) -> Self {
        if let Shape::Point(q) = shape {
            return Self::point(p, q);
        }
        Self::prepared(p, shape, None)
    }

    /// A stored point's witness: only the distance needs computing.
    fn point(p: Point, q: Point) -> Self {
        Self {
            p,
            shape: Shape::Point(q),
            branch: Branch::Fixed,
            dist: add(sq(dif(p.x, q.x)), sq(dif(p.y, q.y))),
            x: ex(q.x),
            y: ex(q.y),
            sd: [None; 2],
            lazy: OnceCell::new(),
        }
    }

    /// [`Self::new`] with the feature's direction enclosures supplied.
    /// `prep` must be [`LinePrep::of`] the shape's own direction.
    pub(crate) fn prepared(p: Point, shape: Shape, prep: Option<&LinePrep>) -> Self {
        Self::build(p, shape, prep, None)
    }

    fn build(p: Point, shape: Shape, prep: Option<&LinePrep>, consts: Option<&RectConsts>) -> Self {
        let mut w = Self {
            p,
            shape,
            branch: Branch::Fixed,
            dist: None,
            x: None,
            y: None,
            sd: [None; 2],
            lazy: OnceCell::new(),
        };
        let basic = if matches!(shape, Shape::Linear { .. } | Shape::Rect { .. }) {
            w.compute_basics(prep)
        } else {
            [None; 5]
        };
        w.sd = [basic[3], basic[2]];
        // Borrowed, not copied: the constants are a couple of hundred bytes
        // and a pointer query builds several of these.
        let owned;
        let rc = match (shape, consts) {
            (Shape::Rect { a, b, .. }, None) => {
                let line = prep
                    .copied()
                    .unwrap_or_else(|| LinePrep::of(&Dir::between(a, b)));
                owned = RectConsts::of(&shape, &line);
                Some(&owned)
            }
            (_, c) => c,
        };
        w.branch = w.decide_branch(&basic, rc);
        w.enclose(&basic, rc);
        w
    }

    /// The witness of a pointer KNOWN to be inside or on rectangle `shape`
    /// (the caller has certified it): distance 0, closest point `p`.
    fn inside(p: Point, shape: Shape) -> Self {
        Self {
            p,
            shape,
            branch: Branch::Fixed,
            dist: ex(0.0),
            x: ex(p.x),
            y: ex(p.y),
            sd: [None; 2],
            lazy: OnceCell::new(),
        }
    }

    pub(crate) fn branch(&self) -> Branch {
        self.branch
    }

    // --- exact intermediates ---------------------------------------------

    fn lazy(&self) -> &Lazy {
        self.lazy.get_or_init(|| {
            Box::new(Lazy {
                core: matches!(self.shape, Shape::Linear { .. } | Shape::Rect { .. })
                    .then(|| self.compute_core()),
                dist: OnceCell::new(),
                x: OnceCell::new(),
                y: OnceCell::new(),
            })
        })
    }

    fn core(&self) -> &Core {
        self.lazy()
            .core
            .as_ref()
            .expect("core of a linear or rectangle witness")
    }

    fn compute_core(&self) -> Core {
        {
            let (px, py) = (d(self.p.x), d(self.p.y));
            let zero = Dyadic::zero;
            match self.shape {
                Shape::Linear { origin, dir, .. } => {
                    let (vx, vy) = dir.exact();
                    let (wx, wy) = (px.sub(&d(origin.x)), py.sub(&d(origin.y)));
                    Core {
                        dd: vx.square().add(&vy.square()),
                        s: wx.mul(&vx).add(&wy.mul(&vy)),
                        c: vx.mul(&wy).sub(&vy.mul(&wx)),
                        vx,
                        vy,
                        h: zero(),
                        u: zero(),
                        k: zero(),
                        m: zero(),
                    }
                }
                Shape::Rect {
                    a,
                    b,
                    thickness,
                    along,
                } => {
                    let (vx, vy) = (d(b.x).sub(&d(a.x)), d(b.y).sub(&d(a.y)));
                    let (wx, wy) = (px.sub(&d(a.x)), py.sub(&d(a.y)));
                    let dd = vx.square().add(&vy.square());
                    let s = wx.mul(&vx).add(&wy.mul(&vy));
                    let (u, k) = match along {
                        Along::Wall => (zero(), zero()),
                        Along::Opening { u, width } => (d(u), d(width).halve()),
                    };
                    let m = s.sub(&u.mul(&dd));
                    Core {
                        c: vx.mul(&wy).sub(&vy.mul(&wx)),
                        h: d(thickness).halve(),
                        vx,
                        vy,
                        dd,
                        s,
                        u,
                        k,
                        m,
                    }
                }
                Shape::Point(_) | Shape::Lattice { .. } => unreachable!("no core for points"),
            }
        }
    }

    /// Enclosures of `(w, v, D, S, C)` for a linear or rectangle shape.
    #[allow(clippy::type_complexity)]
    fn basics(b: &[E; 5]) -> (E, E, E, E, E, E, E) {
        let [vx, vy, dd, s, c] = *b;
        (None, None, vx, vy, dd, s, c)
    }

    fn compute_basics(&self, prep: Option<&LinePrep>) -> [E; 5] {
        let origin = match self.shape {
            Shape::Linear { origin, .. } => origin,
            Shape::Rect { a, .. } => a,
            _ => unreachable!(),
        };
        let [vx, vy, dd] = match (prep, self.shape) {
            (Some(p), _) => p.0,
            (None, Shape::Linear { dir, .. }) => LinePrep::of(&dir).0,
            (None, Shape::Rect { a, b, .. }) => LinePrep::of(&Dir::between(a, b)).0,
            _ => unreachable!(),
        };
        let (wx, wy) = (dif(self.p.x, origin.x), dif(self.p.y, origin.y));
        let s = add(mul(wx, vx), mul(wy, vy));
        let c = sub(mul(vx, wy), mul(vy, wx));
        [vx, vy, dd, s, c]
    }

    // --- branch ------------------------------------------------------------

    fn decide_branch(&self, b: &[E; 5], rc: Option<&RectConsts>) -> Branch {
        match self.shape {
            Shape::Point(_) | Shape::Lattice { .. } => Branch::Fixed,
            Shape::Linear { extent, .. } => {
                let (_, _, _, _, dd, s, _) = Self::basics(b);
                // `D > 0` is certified by its lower bound in the ordinary case.
                let ds = match dd {
                    Some(e) if e.lo > 0.0 => Ordering::Greater,
                    _ => sign_of(dd).unwrap_or_else(|| dsign(&self.core().dd)),
                };
                if ds == Ordering::Equal {
                    // A zero direction: a segment is its origin point; a
                    // line or ray has no direction and admits nothing.
                    return match extent {
                        Extent::Segment { .. } => Branch::Start,
                        _ => Branch::Rejected,
                    };
                }
                if extent == Extent::Line {
                    return Branch::Interior;
                }
                let s_sign = sign_of(s).unwrap_or_else(|| dsign(&self.core().s));
                match extent {
                    Extent::Line => Branch::Interior,
                    Extent::Ray => {
                        if s_sign == Ordering::Greater {
                            Branch::Interior
                        } else {
                            Branch::Rejected
                        }
                    }
                    Extent::Segment { .. } => {
                        if s_sign != Ordering::Greater {
                            return Branch::Start;
                        }
                        let past = sign_of(sub(s, dd)).unwrap_or_else(|| {
                            let c = self.core();
                            dsign(&c.s.sub(&c.dd))
                        });
                        if past == Ordering::Less {
                            Branch::Interior
                        } else {
                            Branch::End
                        }
                    }
                }
            }
            Shape::Rect { along, .. } => {
                let rc = rc.expect("rectangle constants");
                let (lon, lat) = (self.lon_side(b, along, rc), self.lat_side(b, rc));
                if lon == 0 && lat == 0 {
                    Branch::Fixed
                } else {
                    Branch::Outside { lon, lat }
                }
            }
        }
    }

    /// `−1`/`+1` when `C` lies beyond `∓h·√D` (outside the band), else 0:
    /// the sign of `C² − h²·D`, then of `C`. Boundary inclusive.
    fn lat_side(&self, b: &[E; 5], rc: &RectConsts) -> i8 {
        let (_, _, _, _, _, _, c) = Self::basics(b);
        let excess = sub(sq(c), rc.h2d);
        let out = sign_of(excess).unwrap_or_else(|| {
            let k = self.core();
            dsign(&k.c.square().sub(&k.h.square().mul(&k.dd)))
        });
        if out != Ordering::Greater {
            return 0;
        }
        let cs = sign_of(c).unwrap_or_else(|| dsign(&self.core().c));
        if cs == Ordering::Less {
            -1
        } else {
            1
        }
    }

    fn lon_side(&self, b: &[E; 5], along: Along, rc: &RectConsts) -> i8 {
        let (_, _, _, _, dd, s, _) = Self::basics(b);
        match along {
            Along::Wall => {
                let below = sign_of(s).unwrap_or_else(|| dsign(&self.core().s));
                if below == Ordering::Less {
                    return -1;
                }
                let past = sign_of(sub(s, dd)).unwrap_or_else(|| {
                    let k = self.core();
                    dsign(&k.s.sub(&k.dd))
                });
                if past == Ordering::Greater {
                    1
                } else {
                    0
                }
            }
            Along::Opening { u, .. } => {
                let m = sub(s, mul(ex(u), dd));
                let excess = sub(sq(m), rc.k2d);
                let out = sign_of(excess).unwrap_or_else(|| {
                    let k = self.core();
                    dsign(&k.m.square().sub(&k.k.square().mul(&k.dd)))
                });
                if out != Ordering::Greater {
                    return 0;
                }
                let ms = sign_of(m).unwrap_or_else(|| dsign(&self.core().m));
                if ms == Ordering::Less {
                    -1
                } else {
                    1
                }
            }
        }
    }

    // --- enclosures --------------------------------------------------------

    fn enclose(&mut self, b: &[E; 5], rc: Option<&RectConsts>) {
        let p = self.p;
        match self.shape {
            Shape::Point(q) => {
                self.dist = add(sq(dif(p.x, q.x)), sq(dif(p.y, q.y)));
                (self.x, self.y) = (ex(q.x), ex(q.y));
            }
            Shape::Lattice { i, j, s } => {
                let idx = |n: i64| {
                    let f = n as f64;
                    if f as i64 == n && f.abs() < 9.0e15 {
                        ex(f)
                    } else {
                        Some(Enclosure {
                            lo: f.next_down(),
                            hi: f.next_up(),
                        })
                    }
                };
                let (x, y) = (mul(idx(i), ex(s)), mul(idx(j), ex(s)));
                self.dist = add(sq(sub(ex(p.x), x)), sq(sub(ex(p.y), y)));
                (self.x, self.y) = (x, y);
            }
            Shape::Linear { origin, extent, .. } => {
                let (_, _, vx, vy, dd, s, c) = Self::basics(b);
                let at = |q: Point| (add(sq(dif(p.x, q.x)), sq(dif(p.y, q.y))), ex(q.x), ex(q.y));
                (self.dist, self.x, self.y) = match (self.branch, extent) {
                    (Branch::Start, _) => at(origin),
                    (Branch::End, Extent::Segment { end }) => at(end),
                    // `(v·S)/D`, not `v·(S/D)`: on axis-aligned and grid
                    // geometry the former is exact where the latter is not,
                    // and an exact singleton is what lets the output be the
                    // exact point and ties decide without big integers.
                    (Branch::Interior, _) => (
                        div(sq(c), dd),
                        add(ex(origin.x), div(mul(vx, s), dd)),
                        add(ex(origin.y), div(mul(vy, s), dd)),
                    ),
                    _ => (None, None, None),
                };
            }
            Shape::Rect { a, along, .. } => {
                let rc = rc.expect("rectangle constants");
                if self.branch == Branch::Fixed {
                    (self.dist, self.x, self.y) = (ex(0.0), ex(p.x), ex(p.y));
                    return;
                }
                let Branch::Outside { lon, lat } = self.branch else {
                    unreachable!()
                };
                let (_, _, vx, vy, dd, s, c) = Self::basics(b);
                // Longitudinal excess U and clamp Lc (both in units of D).
                let (u_ex, lc, base_x, base_y) = match along {
                    Along::Wall => {
                        let (u_ex, lc) = match lon {
                            -1 => (neg(s), ex(0.0)),
                            1 => (sub(s, dd), dd),
                            _ => (ex(0.0), s),
                        };
                        (u_ex, lc, ex(a.x), ex(a.y))
                    }
                    Along::Opening { u, .. } => {
                        let m = sub(s, mul(ex(u), dd));
                        let krd = rc.krd;
                        let (u_ex, lc) = match lon {
                            0 => (ex(0.0), m),
                            side => (sub(abs(m), krd), if side < 0 { neg(krd) } else { krd }),
                        };
                        (u_ex, lc, add(ex(a.x), rc.uvx), add(ex(a.y), rc.uvy))
                    }
                };
                let hrd = rc.hrd;
                let (v_ex, cc) = match lat {
                    0 => (ex(0.0), c),
                    side => (sub(abs(c), hrd), if side < 0 { neg(hrd) } else { hrd }),
                };
                let nonneg = |e: E| {
                    e.map(|e| Enclosure {
                        lo: e.lo.max(0.0),
                        hi: e.hi.max(0.0),
                    })
                };
                self.dist = div(add(sq(nonneg(u_ex)), sq(nonneg(v_ex))), dd);
                self.x = add(base_x, div(sub(mul(vx, lc), mul(vy, cc)), dd));
                self.y = add(base_y, div(add(mul(vy, lc), mul(vx, cc)), dd));
            }
        }
    }

    // --- exact values -------------------------------------------------------

    fn exact_dist(&self) -> &Quad {
        self.lazy().dist.get_or_init(|| {
            let p = self.p;
            let point = |q: Point| {
                let (dx, dy) = (d(p.x).sub(&d(q.x)), d(p.y).sub(&d(q.y)));
                Quad::ratio(dx.square().add(&dy.square()), one())
            };
            match self.shape {
                Shape::Point(q) => point(q),
                Shape::Lattice { .. } => {
                    let (x, y) = (self.exact_x(), self.exact_y());
                    let dx = d(p.x).sub(&x.x);
                    let dy = d(p.y).sub(&y.x);
                    Quad::ratio(dx.square().add(&dy.square()), one())
                }
                Shape::Linear { origin, extent, .. } => match (self.branch, extent) {
                    (Branch::Start, _) => point(origin),
                    (Branch::End, Extent::Segment { end }) => point(end),
                    _ => {
                        let k = self.core();
                        Quad::ratio(k.c.square(), k.dd.clone())
                    }
                },
                Shape::Rect { .. } => {
                    let Branch::Outside { lon, lat } = self.branch else {
                        return Quad::ratio(Dyadic::zero(), one());
                    };
                    let k = self.core();
                    let (mut a, mut b) = (Dyadic::zero(), Dyadic::zero());
                    // Rational longitudinal excess (walls).
                    if let Shape::Rect {
                        along: Along::Wall, ..
                    } = self.shape
                    {
                        let u = match lon {
                            -1 => k.s.neg(),
                            1 => k.s.sub(&k.dd),
                            _ => Dyadic::zero(),
                        };
                        a = a.add(&u.square());
                    } else if lon != 0 {
                        // (|M| − k√D)² = M² + k²D − 2k|M|√D.
                        a = a.add(&k.m.square()).add(&k.k.square().mul(&k.dd));
                        b = b.add(&k.k.mul(&k.m.abs()).add(&k.k.mul(&k.m.abs())));
                    }
                    if lat != 0 {
                        a = a.add(&k.c.square()).add(&k.h.square().mul(&k.dd));
                        b = b.add(&k.h.mul(&k.c.abs()).add(&k.h.mul(&k.c.abs())));
                    }
                    Quad::new(a, b.neg(), k.dd.clone(), k.dd.clone())
                }
            }
        })
    }

    fn exact_coord(&self, is_x: bool) -> Quad {
        let pick = |q: Point| if is_x { q.x } else { q.y };
        match self.shape {
            Shape::Point(q) => Quad::ratio(d(pick(q)), one()),
            Shape::Lattice { i, j, s } => {
                Quad::ratio(Dyadic::from_i64(if is_x { i } else { j }).mul(&d(s)), one())
            }
            Shape::Linear { origin, extent, .. } => match (self.branch, extent) {
                (Branch::Start, _) => Quad::ratio(d(pick(origin)), one()),
                (Branch::End, Extent::Segment { end }) => Quad::ratio(d(pick(end)), one()),
                _ => {
                    let k = self.core();
                    let v = if is_x { &k.vx } else { &k.vy };
                    Quad::ratio(d(pick(origin)).mul(&k.dd).add(&v.mul(&k.s)), k.dd.clone())
                }
            },
            Shape::Rect { a, .. } => {
                let Branch::Outside { lon, lat } = self.branch else {
                    return Quad::ratio(d(pick(self.p)), one());
                };
                let k = self.core();
                // q = base + (v·Lc + Jv·Cc)/D with Jv = (−v_y, vₓ); each clamp
                // is rational (r) or a multiple of √D (ρ).
                let (lr, lq) = match (self.shape, lon) {
                    (
                        Shape::Rect {
                            along: Along::Wall, ..
                        },
                        -1,
                    ) => (Dyadic::zero(), Dyadic::zero()),
                    (
                        Shape::Rect {
                            along: Along::Wall, ..
                        },
                        1,
                    ) => (k.dd.clone(), Dyadic::zero()),
                    (
                        Shape::Rect {
                            along: Along::Wall, ..
                        },
                        _,
                    ) => (k.s.clone(), Dyadic::zero()),
                    (_, 0) => (k.m.clone(), Dyadic::zero()),
                    (_, side) => (
                        Dyadic::zero(),
                        if side < 0 { k.k.neg() } else { k.k.clone() },
                    ),
                };
                let (cr, cq) = match lat {
                    0 => (k.c.clone(), Dyadic::zero()),
                    side => (
                        Dyadic::zero(),
                        if side < 0 { k.h.neg() } else { k.h.clone() },
                    ),
                };
                // x: vₓ·L − v_y·C;  y: v_y·L + vₓ·C.
                let (f, g) = if is_x {
                    (k.vx.clone(), k.vy.neg())
                } else {
                    (k.vy.clone(), k.vx.clone())
                };
                let base = d(pick(a)).add(&k.u.mul(if is_x { &k.vx } else { &k.vy }));
                let x = base.mul(&k.dd).add(&f.mul(&lr)).add(&g.mul(&cr));
                let y = f.mul(&lq).add(&g.mul(&cq));
                Quad::new(x, y, k.dd.clone(), k.dd.clone())
            }
        }
    }

    fn exact_x(&self) -> &Quad {
        self.lazy().x.get_or_init(|| self.exact_coord(true))
    }

    fn exact_y(&self) -> &Quad {
        self.lazy().y.get_or_init(|| self.exact_coord(false))
    }

    // --- decisions ----------------------------------------------------------

    /// `[d², x, y]` when all three are EXACT binary64 values (singleton
    /// enclosures) — then plain float comparison of these keys IS the exact
    /// order of [`Self::cmp_dist`] then [`Self::cmp_xy`]. Common on grid-
    /// aligned plans and for every stored point.
    pub(crate) fn exact_key(&self) -> Option<[f64; 3]> {
        let one = |e: E| e.filter(|e| e.lo == e.hi).map(|e| e.lo);
        Some([one(self.dist)?, one(self.x)?, one(self.y)?])
    }

    /// Exact order of the two squared distances (each witness may have its
    /// own pointer; R6/R7 always compare from one).
    pub(crate) fn cmp_dist(&self, other: &Self) -> Ordering {
        order(self.dist, other.dist).unwrap_or_else(|| self.exact_dist().cmp(other.exact_dist()))
    }

    /// Exact lexicographic order of the two closest points.
    pub(crate) fn cmp_xy(&self, other: &Self) -> Ordering {
        order(self.x, other.x)
            .unwrap_or_else(|| self.exact_x().cmp(other.exact_x()))
            .then_with(|| {
                order(self.y, other.y).unwrap_or_else(|| self.exact_y().cmp(other.exact_y()))
            })
    }

    /// Whether the closest point is within `r`, inclusive, exactly.
    pub(crate) fn within(&self, r: &Radius) -> bool {
        if self.branch == Branch::Rejected {
            return false;
        }
        order(self.dist, r.enc).unwrap_or_else(|| self.exact_dist().cmp(r.exact()))
            != Ordering::Greater
    }

    /// Whether the closest point IS `q`, exactly.
    pub(crate) fn is_at(&self, q: Point) -> bool {
        let eq = |e: E, v: f64, exact: &dyn Fn() -> Ordering| match order(e, ex(v)) {
            Some(o) => o == Ordering::Equal,
            None => exact() == Ordering::Equal,
        };
        eq(self.x, q.x, &|| {
            self.exact_x().cmp(&Quad::ratio(d(q.x), one()))
        }) && eq(self.y, q.y, &|| {
            self.exact_y().cmp(&Quad::ratio(d(q.y), one()))
        })
    }

    /// The closest point in binary64: the midpoint of the certified
    /// coordinate enclosure (error ≤ half its width), exact for singleton
    /// enclosures and for a pointer on the feature. When an enclosure was
    /// lost, the exact coordinates converted in scaled arithmetic
    /// ([`Quad::approx`]) — always finite for finite inputs.
    pub(crate) fn output(&self) -> Point {
        // On the feature (exact distance zero), the closest point IS the
        // pointer: report it exactly. Exact work only when the distance
        // enclosure reaches zero.
        let on_feature = match self.dist {
            Some(e) if e.lo == 0.0 && e.hi == 0.0 => true,
            Some(e) if e.lo > 0.0 => false,
            _ => self.exact_dist().sign() == Ordering::Equal,
        };
        if !matches!(self.shape, Shape::Point(_) | Shape::Lattice { .. }) && on_feature {
            return self.p;
        }
        if let Shape::Point(q) = self.shape {
            return q;
        }
        match (self.x, self.y) {
            (Some(x), Some(y)) => Point::new(x.mid(), y.mid()),
            _ => self.fallback_point(),
        }
    }

    /// The closest point from its EXACT coordinates, converted in scaled
    /// arithmetic: the path for a witness whose coordinate enclosure was lost
    /// (a direction so short or long that the float evaluation under- or
    /// overflowed). Always finite for finite inputs, and never a stand-in
    /// point — an outside rectangle reports its true closest point, not `p`.
    fn fallback_point(&self) -> Point {
        Point::new(self.exact_x().approx(), self.exact_y().approx())
    }

    /// Approximate squared distance, for display only.
    ///
    /// When the enclosure was lost, this is the squared distance from the
    /// pointer to the REPORTED point ([`Self::output`]): non-negative and
    /// consistent with the point shown. Evaluating the expanded exact form
    /// `(A − B·√D)/D` in floats instead cancels catastrophically (Codex
    /// GPT-6.1-Sol review: negative squared distances).
    pub(crate) fn approx_dist2(&self) -> f64 {
        match self.dist {
            Some(e) => e.mid(),
            None => {
                let q = self.output();
                (q.x - self.p.x).powi(2) + (q.y - self.p.y).powi(2)
            }
        }
    }

    /// The exact order of the pointer's projection parameter `S` against
    /// `p`'s, both along this linear witness's own origin and direction.
    pub(crate) fn cmp_param(&self, p: &Param) -> Ordering {
        order(self.sd[0], p.enc).unwrap_or_else(|| {
            let Shape::Linear { origin, .. } = self.shape else {
                unreachable!("cmp_param takes a linear witness")
            };
            let k = self.core();
            let exact = d(p.q.x)
                .sub(&d(origin.x))
                .mul(&k.vx)
                .add(&d(p.q.y).sub(&d(origin.y)).mul(&k.vy));
            k.s.cmp(&exact)
        })
    }
}

/// The lattice indices `n` whose points `n·s` can lie within `r` of `c`:
/// `⌊(c − r)/s⌋ ..= ⌈(c + r)/s⌉` over certified enclosures, so no admissible
/// index is missed and at most one extra on each side is included. `None`
/// when the bounds are not finite.
pub(crate) fn lattice_window(c: f64, r: f64, s: f64) -> Option<(f64, f64)> {
    let lo = div(sub(ex(c), ex(r)), ex(s))?.lo.floor();
    let hi = div(add(ex(c), ex(r)), ex(s))?.hi.ceil();
    Some((lo, hi))
}

/// Whether the lattice point `(i·s, j·s)` is CERTAINLY farther than `r`
/// from `p` along x or y alone — a cheap sound rejection before building a
/// witness (most of a grid window lies outside the radius box).
pub(crate) fn lattice_certainly_outside(p: Point, r: f64, i: i64, j: i64, s: f64) -> bool {
    let beyond = |c: f64, n: i64| {
        // An index beyond 2^53 has no exact binary64 form: no pre-test (the
        // witness handles it exactly), rather than a bound to justify.
        if (n as f64) as i64 != n || (n as f64).abs() >= 9.0e15 {
            return false;
        }
        match sub(ex(c), mul(ex(n as f64), ex(s))) {
            Some(d) => d.lo > r || d.hi < -r,
            None => false,
        }
    };
    beyond(p.x, i) || beyond(p.y, j)
}

/// Exact, boundary-inclusive intersection of an ideal rectangle with an
/// axis-aligned box (R7.3.1), by separating axes: x, y, the axis `v` and its
/// normal `Jv`. Each test is `|centre gap| ≤ sum of support radii`, decided
/// in exact arithmetic with `√D` kept symbolic. Also exact for a line box.
pub(crate) fn rect_meets_box(shape: &Shape, b: &crate::spatial::Aabb) -> bool {
    let Shape::Rect {
        a,
        b: e,
        thickness,
        along,
    } = *shape
    else {
        unreachable!("rect_meets_box takes a rectangle")
    };
    // An exactly representable axis-aligned wall IS its box.
    if let Some(exact) = axis_box(shape) {
        return exact.intersects(b);
    }
    // A box containing the rectangle's conservative bounds meets it.
    if let Some(bounds) = rect_bounds(shape) {
        if b.min_x <= bounds.min_x
            && b.min_y <= bounds.min_y
            && b.max_x >= bounds.max_x
            && b.max_y >= bounds.max_y
        {
            return true;
        }
        if !bounds.intersects(b) {
            return false;
        }
    }
    // Each test is `rat + rad·√D ≥ 0` (see `exact_meets` for the
    // derivation). Decide all four in floats first; only a test whose
    // enclosure straddles zero runs the exact path.
    let (vx, vy) = (dif(e.x, a.x), dif(e.y, a.y));
    let dd = add(sq(vx), sq(vy));
    let rd = root(dd);
    let h = half(thickness);
    let (mx, my, len_rat, len_rad) = match along {
        Along::Wall => (
            mul(add(ex(a.x), ex(e.x)), ex(0.5)),
            mul(add(ex(a.y), ex(e.y)), ex(0.5)),
            mul(dd, ex(0.5)),
            ex(0.0),
        ),
        Along::Opening { u, width } => (
            add(ex(a.x), mul(ex(u), vx)),
            add(ex(a.y), mul(ex(u), vy)),
            ex(0.0),
            half(width),
        ),
    };
    let (cx, cy) = (
        mul(add(ex(b.min_x), ex(b.max_x)), ex(0.5)),
        mul(add(ex(b.min_y), ex(b.max_y)), ex(0.5)),
    );
    let (rx, ry) = (
        mul(dif(b.max_x, b.min_x), ex(0.5)),
        mul(dif(b.max_y, b.min_y), ex(0.5)),
    );
    let (gx, gy) = (sub(mx, cx), sub(my, cy));
    // Some(true/false) when certain, None when the exact path must decide.
    let fits = |rat: E, rad: E| -> Option<bool> {
        let v = add(rat, mul(rad, rd))?;
        if v.lo >= 0.0 {
            Some(true)
        } else if v.hi < 0.0 {
            Some(false)
        } else {
            None
        }
    };
    let axis_xy = |g: E, r: E, va: E, vb: E| {
        fits(
            add(mul(sub(r, abs(g)), dd), mul(len_rat, abs(va))),
            add(mul(h, abs(vb)), mul(len_rad, abs(va))),
        )
    };
    let gv = add(mul(gx, vx), mul(gy, vy));
    let box_v = add(mul(rx, abs(vx)), mul(ry, abs(vy)));
    let gj = sub(mul(gx, vy), mul(gy, vx));
    let box_j = add(mul(rx, abs(vy)), mul(ry, abs(vx)));
    let tests = [
        axis_xy(gx, rx, vx, vy),
        axis_xy(gy, ry, vy, vx),
        fits(add(sub(box_v, abs(gv)), len_rat), len_rad),
        fits(sub(box_j, abs(gj)), h),
    ];
    if tests.contains(&Some(false)) {
        return false;
    }
    if tests.iter().all(|t| *t == Some(true)) {
        return true;
    }
    exact_meets(shape, b)
}

/// [`rect_meets_box`]'s exact path: separating axes x, y, the axis `v` and
/// its normal `Jv`, each `|centre gap| ≤ sum of support radii` multiplied
/// through by `D` or `√D` so only `rat + rad·√D ≥ 0` remains.
fn exact_meets(shape: &Shape, b: &crate::spatial::Aabb) -> bool {
    let Shape::Rect {
        a,
        b: e,
        thickness,
        along,
    } = *shape
    else {
        unreachable!()
    };
    let (vx, vy) = (d(e.x).sub(&d(a.x)), d(e.y).sub(&d(a.y)));
    let dd = vx.square().add(&vy.square());
    let h = d(thickness).halve();
    // Rectangle centre m; half length along v is `(len_rat + len_rad·√D)/√D`
    // (wall: D/2 over √D; opening: half the width).
    let (mx, my, len_rat, len_rad) = match along {
        Along::Wall => (
            d(a.x).add(&d(e.x)).halve(),
            d(a.y).add(&d(e.y)).halve(),
            dd.halve(),
            Dyadic::zero(),
        ),
        Along::Opening { u, width } => (
            d(a.x).add(&d(u).mul(&vx)),
            d(a.y).add(&d(u).mul(&vy)),
            Dyadic::zero(),
            d(width).halve(),
        ),
    };
    let (cx, cy) = (
        d(b.min_x).add(&d(b.max_x)).halve(),
        d(b.min_y).add(&d(b.max_y)).halve(),
    );
    let (rx, ry) = (
        d(b.max_x).sub(&d(b.min_x)).halve(),
        d(b.max_y).sub(&d(b.min_y)).halve(),
    );
    let (gx, gy) = (mx.sub(&cx), my.sub(&cy));
    let fits = |rat: Dyadic, rad: Dyadic| crate::quad::sign1(&rat, &rad, &dd) != Ordering::Less;
    // Axes x and y (times D): (r − |g|)·D + len_rat·|vₐ| + (h·|v_b| + len_rad·|vₐ|)·√D ≥ 0.
    let axis_xy = |g: &Dyadic, r: &Dyadic, va: &Dyadic, vb: &Dyadic| {
        let rat = r.sub(&g.abs()).mul(&dd).add(&len_rat.mul(&va.abs()));
        let rad = h.mul(&vb.abs()).add(&len_rad.mul(&va.abs()));
        fits(rat, rad)
    };
    if !axis_xy(&gx, &rx, &vx, &vy) || !axis_xy(&gy, &ry, &vy, &vx) {
        return false;
    }
    // Axis v (times √D): |g·v| ≤ rx|vₓ| + ry|v_y| + half length·√D.
    let gv = gx.mul(&vx).add(&gy.mul(&vy));
    let box_v = rx.mul(&vx.abs()).add(&ry.mul(&vy.abs()));
    if !fits(box_v.sub(&gv.abs()).add(&len_rat), len_rad.clone()) {
        return false;
    }
    // Axis Jv (times √D): |g × v| ≤ rx|v_y| + ry|vₓ| + h·√D.
    let gj = gx.mul(&vy).sub(&gy.mul(&vx));
    let box_j = rx.mul(&vy.abs()).add(&ry.mul(&vx.abs()));
    fits(box_j.sub(&gj.abs()), h)
}

/// The four corners of an ideal rectangle in R3.2.4 canonical order
/// (counter-clockwise from the least `(x, y)`), as exact coordinates with
/// their enclosures — the region tie key of R7.1.3.
#[derive(Debug, Clone)]
pub(crate) struct Corners {
    /// Enclosures, the lazily built exact pair (boxed: two `Quad`s are
    /// hundreds of bytes and rarely needed), and the recipe to build it.
    pts: [Corner; 4],
    /// The corners holding the least x and the least y: the ideal bounding
    /// box's minimum (R7.3.2's marquee key).
    least_x: usize,
    least_y: usize,
}

/// One corner: its coordinate enclosures, the lazily built exact pair, and
/// the recipe that builds it.
type Corner = (E, E, OnceCell<Box<(Quad, Quad)>>, CornerRecipe);

#[derive(Debug, Clone, Copy)]
struct CornerRecipe {
    shape: Shape,
    /// End: −1 the low end, +1 the high end; side: −1 right, +1 left.
    end: i8,
    side: i8,
}

/// What all four corners of a rectangle share: `v`, `√D`, `h`, `k`.
struct CornerBasis {
    vx: E,
    vy: E,
    rd: E,
    h: E,
    k: E,
}

impl CornerBasis {
    fn of(shape: &Shape) -> Self {
        let Shape::Rect {
            a,
            b,
            thickness,
            along,
        } = *shape
        else {
            unreachable!()
        };
        let (vx, vy) = (dif(b.x, a.x), dif(b.y, a.y));
        Self {
            rd: root(add(sq(vx), sq(vy))),
            h: half(thickness),
            k: match along {
                Along::Wall => ex(0.0),
                Along::Opening { width, .. } => half(width),
            },
            vx,
            vy,
        }
    }
}

impl CornerRecipe {
    fn enclose(&self, basis: &CornerBasis) -> (E, E) {
        let Shape::Rect { a, b, along, .. } = self.shape else {
            unreachable!()
        };
        let CornerBasis { vx, vy, rd, h, k } = *basis;
        let (ex_, ey_) = match along {
            Along::Wall => {
                if self.end < 0 {
                    (ex(a.x), ex(a.y))
                } else {
                    (ex(b.x), ex(b.y))
                }
            }
            Along::Opening { u, .. } => {
                let t = |vv: E| add(mul(ex(u), vv), scale(div(mul(k, vv), rd), self.end));
                (add(ex(a.x), t(vx)), add(ex(a.y), t(vy)))
            }
        };
        // + side·h·Jv/√D, Jv = (−v_y, vₓ).
        let off = |vv: E| scale(div(mul(h, vv), rd), self.side);
        (sub(ex_, off(vy)), add(ey_, off(vx)))
    }

    fn exact(&self) -> (Quad, Quad) {
        let Shape::Rect {
            a,
            b,
            thickness,
            along,
        } = self.shape
        else {
            unreachable!()
        };
        let (vx, vy) = (d(b.x).sub(&d(a.x)), d(b.y).sub(&d(a.y)));
        let dd = vx.square().add(&vy.square());
        let h = d(thickness).halve();
        let sgn = |v: Dyadic, s: i8| if s < 0 { v.neg() } else { v };
        // corner = base + (end·len·v + side·h·Jv)/√D, written over D with a
        // √D coefficient: x = (base_x·D + (end·len·vₓ − side·h·v_y)·√D)/D.
        let (bx, by, len) = match along {
            Along::Wall => {
                let e = if self.end < 0 { a } else { b };
                (d(e.x), d(e.y), Dyadic::zero())
            }
            Along::Opening { u, width } => (
                d(a.x).add(&d(u).mul(&vx)),
                d(a.y).add(&d(u).mul(&vy)),
                sgn(d(width).halve(), self.end),
            ),
        };
        let hs = sgn(h, self.side);
        let x = Quad::new(
            bx.mul(&dd),
            len.mul(&vx).sub(&hs.mul(&vy)),
            dd.clone(),
            dd.clone(),
        );
        let y = Quad::new(by.mul(&dd), len.mul(&vy).add(&hs.mul(&vx)), dd.clone(), dd);
        (x, y)
    }
}

impl Corners {
    pub(crate) fn of(shape: Shape) -> Self {
        // Counter-clockwise: low end right, high end right, high end left,
        // low end left (right = −Jv side).
        let order = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
        // An exactly representable axis-aligned wall: each corner is an end
        // point moved by exactly ±h across the axis — `side·h·Jv/|v|` with
        // `Jv/|v| = (−sgn v_y, sgn vₓ)` — so the enclosures are exact
        // singletons without a root or a division.
        let axis = axis_box(&shape).and_then(|_| {
            let Shape::Rect {
                a, b, thickness, ..
            } = shape
            else {
                unreachable!()
            };
            let h = thickness * 0.5;
            let (sx, sy) = ((b.x - a.x).signum(), (b.y - a.y).signum());
            let (sx, sy) = (
                if a.x == b.x { 0.0 } else { sx },
                if a.y == b.y { 0.0 } else { sy },
            );
            let mut out = [(None, None); 4];
            for (i, &(end, side)) in order.iter().enumerate() {
                let e = if end < 0 { a } else { b };
                let off = side as f64 * h;
                out[i] = (
                    exact_sum(e.x, -off * sy).filter(|x| x.lo == x.hi),
                    exact_sum(e.y, off * sx).filter(|y| y.lo == y.hi),
                );
                out[i].0?;
                out[i].1?;
            }
            Some(out)
        });
        let basis = axis.is_none().then(|| CornerBasis::of(&shape));
        let mut i = 0;
        let pts = order.map(|(end, side)| {
            let r = CornerRecipe { shape, end, side };
            let (x, y) = match (&axis, &basis) {
                (Some(exact), _) => exact[i],
                (None, Some(basis)) => r.enclose(basis),
                _ => unreachable!(),
            };
            i += 1;
            (x, y, OnceCell::new(), r)
        });
        let mut c = Self {
            pts,
            least_x: 0,
            least_y: 0,
        };
        // Rotate so the least (x, y) comes first.
        let least = (1..4).fold(0, |best, i| {
            if c.cmp_pt(i, &c, best) == Ordering::Less {
                i
            } else {
                best
            }
        });
        c.pts.rotate_left(least);
        (c.least_x, c.least_y) = (c.least(true), c.least(false));
        c
    }

    fn exact(&self, i: usize) -> &(Quad, Quad) {
        let (_, _, cell, r) = &self.pts[i];
        cell.get_or_init(|| Box::new(r.exact()))
    }

    fn cmp_pt(&self, i: usize, other: &Self, j: usize) -> Ordering {
        let (ax, ay, ..) = &self.pts[i];
        let (bx, by, ..) = &other.pts[j];
        order(*ax, *bx)
            .unwrap_or_else(|| self.exact(i).0.cmp(&other.exact(j).0))
            .then_with(|| order(*ay, *by).unwrap_or_else(|| self.exact(i).1.cmp(&other.exact(j).1)))
    }

    /// R3.2.4 ring order: corner by corner, `(x, y)` lexicographic.
    pub(crate) fn cmp(&self, other: &Self) -> Ordering {
        (0..4)
            .map(|i| self.cmp_pt(i, other, i))
            .find(|o| *o != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    }

    /// One coordinate (`x` when `is_x`) of corner `i` against corner `j` of
    /// `other`, exactly.
    fn coord_cmp(&self, i: usize, other: &Self, j: usize, is_x: bool) -> Ordering {
        let pick = |c: &Self, k: usize| if is_x { c.pts[k].0 } else { c.pts[k].1 };
        order(pick(self, i), pick(other, j)).unwrap_or_else(|| {
            let (a, b) = (self.exact(i), other.exact(j));
            if is_x {
                a.0.cmp(&b.0)
            } else {
                a.1.cmp(&b.1)
            }
        })
    }

    /// The corner holding the least `x` (or `y`): the ideal bounding box's
    /// minimum on that axis.
    fn least(&self, is_x: bool) -> usize {
        (1..4).fold(0, |best, i| {
            if self.coord_cmp(i, self, best, is_x) == Ordering::Less {
                i
            } else {
                best
            }
        })
    }

    /// R7.3.2's marquee key on the IDEAL rectangle: the bounding box's
    /// minimum `x`, then minimum `y`, exactly.
    #[cfg(test)]
    pub(crate) fn cmp_bbox_min(&self, other: &Self) -> Ordering {
        self.coord_cmp(self.least_x, other, other.least_x, true)
            .then_with(|| self.coord_cmp(self.least_y, other, other.least_y, false))
    }

    /// [`Self::cmp_bbox_min`] against a binary64 point (a placeholder box's
    /// stored min corner).
    #[cfg(test)]
    pub(crate) fn cmp_bbox_min_point(&self, x: f64, y: f64) -> Ordering {
        let axis = |is_x: bool, v: f64| {
            let i = if is_x { self.least_x } else { self.least_y };
            let e = if is_x { self.pts[i].0 } else { self.pts[i].1 };
            order(e, ex(v)).unwrap_or_else(|| {
                let q = self.exact(i);
                (if is_x { &q.0 } else { &q.1 }).cmp(&Quad::ratio(d(v), one()))
            })
        };
        axis(true, x).then_with(|| axis(false, y))
    }
}

/// An ideal rectangle with everything that does not depend on the pointer
/// prepared once (at index sync): its axis enclosures and its canonical
/// corners. A query then pays only for the pointer-dependent part.
///
/// Everything beyond the shape is built on first use, so indexing a level
/// costs no more than storing its shapes: the interval constants on the
/// first query the inside fast path cannot settle, the corners on the first
/// region tie or marquee.
#[derive(Debug, Clone)]
pub(crate) struct PreparedRect {
    pub(crate) shape: Shape,
    /// [`axis_box`], computed once: most walls are axis-aligned, and for
    /// them the tie key and the marquee need nothing else.
    pub(crate) axis: Option<crate::spatial::Aabb>,
    consts: OnceCell<Box<(LinePrep, RectConsts)>>,
    corners: OnceCell<Box<Corners>>,
}

impl PreparedRect {
    pub(crate) fn of(shape: Shape) -> Self {
        debug_assert!(
            matches!(shape, Shape::Rect { .. }),
            "PreparedRect takes a rectangle"
        );
        Self {
            shape,
            axis: axis_box(&shape),
            consts: OnceCell::new(),
            corners: OnceCell::new(),
        }
    }

    /// R7.1.3's region tie key — the canonical ring, compared exactly. Two
    /// exact axis boxes compare their rings `(min,min) (max,min) (max,max)
    /// (min,max)` as floats; anything else compares ideal corners.
    pub(crate) fn cmp_ring(&self, other: &Self) -> Ordering {
        if let (Some(a), Some(b)) = (self.axis, other.axis) {
            let ring = |r: crate::spatial::Aabb| {
                [
                    (r.min_x, r.min_y),
                    (r.max_x, r.min_y),
                    (r.max_x, r.max_y),
                    (r.min_x, r.max_y),
                ]
            };
            return ring(a)
                .iter()
                .zip(ring(b).iter())
                .map(|(p, q)| p.partial_cmp(q).unwrap_or(Ordering::Equal))
                .find(|o| *o != Ordering::Equal)
                .unwrap_or(Ordering::Equal);
        }
        self.corners().cmp(other.corners())
    }

    fn consts(&self) -> &(LinePrep, RectConsts) {
        self.consts.get_or_init(|| {
            let Shape::Rect { a, b, .. } = self.shape else {
                unreachable!()
            };
            let prep = LinePrep::of(&Dir::between(a, b));
            Box::new((prep, RectConsts::of(&self.shape, &prep)))
        })
    }

    /// The pointer's witness on this rectangle. A pointer the float
    /// filters certify inside (the common click) gets the trivial witness —
    /// distance 0 at `p` — without interval arithmetic.
    pub(crate) fn witness(&self, p: Point) -> Witness {
        if certainly_inside(p, &self.shape) {
            return Witness::inside(p, self.shape);
        }
        let (prep, consts) = self.consts();
        Witness::build(p, self.shape, Some(prep), Some(consts))
    }

    /// The canonical ideal corners (R7.1.3's region tie key).
    pub(crate) fn corners(&self) -> &Corners {
        self.corners
            .get_or_init(|| Box::new(Corners::of(self.shape)))
    }

    /// The ideal wall's bounding-box min corner (R7.3.2's marquee key).
    pub(crate) fn bbox_min(&self) -> BboxMin {
        BboxMin::of(self.shape)
    }
}

/// An ideal WALL rectangle's bounding-box minimum, exactly:
/// `min(aₓ, bₓ) − h·|v_y|/√D` and `min(a_y, b_y) − h·|vₓ|/√D` — two
/// coordinates instead of four rotated corners.
#[derive(Debug, Clone)]
pub(crate) struct BboxMin {
    shape: Shape,
    x: E,
    y: E,
    exact: OnceCell<Box<(Quad, Quad)>>,
}

impl BboxMin {
    pub(crate) fn of(shape: Shape) -> Self {
        if let Some(exact) = axis_box(&shape) {
            return Self {
                shape,
                x: ex(exact.min_x),
                y: ex(exact.min_y),
                exact: OnceCell::new(),
            };
        }
        let Shape::Rect {
            a, b, thickness, ..
        } = shape
        else {
            unreachable!()
        };
        let (vx, vy) = (dif(b.x, a.x), dif(b.y, a.y));
        let rd = root(add(sq(vx), sq(vy)));
        let h = half(thickness);
        let reach = |v: E| div(mul(h, abs(v)), rd);
        Self {
            shape,
            x: sub(ex(a.x.min(b.x)), reach(vy)),
            y: sub(ex(a.y.min(b.y)), reach(vx)),
            exact: OnceCell::new(),
        }
    }

    fn exact(&self) -> &(Quad, Quad) {
        self.exact.get_or_init(|| {
            let Shape::Rect {
                a, b, thickness, ..
            } = self.shape
            else {
                unreachable!()
            };
            let (vx, vy) = (d(b.x).sub(&d(a.x)), d(b.y).sub(&d(a.y)));
            let dd = vx.square().add(&vy.square());
            let h = d(thickness).halve();
            // (min·D − h·|v|·√D) / D
            let q = |m: f64, v: &Dyadic| {
                Quad::new(d(m).mul(&dd), h.mul(&v.abs()).neg(), dd.clone(), dd.clone())
            };
            Box::new((q(a.x.min(b.x), &vy), q(a.y.min(b.y), &vx)))
        })
    }

    /// `(x, y)` when both are exact binary64 values: then float order IS
    /// the exact order.
    pub(crate) fn exact_key(&self) -> Option<(f64, f64)> {
        let one = |e: E| e.filter(|e| e.lo == e.hi).map(|e| e.lo);
        Some((one(self.x)?, one(self.y)?))
    }

    pub(crate) fn cmp(&self, other: &Self) -> Ordering {
        order(self.x, other.x)
            .unwrap_or_else(|| self.exact().0.cmp(&other.exact().0))
            .then_with(|| {
                order(self.y, other.y).unwrap_or_else(|| self.exact().1.cmp(&other.exact().1))
            })
    }

    pub(crate) fn cmp_point(&self, x: f64, y: f64) -> Ordering {
        let one = |e: E, v: f64, q: &dyn Fn() -> Ordering| order(e, ex(v)).unwrap_or_else(q);
        one(self.x, x, &|| {
            self.exact().0.cmp(&Quad::ratio(d(x), d(1.0)))
        })
        .then_with(|| {
            one(self.y, y, &|| {
                self.exact().1.cmp(&Quad::ratio(d(y), d(1.0)))
            })
        })
    }
}

/// The sign of `u₁·v₁ + u₂·v₂` where each factor is a rounded DIFFERENCE of
/// binary64 inputs — a dot or cross product of two input vectors — or
/// `None` when the float evaluation cannot be trusted.
///
/// Shewchuk's orientation filter bound applies unchanged (the expression has
/// the same shape: two products of rounded differences, one sum): the
/// computed value is within `(3 + 16ε)·ε·(|u₁v₁| + |u₂v₂|)` of the exact one,
/// `ε = 2⁻⁵³`. A ZERO bound means every product is exactly zero (guarded
/// against underflow below), so the computed sign is exact — the case of a
/// pointer exactly on a wall's end or axis, decided without big integers.
fn filtered_sign(u1: f64, v1: f64, u2: f64, v2: f64) -> Option<Ordering> {
    const EPS: f64 = f64::EPSILON / 2.0;
    let (p, q) = (u1 * v1, u2 * v2);
    let underflow = |x: f64, a: f64, b: f64| x.abs() < f64::MIN_POSITIVE && a != 0.0 && b != 0.0;
    if underflow(p, u1, v1) || underflow(q, u2, v2) {
        return None;
    }
    let sum = p + q;
    let bound = (3.0 + 16.0 * EPS) * EPS * (p.abs() + q.abs());
    if !sum.is_finite() || !bound.is_finite() {
        return None;
    }
    if bound == 0.0 || sum.abs() > bound {
        return Some(sum.partial_cmp(&0.0).unwrap_or(Ordering::Equal));
    }
    None
}

/// Whether `p` is CERTAINLY inside or on the ideal rectangle, decided by
/// float filters alone; `false` means "not certified", never "outside".
///
/// The common pointer case — a click on a wall — then needs no interval
/// arithmetic at all: the witness is "distance 0 at `p`". Longitudinal
/// position uses exact-when-decided dot signs (`(p − a)·v ≥ 0` and
/// `(p − b)·v ≤ 0`); lateral uses the orientation sign (on the axis) or a
/// float comparison of `|C|` with `h·√D` whose error bound is a generous
/// `16ε` relative margin (each side carries at most a handful of roundings).
pub(crate) fn certainly_inside(p: Point, shape: &Shape) -> bool {
    const EPS: f64 = f64::EPSILON / 2.0;
    let Shape::Rect {
        a,
        b,
        thickness,
        along,
    } = *shape
    else {
        return false;
    };
    let (vx, vy) = (b.x - a.x, b.y - a.y);
    let (wx, wy) = (p.x - a.x, p.y - a.y);
    let dd = vx * vx + vy * vy;
    let normal = |x: f64| x.is_finite() && x.abs() >= 1e-150 && x.abs() <= 1e150;
    if !normal(dd) || !normal(thickness) {
        return false;
    }
    let root_d = dd.sqrt();
    // Longitudinal.
    match along {
        Along::Wall => {
            let start = filtered_sign(wx, vx, wy, vy);
            let end = filtered_sign(p.x - b.x, vx, p.y - b.y, vy);
            if start != Some(Ordering::Greater) && start != Some(Ordering::Equal) {
                return false;
            }
            if end != Some(Ordering::Less) && end != Some(Ordering::Equal) {
                return false;
            }
        }
        Along::Opening { u, width } => {
            // |S − u·D| < k·√D, strictly, with a relative margin.
            let s = wx * vx + wy * vy;
            let m = s - u * dd;
            let err = 16.0 * EPS * ((wx * vx).abs() + (wy * vy).abs() + (u * dd).abs());
            let k = width * 0.5;
            if !(m.is_finite() && normal(k)) || m.abs() + err >= k * root_d * (1.0 - 16.0 * EPS) {
                return false;
            }
        }
    }
    // Lateral: |C| ≤ h·√D, with C = v × w.
    let c = filtered_sign(vx, wy, -vy, wx);
    match c {
        Some(Ordering::Equal) => true,
        None => false,
        Some(_) => {
            let cf = vx * wy - vy * wx;
            let err = (3.0 + 16.0 * EPS) * EPS * ((vx * wy).abs() + (vy * wx).abs());
            cf.abs() + err < thickness * 0.5 * root_d * (1.0 - 16.0 * EPS)
        }
    }
}

/// The ideal WALL rectangle as an exact binary64 box, when the wall is
/// axis-aligned and every bound `coordinate ± h` is exactly representable
/// (proved by a zero two-sum error) — the common case in floor plans. Then
/// intersection, bounds and the marquee key are plain float comparisons.
pub(crate) fn axis_box(shape: &Shape) -> Option<crate::spatial::Aabb> {
    let Shape::Rect {
        a,
        b,
        thickness,
        along: Along::Wall,
    } = *shape
    else {
        return None;
    };
    let h = single(&half(thickness)?)?;
    let exact = |c: f64, d: f64| single(&exact_sum(c, d)?);
    let (x0, x1, y0, y1) = if a.x == b.x && a.y != b.y {
        (exact(a.x, -h)?, exact(a.x, h)?, a.y.min(b.y), a.y.max(b.y))
    } else if a.y == b.y && a.x != b.x {
        (a.x.min(b.x), a.x.max(b.x), exact(a.y, -h)?, exact(a.y, h)?)
    } else {
        return None;
    };
    Some(crate::spatial::Aabb {
        min_x: x0,
        min_y: y0,
        max_x: x1,
        max_y: y1,
    })
}

/// A conservative (outward) binary64 bounding box of an ideal rectangle:
/// every true corner is within `h` (plus, for openings, the half width) of
/// the axis points, coordinate-wise.
pub(crate) fn rect_bounds(shape: &Shape) -> Option<crate::spatial::Aabb> {
    if let Some(exact) = axis_box(shape) {
        return Some(exact);
    }
    let Shape::Rect {
        a,
        b,
        thickness,
        along,
    } = *shape
    else {
        unreachable!()
    };
    let h = half(thickness)?.hi;
    let (lo_x, hi_x, lo_y, hi_y, reach) = match along {
        Along::Wall => (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y), h),
        Along::Opening { u, width } => {
            let cx = add(ex(a.x), mul(ex(u), dif(b.x, a.x)))?;
            let cy = add(ex(a.y), mul(ex(u), dif(b.y, a.y)))?;
            let k = half(width)?.hi;
            (cx.lo, cx.hi, cy.lo, cy.hi, add(ex(h), ex(k))?.hi)
        }
    };
    let out = crate::spatial::Aabb {
        min_x: (lo_x - reach).next_down(),
        min_y: (lo_y - reach).next_down(),
        max_x: (hi_x + reach).next_up(),
        max_y: (hi_y + reach).next_up(),
    };
    [out.min_x, out.min_y, out.max_x, out.max_y]
        .iter()
        .all(|v| v.is_finite())
        .then_some(out)
}

#[cfg(test)]
mod tests;
