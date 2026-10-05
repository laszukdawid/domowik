package geometry

import (
	"math/big"
	"testing"
)

// Named corpus for issue 0007 (AC3). Each case is a list of occupied
// rectangles (wall bands / patch proxies) shared between the exact
// oracle, the candidate face extractor, and both candidate boolean
// policies. Expected free-face topology is stated per case.
type corpusCase struct {
	name     string
	rects    [][4]float64
	boolOnly bool // excluded from CandFaces domain; boolean/oracle only
	expFaces int
	expHoles int
	// expOuters holds INDEPENDENT analytically-derived canonical outer
	// rings (Tier-1 rational cases with known interiors). Cases with
	// complex lobes rely on count-level + F35-style exact fixtures.
	expOuters [][][2]float64
}

func concat(a, b [][4]float64) [][4]float64 {
	return append(append([][4]float64{}, a...), b...)
}

func concat3(a, b, c [][4]float64) [][4]float64 {
	return append(concat(a, b), c...)
}

func rectList(x0, y0, x1, y1 float64) [][4]float64 {
	return [][4]float64{{x0, y0, x1, y1}}
}

// bandRects returns four corner-butted wall bands enclosing the open
// square (x0+t,y0+t)-(x1-t,y1-t).
func bandRects(x0, y0, x1, y1, t float64) [][4]float64 {
	return [][4]float64{
		{x0, y0, x1, y0 + t},
		{x0, y1 - t, x1, y1},
		{x0, y0 + t, x0 + t, y1 - t},
		{x1 - t, y0 + t, x1, y1 - t},
	}
}

func islandBands() [][4]float64 {
	return bandRects(10, 10, 20, 18, 2)
}

func buildCorpus() []corpusCase {
	return []corpusCase{
		{name: "single-room", rects: bandRects(0, 0, 12, 12, 2), expFaces: 1,
			expOuters: [][][2]float64{{{2, 2}, {10, 2}, {10, 10}, {2, 10}}}},
		{name: "disjoint-rooms",
			rects:    concat(bandRects(0, 0, 12, 12, 2), bandRects(40, 0, 52, 12, 2)),
			expFaces: 2},
		{name: "thick-L-join",
			rects:    concat(rectList(0, 8, 20, 12), rectList(0, 0, 4, 12)),
			expFaces: 0},
		{name: "unequal-thickness-T",
			rects:    concat(rectList(0, 8, 20, 12), rectList(9, 0, 11, 20)),
			expFaces: 0},
		{name: "X-crossing",
			rects:    concat(rectList(8, 9, 24, 11), rectList(15, 2, 17, 18)),
			expFaces: 0},
		{name: "shared-edge-touching-bands",
			rects:    concat(rectList(0, 0, 10, 5), rectList(10, 0, 20, 5)),
			expFaces: 0},
		{name: "touching-vertices-pinch-corner",
			rects:    concat(rectList(0, 0, 6, 6), rectList(6, 6, 12, 12)),
			expFaces: 0},
		{name: "sliver-overlap",
			rects:    concat(rectList(0, 0, 2000, 10), rectList(1000, 9, 3000, 19)),
			expFaces: 0},
		{name: "courtyard-island",
			rects:    concat(bandRects(0, 0, 30, 30, 2), islandBands()),
			expFaces: 2, expHoles: 1,
			expOuters: [][][2]float64{
				{{2, 2}, {28, 2}, {28, 28}, {2, 28}},
				{{12, 12}, {18, 12}, {18, 16}, {12, 16}},
			}},
		{name: "point-pinch",
			rects:    concat3(bandRects(0, 0, 30, 30, 2), rectList(10, 2, 18, 18), rectList(18, 18, 26, 28)),
			expFaces: 2},
		{name: "large-translation",
			rects:    bandRects(4e6, 4e6, 4e6+12, 4e6+12, 2),
			expFaces: 1,
			expOuters: [][][2]float64{{
				{4000002, 4000002}, {4000010, 4000002},
				{4000010, 4000010}, {4000002, 4000010},
			}}},
		{name: "near-B-translation",
			rects:    bandRects(1e9-60, 1e9-60, 1e9-40, 1e9-20, 2),
			boolOnly: true,
			expFaces: 0},
	}
}

func rectsToSegs(rects [][4]float64) [][2][2]float64 {
	var out [][2][2]float64
	for _, r := range rects {
		out = append(out, rectSegsF(r[0], r[1], r[2], r[3])...)
	}
	return out
}

func rectSegsF(x0, y0, x1, y1 float64) [][2][2]float64 {
	pts := [][2]float64{{x0, y0}, {x1, y0}, {x1, y1}, {x0, y1}}
	var out [][2][2]float64
	for i := range pts {
		j := (i + 1) % len(pts)
		out = append(out, [2][2]float64{pts[i], pts[j]})
	}
	return out
}

