package predicates

import (
	"errors"
	"math"
	"math/rand"
	"testing"
)

// Generators produce deterministic corpora across the bit-pattern classes
// that stress binary64 predicates: normals at every exponent scale,
// subnormals, signed zeros, maximal values, and near-degenerate geometric
// constructions.

type coordGen struct {
	rng *rand.Rand
}

func newCoordGen(seed int64) *coordGen {
	return &coordGen{rng: rand.New(rand.NewSource(seed))}
}

// uniformBits produces arbitrary finite bit patterns (normals, subnormals,
// zeros) within the candidate bound.
func (g *coordGen) uniformBits(n int) []float64 {
	out := make([]float64, 0, n)
	for len(out) < n {
		f := math.Float64frombits(g.rng.Uint64())
		if math.IsNaN(f) || math.IsInf(f, 0) || abs(f) > CandidateCoordinateBoundMm {
			continue
		}
		out = append(out, f)
	}
	return out
}

func (g *coordGen) uniformIn(lo, hi float64, n int) []float64 {
	out := make([]float64, n)
	for i := range out {
		out[i] = lo + (hi-lo)*g.rng.Float64()
	}
	return out
}

// smallInts produce integer coordinates where exact ties are common.
func (g *coordGen) smallInts(n int, maxAbs int64) []float64 {
	out := make([]float64, n)
	for i := range out {
		out[i] = float64(g.rng.Int63n(2*maxAbs+1) - maxAbs)
	}
	return out
}

// powScales produce mantissas times powers of two across the full normal
// and subnormal exponent range, clamped to the candidate bound.
func (g *coordGen) powScales(n int) []float64 {
	out := make([]float64, 0, n)
	for len(out) < n {
		mant := 1 + g.rng.Float64()
		exp := g.rng.Intn(2098) - 1074
		f := math.Ldexp(mant, exp)
		if math.IsInf(f, 0) || abs(f) > CandidateCoordinateBoundMm {
			continue
		}
		out = append(out, f)
	}
	return out
}

func checkOrientation(t *testing.T, ax, ay, bx, by, cx, cy float64) {
	t.Helper()
	got, err := Orientation(Point{ax, ay}, Point{bx, by}, Point{cx, cy})
	if err != nil {
		t.Fatalf("Orientation(%g,%g,%g,%g,%g,%g) unexpected error: %v",
			ax, ay, bx, by, cx, cy, err)
	}
	want := oracleOrient(t, ax, ay, bx, by, cx, cy)
	if signToOracle(got) != want {
		t.Fatalf("Orientation mismatch at (%g,%g)(%g,%g)(%g,%g): got %v want %d",
			ax, ay, bx, by, cx, cy, got, want)
	}
}

func TestOrientationAgainstOracle(t *testing.T) {
	cases := []struct {
		name string
		gen  func() [][6]float64
	}{
		{"uniform-bits", func() [][6]float64 {
			g := newCoordGen(101)
			v := g.uniformBits(6000)
			var out [][6]float64
			for i := 0; i+5 < len(v); i += 6 {
				out = append(out, [6]float64{v[i], v[i+1], v[i+2], v[i+3], v[i+4], v[i+5]})
			}
			return out
		}},
		{"small-ints", func() [][6]float64 {
			g := newCoordGen(102)
			v := g.smallInts(3000, 50)
			var out [][6]float64
			for i := 0; i+5 < len(v); i += 6 {
				out = append(out, [6]float64{v[i], v[i+1], v[i+2], v[i+3], v[i+4], v[i+5]})
			}
			return out
		}},
		{"power-scales", func() [][6]float64 {
			g := newCoordGen(103)
			v := g.powScales(3600)
			var out [][6]float64
			for i := 0; i+5 < len(v); i += 6 {
				out = append(out, [6]float64{v[i], v[i+1], v[i+2], v[i+3], v[i+4], v[i+5]})
			}
			return out
		}},
		{"near-degenerate", func() [][6]float64 {
			g := newCoordGen(104)
			var out [][6]float64
			for i := 0; i < 2000; i++ {
				ax, ay := float64(g.rng.Int63n(1000)), float64(g.rng.Int63n(1000))
				bx, by := float64(g.rng.Int63n(1000)), float64(g.rng.Int63n(1000))
				eps := g.powScales(1)[0]
				// c on segment ab plus a tiny dyadic perturbation: forces
				// the filter into its inconclusive band constantly.
				cx := ax + eps*(bx-ax)
				cy := ay + eps*(by-ay)
				if !finiteWithin(cx, cy) {
					continue
				}
				out = append(out, [6]float64{ax, ay, bx, by, cx, cy})
			}
			return out
		}},
	}
	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			for _, c := range tc.gen() {
				checkOrientation(t, c[0], c[1], c[2], c[3], c[4], c[5])
			}
		})
	}
}

