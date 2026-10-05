package geometry

import "math/big"

// Oracle planar face extraction by exact vertical decomposition.
//
// The plane is cut by vertical lines through every distinct fragment
// endpoint x. Within each open slab every boundary fragment either
// does not appear or spans the slab completely; cells are the open
// regions between vertically adjacent spanning fragments (with open
// bottom/top extents). Cell connectivity across a cut line is decided
// by evaluating the two adjacent cells' exact y-ranges AT the cut
// line: they share a face iff their open ranges overlap with positive
// length there. Point-only contact yields zero-length overlap and
// never merges, realizing R5.1.2's open-set pinch semantics
// structurally. Diagonal and quadratic-field boundaries evaluate
// exactly at representative x values, so irrational-bearing walls are
// handled without approximation.
//
// Independence note: this traversal shares no logic with the candidate
// sweep engine or candidate DCEL face walker beyond primitive exact
// arithmetic. Residual shared-family assumptions are recorded in the
// evidence document.

// xiv is an open y-interval with optional infinite ends; nil lo/hi are
// never read while the corresponding inf flag is set.
type xiv struct {
	lo, hi       xnum
	loInf, hiInf bool
}

func ivCmpLo(a, b xiv) int {
	switch {
	case a.loInf && b.loInf:
		return 0
	case a.loInf:
		return -1
	case b.loInf:
		return 1
	}
	return a.lo.Sub(b.lo).Sign()
}

func sortIvs(ivs []xiv) {
	for i := 1; i < len(ivs); i++ {
		for j := i; j > 0 && ivCmpLo(ivs[j], ivs[j-1]) < 0; j-- {
			ivs[j], ivs[j-1] = ivs[j-1], ivs[j]
		}
	}
}

// mergeClosedIv merges lo-sorted intervals treating contact as
// connected (the occupied region is a closed set).
func mergeClosedIv(ivs []xiv) []xiv {
	out := make([]xiv, 0, len(ivs))
	for _, v := range ivs {
		if len(out) > 0 {
			c := &out[len(out)-1]
			touch := c.hiInf || v.loInf ||
				(!c.hiInf && !v.loInf && c.hi.Sub(v.lo).Sign() >= 0)
			if touch {
				if !c.hiInf {
					if v.hiInf || (!v.hiInf && v.hi.Sub(c.hi).Sign() > 0) {
						c.hi, c.hiInf = v.hi, v.hiInf
					}
				}
				continue
			}
		}
		out = append(out, v)
	}
	return out
}

// complementOfClosed returns the open complement of merged closed
// intervals. An empty input yields exactly one whole-line interval.
func complementOfClosed(merged []xiv) []xiv {
	if len(merged) == 0 {
		return []xiv{{loInf: true, hiInf: true}}
	}
	var out []xiv
	curLoInf := true
	var curHi xnum
	for _, m := range merged {
		out = append(out, xiv{lo: curHi, loInf: curLoInf, hi: m.lo, hiInf: m.loInf})
		curLoInf = m.hiInf
		curHi = m.hi
	}
	out = append(out, xiv{lo: curHi, loInf: curLoInf, hiInf: true})
	return out
}

// SpikeError marks deterministic spike-local rejection conditions; it
// is not a ratified diagnostic token.
type SpikeError struct{ Code string }

func (e *SpikeError) Error() string { return "geometry spike: " + e.Code }

// cellT is one trapezoid cell bounded vertically by fragment indices
// (-1 = open bottom/top extent).
type cellT struct{ lo, hi int }

// ocls carries one union-find class's representative point.
type ocls struct{ rep oPt }

// OFace is one oracle-extracted face in canonical form.
type OFace struct {
	Outer  oRing   // CCW, rotated to least vertex; nil for the exterior
	Holes  []oRing // CW, canonically rotated and sorted
	Parent int     // index into Faces whose hole encloses this face; -1 none
}

// OArrangement is the oracle decomposition of the free space.
type OArrangement struct {
	Faces    []OFace // bounded faces in canonical total order (R5.1.3)
	Exterior OFace   // unique unbounded component; Holes are CW obstacle loops
}

// scaleDown returns v * r computed in v's own field (r rational).
func scaleDown(v xnum, r *big.Rat) xnum {
	switch t := v.(type) {
	case ratNum:
		return xr(new(big.Rat).Mul(t.v, r))
	case quadNum:
		return quadNum{
			p: new(big.Rat).Mul(t.p, r),
			q: new(big.Rat).Mul(t.q, r),
			d: t.d,
		}
	}
	panic("geometry: unknown exact field")
}

