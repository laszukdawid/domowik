package geometry

import (
	"math/big"
	"math/rand"
	"reflect"
	"testing"
)

// rectRingsF converts corpus rectangles into per-rect operand rings.
func rectRingF(r [4]float64) candRing {
	return candRing{{r[0], r[1]}, {r[2], r[1]}, {r[2], r[3]}, {r[0], r[3]}}
}

func rectsHalvedRings(rects [][4]float64) (candPolySet, candPolySet) {
	var a, b candPolySet
	for i, r := range rects {
		ring := rectRingF(r)
		if i%2 == 0 {
			a.outer = append(a.outer, ring)
		} else {
			b.outer = append(b.outer, ring)
		}
	}
	a.holes = make([][]candRing, len(a.outer))
	b.holes = make([][]candRing, len(b.outer))
	return a, b
}

func permutePolySet(rng *rand.Rand, s candPolySet) candPolySet {
	out := candPolySet{}
	idx := rng.Perm(len(s.outer))
	for _, i := range idx {
		r := append(candRing{}, s.outer[i]...)
		if rng.Intn(2) == 0 { // whole-ring reversal (normative R3.3.6 form)
			for j := range r {
				r[len(r)-1-j] = s.outer[i][j]
			}
		}
		rot := rng.Intn(len(r)) // arbitrary start rotation
		rotated := append(append(candRing{}, r[rot:]...), r[:rot]...)
		out.outer = append(out.outer, rotated)
		var holes []candRing
		for _, h := range s.holes[i] {
			hh := append(candRing{}, h...)
			if rng.Intn(2) == 0 {
				for j := range hh {
					hh[len(hh)-1-j] = h[j]
				}
			}
			hrot := rng.Intn(len(hh))
			holes = append(holes, append(append(candRing{}, hh[hrot:]...), hh[:hrot]...))
		}
		out.holes = append(out.holes, holes)
	}
	return out
}

// TestCandBooleanInputPermutationInvariance asserts that both boolean
// policies produce IDENTICAL canonical polysets when operand ring
// order, ring start vertices, and ring directions are permuted
// arbitrarily, for union, intersection, and difference. This is
// input-permutation evidence, distinct from Go test-order shuffle.
// All comparisons are exact canonical equality on complete polysets
// (outer + holes); no raw ring-order tolerance exists anywhere in the
// comparison path.
func TestCandBooleanInputPermutationInvariance(t *testing.T) {
	ops := []struct {
		name string
		mem  candMem
	}{
		{"union", candMemUnion},
		{"intersection", candMemInter},
		{"difference", candMemDiff},
	}
	for _, tc := range buildCorpus() {
		baseA, baseB := rectsHalvedRings(tc.rects)
		type key struct {
			op  string
			pol string
		}
		baselines := map[key]candPolySet{}
		for _, op := range ops {
			for _, pol := range []CandPolicy{polX, polQ} {
				out, err := CandBoolean(op.mem, pol, baseA, baseB)
				if err != nil {
					t.Fatalf("%s/%s/%s baseline: %v", tc.name, op.name, pol.Name, err)
				}
				baselines[key{op.name, pol.Name}] = out
			}
		}
		for seed := 1; seed <= 32; seed++ {
			rng := rand.New(rand.NewSource(int64(seed)))
			pa := permutePolySet(rng, baseA)
			pb := permutePolySet(rng, baseB)
			for _, op := range ops {
				for _, pol := range []CandPolicy{polX, polQ} {
					got, err := CandBoolean(op.mem, pol, pa, pb)
					if err != nil {
						t.Fatalf("%s/%s/%s seed %d: %v", tc.name, op.name, pol.Name, seed, err)
					}
					if !reflect.DeepEqual(baselines[key{op.name, pol.Name}], got) {
						t.Fatalf("%s/%s/%s seed %d: canonical output depends on input permutation", tc.name, op.name, pol.Name, seed)
					}
				}
			}
		}
	}
}

