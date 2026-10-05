package predicates

import (
	"fmt"
	"math"
	"math/big"
)

// DirectionPath records which construction path produced a Direction.
type DirectionPath uint8

const (
	// PathGatedFast is the section 4.2 fast path: joint lossless
	// power-of-two scaling followed by gated binary64 squaring/addition/
	// root/division, certified by the gamma_5 derivation.
	PathGatedFast DirectionPath = iota
	// PathHighPrecisionFallback is the escalation route for bypassed or
	// gate-failing directions. SPIKE LIMITATION: NUMERICAL-MODEL.md section
	// 6 prescribes canonical multiquadratic evaluation (section 4.3) for
	// this rung; that machinery is not implemented in this spike. The
	// fallback instead computes the direction in 256-bit big.Float
	// arithmetic with an outward-rational error certificate, which is
	// evidence-grade for the gate/certificate obligations but is NOT full
	// section 4.3 compliance.
	PathHighPrecisionFallback DirectionPath = iota + 1
)

func (p DirectionPath) String() string {
	switch p {
	case PathGatedFast:
		return "gated-fast"
	case PathHighPrecisionFallback:
		return "high-precision-fallback"
	default:
		return fmt.Sprintf("invalid DirectionPath(%d)", uint8(p))
	}
}

// ErrZeroDirection reports a direction vector with both components zero.
var ErrZeroDirection = fmt.Errorf("predicates: zero-length direction")

// ErrDerivedVertexInvalid reports a committed derived vertex that is not a
// finite binary64 value. PROVISIONAL diagnostic name; the ratified token is
// owned by later ratification (cf. NUMERICAL-MODEL.md section 9).
var ErrDerivedVertexInvalid = fmt.Errorf("predicates: derived vertex not finite")

// Direction is a committed binary64 unit direction plus an OUTWARD rational
// certificate: Cert >= the true Euclidean distance between the committed
// pair (NX, NY) and the exact algebraic direction (dx,dy)/|(dx,dy)| over
// the stored inputs. The certificate is a guaranteed upper bound, never an
// estimate.
type Direction struct {
	NX, NY float64
	Path   DirectionPath
	Cert   *big.Rat
}

// Direction gate policy constants follow NUMERICAL-MODEL.md section 4.2.
// This implementation applies them CONSERVATIVELY through binary exponent
// differences: a direction takes the fast path only when
// emax(|dx|,|dy|)-emin(|dx|,|dy|) <= 97, which provably implies both
// kappa_dir <= 2^998 and the minor-component ratio >= 2^-1000 conditions.
// Borderline directions escalate instead of gambling on an exact-gate
// reimplementation, so the fast path is never used outside the certified
// domain (soundness over completeness of the fast path).
const fastPathMaxExpDelta = 97

// fallbackPrec is the big.Float precision of the fallback path.
const fallbackPrec = 256

// smallestNormal is 2^-1022, the smallest positive normal binary64.
const smallestNormal = 2.2250738585072014e-308

// UnitDirection constructs the committed binary64 unit direction of
// (dx, dy) per NUMERICAL-MODEL.md section 4.2 and returns it with its
// outward certificate. Bounds contract: arguments are endpoint differences,
// so each component is bounded by the PROVISIONAL difference domain
// |dx|,|dy| <= 2*CandidateCoordinateBoundMm (= 2B); violations yield
// ErrOutOfBounds, non-finite input ErrNonFinite, a zero vector
// ErrZeroDirection. Values beyond 2B are outside this package's analyzed
// evidence domain.
//
// FMA independence: correctness does not depend on fusion behavior. The
// arm64 compiler may fuse the sum into an FMADD (verified by disassembly);
// the gamma_5 certificate envelope remains valid under either evaluation
// because fusion only removes intermediate roundings. See FINDINGS.md.
func UnitDirection(dx, dy float64) (Direction, error) {
	if err := validateDifferenceCoord(dx); err != nil {
		return Direction{}, fmt.Errorf("dx: %w", err)
	}
	if err := validateDifferenceCoord(dy); err != nil {
		return Direction{}, fmt.Errorf("dy: %w", err)
	}
	if dx == 0 && dy == 0 {
		return Direction{}, ErrZeroDirection
	}
	if dx == 0 || dy == 0 {
		return unitDirectionFallback(dx, dy)
	}

	eMax, eMin := ilogbAbs(dx), ilogbAbs(dy)
	if eMax < eMin {
		eMax, eMin = eMin, eMax
	}

	if eMax-eMin <= fastPathMaxExpDelta {
		if d, ok := unitDirectionGated(dx, dy); ok {
			return d, nil
		}
	}
	return unitDirectionFallback(dx, dy)
}