// yAt evaluates the fragment's y at the exact vertical line x=c. The
// caller guarantees c lies within the fragment's closed x-span; a
// zero-width fragment is an invariant violation and fails
// deterministically instead of dividing by zero.
func yAt(s oSeg, c xnum) xnum {
	dx := s.dx()
	if dx.Sign() == 0 {
		panic("geometry: yAt on zero-width fragment")
	}
	t := c.Sub(s.a.x).Div(dx)
	return s.a.y.Add(s.dy().Mul(t))
}

// OracleFaces extracts the faces of the complement of obstacle
// boundary segments. All segments must lie in one exact field.
func OracleFaces(segs []oSeg) (OArrangement, error) {
	if len(segs) == 0 {
		return OArrangement{}, nil // empty level: bare exterior (R5.1.6)
	}
	frags := splitAll(segs)
	xs := distinctXs(frags)
	if len(xs) < 2 {
		return OArrangement{}, &SpikeError{"degenerate-arrangement"}
	}
	ymin, ymax := bboxY(frags)
	openLo := func() xnum { return ymin.Sub(xone(ymin)) }
	openHi := func() xnum { return ymax.Add(xone(ymax)) }

	// Slabs: index 0 is the left outer slab (-inf, xs[0]), indices
	// 1..len(xs)-1 are the finite slabs, and index n-1 is the right
	// outer slab (xs[last], +inf). Outer slabs carry a single
	// whole-plane cell so free space connects around every obstacle.
	n := len(xs) + 1
	spans := make([][]int, n) // fragment indices spanning each slab
	for fi, f := range frags {
		ax, bx := f.a.x, f.b.x
		if cmpX(ax, bx) > 0 {
			ax, bx = bx, ax
		}
		if cmpX(ax, bx) == 0 || f.dx().Sign() == 0 {
			// Vertical or zero-length fragments live on cut lines and
			// never span an open slab.
			continue
		}
		for i := 1; i < n-1; i++ {
			if cmpX(ax, xs[i-1]) <= 0 && cmpX(bx, xs[i]) >= 0 {
				spans[i] = append(spans[i], fi)
			}
		}
	}
	cells := make([][]cellT, n)
	cells[0] = []cellT{{lo: -1, hi: -1}}
	cells[n-1] = []cellT{{lo: -1, hi: -1}}
	for i := 1; i < n-1; i++ {
		xm := mid(xs[i-1], xs[i])
		fs := append([]int{}, spans[i]...)
		// Insertion sort by exact y at the slab's representative x;
		// ties fall back to index order deterministically.
		for a := 1; a < len(fs); a++ {
			for b := a; b > 0 && yAt(frags[fs[b]], xm).Sub(yAt(frags[fs[b-1]], xm)).Sign() < 0; b-- {
				fs[b], fs[b-1] = fs[b-1], fs[b]
			}
		}
		prev := -1
		for _, f := range fs {
			cells[i] = append(cells[i], cellT{lo: prev, hi: f})
			prev = f
		}
		cells[i] = append(cells[i], cellT{lo: prev, hi: -1})
	}

	// Union-find across cut lines using exact y-range overlap of the
	// two cells' open y-extents at the joining line. All unbounded
	// flags are handled before any value is read; no nil xnum is ever
	// used as a finite endpoint.
	total := 0
	ids := make([][]int, n)
	for i := 0; i < n; i++ {
		for range cells[i] {
			ids[i] = append(ids[i], total)
			total++
		}
	}
	// Exact occupied cross-sections and free complements per cut
	// line: vertical fragments contribute full y-intervals, crossing/
	// ending nonvertical fragments contribute exact points. Cells may
	// connect across a line only through a FREE interval of that line,
	// so walls lying on a cut line block open-set connectivity.
	lineFree := make([][]xiv, len(xs))
	for k, c := range xs {
		var ivs []xiv
		for _, f := range frags {
			if f.dx().Sign() == 0 {
				if f.a.x.Sub(c).Sign() == 0 {
					lo, hi := f.a.y, f.b.y
					if lo.Sub(hi).Sign() > 0 {
						lo, hi = hi, lo
					}
					ivs = append(ivs, xiv{lo: lo, hi: hi})
				}
				continue
			}
			ax, bx := f.a.x, f.b.x
			sf := f
			if cmpX(ax, bx) > 0 {
				ax, bx = bx, ax
				sf = oSeg{f.b, f.a}
			}
			if cmpX(ax, c) <= 0 && cmpX(c, bx) <= 0 {
				y := yAt(sf, c)
				ivs = append(ivs, xiv{lo: y, hi: y})
			}
		}
		sortIvs(ivs)
		lineFree[k] = complementOfClosed(mergeClosedIv(ivs))
	}

	uf := newUnionFind(total)
	cellLoHi := func(i, g int, c xnum) (lo, hi xnum, loInf, hiInf bool) {
		cl := cells[i][g]
		if cl.lo >= 0 {
			lo = yAt(frags[cl.lo], c)
		} else {
			loInf = true
		}
		if cl.hi >= 0 {
			hi = yAt(frags[cl.hi], c)
		} else {
			hiInf = true
		}
		return
	}
	maxXnum := func(a, b xnum) xnum {
		if b.Sub(a).Sign() > 0 {
			return b
		}
		return a
	}
	minXnum := func(a, b xnum) xnum {
		if b.Sub(a).Sign() < 0 {
			return b
		}
		return a
	}
	for j := 0; j+1 < n; j++ {
		// Joining cut line between slab j and slab j+1.
		c := xs[j]
		free := lineFree[j]
		for g := range cells[j] {
			loL, hiL, loInfL, hiInfL := cellLoHi(j, g, c)
			for h := range cells[j+1] {
				loR, hiR, loInfR, hiInfR := cellLoHi(j+1, h, c)
				// Overlap bounds: lower = max of lowers (-inf iff both
				// are -inf), upper = min of uppers (+inf iff both are).
				// Unbounded sides never feed arithmetic; a nil xnum is
				// never used as a finite endpoint.
				loInf := loInfL && loInfR
				hiInf := hiInfL && hiInfR
				var lo, hi xnum
				if !loInf {
					switch {
					case loInfL:
						lo = loR
					case loInfR:
						lo = loL
					default:
						lo = maxXnum(loL, loR)
					}
				}
				if !hiInf {
					switch {
					case hiInfL:
						hi = hiR
					case hiInfR:
						hi = hiL
					default:
						hi = minXnum(hiL, hiR)
					}
				}
				// Cells connect only through a FREE interval of the cut
				// line overlapping their shared range with positive
				// length: walls lying on a cut line block open-set
				// connectivity even for whole-line outer cells.
				//
				// Intersection bounds: lower = max of lowers (-inf iff
				// both are), upper = min of uppers (+inf iff both are).
				// One-sided infinity never implies positivity by
				// itself: disjoint intervals like (-inf,0) and
				// (12,+inf) must compare their finite sides.
				pos := false
				for _, fiv := range free {
					if ivIntersectPositive(lo, hi, loInf, hiInf, fiv) {
						pos = true
						break
					}
				}
				if pos {
					uf.union(ids[j][g], ids[j+1][h])
				}
			}
		}
	}

	classOfCell := func(i, g int) int { return uf.find(ids[i][g]) }
	grid := &slabGrid{xs: xs, cells: cells, frags: frags, class: classOfCell, openLo: openLo, openHi: openHi}

	// Free-class registration: a class is registered only through a
	// cell that provably carries open free area. Skipped cells do NOT
	// mark their class, so a later legitimate cell can still register
	// it. Zero-height pseudo-cells (coincident collinear fragments at
	// butt seams) and occupied cells (e.g. a closed rectangle's
	// interior column) own no free-space face.
	classes := map[int]*ocls{}
	registered := map[int]bool{}
	for i := 0; i < n; i++ {
		xm := grid.cellX(i)
		for g := range cells[i] {
			id := classOfCell(i, g)
			if _, done := classes[id]; done {
				continue
			}
			cl := cells[i][g]
			if cl.lo >= 0 && cl.hi >= 0 &&
				yAt(frags[cl.lo], xm).Sub(yAt(frags[cl.hi], xm)).Sign() >= 0 {
				continue // zero-height pseudo-cell
			}
			rep := grid.cellRep(i, g)
			inObs, err := probeState(rep, frags)
			if err != nil {
				return OArrangement{}, err
			}
			if inObs == 1 {
				continue // occupied cell
			}
			classes[id] = &ocls{rep: rep}
			registered[id] = true
		}
	}

	// Explicit far-field seed: an exact point beyond the bounding box,
	// classified through the same cell machinery as any other probe.
	seedX := xs[len(xs)-1].Add(xone(xs[len(xs)-1]))
	seedY := ymax.Add(xone(ymax))
	extID, err := grid.classAt(oPt{x: seedX, y: seedY})
	if err != nil {
		return OArrangement{}, err
	}
	if !registered[extID] {
		return OArrangement{}, &SpikeError{"seed-cell-not-free"}
	}

	// Boundary fragments oriented with the free side on their left.
	directed, err := orientedFragments(frags, grid.classAt)
	if err != nil {
		return OArrangement{}, err
	}
	perFace := map[int][]oSeg{}
	for _, d := range directed {
		perFace[d.face] = append(perFace[d.face], d.s)
	}

	build := func(id int) (OFace, error) {
		cycles := chainCycles(perFace[id])
		fc := OFace{Parent: -1}
		var holes []oRing
		outers := 0
		for _, c := range cycles {
			cleaned := canonCleanRing(c)
			if len(cleaned) < 3 || shoelaceSign(cleaned) == 0 {
				continue
			}
			cleaned = canonRotate(cleaned)
			if shoelaceSign(cleaned) > 0 {
				outers++
				fc.Outer = cleaned
			} else {
				holes = append(holes, cleaned)
			}
		}
		if id == extID {
			if outers > 0 {
				return OFace{}, &SpikeError{"exterior-with-positive-cycle"}
			}
			fc.Outer = nil
		} else if outers != 1 {
			return OFace{}, &SpikeError{"face-outer-count"}
		}
		sortHoles(holes)
		fc.Holes = holes
		return fc, nil
	}

	arr := OArrangement{}
	var bounded []indexedFace
	for id := range classes {
		fc, err := build(id)
		if err != nil {
			return OArrangement{}, err
		}
		if id == extID {
			arr.Exterior = fc
			continue
		}
		bounded = append(bounded, indexedFace{id: id, f: fc})
	}

	// Parent assignment before sorting, recorded as class IDs and
	// remapped to final canonical indices afterwards.
	parentID := map[int]int{}
	for i := range bounded {
		for j := range bounded {
			if i == j {
				continue
			}
			inside := false
			for _, hr := range bounded[j].f.Holes {
				if ptInRingsWinding(classes[bounded[i].id].rep, ringSegs(hr)) {
					inside = true
					break
				}
			}
			if inside {
				if _, dup := parentID[bounded[i].id]; dup {
					return OArrangement{}, &SpikeError{"two-parents"}
				}
				parentID[bounded[i].id] = bounded[j].id
			}
		}
	}
	sortFaces(bounded)
	oldToNew := map[int]int{}
	for newI, b := range bounded {
		oldToNew[b.id] = newI
	}
	for i := range bounded {
		bounded[i].f.Parent = -1
		if pid, ok := parentID[bounded[i].id]; ok {
			bounded[i].f.Parent = oldToNew[pid]
		}
	}
	for _, b := range bounded {
		arr.Faces = append(arr.Faces, b.f)
	}
	return arr, nil
}

