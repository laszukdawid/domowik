package predicates

import (
	"math"
	"math/big"
	"testing"
)

// High-precision reference machinery for EMPIRICAL VALIDATION of the
// section 4.2 gate claims and section 4.4 certificates.
//
// DIRECTED-ROUNDING REFERENCE (review findings C5/C6): the exact unit
// direction n* = (dx,dy)/|(dx,dy)| is enclosed in an axis-aligned box
// computed entirely with big.Float directed rounding (SetMode
// ToNegativeInf / ToPositiveInf) at refPrec bits: every operation rounds
// outward on its side, so n* provably lies inside the box. For any
// committed point c,
//
//	err = ||c - n*||_2 <= max over box corners of cornerDistUB(c, corner),
//
// where cornerDistUB bounds the corner distance using interval endpoints
// for each |difference| (both rounding directions) and upward-only
// squaring/addition/sqrt. No heuristic margins are used anywhere. This is
// a finite-precision MEASUREMENT reference with declared precision and
// rounding; it is NOT the discrete oracle (predicates_oracle_test.go).

const refPrec = 512

type dirBox struct {
	xlo, xhi, ylo, yhi *big.Float
}

func (b dirBox) corners() [][2]*big.Float {
	return [][2]*big.Float{
		{b.xlo, b.ylo}, {b.xlo, b.yhi}, {b.xhi, b.ylo}, {b.xhi, b.yhi},
	}
}

// dirEnclosure returns an outward box enclosure of n*(dx,dy) using proper
// interval arithmetic (review finding C13): the radicand sum gets its own
// lower/upper directed endpoints, the square root is taken downward from
// the lower endpoint and upward from the upper endpoint, and each signed
// division uses the numerator over the OPPOSITE denominator endpoint so
// monotonicity carries the bound through the reciprocal. Numerators are
// exact (float64 -> big.Float at refPrec is lossless); only the
// denominator chain requires enclosure.
func dirEnclosure(t *testing.T, dx, dy float64) dirBox {
	t.Helper()
	sq := func(f float64) (lo, hi *big.Float) {
		fx := new(big.Float).SetPrec(refPrec).SetFloat64(f)
		lo = new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Mul(fx, fx)
		hi = new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Mul(fx, fx)
		return lo, hi
	}
	sxLo, sxHi := sq(dx)
	syLo, syHi := sq(dy)

	sumLo := new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Add(sxLo, syLo)
	sumHi := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Add(sxHi, syHi)

	// Directed square roots of the radicand interval endpoints. Note:
	// Float.Sqrt mutates its receiver, so fresh copies are used.
	denLo := new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Sqrt(new(big.Float).SetPrec(refPrec).Set(sumLo))
	denHi := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Sqrt(new(big.Float).SetPrec(refPrec).Set(sumHi))

	// divDirected: a/b rounded outward across b's interval according to a's
	// sign.
	divDirected := func(a float64, bLo, bHi *big.Float) (lo, hi *big.Float) {
		fa := new(big.Float).SetPrec(refPrec).SetFloat64(a)
		if fa.Signbit() {
			// negative / larger-denominator -> less negative (upper);
			// negative / smaller-denominator -> more negative (lower)
			lo = new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Quo(fa, bLo)
			hi = new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Quo(fa, bHi)
		} else {
			lo = new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Quo(fa, bHi)
			hi = new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Quo(fa, bLo)
		}
		return lo, hi
	}

	xlo, xhi := divDirected(dx, denLo, denHi)
	ylo, yhi := divDirected(dy, denLo, denHi)
	box := dirBox{xlo: xlo, xhi: xhi, ylo: ylo, yhi: yhi}
	assertEnclosureOrdered(t, box)
	return box
}