// rectsToCand keeps stored binary64 values untouched (no int64 cast)
// and allocates holes with the same length as outer per shape policy.
func rectsToCand(rects [][4]float64) candPolySet {
	out := candPolySet{holes: make([][]candRing, len(rects))}
	for i, r := range rects {
		out.outer = append(out.outer, candRing{{r[0], r[1]}, {r[2], r[1]}, {r[2], r[3]}, {r[0], r[3]}})
		out.holes[i] = []candRing{}
	}
	return out
}

// oracleFacesFloat runs the independent oracle over corpus rectangles.
func oracleFacesFloat(t *testing.T, rects [][4]float64) OArrangement {
	t.Helper()
	var segs []oSeg
	for _, sg := range rectsToSegs(rects) {
		ax, err := XFloat(sg[0][0])
		if err != nil {
			t.Fatal(err)
		}
		ay, err := XFloat(sg[0][1])
		if err != nil {
			t.Fatal(err)
		}
		bx, err := XFloat(sg[1][0])
		if err != nil {
			t.Fatal(err)
		}
		by, err := XFloat(sg[1][1])
		if err != nil {
			t.Fatal(err)
		}
		segs = append(segs, oSeg{a: oPt{x: ax, y: ay}, b: oPt{x: bx, y: by}})
	}
	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	return arr
}

// TestCorpusTopologyMatchesOracle is the AC4/AC5 discrete results
// table: face counts, canonical rings, hole totals, and parent links
// must agree between oracle and candidate extractor. Failures are the
// recorded topology mismatches.
func TestCorpusTopologyMatchesOracle(t *testing.T) {
	mismatches := 0
	for _, tc := range buildCorpus() {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			if tc.boolOnly {
				t.Skip("outside documented candidate-face domain; covered by boolean/near-B tests")
			}
			arr := oracleFacesFloat(t, tc.rects)
			if len(arr.Faces) != tc.expFaces {
				t.Fatalf("oracle faces=%d want %d", len(arr.Faces), tc.expFaces)
			}
			cf, err := CandFaces(rectsToSegs(tc.rects))
			if err != nil {
				t.Fatalf("candidate faces: %v", err)
			}
			if len(cf.Faces) != tc.expFaces {
				t.Fatalf("candidate faces=%d want %d", len(cf.Faces), tc.expFaces)
			}
			// Independent expectation check (F11): both engines must
			// equal the analytically derived rings where provided.
			for i, wo := range tc.expOuters {
				gotO := make([][2]float64, len(arr.Faces[i].Outer))
				for j, p := range arr.Faces[i].Outer {
					rx, _ := p.x.Rat()
					fx, _ := rx.Float64()
					ry, _ := p.y.Rat()
					fy, _ := ry.Float64()
					gotO[j] = [2]float64{fx, fy}
				}
				// Exact canonical assertion: no reversal tolerance —
				// both engines must emit least-vertex CCW form.
				if !ringsEqual(gotO, wo) {
					t.Fatalf("oracle face %d violates INDEPENDENT expectation %v", i, wo)
				}
				if !ringsEqual(cf.Faces[i].Outer, wo) {
					t.Fatalf("candidate face %d violates INDEPENDENT expectation %v", i, wo)
				}
			}
			totalHoles := 0
			for i := range arr.Faces {
				want := make([][2]float64, len(arr.Faces[i].Outer))
				for j, p := range arr.Faces[i].Outer {
					rx, _ := p.x.Rat()
					f, _ := rx.Float64()
					ry, _ := p.y.Rat()
					g, _ := ry.Float64()
					want[j] = [2]float64{f, g}
				}
				if !ringsEqual(cf.Faces[i].Outer, want) {
					mismatches++
					t.Fatalf("face %d ring differs from oracle (canonical)", i)
				}
				if cf.Faces[i].Parent != arr.Faces[i].Parent {
					mismatches++
					t.Fatalf("face %d parent mismatch", i)
				}
				// Cross-engine hole ring equality (exact canonical).
				if len(cf.Faces[i].Holes) != len(arr.Faces[i].Holes) {
					mismatches++
					t.Fatalf("face %d hole count differs from oracle", i)
				}
				for hi := range arr.Faces[i].Holes {
					wantH := make([][2]float64, len(arr.Faces[i].Holes[hi]))
					for vi, p := range arr.Faces[i].Holes[hi] {
						rx, _ := p.x.Rat()
						fx, _ := rx.Float64()
						ry, _ := p.y.Rat()
						fy, _ := ry.Float64()
						wantH[vi] = [2]float64{fx, fy}
					}
					if !ringsEqual(cf.Faces[i].Holes[hi], wantH) {
						mismatches++
						t.Fatalf("face %d hole %d ring differs from oracle", i, hi)
					}
				}
				totalHoles += len(cf.Faces[i].Holes)
			}
			if totalHoles != tc.expHoles {
				t.Fatalf("holes=%d want %d", totalHoles, tc.expHoles)
			}
		})
	}
	if mismatches != 0 {
		t.Fatalf("corpus topology mismatches: %d", mismatches)
	}
}

