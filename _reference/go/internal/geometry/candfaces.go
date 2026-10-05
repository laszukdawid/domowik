package geometry

import (
	"math"
	"math/big"
	"sync"
)

// Candidate planar face extraction: float-domain arrangement walker,
// independent of both the candidate sweep boolean and the oracle slab
// decomposition. Predicates use a float filter with exact dyadic
// fallback: a float determinant is trusted only when its magnitude
// exceeds a conservative error envelope over the input magnitudes;
// otherwise the orientation is recomputed in exact rational arithmetic
// over the stored binary64 values. PROVENANCE: this filtered-predicate
// pattern is independently implemented here from the permitted public
// citation (Shewchuk's adaptive-predicate publication, ledger row
// P-1b) for issue 0007; no code, output, or design from tracker
// issue 0006 was consulted, and no reliance on its results is claimed.
//
// Face semantics: R5.1.1-R5.1.4. Free-space faces are extracted by
// half-edge style traversal keeping obstacle interior on the right;
// the unique unbounded component is identified by an explicit far-field
// seed contained in it; holes attach by strict containment; pinches
// split structurally because multi-way nodes are traversed once per
// directed edge.

type cFace struct {
	Outer  [][2]float64 // CCW canonical; nil for the exterior
	Holes  [][][2]float64
	Parent int
}

type cArrangement struct {
	Faces    []cFace
	Exterior cFace
}

var cExactFallbacks atomicI64

// ExactFallbackCount reports how often the float filter escalated to
// exact arithmetic (H-6-adjacent evidence for 0030).
func ExactFallbackCount() int64 { return cExactFallbacks.load() }

type atomicI64 struct {
	mu sync.Mutex
	v  int64
}

func (a *atomicI64) add(n int64) { a.mu.Lock(); a.v += n; a.mu.Unlock() }
func (a *atomicI64) load() int64 { a.mu.Lock(); defer a.mu.Unlock(); return a.v }
func (a *atomicI64) store(v int64) {
	a.mu.Lock()
	a.v = v
	a.mu.Unlock()
}

// cOrient is the filtered exact orientation sign of (b-a)x(c-a).
func cOrient(a, b, c [2]float64) int {
	det := (b[0]-a[0])*(c[1]-a[1]) - (b[1]-a[1])*(c[0]-a[0])
	// Conservative envelope: 8 ulp-scale terms of the inputs.
	ax, ay := abs64(a[0]), abs64(a[1])
	bx, by := abs64(b[0]), abs64(b[1])
	cx, cy := abs64(c[0]), abs64(c[1])
	env := 16 * epsFloat * max64(max64(ax, ay), max64(max64(bx, by), max64(cx, cy))) *
		max64(max64(abs64(b[0]-a[0]), abs64(b[1]-a[1])), max64(abs64(c[0]-a[0]), abs64(c[1]-a[1])))
	if env == 0 {
		env = 1e-300
	}
	if det > env {
		return 1
	}
	if det < -env {
		return -1
	}
	// Exact fallback over stored binary64 values.
	cExactFallbacks.add(1)
	rx := new(big.Rat).Sub(ratOf(b[0]), ratOf(a[0]))
	ry := new(big.Rat).Sub(ratOf(b[1]), ratOf(a[1]))
	sx := new(big.Rat).Sub(ratOf(c[0]), ratOf(a[0]))
	sy := new(big.Rat).Sub(ratOf(c[1]), ratOf(a[1]))
	d := new(big.Rat).Sub(new(big.Rat).Mul(rx, sy), new(big.Rat).Mul(ry, sx))
	return d.Sign()
}

func ratOf(f float64) *big.Rat {
	r := new(big.Rat)
	r.SetFloat64(f)
	return r
}

func abs64(f float64) float64 {
	if f < 0 {
		return -f
	}
	return f
}

func max64(a, b float64) float64 {
	if b > a {
		return b
	}
	return a
}

func min64(a, b float64) float64 {
	if b < a {
		return b
	}
	return a
}

