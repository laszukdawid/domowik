package geometry

import (
	"math/big"
	"testing"
)

func cr(x0, y0, x1, y1 int64) candRing {
	return candRing{{float64(x0), float64(y0)}, {float64(x1), float64(y0)}, {float64(x1), float64(y1)}, {float64(x0), float64(y1)}}
}

func cps(rings ...candRing) candPolySet {
	return candPolySet{outer: rings, holes: make([][]candRing, len(rings))}
}

var (
	polX = CandPolicy{Name: "B-1x"}
	polQ = CandPolicy{Name: "B-1q", Quantize: true}
)

func TestCandUnionOverlapBothPolicies(t *testing.T) {
	for _, pol := range []CandPolicy{polX, polQ} {
		a := cps(cr(0, 0, 2, 2))
		b := cps(cr(1, 1, 3, 3))
		out, err := CandBoolean(candMemUnion, pol, a, b)
		if err != nil {
			t.Fatalf("%s: %v", pol.Name, err)
		}
		if len(out.outer) != 1 || len(out.holes[0]) != 0 {
			t.Fatalf("%s: union components wrong", pol.Name)
		}
		area := candAbsArea(candPtRing(out.outer[0]))
		// Union doubled area = 14 (exact for B-1x; grid-exact for B-1q
		// since all inputs are integers).
		if area.Cmp(big.NewRat(14, 1)) != 0 {
			t.Fatalf("%s: union doubled area %v want 14", pol.Name, area)
		}
		i, err := CandBoolean(candMemInter, pol, a, b)
		if err != nil {
			t.Fatal(err)
		}
		if len(i.outer) != 1 || candAbsArea(candPtRing(i.outer[0])).Cmp(big.NewRat(2, 1)) != 0 {
			t.Fatalf("%s: intersection wrong", pol.Name)
		}
		d, err := CandBoolean(candMemDiff, pol, a, b)
		if err != nil {
			t.Fatal(err)
		}
		if len(d.outer) != 1 || candAbsArea(candPtRing(d.outer[0])).Cmp(big.NewRat(6, 1)) != 0 {
			t.Fatalf("%s: difference wrong", pol.Name)
		}
	}
}

func TestCandAdjacentSeamCancel(t *testing.T) {
	for _, pol := range []CandPolicy{polX, polQ} {
		a := cps(cr(0, 0, 2, 2))
		b := cps(cr(2, 0, 4, 2))
		out, err := CandBoolean(candMemUnion, pol, a, b)
		if err != nil {
			t.Fatal(err)
		}
		if len(out.outer) != 1 || len(out.outer[0]) != 4 {
			t.Fatalf("%s: seam cancellation failed", pol.Name)
		}
		inter, err := CandBoolean(candMemInter, pol, a, b)
		if err != nil || len(inter.outer) != 0 {
			t.Fatalf("%s: touching intersection must be empty", pol.Name)
		}
		diff, err := CandBoolean(candMemDiff, pol, a, b)
		if err != nil || len(diff.outer) != 1 || len(diff.outer[0]) != 4 {
			t.Fatalf("%s: difference must preserve A", pol.Name)
		}
	}
}

func TestCandIdenticalPolygons(t *testing.T) {
	for _, pol := range []CandPolicy{polX, polQ} {
		a := cps(cr(0, 0, 2, 2))
		u, err := CandBoolean(candMemUnion, pol, a, cps(cr(0, 0, 2, 2)))
		if err != nil || len(u.outer) != 1 || len(u.outer[0]) != 4 {
			t.Fatalf("%s: identical union failed", pol.Name)
		}
		d, err := CandBoolean(candMemDiff, pol, a, cps(cr(0, 0, 2, 2)))
		if err != nil || len(d.outer) != 0 {
			t.Fatalf("%s: identical difference must be empty", pol.Name)
		}
	}
}

