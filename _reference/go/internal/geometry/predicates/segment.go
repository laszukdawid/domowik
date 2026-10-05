package predicates

import (
	"fmt"
	"math/big"
)

// SegmentClass is the exact contact classification of two closed segments
// under the stored binary64 endpoints. The classes map to the R3.1.2 contact
// semantics of BEHAVIOR-SPEC.md as follows:
//
//	ClassDisjoint         no contact of any kind (including parallel and
//	                      collinear-but-separated segments).
//	ClassCrossingInterior proper crossing: the unique intersection point lies
//	                      strictly interior to BOTH segments.
//	ClassTouchEndpoint    unique intersection point that is an endpoint of at
//	                      least one segment (endpoint-on-interior or
//	                      endpoint-endpoint contact, non-collinear). T and S
//	                      reveal exactly which endpoints: a parameter equal
//	                      to 0 or 1 marks endpoint involvement.
//	ClassCollinearTouch   collinear segments whose closed spans share exactly
//	                      one point.
//	ClassCollinearOverlap positive-length collinear overlap of the closed
//	                      spans with UNEQUAL parameter intervals: partial
//	                      overlap AND strict containment (one span fully
//	                      inside the other) both land here; identical
//	                      spans are ClassCoincident. OverlapT/OverlapS
//	                      carry the exact overlap interval on each segment.
//	ClassCoincident       collinear segments with identical positive-length
//	                      spans (each span's parameter interval is [0,1]).
type SegmentClass uint8

const (
	// ClassInvalid is the zero value of SegmentClass and of SegmentResult
	// construction: a result carrying it is UNCLASSIFIED, never a geometric
	// decision. It is returned alongside an error whenever input policy or
	// degeneracy checks fail, so mishandling the error can never disguise
	// invalid input as valid ClassDisjoint (review finding F1).
	ClassInvalid SegmentClass = iota
	ClassDisjoint
	ClassCrossingInterior
	ClassTouchEndpoint
	ClassCollinearTouch
	ClassCollinearOverlap
	ClassCoincident
)

func (c SegmentClass) String() string {
	switch c {
	case ClassInvalid:
		return "invalid"
	case ClassDisjoint:
		return "disjoint"
	case ClassCrossingInterior:
		return "crossing-interior"
	case ClassTouchEndpoint:
		return "touch-endpoint"
	case ClassCollinearTouch:
		return "collinear-touch"
	case ClassCollinearOverlap:
		return "collinear-overlap"
	case ClassCoincident:
		return "coincident"
	default:
		return fmt.Sprintf("invalid SegmentClass(%d)", uint8(c))
	}
}

// SegmentResult reports the exact classification and, where defined, exact
// rational intersection parameters over the stored float64 endpoints.
//
// Parameter conventions: T is measured along p1->p2 (point = p1 + T*(p2-p1))
// and S along q1->q2. All parameters are class-C1 values: exact rationals
// computed from the stored coordinates by integer arithmetic; they are never
// rounded floats.
//
// On any error IntersectSegments returns the zero SegmentResult whose Class
// is ClassInvalid.
//
// Which fields are set per class:
//
//	ClassDisjoint         no fields.
//	ClassCrossingInterior T, S in the open interval (0,1).
//	ClassTouchEndpoint    T, S with at least one of them equal to 0 or 1.
//	ClassCollinearTouch   T, S at the single shared point.
//	ClassCollinearOverlap OverlapT/OverlapS; T/S nil.
//	ClassCoincident       OverlapT/OverlapS both exactly [0,1]; T/S nil.
//
// Positive-length overlaps have no unique intersection point, so T and S are
// deliberately nil there; use OverlapT/OverlapS instead.
type SegmentResult struct {
	Class    SegmentClass
	T        *big.Rat
	S        *big.Rat
	OverlapT [2]*big.Rat
	OverlapS [2]*big.Rat
}

