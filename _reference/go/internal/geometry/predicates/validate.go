package predicates

import (
	"fmt"
	"math"
	"math/big"
)

// validateFinite enforces the finite-input policy alone (no magnitude
// bound). Used where no candidate magnitude bound applies, such as the
// half-thickness s of offset APIs and certificate-internal components;
// endpoint-difference arguments use validateDifferenceCoord instead.
func validateFinite(f float64) error {
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return ErrNonFinite
	}
	return nil
}

// validateCoord enforces the finite-input policy AND the provisional
// candidate coordinate bound against a stored point coordinate.
func validateCoord(f float64) error {
	if err := validateFinite(f); err != nil {
		return err
	}
	if f > CandidateCoordinateBoundMm || f < -CandidateCoordinateBoundMm {
		return fmt.Errorf("%w: %v exceeds provisional bound %g",
			ErrOutOfBounds, f, CandidateCoordinateBoundMm)
	}
	return nil
}

// validatePoint enforces both policy checks against p.
func validatePoint(p Point) error {
	if err := validateCoord(p.X); err != nil {
		return err
	}
	return validateCoord(p.Y)
}

// validateDirection checks a caller-supplied Direction for structural
// validity at this API boundary: finite committed components, a recognized
// construction path, and a present non-negative rational certificate. It
// cannot re-derive the certificate's soundness; callers obtain Directions
// from UnitDirection or an equally certified construction.
func validateDirection(d Direction) error {
	if err := validateFinite(d.NX); err != nil {
		return fmt.Errorf("direction NX: %w", err)
	}
	if err := validateFinite(d.NY); err != nil {
		return fmt.Errorf("direction NY: %w", err)
	}
	switch d.Path {
	case PathGatedFast, PathHighPrecisionFallback:
	default:
		return fmt.Errorf("predicates: unknown direction path %d", uint8(d.Path))
	}
	if d.Cert == nil {
		return fmt.Errorf("predicates: missing direction certificate")
	}
	if d.Cert.Sign() < 0 {
		return fmt.Errorf("predicates: negative direction certificate")
	}
	// A certificate of one or more clamps the lower squared-norm band to
	// zero and would admit forged degenerate components (e.g. a zero
	// vector) as "unit directions". Every legitimate construction in this
	// package certifies errors of order gamma_5 << 1, so requiring C < 1
	// keeps all valid directions while making the lower band strictly
	// positive.
	if d.Cert.Cmp(big.NewRat(1, 1)) >= 0 {
		return fmt.Errorf("predicates: direction certificate %s exceeds 1", d.Cert)
	}
	// The committed pair satisfies ||n||_2 <= ||n*||_2 + C <= 1 + C, hence
	// sq <= (1+C)^2; and by the triangle inequality
	// ||n||_2 >= ||n*||_2 - C >= 1 - C > 0 (C < 1 is enforced above), so
	// the lower band is strictly positive and forged degenerate components
	// such as zero vectors are always rejected.
	sq := new(big.Rat).Mul(floatToRat(d.NX), floatToRat(d.NX))
	sq.Add(sq, new(big.Rat).Mul(floatToRat(d.NY), floatToRat(d.NY)))
	c := d.Cert
	sqHi := new(big.Rat).Add(c, big.NewRat(1, 1)) // 1 + C
	sqHi.Mul(sqHi, sqHi)
	oneMinusC := new(big.Rat).Sub(big.NewRat(1, 1), c)
	sqLo := new(big.Rat).Mul(oneMinusC, oneMinusC)
	if sq.Cmp(sqLo) < 0 || sq.Cmp(sqHi) > 0 {
		return fmt.Errorf("predicates: committed components outside the certified unit-direction band")
	}
	return nil
}

// operandNonNormal reports whether a coordinate is outside the normal range,
// including +-zero and subnormals. INTERPRETATION NOTE (recorded for issue
// 0030): NUMERICAL-MODEL.md section 4.1 requires operands of gated
// operations to be normal-range unless exactness is separately proved.
// This implementation applies the LITERAL reading conservatively: any
// non-normal original coordinate (zero included) escalates to the exact
// integer path instead of claiming the subtraction-exactness exception.
// Cost: common origin-anchored triangles take the fallback; measured in
// BENCHMARKS.md.
func operandNonNormal(p Point) bool {
	return nonNormalRange(p.X) || nonNormalRange(p.Y)
}

func nonNormalRange(f float64) bool {
	a := abs(f)
	return a < smallestNormal // covers +0, -0, subnormals
}

// validateDifferenceCoord enforces the finiteness policy AND the candidate
// COMPONENTWISE endpoint-difference bound 2*CandidateCoordinateBoundMm: the
// largest exact componentwise difference of two stored coordinates within
// the candidate domain B. The vector NORM is deliberately not bounded here
// (it reaches 2*sqrt(2)*B for diagonal differences); only per-component
// magnitude is policy. Used by direction arguments (review finding F6).
func validateDifferenceCoord(f float64) error {
	if err := validateFinite(f); err != nil {
		return err
	}
	limit := 2 * CandidateCoordinateBoundMm
	if f > limit || f < -limit {
		return fmt.Errorf("%w: %v exceeds provisional difference bound %g",
			ErrOutOfBounds, f, limit)
	}
	return nil
}
