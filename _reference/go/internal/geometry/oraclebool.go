package geometry

import "math/big"

// Oracle boolean operations.
//
// Method: brute-force pairwise subdivision of all input edges into
// fragments, then per-fragment sided-membership classification — the
// fragment midpoint is nudged a bounded epsilon to its left and right,
// each probe is classified exactly against both operands, and the
// operation's closed-set membership predicate decides whether each
// side belongs to the result:
//
//	union:   member(sA,sB) = sA >= 0 || sB >= 0
//	inter:   member(sA,sB) = sA >= 0 && sB >= 0
//	diff:    member(sA,sB) = sA >= 0 && sB != 1   (A minus OPEN B)
//
// A fragment with equal membership on both sides is interior to the
// result or exterior to it and is dropped; otherwise it is directed so
// the result interior lies on its left. This automatically cancels
// internal seams (adjacent union), drops buried boundaries, retains
// coincident boundaries of identical operands once, and implements the
// R4.2.2 open-interior subtraction without special cases.
//
// This path shares no logic with the candidate sweep engine
// (sweep.go). Residual shared-family assumptions are recorded in the
// evidence document.

// polySet is a set of polygonal regions: outer rings CCW, hole rings
// CW, open rings without repeated closing vertices, all in one exact
// field.
type polySet struct {
	outer []oRing
	holes [][]oRing // holes[i] belongs to outer[i]
}

type oRing = []oPt

func ringSegs(r oRing) []oSeg {
	segs := make([]oSeg, len(r))
	for i := range r {
		segs[i] = oSeg{r[i], r[(i+1)%len(r)]}
	}
	return segs
}

func setSegs(s polySet) []oSeg {
	var out []oSeg
	for _, r := range s.outer {
		out = append(out, ringSegs(r)...)
	}
	for _, hs := range s.holes {
		for _, r := range hs {
			out = append(out, ringSegs(r)...)
		}
	}
	return out
}

type memFn func(sA, sB int) bool

func memUnion(sA, sB int) bool { return sA >= 0 || sB >= 0 }
func memInter(sA, sB int) bool { return sA >= 0 && sB >= 0 }

// memDiff implements A minus the OPEN interior of B: only strict B-
// interior points are removed; B's boundary remains.
func memDiff(sA, sB int) bool { return sA >= 0 && sB != 1 }

func isEmptySet(s polySet) bool { return len(setSegs(s)) == 0 }

// BooleanUnion assembles A ∪ B. Empty operands follow the identity laws
// deterministically instead of entering the probe machinery.
func BooleanUnion(a, b polySet) (polySet, error) {
	switch {
	case isEmptySet(a):
		return cloneSet(b), nil
	case isEmptySet(b):
		return cloneSet(a), nil
	}
	return booleanSetOp(a, b, memUnion)
}

// BooleanIntersection computes positive-area A ∩ B.
func BooleanIntersection(a, b polySet) (polySet, error) {
	if isEmptySet(a) || isEmptySet(b) {
		return polySet{holes: [][]oRing{}}, nil
	}
	return booleanSetOp(a, b, memInter)
}

// BooleanDifference computes A minus the OPEN interiors of B.
func BooleanDifference(a, b polySet) (polySet, error) {
	if isEmptySet(a) {
		return polySet{holes: [][]oRing{}}, nil
	}
	if isEmptySet(b) {
		return cloneSet(a), nil
	}
	return booleanSetOp(a, b, memDiff)
}

func cloneSet(s polySet) polySet {
	out := polySet{outer: append([]oRing{}, s.outer...), holes: make([][]oRing, len(s.holes))}
	for i := range s.holes {
		out.holes[i] = append([]oRing{}, s.holes[i]...)
	}
	return out
}