// unitDirectionGated implements section 4.2 steps 2-3. It reports ok=false
// when any gate fails, deferring to the fallback.
func unitDirectionGated(dx, dy float64) (Direction, bool) {
	mag := maxAbs(dx, dy)

	// Step 2: choose integer k placing mag*2^k in [2^499, 2^500).
	// Power-of-two scaling of a normal-range value is exact.
	k := 500 - (math.Ilogb(mag) + 1)
	dxS := math.Ldexp(dx, k)
	dyS := math.Ldexp(dy, k)

	// Step 3 gates: every intermediate must be finite and normal-range.
	gate := func(f float64) bool {
		a := abs(f)
		return !math.IsInf(a, 0) && a == a && (a == 0 || a >= smallestNormal)
	}
	sx := dxS * dxS
	sy := dyS * dyS
	sum := sx + sy
	r := math.Sqrt(sum)
	nx := dxS / r
	ny := dyS / r
	if !gate(sx) || !gate(sy) || !gate(sum) || !gate(r) ||
		!gate(nx) || !gate(ny) {
		return Direction{}, false
	}
	return Direction{
		NX:   nx,
		NY:   ny,
		Path: PathGatedFast,
		Cert: new(big.Rat).Set(fastDirCert),
	}, true
}

// unitDirectionFallback computes the direction at 256-bit precision and
// derives an outward certificate covering the fallback's own rounding plus
// commitment to binary64. See PathHighPrecisionFallback's documented spike
// limitation regarding section 4.3 machinery.
func unitDirectionFallback(dx, dy float64) (Direction, error) {
	fx := new(big.Float).SetPrec(fallbackPrec).SetFloat64(dx)
	fy := new(big.Float).SetPrec(fallbackPrec).SetFloat64(dy)
	sqx := new(big.Float).SetPrec(fallbackPrec).Mul(fx, fx)
	sqy := new(big.Float).SetPrec(fallbackPrec).Mul(fy, fy)
	sum := sqx.Add(sqx, sqy)
	den := sum.Sqrt(sum)

	nxRef := new(big.Float).SetPrec(fallbackPrec).Quo(fx, den)
	nyRef := new(big.Float).SetPrec(fallbackPrec).Quo(fy, den)

	nx, _ := nxRef.Float64()
	ny, _ := nyRef.Float64()
	if math.IsNaN(nx) || math.IsNaN(ny) {
		return Direction{}, ErrNonFinite
	}

	cert := new(big.Rat).Set(fallbackBaseCert)
	cert.Add(cert, halfULPRat(nx))
	cert.Add(cert, halfULPRat(ny))
	return Direction{
		NX:   nx,
		NY:   ny,
		Path: PathHighPrecisionFallback,
		Cert: cert,
	}, nil
}

// Certificate constants: gamma_n = n*u/(1-n*u) with u = 2^-53
// (NUMERICAL-MODEL.md section 4 definitions).

// gamma5Den = 2^53 - 5 makes gamma_5 = 5/gamma5Den an exact rational.
var gamma5Den = new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 53), big.NewInt(5))

// gamma3Den = 2^53 - 3 makes gamma_3 = 3/gamma3Den an exact rational.
var gamma3Den = new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 53), big.NewInt(3))

// certPrec fixes the outward-rounding granularity of irrational factors.
const certPrec = 64

// fastDirCert is an outward rational bound for sqrt(2)*gamma_5, the L2 norm
// bound on the gated fast-path direction error (NUMERICAL-MODEL.md section
// 4.2 corollary ||dn||_2 <= sqrt(2)*gamma_5).
var fastDirCert = outwardSqrtOverInt(50, gamma5Den)

// fallbackBaseCert is the outward bound of sqrt(2)*gamma_5 at
// fallbackPrec-bit arithmetic (five roundings of relative error
// 2^-fallbackPrec), covering fallback evaluation before commitment.
var fallbackBaseCert = outwardSqrtOverIntPrec(50,
	new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), fallbackPrec), big.NewInt(5)),
	fallbackPrec)

// outwardSqrtOverInt bounds sqrt(n)/den outward as
// CeilSqrt(n*2^(2*certPrec)) / (den*2^certPrec).
func outwardSqrtOverInt(n int64, den *big.Int) *big.Rat {
	root := ceilSqrt(new(big.Int).Lsh(big.NewInt(n), 2*certPrec))
	r := new(big.Rat).SetFrac(root, new(big.Int).Lsh(den, certPrec))
	return r
}

