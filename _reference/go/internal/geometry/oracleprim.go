package geometry

import (
	"math"
	"math/big"
	"strconv"
)

// Oracle primitive geometry over exact fields. Everything here serves
// the independent exact-decision oracle only; the candidate engines
// under sweep*.go/faces.go never call these decision routines.

type oPt struct{ x, y xnum }

func (p oPt) eq(q oPt) bool {
	return p.x.Sub(q.x).Sign() == 0 && p.y.Sub(q.y).Sign() == 0
}

// oLess orders points lexicographically (x first, then y) within one
// field, matching the canonical tuple order of R3.2.4/R5.1.3.
func oLess(a, b oPt) bool {
	c := a.x.Sub(b.x).Sign()
	if c != 0 {
		return c < 0
	}
	return a.y.Sub(b.y).Sign() < 0
}

type oSeg struct{ a, b oPt }

func (s oSeg) dx() xnum { return s.b.x.Sub(s.a.x) }
func (s oSeg) dy() xnum { return s.b.y.Sub(s.a.y) }

// orient returns the exact sign of (q-p) x (r-p).
func orient(p, q, r oPt) int {
	dqx := q.x.Sub(p.x)
	dqy := q.y.Sub(p.y)
	drx := r.x.Sub(p.x)
	dry := r.y.Sub(p.y)
	return dqx.Mul(dry).Sub(dqy.Mul(drx)).Sign()
}

// xhalf returns v/2 computed entirely within v's own field so that
// quadratic-field values never mix tags with rational constants.
func xhalf(v xnum) xnum {
	switch t := v.(type) {
	case ratNum:
		return xr(new(big.Rat).Quo(t.v, new(big.Rat).SetInt64(2)))
	case quadNum:
		inv := new(big.Rat).Inv(new(big.Rat).SetInt64(2))
		return quadNum{
			p: new(big.Rat).Mul(t.p, inv),
			q: new(big.Rat).Mul(t.q, inv),
			d: t.d,
		}
	}
	panic("geometry: unknown exact field")
}

// xone returns the multiplicative identity of v's own field.
func xone(v xnum) xnum {
	switch t := v.(type) {
	case ratNum:
		return xr(new(big.Rat).SetInt64(1))
	case quadNum:
		return quadNum{p: new(big.Rat).SetInt64(1), q: new(big.Rat).SetInt64(0), d: t.d}
	}
	panic("geometry: unknown exact field")
}

func mid(a, b xnum) xnum { return xhalf(a.Add(b)) }

func segMid(s oSeg) oPt { return oPt{mid(s.a.x, s.b.x), mid(s.a.y, s.b.y)} }

// onSegment reports whether p lies on the closed segment s given that
// collinearity has already been established.
func onSegment(s oSeg, p oPt) bool {
	return !oLess(p, minPt(s)) && !oLess(maxPt(s), p)
}

func minPt(s oSeg) oPt {
	if oLess(s.a, s.b) {
		return s.a
	}
	return s.b
}

func maxPt(s oSeg) oPt {
	if oLess(s.a, s.b) {
		return s.b
	}
	return s.a
}

// colinearBetween reports whether p is collinear with s and lies on the
// closed segment.
func colinearBetween(s oSeg, p oPt) bool {
	return orient(s.a, s.b, p) == 0 && onSegment(s, p)
}

// segIntersectParam computes the parameter t of the intersection of the
// line a+t*(b-a) with segment cd, or reports none. Requires nonparallel
// segments.
func segIntersectParam(a, b, c, d oPt) (t xnum, ok bool) {
	d1x, d1y := b.x.Sub(a.x), b.y.Sub(a.y)
	d2x, d2y := d.x.Sub(c.x), d.y.Sub(c.y)
	denom := d1x.Mul(d2y).Sub(d1y.Mul(d2x))
	if denom.Sign() == 0 {
		return nil, false
	}
	numT := c.x.Sub(a.x).Mul(d2y).Sub(c.y.Sub(a.y).Mul(d2x))
	return numT.Div(denom), true
}

// angHalf classifies a direction vector into the two angular halves
// used for exact CCW angular ordering: upper/lower semicircle split at
// the negative-x direction.
func angHalf(dx, dy xnum) int {
	sy := dy.Sign()
	if sy > 0 {
		return 0
	}
	if sy < 0 {
		return 1
	}
	if dx.Sign() >= 0 {
		return 0
	}
	return 1
}