// subdivideAll splits every segment at every meeting point with any
// other segment: proper crossings, endpoint-on-edge touches, and
// collinear overlaps.
func subdivideAll(a, b []oSeg) []oSeg {
	type work struct {
		s      oSeg
		params []xnum
	}
	var ws []work
	for _, s := range append(append([]oSeg{}, a...), b...) {
		ws = append(ws, work{s: s})
	}

	all := make([]oSeg, 0, len(a)+len(b))
	all = append(all, a...)
	all = append(all, b...)

	for i := range ws {
		s := ws[i].s
		for _, o := range all {
			same := (o.a.eq(s.a) && o.b.eq(s.b)) || (o.a.eq(s.b) && o.b.eq(s.a))
			if same {
				continue
			}
			ws[i].params = append(ws[i].params, intersectParams(s, o)...)
		}
	}

	var out []oSeg
	for i := range ws {
		ps := sortXnums(dedupeXnums(strictInterior(ws[i].params)))
		cur := ws[i].s
		for _, t := range ps {
			p := pointAt(ws[i].s, t)
			out = append(out, oSeg{cur.a, p})
			cur = oSeg{p, cur.b}
		}
		out = append(out, cur)
	}
	return out
}

func pointAt(s oSeg, t xnum) oPt {
	return oPt{s.a.x.Add(s.dx().Mul(t)), s.a.y.Add(s.dy().Mul(t))}
}

// intersectParams returns parameters on s where o meets s, restricted
// to the closed unit interval.
func intersectParams(s, o oSeg) []xnum {
	t, ok := lineParam(s, o)
	if ok {
		u, ok2 := lineParam(o, s)
		if ok2 && inUnit(t) && inUnit(u) {
			return []xnum{t}
		}
		return nil
	}
	// Parallel: only collinear overlap contributes parameters.
	if orient(s.a, s.b, o.a) != 0 || orient(s.a, s.b, o.b) != 0 {
		return nil
	}
	var out []xnum
	for _, p := range []oPt{o.a, o.b} {
		if t := clampedParam(s, p); t != nil {
			out = append(out, t)
		}
	}
	return out
}

// lineParam solves for the intersection of segments s and o as a
// parameter on s; ok=false when parallel.
func lineParam(s, o oSeg) (t xnum, ok bool) {
	denom := s.dx().Mul(o.dy()).Sub(s.dy().Mul(o.dx()))
	if denom.Sign() == 0 {
		return nil, false
	}
	rx := o.a.x.Sub(s.a.x)
	ry := o.a.y.Sub(s.a.y)
	numT := rx.Mul(o.dy()).Sub(ry.Mul(o.dx()))
	return numT.Div(denom), true
}

func inUnit(t xnum) bool {
	return t != nil && t.Sign() >= 0 && t.Sub(xone(t)).Sign() <= 0
}

func strictInterior(ts []xnum) []xnum {
	var out []xnum
	for _, t := range ts {
		if t == nil || t.Sign() <= 0 || t.Sub(xone(t)).Sign() >= 0 {
			continue
		}
		out = append(out, t)
	}
	return out
}

func dedupeXnums(ts []xnum) []xnum {
	var out []xnum
	for _, t := range ts {
		dup := false
		for _, u := range out {
			if t.Sub(u).Sign() == 0 {
				dup = true
				break
			}
		}
		if !dup {
			out = append(out, t)
		}
	}
	return out
}

func sortXnums(ts []xnum) []xnum {
	out := append([]xnum(nil), ts...)
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j].Sub(out[j-1]).Sign() < 0; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

// clampedParam returns p's parameter along s when p is collinear and
// within the closed segment, else nil.
func clampedParam(s oSeg, p oPt) xnum {
	if !colinearBetween(s, p) {
		return nil
	}
	dx, dy := s.dx(), s.dy()
	switch {
	case dx.Sign() != 0:
		return p.x.Sub(s.a.x).Div(dx)
	case dy.Sign() != 0:
		return p.y.Sub(s.a.y).Div(dy)
	}
	return nil
}

// bboxEps derives the probe epsilon from the combined bounding box: at
// most 1/32 of the smaller box side, computed exactly in-field.
func bboxEps(segs []oSeg) (xnum, error) {
	xmin, xmax := segs[0].a.x, segs[0].a.x
	ymin, ymax := segs[0].a.y, segs[0].a.y
	for _, s := range segs {
		for _, p := range []oPt{s.a, s.b} {
			if p.x.Sub(xmin).Sign() < 0 {
				xmin = p.x
			}
			if p.x.Sub(xmax).Sign() > 0 {
				xmax = p.x
			}
			if p.y.Sub(ymin).Sign() < 0 {
				ymin = p.y
			}
			if p.y.Sub(ymax).Sign() > 0 {
				ymax = p.y
			}
		}
	}
	w := xmax.Sub(xmin)
	h := ymax.Sub(ymin)
	m := w
	if h.Sub(m).Sign() < 0 {
		m = h
	}
	if m.Sign() <= 0 {
		return nil, &SpikeError{"degenerate-bbox"}
	}
	return scaleDown(m, big.NewRat(1, 32)), nil
}