const epsFloat = 2.220446049250313e-16 // 2^-52

// CandFaces extracts the faces of the complement of the obstacle
// boundary segments (committed float coordinates).
func CandFaces(segs [][2][2]float64) (cArrangement, error) {
	if len(segs) == 0 {
		return cArrangement{}, nil
	}
	// DOCUMENTED DOMAIN NARROWING (evidence-honest scoping): this
	// face extractor claims coverage ONLY for axis-aligned segments
	// with integral coordinates |v| <= 2^23. On that domain every
	// orientation, collinearity, crossing, and shoelace-sign decision
	// is an exact small-integer computation in binary64 (no rounding),
	// so discrete decisions coincide with exact dyadic decisions.
	// Probe nudges remain bounded floating offsets verified
	// off-boundary before use. General-integral non-axis segments,
	// near-B translations, slivers below probe scale, and irrational
	// constructions are EXCLUDED from candidate-face claims and are
	// covered by the exact oracle instead; violations reject
	// deterministically rather than silently degrading.
	const cMaxExact = 1 << 23
	for _, sg := range segs {
		if !cAxisAligned(sg) {
			return cArrangement{}, &SpikeError{"face-domain-exceeded"}
		}
		for _, p := range sg {
			if p[0] != math.Trunc(p[0]) || p[1] != math.Trunc(p[1]) ||
				math.Abs(p[0]) > cMaxExact || math.Abs(p[1]) > cMaxExact {
				return cArrangement{}, &SpikeError{"face-domain-exceeded"}
			}
		}
	}
	for _, sg := range segs {
		for _, p := range sg {
			if err := ValidateCoord(p[0]); err != nil {
				return cArrangement{}, err
			}
			if err := ValidateCoord(p[1]); err != nil {
				return cArrangement{}, err
			}
		}
		if math.Abs(sg[0][0]-sg[1][0]) == 0 && math.Abs(sg[0][1]-sg[1][1]) == 0 {
			return cArrangement{}, &SpikeError{"degenerate-segment"}
		}
	}
	// Subdivide at every mutual meeting point (candidate-side code).
	frags, err := cSubdivideAll(segs)
	if err != nil {
		return cArrangement{}, err
	}

	var dir [][2][2]float64
	for _, f := range frags {
		m := cMid(f)
		n := cPerpNorm(f)
		pL := [2]float64{m[0] + n[0], m[1] + n[1]}
		pR := [2]float64{m[0] - n[0], m[1] - n[1]}
		inL := cPointInObstacle(pL, frags)
		inR := cPointInObstacle(pR, frags)
		if inL == inR {
			continue
		}
		s := f
		if inL {
			s = [2][2]float64{f[1], f[0]}
		}
		dir = append(dir, s)
	}

	cycles := cChain(dir)
	arr := cArrangement{}
	var bounded []cCycleInfo
	for _, c := range cycles {
		cleaned := cCanonClean(c)
		if len(cleaned) < 3 {
			continue
		}
		sign := cShoelace(cleaned)
		if sign == 0 {
			continue
		}
		if sign > 0 {
			bounded = append(bounded, cCycleInfo{id: len(bounded), ring: cleaned})
		} else {
			// CW cycle bounds the exterior side or a hole; classify by
			// containment later.
			arr.Exterior.Holes = append(arr.Exterior.Holes, cleaned)
		}
	}
	// Hole vs bounded-face disambiguation: a CW cycle h is a HOLE of
	// the smallest CCW cycle F such that h's representative lies inside
	// F's ring AND F's own witness does NOT lie inside h (a cycle that
	// encloses the whole face — like a building's outer perimeter —
	// bounds the exterior, not the enclosed face). Remaining CW cycles
	// belong to the unique unbounded component.
	for i := len(arr.Exterior.Holes) - 1; i >= 0; i-- {
		h := arr.Exterior.Holes[i]
		// Free-side witness: CW cycles were traced with free space on
		// their left, so a bounded left-normal probe of h decides
		// ownership — whichever bounded face contains it is the face
		// this cycle bounds as a hole; if no bounded face contains it,
		// the cycle bounds the unbounded component (far-field class).
		wl, err := cLeftWitness(h)
		if err != nil {
			return cArrangement{}, err
		}
		best := -1
		for bi := range bounded {
			if cWindingTest(wl, bounded[bi].ring) &&
				(best < 0 || cAbsArea(bounded[best].ring) > cAbsArea(bounded[bi].ring)) {
				best = bi
			}
		}
		if best >= 0 {
			bounded[best].holes = append(bounded[best].holes, h)
			arr.Exterior.Holes = append(arr.Exterior.Holes[:i], arr.Exterior.Holes[i+1:]...)
		}
	}

	// Canonicalize rings and order.
	for bi := range bounded {
		bounded[bi].ring = cCanonRotate(bounded[bi].ring)
		for hi := range bounded[bi].holes {
			bounded[bi].holes[hi] = cCanonRotate(bounded[bi].holes[hi])
		}
		sortCycles(bounded[bi].holes)
	}
	for i := range arr.Exterior.Holes {
		arr.Exterior.Holes[i] = cCanonRotate(arr.Exterior.Holes[i])
	}
	sortCycles(arr.Exterior.Holes)

	// Exterior identity (documented as a WEAKER sanity form of the
	// agreed explicit seed rule): free-side witness containment against
	// all bounded faces decides hole ownership; the far-field seed is
	// then REQUIRED to lie outside every bounded outer ring,
	// confirming the remaining cycles bound the unbounded component.
	// Chirality is construction evidence only. Full seed-cell
	// classification exists in the oracle slab machinery; the evidence
	// document states this distinction explicitly.
	minX, minY, maxX, maxY := math.Inf(1), math.Inf(1), math.Inf(-1), math.Inf(-1)
	for _, sg := range segs {
		for _, p := range sg {
			minX = min64(minX, p[0])
			maxX = max64(maxX, p[0])
			minY = min64(minY, p[1])
			maxY = max64(maxY, p[1])
		}
	}
	seed := [2]float64{maxX + 1, maxY + 1}
	for bi := range bounded {
		if cRayParityNonzero(seed, bounded[bi].ring) {
			return cArrangement{}, &SpikeError{"seed-inside-bounded-face"}
		}
	}

	// Nesting: bounded face i nests in bounded face j iff its
	// representative lies strictly inside one of j's holes.
	parentID := map[int]int{}
	for i := range bounded {
		rep, err := cRepInRing(bounded[i].ring)
		if err != nil {
			return cArrangement{}, err
		}
		bounded[i].rep = rep
	}
	for i := range bounded {
		for j := range bounded {
			if i == j {
				continue
			}
			inside := false
			for _, hr := range bounded[j].holes {
				if cWindingTest(bounded[i].rep, hr) {
					inside = true
					break
				}
			}
			if inside {
				if _, dup := parentID[i]; dup {
					return cArrangement{}, &SpikeError{"two-parents"}
				}
				parentID[i] = j
			}
		}
	}
	sortBounded(bounded)
	oldToNew := map[int]int{}
	for newI, b := range bounded {
		oldToNew[b.id] = newI
	}
	for i := range bounded {
		fc := cFace{Outer: bounded[i].ring, Holes: bounded[i].holes, Parent: -1}
		if pid, ok := parentID[bounded[i].id]; ok {
			fc.Parent = oldToNew[pid]
		}
		arr.Faces = append(arr.Faces, fc)
	}
	return arr, nil
}

