package geometry

import (
	"math/big"
)

// Candidate sweep boolean: issue-0007 candidate strategy B-1.
//
// One candidate codebase, two policies (disclosed in the evidence
// document):
//
//	B-1x: exact rational event decisions over stored-float-derived
//	      constructions; each OUTPUT vertex commits to binary64 exactly
//	      once at pipeline end. No intermediate commitment feeds a
//	      later decision, so the 1/2-ulp claim scopes to that single
//	      terminal commitment.
//	B-1q: deliberately NONCONFORMING quantization baseline. Inputs
//	      round to a pinned grid before evaluation and every event
//	      ordinate rounds to the grid as produced. Grid ordinates are
//	      k/64 mm — dyadic, hence exactly representable in binary64.
//	      It exists ONLY to measure AC1 coordinate error / topology
//	      risk / overflow behavior; it does NOT satisfy
//	      NUMERICAL-MODEL section 7 runtime certification and is not
//	      eligible as the selected strategy in this issue.
//
// Decision mechanism: x-column sweep. Event ordinates split the plane
// into open columns; inside a column, spanning fragments sort by their
// ordinate at the column representative and SIGNED WINDING accumulators
// propagate neighbor-to-neighbor bottom-up (each boundary contributes
// its orientation delta; membership is winding != 0), rather than being
// re-derived by point probes. Vertical boundaries on
// cut lines are recovered by comparing the member states of the cells
// adjacent across the line. This mechanism differs from the
// independent oracle's direct sided probes and free-interval merging;
// primitive big.Rat arithmetic is the only common surface.
//
// Provenance: standard computational-geometry sweep-line literature
// (de Berg et al. 2008 ch.2; Martínez et al. 2009 — citation only,
// artifact unconsulted); independently implemented for Domowik. See
// evidence document ledger rows.

// CandPolicy selects a candidate backend policy.
type CandPolicy struct {
	Name     string
	Quantize bool // B-1q pinned-grid rounding + range gate
}

// Pinned B-1q grid constants (AC1): pitch 2^-6 mm (scale 64 units/mm);
// |v| <= B = 10^9 mm maps to <= 6.4e10 grid units; overflow or
// out-of-range input rejects deterministically; silent rescaling is
// prohibited.
const (
	candGridScale    = int64(64)
	candGridMaxUnits = int64(64_000_000_000)
)

var (
	candQ     = new(big.Rat).SetInt64(candGridScale)
	candUnitR = new(big.Rat).SetInt64(1)
)

func ratFromFloat(f float64) (*big.Rat, error) {
	if err := ValidateCoord(f); err != nil {
		return nil, err
	}
	r := new(big.Rat)
	r.SetFloat64(f)
	return r, nil
}

// quantizeGrid rounds v (mm) to the pinned grid returning grid units.
// Ties resolve to even per the documented tie rule.
func quantizeGrid(v *big.Rat) (*big.Int, error) {
	scaled := new(big.Rat).Mul(v, candQ)
	num := scaled.Num()
	den := scaled.Denom()
	q, rem := new(big.Int).QuoRem(num, den, new(big.Int))
	if rem.Sign() != 0 {
		abs := new(big.Int).Abs(rem)
		twice := new(big.Int).Lsh(abs, 1)
		if twice.Cmp(den) > 0 || (twice.Cmp(den) == 0 && q.Bit(0) == 1) {
			if num.Sign() > 0 {
				q.Add(q, big.NewInt(1))
			} else {
				q.Sub(q, big.NewInt(1))
			}
		}
	}
	if !q.IsInt64() || q.Int64() > candGridMaxUnits || q.Int64() < -candGridMaxUnits {
		return nil, &SpikeError{"grid-range-overflow"}
	}
	return q, nil
}

type candPt struct{ x, y *big.Rat }

// toCand converts a stored-float pair into the policy's working domain.
// Fresh receivers are used throughout; inputs are never mutated.
func toCand(pol CandPolicy, fx, fy float64) (candPt, error) {
	x, err := ratFromFloat(fx)
	if err != nil {
		return candPt{}, err
	}
	y, err := ratFromFloat(fy)
	if err != nil {
		return candPt{}, err
	}
	if pol.Quantize {
		gx, err := quantizeGrid(x)
		if err != nil {
			return candPt{}, err
		}
		gy, err := quantizeGrid(y)
		if err != nil {
			return candPt{}, err
		}
		x = gridUnitsToMM(gx)
		y = gridUnitsToMM(gy)
	}
	return candPt{x: x, y: y}, nil
}

// gridUnitsToMM converts pinned-grid integer units to exact millimetres
// (dyadic: k/64). Grid ordinates are therefore exactly representable in
// binary64 at commitment time.
func gridUnitsToMM(g *big.Int) *big.Rat {
	r := new(big.Rat).SetInt(g)
	return r.Quo(r, candQ)
}

// roundMMToGrid rounds a millimetre ordinate to the pinned grid,
// returning millimetres again (ties-to-even). Deterministic range
// overflow rejects.
func roundMMToGrid(v *big.Rat) (*big.Rat, error) {
	g, err := quantizeGrid(v)
	if err != nil {
		return nil, err
	}
	return gridUnitsToMM(g), nil
}

// candRing is a float-domain ring: stored binary64 vertex pairs.
type candRing [][2]float64

// candPolySet mirrors polySet in the float domain.
type candPolySet struct {
	outer []candRing
	holes [][]candRing
}

type candSeg [2]candPt

func candSegsOf(pol CandPolicy, s candPolySet) ([]candSeg, error) {
	var out []candSeg
	add := func(r candRing) error {
		for i := range r {
			j := (i + 1) % len(r)
			a, err := toCand(pol, r[i][0], r[i][1])
			if err != nil {
				return err
			}
			b, err := toCand(pol, r[j][0], r[j][1])
			if err != nil {
				return err
			}
			out = append(out, candSeg{a, b})
		}
		return nil
	}
	for _, r := range s.outer {
		if err := add(r); err != nil {
			return nil, err
		}
	}
	for _, hs := range s.holes {
		for _, r := range hs {
			if err := add(r); err != nil {
				return nil, err
			}
		}
	}
	return out, nil
}

func candPointAt(s candSeg, t *big.Rat) candPt {
	dx := new(big.Rat).Sub(s[1].x, s[0].x)
	dy := new(big.Rat).Sub(s[1].y, s[0].y)
	return candPt{
		x: new(big.Rat).Add(s[0].x, new(big.Rat).Mul(dx, t)),
		y: new(big.Rat).Add(s[0].y, new(big.Rat).Mul(dy, t)),
	}
}

