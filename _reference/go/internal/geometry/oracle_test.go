package geometry

import (
	"math"
	"math/big"
	"testing"
)

// ---- helpers ---------------------------------------------------------

func ipt(x, y int64) oPt {
	return oPt{x: XRatio(x, 1), y: XRatio(y, 1)}
}

// rectRing returns the CCW ring of the axis-aligned rectangle.
func rectRing(x0, y0, x1, y1 int64) oRing {
	return oRing{ipt(x0, y0), ipt(x1, y0), ipt(x1, y1), ipt(x0, y1)}
}

func ps(rings ...oRing) polySet {
	return polySet{outer: rings, holes: make([][]oRing, len(rings))}
}

func segsOfRings(rs ...oRing) []oSeg {
	var out []oSeg
	for _, r := range rs {
		out = append(out, ringSegs(r)...)
	}
	return out
}

func mustUnion(t *testing.T, a, b polySet) polySet {
	t.Helper()
	out, err := BooleanUnion(a, b)
	if err != nil {
		t.Fatalf("union: %v", err)
	}
	return out
}

// ---- exact layer -----------------------------------------------------

func TestXFloatRejectsNonFinite(t *testing.T) {
	for _, f := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, err := XFloat(f); err == nil {
			t.Fatalf("XFloat(%v) accepted non-finite input", f)
		}
	}
	r, err := XFloat(0.1)
	if err != nil {
		t.Fatalf("XFloat(0.1): %v", err)
	}
	// 0.1 as binary64 is exactly 3602879701896397 / 2^55.
	got, _ := r.Rat()
	want := new(big.Rat).SetFrac64(3602879701896397, 1<<55)
	if got.Cmp(want) != 0 {
		t.Fatalf("0.1 conversion: got %s want %s", got.RatString(), want.RatString())
	}
}

func TestQuadFieldArithmetic(t *testing.T) {
	d := int64(2)
	a := XQuad(big.NewRat(1, 2), big.NewRat(1, 3), d)  // 1/2 + (1/3)sqrt2 > 0
	b := XQuad(big.NewRat(-1, 2), big.NewRat(1, 3), d) // -1/2 + (1/3)sqrt2 < 0 (~ -0.0286)
	if got := b.Sign(); got != -1 {
		t.Fatalf("sign(-1/2 + sqrt2/3) = %d, want -1", got)
	}
	pos := XQuad(big.NewRat(-1, 2), big.NewRat(1, 2), d) // -1/2 + sqrt2/2 > 0
	if pos.Sign() != 1 {
		t.Fatalf("sign(-1/2 + sqrt2/2) = %d, want 1", pos.Sign())
	}
	sum := a.Add(b) // sqrt2 * 2/3 > 0
	if sum.Sign() != 1 {
		t.Fatalf("sum sign = %d", sum.Sign())
	}
	prod := a.Mul(a) // 1/4 + 2/9 + (1/3)sqrt2 > 0
	if prod.Sign() != 1 {
		t.Fatalf("(a)^2 sign = %d", prod.Sign())
	}
	z := XQuad(big.NewRat(3, 1), big.NewRat(-3, 2), d) // 3 - 1.5*sqrt2 != 0
	if z.Sign() <= 0 {
		t.Fatalf("3 - 1.5sqrt2 should be positive")
	}
	// Field-tag mismatch must panic deterministically.
	func() {
		defer func() {
			if recover() == nil {
				t.Fatal("mixed-field add did not panic")
			}
		}()
		_ = a.Add(XRatio(1, 1))
	}()
	// Midpoint stays in the quad field.
	qm := mid(XQuad(big.NewRat(0, 1), big.NewRat(1, 1), d), XQuad(big.NewRat(1, 1), big.NewRat(1, 1), d))
	q, ok := qm.(quadNum)
	if !ok || q.d != d || q.p.Cmp(big.NewRat(1, 2)) != 0 {
		t.Fatalf("quad midpoint wrong: %v", qm)
	}
}

// ---- oracle booleans --------------------------------------------------

