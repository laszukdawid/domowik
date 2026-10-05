package predicates

import (
	"fmt"
	"math/big"
)

// Exact dyadic comparators evidencing the along-axis opening checks and
// split-remap containment forms of NUMERICAL-MODEL.md section 3 (rules
// R4.1.2, R4.1.4, R4.3.3). These are EVIDENCE-GRADE primitives only: they
// decide the exact sign of the named quadratic forms over stored binary64
// values. They contain NO opening-validation business logic; entity-level
// semantics remain owned by the behavior specification and later issues.

// ExactSquaredLength returns m = dx^2 + dy^2 computed exactly over the
// stored endpoint difference (bx-ax, by-ay). This is the candidate model's
// m for all along-axis forms. Both endpoints must satisfy the package
// coordinate policy.
func ExactSquaredLength(ax, ay, bx, by float64) (*big.Rat, error) {
	if err := validateCoord(ax); err != nil {
		return nil, fmt.Errorf("ax: %w", err)
	}
	if err := validateCoord(ay); err != nil {
		return nil, fmt.Errorf("ay: %w", err)
	}
	if err := validateCoord(bx); err != nil {
		return nil, fmt.Errorf("bx: %w", err)
	}
	if err := validateCoord(by); err != nil {
		return nil, fmt.Errorf("by: %w", err)
	}
	dpx := new(big.Rat).Sub(floatToRat(bx), floatToRat(ax))
	dpy := new(big.Rat).Sub(floatToRat(by), floatToRat(ay))
	sqx := new(big.Rat).Mul(dpx, dpx)
	sqy := new(big.Rat).Mul(dpy, dpy)
	return sqx.Add(sqx, sqy), nil
}

// requireSquaredLength guards the shared m argument of the form deciders.
func requireSquaredLength(m *big.Rat) error {
	if m == nil {
		return fmt.Errorf("m: nil squared length")
	}
	if m.Sign() < 0 {
		return fmt.Errorf("m: negative squared length")
	}
	return nil
}

// requireUnitIntervalU enforces the candidate u domain [0,1] (R4.0.5).
func requireUnitIntervalU(u float64, name string) error {
	if err := validateFinite(u); err != nil {
		return fmt.Errorf("%s: %w", name, err)
	}
	ur := floatToRat(u)
	if ur.Cmp(ratZero) < 0 || ur.Cmp(ratOne) > 0 {
		return fmt.Errorf("%s: %v outside [0,1]", name, u)
	}
	return nil
}

// SignStartFit decides sign(4*u^2*m - w^2) exactly: the strict start-fit
// form of R4.1.2 (u*L - w/2 > 0 iff this is positive; zero clearance
// rejects). w must be finite and non-negative and is the FULL opening
// width: the form squares w because the inequality compares the half-width
// w/2 against the clearance u*L. This contrasts with OffsetVertex's s
// argument, which carries a HALF-thickness (s = t/2).
func SignStartFit(u, w float64, m *big.Rat) (Sign, error) {
	if err := requireUnitIntervalU(u, "u"); err != nil {
		return 0, err
	}
	if err := requireSquaredLength(m); err != nil {
		return 0, err
	}
	wr, err := nonNegWidth(w)
	if err != nil {
		return 0, err
	}
	ur := floatToRat(u)
	// 4*u^2*m - w^2, exact rational arithmetic throughout.
	t := new(big.Rat).Mul(ur, ur)
	t.Mul(t, m)
	t.Mul(t, new(big.Rat).SetInt64(4))
	t.Sub(t, new(big.Rat).Mul(wr, wr))
	return ratSign(t), nil
}

// SignEndFit decides sign(4*(1-u)^2*m - w^2) exactly: the strict end-fit
// form of R4.1.2 (L - u*L - w/2 > 0 iff positive).
func SignEndFit(u, w float64, m *big.Rat) (Sign, error) {
	if err := requireUnitIntervalU(u, "u"); err != nil {
		return 0, err
	}
	if err := requireSquaredLength(m); err != nil {
		return 0, err
	}
	wr, err := nonNegWidth(w)
	if err != nil {
		return 0, err
	}
	ur := floatToRat(u)
	oneMinusU := new(big.Rat).Sub(ratOne, ur)
	t := new(big.Rat).Mul(oneMinusU, oneMinusU)
	t.Mul(t, m)
	t.Mul(t, new(big.Rat).SetInt64(4))
	t.Sub(t, new(big.Rat).Mul(wr, wr))
	return ratSign(t), nil
}

// SignOverlapForm decides sign(4*(u1-u2)^2*m - (w1+w2)^2) exactly for two
// openings' along-axis spans (R4.1.4). Sign convention: NEGATIVE means the
// closed spans overlap with positive length; ZERO means they touch at
// exactly one point; POSITIVE means they are separated by a positive gap.
func SignOverlapForm(u1, u2, w1, w2 float64, m *big.Rat) (Sign, error) {
	if err := requireSquaredLength(m); err != nil {
		return 0, err
	}
	for _, tc := range []struct {
		v   float64
		nme string
	}{{u1, "u1"}, {u2, "u2"}} {
		if err := requireUnitIntervalU(tc.v, tc.nme); err != nil {
			return 0, err
		}
	}
	a, err := nonNegWidth(w1)
	if err != nil {
		return 0, fmt.Errorf("w1: %w", err)
	}
	b, err := nonNegWidth(w2)
	if err != nil {
		return 0, fmt.Errorf("w2: %w", err)
	}
	diff := new(big.Rat).Sub(floatToRat(u1), floatToRat(u2))
	t := new(big.Rat).Mul(diff, diff)
	t.Mul(t, m)
	t.Mul(t, new(big.Rat).SetInt64(4))
	sumW := a.Add(a, b)
	t.Sub(t, new(big.Rat).Mul(sumW, sumW))
	return ratSign(t), nil
}

// RatStrictlyBetween reports lo < v < hi with exact rational comparisons:
// the R4.3.3 remap-containment form over child split parameters t (class-C1
// rationals from IntersectSegments) and an aperture center parameter u.
func RatStrictlyBetween(v, lo, hi *big.Rat) bool {
	if v == nil || lo == nil || hi == nil {
		return false
	}
	return v.Cmp(lo) > 0 && v.Cmp(hi) < 0
}

func nonNegWidth(w float64) (*big.Rat, error) {
	if err := validateFinite(w); err != nil {
		return nil, fmt.Errorf("w: %w", err)
	}
	if w < 0 {
		return nil, fmt.Errorf("w: negative width %v", w)
	}
	return floatToRat(w), nil
}