func candInUnit(t *big.Rat) bool {
	return t.Sign() >= 0 && t.Cmp(candUnitR) <= 0
}

func candBetween(s candSeg, p candPt) bool {
	cross := new(big.Rat).Sub(
		new(big.Rat).Mul(new(big.Rat).Sub(s[1].x, s[0].x), new(big.Rat).Sub(p.y, s[0].y)),
		new(big.Rat).Mul(new(big.Rat).Sub(s[1].y, s[0].y), new(big.Rat).Sub(p.x, s[0].x)))
	if cross.Sign() != 0 {
		return false
	}
	return p.x.Cmp(minRat(s[0].x, s[1].x)) >= 0 && p.x.Cmp(maxRat(s[0].x, s[1].x)) <= 0 &&
		p.y.Cmp(minRat(s[0].y, s[1].y)) >= 0 && p.y.Cmp(maxRat(s[0].y, s[1].y)) <= 0
}

func minRat(a, b *big.Rat) *big.Rat {
	if b.Cmp(a) < 0 {
		return b
	}
	return a
}

func maxRat(a, b *big.Rat) *big.Rat {
	if b.Cmp(a) > 0 {
		return b
	}
	return a
}

func candIntersectParams(s, o candSeg) []*big.Rat {
	dx1 := new(big.Rat).Sub(s[1].x, s[0].x)
	dy1 := new(big.Rat).Sub(s[1].y, s[0].y)
	dx2 := new(big.Rat).Sub(o[1].x, o[0].x)
	dy2 := new(big.Rat).Sub(o[1].y, o[0].y)
	den := new(big.Rat).Sub(new(big.Rat).Mul(dx1, dy2), new(big.Rat).Mul(dy1, dx2))
	if den.Sign() != 0 {
		rx := new(big.Rat).Sub(o[0].x, s[0].x)
		ry := new(big.Rat).Sub(o[0].y, s[0].y)
		numT := new(big.Rat).Mul(rx, dy2)
		numT.Sub(numT, new(big.Rat).Mul(ry, dx2))
		numU := new(big.Rat).Mul(rx, dy1)
		numU.Sub(numU, new(big.Rat).Mul(ry, dx1))
		t := numT.Quo(numT, den)
		u := numU.Quo(numU, den)
		if candInUnit(t) && candInUnit(u) {
			return []*big.Rat{t}
		}
		return nil
	}
	cr := new(big.Rat).Sub(
		new(big.Rat).Mul(dx1, new(big.Rat).Sub(o[0].y, s[0].y)),
		new(big.Rat).Mul(dy1, new(big.Rat).Sub(o[0].x, s[0].x)))
	if cr.Sign() != 0 {
		return nil
	}
	var out []*big.Rat
	for _, p := range []candPt{o[0], o[1]} {
		var t *big.Rat
		switch {
		case dx1.Sign() != 0:
			t = new(big.Rat).Quo(new(big.Rat).Sub(p.x, s[0].x), dx1)
		case dy1.Sign() != 0:
			t = new(big.Rat).Quo(new(big.Rat).Sub(p.y, s[0].y), dy1)
		default:
			continue
		}
		if candInUnit(t) && candBetween(s, p) {
			out = append(out, t)
		}
	}
	return out
}

func candSortDedupe(ts []*big.Rat) []*big.Rat {
	out := make([]*big.Rat, 0, len(ts))
	for _, t := range ts {
		if t.Sign() <= 0 || t.Cmp(candUnitR) >= 0 {
			continue
		}
		dup := false
		for _, u := range out {
			if t.Cmp(u) == 0 {
				dup = true
				break
			}
		}
		if !dup {
			out = append(out, t)
		}
	}
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j].Cmp(out[j-1]) < 0; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

// Candidate closed-set membership semantics (normative set definitions,
// not decision logic): union keeps any operand interior/boundary;
// intersection requires both; difference removes only B's strict
// interior (R4.2.2 open-interior subtraction).
type candMem func(inA, inB bool) bool

func candMemUnion(a, b bool) bool { return a || b }
func candMemInter(a, b bool) bool { return a && b }
func candMemDiff(a, b bool) bool  { return a && !b }

type candFrag struct {
	s   candSeg
	opA bool
	inA bool // midpoint inside operand A (candidate winding)
	inB bool
}

func candMidpoint(s candSeg) candPt {
	half := new(big.Rat).SetInt64(1)
	half.Quo(half, new(big.Rat).SetInt64(2))
	return candPt{
		x: new(big.Rat).Add(s[0].x, new(big.Rat).Mul(new(big.Rat).Sub(s[1].x, s[0].x), half)),
		y: new(big.Rat).Add(s[0].y, new(big.Rat).Mul(new(big.Rat).Sub(s[1].y, s[0].y), half)),
	}
}

// candWinding returns the nonzero winding number of m against segs.
func candWinding(m candPt, segs []candSeg) int {
	w := 0
	for _, s := range segs {
		aAbove := s[0].y.Cmp(m.y) > 0
		bAbove := s[1].y.Cmp(m.y) > 0
		if aAbove == bAbove {
			continue
		}
		t := new(big.Rat).Quo(
			new(big.Rat).Sub(m.y, s[0].y),
			new(big.Rat).Sub(s[1].y, s[0].y))
		xi := new(big.Rat).Add(s[0].x,
			new(big.Rat).Mul(new(big.Rat).Sub(s[1].x, s[0].x), t))
		if xi.Cmp(m.x) > 0 {
			if bAbove && !aAbove {
				w++
			} else {
				w--
			}
		}
	}
	return w
}