var (
	ratZero = new(big.Rat).SetInt64(0)
	ratOne  = new(big.Rat).SetInt64(1)
)

// IntersectSegments classifies the contact of closed segments p1->p2 and
// q1->q2 exactly over the stored binary64 endpoints, per the SegmentClass
// documentation. Every sign decision involved (parallelism, collinearity,
// side tests) is exact; parameters are exact rationals.
//
// Errors: ErrNonFinite / ErrOutOfBounds for policy-invalid input,
// ErrDegenerateSegment when either segment has identical endpoints (upstream
// validation rejects such walls earlier per R3.0.5). Invalid input can never
// produce a classification.
func IntersectSegments(p1, p2, q1, q2 Point) (SegmentResult, error) {
	for i, p := range []Point{p1, p2, q1, q2} {
		if err := validatePoint(p); err != nil {
			return SegmentResult{}, fmt.Errorf("point %d: %w", i, err)
		}
	}
	pDegenerate := p1 == p2
	qDegenerate := q1 == q2
	switch {
	case pDegenerate && qDegenerate:
		return SegmentResult{}, fmt.Errorf("both segments: %w", ErrDegenerateSegment)
	case pDegenerate, qDegenerate:
		return SegmentResult{}, ErrDegenerateSegment
	}

	denom := crossOfDiffs(p1.X, p1.Y, p2.X, p2.Y, q1.X, q1.Y, q2.X, q2.Y)
	if denom.Sign() != 0 {
		return intersectNonParallel(p1, p2, q1, q2, denom), nil
	}
	// Parallel: disjoint or collinear. With q non-degenerate and its
	// direction parallel to p's, one endpoint off p's line means both are.
	side := crossOfDiffs(p1.X, p1.Y, p2.X, p2.Y, p1.X, p1.Y, q1.X, q1.Y)
	if side.Sign() != 0 {
		return SegmentResult{Class: ClassDisjoint}, nil
	}
	return intersectCollinear(p1, p2, q1, q2), nil
}

// intersectNonParallel resolves the unique line-intersection candidate and
// tests incidence exactly. denom != 0 holds.
func intersectNonParallel(p1, p2, q1, q2 Point, denom *big.Rat) SegmentResult {
	// With dp = p2-p1, dq = q2-q1, w = q1-p1:
	//   t = cross(w, dq)/cross(dp, dq),  s = cross(w, dp)/cross(dp, dq).
	wx := diffRat(q1.X, p1.X)
	wy := diffRat(q1.Y, p1.Y)
	dqx := diffRat(q2.X, q1.X)
	dqy := diffRat(q2.Y, q1.Y)
	dpx := diffRat(p2.X, p1.X)
	dpy := diffRat(p2.Y, p1.Y)

	numT := new(big.Rat).Sub(
		new(big.Rat).Mul(wx, dqy),
		new(big.Rat).Mul(wy, dqx))
	t := numT.Quo(numT, denom)

	numS := new(big.Rat).Sub(
		new(big.Rat).Mul(wx, dpy),
		new(big.Rat).Mul(wy, dpx))
	s := numS.Quo(numS, denom)

	if !inUnitInterval(t) || !inUnitInterval(s) {
		return SegmentResult{Class: ClassDisjoint}
	}
	class := ClassCrossingInterior
	if isBoundaryParam(t) || isBoundaryParam(s) {
		class = ClassTouchEndpoint
	}
	return SegmentResult{Class: class, T: t, S: s}
}