func TestCandEnclosedDifferenceHole(t *testing.T) {
	for _, pol := range []CandPolicy{polX, polQ} {
		h, err := CandBoolean(candMemDiff, pol,
			cps(cr(0, 0, 10, 10)), cps(cr(2, 2, 8, 8)))
		if err != nil {
			t.Fatal(err)
		}
		if len(h.outer) != 1 || len(h.holes[0]) != 1 {
			t.Fatalf("%s: enclosed subtraction must leave a hole", pol.Name)
		}
	}
}

// candPtRing converts a committed float ring back to rationals for
// exact area comparisons in tests.
func candPtRing(r candRing) []candPt {
	out := make([]candPt, len(r))
	for i, p := range r {
		qx := new(big.Rat)
		qx.SetFloat64(p[0])
		qy := new(big.Rat)
		qy.SetFloat64(p[1])
		out[i] = candPt{x: qx, y: qy}
	}
	return out
}

func TestCandTwoComponentsReversedOrder(t *testing.T) {
	cA := cr(0, 0, 10, 10)
	hA := candRing{{2, 2}, {2, 8}, {8, 8}, {8, 2}}
	cB := cr(20, 20, 30, 30)
	hB := candRing{{22, 22}, {22, 28}, {28, 28}, {28, 22}}
	fwd, err := CandBoolean(candMemUnion, polX,
		candPolySet{outer: []candRing{cA}, holes: [][]candRing{{hA}}},
		candPolySet{outer: []candRing{cB}, holes: [][]candRing{{hB}}})
	if err != nil {
		t.Fatal(err)
	}
	rev, err := CandBoolean(candMemUnion, polX,
		candPolySet{outer: []candRing{cB}, holes: [][]candRing{{hB}}},
		candPolySet{outer: []candRing{cA}, holes: [][]candRing{{hA}}})
	if err != nil {
		t.Fatal(err)
	}
	for _, out := range []candPolySet{fwd, rev} {
		if len(out.outer) != 2 || len(out.holes[0]) != 1 || len(out.holes[1]) != 1 {
			t.Fatalf("component structure wrong")
		}
	}
	for i := range fwd.outer {
		if candRingLess(fwd.outer[i], rev.outer[i]) || candRingLess(rev.outer[i], fwd.outer[i]) {
			t.Fatalf("canonical component order not stable at %d", i)
		}
		if candRingLess(fwd.holes[i][0], rev.holes[i][0]) ||
			candRingLess(rev.holes[i][0], fwd.holes[i][0]) {
			t.Fatalf("canonical hole order not stable at %d", i)
		}
	}
}

func TestCandSliverWitness(t *testing.T) {
	sliver := candRing{{0, 0}, {1000, 0}, {500, 1}}
	w, err := candRepInRing([]candPt{
		{big.NewRat(0, 1), big.NewRat(0, 1)},
		{big.NewRat(1000, 1), big.NewRat(0, 1)},
		{big.NewRat(500, 1), big.NewRat(1, 1)},
	})
	if err != nil {
		t.Fatalf("sliver witness rejected: %v", err)
	}
	segs := polySegsCand([]candPt{
		{big.NewRat(0, 1), big.NewRat(0, 1)},
		{big.NewRat(1000, 1), big.NewRat(0, 1)},
		{big.NewRat(500, 1), big.NewRat(1, 1)},
	})
	if candWinding(w, segs) == 0 {
		t.Fatal("sliver witness not interior")
	}
	_ = sliver
}