func revRingF(r [][2]float64) [][2]float64 {
	out := make([][2]float64, len(r))
	for i := range r {
		out[len(r)-1-i] = r[i]
	}
	return out
}

// TestCorpusOpeningDifference exercises R4.2.2 cut-before-union,
// order invariance, join patches never subtracted, full-thickness
// disconnection versus partial notch.
func TestCorpusOpeningDifference(t *testing.T) {
	wall := cps(cr(0, 0, 12, 2))
	door := cps(cr(4, 0, 8, 2))
	for _, pol := range []CandPolicy{polX, polQ} {
		d, err := CandBoolean(candMemDiff, pol, wall, door)
		if err != nil {
			t.Fatal(err)
		}
		if len(d.outer) != 2 {
			t.Fatalf("%s: full cut must disconnect into 2 stubs, got %d", pol.Name, len(d.outer))
		}
		second := cps(cr(9, 0, 11, 2))
		uab, err := CandBoolean(candMemUnion, pol, door, second)
		if err != nil {
			t.Fatal(err)
		}
		uba, err := CandBoolean(candMemUnion, pol, second, door)
		if err != nil {
			t.Fatal(err)
		}
		ab, err := CandBoolean(candMemDiff, pol, wall, uab)
		if err != nil {
			t.Fatal(err)
		}
		ba, err := CandBoolean(candMemDiff, pol, wall, uba)
		if err != nil {
			t.Fatal(err)
		}
		if len(ab.outer) != len(ba.outer) {
			t.Fatalf("%s: opening order changes component count", pol.Name)
		}
		window := cps(cr(5, 0, 7, 1)) // partial depth: NOTCH
		n, err := CandBoolean(candMemDiff, pol, wall, window)
		if err != nil {
			t.Fatal(err)
		}
		if len(n.outer) != 1 || len(n.outer[0]) < 3 {
			t.Fatalf("%s: partial depth must stay connected", pol.Name)
		}
		// Join patch never subtracted (R4.2.2): patch rectangle over
		// the aperture zone survives difference.
		patch := cps(cr(5, 0, 7, 2))
		pd, err := CandBoolean(candMemDiff, pol, patch, window)
		if err != nil {
			t.Fatal(err)
		}
		if len(pd.outer) == 0 {
			t.Fatalf("%s: join patch was subtracted", pol.Name)
		}
	}
	oDiff, err := BooleanDifference(
		polySet{outer: []oRing{rectRing(0, 0, 12, 2)}, holes: make([][]oRing, 1)},
		polySet{outer: []oRing{rectRing(4, 0, 8, 2)}, holes: make([][]oRing, 1)})
	if err != nil {
		t.Fatal(err)
	}
	if len(oDiff.outer) != 2 {
		t.Fatalf("oracle full-cut components=%d want 2", len(oDiff.outer))
	}
}

// TestCorpusReversalInvariance asserts R3.3.6/R4.2.3 neutrality.
func TestCorpusReversalInvariance(t *testing.T) {
	for _, tc := range buildCorpus() {
		if tc.boolOnly {
			continue
		}
		fwd, err := CandFaces(rectsToSegs(tc.rects))
		if err != nil {
			t.Fatalf("%s: %v", tc.name, err)
		}
		var revBands [][2][2]float64
		for _, sg := range rectsToSegs(tc.rects) {
			revBands = append(revBands, [2][2]float64{sg[1], sg[0]})
		}
		rev, err := CandFaces(revBands)
		if err != nil {
			t.Fatalf("%s reversed: %v", tc.name, err)
		}
		if len(fwd.Faces) != len(rev.Faces) ||
			len(fwd.Exterior.Holes) != len(rev.Exterior.Holes) {
			t.Fatalf("%s: reversal changes topology", tc.name)
		}
		for hi := range fwd.Exterior.Holes {
			if !ringsEqual(fwd.Exterior.Holes[hi], rev.Exterior.Holes[hi]) {
				t.Fatalf("%s: exterior hole %d contents changed under reversal",
					tc.name, hi)
			}
		}
		for i := range fwd.Faces {
			if fwd.Faces[i].Parent != rev.Faces[i].Parent {
				t.Fatalf("%s: face %d Parent changed under reversal", tc.name, i)
			}
			if !ringsEqual(fwd.Faces[i].Outer, rev.Faces[i].Outer) {
				t.Fatalf("%s: face %d ring changed under reversal", tc.name, i)
			}
			if len(fwd.Faces[i].Holes) != len(rev.Faces[i].Holes) {
				t.Fatalf("%s: face %d hole count changed under reversal", tc.name, i)
			}
			for hi := range fwd.Faces[i].Holes {
				if !ringsEqual(fwd.Faces[i].Holes[hi], rev.Faces[i].Holes[hi]) {
					t.Fatalf("%s: face %d hole %d contents changed under reversal",
						tc.name, i, hi)
				}
			}
		}
	}
}