// candSubdivide splits segments at every mutual meeting point;
// fragment direction inherits parent ring orientation.
// candSubdivide splits segments at every mutual meeting point.
// Under the quantize policy every NEW meeting coordinate is rounded to
// the pinned grid once at creation, so all downstream decisions see
// rounded values only (input vertices were already rounded in toCand).
func candSubdivide(pol CandPolicy, a, b []candSeg) ([]candFrag, error) {
	var subErr error
	roundPt := func(p candPt) candPt {
		if !pol.Quantize || subErr != nil {
			return p
		}
		gx, err := roundMMToGrid(p.x)
		if err != nil {
			subErr = err
			return p
		}
		gy, err := roundMMToGrid(p.y)
		if err != nil {
			subErr = err
			return p
		}
		return candPt{x: gx, y: gy}
	}
	all := make([]candSeg, 0, len(a)+len(b))
	all = append(all, a...)
	all = append(all, b...)

	type work struct {
		s      candSeg
		opA    bool
		params []*big.Rat
	}
	ws := make([]work, len(all))
	for i, s := range all {
		ws[i] = work{s: s, opA: i < len(a)}
	}
	for i := range ws {
		for j := range all {
			if i == j {
				continue
			}
			ws[i].params = append(ws[i].params, candIntersectParams(ws[i].s, all[j])...)
		}
	}
	out := make([]candFrag, 0, len(all))
	for i := range ws {
		ps := candSortDedupe(ws[i].params)
		cur := ws[i].s
		for _, t := range ps {
			p := roundPt(candPointAt(ws[i].s, t))
			// Grid rounding can collapse adjacent meeting points onto
			// one grid node: skip degenerate pieces instead of emitting
			// zero-length fragments.
			if p.x.Cmp(cur[0].x) == 0 && p.y.Cmp(cur[0].y) == 0 {
				continue
			}
			if p.x.Cmp(cur[1].x) == 0 && p.y.Cmp(cur[1].y) == 0 {
				break // remaining splits collapsed onto the endpoint
			}
			out = append(out, candFrag{s: candSeg{cur[0], p}, opA: ws[i].opA})
			cur = candSeg{p, cur[1]}
		}
		if cur[0].x.Cmp(cur[1].x) != 0 || cur[0].y.Cmp(cur[1].y) != 0 {
			out = append(out, candFrag{s: cur, opA: ws[i].opA})
		}
	}
	if subErr != nil {
		return nil, subErr
	}
	return out, nil
}

// candEventY computes the fragment ordinate at abscissa x under the
// policy; quantize mode rounds to the pinned grid immediately so all
// downstream comparisons see grid values only. Fresh receivers only.
func candEventY(f candSeg, x *big.Rat, pol CandPolicy) (*big.Rat, error) {
	dx := new(big.Rat).Sub(f[1].x, f[0].x)
	dy := new(big.Rat).Sub(f[1].y, f[0].y)
	t := new(big.Rat).Quo(new(big.Rat).Sub(x, f[0].x), dx)
	y := new(big.Rat).Add(f[0].y, new(big.Rat).Mul(dy, t))
	if pol.Quantize {
		return roundMMToGrid(y)
	}
	return y, nil
}

type candPiece struct {
	s    candSeg
	from bool // from the left column at a cut line (vertical pieces)
}

// candChain assembles directed pieces into closed half-edge orbits.
//
// Convention contract: pieces are oriented with the OCCUPIED side on
// their LEFT, so the free face lies on the RIGHT of every piece. The
// successor at a head node is therefore the outgoing piece with the
// greatest exact CCW angle measured from the FORWARD incoming
// direction - equivalently, the first piece met rotating clockwise
// from it - which turns right to keep the same free face connected
// through the corner. (The classic first-clockwise-from-REVERSED rule
// applies to twin-edge face-on-left graphs; this graph carries one
// direction per boundary piece, where the forward reference is the
// correct dual.) The successor is computed ONCE over the full local
// outgoing multiset - a fixed geometric corner pairing, never a
// function of traversal state - so orbits do not depend on start
// order. At a closed point-contact the fixed pairing intentionally
// crosses the pinch, producing ONE repeated-node walk; candCyclesToSet
// then restores simple rings via candSplitSimpleWalk and validates the
// simple-ring postcondition before terminal commitment. An orbit
// closes only when the fixed successor maps back to its starting
// piece; a piece whose node has no outgoing continuation, or any
// non-bijective/non-cyclic structure, fails closed. The convention and
// the cross-pinch walk/split behavior are pinned by literal fixtures in
// boolperm_test.go (TestCandChainPinchBranchPinning,
// TestBooleanPinchSemantics).
func candChain(pieces []candSeg) ([][]candPt, error) {
	at := map[string][]int{}
	key := func(p candPt) string { return p.x.RatString() + "," + p.y.RatString() }
	for i, s := range pieces {
		at[key(s[0])] = append(at[key(s[0])], i)
	}
	succ := make([]int, len(pieces))
	for i := range pieces {
		head := pieces[i][1]
		inx := new(big.Rat).Sub(pieces[i][1].x, pieces[i][0].x)
		iny := new(big.Rat).Sub(pieces[i][1].y, pieces[i][0].y)
		best := -1
		var bdx, bdy *big.Rat
		for _, cand := range at[key(head)] {
			cdx := new(big.Rat).Sub(pieces[cand][1].x, head.x)
			cdy := new(big.Rat).Sub(pieces[cand][1].y, head.y)
			if best < 0 {
				best, bdx, bdy = cand, cdx, cdy
				continue
			}
			if candAngAfter(inx, iny, cdx, cdy, bdx, bdy) {
				best, bdx, bdy = cand, cdx, cdy
			}
		}
		if best < 0 {
			return nil, &SpikeError{"open-boundary-walk"}
		}
		succ[i] = best
	}
	// The successor map must be a permutation of the pieces: every
	// piece is the successor of exactly one predecessor. A
	// non-bijective map would let an orbit merge into another and loop
	// forever; that is a structural defect and fails closed.
	count := make([]int, len(pieces))
	for _, s := range succ {
		count[s]++
		if count[s] > 1 {
			return nil, &SpikeError{"open-boundary-walk"}
		}
	}
	used := make([]bool, len(pieces))
	var cycles [][]candPt
	for start := range pieces {
		if used[start] {
			continue
		}
		var cyc []candPt
		cur := start
		for {
			if used[cur] {
				return nil, &SpikeError{"open-boundary-walk"} // non-start repetition
			}
			used[cur] = true
			cyc = append(cyc, pieces[cur][0])
			cur = succ[cur]
			if cur == start {
				break
			}
		}
		cycles = append(cycles, cyc)
	}
	return cycles, nil
}

// candAngAfter reports whether direction (cx,cy) lies at a strictly
// greater CCW angle measured from reference (inx,iny) than (bx,by);
// exact ties keep the earlier candidate. Buckets: 0 parallel-same,
// 1 strict left, 2 opposite, 3 strict right; within left/right buckets
// cross(c,b)<0 means c has the greater angle. All decisions are exact
// big.Rat sign tests.
func candAngAfter(inx, iny, cx, cy, bx, by *big.Rat) bool {
	cb := candAngBucket(inx, iny, bx, by)
	cc := candAngBucket(inx, iny, cx, cy)
	if cc != cb {
		return cc > cb
	}
	cr := new(big.Rat).Sub(
		new(big.Rat).Mul(cx, by),
		new(big.Rat).Mul(cy, bx))
	return cr.Sign() < 0
}