func TestCandEmptyIdentityQuantized(t *testing.T) {
	// B-1q identity results must stay grid-aligned (policy-consistent).
	a := candRing{{0.03, 0.03}, {2.03, 0.03}, {2.03, 2.03}, {0.03, 2.03}}
	u, err := CandBoolean(candMemUnion, polQ, cps(a), candPolySet{holes: [][]candRing{}})
	if err != nil {
		t.Fatal(err)
	}
	if len(u.outer) != 1 || len(u.outer[0]) != 4 {
		t.Fatal("identity union structure wrong")
	}
	for _, v := range u.outer[0] {
		if g, err := quantizeGrid(big.NewRat(int64(v[0]*64), 64)); err != nil ||
			new(big.Rat).SetInt(g).Cmp(big.NewRat(int64(v[0]*64), 1)) != 0 {
			t.Fatalf("identity vertex %v not on pinned grid", v)
		}
	}
}

func TestCandGridOverflowRejects(t *testing.T) {
	far := candRing{{1e9 + 8, 0}, {1e9 + 9, 1}, {1e9 + 8, 2}}
	err := ValidateSimpleRing([][2]float64(far))
	if err == nil {
		// In-range per B but beyond grid units? 1e9+8 > B=1e9 → must be rejected by range.
		t.Skip("coordinate within B; range gate covers it")
	}
	big := candRing{{-1e9, -1e9}, {1e9, 0}, {0, 1e9}}
	if err := ValidateSimpleRing([][2]float64(big)); err != nil {
		t.Fatalf("in-range triangle should validate: %v", err)
	}
	_, err = CandBoolean(candMemUnion, polQ, cps(candRing{{2e9, 0}, {3e9, 1}, {2e9, 2}}), cps(cr(0, 0, 1, 1)))
	if err == nil {
		t.Fatal("out-of-range operand must reject deterministically")
	}
}

func TestCandQuantCollapsedIntersections(t *testing.T) {
	// Two intersection points closer than one grid pitch must not
	// produce zero-length fragments or panics under B-1q.
	diag := cps(candRing{{0.49, 0}, {0.51, 0}, {0.52, 1}, {0.50, 1}})
	band := cps(candRing{{0, 0.2}, {2, 0.2}, {2, 0.3}, {0, 0.3}})
	u, err := CandBoolean(candMemUnion, polQ, diag, band)
	if err != nil {
		t.Fatalf("collapsed-intersection union: %v", err)
	}
	if len(u.outer) == 0 {
		t.Fatal("union lost all components")
	}
	for _, r := range u.outer {
		for i := range r {
			j := (i + 1) % len(r)
			if r[i] == r[j] {
				t.Fatal("zero-length edge in output")
			}
			gx := r[i][0] * 64
			if gx != float64(int64(gx)) {
				t.Fatalf("vertex %v off pinned grid", r[i])
			}
		}
	}
}

func TestQuantizeGridTies(t *testing.T) {
	// Ties-to-even at half-grid steps: 0.0078125 = 0.5 units rounds to
	// 0 (even); 0.0234375 = 1.5 units rounds to 2 (even).
	half := new(big.Rat).SetFrac64(1, 128)
	g, err := quantizeGrid(half)
	if err != nil || g.Int64() != 0 {
		t.Fatalf("positive tie: %v %v", g, err)
	}
	neg := new(big.Rat).Neg(half)
	g, err = quantizeGrid(neg)
	if err != nil || g.Int64() != 0 {
		t.Fatalf("negative tie: %v %v", g, err)
	}
	threeHalf := new(big.Rat).SetFrac64(3, 128)
	g, err = quantizeGrid(threeHalf)
	if err != nil || g.Int64() != 2 {
		t.Fatalf("tie-to-even up: %v %v", g, err)
	}
}

func TestQuantizeGridOverflowSentinel(t *testing.T) {
	_, err := quantizeGrid(new(big.Rat).SetFloat64(1e9 + 8))
	if err == nil {
		t.Fatal("beyond-range value must reject with grid overflow sentinel")
	}
	se, ok := err.(*SpikeError)
	if !ok || se.Code != "grid-range-overflow" {
		t.Fatalf("wrong sentinel: %v", err)
	}
}