// slabGrid maps an exact probe point to its free-cell class.
type slabGrid struct {
	xs     []xnum
	cells  [][]cellT
	frags  []oSeg
	class  func(i, g int) int
	openLo func() xnum
	openHi func() xnum
}

// classAt locates the slab containing p.x — clamped into the outer
// slabs — then the cell whose exact y-range at p.x contains p.y.
// Probes are tiny boundary nudges, so strict containment holds.
func (sg *slabGrid) classAt(p oPt) (int, error) {
	// Slab indexing: 0 = left outer, len(cells)-1 = right outer,
	// finite slab j covers (xs[j-1], xs[j]).
	nSlabs := len(sg.cells)
	k := nSlabs - 1
	if cmpX(p.x, sg.xs[0]) < 0 {
		k = 0
	} else {
		for i := 0; i+1 < len(sg.xs); i++ {
			if cmpX(sg.xs[i], p.x) <= 0 && cmpX(p.x, sg.xs[i+1]) < 0 {
				k = i + 1
				break
			}
		}
	}
	for g, cl := range sg.cells[k] {
		ok := true
		if cl.lo >= 0 {
			if yAt(sg.frags[cl.lo], p.x).Sub(p.y).Sign() >= 0 {
				ok = false // p.y at or below the lower bound
			}
		}
		if ok && cl.hi >= 0 {
			if p.y.Sub(yAt(sg.frags[cl.hi], p.x)).Sign() >= 0 {
				ok = false // p.y at or above the upper bound
			}
		}
		if ok {
			return sg.class(k, g), nil
		}
	}
	return 0, &SpikeError{"probe-outside-cells"}
}