func candAngBucket(inx, iny, vx, vy *big.Rat) int {
	cr := new(big.Rat).Sub(
		new(big.Rat).Mul(inx, vy),
		new(big.Rat).Mul(iny, vx))
	if cr.Sign() != 0 {
		if cr.Sign() > 0 {
			return 1
		}
		return 3
	}
	dt := new(big.Rat).Add(
		new(big.Rat).Mul(inx, vx),
		new(big.Rat).Mul(iny, vy))
	if dt.Sign() >= 0 {
		return 0
	}
	return 2
}

// candCanonClean removes duplicate and exactly-collinear vertices.
func candCanonClean(r []candPt) []candPt {
	var out []candPt
	for _, p := range r {
		if len(out) > 0 && p.x.Cmp(out[len(out)-1].x) == 0 && p.y.Cmp(out[len(out)-1].y) == 0 {
			continue
		}
		out = append(out, p)
	}
	for len(out) > 1 && out[0].x.Cmp(out[len(out)-1].x) == 0 && out[0].y.Cmp(out[len(out)-1].y) == 0 {
		out = out[:len(out)-1]
	}
	changed := true
	for changed && len(out) > 2 {
		changed = false
		var next []candPt
		for i := 0; i < len(out); i++ {
			pv := out[(i-1+len(out))%len(out)]
			nx := out[(i+1)%len(out)]
			c := new(big.Rat).Sub(
				new(big.Rat).Mul(new(big.Rat).Sub(nx.x, pv.x), new(big.Rat).Sub(out[i].y, pv.y)),
				new(big.Rat).Mul(new(big.Rat).Sub(nx.y, pv.y), new(big.Rat).Sub(out[i].x, pv.x)))
			if c.Sign() == 0 {
				changed = true
				continue
			}
			next = append(next, out[i])
		}
		out = next
	}
	return out
}

func candShoelaceSign(r []candPt) int {
	sum := new(big.Rat)
	for i := 0; i < len(r); i++ {
		a := r[i]
		b := r[(i+1)%len(r)]
		sum.Add(sum, new(big.Rat).Sub(
			new(big.Rat).Mul(a.x, b.y),
			new(big.Rat).Mul(b.x, a.y)))
	}
	return sum.Sign()
}

// CandBoolean runs the sweep boolean under the policy and returns the
// result committed to the float domain: B-1x performs its single
// terminal binary64 commitment here; B-1q grid ordinates are dyadic
// and convert exactly.
func candIsEmpty(s candPolySet) bool { return len(s.outer) == 0 }

// validateCandShape enforces the structural invariant holes[i] belongs
// to outer[i]: equal slice lengths and no nil/empty ring entries.
// Violations reject deterministically instead of panicking later.
func validateCandShape(s candPolySet) error {
	if len(s.holes) != len(s.outer) {
		return &SpikeError{"malformed-polyset-shape"}
	}
	for _, r := range s.outer {
		if len(r) < 3 {
			return &SpikeError{"degenerate-ring"}
		}
	}
	for _, hs := range s.holes {
		for _, r := range hs {
			if len(r) < 3 {
				return &SpikeError{"degenerate-ring"}
			}
		}
	}
	return nil
}