// rectsToOraclePolySet builds exact operand polySets from corpus
// rectangles, alternating operands so real boolean work happens.
func rectsToOraclePolySet(rects [][4]float64) (polySet, polySet) {
	var oa, ob polySet
	for i, r := range rects {
		ring := rectRing(int64(r[0]), int64(r[1]), int64(r[2]), int64(r[3]))
		if i%2 == 0 {
			oa.outer = append(oa.outer, ring)
			oa.holes = append(oa.holes, nil)
		} else {
			ob.outer = append(ob.outer, ring)
			ob.holes = append(ob.holes, nil)
		}
	}
	return oa, ob
}

// candRectsHalved splits the rect list into two candidate operands,
// matching the oracle operand split above.
func candRectsHalved(rects [][4]float64) (candPolySet, candPolySet) {
	var a, b candPolySet
	for i, r := range rects {
		ring := candRing{{r[0], r[1]}, {r[2], r[1]}, {r[2], r[3]}, {r[0], r[3]}}
		if i%2 == 0 {
			a.outer = append(a.outer, ring)
		} else {
			b.outer = append(b.outer, ring)
		}
	}
	// Shape invariant: one hole slice per outer ring.
	a.holes = make([][]candRing, len(a.outer))
	b.holes = make([][]candRing, len(b.outer))
	return a, b
}

// TestCorpusBooleanVsOracle is the AC5 harness for the candidate
// booleans: per case and per policy the union result is compared to
// the exact oracle BooleanUnion over the same corpus rectangles, with
// operands split even/odd on both sides by rectsToOraclePolySet /
// candRectsHalved. Component or
// vertex-count mismatches increment that policy's topology-mismatch
// count; matched rings contribute vertex-wise coordinate error to that
// policy's separate continuous budget. Mismatch counts/rates and
// coordinate errors are reported separately; NO pass/fail threshold
// exists here — tracker 0030 judges recorded evidence.
func TestCorpusBooleanVsOracle(t *testing.T) {
	type stats struct {
		cases      int
		mismatches int
		maxErrSq   *big.Rat
		errVerts   int
	}
	res := map[string]*stats{
		"B-1x": {maxErrSq: new(big.Rat)},
		"B-1q": {maxErrSq: new(big.Rat)},
	}
	for _, tc := range buildCorpus() {
		// NOTE: near-B (boolOnly) is INCLUDED here — the exclusion
		// applies only to CandFaces face-domain metrics.
		oa, ob := rectsToOraclePolySet(tc.rects)
		oUnion, err := BooleanUnion(oa, ob)
		if err != nil {
			t.Fatalf("%s: oracle union: %v", tc.name, err)
		}
		for _, pol := range []CandPolicy{polX, polQ} {
			st := res[pol.Name]
			st.cases++
			ca, cb := candRectsHalved(tc.rects)
			out, err := CandBoolean(candMemUnion, pol, ca, cb)
			if err != nil {
				t.Logf("%s/%s: candidate err %v", tc.name, pol.Name, err)
				st.mismatches++
				continue
			}
			matched := len(out.outer) == len(oUnion.outer)
			holeMatched := matched
			merrSq := new(big.Rat)
			if matched {
				for fi := range oUnion.outer {
					or := oUnion.outer[fi]
					crg := out.outer[fi]
					if len(or) != len(crg) ||
						len(oUnion.holes[fi]) != len(out.holes[fi]) {
						t.Logf("%s/%s: face %d structure cand=(%d verts,%d holes) oracle=(%d verts,%d holes)",
							pol.Name, tc.name, fi, len(crg), len(out.holes[fi]),
							len(or), len(oUnion.holes[fi]))
						matched = false
						holeMatched = false
						break
					}
					for hi := range oUnion.holes[fi] {
						oh := oUnion.holes[fi][hi]
						ch := out.holes[fi][hi]
						if len(oh) != len(ch) {
							t.Logf("%s/%s: hole %d of %d vert counts differ",
								pol.Name, tc.name, fi, hi)
							holeMatched = false
							break
						}
						for vi := range oh {
							rx, okX := oh[vi].x.Rat()
							ry, okY := oh[vi].y.Rat()
							if !okX || !okY {
								continue
							}
							sq := cSqDistF(ch[vi], rx, ry)
							if sq.Cmp(merrSq) > 0 {
								merrSq = sq
							}
							st.errVerts++
						}
					}
					for vi := range or {
						rx, okX := or[vi].x.Rat()
						ry, okY := or[vi].y.Rat()
						if !okX || !okY {
							matched = false
							break
						}
						sq := cSqDistF(crg[vi], rx, ry)
						if sq.Cmp(merrSq) > 0 {
							merrSq = sq
						}
						st.errVerts++
					}
				}
			}
			if !matched || !holeMatched {
				t.Logf("%s/%s: MISMATCH", pol.Name, tc.name)
				st.mismatches++
				continue
			}
			if merrSq.Cmp(st.maxErrSq) > 0 {
				st.maxErrSq = merrSq
			}
			t.Logf("%s/%s: MATCH comps=%d maxSquaredDist=%s",
				pol.Name, tc.name, len(oUnion.outer), merrSq.RatString())
		}
	}
	for _, name := range []string{"B-1x", "B-1q"} {
		st := res[name]
		rate := 0.0
		if st.cases > 0 {
			rate = float64(st.mismatches) / float64(st.cases)
		}
		t.Logf("%s: cases=%d topologyMismatches=%d mismatchRate=%.3f matchedVertices=%d maxSquaredEuclideanDist=%s",
			name, st.cases, st.mismatches, rate, st.errVerts,
			st.maxErrSq.RatString())
	}
}

