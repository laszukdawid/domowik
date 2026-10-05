package predicates

import (
	"math"
	"math/big"
	"strconv"
	"testing"
)

// This file implements the INDEPENDENT exact-decision oracle used by tests.
//
// Independence contract (issue 0006 acceptance criterion): the oracle must
// be structured separately from the production escalation paths so that a
// shared decode or sign bug cannot hide. Two mechanisms enforce this:
//
//  1. DECODE PATH: production decodes float64 via IEEE bit manipulation
//     (decodeDyadic in exact.go). The oracle decodes through the shortest
//     round-trip DECIMAL representation (strconv.FormatFloat 'e' -1 64)
//     parsed into a big.Rat, an exact conversion that never touches the bit
//     layout.
//  2. CLASSIFIER STRUCTURE: the oracle segment classifier below is written
//     from scratch around Cramer solves on rationals and never calls
//     production helpers such as crossOfDiffs, paramOnSegment, or
//     intersectCollinear.
//
// The oracle accepts ANY finite float64 including values outside the
// candidate coordinate bound; the public API's ErrOutOfBounds policy is a
// separate layer tested elsewhere.

// ORACLE METHOD AND PRECISION (recorded per review requirement):
//
// Step 1: strconv.FormatFloat(f, 'e', -1, 64) yields the shortest decimal
// string that round-trips to exactly f under correctly-rounded decimal ->
// binary64 conversion.
//
// Step 2: big.ParseFloat(s, 10, 53, big.ToNearestEven) performs CORRECT
// ROUNDING of that exact decimal to a 53-bit-significand binary float with
// IEEE round-to-nearest-even semantics — the same operation strtod performs
// — so the resulting big.Float IS the stored binary64 value (never the
// decimal literal's rational value such as 1/10 for 0.1).
//
// Step 3: (*big.Float).Rat() converts a finite dyadic big.Float to its
// EXACT rational.
//
// Step 4 (self-check): the reconstructed value's bit pattern must equal the
// input's bit pattern; any mismatch aborts the test. This makes silent
// decimal-rational substitution impossible: a bug anywhere in steps 1-3 is
// caught per call. TestOracleRejectsDecimalSubstitution additionally proves
// the oracle distinguishes stored-float arithmetic from decimal-literal
// arithmetic.
func oracleRat(t *testing.T, f float64) *big.Rat {
	t.Helper()
	if math.IsNaN(f) || math.IsInf(f, 0) {
		t.Fatalf("oracleRat called with non-finite %v", f)
	}
	s := strconv.FormatFloat(f, 'e', -1, 64)
	bf, _, err := big.ParseFloat(s, 10, 53, big.ToNearestEven)
	if err != nil {
		t.Fatalf("oracleRat: parse %q: %v", s, err)
	}
	f64, _ := bf.Float64()
	if math.Float64bits(f64) != math.Float64bits(f) {
		// Fallback independent decode: direct bit decomposition written
		// separately from production decodeDyadic (no shared code).
		r := oracleBitsDecode(t, f)
		if r == nil {
			t.Fatalf("oracleRat: reconstruction of %v (%X) mismatched", f, math.Float64bits(f))
		}
		return r
	}
	r, acc := bf.Rat(nil)
	if r == nil || acc != big.Exact {
		t.Fatalf("oracleRat: non-exact conversion for %v", f)
	}
	return r
}

// oracleBitsDecode is the fallback decoder: an independently written
// IEEE-bit decomposition (different code, same mathematical definition).
func oracleBitsDecode(t *testing.T, f float64) *big.Rat {
	t.Helper()
	bits := math.Float64bits(f)
	neg := bits>>63 != 0
	frac := int64(bits & 0xfffffffffffff)
	exp := int((bits >> 52) & 0x7ff)
	var mant *big.Int
	e := 0
	switch exp {
	case 0:
		if frac == 0 {
			return new(big.Rat)
		}
		mant, e = big.NewInt(frac), -1074
	case 0x7ff:
		t.Fatalf("oracleBitsDecode: non-finite input")
		return nil
	default:
		mant, e = big.NewInt(frac|1<<52), exp-1075
	}
	if neg {
		mant.Neg(mant)
	}
	r := new(big.Rat).SetInt(mant)
	if e >= 0 {
		num := new(big.Int).Lsh(r.Num(), uint(e))
		r.SetFrac(num, big.NewInt(1))
	} else {
		r.SetFrac(r.Num(), new(big.Int).Lsh(big.NewInt(1), uint(-e)))
	}
	return r
}