// CandBoolean runs the sweep boolean under the policy. Empty operands
// follow identity laws deterministically; rings are validated
// (finite, in-range, non-degenerate, simple) before any decision.
func CandBoolean(op candMem, pol CandPolicy, a, b candPolySet) (candPolySet, error) {
	for _, r := range append(append([]candRing{}, a.outer...), b.outer...) {
		if err := ValidateSimpleRing([][2]float64(r)); err != nil {
			return candPolySet{}, err
		}
	}
	for _, hs := range append(append([][]candRing{}, a.holes...), b.holes...) {
		for _, r := range hs {
			if err := ValidateSimpleRing([][2]float64(r)); err != nil {
				return candPolySet{}, err
			}
		}
	}
	if err := validateCandShape(a); err != nil {
		return candPolySet{}, err
	}
	if err := validateCandShape(b); err != nil {
		return candPolySet{}, err
	}
	switch {
	case candIsEmpty(a):
		if candIsEmpty(b) {
			return candPolySet{holes: [][]candRing{}}, nil
		}
		out, err := candCloneQuantized(pol, b)
		if err != nil {
			return candPolySet{}, err
		}
		candNormalizeSet(&out)
		return out, nil
	case candIsEmpty(b):
		out, err := candCloneQuantized(pol, a)
		if err != nil {
			return candPolySet{}, err
		}
		candNormalizeSet(&out)
		return out, nil
	}
	if op == nil {
		op = candMemUnion
	}

	segsA, err := candSegsOf(pol, a)
	if err != nil {
		return candPolySet{}, err
	}
	segsB, err := candSegsOf(pol, b)
	if err != nil {
		return candPolySet{}, err
	}
	frags, err := candSubdivide(pol, segsA, segsB)
	if err != nil {
		return candPolySet{}, err
	}
	for i := range frags {
		m := candMidpoint(frags[i].s)
		frags[i].inA = candWinding(m, segsA) != 0
		frags[i].inB = candWinding(m, segsB) != 0
	}

	xset := map[string]*big.Rat{}
	var xs []*big.Rat
	for _, f := range frags {
		for _, e := range []candPt{f.s[0], f.s[1]} {
			k := e.x.RatString()
			if _, ok := xset[k]; !ok {
				xset[k] = e.x
				xs = append(xs, e.x)
			}
		}
	}
	xs = candSortRats(xs)

	type spanT struct {
		fi    int
		y     *big.Rat
		delta int // signed winding contribution (+1/-1)
		opA   bool
	}
	var dirPieces []candSeg
	var sweepErr error
	addBoundary := func(fragIdx int, xl, xr *big.Rat, memberAbove bool) {
		f := frags[fragIdx].s
		ya, err := candEventY(f, xl, pol)
		if err != nil {
			sweepErr = err
			return
		}
		yb, err := candEventY(f, xr, pol)
		if err != nil {
			sweepErr = err
			return
		}
		pa := candPt{x: new(big.Rat).Set(xl), y: ya}
		pb := candPt{x: new(big.Rat).Set(xr), y: yb}
		dx := new(big.Rat).Sub(pb.x, pa.x)
		// Walking with dx>0 puts the above side on the left.
		if (dx.Sign() > 0) != memberAbove {
			pa, pb = pb, pa
		}
		dirPieces = append(dirPieces, candSeg{pa, pb})
	}
	addVertical := func(x *big.Rat, lo, hi *big.Rat, memberWest bool) {
		pa := candPt{x: new(big.Rat).Set(x), y: new(big.Rat).Set(lo)}
		pb := candPt{x: new(big.Rat).Set(x), y: new(big.Rat).Set(hi)}
		if !memberWest {
			pa, pb = pb, pa // walking down puts east on the left
		}
		dirPieces = append(dirPieces, candSeg{pa, pb})
	}

	// Column sweep with SIGNED WINDING propagation: each boundary
	// contributes its orientation delta; membership is winding != 0.
	// Equal-ordinate boundaries group into one event so coincident
	// operands and internal seams collapse deterministically.
	// Cells reference bounding fragments; ordinates evaluate exactly at
	// whichever cut line is crossed (midpoints only order the column).
	type cellT2 struct {
		loFi, hiFi int // -1 = unbounded side
		mem        bool
	}
	columnStates := make([][]cellT2, len(xs)+1)
	for c := 0; c+1 < len(xs) && sweepErr == nil; c++ {
		xl, xr := xs[c], xs[c+1]
		xm := new(big.Rat).Add(xl, xr)
		xm.Quo(xm, new(big.Rat).SetInt64(2))
		var spans []spanT
		for fi := range frags {
			f := frags[fi].s
			ax, bx := f[0].x, f[1].x
			if ax.Cmp(bx) > 0 {
				ax, bx = bx, ax
			}
			if sweepErr != nil {
				break
			}
			if ax.Cmp(xl) <= 0 && bx.Cmp(xr) >= 0 {
				fy, err := candEventY(frags[fi].s, xm, pol)
				if err != nil {
					sweepErr = err
					break
				}
				spans = append(spans, spanT{
					fi:    fi,
					y:     fy,
					delta: candWindingDelta(frags[fi].s),
					opA:   frags[fi].opA,
				})
			}
		}
		for i := 1; i < len(spans); i++ {
			for j := i; j > 0 && (spans[j].y.Cmp(spans[j-1].y) < 0 ||
				(spans[j].y.Cmp(spans[j-1].y) == 0 && spans[j].fi < spans[j-1].fi)); j-- {
				spans[j], spans[j-1] = spans[j-1], spans[j]
			}
		}
		wA, wB := 0, 0
		prevMem := false
		prevFi := -1
		g := 0
		for g < len(spans) {
			gy := spans[g].y
			gEnd := g
			for gEnd < len(spans) && spans[gEnd].y.Cmp(gy) == 0 {
				gEnd++
			}
			// State below this group's boundaries.
			memBelow := op(wA != 0, wB != 0)
			// Apply the whole group's winding deltas.
			for k := g; k < gEnd; k++ {
				if spans[k].opA {
					wA += spans[k].delta
				} else {
					wB += spans[k].delta
				}
			}
			hereMem := op(wA != 0, wB != 0)
			if prevMem != hereMem {
				addBoundary(spans[g].fi, xl, xr, hereMem)
			}
			columnStates[c] = append(columnStates[c], cellT2{
				loFi: prevFi,
				hiFi: spans[g].fi,
				mem:  memBelow,
			})
			prevMem = hereMem
			prevFi = spans[gEnd-1].fi
			g = gEnd
		}
		columnStates[c] = append(columnStates[c],
			cellT2{loFi: prevFi, hiFi: -1, mem: prevMem})
	}

	// Vertical boundaries across every cut line: walk the elementary
	// y-intervals formed by both adjacent columns' bounds and emit one
	// directed piece wherever membership differs. This avoids the
	// overlapping/duplicate pieces an all-pairs comparison produces.
	lineStates := func(col int) []cellT2 {
		if col < 0 || col >= len(xs) {
			return []cellT2{{loFi: -1, hiFi: -1, mem: false}}
		}
		return columnStates[col]
	}
	var vertErr error
	for c := 0; c <= len(xs) && vertErr == nil && sweepErr == nil; c++ {
		var x *big.Rat
		switch {
		case c == 0:
			x = xs[0]
		case c == len(xs):
			x = xs[len(xs)-1]
		default:
			x = xs[c]
		}
		left := lineStates(c - 1)
		right := lineStates(c)
		cellLoY := func(cc cellT2) *big.Rat {
			if sweepErr != nil || cc.loFi < 0 {
				return nil
			}
			y, err := candEventY(frags[cc.loFi].s, x, pol)
			if err != nil {
				sweepErr = err
				return nil
			}
			return y
		}
		cellHiY := func(cc cellT2) *big.Rat {
			if sweepErr != nil || cc.hiFi < 0 {
				return nil
			}
			y, err := candEventY(frags[cc.hiFi].s, x, pol)
			if err != nil {
				sweepErr = err
				return nil
			}
			return y
		}
		var cuts []*big.Rat
		for _, cc := range append(append([]cellT2{}, left...), right...) {
			if y := cellLoY(cc); y != nil {
				cuts = append(cuts, y)
			}
			if y := cellHiY(cc); y != nil {
				cuts = append(cuts, y)
			}
		}
		cuts = candSortRatsDedupe(cuts)
		stateAt := func(cells []cellT2, m *big.Rat) bool {
			for _, cc := range cells {
				ly := cellLoY(cc)
				hy := cellHiY(cc)
				loOK := ly == nil || ly.Cmp(m) < 0
				hiOK := hy == nil || m.Cmp(hy) < 0
				if loOK && hiOK {
					return cc.mem
				}
			}
			return false
		}
		emitDiff := func(lo, hi *big.Rat, mL, mR bool) {
			if mL == mR || vertErr != nil {
				return
			}
			if lo == nil || hi == nil {
				// Differing membership extending to infinity cannot
				// occur for bounded validated operands; deterministic
				// invariant rejection instead of silent mis-output.
				vertErr = &SpikeError{"unbounded-membership-difference"}
				return
			}
			addVertical(x, lo, hi, mL)
		}
		if len(cuts) == 0 {
			continue
		}
		mBelow := new(big.Rat).Sub(cuts[0], candUnitR)
		emitDiff(nil, cuts[0], stateAt(left, mBelow), stateAt(right, mBelow))
		for i := 0; i+1 < len(cuts); i++ {
			m := new(big.Rat).Add(cuts[i], cuts[i+1])
			m.Quo(m, new(big.Rat).SetInt64(2))
			emitDiff(cuts[i], cuts[i+1], stateAt(left, m), stateAt(right, m))
		}
		mAbove := new(big.Rat).Add(cuts[len(cuts)-1], candUnitR)
		emitDiff(cuts[len(cuts)-1], nil, stateAt(left, mAbove), stateAt(right, mAbove))
	}

	// Drop any degenerate zero-length pieces defensively.
	filtered := dirPieces[:0]
	for _, p := range dirPieces {
		if p[0].x.Cmp(p[1].x) != 0 || p[0].y.Cmp(p[1].y) != 0 {
			filtered = append(filtered, p)
		}
	}
	dirPieces = filtered

	if vertErr != nil {
		return candPolySet{}, vertErr
	}
	if sweepErr != nil {
		return candPolySet{}, sweepErr
	}
	cycles, err := candChain(dirPieces)
	if err != nil {
		return candPolySet{}, err
	}
	return candCyclesToSet(pol, cycles)
}