func assertEnclosureOrdered(t *testing.T, b dirBox) {
	t.Helper()
	if b.xlo.Cmp(b.xhi) > 0 || b.ylo.Cmp(b.yhi) > 0 {
		t.Fatal("directed enclosure endpoints out of order")
	}
	slack := powTwoRat(-500)
	one := new(big.Rat).SetInt64(1)
	hiLimit := new(big.Rat).Add(one, slack)
	loLimit := new(big.Rat).Sub(new(big.Rat), hiLimit)
	for name, f := range map[string]*big.Float{"xlo": b.xlo, "xhi": b.xhi, "ylo": b.ylo, "yhi": b.yhi} {
		r := ratExactT(f)
		if r.Cmp(loLimit) < 0 || r.Cmp(hiLimit) > 0 {
			t.Fatalf("enclosure endpoint %s outside unit band: %v", name, r)
		}
	}
}

// ratExact converts a finite big.Float to its exact rational value.
func ratExact(t *testing.T, f *big.Float) *big.Rat {
	t.Helper()
	r, acc := f.Rat(nil)
	if r == nil || acc != big.Exact {
		t.Fatalf("value not exactly representable as a rational")
	}
	return r
}

// absBoundOfDiff returns max(|fl_down(a-b)|, |fl_up(a-b)|), an exact
// dyadic bound of the true |a-b|: the true difference lies between the two
// directed endpoints, so its absolute value is at most the larger endpoint
// magnitude. Negation/abs on a finite big.Float is exact.
func absBoundOfDiff(a float64, b *big.Float) *big.Rat {
	fa := new(big.Float).SetPrec(refPrec).SetFloat64(a)
	lo := new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Sub(fa, b)
	hi := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Sub(fa, b)
	absLo := new(big.Float).Abs(lo)
	absHi := new(big.Float).Abs(hi)
	if absLo.Cmp(absHi) >= 0 {
		return ratExactT(absLo)
	}
	return ratExactT(absHi)
}

func ratExactT(f *big.Float) *big.Rat {
	r, acc := f.Rat(nil)
	if r == nil || acc != big.Exact {
		panic("directed endpoint not exactly representable")
	}
	return r
}

// cornerDistUB returns an exact rational upper bound of the Euclidean
// distance from committed point (cx,cy) to reference point (px,py):
// differences bounded as above, squares/addition/sqrt rounded upward only.
func cornerDistUB(t *testing.T, cx, cy float64, px, py *big.Float) *big.Rat {
	t.Helper()
	modeUp := big.ToPositiveInf

	ax := absBoundOfDiff(cx, px)
	ay := absBoundOfDiff(cy, py)

	// C12: the difference bounds may need more than refPrec bits to be
	// exact; convert with UPWARD rounding so the squared chain stays a
	// proven upper bound (both values are non-negative).
	axF := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).SetRat(ax)
	ayF := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).SetRat(ay)
	sqx := new(big.Float).SetPrec(refPrec).SetMode(modeUp).Mul(axF, axF)
	sqy := new(big.Float).SetPrec(refPrec).SetMode(modeUp).Mul(ayF, ayF)
	sum := new(big.Float).SetPrec(refPrec).SetMode(modeUp).Add(sqx, sqy)
	dist := sum.Sqrt(sum)
	return ratExact(t, dist)
}

// errBound returns a rigorous upper bound of ||committed - n*||_2 over the
// enclosure box corners.
func errBound(t *testing.T, nx, ny float64, box dirBox) *big.Rat {
	t.Helper()
	worst := new(big.Rat)
	for _, c := range box.corners() {
		d := cornerDistUB(t, nx, ny, c[0], c[1])
		if d.Cmp(worst) > 0 {
			worst = d
		}
	}
	return worst
}