// TestBooleanPinchSemantics pins the repaired closed-contact semantics
// to LITERAL independent fixtures for BOTH policies AND the oracle:
//
//   - touching-vertices-pinch-corner (two squares sharing corner
//     (6,6)): exactly two simple 4-vertex outers;
//   - point-pinch: one outer with exactly two 6-vertex holes (free
//     lobes split per R5.1.2/F35).
func TestBooleanPinchSemantics(t *testing.T) {
	pinchRects := concat(rectList(0, 0, 6, 6), rectList(6, 6, 12, 12))
	wantOuters := [][][2]float64{
		{{0, 0}, {6, 0}, {6, 6}, {0, 6}},
		{{6, 6}, {12, 6}, {12, 12}, {6, 12}},
	}
	ca, cb := rectsHalvedRings(pinchRects)
	for _, pol := range []CandPolicy{polX, polQ} {
		out, err := CandBoolean(candMemUnion, pol, ca, cb)
		if err != nil {
			t.Fatalf("%s: %v", pol.Name, err)
		}
		if len(out.outer) != 2 {
			t.Fatalf("%s: pinch corner must stay two components, got %d", pol.Name, len(out.outer))
		}
		for i := range wantOuters {
			if !ringsEqual(out.outer[i], wantOuters[i]) {
				t.Fatalf("%s: outer %d = %v want %v", pol.Name, i, out.outer[i], wantOuters[i])
			}
		}
	}
	oUnion := mustOracleUnion(t, pinchRects)
	if len(oUnion.outer) != 2 {
		t.Fatalf("oracle pinch corner must normalize to two simple outers, got %d", len(oUnion.outer))
	}
	for i := range wantOuters {
		got := oracleRingToF(oUnion.outer[i])
		if !ringsEqual(got, wantOuters[i]) {
			t.Fatalf("oracle outer %d = %v want %v", i, got, wantOuters[i])
		}
	}

	ppRects := concat3(bandRects(0, 0, 30, 30, 2), rectList(10, 2, 18, 18), rectList(18, 18, 26, 28))
	// Exact free-lobe rings (oracle-derived, independently verified
	// against geometry): NW lobe and SE lobe, six vertices each,
	// pinched apart at the single occupied point (18,18) per
	// R5.1.2/F35 open-set splitting.
	wantHole0 := [][2]float64{{2, 2}, {2, 28}, {18, 28}, {18, 18}, {10, 18}, {10, 2}}
	wantHole1 := [][2]float64{{18, 2}, {18, 18}, {26, 18}, {26, 28}, {28, 28}, {28, 2}}
	pa, pb := rectsHalvedRings(ppRects)
	for _, pol := range []CandPolicy{polX, polQ} {
		out, err := CandBoolean(candMemUnion, pol, pa, pb)
		if err != nil {
			t.Fatalf("%s point-pinch: %v", pol.Name, err)
		}
		if len(out.outer) != 1 || len(out.holes[0]) != 2 {
			t.Fatalf("%s: point-pinch must be one outer with two lobes, got %d/%d",
				pol.Name, len(out.outer), len(out.holes[0]))
		}
		for hi, want := range [][][2]float64{wantHole0, wantHole1} {
			if len(out.holes[0][hi]) != 6 {
				t.Fatalf("%s: lobe %d must have 6 vertices, got %d", pol.Name, hi, len(out.holes[0][hi]))
			}
			if !ringsEqual(out.holes[0][hi], want) {
				t.Fatalf("%s: lobe %d = %v want %v", pol.Name, hi, out.holes[0][hi], want)
			}
		}
	}
	ppUnion := mustOracleUnion(t, ppRects)
	if len(ppUnion.outer) != 1 || len(ppUnion.holes[0]) != 2 {
		t.Fatalf("oracle point-pinch must be one outer with two holes, got %d/%d",
			len(ppUnion.outer), len(ppUnion.holes[0]))
	}
	for hi := 0; hi < 2; hi++ {
		got := oracleRingToF(ppUnion.holes[0][hi])
		want := wantHole0
		if hi == 1 {
			want = wantHole1
		}
		if !ringsEqual(got, want) {
			t.Fatalf("oracle lobe %d = %v want %v (exact normalization split required)", hi, got, want)
		}
	}
}

// mustOracleUnion runs the exact oracle union over corpus rectangles.
func mustOracleUnion(t *testing.T, rects [][4]float64) polySet {
	t.Helper()
	oa, ob := rectsToOraclePolySet(rects)
	out, err := BooleanUnion(oa, ob)
	if err != nil {
		t.Fatalf("oracle union: %v", err)
	}
	return out
}