// cellX returns the representative x of slab i (bounding-box extremes
// for the outer slabs).
func (sg *slabGrid) cellX(i int) xnum {
	switch {
	case i == 0:
		return sg.xs[0].Sub(xone(sg.xs[0]))
	case i == len(sg.cells)-1:
		return sg.xs[len(sg.xs)-1].Add(xone(sg.xs[len(sg.xs)-1]))
	default:
		return mid(sg.xs[i-1], sg.xs[i])
	}
}

// cellRep returns a representative point strictly inside cell (i, g):
// the slab's representative x with the exact mid-y of the cell's open
// extent there (bounding-box extremes for open sides).
func (sg *slabGrid) cellRep(i, g int) oPt {
	cl := sg.cells[i][g]
	xm := sg.cellX(i)
	var yl, yh xnum
	if cl.lo >= 0 {
		yl = yAt(sg.frags[cl.lo], xm)
	} else {
		yl = sg.openLo()
	}
	if cl.hi >= 0 {
		yh = yAt(sg.frags[cl.hi], xm)
	} else {
		yh = sg.openHi()
	}
	return oPt{x: xm, y: mid(yl, yh)}
}

type dirFrag struct {
	s    oSeg
	face int
}

// orientedFragments orients each subdivided boundary fragment so its
// free side is on the left, assigning it to the owning face.
//
// Nudge parameterization: offset ±eps·(-dy,dx)/(2·max(|dx|,|dy|)) has
// perpendicular displacement within [eps/2, eps/sqrt(2)], computed
// entirely in-field; eps derives from the bounding box (see
// bboxEps). Corpus feature separation dominates eps; violations fail
// deterministically instead of misclassifying silently.
func orientedFragments(frags []oSeg, classAt func(oPt) (int, error)) ([]dirFrag, error) {
	eps, err := bboxEps(frags)
	if err != nil {
		return nil, err
	}
	var out []dirFrag
	for _, f := range frags {
		ff := f
		inLb, inRb, lastEps, err := probeWithRetryB(eps,
			func(e xnum) (oPt, oPt) { return nudgeProbes(ff, e) },
			func(pl, pr oPt) (bool, bool, error) {
				a, err := probeState(pl, frags)
				if err != nil {
					return false, false, err
				}
				b, err := probeState(pr, frags)
				if err != nil {
					return false, false, err
				}
				return a == 1, b == 1, nil
			})
		if err != nil {
			return nil, err
		}
		inL, inR := 0, 0
		if inLb {
			inL = 1
		}
		if inRb {
			inR = 1
		}
		if inL == inR {
			// Both probes land on the same side of the occupied
			// region: the fragment is interior to the union (butt-joint
			// seams) or exterior residue and owns no free-space
			// boundary. Drop it deterministically.
			continue
		}
		s := f
		// The probe points are rebuilt at the final epsilon for the
		// face lookup; recompute them from the winning epsilon.
		pL, pR := nudgeProbes(f, lastEps)
		freeP := pL
		if inL == 1 {
			s = oSeg{f.b, f.a} // reverse so the free right side is left
			freeP = pR
		}
		fid, err := classAt(freeP)
		if err != nil {
			return nil, err
		}
		out = append(out, dirFrag{s: s, face: fid})
	}
	return out, nil
}