// cSqDistF returns the exact squared distance between a committed
// float vertex and an exact rational oracle vertex.
func cSqDistF(p [2]float64, ex, ey *big.Rat) *big.Rat {
	cx := new(big.Rat)
	cx.SetFloat64(p[0])
	cy := new(big.Rat)
	cy.SetFloat64(p[1])
	dx := new(big.Rat).Sub(cx, ex)
	dy := new(big.Rat).Sub(cy, ey)
	return new(big.Rat).Add(new(big.Rat).Mul(dx, dx), new(big.Rat).Mul(dy, dy))
}

// TestCorpusNearBBoolean runs the NAMED near-B corpus case: union of
// two wall bands translated flush against the proposed bound
// B = 10^9 mm, through both candidate policies plus the exact oracle;
// grid overflow rejects deterministically under B-1q policy if any
// conversion exceeds the pinned unit range.
func TestCorpusNearBBoolean(t *testing.T) {
	const B = 1e9
	a := cps(cr(int64(B-40), int64(B-40), int64(B-30), int64(B)))
	bb := cps(cr(int64(B-30), int64(B-30), int64(B-20), int64(B)))
	for _, pol := range []CandPolicy{polX, polQ} {
		u, err := CandBoolean(candMemUnion, pol, a, bb)
		if err != nil {
			t.Fatalf("%s: near-B union: %v", pol.Name, err)
		}
		if len(u.outer) != 1 {
			t.Fatalf("%s: near-B union must merge into one component", pol.Name)
		}
		d, err := CandBoolean(candMemDiff, pol, u, a)
		if err != nil {
			t.Fatal(err)
		}
		// (A∪B) − A leaves exactly B.
		if len(d.outer) != 1 {
			t.Fatalf("%s: (A∪B)-A must leave exactly B, got %d components",
				pol.Name, len(d.outer))
		}
	}
	oa := polySet{outer: []oRing{rectRing(int64(B-40), int64(B-40), int64(B-30), int64(B))}, holes: make([][]oRing, 1)}
	ob := polySet{outer: []oRing{rectRing(int64(B-30), int64(B-30), int64(B-20), int64(B))}, holes: make([][]oRing, 1)}
	oOut, oerr := BooleanUnion(oa, ob)
	if oerr != nil || len(oOut.outer) != 1 {
		t.Fatalf("oracle near-B union: %v", oerr)
	}
}

// TestQuantizationProbeNonGrid is the named non-grid quantization
// probe (AC1): rectangles with stored fractional coordinates (0.03 /
// 2.03 class) run through B-1x and B-1q union; the EXACT squared
// Euclidean deviation (big.Rat) of committed output vertices from the
// BooleanUnion exact result is logged as RatString per policy with a
// separate topology-mismatch count. No threshold; 0030 judges.
func TestQuantizationProbeNonGrid(t *testing.T) {
	a := cps(candRing{{0.03, 0.03}, {2.03, 0.03}, {2.03, 2.03}, {0.03, 2.03}})
	b := cps(candRing{{1.17, 1.17}, {3.11, 1.17}, {3.11, 3.11}, {1.17, 3.11}})
	// Oracle operands use the SAME stored binary64 coordinates,
	// converted to their exact dyadic rationals.
	mk := func(r candRing) oRing {
		out := make(oRing, len(r))
		for i, v := range r {
			x, err := XFloat(v[0])
			if err != nil {
				t.Fatal(err)
			}
			y, err := XFloat(v[1])
			if err != nil {
				t.Fatal(err)
			}
			out[i] = oPt{x: x, y: y}
		}
		return out
	}
	oa := polySet{outer: []oRing{mk(a.outer[0])}, holes: make([][]oRing, 1)}
	ob := polySet{outer: []oRing{mk(b.outer[0])}, holes: make([][]oRing, 1)}
	oUnion, err := BooleanUnion(oa, ob)
	if err != nil {
		t.Fatal(err)
	}
	for _, pol := range []CandPolicy{polX, polQ} {
		out, cerr := CandBoolean(candMemUnion, pol, a, b)
		if cerr != nil {
			t.Logf("%s: candidate error %v", pol.Name, cerr)
			continue
		}
		if len(out.outer) != len(oUnion.outer) ||
			len(out.outer[0]) != len(oUnion.outer[0]) {
			t.Logf("%s: TOPOLOGY MISMATCH comps=%d/%d",
				pol.Name, len(out.outer), len(oUnion.outer))
			continue
		}
		maxSq := new(big.Rat)
		or := oUnion.outer[0]
		crg := out.outer[0]
		for vi := range or {
			rx, okX := or[vi].x.Rat()
			ry, okY := or[vi].y.Rat()
			if !okX || !okY {
				continue
			}
			sq := cSqDistF(crg[vi], rx, ry)
			if sq.Cmp(maxSq) > 0 {
				maxSq = sq
			}
		}
		t.Logf("%s: matched ring, measured max squared Euclidean deviation %s (mm^2)",
			pol.Name, maxSq.RatString())
	}
}