func TestOracleUnionIdenticalPolygons(t *testing.T) {
	a := ps(rectRing(0, 0, 2, 2))
	out := mustUnion(t, a, ps(rectRing(0, 0, 2, 2)))
	if len(out.outer) != 1 || len(out.holes[0]) != 0 {
		t.Fatalf("identical union shape wrong: %+v", out)
	}
	if c := compareRings(out.outer[0], rectRing(0, 0, 2, 2)); c != 0 {
		t.Fatalf("identical union ring mismatch")
	}
}

func TestOracleUnionAdjacentSquaresCancelSeam(t *testing.T) {
	a := ps(rectRing(0, 0, 2, 2))
	b := ps(rectRing(2, 0, 4, 2))
	out := mustUnion(t, a, b)
	if len(out.outer) != 1 || len(out.holes[0]) != 0 {
		t.Fatalf("adjacent union components wrong")
	}
	want := rectRing(0, 0, 4, 2)
	if c := compareRings(out.outer[0], want); c != 0 {
		t.Fatalf("seam cancellation produced %+v want %+v", out.outer[0], want)
	}
	// Positive-area intersection is empty; the retained seam yields no
	// zero-area residue.
	inter, err := BooleanIntersection(ps(rectRing(0, 0, 2, 2)), ps(rectRing(2, 0, 4, 2)))
	if err != nil {
		t.Fatalf("intersection: %v", err)
	}
	if len(inter.outer) != 0 {
		t.Fatalf("touching squares must have empty positive-area intersection")
	}
	diff, err := BooleanDifference(ps(rectRing(0, 0, 2, 2)), ps(rectRing(2, 0, 4, 2)))
	if err != nil {
		t.Fatalf("difference: %v", err)
	}
	if c := compareRings(diff.outer[0], rectRing(0, 0, 2, 2)); c != 0 || len(diff.outer) != 1 {
		t.Fatalf("difference of disjoint-interior operands must preserve A")
	}
}

func TestOraclePartialCollinearOverlap(t *testing.T) {
	// Two squares sharing part of an edge (collinear overlap fragment).
	a := ps(rectRing(0, 0, 2, 2))
	b := ps(rectRing(2, -1, 4, 1))
	out := mustUnion(t, a, b)
	area := absDoubledArea(out.outer[0])
	// Areas 4 + 4, doubled union area = 16.
	if area.Sign() <= 0 || area.Sub(XRatio(16, 1)).Sign() != 0 {
		v, _ := area.Rat()
		t.Fatalf("partial-overlap union doubled area = %s want 16", v.RatString())
	}
}

func TestOracleOverlapUnionIntersectionDifference(t *testing.T) {
	a := ps(rectRing(0, 0, 2, 2))
	b := ps(rectRing(1, 1, 3, 3))
	u := mustUnion(t, a, b)
	if got := absDoubledArea(u.outer[0]).Sub(XRatio(14, 1)).Sign(); got != 0 || len(u.outer) != 1 {
		t.Fatalf("union doubled area wrong")
	}
	i, err := BooleanIntersection(a, b)
	if err != nil {
		t.Fatal(err)
	}
	if got := absDoubledArea(i.outer[0]).Sub(XRatio(2, 1)).Sign(); got != 0 {
		t.Fatalf("intersection doubled area wrong")
	}
	d, err := BooleanDifference(a, b)
	if err != nil {
		t.Fatal(err)
	}
	if got := absDoubledArea(d.outer[0]).Sub(XRatio(6, 1)).Sign(); got != 0 || len(d.outer) != 1 {
		t.Fatalf("difference doubled area wrong")
	}
	// Difference removes only the OPEN interior of B: B's boundary
	// fragments remain, so a fully enclosed hole keeps its ring.
	h, err := BooleanDifference(ps(rectRing(0, 0, 10, 10)), ps(rectRing(2, 2, 8, 8)))
	if err != nil {
		t.Fatal(err)
	}
	if len(h.outer) != 1 || len(h.holes[0]) != 1 {
		t.Fatalf("enclosed subtraction must leave a hole ring: %+v", h)
	}
}