func finiteWithin(fs ...float64) bool {
	for _, f := range fs {
		if math.IsNaN(f) || math.IsInf(f, 0) || abs(f) > CandidateCoordinateBoundMm {
			return false
		}
	}
	return true
}

// TestOracleRejectsDecimalSubstitution proves the oracle evaluates STORED
// values, not decimal literals. The decimal determinant of
// (0.01, 0.03, 0.05, 0.15) is EXACTLY zero because 0.01*0.15 == 0.03*0.05
// in decimal arithmetic. Over stored binary64 values the exact determinant
// is strictly nonzero (negative), so any oracle that silently substitutes
// decimal rationals for stored values would report SignZero here.
func TestOracleRejectsDecimalSubstitution(t *testing.T) {
	const a, b, c, d = 0.01, 0.03, 0.05, 0.15
	got := oracleOrient(t, 0, 0, c, d, a, b) // det = c*b - d*a over stored values
	if got == 0 {
		t.Fatal("oracle reported exact zero: it evaluated decimal literals," +
			" not stored binary64 values")
	}
	checkOrientation(t, 0, 0, c, d, a, b)
}

// naiveFailureVectors are FIXED adversarial cases where the unguarded float
// determinant returns exactly 0.0 while the exact determinant over the
// stored dyadic values is strictly negative.
//
// Provenance: ORIGINAL fixtures generated by a deterministic lattice
// construction searched offline with seed 20260823 within bounded attempts
// (<300000 trials; three representative hits recorded here of six found; full
// docs/evidence/0006-predicates/FINDINGS.md). No external test data was
// copied. Construction: direction components near 2^k scaled by powers of
// two so stored coordinates stay dyadic-exact while the two naive products
// exceed 53 significant bits and round to the same value.
var naiveFailureVectors = [][6]float64{
	{0.0, 0.0, 536870908.0, 536870912.0, 536870912.0, 536870916.0},
	{0.0, 0.0, 33554432.25, 33554432.0, 33554432.0, 33554431.75},
	{0.0, 0.0, 134217727.75, 134217728.0, 134217728.0, 134217728.25},
}

// TestNaiveFloatFailsAdversarial demonstrates that unguarded float
// evaluation misclassifies reachable inputs inside the candidate domain:
// for each fixed vector the naive determinant collapses to zero ("exact
// collinearity") while the true sign is strictly negative. The adaptive
// predicate must classify all of them correctly. Correctness assertions
// only; no timing or attempt-count sensitivity.
func TestNaiveFloatFailsAdversarial(t *testing.T) {
	for i, v := range naiveFailureVectors {
		if !finiteWithin(v[:]...) {
			t.Fatalf("vector %d out of policy", i)
		}
		want := oracleOrient(t, v[0], v[1], v[2], v[3], v[4], v[5])
		gotNaive := signToOracle(naiveOrient(v[0], v[1], v[2], v[3], v[4], v[5]))
		if gotNaive == want {
			t.Fatalf("vector %d no longer discriminates: naive=%d exact=%d", i, gotNaive, want)
		}
		if gotNaive != 0 {
			t.Fatalf("vector %d: expected naive collapse to zero, got %d", i, gotNaive)
		}
		checkOrientation(t, v[0], v[1], v[2], v[3], v[4], v[5])
	}
}