// ---- Q(sqrt(d)) end-to-end oracle coverage ----------------------------

// qmap maps an integer (s,w) parameter point through the 45-degree
// rotation-with-half-scaling c(s,w)=((s-w)*sqrt2/2,(s+w)*sqrt2/2),
// producing genuine sqrt(2)-bearing oracle coordinates (odd s-w or
// s+w yield non-dyadic rational coefficients on sqrt(2)).
func qmap(sv, wv int64) oPt {
	return oPt{
		x: XQuad(new(big.Rat).SetInt64(0), new(big.Rat).SetFrac64(sv-wv, 2), 2),
		y: XQuad(new(big.Rat).SetInt64(0), new(big.Rat).SetFrac64(sv+wv, 2), 2),
	}
}

// diagBandSegs converts axis-aligned wall-band rectangles given in
// integer (s,w) parameter space into mapped surd boundary segments.
// bandRectsI is the integer-parameter variant of corner-butted bands.
func bandRectsI(x0, y0, x1, y1, t int64) [][4]int64 {
	return [][4]int64{
		{x0, y0, x1, y0 + t},
		{x0, y1 - t, x1, y1},
		{x0, y0 + t, x0 + t, y1 - t},
		{x1 - t, y0 + t, x1, y1 - t},
	}
}

func diagBandSegs(bands [][4]int64) []oSeg {
	var out []oSeg
	for _, r := range bands {
		corners := [][2]int64{{r[0], r[1]}, {r[2], r[1]}, {r[2], r[3]}, {r[0], r[3]}}
		for i := range corners {
			j := (i + 1) % len(corners)
			out = append(out, oSeg{
				a: qmap(corners[i][0], corners[i][1]),
				b: qmap(corners[j][0], corners[j][1]),
			})
		}
	}
	return out
}

func TestFieldRejectsNonSquarefree(t *testing.T) {
	if _, err := Field(4); err == nil {
		t.Fatal("non-squarefree field must reject")
	}
	if _, err := Field(1); err == nil {
		t.Fatal("degenerate field must reject")
	}
	if _, err := Field(2); err != nil {
		t.Fatalf("valid field rejected: %v", err)
	}
}

// TestOracleSurd45DegreeLJoin: an L of two diagonal wall bands (all
// corners genuine Q(sqrt(2)) values) encloses no free space; the
// oracle must report zero bounded faces and exactly one exterior loop.
func TestOracleSurd45DegreeLJoin(t *testing.T) {
	segs := diagBandSegs([][4]int64{
		{-8, -1, 2, 1}, // horizontal arm in (s,w) space
		{0, -1, 2, 3},  // vertical arm overlapping the elbow
	})
	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatalf("surd arrangement failed: %v", err)
	}
	if len(arr.Faces) != 0 {
		t.Fatalf("L-join encloses nothing: %d faces", len(arr.Faces))
	}
	if len(arr.Exterior.Holes) != 1 {
		t.Fatalf("single connected L must give one exterior loop, got %d",
			len(arr.Exterior.Holes))
	}
	if MixedFieldEvents() != 0 {
		t.Fatalf("unexpected mixed-field events: %d", MixedFieldEvents())
	}
}