func TestCertificateConstantsHaveSaneMagnitude(t *testing.T) {
	// Guard against accidental giant "certificates" (review finding B1).
	// Analytic targets: sqrt(2)*gamma_5 ~= 1.57e-15; sqrt(2)*gamma_3 ~=
	// 9.44e-16; fallback base ~= sqrt(2)*5/(2^256-5) ~ 4.4e-77.
	bound := func(lo, hi string, r *big.Rat, name string) {
		t.Helper()
		l, ok := new(big.Rat).SetString(lo)
		if !ok {
			t.Fatalf("bad literal %s", lo)
		}
		h, _ := new(big.Rat).SetString(hi)
		if r.Cmp(l) <= 0 || r.Cmp(h) >= 0 {
			t.Fatalf("%s out of expected range (%s, %s): %v", name, lo, hi, r)
		}
	}
	bound("1e-16", "1e-14", fastDirCert, "fastDirCert")
	bound("1e-16", "1e-14", outwardSqrtOverInt(18, gamma3Den), "gamma3Sqrt2")
	bound("1e-80", "1e-70", fallbackBaseCert, "fallbackBaseCert")

	if etaHalfRat.Cmp(powTwoRat(-1075)) != 0 {
		t.Fatal("etaHalfRat wrong")
	}
}

// TestHalfULPRatContract pins the corrected half-ulp formula (review
// finding C9): half ulp at 1.0 is 2^-53; at 2.0 it is 2^-52; zero and
// subnormals use eta/2 = 2^-1075.
func TestHalfULPRatContract(t *testing.T) {
	cases := []struct {
		f    float64
		want int // exponent of two
	}{
		{1, -53},
		{2, -52},
		{4, -51},
		{0.5, -54},
		{math.Ldexp(1, -1022), -1075}, // smallest normal: ilogb -1022 -> 2^-1075
		{math.Ldexp(1, -1074), -1075}, // eta: subnormal -> eta/2
		{math.Ldexp(3, -1074), -1075}, // subnormal -> eta/2
		{0, -1075},
	}
	for _, tc := range cases {
		want := powTwoRat(tc.want)
		got := halfULPRat(tc.f)
		if got.Cmp(want) != 0 {
			t.Fatalf("halfULPRat(%v) = %v want %v", tc.f, got, want)
		}
	}
}

func TestUnitDirectionPathsAndPolicy(t *testing.T) {
	t.Run("typical-fast", func(t *testing.T) {
		d, err := UnitDirection(3, 4)
		if err != nil {
			t.Fatal(err)
		}
		if d.Path != PathGatedFast {
			t.Fatalf("path %v", d.Path)
		}
		if d.NX != 0.6 || d.NY != 0.8 {
			t.Fatalf("n=(%v,%v)", d.NX, d.NY)
		}
	})

	t.Run("extreme-kappa-fallback", func(t *testing.T) {
		// Extreme exponent ratio within the provisional difference domain
		// 2B: 2^30 ~ 1.07e9 <= 2B, delta = 940 >> 97.
		d, err := UnitDirection(math.Ldexp(1, 30), math.Ldexp(1, -910))
		if err != nil {
			t.Fatal(err)
		}
		if d.Path != PathHighPrecisionFallback {
			t.Fatalf("extreme kappa took fast path %v", d.Path)
		}
	})

	t.Run("mixed-scale-subnormal-minor-fallback", func(t *testing.T) {
		d, err := UnitDirection(math.Ldexp(1, -500), math.Ldexp(1, -1074))
		if err != nil {
			t.Fatal(err)
		}
		if d.Path != PathHighPrecisionFallback {
			t.Fatalf("mixed-scale pair took fast path %v", d.Path)
		}
	})

	t.Run("axis-aligned-fallback-exact", func(t *testing.T) {
		d, err := UnitDirection(2.5, 0)
		if err != nil {
			t.Fatal(err)
		}
		if d.Path != PathHighPrecisionFallback || d.NX != 1 || d.NY != 0 {
			t.Fatalf("axis: path %v n=(%v,%v)", d.Path, d.NX, d.NY)
		}
	})

	t.Run("policy-errors", func(t *testing.T) {
		if _, err := UnitDirection(0, 0); !isErr(err, ErrZeroDirection) {
			t.Fatalf("zero vector: %v", err)
		}
		if _, err := UnitDirection(math.NaN(), 1); !isErr(err, ErrNonFinite) {
			t.Fatalf("NaN accepted: %v", err)
		}
		if _, err := UnitDirection(1, math.Inf(1)); !isErr(err, ErrNonFinite) {
			t.Fatalf("Inf accepted: %v", err)
		}
	})

	t.Run("endpoint-difference-domain", func(t *testing.T) {
		d, err := UnitDirection(2*CandidateCoordinateBoundMm, 0)
		if err != nil {
			t.Fatalf("difference-bound edge rejected: %v", err)
		}
		if d.NX != 1 {
			t.Fatalf("n=(%v,%v)", d.NX, d.NY)
		}
		// F6: beyond the provisional 2B difference domain is out of policy.
		if _, err := UnitDirection(math.Nextafter(2*CandidateCoordinateBoundMm, math.Inf(1)), 0); !isErr(err, ErrOutOfBounds) {
			t.Fatalf("over-2B accepted: %v", err)
		}
		if _, err := UnitDirection(0, -3*CandidateCoordinateBoundMm); !isErr(err, ErrOutOfBounds) {
			t.Fatalf("negative over-2B accepted: %v", err)
		}
	})
}