// candSplitSimpleWalk decomposes a closed vertex walk into simple
// rings by splitting at repeated vertices: the first repeated pair
// divides the walk into two closed sub-walks, each processed
// recursively. Sub-walks inherit direction; the decomposition is
// deterministic for a given walk.
func candSplitSimpleWalk(walk []candPt) [][]candPt {
	pos := map[string]int{}
	key := func(p candPt) string { return p.x.RatString() + "," + p.y.RatString() }
	for i, p := range walk {
		if j, ok := pos[key(p)]; ok {
			first := append([]candPt{}, walk[j:i]...)
			second := append(append([]candPt{}, walk[i:]...), walk[:j]...)
			var out [][]candPt
			out = append(out, candSplitSimpleWalk(first)...)
			out = append(out, candSplitSimpleWalk(second)...)
			return out
		}
		pos[key(p)] = i
	}
	return [][]candPt{walk}
}

// candRingIsSimple validates the simple-ring postcondition exactly
// before terminal commitment: no repeated vertices or zero-length
// edges, no proper crossings, no collinear overlaps, and no
// non-adjacent endpoint touches. All predicates are exact big.Rat
// sign tests over axis-aligned pieces.
func candRingIsSimple(r []candPt) bool {
	n := len(r)
	if n < 3 {
		return false
	}
	seen := map[string]bool{}
	key := func(p candPt) string { return p.x.RatString() + "," + p.y.RatString() }
	for _, p := range r {
		if seen[key(p)] {
			return false
		}
		seen[key(p)] = true
	}
	crossAt := func(o, a, b candPt) *big.Rat {
		return new(big.Rat).Sub(
			new(big.Rat).Mul(new(big.Rat).Sub(a.x, o.x), new(big.Rat).Sub(b.y, o.y)),
			new(big.Rat).Mul(new(big.Rat).Sub(a.y, o.y), new(big.Rat).Sub(b.x, o.x)))
	}
	onSeg := func(a, b, p candPt) bool {
		if crossAt(a, b, p).Sign() != 0 {
			return false
		}
		return minRat(a.x, b.x).Cmp(p.x) <= 0 && p.x.Cmp(maxRat(a.x, b.x)) <= 0 &&
			minRat(a.y, b.y).Cmp(p.y) <= 0 && p.y.Cmp(maxRat(a.y, b.y)) <= 0
	}
	for i := 0; i < n; i++ {
		a, b := r[i], r[(i+1)%n]
		if a.x.Cmp(b.x) == 0 && a.y.Cmp(b.y) == 0 {
			return false
		}
	}
	for i := 0; i < n; i++ {
		a1, b1 := r[i], r[(i+1)%n]
		for j := i + 1; j < n; j++ {
			a2, b2 := r[j], r[(j+1)%n]
			if j == i+1 || (i == 0 && j == n-1) {
				continue // adjacent edges share exactly one ring vertex
			}
			d1 := crossAt(a2, b2, a1).Sign()
			d2 := crossAt(a2, b2, b1).Sign()
			d3 := crossAt(a1, b1, a2).Sign()
			d4 := crossAt(a1, b1, b2).Sign()
			if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) &&
				((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
				return false // proper crossing
			}
			if onSeg(a1, b1, a2) || onSeg(a1, b1, b2) ||
				onSeg(a2, b2, a1) || onSeg(a2, b2, b1) {
				return false // interior contact or non-adjacent touch
			}
			// Collinear overlap with positive length.
			if d1 == 0 && d2 == 0 &&
				minRat(a1.x, b1.x).Cmp(maxRat(a2.x, b2.x)) <= 0 &&
				minRat(a2.x, b2.x).Cmp(maxRat(a1.x, b1.x)) <= 0 &&
				minRat(a1.y, b1.y).Cmp(maxRat(a2.y, b2.y)) <= 0 &&
				minRat(a2.y, b2.y).Cmp(maxRat(a1.y, b1.y)) <= 0 {
				return false
			}
		}
	}
	return true
}

func candWindingDelta(s candSeg) int {
	dy := new(big.Rat).Sub(s[1].y, s[0].y)
	if dy.Sign() != 0 {
		return dy.Sign()
	}
	dx := new(big.Rat).Sub(s[1].x, s[0].x)
	return dx.Sign()
}

func candSortRatsDedupe(xs []*big.Rat) []*big.Rat {
	sorted := candSortRats(xs)
	var out []*big.Rat
	for _, v := range sorted {
		if len(out) == 0 || out[len(out)-1].Cmp(v) != 0 {
			out = append(out, v)
		}
	}
	return out
}

func candSortRats(xs []*big.Rat) []*big.Rat {
	out := make([]*big.Rat, len(xs))
	copy(out, xs)
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j].Cmp(out[j-1]) < 0; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