func TestOrientationFixedHardVectors(t *testing.T) {
	// Hand-built families at the domain edges; all values are finite and
	// within the candidate bound.
	vectors := [][6]float64{
		// Maximal-magnitude coordinates with unit-scale differences.
		{CandidateCoordinateBoundMm, CandidateCoordinateBoundMm,
			CandidateCoordinateBoundMm - 1, CandidateCoordinateBoundMm,
			CandidateCoordinateBoundMm - 1, CandidateCoordinateBoundMm - 1},
		// Subnormal-bearing triangle.
		{0, 0, math.SmallestNonzeroFloat64, 0, 0, math.SmallestNonzeroFloat64},
		// Behavior-spec rational family fixture shape: direction from a
		// 9-40-41 triple scaled into the bound (900/41 and 4000/41 are not
		// exactly representable; their stored values must classify exactly).
		{0, 0, 900.0 / 41.0, 4000.0 / 41.0, 900.0 / 41.0, 4000.0/41.0 + math.Ldexp(1, -30)},
		// Signed zeros everywhere: degenerate collinear regardless of signs.
		{math.Copysign(0, -1), math.Copysign(0, -1), 0, 1, 1, math.Copysign(0, -1)},
		// Exact tie requiring the fallback: midpoint construction.
		{0, 0, 4, 0, 2, math.Ldexp(1, -800)},
	}
	for i, v := range vectors {
		if !finiteWithin(v[:]...) {
			t.Fatalf("vector %d out of policy: %v", i, v)
		}
		checkOrientation(t, v[0], v[1], v[2], v[3], v[4], v[5])
	}
}

func TestOrientationSignedZeroCanonical(t *testing.T) {
	zeroPos := 0.0
	zeroNeg := math.Copysign(0, -1)
	s1, err := Orientation(Point{zeroNeg, zeroNeg}, Point{0, 1}, Point{1, zeroNeg})
	if err != nil {
		t.Fatal(err)
	}
	s2, err := Orientation(Point{zeroPos, zeroPos}, Point{0, 1}, Point{1, zeroPos})
	if err != nil {
		t.Fatal(err)
	}
	if s1 != s2 || s1 != SignNegative {
		t.Fatalf("signed zero changed orientation or wrong sign: %v vs %v", s1, s2)
	}
}

func TestOrientationInvariances(t *testing.T) {
	g := newCoordGen(555)

	t.Run("cyclic-permutation", func(t *testing.T) {
		for i := 0; i < 500; i++ {
			v := g.smallInts(6, 1<<28)
			a := Point{v[0], v[1]}
			b := Point{v[2], v[3]}
			c := Point{v[4], v[5]}
			s1 := mustOrient(t, a, b, c)
			if s1 != mustOrient(t, b, c, a) || s1 != mustOrient(t, c, a, b) {
				t.Fatalf("cyclic permutation changed sign: %v %v %v", a, b, c)
			}
			if r := mustOrient(t, b, a, c); r != negSign(s1) {
				t.Fatalf("reversal did not negate sign: %v %v %v", a, b, c)
			}
		}
	})

	t.Run("power-of-two-scaling", func(t *testing.T) {
		for i := 0; i < 500; i++ {
			v := g.smallInts(6, 1<<26)
			scale := math.Ldexp(1, g.rng.Intn(20)-10) // 2^-10..2^9
			scaled := make([]float64, 6)
			ok := true
			for j, x := range v {
				scaled[j] = x * scale
				if !finiteWithin(scaled[j]) {
					ok = false
					break
				}
			}
			if !ok {
				continue
			}
			base := mustOrient(t, Point{v[0], v[1]}, Point{v[2], v[3]}, Point{v[4], v[5]})
			got := mustOrient(t, Point{scaled[0], scaled[1]}, Point{scaled[2], scaled[3]}, Point{scaled[4], scaled[5]})
			if base != got {
				t.Fatalf("power-of-two scaling changed implementation sign: %d vs %d", base, got)
			}
			baseOracle := oracleOrient(t, v[0], v[1], v[2], v[3], v[4], v[5])
			if signToOracle(base) != baseOracle || signToOracle(got) != baseOracle {
				t.Fatalf("implementation/oracle disagreement under scaling")
			}
		}
	})

	t.Run("dyadic-translation", func(t *testing.T) {
		for i := 0; i < 500; i++ {
			v := g.smallInts(6, 1<<26)
			dx, dy := 1e6, -2e6
			sBase := mustOrient(t, Point{v[0], v[1]}, Point{v[2], v[3]}, Point{v[4], v[5]})
			sTrans := mustOrient(t,
				Point{v[0] + dx, v[1] + dy},
				Point{v[2] + dx, v[3] + dy},
				Point{v[4] + dx, v[5] + dy})
			if sBase != sTrans {
				t.Fatalf("integer translation changed exact sign")
			}
		}
	})
}