func isErr(err, target error) bool {
	if err == nil {
		return false
	}
	for e := err; e != nil; {
		if e == target {
			return true
		}
		u, ok := e.(interface{ Unwrap() error })
		if !ok {
			return false
		}
		e = u.Unwrap()
	}
	return false
}

// dirCase pairs source inputs with the constructed Direction so offset
// validation compares against the ORIGINAL algebraic ideal (C6).
type dirCase struct {
	dx, dy float64
	dir    Direction
}

func buildDirCase(t *testing.T, dx, dy float64) dirCase {
	t.Helper()
	d, err := UnitDirection(dx, dy)
	if err != nil {
		t.Fatalf("(%v,%v): %v", dx, dy, err)
	}
	return dirCase{dx: dx, dy: dy, dir: d}
}

func TestGateConsistency(t *testing.T) {
	g := newCoordGen(4242)
	for i := 0; i < 2000; i++ {
		var dx, dy float64
		switch g.rng.Intn(3) {
		case 0:
			v := g.uniformIn(-1e6, 1e6, 2)
			dx, dy = v[0], v[1]
		case 1:
			v := g.powScales(2)
			dx, dy = v[0], v[1]
		default:
			dx = math.Ldexp(1+g.rng.Float64(), g.rng.Intn(140)-60)
			dy = math.Ldexp(1+g.rng.Float64(), g.rng.Intn(140)-60)
		}
		if (dx == 0 && dy == 0) || math.IsNaN(dx) || math.IsNaN(dy) ||
			math.IsInf(dx, 0) || math.IsInf(dy, 0) ||
			abs(dx) > 2*CandidateCoordinateBoundMm || abs(dy) > 2*CandidateCoordinateBoundMm {
			continue
		}
		dc := buildDirCase(t, dx, dy)
		delta := abs(float64(math.Ilogb(abs(dx)) - math.Ilogb(abs(dy))))
		if dx == 0 || dy == 0 {
			if dc.dir.Path != PathHighPrecisionFallback {
				t.Fatalf("zero component took fast path")
			}
			continue
		}
		if delta > fastPathMaxExpDelta && dc.dir.Path != PathHighPrecisionFallback {
			t.Fatalf("delta %.0f took fast path", delta)
		}
		validateDirCert(t, dc)
	}
}

// validateDirCert asserts cert >= the rigorous directed-rounding error
// bound over the enclosure of the exact direction of the SOURCE inputs.
func validateDirCert(t *testing.T, dc dirCase) {
	t.Helper()
	box := dirEnclosure(t, dc.dx, dc.dy)
	bound := errBound(t, dc.dir.NX, dc.dir.NY, box)
	if bound.Cmp(dc.dir.Cert) > 0 {
		t.Fatalf("direction certificate violated for (%v,%v): bound=%v cert=%v",
			dc.dx, dc.dy, bound, dc.dir.Cert)
	}
}