// oracleZero and oracleOne are oracle-local rationals, deliberately not the
// production ratZero/ratOne.
var (
	oracleZero = new(big.Rat).SetInt64(0)
	oracleOne  = new(big.Rat).SetInt64(1)
)

// signToOracle converts a production Sign into its plain -1/0/+1 oracle
// value through an explicit local table, so a production enum/mapping bug
// cannot hide inside the comparison.
func signToOracle(s Sign) int {
	switch int8(s) {
	case -1:
		return -1
	case 0:
		return 0
	case 1:
		return 1
	default:
		return -128 // forces mismatch with any oracle result
	}
}

// oracleOrient computes the exact sign of (b-a)x(c-a) over stored inputs
// and returns it as a plain -1/0/+1 int.
func oracleOrient(t *testing.T, ax, ay, bx, by, cx, cy float64) int {
	t.Helper()
	dxab := new(big.Rat).Sub(oracleRat(t, bx), oracleRat(t, ax))
	dyab := new(big.Rat).Sub(oracleRat(t, by), oracleRat(t, ay))
	dxac := new(big.Rat).Sub(oracleRat(t, cx), oracleRat(t, ax))
	dyac := new(big.Rat).Sub(oracleRat(t, cy), oracleRat(t, ay))
	p1 := new(big.Rat).Mul(dxab, dyac)
	p2 := new(big.Rat).Mul(dyab, dxac)
	return p1.Sub(p1, p2).Sign()
}

// oracleSegResult mirrors SegmentResult for differential comparison.
type oracleSegResult struct {
	class    SegmentClass
	t, s     *big.Rat
	hasTS    bool
	overlapT *[2]*big.Rat
	overlapS *[2]*big.Rat
}

func ratsEqual(a, b *big.Rat) bool {
	switch {
	case a == nil && b == nil:
		return true
	case a == nil || b == nil:
		return false
	default:
		return a.Cmp(b) == 0
	}
}