// angCCW compares direction v before w in CCW order starting at the
// positive-x direction.
func angCCW(v, w oPt) bool {
	hv, hw := angHalf(v.x, v.y), angHalf(w.x, w.y)
	if hv != hw {
		return hv < hw
	}
	cross := v.x.Mul(w.y).Sub(v.y.Mul(w.x)).Sign()
	if cross != 0 {
		return cross > 0
	}
	return false // same direction; order irrelevant, keep stable
}

// shoelaceSign returns the exact sign of the signed area doubled
// (shoelace sum) of a closed ring without a repeated closing vertex.
func shoelaceSign(ring []oPt) int {
	if len(ring) < 3 {
		return 0
	}
	sum := ring[0].x.Sub(ring[0].x) // exact field zero
	for i := 0; i < len(ring); i++ {
		a := ring[i]
		b := ring[(i+1)%len(ring)]
		sum = sum.Add(a.x.Mul(b.y)).Sub(b.x.Mul(a.y))
	}
	return sum.Sign()
}

// ptKey builds a deterministic string key for an exact point within
// its field. Values from different quadratic fields never collide.
func ptKey(p oPt) string {
	// Field selection is by CONCRETE TYPE: quadNum.Rat succeeds even
	// for zero-q values, so Rat-success must never decide the tag.
	switch px := p.x.(type) {
	case ratNum:
		switch py := p.y.(type) {
		case ratNum:
			return px.v.RatString() + "," + py.v.RatString()
		default:
			mixedFieldMu.Lock()
			mixedFieldEvents++
			mixedFieldMu.Unlock()
			panic("geometry: mixed-field point")
		}
	case quadNum:
		qy, ok := p.y.(quadNum)
		if !ok || qy.d != px.d {
			mixedFieldMu.Lock()
			mixedFieldEvents++
			mixedFieldMu.Unlock()
			panic("geometry: mixed-field point")
		}
		return "q" + strconv.FormatInt(px.d, 10) +
			":" + px.p.RatString() + "+" + px.q.RatString() +
			"," + qy.p.RatString() + "+" + qy.q.RatString()
	}
	mixedFieldMu.Lock()
	mixedFieldEvents++
	mixedFieldMu.Unlock()
	panic("geometry: unknown exact field in ptKey")
}

// xsortPts sorts points with the canonical lexicographic order.
func xsortPts(pts []oPt) {
	for i := 1; i < len(pts); i++ {
		for j := i; j > 0 && oLess(pts[j], pts[j-1]); j-- {
			pts[j], pts[j-1] = pts[j-1], pts[j]
		}
	}
}

// ptInRingsWinding is the AC2 semantic containment probe: nonzero
// winding number over orientation-carrying rings (outer CCW, hole CW).
// Overlapping same-orientation outers accumulate winding and stay
// inside; CW holes cancel their outer. Used by all oracle membership
// decisions for union/intersection/difference and face classification.
func ptInRingsWinding(m oPt, segs []oSeg) bool {
	w := 0
	for _, s := range segs {
		aAbove := s.a.y.Sub(m.y).Sign() > 0
		bAbove := s.b.y.Sub(m.y).Sign() > 0
		if aAbove == bAbove {
			continue
		}
		t := m.y.Sub(s.a.y).Div(s.dy())
		xi := s.a.x.Add(t.Mul(s.dx()))
		if xi.Sub(m.x).Sign() > 0 {
			if bAbove && !aAbove {
				w++ // upward crossing
			} else {
				w-- // downward crossing
			}
		}
	}
	return w != 0
}

// ptInRingsEvenOdd is an exact even-odd containment test. It is NOT
// the semantic oracle rule (AC2 fixes nonzero-winding); it is retained
// only as a separately labeled comparison probe in tests.
func ptInRingsEvenOdd(m oPt, segs []oSeg) bool {
	count := 0
	for _, s := range segs {
		ay, by := s.a.y, s.b.y
		c1 := ay.Sub(m.y).Sign() > 0
		c2 := by.Sub(m.y).Sign() > 0
		if c1 != c2 {
			// Compute exact x where the edge crosses the horizontal
			// line through m.
			t := m.y.Sub(ay).Div(by.Sub(ay))
			xi := s.a.x.Add(t.Mul(s.dx()))
			if xi.Sub(m.x).Sign() > 0 {
				count++
			}
		}
	}
	return count%2 == 1
}