// oracleRingToF converts an exact oracle ring to committed floats.
func oracleRingToF(r oRing) [][2]float64 {
	out := make([][2]float64, len(r))
	for i, p := range r {
		rx, _ := p.x.Rat()
		fx, _ := rx.Float64()
		ry, _ := p.y.Rat()
		fy, _ := ry.Float64()
		out[i] = [2]float64{fx, fy}
	}
	return out
}

func crat(x, y float64) candPt {
	rx := new(big.Rat)
	rx.SetFloat64(x)
	ry := new(big.Rat)
	ry.SetFloat64(y)
	return candPt{x: rx, y: ry}
}

// oIntPt builds an exact rational oracle point from integers.
func oIntPt(x, y int64) oPt {
	return oPt{x: xr(new(big.Rat).SetInt64(x)), y: xr(new(big.Rat).SetInt64(y))}
}

// TestCandChainPinchBranchPinning pins the successor convention at
// the exact balanced branch node of the touching-corner union: four
// pieces meet at (6,6) - incoming northbound (bottom square right
// edge) and southbound (top square left edge); outgoing westbound
// (bottom square top edge) and eastbound (top square bottom edge).
// The fixed pairing crosses the contact (N-in -> E-out, S-in -> W-out)
// so candChain yields ONE closed walk through the pinch, whose lobes
// are then separated by candSplitSimpleWalk into exactly two simple
// square rings. SCOPE: this is traversal-mechanics coverage of the
// angular comparator and orbit closure only; it is NOT a normative
// proof of occupied-left semantics. The independent semantic
// expectation for this geometry is asserted against oracle-derived
// literals in TestBooleanPinchSemantics. (The abstract one-in/two-out
// node cannot occur here: closed obstacle-left boundary graphs have
// equal in/out degree at every node.)
func TestCandChainPinchBranchPinning(t *testing.T) {
	pieces := []candSeg{
		{crat(0, 0), crat(6, 0)}, // bottom square CCW
		{crat(6, 0), crat(6, 6)},
		{crat(6, 6), crat(0, 6)},
		{crat(0, 6), crat(0, 0)},
		{crat(6, 6), crat(12, 6)}, // top square CCW
		{crat(12, 6), crat(12, 12)},
		{crat(12, 12), crat(6, 12)},
		{crat(6, 12), crat(6, 6)},
	}
	cycles, err := candChain(pieces)
	if err != nil {
		t.Fatalf("candChain: %v", err)
	}
	var rings [][]candPt
	for _, c := range cycles {
		rings = append(rings, candSplitSimpleWalk(c)...)
	}
	if len(rings) != 2 {
		t.Fatalf("pinch walk must split into two simple rings, got %d", len(rings))
	}
	wantA := []candPt{crat(0, 0), crat(6, 0), crat(6, 6), crat(0, 6)}
	wantB := []candPt{crat(6, 6), crat(12, 6), crat(12, 12), crat(6, 12)}
	rot := func(r []candPt) []candPt {
		least := 0
		for i := 1; i < len(r); i++ {
			if r[i].x.Cmp(r[least].x) < 0 || (r[i].x.Cmp(r[least].x) == 0 && r[i].y.Cmp(r[least].y) < 0) {
				least = i
			}
		}
		return append(append([]candPt{}, r[least:]...), r[:least]...)
	}
	eqCyc := func(a, b []candPt) bool {
		for i := range a {
			if a[i].x.Cmp(b[i].x) != 0 || a[i].y.Cmp(b[i].y) != 0 {
				return false
			}
		}
		return true
	}
	gotA, gotB := rot(rings[0]), rot(rings[1])
	wantA = append([]candPt{}, wantA...)
	wantB = append([]candPt{}, wantB...)
	if !eqCyc(gotA, wantA) || !eqCyc(gotB, wantB) {
		if !eqCyc(gotA, wantB) || !eqCyc(gotB, wantA) {
			t.Fatalf("split rings mismatch: got %v / %v", rings[0], rings[1])
		}
	}
}