func mustOrient(t *testing.T, a, b, c Point) Sign {
	t.Helper()
	s, err := Orientation(a, b, c)
	if err != nil {
		t.Fatalf("Orientation error: %v", err)
	}
	return s
}

func negSign(s Sign) Sign {
	switch s {
	case SignPositive:
		return SignNegative
	case SignNegative:
		return SignPositive
	default:
		return SignZero
	}
}

func TestOrientationPolicyErrors(t *testing.T) {
	bound := CandidateCoordinateBoundMm
	cases := []struct {
		name string
		p    [3]Point
		want error
	}{
		{"nan-x", [3]Point{{math.NaN(), 0}, {1, 0}, {0, 1}}, ErrNonFinite},
		{"inf-y", [3]Point{{0, math.Inf(-1)}, {1, 0}, {0, 1}}, ErrNonFinite},
		{"oob-positive", [3]Point{{bound * 2, 0}, {1, 0}, {0, 1}}, ErrOutOfBounds},
		{"oob-negative", [3]Point{{0, 0}, {-bound * 1.0000001, 0}, {0, 1}}, ErrOutOfBounds},
		// Bit-pattern maxima class: the largest finite binary64 values are
		// far outside the candidate bound and must be rejected by policy,
		// not classified geometrically.
		{"max-finite", [3]Point{{math.MaxFloat64, 0}, {0, math.MaxFloat64 / 2}, {0, 1}}, ErrOutOfBounds},
		{"max-finite-neg", [3]Point{{-math.MaxFloat64, -math.MaxFloat64}, {1, 0}, {0, 1}}, ErrOutOfBounds},
	}
	for _, tc := range cases {
		_, err := Orientation(tc.p[0], tc.p[1], tc.p[2])
		if !errors.Is(err, tc.want) {
			t.Fatalf("%s: want %v got %v", tc.name, tc.want, err)
		}
	}
	// Invalid input must never yield a Sign decision disguised as zero.
	_, err := Orientation(Point{math.NaN(), 0}, Point{1, 0}, Point{0, 1})
	if err == nil {
		t.Fatal("NaN input produced no error")
	}
}

func TestIntersectPolicyErrors(t *testing.T) {
	p := Point{0, 0}
	q := Point{10, 10}
	cases := []struct {
		name           string
		p1, p2, q1, q2 Point
		want           error
	}{
		{"degenerate-p", p, p, Point{0, 1}, Point{5, 6}, ErrDegenerateSegment},
		{"degenerate-q", p, q, q, q, ErrDegenerateSegment},
		{"both-degenerate", p, p, q, q, ErrDegenerateSegment},
		{"nonfinite", Point{math.NaN(), 0}, p, q, Point{1, 2}, ErrNonFinite},
		{"oob", Point{CandidateCoordinateBoundMm + 1, 0}, p, q, Point{1, 2}, ErrOutOfBounds},
	}
	for _, tc := range cases {
		res, err := IntersectSegments(tc.p1, tc.p2, tc.q1, tc.q2)
		if !errors.Is(err, tc.want) {
			t.Fatalf("%s: want %v got %v", tc.name, tc.want, err)
		}
		// F1: the returned result must carry the unambiguous ClassInvalid
		// sentinel, never a valid geometric class.
		if res.Class != ClassInvalid {
			t.Fatalf("%s: invalid input yielded %v, want ClassInvalid", tc.name, res.Class)
		}
	}
}

// TestOrientationExactPathZeros regression: zero and signed-zero coordinates
// must traverse the exact scaled-integer path without panicking after the
// filter gate routes them out (C11).
func TestOrientationExactPathZeros(t *testing.T) {
	zeroNeg := math.Copysign(0, -1)
	vectors := [][6]float64{
		{0, 0, 1, 0, 0, 1},
		{zeroNeg, zeroNeg, 1, 0, 0, 1},
		{5, 5, zeroNeg, 2, 3, zeroNeg},
		{0, 0, 0, 0, 1, 1}, // degenerate: a==b==origin
	}
	for i, v := range vectors {
		if !finiteWithin(v[:]...) {
			t.Fatalf("vector %d out of policy", i)
		}
		checkOrientation(t, v[0], v[1], v[2], v[3], v[4], v[5])
	}
}