func TestOracleCanonicalizationInvariance(t *testing.T) {
	// Same operand geometry as base, varying only ring start,
	// direction, and operand order: canonical output must match.
	base := mustUnion(t, ps(rectRing(0, 0, 2, 2)), ps(rectRing(1, 1, 3, 3)))
	a := rectRing(0, 0, 2, 2)
	aRot := append(oRing{a[2]}, append(append(oRing{}, a[3:]...), a[:2]...)...)
	b := rectRing(1, 1, 3, 3)
	bRev := oRing{b[0], b[3], b[2], b[1]} // CW input
	alt := mustUnion(t, ps(bRev), ps(aRot))
	if len(base.outer) != 1 || len(alt.outer) != 1 {
		t.Fatal("component counts differ")
	}
	if c := compareRings(base.outer[0], alt.outer[0]); c != 0 {
		t.Fatal("canonical output depends on input rotation/direction/order")
	}
	// Least-vertex rotation is applied.
	rot := canonRotate(base.outer[0])
	if !rot[0].eq(ipt(0, 0)) {
		t.Fatal("output ring does not start at lexicographic least vertex")
	}
}

// ---- oracle faces ------------------------------------------------------

// roomBands builds four corner-butted wall bands enclosing the open
// square (x0+t, y0+t)-(x1-t, y1-t) with no overlapping rings.
func roomBands(x0, y0, x1, y1, t int64) []oRing {
	return []oRing{
		rectRing(x0, y0, x1, y0+t),
		rectRing(x0, y1-t, x1, y1),
		rectRing(x0, y0+t, x0+t, y1-t),
		rectRing(x1-t, y0+t, x1, y1-t),
	}
}

func TestOracleFacesEmpty(t *testing.T) {
	arr, err := OracleFaces(nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 0 || arr.Exterior.Outer != nil || len(arr.Exterior.Holes) != 0 {
		t.Fatalf("empty level must have exactly the bare exterior (R5.1.6)")
	}
}

func TestOracleFacesSolidObstacleNoRooms(t *testing.T) {
	arr, err := OracleFaces(segsOfRings(rectRing(0, 0, 4, 4)))
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 0 {
		t.Fatalf("solid obstacle must create no bounded face")
	}
	if len(arr.Exterior.Holes) != 1 {
		t.Fatalf("exterior must carry one CW loop around the obstacle")
	}
}

func TestOracleFacesSingleRoom(t *testing.T) {
	arr, err := OracleFaces(segsOfRings(roomBands(0, 0, 12, 12, 2)...))
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 1 {
		t.Fatalf("corridor must enclose exactly one room, got %d", len(arr.Faces))
	}
	want := rectRing(2, 2, 10, 10)
	if c := compareRings(arr.Faces[0].Outer, want); c != 0 || len(arr.Faces[0].Holes) != 0 {
		t.Fatalf("room ring mismatch: %+v", arr.Faces[0])
	}
	if arr.Faces[0].Parent != -1 {
		t.Fatalf("room must have no parent")
	}
}

func TestOracleFacesDisjointRooms(t *testing.T) {
	segs := segsOfRings(roomBands(0, 0, 12, 12, 2)...)
	segs = append(segs, segsOfRings(roomBands(40, 0, 52, 12, 2)...)...)
	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 {
		t.Fatalf("two disjoint rooms expected, got %d", len(arr.Faces))
	}
	for i, f := range arr.Faces {
		if f.Parent != -1 || len(f.Holes) != 0 {
			t.Fatalf("room %d parent/hole structure wrong: %+v", i, f)
		}
	}
	if len(arr.Exterior.Holes) != 2 {
		t.Fatalf("exterior must carry two loops")
	}
}

func TestOracleFacesPointPinch(t *testing.T) {
	// F35-equivalent clean-room reconstruction: inside an enclosure,
	// obstacle S1 spans from the bottom wall up to P=(18,18) and S2
	// spans from P to the top wall. The free lobes NW/above-left and
	// below-right of P meet only at the occupied point P and must
	// split into two faces (R5.1.2).
	segs := segsOfRings(roomBands(0, 0, 30, 30, 2)...)
	segs = append(segs,
		segsOfRings(rectRing(10, 2, 18, 18),
			rectRing(18, 18, 26, 28))...)
	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 {
		t.Fatalf("pinch must split into 2 faces, got %d", len(arr.Faces))
	}
	for i, f := range arr.Faces {
		if f.Parent != -1 {
			t.Fatalf("pinch face %d must not nest", i)
		}
		if len(f.Outer) < 3 {
			t.Fatalf("pinch face %d missing simple outer ring", i)
		}
	}
}

func TestOracleFacesCourtyardIslandNesting(t *testing.T) {
	segs := segsOfRings(roomBands(0, 0, 30, 30, 2)...)
	// Hollow island: corner-butted bands enclosing (12,12)-(18,16).
	island := []oRing{
		rectRing(10, 10, 20, 12),
		rectRing(10, 16, 20, 18),
		rectRing(10, 12, 12, 16),
		rectRing(18, 12, 20, 16),
	}
	segs = append(segs, segsOfRings(island...)...)
	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 {
		t.Fatalf("courtyard+island expected 2 faces, got %d", len(arr.Faces))
	}
	courtyard := arr.Faces[0]
	if c := compareRings(courtyard.Outer, rectRing(2, 2, 28, 28)); c != 0 {
		t.Fatalf("courtyard outer ring mismatch")
	}
	if len(courtyard.Holes) != 1 {
		t.Fatalf("courtyard must carry the island loop as its single hole")
	}
	// Canonical hole rings are CW (R3.2.4).
	islandHole := oRing{ipt(10, 10), ipt(10, 18), ipt(20, 18), ipt(20, 10)}
	if c := compareRings(courtyard.Holes[0], islandHole); c != 0 {
		t.Fatalf("island hole ring mismatch")
	}
	islandFace := arr.Faces[1]
	if c := compareRings(islandFace.Outer, rectRing(12, 12, 18, 16)); c != 0 {
		t.Fatalf("island room ring mismatch")
	}
	if islandFace.Parent != 0 {
		t.Fatalf("island room must nest inside the courtyard")
	}
	if len(arr.Exterior.Holes) != 1 {
		t.Fatalf("exterior carries the enclosure loop")
	}
}

// ---- mutation probes ---------------------------------------------------

func TestOracleMutationProbes(t *testing.T) {
	// Chirality sensitivity: flipping a ring must flip its shoelace
	// sign, so corrupted winding cannot silently pass.
	r := rectRing(0, 0, 4, 4)
	if shoelaceSign(r) <= 0 || shoelaceSign(oRing{r[0], r[3], r[2], r[1]}) >= 0 {
		t.Fatal("chirality probe insensitive")
	}
	// Canonical comparison must detect a mutated vertex.
	mut := rectRing(0, 0, 4, 4)
	mut[1] = ipt(4, 5)
	if compareRings(rectRing(0, 0, 4, 4), mut) == 0 {
		t.Fatal("comparison failed to detect mutated vertex")
	}
	// Face-count assertions must fail under corruption.
	arr, err := OracleFaces(segsOfRings(roomBands(0, 0, 12, 12, 2)...))
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) == 2 {
		t.Fatal("corrupted expectation unexpectedly satisfied")
	}
}