// nudgeProbes returns points strictly left/right of the fragment
// midpoint. Offset = eps·(-dy,dx)/(2·max(|dx|,|dy|)): perpendicular
// distance within [eps/2, eps/sqrt(2)], computed in-field.
func nudgeProbes(f oSeg, eps xnum) (pL, pR oPt) {
	m := segMid(f)
	dx, dy := f.dx(), f.dy()
	adx := dx
	if adx.Sign() < 0 {
		adx = adx.Neg()
	}
	ady := dy
	if ady.Sign() < 0 {
		ady = ady.Neg()
	}
	maxAD := adx
	if ady.Sub(adx).Sign() > 0 {
		maxAD = ady
	}
	twoF := xone(maxAD).Add(xone(maxAD))
	den := maxAD.Mul(twoF)
	nx := dy.Neg().Mul(eps).Div(den)
	ny := dx.Mul(eps).Div(den)
	return oPt{x: m.x.Add(nx), y: m.y.Add(ny)}, oPt{x: m.x.Sub(nx), y: m.y.Sub(ny)}
}

func booleanSetOp(a, b polySet, member memFn) (polySet, error) {
	cycles, err := rawBoundaryCycles(a, b, member)
	if err != nil {
		return polySet{}, err
	}
	ps, err := cyclesToPolySet(cycles)
	if err != nil {
		return polySet{}, err
	}
	return ps, nil
}

// rawBoundaryCycles runs the oracle's decision machinery (sided-probe
// classification over subdivided fragments) up to, but not including,
// post-decision normalization, and returns the raw chained boundary
// walks. Exposed so tests can characterize pre-normalization
// repeated-node walks alongside the normalized pipeline output.
func rawBoundaryCycles(a, b polySet, member memFn) ([][]oPt, error) {
	segsA, segsB := setSegs(a), setSegs(b)
	frags := subdivideAll(segsA, segsB)
	eps, err := bboxEps(frags)
	if err != nil {
		return nil, err
	}

	index := map[string]int{}
	var dir []oSeg
	for _, f := range frags {
		pL, pR := nudgeProbes(f, eps)
		sAL, err := probeState(pL, segsA)
		if err != nil {
			return nil, err
		}
		sBL, err := probeState(pL, segsB)
		if err != nil {
			return nil, err
		}
		sAR, err := probeState(pR, segsA)
		if err != nil {
			return nil, err
		}
		sBR, err := probeState(pR, segsB)
		if err != nil {
			return nil, err
		}
		lm := member(sAL, sBL)
		rm := member(sAR, sBR)
		if lm == rm {
			continue // interior or exterior to the result: drop
		}
		s := f
		if !lm {
			s = oSeg{f.b, f.a} // result interior goes on the left
		}
		k := ptKey(minPt(s)) + "|" + ptKey(maxPt(s))
		if j, ok := index[k]; ok {
			// Duplicate survivor: directions must agree; retain once.
			e := dir[j]
			if !(e.a.eq(s.a) && e.b.eq(s.b)) {
				return nil, &SpikeError{"duplicate-direction-conflict"}
			}
			continue
		}
		index[k] = len(dir)
		dir = append(dir, s)
	}

	return chainCycles(dir), nil
}

// probeState classifies a nudged probe point; landing exactly on a
// boundary is a deterministic failure rather than a silent guess.
func probeState(p oPt, segs []oSeg) (int, error) {
	for _, s := range segs {
		if colinearBetween(s, p) {
			return 0, &SpikeError{"probe-on-boundary"}
		}
	}
	if ptInRingsWinding(p, segs) {
		return 1, nil
	}
	return -1, nil
}