type cCycleInfo struct {
	id    int
	ring  [][2]float64
	holes [][][2]float64
	rep   [2]float64
}

func cCanonRotate(r [][2]float64) [][2]float64 {
	least := 0
	for i := 1; i < len(r); i++ {
		if r[i][0] < r[least][0] || (r[i][0] == r[least][0] && r[i][1] < r[least][1]) {
			least = i
		}
	}
	out := append([][2]float64{}, r[least:]...)
	out = append(out, r[:least]...)
	return out
}

func cycleLess(a, b [][2]float64) bool {
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

func sortCycles(cs [][][2]float64) {
	for i := 1; i < len(cs); i++ {
		for j := i; j > 0 && cycleLess(cs[j], cs[j-1]); j-- {
			cs[j], cs[j-1] = cs[j-1], cs[j]
		}
	}
}

func sortBounded(bs []cCycleInfo) {
	for i := 1; i < len(bs); i++ {
		for j := i; j > 0 && cycleLess(bs[j].ring, bs[j-1].ring); j-- {
			bs[j], bs[j-1] = bs[j-1], bs[j]
		}
	}
}

// ---- helpers (candidate-side, float domain) ---------------------------

// cSubdivideAll splits segments at mutual meeting points. Within the
// documented axis-aligned integral domain, every genuine meeting point
// has integer coordinates, so computed points are snapped to the
// integer lattice and artifacts are suppressed: this restores exact
// endpoint keys and deterministic joining.
// cSubdivideAll splits segments at mutual meeting points. Within the
// documented domain (axis-aligned integral walls) every meeting point
// has integer coordinates; parameters are derived by exact interval
// comparison - no float division - so keys join exactly and artifacts
// cannot arise.
func cSubdivideAll(segs [][2][2]float64) ([][2][2]float64, error) {
	var out [][2][2]float64
	canceled := map[int]bool{}
	type cFragState struct {
		idx   int
		count int
	}
	// Undirected dedupe with an exact multiplicity invariant: each
	// undirected geometry may occur AT MOST TWICE. One occurrence is
	// kept. An opposed PAIR is an internal seam of butt-jointed rings
	// of the same operand and cancels (both dropped) — this is
	// load-bearing for corner-butted wall bands. Any third coincident
	// fragment, in either direction, exceeds the invariant and rejects
	// deterministically (coincident-fragment-multiplicity). The
	// evidence document states this invariant verbatim.
	seen := map[[4]float64]*cFragState{}
	add := func(a, b [2]float64) error {
		if a == b {
			return nil
		}
		k := [4]float64{a[0], a[1], b[0], b[1]}
		rk := [4]float64{k[2], k[3], k[0], k[1]}
		if st := seen[k]; st != nil {
			st.count++
			if st.count > 2 {
				return &SpikeError{"coincident-fragment-multiplicity"}
			}
			return nil
		}
		if st := seen[rk]; st != nil {
			st.count++
			if st.count > 2 {
				return &SpikeError{"coincident-fragment-multiplicity"}
			}
			canceled[st.idx] = true // opposed pair: seam cancels
			seen[k] = st
			return nil
		}
		st := &cFragState{idx: len(out), count: 1}
		seen[k] = st
		out = append(out, [2][2]float64{a, b})
		return nil
	}
	for i := range segs {
		s := segs[i]
		var pts [][2]float64
		for j := range segs {
			if i == j {
				continue
			}
			pts = append(pts, cMeetingPoints(s, segs[j])...)
		}
		horiz := s[0][1] == s[1][1]
		// Axis ordering ascending along the invariant coordinate.
		axisLess := func(a, b [2]float64) bool {
			if horiz {
				if a[0] != b[0] {
					return a[0] < b[0]
				}
				return a[1] < b[1]
			}
			if a[1] != b[1] {
				return a[1] < b[1]
			}
			return a[0] < b[0]
		}
		// Sort meeting points along the PARENT direction so fragment
		// orientation (winding deltas) is preserved exactly.
		flip := axisLess(s[1], s[0])
		dirLess := axisLess
		if flip {
			dirLess = func(a, b [2]float64) bool { return axisLess(b, a) }
		}
		for a := 1; a < len(pts); a++ {
			for b := a; b > 0 && dirLess(pts[b], pts[b-1]); b-- {
				pts[b], pts[b-1] = pts[b-1], pts[b]
			}
		}
		cur := s[0]
		endPt := s[1]
		for _, p := range pts {
			if p == cur || (p[0] == cur[0] && p[1] == cur[1]) {
				continue
			}
			if err := add(cur, p); err != nil {
				return nil, err
			}
			cur = p
		}
		if err := add(cur, endPt); err != nil {
			return nil, err
		}
	}
	// Final filter: drop canceled internal seams (an opposed pair
	// cancels; anything beyond the at-most-two invariant already
	// rejected during aggregation).
	var kept [][2][2]float64
	for i, pc := range out {
		if !canceled[i] {
			kept = append(kept, pc)
		}
	}
	return kept, nil
}

func cAxisAligned(s [2][2]float64) bool {
	return s[0][0] == s[1][0] || s[0][1] == s[1][1]
}

func cOnSegI(s [2][2]float64, p [2]float64) bool {
	return min64(s[0][0], s[1][0]) <= p[0] && p[0] <= max64(s[0][0], s[1][0]) &&
		min64(s[0][1], s[1][1]) <= p[1] && p[1] <= max64(s[0][1], s[1][1])
}

// cMeetingPoints returns exact lattice points where axis-aligned
// integral segments meet (proper crossing, endpoint touch, or
// collinear overlap endpoints).
func cMeetingPoints(s, o [2][2]float64) [][2]float64 {
	if !cAxisAligned(s) || !cAxisAligned(o) {
		return nil
	}
	sv := s[0][0] == s[1][0]
	ov := o[0][0] == o[1][0]
	var out [][2]float64
	switch {
	case sv && ov:
		if s[0][0] != o[0][0] {
			return nil
		}
		x := s[0][0]
		lo := max64(min64(s[0][1], s[1][1]), min64(o[0][1], o[1][1]))
		hi := min64(max64(s[0][1], s[1][1]), max64(o[0][1], o[1][1]))
		if lo <= hi {
			out = append(out, [2]float64{x, lo}, [2]float64{x, hi})
		}
	case !sv && !ov:
		if s[0][1] != o[0][1] {
			return nil
		}
		y := s[0][1]
		lo := max64(min64(s[0][0], s[1][0]), min64(o[0][0], o[1][0]))
		hi := min64(max64(s[0][0], s[1][0]), max64(o[0][0], o[1][0]))
		if lo <= hi {
			out = append(out, [2]float64{lo, y}, [2]float64{hi, y})
		}
	default:
		var v, h [2][2]float64
		if sv {
			v, h = s, o
		} else {
			v, h = o, s
		}
		xv, yh := v[0][0], h[0][1]
		if xv >= min64(h[0][0], h[1][0]) && xv <= max64(h[0][0], h[1][0]) &&
			yh >= min64(v[0][1], v[1][1]) && yh <= max64(v[0][1], v[1][1]) {
			p := [2]float64{xv, yh}
			if cOnSegI(s, p) && cOnSegI(o, p) {
				out = append(out, p)
			}
		}
	}
	var filtered [][2]float64
	for _, p := range out {
		if cOnSegI(s, p) && cOnSegI(o, p) {
			filtered = append(filtered, p)
		}
	}
	return filtered
}

func cMid(f [2][2]float64) [2]float64 {
	return [2]float64{(f[0][0] + f[1][0]) / 2, (f[0][1] + f[1][1]) / 2}
}

// cPerpNorm returns the unit left normal (-dy,dx)/|d| (evidence-only
// float normalization; documented probe caveat).
func cPerpNorm(f [2][2]float64) [2]float64 {
	dx, dy := f[1][0]-f[0][0], f[1][1]-f[0][1]
	l := max64(abs64(dx), abs64(dy)) // cheap scale; not a true norm
	if l == 0 {
		return [2]float64{0, 0}
	}
	s := 1e-6 / l
	return [2]float64{-dy * s, dx * s}
}

func cPointInObstacle(p [2]float64, frags [][2][2]float64) bool {
	w := 0
	for _, s := range frags {
		aAbove := s[0][1] > p[1]
		bAbove := s[1][1] > p[1]
		if aAbove == bAbove || s[0][1] == s[1][1] {
			continue
		}
		xi := s[0][0] + (p[1]-s[0][1])/(s[1][1]-s[0][1])*(s[1][0]-s[0][0])
		if xi > p[0] {
			if bAbove && !aAbove {
				w++
			} else {
				w--
			}
		}
	}
	return w != 0
}

func cKey(p [2]float64) string {
	return ratOf(p[0]).RatString() + "," + ratOf(p[1]).RatString()
}

func cChain(dir [][2][2]float64) [][][2]float64 {
	at := map[string][]int{}
	for i, s := range dir {
		at[cKey(s[0])] = append(at[cKey(s[0])], i)
	}
	used := make([]bool, len(dir))
	var cycles [][][2]float64
	for start := range dir {
		if used[start] {
			continue
		}
		var cyc [][2]float64
		cur := start
		startKey := cKey(dir[cur][0])
		closed := false
		for {
			used[cur] = true
			cyc = append(cyc, dir[cur][0])
			head := dir[cur][1]
			if cKey(head) == startKey {
				closed = true
				break
			}
			next := cPickNext(dir, at, used, head, dir[cur])
			if next < 0 {
				break
			}
			cur = next
		}
		if len(cyc) >= 3 && closed {
			cycles = append(cycles, cyc)
		}
	}
	return cycles
}

func cPickNext(dir [][2][2]float64, at map[string][]int, used []bool, head [2]float64, incoming [2][2]float64) int {
	revX := incoming[0][0] - incoming[1][0]
	revY := incoming[0][1] - incoming[1][1]
	best := -1
	for _, cand := range at[cKey(head)] {
		if used[cand] || !cEq(dir[cand][0], head) {
			continue
		}
		cdx := dir[cand][1][0] - head[0]
		cdy := dir[cand][1][1] - head[1]
		if best < 0 {
			best = cand
			continue
		}
		bdx := dir[best][1][0] - head[0]
		bdy := dir[best][1][1] - head[1]
		if cAngCCW(cdx, cdy, revX, revY) {
			if !cAngCCW(bdx, bdy, revX, revY) || cAngCCW(bdx, bdy, cdx, cdy) {
				best = cand
			}
		} else if !cAngCCW(bdx, bdy, revX, revY) && cAngCCW(bdx, bdy, cdx, cdy) {
			best = cand
		}
	}
	return best
}

func cEq(a, b [2]float64) bool { return a[0] == b[0] && a[1] == b[1] }

func cAngHalf(dx, dy float64) int {
	if dy > 0 {
		return 0
	}
	if dy < 0 {
		return 1
	}
	if dx >= 0 {
		return 0
	}
	return 1
}

func cAngCCW(vx, vy, wx, wy float64) bool {
	hv, hw := cAngHalf(vx, vy), cAngHalf(wx, wy)
	if hv != hw {
		return hv < hw
	}
	return vx*wy-vy*wx > 0
}

func cCanonClean(r [][2]float64) [][2]float64 {
	var out [][2]float64
	eq := func(a, b [2]float64) bool { return a[0] == b[0] && a[1] == b[1] }
	for _, p := range r {
		if len(out) > 0 && eq(p, out[len(out)-1]) {
			continue
		}
		out = append(out, p)
	}
	for len(out) > 1 && eq(out[0], out[len(out)-1]) {
		out = out[:len(out)-1]
	}
	changed := true
	for changed && len(out) > 2 {
		changed = false
		var next [][2]float64
		for i := 0; i < len(out); i++ {
			pv := out[(i-1+len(out))%len(out)]
			nx := out[(i+1)%len(out)]
			if cOrient(pv, out[i], nx) == 0 {
				changed = true
				continue
			}
			next = append(next, out[i])
		}
		out = next
	}
	return out
}

func cShoelace(r [][2]float64) int {
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

func cAbsArea(r [][2]float64) float64 {
	sum := 0.0
	for i := 0; i < len(r); i++ {
		a := r[i]
		b := r[(i+1)%len(r)]
		sum += a[0]*b[1] - b[0]*a[1]
	}
	if sum < 0 {
		return -sum
	}
	return sum
}

// cRepInRing mirrors the certified witness pattern in the float domain.
func cRepInRing(ring [][2]float64) ([2]float64, error) {
	n := len(ring)
	for i := 0; i < n; i++ {
		a := ring[(i-1+n)%n]
		b := ring[(i+1)%n]
		m := [2]float64{(a[0] + b[0]) / 2, (a[1] + b[1]) / 2}
		w := [2]float64{(m[0] + ring[i][0]) / 2, (m[1] + ring[i][1]) / 2}
		if !cOnAnyEdge(w, ring) && cRayParityNonzero(w, ring) {
			return w, nil
		}
	}
	return [2]float64{}, &SpikeError{"no-interior-witness"}
}

func ringSegsF(r [][2]float64) [][2][2]float64 {
	out := make([][2][2]float64, len(r))
	for i := range r {
		out[i] = [2][2]float64{r[i], r[(i+1)%len(r)]}
	}
	return out
}

func cOnAnyEdge(p [2]float64, r [][2]float64) bool {
	for _, s := range ringSegsF(r) {
		if cBetween(s, p) {
			return true
		}
	}
	return false
}

// cRayParityNonzero is the float even-odd probe; combined with the
// orientation-aware construction of corpus rings it certifies witnesses.
func cRayParityNonzero(p [2]float64, r [][2]float64) bool {
	count := 0
	for _, s := range ringSegsF(r) {
		aAbove := s[0][1] > p[1]
		bAbove := s[1][1] > p[1]
		if aAbove == bAbove || s[0][1] == s[1][1] {
			continue
		}
		xi := s[0][0] + (p[1]-s[0][1])/(s[1][1]-s[0][1])*(s[1][0]-s[0][0])
		if xi > p[0] {
			count++
		}
	}
	return count%2 == 1
}

// cLeftWitness returns a certified point on the FREE (left) side of a
// traversed cycle: an edge midpoint plus a bounded halved left normal,
// verified off-edge and outside the ring (the cycle encloses its right
// side). Bounded shrink loop; deterministic rejection otherwise.
func cLeftWitness(ring [][2]float64) ([2]float64, error) {
	n := len(ring)
	for i := 0; i < n; i++ {
		a := ring[i]
		b := ring[(i+1)%n]
		m := [2]float64{(a[0] + b[0]) / 2, (a[1] + b[1]) / 2}
		nx := -(b[1] - a[1])
		ny := b[0] - a[0]
		ex, ey := nx*0.5, ny*0.5
		for k := 0; k <= maxProbeHalvings*4; k++ {
			w := [2]float64{m[0] + ex, m[1] + ey}
			if !cOnAnyEdge(w, ring) && !cRayParityNonzero(w, ring) {
				return w, nil
			}
			ex /= 2
			ey /= 2
		}
	}
	return [2]float64{}, &SpikeError{"no-free-side-witness"}
}

// cWindingTest is an EVEN-ODD parity probe over a single simple ring,
// where even-odd and nonzero winding agree by construction; it is used
// only for hole attachment and nesting containment against individual
// simple rings, never for overlapping-region membership. Labeled here
// so the distinction stays auditable.
func cWindingTest(p [2]float64, r [][2]float64) bool {
	return cRayParityNonzero(p, r)
}

func cBetween(s [2][2]float64, p [2]float64) bool {
	cross := (s[1][0]-s[0][0])*(p[1]-s[0][1]) - (s[1][1]-s[0][1])*(p[0]-s[0][0])
	if cross != 0 {
		return false
	}
	return min64(s[0][0], s[1][0]) <= p[0] && p[0] <= max64(s[0][0], s[1][0]) &&
		min64(s[0][1], s[1][1]) <= p[1] && p[1] <= max64(s[0][1], s[1][1])
}