// splitAll subdivides segments at every mutual meeting point.
func splitAll(segs []oSeg) []oSeg {
	var out []oSeg
	for i := range segs {
		params := []xnum{}
		for j := range segs {
			if i == j {
				continue
			}
			params = append(params, intersectParams(segs[i], segs[j])...)
		}
		ps := sortXnums(dedupeXnums(strictInterior(params)))
		cur := segs[i]
		for _, t := range ps {
			p := pointAt(segs[i], t)
			out = append(out, oSeg{cur.a, p})
			cur = oSeg{p, cur.b}
		}
		out = append(out, cur)
	}
	return out
}

type indexedFace struct {
	id int
	f  OFace
}

func faceLess(a, b OFace) bool {
	if c := compareRings(a.Outer, b.Outer); c != 0 {
		return c < 0
	}
	n := len(a.Holes)
	if len(b.Holes) < n {
		n = len(b.Holes)
	}
	for i := 0; i < n; i++ {
		if c := compareRings(a.Holes[i], b.Holes[i]); c != 0 {
			return c < 0
		}
	}
	return len(a.Holes) < len(b.Holes)
}

// compareRings compares canonically rotated rings lexicographically;
// prefix-shorter compares less (R3.2.4).
func compareRings(a, b oRing) int {
	a = canonRotate(a)
	b = canonRotate(b)
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if !a[i].eq(b[i]) {
			if oLess(a[i], b[i]) {
				return -1
			}
			return 1
		}
	}
	switch {
	case len(a) < len(b):
		return -1
	case len(a) > len(b):
		return 1
	}
	return 0
}