// chainCycles links directed segments into closed cycles: at each node
// the next segment is the angular predecessor (in CCW order) of the
// reversed incoming direction — the face-on-left rule. Unclosed walks
// are dropped deterministically.
func chainCycles(dir []oSeg) [][]oPt {
	at := map[string][]int{}
	for i, s := range dir {
		at[ptKey(s.a)] = append(at[ptKey(s.a)], i)
	}
	used := make([]bool, len(dir))
	var cycles [][]oPt
	for i := range dir {
		if used[i] {
			continue
		}
		var cyc []oPt
		cur := i
		startNode := ptKey(dir[cur].a)
		closed := false
		for {
			used[cur] = true
			cyc = append(cyc, dir[cur].a)
			head := dir[cur].b
			if ptKey(head) == startNode {
				closed = true
				break
			}
			next := pickNext(dir, at, used, head, dir[cur])
			if next < 0 {
				break // open chain: dropped deterministically
			}
			cur = next
		}
		if len(cyc) >= 3 && closed {
			cycles = append(cycles, cyc)
		}
	}
	return cycles
}

// pickNext chooses the outgoing unused segment from node head that is
// the angular predecessor (in CCW order) of the reversed incoming
// direction — the face-on-left continuation rule.
func pickNext(dir []oSeg, at map[string][]int, used []bool, head oPt, incoming oSeg) int {
	revDir := oPt{x: incoming.b.x.Sub(incoming.a.x), y: incoming.b.y.Sub(incoming.a.y)}
	best := -1
	for _, cand := range at[ptKey(head)] {
		if used[cand] {
			continue
		}
		if !dir[cand].a.eq(head) {
			continue
		}
		cd := oPt{x: dir[cand].dx(), y: dir[cand].dy()}
		if best < 0 {
			best = cand
			continue
		}
		bd := oPt{x: dir[best].dx(), y: dir[best].dy()}
		// Among candidates strictly CCW-before rev choose the max; if
		// none exist choose the overall max (cyclic predecessor).
		if angCCW(cd, revDir) {
			if !angCCW(bd, revDir) || angCCW(bd, cd) {
				best = cand
			}
		} else {
			if angCCW(bd, revDir) {
				continue
			}
			if angCCW(bd, cd) {
				best = cand
			}
		}
	}
	return best
}

// cyclesToPolySet converts chained boundary walks into canonical
// SIMPLE rings per R3.2.4. Post-decision normalization (disclosed):
// the raw frontier walk of a closed point-contact may pass through a
// pinch vertex more than once; such walks are split at their first
// repeated vertex into simple sub-rings before classification, so
// corner-touching components stay distinct simple outers instead of
// one self-touching ring. Zero-area pieces are dropped (they cannot
// bound free or occupied area). Output rings are rotated to their
// lexicographically least vertex; holes and components are placed in
// canonical tuple order.
func cyclesToPolySet(cycles [][]oPt) (polySet, error) {
	var out polySet
	var holes []oRing
	for _, c := range cycles {
		for _, walk := range splitRepeatedWalk(c) {
			cleaned := canonCleanRing(walk)
			if len(cleaned) < 3 {
				continue // permitted zero-area residue: degenerate residue piece
			}
			if !oRingSimple(cleaned) {
				return polySet{}, &SpikeError{"oracle-non-simple-ring"}
			}
			sign := shoelaceSign(cleaned)
			if sign == 0 {
				continue
			}
			cleaned = canonRotate(cleaned)
			if sign > 0 {
				out.outer = append(out.outer, cleaned)
			} else {
				holes = append(holes, cleaned)
			}
		}
	}
	out.holes = make([][]oRing, len(out.outer))
	for _, hole := range holes {
		rep, err := repInRing(hole)
		if err != nil {
			return polySet{}, err
		}
		best := -1
		for oi := range out.outer {
			if ptInRingsWinding(rep, ringSegs(out.outer[oi])) {
				if best < 0 || absAreaLess(out.outer[oi], out.outer[best]) {
					best = oi
				}
			}
		}
		if best < 0 {
			return polySet{}, &SpikeError{"orphan-hole"}
		}
		out.holes[best] = append(out.holes[best], hole)
	}
	for i := range out.holes {
		sortHoles(out.holes[i])
	}
	sortComponents(&out)
	return out, nil
}