// Coordinate bounds proposed by NUMERICAL-MODEL section 3 (B = 10^9 mm;
// final ratification belongs to tracker 0030). The spike validates
// against this bound to exercise out-of-range rejection.
const boundBmm = 1e9

// ValidateCoord enforces finiteness and the proposed magnitude bound B
// for stored binary64 coordinates. Rejection codes are spike-local
// sentinels, not ratified diagnostic tokens.
func ValidateCoord(f float64) error {
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return &SpikeError{"nonfinite-coordinate"}
	}
	if math.Abs(f) > boundBmm {
		return &SpikeError{"coordinate-out-of-range"}
	}
	return nil
}

// ValidateSimpleRing checks a float-domain ring for finiteness, range,
// nonzero area, and self-intersection (O(n^2), adequate at spike
// scale). Adjacent edges touching at one shared vertex is allowed;
// any other vertex/edge incidence or proper crossing is rejected.
func ValidateSimpleRing(pts [][2]float64) error {
	n := len(pts)
	if n < 3 {
		return &SpikeError{"degenerate-ring"}
	}
	for _, p := range pts {
		if err := ValidateCoord(p[0]); err != nil {
			return err
		}
		if err := ValidateCoord(p[1]); err != nil {
			return err
		}
	}
	for i := 0; i < n; i++ {
		a := pts[i]
		b := pts[(i+1)%n]
		if a[0] == b[0] && a[1] == b[1] {
			return &SpikeError{"zero-length-edge"}
		}
	}
	// Exact dyadic shoelace: near-B coordinates make float
	// accumulation catastrophically cancel, so the area gate never
	// trusts binary64 sums.
	exactArea := new(big.Rat)
	for i := 0; i < n; i++ {
		a := pts[i]
		b := pts[(i+1)%n]
		rax := ratOf(a[0])
		ray := ratOf(a[1])
		rbx := ratOf(b[0])
		rby := ratOf(b[1])
		term := new(big.Rat).Sub(
			new(big.Rat).Mul(rax, rby),
			new(big.Rat).Mul(rbx, ray))
		exactArea.Add(exactArea, term)
	}
	if exactArea.Sign() == 0 {
		return &SpikeError{"zero-area-ring"}
	}
	cross := func(o, a, b [2]float64) float64 {
		return (a[0]-o[0])*(b[1]-o[1]) - (a[1]-o[1])*(b[0]-o[0])
	}
	onSeg := func(p, a, b [2]float64) bool {
		return math.Min(a[0], b[0]) <= p[0] && p[0] <= math.Max(a[0], b[0]) &&
			math.Min(a[1], b[1]) <= p[1] && p[1] <= math.Max(a[1], b[1])
	}
	segsHit := 0
	for i := 0; i < n; i++ {
		for j := i + 1; j < n; j++ {
			p1, p2 := pts[i], pts[(i+1)%n]
			p3, p4 := pts[j], pts[(j+1)%n]
			d1 := cross(p3, p4, p2)
			d2 := cross(p3, p4, p1)
			d3 := cross(p1, p2, p4)
			d4 := cross(p1, p2, p3)
			if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) &&
				((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
				return &SpikeError{"self-intersecting-ring"}
			}
			// Touching incidences between non-adjacent edges also
			// violate simplicity for our purposes.
			adjacent := i == 0 && j == n-1 || j == i+1
			touch := func(p, a, b [2]float64) bool {
				return cross(a, b, p) == 0 && onSeg(p, a, b)
			}
			hits := 0
			if touch(p1, p3, p4) {
				hits++
			}
			if touch(p2, p3, p4) {
				hits++
			}
			if touch(p3, p1, p2) {
				hits++
			}
			if touch(p4, p1, p2) {
				hits++
			}
			if hits > 0 {
				segsHit++
				if !adjacent && segsHit > 0 {
					return &SpikeError{"ring-touching-incidence"}
				}
			}
		}
	}
	return nil
}