func canonRotate(r oRing) oRing {
	least := 0
	for i := 1; i < len(r); i++ {
		if oLess(r[i], r[least]) {
			least = i
		}
	}
	out := make(oRing, 0, len(r))
	out = append(out, r[least:]...)
	out = append(out, r[:least]...)
	return out
}

func sortFaces(fs []indexedFace) {
	for i := 1; i < len(fs); i++ {
		for j := i; j > 0 && faceLess(fs[j].f, fs[j-1].f); j-- {
			fs[j], fs[j-1] = fs[j-1], fs[j]
		}
	}
}

func distinctXs(segs []oSeg) []xnum {
	var xs []xnum
	add := func(x xnum) {
		for _, e := range xs {
			if x.Sub(e).Sign() == 0 {
				return
			}
		}
		xs = append(xs, x)
	}
	for _, s := range segs {
		add(s.a.x)
		add(s.b.x)
	}
	return sortXnums(xs) // sortXnums returns a new sorted slice
}

func bboxY(segs []oSeg) (ymin, ymax xnum) {
	ymin, ymax = segs[0].a.y, segs[0].a.y
	for _, s := range segs {
		for _, y := range []xnum{s.a.y, s.b.y} {
			if y.Sub(ymin).Sign() < 0 {
				ymin = y
			}
			if y.Sub(ymax).Sign() > 0 {
				ymax = y
			}
		}
	}
	return
}

func cmpX(a, b xnum) int { return a.Sub(b).Sign() }

type unionFind struct {
	p []int
}

func newUnionFind(n int) *unionFind {
	p := make([]int, n)
	for i := range p {
		p[i] = i
	}
	return &unionFind{p: p}
}

func (u *unionFind) find(i int) int {
	for u.p[i] != i {
		u.p[i] = u.p[u.p[i]]
		i = u.p[i]
	}
	return i
}

func (u *unionFind) union(a, b int) {
	ra, rb := u.find(a), u.find(b)
	if ra != rb {
		u.p[maxInt(ra, rb)] = minInt(ra, rb)
	}
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// ivIntersectPositive reports whether the open intersection of the
// range (lo,hi) — whose infinite ends are flagged, with nil xnum never
// read on an infinite side — and the open interval f has positive
// length. Intersection bounds: lower = max of lowers (-inf iff both
// are), upper = min of uppers (+inf iff both are). One-sided infinity
// never implies positivity by itself: (-inf,0) ∩ (12,+inf) is empty.
func maxXnum(a, b xnum) xnum {
	if b.Sub(a).Sign() > 0 {
		return b
	}
	return a
}

func minXnum(a, b xnum) xnum {
	if b.Sub(a).Sign() < 0 {
		return b
	}
	return a
}

func ivIntersectPositive(lo, hi xnum, loInf, hiInf bool, f xiv) bool {
	floInf := loInf && f.loInf
	fhiInf := hiInf && f.hiInf
	var flo, fhi xnum
	if !floInf {
		switch {
		case loInf:
			flo = f.lo
		case f.loInf:
			flo = lo
		default:
			flo = maxXnum(lo, f.lo)
		}
	}
	if !fhiInf {
		switch {
		case hiInf:
			fhi = f.hi
		case f.hiInf:
			fhi = hi
		default:
			fhi = minXnum(hi, f.hi)
		}
	}
	switch {
	case floInf && fhiInf:
		return true // (-inf, +inf)
	case floInf:
		return true // (-inf, finite): always positive length
	case fhiInf:
		return true // (finite, +inf): always positive length
	default:
		return fhi.Sub(flo).Sign() > 0
	}
}