// splitRepeatedWalk decomposes a closed boundary walk into simple
// sub-walks by splitting at the first repeated vertex (earliest
// second occurrence); each part is processed recursively. Sub-walks
// inherit direction; the decomposition is deterministic for a given
// walk and uses only exact vertex identity.
func splitRepeatedWalk(walk []oPt) [][]oPt {
	pos := map[string]int{}
	for i, p := range walk {
		k := ptKey(p)
		if j, ok := pos[k]; ok {
			first := append([]oPt{}, walk[j:i]...)
			second := append(append([]oPt{}, walk[i:]...), walk[:j]...)
			var out [][]oPt
			out = append(out, splitRepeatedWalk(first)...)
			out = append(out, splitRepeatedWalk(second)...)
			return out
		}
		pos[k] = i
	}
	return [][]oPt{walk}
}

// oRingSimple validates the exact simple-ring postcondition over an
// oracle ring: no repeated vertices or zero-length edges; no proper
// crossings; no endpoint contact with a non-adjacent edge; and no
// non-adjacent collinear pair sharing interval contact (positive-length
// overlap or a shared point). Disjoint collinear edges are valid and
// accepted. All decisions use exact field-aware signs.
func oRingSimple(r oRing) bool {
	n := len(r)
	if n < 3 {
		return false
	}
	seen := map[string]bool{}
	for _, p := range r {
		k := ptKey(p)
		if seen[k] {
			return false // repeated vertex
		}
		seen[k] = true
	}
	for i := 0; i < n; i++ {
		if r[i].eq(r[(i+1)%n]) {
			return false // zero-length edge
		}
	}
	for i := 0; i < n; i++ {
		a1, b1 := r[i], r[(i+1)%n]
		for j := i + 1; j < n; j++ {
			if j == i+1 || (i == 0 && j == n-1) {
				continue // adjacent edges share exactly one ring vertex
			}
			a2, b2 := r[j], r[(j+1)%n]
			d1 := orient(a2, b2, a1)
			d2 := orient(a2, b2, b1)
			if d1 == 0 && d2 == 0 {
				// Collinear non-adjacent edges: reject exactly when the
				// parameter intervals share any point (positive-length
				// overlap OR a shared endpoint). Disjoint collinear
				// edges remain valid.
				s1 := oSeg{a1, b1}
				s2 := oSeg{a2, b2}
				lo := maxPt(oSeg{minPt(s1), minPt(s2)})
				hi := minPt(oSeg{maxPt(s1), maxPt(s2)})
				if !oLess(hi, lo) {
					return false
				}
				continue
			}
			d3 := orient(a1, b1, a2)
			d4 := orient(a1, b1, b2)
			if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) &&
				((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
				return false // proper crossing
			}
			if colinearBetween(oSeg{a1, b1}, a2) || colinearBetween(oSeg{a1, b1}, b2) ||
				colinearBetween(oSeg{a2, b2}, a1) || colinearBetween(oSeg{a2, b2}, b1) {
				return false // interior contact or non-adjacent touch
			}
		}
	}
	return true
}

// canonCleanRing removes duplicate and exactly-collinear vertices.
func canonCleanRing(r oRing) oRing {
	var out oRing
	for _, p := range r {
		if len(out) > 0 && p.eq(out[len(out)-1]) {
			continue
		}
		out = append(out, p)
	}
	for len(out) > 1 && out[0].eq(out[len(out)-1]) {
		out = out[:len(out)-1]
	}
	changed := true
	for changed && len(out) > 2 {
		changed = false
		var next oRing
		for i := 0; i < len(out); i++ {
			pv := out[(i-1+len(out))%len(out)]
			nx := out[(i+1)%len(out)]
			if orient(pv, out[i], nx) == 0 {
				changed = true
				continue
			}
			next = append(next, out[i])
		}
		out = next
	}
	return out
}

func absAreaLess(a, b oRing) bool {
	return absDoubledArea(a).Sub(absDoubledArea(b)).Sign() < 0
}