// outwardSqrtOverIntPrec is outwardSqrtOverInt with explicit precision.
func outwardSqrtOverIntPrec(n int64, den *big.Int, prec uint) *big.Rat {
	root := ceilSqrt(new(big.Int).Lsh(big.NewInt(n), 2*prec))
	r := new(big.Rat).SetFrac(root, new(big.Int).Lsh(den, prec))
	return r
}

// ceilSqrt returns the ceiling of the integer square root of n (n > 0).
func ceilSqrt(n *big.Int) *big.Int {
	r := new(big.Int).Sqrt(n)
	if new(big.Int).Mul(r, r).Cmp(n) != 0 {
		r.Add(r, big.NewInt(1))
	}
	return r
}

// OffsetVertexCertBounds carries the outward rational certificate for one
// offset vertex.
type OffsetVertexCertBounds struct {
	// Eps is the total outward bound on ||v_committed - v_ideal||_2 built
	// from NUMERICAL-MODEL.md section 4.4 terms 1-6.
	Eps *big.Rat
}

// offsetAssemblyOps counts the elementary binary64 operations in the vertex
// assembly v = P +- s*n-hat (two multiplications, two additions), sizing
// the conservative subnormal-floor blanket of term 6.
const offsetAssemblyOps = 4

// OffsetVertexCertificate builds the NUMERICAL-MODEL.md section 4.4
// certificate for the vertex obtained by offsetting stored point P by
// signed half-thickness s along certified direction d; sigma selects the
// side (+1/-1). All terms are exact rationals or outward-rounded bounds;
// Eps >= the true committed-vertex error holds constructively. Inputs must
// be finite (ErrNonFinite); s may be any non-negative finite value,
// including subnormals, exercising term 2's subnormal floor.
//
// Failure behavior mirrors OffsetVertex: a committed vertex that is not a
// finite binary64 value (including overflow of s*n-hat) fails with
// ErrDerivedVertexInvalid, never yielding a certificate for a
// non-existent vertex.
//
// Coverage limits: the certificate covers ONLY construction-chain terms 1-6.
// Downstream boolean/face stages (hypotheses H-3/H-4) are outside this
// package; uncertified data feeding observables escalates under the
// candidate model, which is issue 0007's obligation.
func OffsetVertexCertificate(p Point, s float64, sigma int, d Direction) (OffsetVertexCertBounds, error) {
	if err := validateCoord(p.X); err != nil {
		return OffsetVertexCertBounds{}, fmt.Errorf("p.X: %w", err)
	}
	if err := validateCoord(p.Y); err != nil {
		return OffsetVertexCertBounds{}, fmt.Errorf("p.Y: %w", err)
	}
	if err := validateFinite(s); err != nil {
		return OffsetVertexCertBounds{}, fmt.Errorf("s: %w", err)
	}
	if sigma != 1 && sigma != -1 {
		return OffsetVertexCertBounds{}, fmt.Errorf("sigma must be -1 or +1")
	}
	if s < 0 {
		return OffsetVertexCertBounds{}, fmt.Errorf("s must be non-negative")
	}
	if err := validateDirection(d); err != nil {
		return OffsetVertexCertBounds{}, err
	}
	// F4: a certificate is only meaningful for an existing finite committed
	// vertex. Reject here when OffsetVertex would fail, so the pair can
	// never disagree.
	if _, _, verr := commitOffsetVertex(p, s, sigma, d); verr != nil {
		return OffsetVertexCertBounds{}, verr
	}

	eps := new(big.Rat)

	// Term 1: commitment of P (per-coordinate half-ulp; L2 <= c1 + c2).
	eps.Add(eps, halfULPRat(p.X))
	eps.Add(eps, halfULPRat(p.Y))

	// Term 2: signed half-thickness rounding (includes the subnormal floor).
	halfULPs := halfULPRat(s)
	eps.Add(eps, halfULPs)

	// Term 3: direction error scaled by s.
	eps.Add(eps, new(big.Rat).Mul(floatToRat(s), d.Cert))

	// Term 4: offset assembly rounding, per-coordinate
	// gamma_3*max(|Px|,|Py|,|s|), converted x sqrt(2) outward
	// (sqrt(2)*sqrt(9) = sqrt(18)).
	gamma3Sqrt2 := outwardSqrtOverInt(18, gamma3Den)
	eps.Add(eps, new(big.Rat).Mul(gamma3Sqrt2, maxAbsAsRat(p.X, p.Y, s)))

	// Term 5: cross term combining rounded s and rounded direction,
	// retained rather than dropped.
	eps.Add(eps, new(big.Rat).Mul(halfULPs, d.Cert))

	// Term 6: conservative subnormal-floor blanket over the assembly ops.
	blanket := new(big.Rat).SetInt64(offsetAssemblyOps)
	blanket.Mul(blanket, etaHalfRat)
	eps.Add(eps, blanket)

	return OffsetVertexCertBounds{Eps: eps}, nil
}