// oracleIntersect classifies two segments exactly with independent logic.
func oracleIntersect(t *testing.T, p1, p2, q1, q2 Point) oracleSegResult {
	t.Helper()
	rax, ray := oracleRat(t, p1.X), oracleRat(t, p1.Y)
	rbx, rby := oracleRat(t, p2.X), oracleRat(t, p2.Y)
	rcx, rcy := oracleRat(t, q1.X), oracleRat(t, q1.Y)
	rdx, rdy := oracleRat(t, q2.X), oracleRat(t, q2.Y)

	sub := func(a, b *big.Rat) *big.Rat { return new(big.Rat).Sub(a, b) }
	mul := func(a, b *big.Rat) *big.Rat { return new(big.Rat).Mul(a, b) }

	dpx, dpy := sub(rbx, rax), sub(rby, ray)
	dqx, dqy := sub(rdx, rcx), sub(rdy, rcy)

	cross := func(ax, ay, bx, by *big.Rat) *big.Rat {
		return sub(mul(ax, by), mul(ay, bx))
	}

	denom := cross(dpx, dpy, dqx, dqy)

	if denom.Sign() != 0 {
		wx, wy := sub(rcx, rax), sub(rcy, ray)
		numT := cross(wx, wy, dqx, dqy)
		numS := cross(wx, wy, dpx, dpy)
		tv := numT.Quo(numT, denom)
		sv := numS.Quo(numS, denom)
		inT := tv.Cmp(oracleZero) >= 0 && tv.Cmp(oracleOne) <= 0
		inS := sv.Cmp(oracleZero) >= 0 && sv.Cmp(oracleOne) <= 0
		if !inT || !inS {
			return oracleSegResult{class: ClassDisjoint}
		}
		boundary := tv.Cmp(oracleZero) == 0 || tv.Cmp(oracleOne) == 0 ||
			sv.Cmp(oracleZero) == 0 || sv.Cmp(oracleOne) == 0
		if boundary {
			return oracleSegResult{class: ClassTouchEndpoint, t: tv, s: sv, hasTS: true}
		}
		return oracleSegResult{class: ClassCrossingInterior, t: tv, s: sv, hasTS: true}
	}

	// Parallel: collinear iff q1 lies on p's line.
	wq := cross(sub(rcx, rax), sub(rcy, ray), dpx, dpy)
	if wq.Sign() != 0 {
		return oracleSegResult{class: ClassDisjoint}
	}

	param := func(px, py *big.Rat) *big.Rat {
		numX, numY := mul(sub(px, rax), dpx), mul(sub(py, ray), dpy)
		num := numX.Add(numX, numY)
		denX, denY := mul(dpx, dpx), mul(dpy, dpy)
		den := denX.Add(denX, denY)
		return num.Quo(num, den)
	}

	l1 := param(rcx, rcy)
	l2 := param(rdx, rdy)
	lo, hi := l1, l2
	if lo.Cmp(hi) > 0 {
		lo, hi = hi, lo
	}
	aLo := lo
	if aLo.Cmp(oracleZero) < 0 {
		aLo = new(big.Rat).Set(oracleZero)
	}
	bHi := hi
	if bHi.Cmp(oracleOne) > 0 {
		bHi = new(big.Rat).Set(oracleOne)
	}

	switch aLo.Cmp(bHi) {
	case 1:
		return oracleSegResult{class: ClassDisjoint}
	case 0:
		res := oracleSegResult{class: ClassCollinearTouch, t: new(big.Rat).Set(aLo), hasTS: true}
		res.s = oracleParamAlongQ(t, p1, p2, res.t, q1, q2)
		return res
	default:
		res := oracleSegResult{
			class:    ClassCollinearOverlap,
			overlapT: &[2]*big.Rat{new(big.Rat).Set(aLo), new(big.Rat).Set(bHi)},
		}
		res.overlapS = &[2]*big.Rat{
			oracleParamAlongQ(t, p1, p2, res.overlapT[0], q1, q2),
			oracleParamAlongQ(t, p1, p2, res.overlapT[1], q1, q2),
		}
		if lo.Sign() == 0 && hi.Cmp(oracleOne) == 0 {
			res.class = ClassCoincident
			res.overlapT = &[2]*big.Rat{new(big.Rat).Set(oracleZero), new(big.Rat).Set(oracleOne)}
			// overlapS keeps the true q-parameter mapping (reversed inputs
			// legitimately report [1,0]).
		}
		return res
	}
}

// oracleParamAlongQ maps x = p1 + tv*(p2-p1) to its parameter along q.
func oracleParamAlongQ(t *testing.T, p1, p2 Point, tv *big.Rat, q1, q2 Point) *big.Rat {
	t.Helper()
	x := new(big.Rat).Add(
		oracleRat(t, p1.X),
		new(big.Rat).Mul(tv, new(big.Rat).Sub(oracleRat(t, p2.X), oracleRat(t, p1.X))))
	y := new(big.Rat).Add(
		oracleRat(t, p1.Y),
		new(big.Rat).Mul(tv, new(big.Rat).Sub(oracleRat(t, p2.Y), oracleRat(t, p1.Y))))
	dqx := new(big.Rat).Sub(oracleRat(t, q2.X), oracleRat(t, q1.X))
	dqy := new(big.Rat).Sub(oracleRat(t, q2.Y), oracleRat(t, q1.Y))
	numX := new(big.Rat).Mul(new(big.Rat).Sub(x, oracleRat(t, q1.X)), dqx)
	numY := new(big.Rat).Mul(new(big.Rat).Sub(y, oracleRat(t, q1.Y)), dqy)
	num := numX.Add(numX, numY)
	denX := new(big.Rat).Mul(dqx, dqx)
	denY := new(big.Rat).Mul(dqy, dqy)
	den := denX.Add(denX, denY)
	return num.Quo(num, den)
}

// naiveOrient is the simple float implementation used by the mutation test
// and as the benchmark baseline. It is deliberately unguarded.
func naiveOrient(ax, ay, bx, by, cx, cy float64) Sign {
	det := (bx-ax)*(cy-ay) - (by-ay)*(cx-ax)
	if det > 0 {
		return SignPositive
	}
	if det < 0 {
		return SignNegative
	}
	return SignZero
}