func TestDirectionCertificateValidityCorpus(t *testing.T) {
	g := newCoordGen(90210)
	// All magnitudes respect the provisional COMPONENTWISE difference
	// domain |component| <= 2B (the norm may reach 2*sqrt(2)*B).
	cases := []dirCase{
		buildDirCase(t, 3, 4),
		buildDirCase(t, math.Ldexp(1, 29), math.Ldexp(1, 27)),             // high kappa in-band
		buildDirCase(t, math.Ldexp(1, -500), math.Ldexp(1, -1074)),        // mixed scale w/ subnormal minor
		buildDirCase(t, math.Ldexp(12345, -900), math.Ldexp(6789, -1074)), // subnormal minors
		buildDirCase(t, math.Ldexp(1, 28), math.Ldexp(1, -100)),           // large/small split
		buildDirCase(t, CandidateCoordinateBoundMm, CandidateCoordinateBoundMm-1),
		buildDirCase(t, -2*CandidateCoordinateBoundMm, CandidateCoordinateBoundMm), // componentwise edges valid (F6)
	}
	for i := 0; i < 300; i++ {
		v := g.powScales(2)
		if math.IsNaN(v[0]) || math.IsNaN(v[1]) ||
			math.IsInf(v[0], 0) || math.IsInf(v[1], 0) ||
			abs(v[0]) > 2*CandidateCoordinateBoundMm || abs(v[1]) > 2*CandidateCoordinateBoundMm {
			continue
		}
		if v[0] == 0 && v[1] == 0 {
			continue
		}
		cases = append(cases, buildDirCase(t, v[0], v[1]))
	}
	for _, dc := range cases {
		validateDirCert(t, dc)
	}
}

func TestOffsetCertificateValidity(t *testing.T) {
	g := newCoordGen(31337)

	points := []Point{
		{0, 0},
		{1, 2},
		{CandidateCoordinateBoundMm, -CandidateCoordinateBoundMm},
		{math.Ldexp(1, -1030), math.Ldexp(3, -1040)},
	}
	scales := []float64{
		math.Ldexp(1, -1074), // eta: minimal thickness t = 2*eta
		math.Ldexp(2, -1074), // 2*eta
		smallestNormal,       // 2^-1022
		1e-300,
		0.5,
		1e6,
	}
	dirs := []dirCase{
		buildDirCase(t, 3, 4),
		buildDirCase(t, 1, 0),
		buildDirCase(t, math.Ldexp(1, -500), math.Ldexp(1, -1074)),
		buildDirCase(t, -7, 24),
	}
	for i := 0; i < 50; i++ {
		v := g.powScales(2)
		if v[0] == 0 && v[1] == 0 {
			continue
		}
		dirs = append(dirs, buildDirCase(t, v[0], v[1]))
	}

	count := 0
	for _, p := range points {
		for _, s := range scales {
			for _, dc := range dirs {
				for _, sigma := range []int{-1, 1} {
					count++
					validateOffsetCert(t, p, s, sigma, dc)
				}
			}
		}
	}
	t.Logf("validated %d offset certificates", count)
}

// validateOffsetCert compares the committed vertex against an outward box
// around the IDEAL vertex built from the ORIGINAL (dx,dy) inputs through
// the directed enclosure of n*, so certificate terms 3 and 5 (direction
// error and its cross term with rounded s) are genuinely exercised rather
// than trivially satisfied (C6).
func validateOffsetCert(t *testing.T, p Point, s float64, sigma int, dc dirCase) {
	t.Helper()
	bounds, err := OffsetVertexCertificate(p, s, sigma, dc.dir)
	if err != nil {
		t.Fatalf("certificate build failed: %v", err)
	}
	v, err := OffsetVertex(p, s, sigma, dc.dir)
	if err != nil {
		t.Fatalf("vertex commit failed: %v", err)
	}

	box := dirEnclosure(t, dc.dx, dc.dy)
	sigS := float64(sigma) * s

	vxLo, vxHi := vertexAxisBox(box.xlo, box.xhi, sigS, p.X)
	vyLo, vyHi := vertexAxisBox(box.ylo, box.yhi, sigS, p.Y)
	corners := [][2]*big.Float{
		{vxLo, vyLo}, {vxLo, vyHi}, {vxHi, vyLo}, {vxHi, vyHi},
	}

	worst := new(big.Rat)
	for _, c := range corners {
		d := cornerDistUB(t, v.X, v.Y, c[0], c[1])
		if d.Cmp(worst) > 0 {
			worst = d
		}
	}
	if worst.Cmp(bounds.Eps) > 0 {
		t.Fatalf("offset certificate violated: P=%v s=%v sigma=%d path=%v\n bound=%v\n eps=%v",
			p, s, sigma, dc.dir.Path, worst, bounds.Eps)
	}
}