// ---- AC2 fill-rule and field hardening --------------------------------

func TestWindingOverlapProbeOccupied(t *testing.T) {
	// Overlapping same-orientation outers: even-odd would cancel the
	// two crossings; nonzero winding must classify (3/2,3/2) occupied.
	segs := segsOfRings(rectRing(0, 0, 2, 2), rectRing(1, 1, 3, 3))
	p := oPt{x: XRatio(3, 2), y: XRatio(3, 2)}
	if !ptInRingsWinding(p, segs) {
		t.Fatal("winding rule must classify overlap interior as occupied")
	}
	if ptInRingsEvenOdd(p, segs) {
		t.Fatal("comparison probe: even-odd cancels here (labeled contrast)")
	}
	// Hole reversal preserves roles: an outer with a CW hole keeps its
	// interior occupied and hole free under winding; reversing every
	// input ring flips classification, proving role sensitivity.
	withHole := ps(rectRing(0, 0, 10, 10))
	withHole.holes[0] = []oRing{oRing{ipt(2, 2), ipt(2, 8), ipt(8, 8), ipt(8, 2)}}
	segs = setSegs(withHole)
	if !ptInRingsWinding(ipt(1, 1), segs) || ptInRingsWinding(ipt(5, 5), segs) {
		t.Fatal("hole must be free while annulus stays occupied")
	}
	// Reversing ONLY the hole ring (making it CCW like the outer)
	// removes its subtraction role: the former hole becomes occupied,
	// proving winding respects ring roles rather than mere containment.
	flippedHole := ps(rectRing(0, 0, 10, 10))
	flippedHole.holes[0] = []oRing{rectRing(2, 2, 8, 8)} // same geometry, CCW
	fsegs := setSegs(flippedHole)
	if !ptInRingsWinding(ipt(5, 5), fsegs) {
		t.Fatal("CCW inner ring must stop acting as a hole under winding")
	}
	// Reversing every ring flips the winding SIGN everywhere but keeps
	// zero/nonzero structure identical (role-relative, not absolute).
	rev := ps(reverseRing(rectRing(0, 0, 10, 10)))
	rev.outer[0] = reverseRing(rectRing(0, 0, 10, 10))
	rev.holes[0] = []oRing{reverseRing(withHole.holes[0][0])}
	rsegs := setSegs(rev)
	if !ptInRingsWinding(ipt(1, 1), rsegs) || ptInRingsWinding(ipt(5, 5), rsegs) {
		t.Fatal("uniform reversal preserves the free/occupied structure")
	}
}

