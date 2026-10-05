package predicates

import (
	"fmt"
	"math/big"
)

// Sign is the exact sign of an expression over stored binary64 values.
type Sign int8

const (
	// SignNegative means the exact value is strictly negative (clockwise
	// orientation for Orientation).
	SignNegative Sign = -1
	// SignZero means the exact value is exactly zero (exact collinearity
	// for Orientation). It is a genuine geometric decision, never a stand-in
	// for invalid input; invalid input returns an error instead.
	SignZero Sign = 0
	// SignPositive means the exact value is strictly positive
	// (counterclockwise orientation for Orientation).
	SignPositive Sign = 1
)

func (s Sign) String() string {
	switch s {
	case SignNegative:
		return "negative"
	case SignZero:
		return "zero"
	case SignPositive:
		return "positive"
	default:
		return fmt.Sprintf("invalid Sign(%d)", int8(s))
	}
}

// Point is one planar coordinate in millimetres, subject to the package
// finiteness and candidate-bound policy documented in doc.go.
type Point struct {
	X, Y float64
}

// ccwErrboundA is the conservative filter bound (3 + 16u)*u for the
// determinant formula below. It is derived from the standard floating-point
// error analysis of the four input subtractions, two products, and one
// final subtraction, each with relative error at most u under
// round-to-nearest; see Shewchuk (DCG 1997), section 3, filter A derivation
// (provenance P-1b). The bound remains valid when the compiler fuses the
// final subtraction chain into a single FMA (which arm64 does): fusion only
// removes intermediate roundings, shrinking the covered error. Returned
// signs are therefore platform-independent even where filter bits differ.
const ccwErrboundA = 3.3306690738754716e-16

// Orientation returns the exact sign of the signed area determinant of the
// triangle (a, b, c) over the stored binary64 values:
//
//	sign((b.X-a.X)*(c.Y-a.Y) - (b.Y-a.Y)*(c.X-a.X))
//
// Counterclockwise turns are positive. The result is exact: no tolerance is
// used anywhere, and near-degenerate triangles classify through the exact
// fallback. Non-finite or out-of-bound inputs yield ErrNonFinite or
// ErrOutOfBounds respectively and never a Sign decision.
func Orientation(a, b, c Point) (Sign, error) {
	if err := validateTriangle(a, b, c); err != nil {
		return 0, err
	}
	return orientFast(a, b, c), nil
}

// validateTriangle applies the shared finiteness and candidate-bound
// policy to three points, attributing failures to their input.
func validateTriangle(a, b, c Point) error {
	if err := validatePoint(a); err != nil {
		return fmt.Errorf("point a: %w", err)
	}
	if err := validatePoint(b); err != nil {
		return fmt.Errorf("point b: %w", err)
	}
	if err := validatePoint(c); err != nil {
		return fmt.Errorf("point c: %w", err)
	}
	return nil
}

// orientFast applies the filter; it assumes validated finite inputs.
//
// Section 4.1 normality gating: the analytic errbound is claimed only when
// every elementary operand and result is normal-range; any zero or
// subnormal intermediate routes to the exact fallback, whose arithmetic has
// no rounding at all. This implements the per-operation gating obligation
// conservatively for the filter stage.
//
// Impact note: orientFast delegates to orientFastStats with a nil counter,
// which adds two predictable nil-pointer comparisons per decision on this
// path. BenchmarkOrientationAdaptive measures exactly this shipped code
// under the registered protocol; no uninstrumented variant exists.
func orientFast(a, b, c Point) Sign {
	return orientFastStats(a, b, c, nil)
}

// orientFastStats is orientFast with optional H-6 instrumentation. A nil
// st reproduces orientFast exactly. Otherwise it records one decision per
// successful observable sign and increments filterToExact exactly at the
// runtime filter-fail branch immediately before orientExact.
func orientFastStats(a, b, c Point, st *h6Stats) Sign {
	if s, ok := filterSign(a, b, c); ok {
		if st != nil {
			st.decisions++
		}
		return s
	}
	if st != nil {
		st.filterToExact++
		st.decisions++
	}
	return orientExact(a, b, c)
}

// filterSign applies the gated error-bound filter. It returns ok=false when
// the determinant magnitude lies inside the rounding envelope (or any gate
// routed the case out), deferring to the exact fallback. Exported behavior
// of Orientation is unchanged; the split exists so benchmarks can measure
// the fast path alone and corpus builders can prove fallback membership.
func filterSign(a, b, c Point) (Sign, bool) {
	dxab := b.X - a.X
	dyab := b.Y - a.Y
	dxac := c.X - a.X
	dyac := c.Y - a.Y
	detleft := dxab * dyac
	detright := dyab * dxac
	det := detleft - detright

	// Section 4.1 operand gate (conservative, literal): any ORIGINAL
	// coordinate outside the normal range (including +-zero and subnormals)
	// escalates to the exact integer path; no subtraction-exactness
	// exception is claimed. See validate.go's interpretation note.
	if operandNonNormal(a) || operandNonNormal(b) || operandNonNormal(c) {
		return 0, false
	}

	if abs(dxab) < smallestNormal || abs(dyab) < smallestNormal ||
		abs(dxac) < smallestNormal || abs(dyac) < smallestNormal ||
		abs(detleft) < smallestNormal || abs(detright) < smallestNormal ||
		abs(det) < smallestNormal {
		return 0, false
	}

	errbound := ccwErrboundA * (abs(detleft) + abs(detright))
	switch {
	case det > errbound:
		return SignPositive, true
	case det < -errbound:
		return SignNegative, true
	default:
		return 0, false
	}
}

// orientExact decides the sign through exact scaled-integer arithmetic
// (expansion-free; products formed with integer multiplication via
// bits.Mul64/math.big — see intexact.go).
func orientExact(a, b, c Point) Sign {
	return orientExactScaled(a, b, c)
}

// ratSign maps an exact rational to a Sign.
func ratSign(r *big.Rat) Sign {
	s := r.Sign()
	switch {
	case s > 0:
		return SignPositive
	case s < 0:
		return SignNegative
	default:
		return SignZero
	}
}

func abs(f float64) float64 {
	if f < 0 {
		return -f
	}
	return f
}