// TestOracleSurd45DegreePointPinch is the agreed Q(sqrt(2)) pinch
// probe: two diagonal wall loops with centerline squares (0,0)-(8,8)
// and (8,8)-(16,16) in rotated integer parameters (s,w), thickness 2,
// mapped by c(s,w)=((s-w)/2,(s+w)/2)*sqrt2; the inner corners coincide
// at exactly one occupied mapped point. Expected topology: exactly TWO bounded lobes
// (one per loop interior), no holes, exterior carrying both obstacle
// loops; all arithmetic inside Field(2) with zero mixed-field events.
func TestOracleSurd45DegreePointPinch(t *testing.T) {
	// Loop A: centerline square (0,0)-(8,8), thickness 2 -> interior
	// (1,7)x(1,7) in parameter space. Loop B: centerline square
	// (8,8)-(16,16), same thickness; the two inner corners coincide at
	// parameter point (8,8) — a single occupied shared point.
	loopA := bandRectsI(0, 0, 8, 8, 2)
	loopB := bandRectsI(8, 8, 16, 16, 2)
	segs := diagBandSegs(append(loopA, loopB...))
	arr, err := OracleFaces(segs)
	if err != nil {
		t.Fatalf("surd pinch arrangement failed: %v", err)
	}
	if MixedFieldEvents() != 0 {
		t.Fatalf("unexpected mixed-field events: %d", MixedFieldEvents())
	}
	if len(arr.Exterior.Holes) < 1 {
		t.Fatal("pinch must bound obstacle loops")
	}
	if len(arr.Faces) != 2 {
		t.Fatalf("surd pinch must split into exactly 2 lobes, got %d", len(arr.Faces))
	}
	for i, f := range arr.Faces {
		if f.Parent != -1 || len(f.Holes) != 0 {
			t.Fatalf("lobe %d structure wrong", i)
		}
	}
}

// ---- F12/F13/F19 regression batch --------------------------------------

func ringsEqExact(a, b [][2]float64) bool { return ringsEqual(a, b) }

// TestParentMutationProbe ties the nesting assertion to an independent
// expected value and proves a corrupted Parent is DETECTED by the same
// comparison used in the harness (mutation obligation).
func TestParentMutationProbe(t *testing.T) {
	segs := rectsToSegs(concat(bandRects(0, 0, 30, 30, 2), islandBands()))
	arr, err := CandFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 {
		t.Fatal("courtyard fixture changed")
	}
	// Independent expected fixture (literal, not oracle-derived).
	expected := []cFace{
		{Outer: [][2]float64{{2, 2}, {28, 2}, {28, 28}, {2, 28}},
			Holes:  [][][2]float64{{{10, 10}, {10, 18}, {20, 18}, {20, 10}}},
			Parent: -1},
		{Outer: [][2]float64{{12, 12}, {18, 12}, {18, 16}, {12, 16}},
			Holes:  nil,
			Parent: 0},
	}
	faceEq := func(got cFace, want cFace) bool {
		if !ringsEqual(got.Outer, want.Outer) ||
			len(got.Holes) != len(want.Holes) ||
			got.Parent != want.Parent {
			return false
		}
		for hi := range want.Holes {
			if !ringsEqual(got.Holes[hi], want.Holes[hi]) {
				return false // exact hole ring contents
			}
		}
		return true
	}
	if !faceEq(arr.Faces[0], expected[0]) || !faceEq(arr.Faces[1], expected[1]) {
		t.Fatal("candidate deviates from independent expected fixture")
	}
	// Mutation: corrupt the EXPECTED copy's Parent and hole structure;
	// the same equality helper must reject both mutations.
	mut := expected[1]
	mut.Parent = -1
	if faceEq(arr.Faces[1], mut) {
		t.Fatal("corrupted Parent not detected")
	}
	// Corrupted hole structure in the expected copy must also be
	// detected by the same helper: drop the courtyard's island hole
	// expectation and compare against the candidate (which HAS it).
	mutNoHole := expected[0]
	mutNoHole.Holes = nil
	if faceEq(arr.Faces[0], mutNoHole) {
		t.Fatal("hole-structure mutation not detected")
	}
	// Corrupted Parent on the courtyard expectation likewise.
	mutParent := expected[0]
	mutParent.Parent = 1
	if faceEq(arr.Faces[0], mutParent) {
		t.Fatal("courtyard parent mutation not detected")
	}
}