// candCloneQuantized clones an operand, applying the pinned-grid
// quantization when the baseline policy demands it so identity results
// stay policy-consistent.
func candCloneQuantized(pol CandPolicy, s candPolySet) (candPolySet, error) {
	if !pol.Quantize {
		return cloneCand(s), nil
	}
	out := candPolySet{holes: make([][]candRing, len(s.outer))}
	for oi := range s.outer {
		ring := make(candRing, len(s.outer[oi]))
		for vi, v := range s.outer[oi] {
			pt, err := toCand(pol, v[0], v[1])
			if err != nil {
				return candPolySet{}, err
			}
			xf, _ := pt.x.Float64()
			yf, _ := pt.y.Float64()
			ring[vi] = [2]float64{xf, yf}
		}
		out.outer = append(out.outer, ring)
	}
	for oi := range s.holes {
		for _, hr := range s.holes[oi] {
			ring := make(candRing, len(hr))
			for vi, v := range hr {
				pt, err := toCand(pol, v[0], v[1])
				if err != nil {
					return candPolySet{}, err
				}
				xf, _ := pt.x.Float64()
				yf, _ := pt.y.Float64()
				ring[vi] = [2]float64{xf, yf}
			}
			out.holes[oi] = append(out.holes[oi], ring)
		}
	}
	return out, nil
}

func cloneCand(s candPolySet) candPolySet {
	out := candPolySet{outer: append([]candRing{}, s.outer...), holes: make([][]candRing, len(s.outer))}
	for i := range s.holes {
		out.holes[i] = append([]candRing{}, s.holes[i]...)
	}
	return out
}

func candNegInfCell() *big.Rat { return nil }

// candCyclesToSet classifies chained cycles (CCW positive = outer, CW
// negative = hole attached to smallest containing outer), canonicalizes
// order, and commits vertices to binary64 exactly once (B-1x) or
// converts dyadic grid ordinates exactly (B-1q).
func candCyclesToSet(pol CandPolicy, cycles [][]candPt) (candPolySet, error) {
	type ringT struct {
		pts  []candPt
		sign int
	}
	var outers, holes []ringT
	for _, c := range cycles {
		for _, walk := range candSplitSimpleWalk(c) {
			cleaned := candCanonClean(walk)
			if len(cleaned) < 3 {
				continue // permitted zero-area residue: degenerate residue piece
			}
			if !candRingIsSimple(cleaned) {
				return candPolySet{}, &SpikeError{"non-simple-output-ring"}
			}
			sign := candShoelaceSign(cleaned)
			if sign == 0 {
				continue
			}
			// Rotate to lexicographic least vertex.
			least := 0
			for i := 1; i < len(cleaned); i++ {
				if cleaned[i].x.Cmp(cleaned[least].x) < 0 ||
					(cleaned[i].x.Cmp(cleaned[least].x) == 0 && cleaned[i].y.Cmp(cleaned[least].y) < 0) {
					least = i
				}
			}
			rot := append(append([]candPt{}, cleaned[least:]...), cleaned[:least]...)
			if sign > 0 {
				outers = append(outers, ringT{rot, sign})
			} else {
				holes = append(holes, ringT{rot, sign})
			}
		}
	}
	set := candPolySet{holes: make([][]candRing, len(outers))}
	for _, h := range holes {
		rep, err := candRepInRing(h.pts)
		if err != nil {
			return candPolySet{}, err
		}
		best := -1
		for oi := range outers {
			if candWinding(rep, polySegsCand(outers[oi].pts)) != 0 {
				if best < 0 || candAbsArea(outers[oi].pts).Cmp(candAbsArea(outers[best].pts)) < 0 {
					best = oi
				}
			}
		}
		if best < 0 {
			return candPolySet{}, &SpikeError{"orphan-hole"}
		}
		set.holes[best] = append(set.holes[best], candCommitRing(pol, h.pts))
	}
	for _, o := range outers {
		set.outer = append(set.outer, candCommitRing(pol, o.pts))
	}
	candSortComponents(&set)
	return set, nil
}

// candRingLess compares committed float rings lexicographically after
// least-vertex rotation is already applied by construction.
func candRingLess(a, b candRing) bool {
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if a[i][0] != b[i][0] {
			return a[i][0] < b[i][0]
		}
		if a[i][1] != b[i][1] {
			return a[i][1] < b[i][1]
		}
	}
	return len(a) < len(b)
}

// candSortComponents orders disconnected components by canonical outer
// ring then hole tuples, matching R3.2.4 total ordering.
// candNormalizeSet canonicalizes an identity-cloned set: CCW outers,
// least-vertex rotation — matching engine-emitted form. Orientation is
// decided by converting each committed float to its exact dyadic
// rational (no float accumulation), so CW input rings normalize
// correctly.
func candNormalizeSet(ps *candPolySet) {
	// Holes: force CW orientation and least-vertex rotation.
	for i := range ps.holes {
		for j, h := range ps.holes[i] {
			pts := make([]candPt, len(h))
			for k, v := range h {
				x := new(big.Rat)
				x.SetFloat64(v[0])
				y := new(big.Rat)
				y.SetFloat64(v[1])
				pts[k] = candPt{x: x, y: y}
			}
			if candShoelaceSign(pts) > 0 {
				rev := make(candRing, len(h))
				for k := range h {
					rev[len(h)-1-k] = h[k]
				}
				ps.holes[i][j] = rev
				h = ps.holes[i][j]
			}
			least := 0
			for k := 1; k < len(h); k++ {
				if h[k][0] < h[least][0] || (h[k][0] == h[least][0] && h[k][1] < h[least][1]) {
					least = k
				}
			}
			rot := append(append(candRing{}, h[least:]...), h[:least]...)
			ps.holes[i][j] = rot
		}
		// Canonical hole order by tuple.
		hs := ps.holes[i]
		for a := 1; a < len(hs); a++ {
			for b := a; b > 0 && holeLessF(hs[b], hs[a-1]); b-- {
				hs[b], hs[a-1] = hs[a-1], hs[b]
			}
		}
	}
	// Outers: normalize FIRST (orientation + least-vertex rotation)
	// so the component sort below sees canonical tuples.
	for i, r := range ps.outer {
		pts := make([]candPt, len(r))
		for k, v := range r {
			x := new(big.Rat)
			x.SetFloat64(v[0])
			y := new(big.Rat)
			y.SetFloat64(v[1])
			pts[k] = candPt{x: x, y: y}
		}
		if candShoelaceSign(pts) < 0 {
			rev := make(candRing, len(r))
			for k := range r {
				rev[len(r)-1-k] = r[k]
			}
			ps.outer[i] = rev
			r = ps.outer[i]
		}
		least := 0
		for k := 1; k < len(r); k++ {
			if r[k][0] < r[least][0] || (r[k][0] == r[least][0] && r[k][1] < r[least][1]) {
				least = k
			}
		}
		ps.outer[i] = append(append(candRing{}, r[least:]...), r[:least]...)
	}
	// Components: canonical order by canonical outer tuple.
	os := ps.outer
	for a := 1; a < len(os); a++ {
		for b := a; b > 0 && holeLessF(os[b], os[a-1]); b-- {
			os[b], os[a-1] = os[a-1], os[b]
			ps.holes[b], ps.holes[a-1] = ps.holes[a-1], ps.holes[b]
		}
	}
	ps.outer = os
}