func TestQuadConstructorsDoNotAliasOperands(t *testing.T) {
	p := big.NewRat(1, 2)
	q := big.NewRat(1, 4)
	v := XQuad(p, q, 2)
	p.SetInt64(99)
	q.SetInt64(99)
	qv := v.(quadNum)
	if qv.p.Cmp(big.NewRat(1, 2)) != 0 || qv.q.Cmp(big.NewRat(1, 4)) != 0 {
		t.Fatal("XQuad aliases caller-owned operands")
	}
}

func TestRationalLiftIntoSurdField(t *testing.T) {
	// Mixed rational + sqrt(2) construction: P=(10,10) offset by
	// (-sqrt2/2, sqrt2/2). Rational terms lift via XLift so all
	// operands share the field tag.
	d := int64(2)
	x := XLift(big.NewRat(10, 1), d).
		Sub(XQuad(new(big.Rat).SetInt64(0), big.NewRat(1, 2), d))
	y := XLift(big.NewRat(10, 1), d).
		Add(XQuad(new(big.Rat).SetInt64(0), big.NewRat(1, 2), d))
	if x.Sign() <= 0 || y.Sign() <= 0 {
		t.Fatal("45-degree offset point must stay positive")
	}
	qx := x.(quadNum)
	if qx.d != d || qx.p.Cmp(big.NewRat(10, 1)) != 0 || qx.q.Cmp(big.NewRat(-1, 2)) != 0 {
		t.Fatalf("lifted coordinates wrong: %+v", qx)
	}
}

func TestDiagonalSlabCrossing(t *testing.T) {
	// A single diagonal wall band crossing several slabs: free space
	// above and below must remain exactly two exterior-adjacent
	// components with no artificial splits, and the bounded-face count
	// is zero (no enclosure).
	diag := oRing{ipt(-10, -10), ipt(10, 10)}
	cross := rectRing(-10, -2, 10, 2) // forces an interior cut line at x=0
	arr, err := OracleFaces(segsOfRings(diag, cross))
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 0 {
		t.Fatalf("crossing segments enclose nothing bounded here: %d faces", len(arr.Faces))
	}
	// The diagonal genuinely crosses multiple slabs: sanity-check that
	// the arrangement produced at least three distinct cut lines.
	xs := distinctXs(splitAll(segsOfRings(diag, cross)))
	if len(xs) < 4 {
		t.Fatalf("expected >=4 cut lines from a crossing pair, got %d", len(xs))
	}
}

func TestCoincidentEdgeSeams(t *testing.T) {
	// Two bands sharing a full collinear edge piece (butt joint): the
	// shared seam is interior to the union and must vanish without
	// probe failures or stray faces.
	a := rectRing(0, 0, 4, 2)
	b := rectRing(0, 2, 4, 6)
	out := mustUnion(t, ps(a), ps(b))
	if len(out.outer) != 1 || len(out.holes[0]) != 0 {
		t.Fatalf("butt-joint union must be one ring")
	}
	want := rectRing(0, 0, 4, 6)
	if c := compareRings(out.outer[0], want); c != 0 {
		t.Fatal("seam not absorbed")
	}
	arr, err := OracleFaces(segsOfRings(a, b))
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 0 {
		t.Fatalf("stacked bands enclose nothing: %d faces", len(arr.Faces))
	}
}