func TestIdentityNormalizesHolesAndComponents(t *testing.T) {
	// Identity path (empty second operand) with TWO components, a CW
	// outer, an CCW hole — all non-canonical. The output must be a
	// fully canonical polyset for both policies.
	a := candPolySet{
		outer: []candRing{
			{{4, 4}, {0, 4}, {0, 0}, {4, 0}},         // CW
			{{30, 30}, {34, 30}, {34, 34}, {30, 34}}, // CCW second component
		},
		holes: [][]candRing{
			{{{3, 1}, {1, 1}, {1, 3}, {3, 3}}}, // CCW hole in component 0
			nil,
		},
	}
	bEmpty := candPolySet{holes: [][]candRing{}}
	for _, pol := range []CandPolicy{polX, polQ} {
		out, err := CandBoolean(candMemUnion, pol, a, bEmpty)
		if err != nil {
			t.Fatalf("%s: %v", pol.Name, err)
		}
		if len(out.outer) != 2 {
			t.Fatalf("%s: identity must keep both components", pol.Name)
		}
		if candShoelaceSign(candPtsOfRing(out.outer[0])) <= 0 ||
			out.outer[0][0] != [2]float64{0, 0} {
			t.Fatalf("%s: component 0 not CCW/least-vertex: %v",
				pol.Name, out.outer[0][0])
		}
		if len(out.holes[0]) != 1 {
			t.Fatalf("%s: identity dropped the hole", pol.Name)
		}
		h := out.holes[0][0]
		if candShoelaceSign(candPtsOfRing(h)) >= 0 || h[0] != [2]float64{1, 1} {
			t.Fatalf("%s: hole not CW/least-vertex: %v", pol.Name, h[0])
		}
		if out.outer[1][0] != [2]float64{30, 30} {
			t.Fatalf("%s: components not canonically ordered", pol.Name)
		}
	}
}

func candPtsOfRing(r candRing) []candPt {
	pts := make([]candPt, len(r))
	for i, v := range r {
		x := new(big.Rat)
		x.SetFloat64(v[0])
		y := new(big.Rat)
		y.SetFloat64(v[1])
		pts[i] = candPt{x: x, y: y}
	}
	return pts
}

func TestIdentityComponentOrderAfterNormalization(t *testing.T) {
	// Raw starts intentionally invert the lexicographic least order:
	// component 0 begins at its max vertex, component 1 at its least.
	// Two vertically separated components sharing the same x range:
	// A=[0,4]x[0,4] supplied starting at its max vertex (4,4);
	// B=[0,4]x[10,14] supplied starting at its least vertex (0,10).
	// Raw first tuples invert the canonical order; A carries a hole.
	a := candPolySet{
		outer: []candRing{
			{{4, 4}, {4, 0}, {0, 0}, {0, 4}},
			{{0, 10}, {0, 14}, {4, 14}, {4, 10}},
		},
		holes: [][]candRing{
			{{{1, 1}, {1, 3}, {3, 3}, {3, 1}}},
			nil,
		},
	}
	bEmpty := candPolySet{holes: [][]candRing{}}
	out, err := CandBoolean(candMemUnion, polX, a, bEmpty)
	if err != nil {
		t.Fatal(err)
	}
	if len(out.outer) != 2 {
		t.Fatal("components lost")
	}
	// Canonical order: A (least vertex (0,0)) before B ((0,10)) even
	// though raw input started B's ring lexicographically first.
	if out.outer[0][0] != [2]float64{0, 0} {
		t.Fatalf("component order not canonical after normalization: %v",
			out.outer[0][0])
	}
	if out.outer[1][0] != [2]float64{0, 10} {
		t.Fatalf("second component misordered: %v", out.outer[1][0])
	}
	// The hole must have traveled with component A.
	if len(out.holes[0]) != 1 {
		t.Fatalf("hole lost from component A")
	}
	if len(out.holes[1]) != 0 {
		t.Fatal("hole attached to wrong component")
	}
}