// vertexAxisBox returns the outward interval of P_i + sigS*n*_i given the
// directed enclosure endpoints of n*_i. All four signed products are taken
// with both rounding modes and min/max selected, then P added outward.
func vertexAxisBox(nLo, nHi *big.Float, sigS float64, pComp float64) (*big.Float, *big.Float) {
	ss := [2]*big.Float{
		new(big.Float).SetPrec(refPrec).SetFloat64(sigS), // exact conversion
		new(big.Float).SetPrec(refPrec).SetFloat64(sigS),
	}
	ns := [2]*big.Float{nLo, nHi}

	var minP, maxP *big.Float
	for i := 0; i < 2; i++ {
		for j := 0; j < 2; j++ {
			prodLo := new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Mul(ss[i], ns[j])
			prodHi := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Mul(ss[i], ns[j])
			for _, cand := range []*big.Float{prodLo, prodHi} {
				if minP == nil || cand.Cmp(minP) < 0 {
					minP = cand
				}
				if maxP == nil || cand.Cmp(maxP) > 0 {
					maxP = cand
				}
			}
		}
	}

	pLo := new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).SetFloat64(pComp)
	pHi := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).SetFloat64(pComp)
	vLo := new(big.Float).SetPrec(refPrec).SetMode(big.ToNegativeInf).Add(pLo, minP)
	vHi := new(big.Float).SetPrec(refPrec).SetMode(big.ToPositiveInf).Add(pHi, maxP)
	return vLo, vHi
}