func TestIntervalIntersections(t *testing.T) {
	r := func(lo, hi int64) xiv {
		return xiv{lo: XRatio(lo, 1), hi: XRatio(hi, 1)}
	}
	whole := xiv{loInf: true, hiInf: true}
	cases := []struct {
		name string
		a, b xiv
		want bool
	}{
		{"disjoint", r(0, 1), r(2, 3), false},
		{"touch", r(0, 1), r(1, 2), false}, // open intervals: no length
		{"overlap", r(0, 2), r(1, 3), true},
		{"contained", r(0, 10), r(2, 3), true},
		{"negInf-finite", xiv{loInf: true, hi: XRatio(0, 1)}, r(5, 6), false},
		{"negInf-overlap", xiv{loInf: true, hi: XRatio(7, 1)}, r(5, 6), true},
		{"finite-posInf", r(0, 1), xiv{lo: XRatio(5, 1), hiInf: true}, false},
		{"overlap-posInf", r(0, 7), xiv{lo: XRatio(5, 1), hiInf: true}, true},
		{"whole-whole", whole, whole, true},
		{"whole-disjointFinitePairIsWholeOnly", whole, r(0, 1), true},
	}
	for _, tc := range cases {
		got := ivIntersectPositive(tc.a.lo, tc.a.hi, tc.a.loInf, tc.a.hiInf, tc.b)
		if got != tc.want {
			t.Errorf("%s: got %v want %v", tc.name, got, tc.want)
		}
	}
}

func TestFarSeedExteriorIdentity(t *testing.T) {
	// The explicit far-field seed must select a class distinct from
	// bounded faces and consistent across both outer slabs.
	segs := segsOfRings(roomBands(0, 0, 12, 12, 2)...)
	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Exterior.Holes) != 1 {
		t.Fatalf("exterior must carry the building loop once")
	}
	for _, f := range arr.Faces {
		if compareRings(f.Outer, arr.Exterior.Holes[0]) == 0 {
			t.Fatal("bounded face reuses the exterior loop orientation set")
		}
	}
}

// ---- normative F35 fixture ---------------------------------------------

// wallBandAxis returns the occupied band of an axis-aligned wall segment
// with thickness t centered on its axis, extending the full along-axis
// extent between the endpoints.
func wallBandAxis(ax, ay, bx, by, t int64) oRing {
	h := t / 2
	switch {
	case ay == by && ax < bx:
		return rectRing(ax, ay-h, bx, ay+h)
	case ay == by && ax > bx:
		return rectRing(bx, by-h, ax, by+h)
	case ax == bx && ay < by:
		return rectRing(ax-h, ay, ax+h, by)
	default:
		return rectRing(bx-h, by, ax+h, ay)
	}
}

func TestOracleFacesNormativeF35PointPinch(t *testing.T) {
	t200 := int64(200)
	loopA := []oRing{
		wallBandAxis(0, 0, 2000, 0, t200),
		wallBandAxis(2000, 0, 2000, 2000, t200),
		wallBandAxis(2000, 2000, 0, 2000, t200),
		wallBandAxis(0, 2000, 0, 0, t200),
	}
	loopB := []oRing{
		wallBandAxis(2000, 2000, 4000, 2000, t200),
		wallBandAxis(4000, 2000, 4000, 4000, t200),
		wallBandAxis(4000, 4000, 2000, 4000, t200),
		wallBandAxis(2000, 4000, 2000, 2000, t200),
	}
	var segs []oSeg
	segs = append(segs, segsOfRings(loopA...)...)
	segs = append(segs, segsOfRings(loopB...)...)

	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 {
		t.Fatalf("F35 must yield exactly two lobes, got %d", len(arr.Faces))
	}
	wantA := rectRing(100, 100, 1900, 1900)
	wantB := rectRing(2100, 2100, 3900, 3900)
	got := [][2]int{}
	for i, f := range arr.Faces {
		if len(f.Holes) != 0 || f.Parent != -1 {
			t.Fatalf("lobe %d structure wrong", i)
		}
		switch compareRings(f.Outer, wantA) {
		case 0:
			got = append(got, [2]int{i, 0})
		default:
			if compareRings(f.Outer, wantB) == 0 {
				got = append(got, [2]int{i, 1})
			} else {
				t.Fatalf("unexpected lobe ring")
			}
		}
	}
	if len(got) != 2 {
		t.Fatal("both normative lobes required")
	}
	// Exact areas: doubled shoelace of each 1800x1800 lobe.
	for i := range arr.Faces {
		if absDoubledArea(arr.Faces[i].Outer).Sub(XRatio(2*1800*1800, 1)).Sign() != 0 {
			t.Fatalf("lobe %d area mismatch", i)
		}
	}
}