func absDoubledArea(r oRing) xnum {
	sum := r[0].x.Sub(r[0].x)
	for i := 0; i < len(r); i++ {
		a := r[i]
		b := r[(i+1)%len(r)]
		sum = sum.Add(a.x.Mul(b.y)).Sub(b.x.Mul(a.y))
	}
	if sum.Sign() < 0 {
		return sum.Neg()
	}
	return sum
}

// repInRing returns a certified witness point strictly inside the
// region enclosed by a simple ring. Candidates are edge midpoints
// offset by iteratively halved perpendiculars toward the local interior
// side; each candidate is verified by the exact winding test plus an
// off-boundary check before use. The shrink loop is bounded and
// terminates; exhausting it rejects deterministically with
// no-interior-witness (a narrow-feature signal for the evidence
// record), never a silently wrong containment answer.
func repInRing(ring oRing) (oPt, error) {
	n := len(ring)
	segs := ringSegs(ring)
	certified := func(w oPt) bool {
		return ptInRingsWinding(w, segs) && !onAnyEdge(w, ring)
	}
	// Candidate family 1: for each vertex, the midpoint between the
	// vertex and the midpoint of its neighbors' segment — near a convex
	// corner this lies strictly inside for simple polygons.
	for i := 0; i < n; i++ {
		a := ring[(i-1+n)%n]
		b := ring[(i+1)%n]
		m := oPt{mid(a.x, b.x), mid(a.y, b.y)}
		w := oPt{x: mid(m.x, ring[i].x), y: mid(m.y, ring[i].y)}
		if certified(w) {
			return w, nil
		}
	}
	// Candidate family 2: edge-midpoint offsets by iteratively halved
	// perpendiculars toward the local interior side.
	for i := 0; i < n; i++ {
		a := ring[i]
		b := ring[(i+1)%n]
		prev := ring[(i-1+n)%n]
		m := oPt{mid(a.x, b.x), mid(a.y, b.y)}
		nx := a.y.Sub(b.y) // left normal (-dy, dx)
		ny := b.x.Sub(a.x)
		if orient(a, b, prev) < 0 {
			nx, ny = nx.Neg(), ny.Neg()
		}
		e := oPt{x: xhalf(nx), y: xhalf(ny)}
		for k := 0; k <= maxProbeHalvings*4; k++ {
			w := oPt{x: m.x.Add(e.x), y: m.y.Add(e.y)}
			if certified(w) {
				return w, nil
			}
			e = oPt{x: xhalf(e.x), y: xhalf(e.y)}
		}
	}
	return oPt{}, &SpikeError{"no-interior-witness"}
}

func onAnyEdge(p oPt, r oRing) bool {
	for _, s := range ringSegs(r) {
		if colinearBetween(s, p) {
			return true
		}
	}
	return false
}

func sortHoles(hs []oRing) {
	for i := 1; i < len(hs); i++ {
		for j := i; j > 0 && compareRings(hs[j], hs[j-1]) < 0; j-- {
			hs[j], hs[j-1] = hs[j-1], hs[j]
		}
	}
}

// sortComponents orders disconnected components by canonical outer
// ring, then by hole tuples in order.
func sortComponents(ps *polySet) {
	idx := make([]int, len(ps.outer))
	for i := range idx {
		idx[i] = i
	}
	less := func(x, y int) bool {
		if c := compareRings(ps.outer[x], ps.outer[y]); c != 0 {
			return c < 0
		}
		n := len(ps.holes[x])
		if len(ps.holes[y]) < n {
			n = len(ps.holes[y])
		}
		for k := 0; k < n; k++ {
			if c := compareRings(ps.holes[x][k], ps.holes[y][k]); c != 0 {
				return c < 0
			}
		}
		return len(ps.holes[x]) < len(ps.holes[y])
	}
	for i := 1; i < len(idx); i++ {
		for j := i; j > 0 && less(idx[j], idx[j-1]); j-- {
			idx[j], idx[j-1] = idx[j-1], idx[j]
		}
	}
	outers := make([]oRing, len(ps.outer))
	holeSets := make([][]oRing, len(ps.holes))
	for newI, oldI := range idx {
		outers[newI] = ps.outer[oldI]
		holeSets[newI] = ps.holes[oldI]
	}
	ps.outer = outers
	ps.holes = holeSets
}