// TestSimpleRingValidatorRejects exercises the candidate exact
// simple-ring postcondition on literal degenerate shapes.
func TestSimpleRingValidatorRejects(t *testing.T) {
	mk := func(coords ...float64) []candPt {
		var out []candPt
		for i := 0; i+1 < len(coords); i += 2 {
			out = append(out, crat(coords[i], coords[i+1]))
		}
		return out
	}
	if !candRingIsSimple(mk(0, 0, 4, 0, 4, 4, 0, 4)) {
		t.Fatal("valid square must accept")
	}
	if candRingIsSimple(mk(0, 0, 4, 4, 4, 0, 0, 4)) {
		t.Fatal("bow-tie self-crossing must reject")
	}
	if candRingIsSimple(mk(0, 0, 4, 0, 4, 4, 0, 4, 0, 2, 2, 2, 2, 0)) {
		t.Fatal("endpoint contacting a non-adjacent edge must reject")
	}
	if candRingIsSimple(mk(0, 0, 4, 0, 4, 2, 6, 2, 6, 0, 2, 0, 0, 2)) {
		t.Fatal("non-adjacent collinear overlap along y=0 must reject")
	}
	// Disjoint collinear non-adjacent edges (U-notch bottom) remain
	// valid: asserted acceptance.
	if !candRingIsSimple(mk(0, 0, 3, 0, 3, 2, 5, 2, 5, 0, 8, 0, 8, 4, 0, 4)) {
		t.Fatal("disjoint collinear edges must accept")
	}
	if candRingIsSimple(mk(0, 0, 4, 0, 4, 4, 0, 4, 0, 0)) {
		t.Fatal("zero-length closing edge must reject")
	}
}

// TestSplitRepeatedWalkDeterminism pins walk-splitting behavior.
func TestSplitRepeatedWalkDeterminism(t *testing.T) {
	walk := []oPt{
		oIntPt(6, 6), oIntPt(12, 6), oIntPt(12, 12), oIntPt(6, 12),
		oIntPt(6, 6), oIntPt(0, 12), oIntPt(0, 0), oIntPt(6, 0),
	}
	parts := splitRepeatedWalk(walk)
	if len(parts) != 2 {
		t.Fatalf("figure-eight walk must split into two simple walks, got %d", len(parts))
	}
	for _, p := range parts {
		if !oRingSimple(p) {
			t.Fatalf("split part %v is not simple", p)
		}
	}
}

// TestOracleBooleanRawPinchCycles pins the RAW pre-normalization
// chain output of the oracle boolean pipeline for both closed-contact
// corpus cases, characterizing exactly what the post-decision
// normalizer consumes. Run via rawBoundaryCycles (the real decision
// machinery, no normalization).
func TestOracleBooleanRawPinchCycles(t *testing.T) {
	tc := concat(rectList(0, 0, 6, 6), rectList(6, 6, 12, 12))
	oa, ob := rectsToOraclePolySet(tc)
	cycles, err := rawBoundaryCycles(oa, ob, memUnion)
	if err != nil {
		t.Fatalf("raw touching-corner: %v", err)
	}
	if len(cycles) != 1 || len(cycles[0]) != 8 {
		t.Fatalf("raw touching-corner must be ONE repeated-node 8-vertex walk, got %d walks/%v lens",
			len(cycles), len(cycles[0]))
	}
	key := func(p oPt) string { return ptKey(p) }
	counts := map[string]int{}
	for _, p := range cycles[0] {
		counts[key(p)]++
	}
	if counts[ptKey(oIntPt(6, 6))] != 2 {
		t.Fatal("raw touching-corner walk must visit the pinch vertex (6,6) twice")
	}

	pp := concat3(bandRects(0, 0, 30, 30, 2), rectList(10, 2, 18, 18), rectList(18, 18, 26, 28))
	pa, pb := rectsToOraclePolySet(pp)
	pc, err := rawBoundaryCycles(pa, pb, memUnion)
	if err != nil {
		t.Fatalf("raw point-pinch: %v", err)
	}
	// Measured raw structure at base 126fa59/d900326 and after
	// normalization refactor: ONE 8-vertex CCW outer walk (frame
	// perimeter including band-junction collinear vertices) and TWO
	// 6-vertex CW lobe walks. The raw point-pinch chain does NOT
	// contain a repeated-node walk under this operand split; the
	// touching-corner case above is the pinned repeated-node example.
	if len(pc) != 3 {
		t.Fatalf("raw point-pinch must be three walks, got %d", len(pc))
	}
	lens := map[int]int{}
	for _, c := range pc {
		lens[len(c)]++
	}
	if lens[8] != 1 || lens[6] != 2 {
		t.Fatalf("raw point-pinch must be one 8-vertex outer walk and two 6-vertex lobe walks, got %v", lens)
	}
	for _, c := range pc {
		if !oRingSimple(c) {
			t.Fatalf("raw point-pinch walk %v is not simple", c)
		}
	}
}