func reverseRing(r oRing) oRing {
	out := make(oRing, len(r))
	for i := range r {
		out[len(r)-1-i] = r[i]
	}
	return out
}

// ---- validation policy, empty operands, multi-component ordering ------

func TestValidationPolicy(t *testing.T) {
	if err := ValidateCoord(math.NaN()); err == nil {
		t.Fatal("NaN must reject")
	}
	if err := ValidateCoord(2e9); err == nil {
		t.Fatal("out-of-range coordinate must reject")
	}
	if err := ValidateSimpleRing([][2]float64{{0, 0}, {1, 1}}); err == nil {
		t.Fatal("degenerate ring must reject")
	}
	if err := ValidateSimpleRing([][2]float64{{0, 0}, {4, 0}, {4, 4}, {0, 4}}); err != nil {
		t.Fatalf("valid ring rejected: %v", err)
	}
	bowtie := [][2]float64{{0, 0}, {4, 4}, {4, 0}, {0, 4}}
	if err := ValidateSimpleRing(bowtie); err == nil {
		t.Fatal("self-intersecting ring must reject")
	}
}

func TestEmptyOperandIdentities(t *testing.T) {
	empty := polySet{holes: [][]oRing{}}
	a := ps(rectRing(0, 0, 2, 2))
	u, err := BooleanUnion(a, empty)
	if err != nil || len(u.outer) != 1 || compareRings(u.outer[0], a.outer[0]) != 0 {
		t.Fatal("union identity A∪∅ failed")
	}
	i, err := BooleanIntersection(empty, a)
	if err != nil || len(i.outer) != 0 {
		t.Fatal("intersection with empty must be empty")
	}
	d1, err := BooleanDifference(a, empty)
	if err != nil || len(d1.outer) != 1 || compareRings(d1.outer[0], a.outer[0]) != 0 {
		t.Fatal("difference identity A−∅ failed")
	}
	d2, err := BooleanDifference(empty, a)
	if err != nil || len(d2.outer) != 0 {
		t.Fatal("difference ∅−A must be empty")
	}
}

func TestTwoHoledComponentsReversedOrder(t *testing.T) {
	cA := rectRing(0, 0, 10, 10)
	hA := oRing{ipt(2, 2), ipt(2, 8), ipt(8, 8), ipt(8, 2)}
	cB := rectRing(20, 20, 30, 30)
	hB := oRing{ipt(22, 22), ipt(22, 28), ipt(28, 28), ipt(28, 22)}
	fwd := mustUnion(t,
		polySet{outer: []oRing{cA}, holes: [][]oRing{{hA}}},
		polySet{outer: []oRing{cB}, holes: [][]oRing{{hB}}})
	rev := mustUnion(t,
		polySet{outer: []oRing{cB}, holes: [][]oRing{{hB}}},
		polySet{outer: []oRing{cA}, holes: [][]oRing{{hA}}})
	if len(fwd.outer) != 2 || len(fwd.holes[0]) != 1 || len(fwd.holes[1]) != 1 {
		t.Fatalf("forward component structure wrong: %v", len(fwd.outer))
	}
	if len(rev.outer) != 2 || len(rev.holes[0]) != 1 || len(rev.holes[1]) != 1 {
		t.Fatalf("reversed component structure wrong")
	}
	for i := range fwd.outer {
		if compareRings(fwd.outer[i], rev.outer[i]) != 0 ||
			compareRings(fwd.holes[i][0], rev.holes[i][0]) != 0 {
			t.Fatalf("component order not canonicalized: index %d", i)
		}
	}
	if !oLess(cA[0], cB[0]) || !oLess(fwd.holes[0][0][0], fwd.holes[1][0][0]) {
		t.Fatal("canonical order expectation invalid")
	}
}