// intersectCollinear resolves two collinear non-degenerate segments. The
// caller guarantees parallel directions and that q1 lies on p's supporting
// line, hence full collinearity.
func intersectCollinear(p1, p2, q1, q2 Point) SegmentResult {
	lambdaQ1 := paramOnSegment(p1, p2, q1)
	lambdaQ2 := paramOnSegment(p1, p2, q2)

	lo, hi := minRat(lambdaQ1, lambdaQ2), maxRat(lambdaQ1, lambdaQ2)

	aLo := maxRat(lo, ratZero)
	bHi := minRat(hi, ratOne)

	cmp := aLo.Cmp(bHi)
	switch {
	case cmp > 0:
		return SegmentResult{Class: ClassDisjoint}
	case cmp == 0:
		res := SegmentResult{
			Class: ClassCollinearTouch,
			T:     new(big.Rat).Set(aLo),
		}
		res.S = paramAlongQ(q1, q2, p1, p2, res.T)
		return res
	default:
		res := SegmentResult{
			Class:    ClassCollinearOverlap,
			OverlapT: [2]*big.Rat{new(big.Rat).Set(aLo), new(big.Rat).Set(bHi)},
		}
		res.OverlapS[0] = paramAlongQ(q1, q2, p1, p2, res.OverlapT[0])
		res.OverlapS[1] = paramAlongQ(q1, q2, p1, p2, res.OverlapT[1])

		if lo.Sign() == 0 && hi.Cmp(ratOne) == 0 {
			res.Class = ClassCoincident
			// OverlapT is [0,1] by definition; OverlapS keeps the
			// paramAlongQ mapping so a reversed q reports [1,0], which is
			// the correct correspondence T=0 <-> S=1.
			res.OverlapT = [2]*big.Rat{new(big.Rat).Set(ratZero), new(big.Rat).Set(ratOne)}
		}
		return res
	}
}

// paramOnSegment returns the exact rational parameter of pt projected onto
// segment a->b along its direction: dot(pt-a, b-a)/dot(b-a, b-a).
func paramOnSegment(a, b, pt Point) *big.Rat {
	num := dotOfDiffs(a.X, a.Y, b.X, b.Y, a.X, a.Y, pt.X, pt.Y)
	den := dotOfDiffs(a.X, a.Y, b.X, b.Y, a.X, a.Y, b.X, b.Y)
	return num.Quo(num, den)
}

// paramAlongQ maps the point x = p1 + T*(p2-p1) (which lies on q's
// supporting line for collinear inputs) to its exact parameter along
// q1->q2.
func paramAlongQ(q1, q2, p1, p2 Point, t *big.Rat) *big.Rat {
	dqx := diffRat(q2.X, q1.X)
	dqy := diffRat(q2.Y, q1.Y)
	x := offsetRat(floatToRat(p1.X), diffRat(p2.X, p1.X), t)
	y := offsetRat(floatToRat(p1.Y), diffRat(p2.Y, p1.Y), t)

	numX := new(big.Rat).Mul(new(big.Rat).Sub(x, floatToRat(q1.X)), dqx)
	numY := new(big.Rat).Mul(new(big.Rat).Sub(y, floatToRat(q1.Y)), dqy)
	num := numX.Add(numX, numY)
	den := new(big.Rat).Add(
		new(big.Rat).Mul(dqx, dqx),
		new(big.Rat).Mul(dqy, dqy))
	return num.Quo(num, den)
}

// diffRat returns b - a exactly.
func diffRat(b, a float64) *big.Rat {
	return new(big.Rat).Sub(floatToRat(b), floatToRat(a))
}

// offsetRat computes base + t*diff exactly.
func offsetRat(base, diff *big.Rat, t *big.Rat) *big.Rat {
	return new(big.Rat).Add(base, new(big.Rat).Mul(t, diff))
}

func inUnitInterval(r *big.Rat) bool {
	return r.Cmp(ratZero) >= 0 && r.Cmp(ratOne) <= 0
}

func isBoundaryParam(r *big.Rat) bool {
	return r.Cmp(ratZero) == 0 || r.Cmp(ratOne) == 0
}

func minRat(a, b *big.Rat) *big.Rat {
	if a.Cmp(b) <= 0 {
		return a
	}
	return b
}

func maxRat(a, b *big.Rat) *big.Rat {
	if a.Cmp(b) >= 0 {
		return a
	}
	return b
}