// TestPublishedAdversarialVectors exercises the two in-bound coordinate
// triples transcribed from a primary publication: Bartels, Fisikopoulos,
// Weiser, "Fast floating-point filters for robust predicates", BIT
// Numerical Mathematics 63, article 31 (2023), version of record published
// 2023-05-17, DOI 10.1007/s10543-023-00975-x (open access, CC BY 4.0);
// full text verified via arXiv:2208.00497v1. Only the numeric coordinate
// values are used, with attribution; no paper prose, figures, algorithms,
// or code are reproduced (see docs/evidence/0006-predicates/PROVENANCE.md,
// entry P-3).
//
// Domowik semantics decide the EXACT sign over STORED binary64 values
// (NUMERICAL-MODEL.md section 7 conformance protocol). The paper frames its
// decimal example against real arithmetic on the decimal literals; that
// idealized collinearity is deliberately NOT asserted here, because stored
// binary64 approximations of those decimals are not exactly collinear.
func TestPublishedAdversarialVectors(t *testing.T) {
	// naiveDet mirrors the ungated float formula of filterSign; it is used
	// only to document each vector's naive-float failure mode.
	naiveDet := func(a, b, c Point) float64 {
		return (b.X-a.X)*(c.Y-a.Y) - (b.Y-a.Y)*(c.X-a.X)
	}

	// Vector A (paper section 2 near-collinear decimal example):
	// a = (-0.01,-0.59), b = (0.01,0.57), c = (0,-0.01). Exactly collinear
	// as decimal reals, but the stored binary64 triple is not. Naive float
	// evaluation disagrees with the exact stored-value sign in every
	// observed formulation: both plain groupings collapse to a spurious
	// zero, and an FMA-contracted evaluation yields a strictly negative
	// determinant (computed here through math.FMA so the observation is
	// deterministic on every platform, complementing FINDINGS.md section
	// 2's compiler-fusion record).
	a := Point{X: -0.01, Y: -0.59}
	b := Point{X: 0.01, Y: 0.57}
	c := Point{X: 0, Y: -0.01}
	acx, acy := a.X-c.X, a.Y-c.Y
	bcx, bcy := b.X-c.X, b.Y-c.Y
	if nd := acx*bcy - bcx*acy; nd != 0 {
		t.Fatalf("unfused paper-grouping determinant %v not zero; failure-mode drift", nd)
	}
	if fd := math.FMA(acx, bcy, -(bcx * acy)); !(fd < 0) {
		t.Fatalf("FMA-contracted determinant %v not negative; failure-mode drift", fd)
	}
	if nd := naiveDet(a, b, c); nd != 0 {
		t.Fatalf("filter-grouping naive determinant %v not zero; failure-mode drift", nd)
	}
	if _, ok := filterSign(a, b, c); ok {
		t.Fatal("vector A decided by filter; expected exact-path escalation")
	}
	gotA, err := Orientation(a, b, c)
	if err != nil {
		t.Fatal(err)
	}
	if gotA != SignPositive {
		t.Fatalf("vector A exact sign = %v, want positive", gotA)
	}

	// Vector B (paper section 2 underflow example): powers-of-two triangle
	// ((2^-801,2^-801),(2^-800,2^-800),(2^-801,2^-800)). The true
	// determinant t^2 with t = 2^-801 underflows binary64 to zero in the
	// naive formula, while the exact sign is positive.
	tt := math.Pow(2, -801)
	u := Point{X: tt, Y: tt}
	v := Point{X: 2 * tt, Y: 2 * tt}
	w := Point{X: tt, Y: 2 * tt}
	if nd := naiveDet(u, v, w); nd != 0 {
		t.Fatalf("naive determinant %v not zero; failure-mode drift", nd)
	}
	if _, ok := filterSign(u, v, w); ok {
		t.Fatal("vector B decided by filter; expected exact-path escalation")
	}
	gotB, err := Orientation(u, v, w)
	if err != nil {
		t.Fatal(err)
	}
	if gotB != SignPositive {
		t.Fatalf("vector B exact sign = %v, want positive", gotB)
	}
}