// TestBooleanCanonicalInvariances asserts IDENTICAL canonical Boolean
// output under segment reversal and operand-order swap, for BOTH
// policies, comparing complete canonical polysets (outer + holes).
func TestBooleanCanonicalInvariances(t *testing.T) {
	rects := [][4]float64{{0, 8, 20, 12}, {9, 0, 11, 20}}
	ca, cb := candRectsHalved(rects)
	// Holed operand: annulus in A so hole canonicalization is covered.
	holeA := candRing{{3, 11}, {17, 11}, {17, 9}, {3, 9}} // CW = real hole
	ca.holes[len(ca.outer)-1] = []candRing{holeA}
	for _, pol := range []CandPolicy{polX, polQ} {
		ref, err := CandBoolean(candMemUnion, pol, ca, cb)
		if err != nil {
			t.Fatal(err)
		}
		// Reverse every input segment.
		ra := candPolySet{holes: make([][]candRing, len(ca.outer))}
		rb := candPolySet{holes: make([][]candRing, len(cb.outer))}
		for i, r := range ca.outer {
			rr := make(candRing, len(r))
			for j := range r {
				rr[len(r)-1-j] = r[j]
			}
			ra.outer = append(ra.outer, rr)
			for _, h := range ca.holes[i] {
				rh := make(candRing, len(h))
				for j := range h {
					rh[len(h)-1-j] = h[j]
				}
				ra.holes[i] = append(ra.holes[i], rh)
			}
		}
		for i, r := range cb.outer {
			rr := make(candRing, len(r))
			for j := range r {
				rr[len(r)-1-j] = r[j]
			}
			rb.outer = append(rb.outer, rr)
			for _, h := range cb.holes[i] {
				rh := make(candRing, len(h))
				for j := range h {
					rh[len(h)-1-j] = h[j]
				}
				rb.holes[i] = append(rb.holes[i], rh)
			}
		}
		outRev, err := CandBoolean(candMemUnion, pol, ra, rb)
		if err != nil {
			t.Fatal(err)
		}
		outSwap, err := CandBoolean(candMemUnion, pol, cb, ca)
		if err != nil {
			t.Fatal(err)
		}
		cmp := func(x, y candPolySet) bool {
			if len(x.outer) != len(y.outer) {
				return false
			}
			for fi := range x.outer {
				if !ringsEqual(x.outer[fi], y.outer[fi]) {
					return false // exact canonical equality required
				}
				if len(x.holes[fi]) != len(y.holes[fi]) {
					return false
				}
				for hi := range x.holes[fi] {
					if !ringsEqual(x.holes[fi][hi], y.holes[fi][hi]) {
						return false // exact canonical hole equality
					}
				}
			}
			return true
		}
		if !cmp(ref, outRev) || !cmp(ref, outSwap) {
			t.Fatalf("%s: canonical output depends on direction/order", pol.Name)
		}
	}
}

// TestHostReversalOpeningTransform asserts R4.3.1/R4.2.3: reversing a
// host (u -> 1-u, side flip) leaves the cut geometry's canonical
// difference output unchanged.
func TestHostReversalOpeningTransform(t *testing.T) {
	// Host wall (0..24)x(0..2); partial-depth window near the start vs
	// the R4.3.1-mirrored window near the end (u -> 1-u, side flip).
	// The two cut rectangles are physically mirrored placements; the
	// resulting amended regions are mirror images about x=12 and must
	// be canonically identical after mirroring back.
	for _, pol := range []CandPolicy{polX, polQ} {
		wall := cps(cr(0, 0, 24, 2))
		winNearStart := cps(cr(4, 0, 7, 1)) // side Left, u small
		winNearEnd := cps(cr(17, 0, 20, 1)) // transformed placement
		l, err := CandBoolean(candMemDiff, pol, wall, winNearStart)
		if err != nil {
			t.Fatal(err)
		}
		r, err := CandBoolean(candMemDiff, pol, wall, winNearEnd)
		if err != nil {
			t.Fatal(err)
		}
		if len(l.outer) != len(r.outer) || len(l.outer) != 1 {
			t.Fatalf("%s: notch structure changed", pol.Name)
		}
		// Mirror r's ring about x=12 and require exact equality.
		mirrored := make(candRing, len(r.outer[0]))
		for i, v := range r.outer[0] {
			mirrored[i] = [2]float64{24 - v[0], v[1]}
		}
		// Reorient mirrored ring (mirroring flips chirality).
		if candShoelaceSignF(mirrored) < 0 {
			rr := make(candRing, len(mirrored))
			for j := range mirrored {
				rr[len(mirrored)-1-j] = mirrored[j]
			}
			mirrored = rr
		}
		// Canonical least-vertex rotation before exact comparison.
		least := 0
		for j := 1; j < len(mirrored); j++ {
			if mirrored[j][0] < mirrored[least][0] ||
				(mirrored[j][0] == mirrored[least][0] && mirrored[j][1] < mirrored[least][1]) {
				least = j
			}
		}
		rot := append(append(candRing{}, mirrored[least:]...), mirrored[:least]...)
		if !ringsEqual(l.outer[0], rot) {
			t.Fatalf("%s: mirrored cut differs from original notch", pol.Name)
		}
	}
}

// candShoelaceSignF is the float sign helper for test-side mirrors.
func candShoelaceSignF(r candRing) int {
	sum := 0.0
	for i := 0; i < len(r); i++ {
		a := r[i]
		b := r[(i+1)%len(r)]
		sum += a[0]*b[1] - b[0]*a[1]
	}
	if sum > 0 {
		return 1
	}
	if sum < 0 {
		return -1
	}
	return 0
}