func candSortComponents(ps *candPolySet) {
	idx := make([]int, len(ps.outer))
	for i := range idx {
		idx[i] = i
	}
	less := func(x, y int) bool {
		switch {
		case candRingLess(ps.outer[x], ps.outer[y]):
			return true
		case candRingLess(ps.outer[y], ps.outer[x]):
			return false
		}
		n := len(ps.holes[x])
		if len(ps.holes[y]) < n {
			n = len(ps.holes[y])
		}
		for k := 0; k < n; k++ {
			if candRingLess(ps.holes[x][k], ps.holes[y][k]) {
				return true
			}
			if candRingLess(ps.holes[y][k], ps.holes[x][k]) {
				return false
			}
		}
		return len(ps.holes[x]) < len(ps.holes[y])
	}
	for i := 1; i < len(idx); i++ {
		for j := i; j > 0 && less(idx[j], idx[j-1]); j-- {
			idx[j], idx[j-1] = idx[j-1], idx[j]
		}
	}
	outers := make([]candRing, len(ps.outer))
	holeSets := make([][]candRing, len(ps.holes))
	for newI, oldI := range idx {
		outers[newI] = ps.outer[oldI]
		holeSets[newI] = ps.holes[oldI]
	}
	ps.outer = outers
	ps.holes = holeSets
}

func polySegsCand(r []candPt) []candSeg {
	out := make([]candSeg, len(r))
	for i := range r {
		out[i] = candSeg{r[i], r[(i+1)%len(r)]}
	}
	return out
}

func candAbsArea(r []candPt) *big.Rat {
	sum := new(big.Rat)
	for i := 0; i < len(r); i++ {
		a := r[i]
		b := r[(i+1)%len(r)]
		sum.Add(sum, new(big.Rat).Sub(
			new(big.Rat).Mul(a.x, b.y),
			new(big.Rat).Mul(b.x, a.y)))
	}
	return sum.Abs(sum)
}

// candRepInRing mirrors repInRing for candidate rationals: verified
// vertex-neighbor-midpoint witnesses first, halved perpendiculars as
// bounded fallback, deterministic rejection otherwise.
func twoRat() *big.Rat { return new(big.Rat).SetInt64(2) }

func candRepInRing(r []candPt) (candPt, error) {
	n := len(r)
	segs := polySegsCand(r)
	onEdge := func(w candPt) bool {
		for _, sg := range segs {
			if candBetween(sg, w) {
				return true
			}
		}
		return false
	}
	certified := func(w candPt) bool {
		return candWinding(w, segs) != 0 && !onEdge(w)
	}
	half := new(big.Rat).SetInt64(1)
	half.Quo(half, new(big.Rat).SetInt64(2))
	// Family 1: vertex vs neighbor-midpoint witnesses.
	for i := 0; i < n; i++ {
		a := r[(i-1+n)%n]
		b := r[(i+1)%n]
		m := candPt{
			x: new(big.Rat).Add(a.x, b.x).Quo(new(big.Rat).Add(a.x, b.x), new(big.Rat).SetInt64(2)),
			y: new(big.Rat).Add(a.y, b.y).Quo(new(big.Rat).Add(a.y, b.y), new(big.Rat).SetInt64(2)),
		}
		w := candPt{
			x: new(big.Rat).Add(m.x, r[i].x).Quo(new(big.Rat).Add(m.x, r[i].x), twoRat()),
			y: new(big.Rat).Add(m.y, r[i].y).Quo(new(big.Rat).Add(m.y, r[i].y), twoRat()),
		}
		if certified(w) {
			return w, nil
		}
	}
	// Family 2: edge-midpoint perpendicular offsets with bounded halving.
	for i := 0; i < n; i++ {
		a := r[i]
		b := r[(i+1)%n]
		prev := r[(i-1+n)%n]
		m := candPt{
			x: new(big.Rat).Add(a.x, b.x).Quo(new(big.Rat).Add(a.x, b.x), new(big.Rat).SetInt64(2)),
			y: new(big.Rat).Add(a.y, b.y).Quo(new(big.Rat).Add(a.y, b.y), new(big.Rat).SetInt64(2)),
		}
		nx := new(big.Rat).Sub(a.y, b.y)
		ny := new(big.Rat).Sub(b.x, a.x)
		cr := new(big.Rat).Sub(
			new(big.Rat).Mul(new(big.Rat).Sub(b.x, a.x), new(big.Rat).Sub(prev.y, a.y)),
			new(big.Rat).Mul(new(big.Rat).Sub(b.y, a.y), new(big.Rat).Sub(prev.x, a.x)))
		if cr.Sign() < 0 {
			nx.Neg(nx)
			ny.Neg(ny)
		}
		ex := new(big.Rat).Mul(nx, half)
		ey := new(big.Rat).Mul(ny, half)
		for k := 0; k <= maxProbeHalvings*4; k++ {
			w := candPt{
				x: new(big.Rat).Add(m.x, ex),
				y: new(big.Rat).Add(m.y, ey),
			}
			if certified(w) {
				return w, nil
			}
			ex.Quo(ex, new(big.Rat).SetInt64(2))
			ey.Quo(ey, new(big.Rat).SetInt64(2))
		}
	}
	return candPt{}, &SpikeError{"no-interior-witness"}
}

// candCommitRing performs the SINGLE terminal binary64 commitment.
func candCommitRing(pol CandPolicy, r []candPt) candRing {
	out := make(candRing, len(r))
	for i, p := range r {
		xf, _ := p.x.Float64()
		yf, _ := p.y.Float64()
		out[i] = [2]float64{xf, yf}
	}
	return out
}

// holeLessF orders committed rings by their first vertex tuple.
func holeLessF(a, b candRing) bool {
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if a[i][0] != b[i][0] {
			return a[i][0] < b[i][0]
		}
		if a[i][1] != b[i][1] {
			return a[i][1] < b[i][1]
		}
	}
	return len(a) < len(b)
}