func TestOffsetBoundaryValidationNoPanic(t *testing.T) {
	good := buildDirCase(t, 3, 4).dir
	badNilCert := Direction{NX: good.NX, NY: good.NY, Path: PathGatedFast}
	badPath := Direction{NX: good.NX, NY: good.NY, Path: DirectionPath(99), Cert: good.Cert}
	badNaN := Direction{NX: math.NaN(), NY: good.NY, Path: PathGatedFast, Cert: good.Cert}
	negCert := Direction{NX: good.NX, NY: good.NY, Path: PathGatedFast,
		Cert: new(big.Rat).Neg(good.Cert)}
	forgedZero := Direction{NX: 0, NY: 0, Path: PathGatedFast, Cert: new(big.Rat)}
	forgedScaled := Direction{NX: 2, NY: 2, Path: PathGatedFast,
		Cert: new(big.Rat).SetInt64(0)}
	// Supersedes the F3 clamp regression: a loose certificate (Cert >= 1)
	// clamps the lower squared-norm band to zero and admits forged
	// degenerate components, so validateDirection must reject ANY
	// certificate >= 1 regardless of how consistent the pair looks.
	looseZeroForged := Direction{NX: 0, NY: 0, Path: PathGatedFast,
		Cert: big.NewRat(2, 1)}
	looseConsistent := Direction{NX: 1, NY: 0, Path: PathHighPrecisionFallback,
		Cert: new(big.Rat).SetInt64(3)}

	p := Point{1, 2}
	cases := []struct {
		name    string
		p       Point
		s       float64
		sigma   int
		d       Direction
		wantErr bool
	}{
		{"nil-cert", p, 1, 1, badNilCert, true},
		{"bad-path", p, 1, 1, badPath, true},
		{"nan-nx", p, 1, 1, badNaN, true},
		{"negative-cert", p, 1, 1, negCert, true},
		{"forged-zero-direction", p, 1, 1, forgedZero, true},
		{"forged-scaled-direction", p, 1, 1, forgedScaled, true},
		{"forged-zero-with-loose-cert", p, 1, 1, looseZeroForged, true},
		{"loose-certificate-rejected", p, 1, 1, looseConsistent, true},
		{"sigma-zero", p, 1, 0, good, true},
		{"sigma-two", p, 1, 2, good, true},
		{"negative-s", p, -1, 1, good, true},
		{"nan-s", p, math.NaN(), 1, good, true},
		{"oob-p", Point{CandidateCoordinateBoundMm + 1, 2}, 1, 1, good, true},
		// F4 regression, updated for the Cert<1 tightening: a FORGED
		// direction that is still inside the structural band (NX=1.5,
		// Cert=1/2 gives sq=2.25=(1+C)^2) is accepted by
		// validateDirection, which cannot authenticate provenance.
		// MaxFloat64*NX then overflows, so BOTH APIs must fail with the
		// derived-vertex error rather than commit a non-finite vertex.
		// This validates only the defensive commit guard against
		// untrusted structurally-valid input; it does NOT make
		// Direction a ratified or authenticated public API (see the
		// issue-0030 ratification record).
		{"overflowing-forged-band-valid",
			Point{CandidateCoordinateBoundMm, 0}, math.MaxFloat64, 1,
			Direction{NX: 1.5, NY: 0, Path: PathGatedFast,
				Cert: big.NewRat(1, 2)}, true},
		{"valid", p, 0.5, -1, good, false},
	}
	for _, tc := range cases {
		func() {
			defer func() {
				if r := recover(); r != nil {
					t.Fatalf("%s: panicked: %v", tc.name, r)
				}
			}()
			_, cerr := OffsetVertexCertificate(tc.p, tc.s, tc.sigma, tc.d)
			_, verr := OffsetVertex(tc.p, tc.s, tc.sigma, tc.d)
			// F4: certificate and vertex APIs must AGREE on failure.
			if tc.wantErr && (cerr == nil || verr == nil) {
				t.Fatalf("%s: expected both to error, cert=%v vertex=%v", tc.name, cerr, verr)
			}
			if !tc.wantErr && (cerr != nil || verr != nil) {
				t.Fatalf("%s: unexpected error cert=%v vertex=%v", tc.name, cerr, verr)
			}
		}()
	}
}

// TestLegitimateCertificatesBelowOne asserts that every certificate
// produced by UnitDirection across its corpus is strictly below 1 and
// structurally accepted, so the Cert < 1 rejection can never exclude a
// legitimately constructed direction. Narrow regression for the forged
// loose-certificate fix; it does NOT make validateDirection a provenance
// or authentication check (see the issue-0030 ratification record).
func TestLegitimateCertificatesBelowOne(t *testing.T) {
	one := big.NewRat(1, 1)
	cases := []dirCase{
		buildDirCase(t, 3, 4),
		buildDirCase(t, 1, 0),
		buildDirCase(t, math.Ldexp(1, -500), math.Ldexp(1, -1074)),
		buildDirCase(t, CandidateCoordinateBoundMm, CandidateCoordinateBoundMm-1),
	}
	g := newCoordGen(424242)
	for i := 0; i < 200; i++ {
		v := g.powScales(2)
		if v[0] == 0 && v[1] == 0 {
			continue
		}
		if abs(v[0]) > 2*CandidateCoordinateBoundMm || abs(v[1]) > 2*CandidateCoordinateBoundMm {
			continue
		}
		cases = append(cases, buildDirCase(t, v[0], v[1]))
	}
	for _, dc := range cases {
		if dc.dir.Cert == nil {
			t.Fatalf("direction (%v,%v): missing certificate", dc.dx, dc.dy)
		}
		if dc.dir.Cert.Cmp(one) >= 0 {
			t.Fatalf("direction (%v,%v): legitimate certificate %s >= 1", dc.dx, dc.dy, dc.dir.Cert)
		}
		if err := validateDirection(dc.dir); err != nil {
			t.Fatalf("direction (%v,%v): legitimate direction rejected: %v", dc.dx, dc.dy, err)
		}
	}
}