// OffsetVertex commits the vertex v = fl(P + sigma*s*n-hat). The section
// 4.4 certificate covers the assembly chain under fused or unfused
// evaluation alike (fusion only removes roundings). Pair it with
// OffsetVertexCertificate using the same arguments.
//
// Bounds contract: as for OffsetVertexCertificate, P enforces the
// provisional candidate coordinate bound B while s enforces finiteness
// only; sigma must be +-1, the direction must be structurally valid (see
// validateDirection), and
// the committed result must itself be finite; a non-finite committed vertex
// fails with ErrDerivedVertexInvalid rather than returning a silently broken
// point. This spike does not enforce the candidate derived-coordinate bound
// D (unratified); enforcing D belongs to ratified entity validation.
func OffsetVertex(p Point, s float64, sigma int, d Direction) (Point, error) {
	vx, vy, err := commitOffsetVertex(p, s, sigma, d)
	if err != nil {
		return Point{}, err
	}
	return Point{X: vx, Y: vy}, nil
}

// commitOffsetVertex is the shared boundary-checked commit used by both
// OffsetVertexCertificate and OffsetVertex so the two APIs can never
// disagree about whether a committed vertex exists (review finding F4).
func commitOffsetVertex(p Point, s float64, sigma int, d Direction) (float64, float64, error) {
	if err := validateCoord(p.X); err != nil {
		return 0, 0, fmt.Errorf("p.X: %w", err)
	}
	if err := validateCoord(p.Y); err != nil {
		return 0, 0, fmt.Errorf("p.Y: %w", err)
	}
	if err := validateFinite(s); err != nil {
		return 0, 0, fmt.Errorf("s: %w", err)
	}
	if sigma != 1 && sigma != -1 {
		return 0, 0, fmt.Errorf("sigma must be -1 or +1")
	}
	if s < 0 {
		return 0, 0, fmt.Errorf("s must be non-negative")
	}
	if err := validateDirection(d); err != nil {
		return 0, 0, err
	}
	snX := s * d.NX
	snY := s * d.NY
	vx := p.X + float64(sigma)*snX
	vy := p.Y + float64(sigma)*snY
	if math.IsNaN(vx) || math.IsInf(vx, 0) ||
		math.IsNaN(vy) || math.IsInf(vy, 0) {
		return 0, 0, fmt.Errorf("%w: offset vertex (%v, %v)",
			ErrDerivedVertexInvalid, vx, vy)
	}
	return vx, vy, nil
}

// ilogbAbs returns the unbiased IEEE-754 exponent of |f| for nonzero finite
// f (math.Ilogb handles subnormals correctly).
func ilogbAbs(f float64) int { return math.Ilogb(f) }

// etaHalfRat is eta/2 = 2^-1075, the uniform absolute bound for an
// elementary operation whose exact result is zero-or-subnormal
// (NUMERICAL-MODEL.md section 4 definitions; section 4.4 term 6).
var etaHalfRat = powTwoRat(-1075)

// powTwoRat returns 2^e as an exact rational.
func powTwoRat(e int) *big.Rat {
	if e >= 0 {
		return new(big.Rat).SetFrac(
			new(big.Int).Lsh(big.NewInt(1), uint(e)), big.NewInt(1))
	}
	return new(big.Rat).SetFrac(
		big.NewInt(1), new(big.Int).Lsh(big.NewInt(1), uint(-e)))
}

// maxAbsAsRat returns max(|a|,|b|,|c|) as an exact rational.
func maxAbsAsRat(a, b, c float64) *big.Rat {
	m := abs(a)
	if abs(b) > m {
		m = abs(b)
	}
	if abs(c) > m {
		m = abs(c)
	}
	return floatToRat(m)
}

func maxAbs(a, b float64) float64 {
	if abs(a) >= abs(b) {
		return abs(a)
	}
	return abs(b)
}