func TestSliverWitnessCertified(t *testing.T) {
	// Very thin triangle: repInRing must still produce an interior
	// witness or panic deterministically — never return an exterior
	// point. The witness feeds hole attachment in boolean assembly.
	sliver := oRing{ipt(0, 0), ipt(1000, 0), ipt(500, 1)}
	w, err := repInRing(sliver)
	if err != nil {
		t.Fatalf("sliver witness rejected: %v", err)
	}
	if !ptInRingsWinding(w, ringSegs(sliver)) {
		t.Fatal("sliver witness outside ring")
	}
	if onAnyEdge(w, sliver) {
		t.Fatal("sliver witness landed on boundary")
	}
}

// ---- F7 focused validation sentinel tests -----------------------------

func TestValidationFocusedSentinels(t *testing.T) {
	cases := []struct {
		name string
		ring [][2]float64
		code string
	}{
		{"adjacent-zero-length", [][2]float64{
			{0, 0}, {4, 0}, {4, 4}, {4, 4}}, "zero-length-edge"},
		{"non-adjacent-touch-on-closing-edge", [][2]float64{
			{0, 0}, {8, 0}, {8, 8}, {4, 8}, {4, 4}, {0, 4}, {0, 8}}, "ring-touching-incidence"},
		{"repeated-non-adjacent-vertex", [][2]float64{
			{0, 0}, {8, 0}, {8, 8}, {4, 6}, {8, 8}, {0, 8}}, "ring-touching-incidence"},
		{"duplicate-adjacent-zero-length", [][2]float64{
			{0, 0}, {4, 0}, {4, 4}, {4, 4}}, "zero-length-edge"},
		{"non-adjacent-endpoint-on-edge", [][2]float64{
			{0, 0}, {8, 0}, {8, 8}, {4, 0}, {0, 8}}, "ring-touching-incidence"},
	}
	for _, tc := range cases {
		err := ValidateSimpleRing(tc.ring)
		if err == nil {
			t.Fatalf("%s: must reject", tc.name)
		}
		if se, ok := err.(*SpikeError); !ok || se.Code != tc.code {
			t.Fatalf("%s: got %v want %s", tc.name, err, tc.code)
		}
	}
	// Closing-edge touch with exact sentinel (same geometry class as
	// the table case, asserted separately for the documented policy).
	tshape := [][2]float64{{0, 0}, {8, 0}, {8, 8}, {4, 8}, {4, 4}, {0, 4}, {0, 8}}
	terr := ValidateSimpleRing(tshape)
	if terr == nil {
		t.Fatal("closing-edge touch must reject")
	}
	if se, ok := terr.(*SpikeError); !ok || se.Code != "ring-touching-incidence" {
		t.Fatalf("closing-edge touch: got %v want ring-touching-incidence", terr)
	}
	// Malformed polyset shape through CandBoolean: two hole slices
	// against one outer ring violates the structural invariant.
	badShape := candPolySet{
		outer: []candRing{cr(0, 0, 4, 4)},
		holes: [][]candRing{{}, {}},
	}
	good := cps(cr(10, 10, 14, 14))
	_, err := CandBoolean(candMemUnion, polX, badShape, good)
	if err == nil {
		t.Fatal("malformed polyset must reject")
	}
	if se, ok := err.(*SpikeError); !ok || se.Code != "malformed-polyset-shape" {
		t.Fatalf("malformed polyset: got %v want malformed-polyset-shape", err)
	}
	// Degenerate short ring.
	short := candPolySet{outer: []candRing{{{0, 0}, {1, 1}}}, holes: [][]candRing{nil}}
	_, derr := CandBoolean(candMemUnion, polX, short, good)
	if derr == nil {
		t.Fatal("degenerate ring must reject")
	}
	if se, ok := derr.(*SpikeError); !ok || se.Code != "degenerate-ring" {
		t.Fatalf("degenerate ring: got %v want degenerate-ring", derr)
	}
}
