// Package predicates provides robust exact-sign geometric predicates over
// IEEE-754 binary64 coordinates, produced as spike evidence for issue 0006.
//
// # Status and scope
//
// This package is evidence for the candidate numerical model described in
// docs/NUMERICAL-MODEL.md (family C: float64 authoritative coordinates plus
// adaptive exact predicates). That model is a reviewed Candidate, NOT
// ratified; issue 0030 ratifies, revises, or rejects it. Nothing here claims
// ratification or binds product semantics. Only predicates required by the
// candidate's delegated obligations (NUMERICAL-MODEL.md section 11) are
// implemented: orientation (orient2d), segment intersection classification
// with exact rational parameters, gated unit-direction construction
// (section 4.2), and offset-vertex error certificates (section 4.4 terms).
// Polygon booleans, offsets/hulls construction, planar faces, point-in-face
// containment, area/display guards, quantization/snap rounding, and incircle
// are out of scope; see docs/evidence/0006-predicates/FINDINGS.md for the
// recorded incircle verdict.
//
// # Numerical semantics
//
// All exported functions evaluate EXACT SIGN DECISIONS OVER THE STORED
// VALUES: results are the true sign of the stated expression over the exact
// dyadic rationals represented by the passed float64 bit patterns, with no
// tolerance of any kind. A fast filter returns a decision only when the
// computed determinant magnitude provably lies outside its rounding-error
// bound; every other case escalates to an exact fallback over big-integer
// dyadic arithmetic.
//
// FMA independence (correctness never depends on fusion behavior):
// correctness rests on two structural properties, not on preventing fusion.
// First, the exact escalation path forms every decision product with
// INTEGER multiplication (bits.Mul64 / math/big), which no compiler can
// fuse into floating-point FMA. Second, the filter's conservative error
// bound remains valid under BOTH fused and unfused evaluation of the float
// expression: fusing only removes intermediate roundings, which can only
// shrink the covered error. As a bounded compiler observation recorded for
// evidence (exact commands and instruction listings in FINDINGS.md):
// go1.27.0 emits FMADD/FMSUB inside orientFast/unitDirectionGated when
// targeting arm64 and none on amd64; returned signs are identical either
// way because both evaluations stay inside the bound envelope. The section
// 4.1 normality-gating policy is applied before any analytic bound is
// claimed: any non-normal intermediate OR non-normal original operand
// (including ±zero and subnormals, per the literal section 4.1 reading
// recorded in validate.go) routes to the exact path.
//
// # Units, bounds, and finiteness policy
//
// Coordinates are dimensionless floats at this layer; callers pass values in
// millimetres per the behavior specification. The package enforces:
//
//   - Finite-input policy: NaN or ±Inf in any input coordinate yields a
//     typed error (ErrNonFinite). Invalid input can never be mistaken for a
//     geometric decision such as collinearity.
//   - Candidate magnitude bounds: stored coordinates enforce
//     |x|, |y| <= CandidateCoordinateBoundMm and direction arguments
//     (endpoint differences) enforce the COMPONENTWISE bound
//     |dx|, |dy| <= 2*CandidateCoordinateBoundMm; both violations yield
//     ErrOutOfBounds with the exceeded limit named in the message. THESE
//     BOUNDS ARE PROVISIONAL candidates from NUMERICAL-MODEL.md section 3;
//     issue 0030 may change them. They are checked so downstream code
//     cannot silently rely on inputs outside the analyzed domain.
//   - Degenerate segments (zero length) are rejected with
//     ErrDegenerateSegment; upstream validation rejects them earlier per
//     R3.0.5.
//   - Invalid-input results are unambiguous: whenever a call fails, the
//     returned SegmentResult carries ClassInvalid (the zero value), never
//     a valid geometric class, so mishandling the error cannot disguise
//     invalid input as a disjoint-segments decision.
//
// # Orientation convention
//
// Orientation(a, b, c) returns SignPositive when the path a->b->c turns
// counterclockwise (positive signed area), SignNegative when clockwise, and
// SignZero exactly when the three stored points are exactly collinear.
//
// # Determinism
//
// Same toolchain + same released binary + same input bytes => identical
// results. No global mutable state exists; all error values are immutable.
// Cross-platform bit-identity is not claimed (NUMERICAL-MODEL.md section 8);
// correctness does not depend on it.
package predicates

import "errors"

// CandidateCoordinateBoundMm is the provisional coordinate magnitude bound
// proposed in NUMERICAL-MODEL.md section 3 (B = 1e9 mm = 1000 km). It is a
// CANDIDATE value pending ratification by issue 0030 and must not be treated
// as settled product semantics.
const CandidateCoordinateBoundMm = 1e9

// Errors returned by this package. Callers should test with errors.Is.
var (
	// ErrNonFinite reports a NaN or infinite input coordinate.
	ErrNonFinite = errors.New("predicates: non-finite input coordinate")

	// ErrOutOfBounds reports a finite value outside the applicable
	// provisional candidate magnitude bound: stored coordinates against
	// CandidateCoordinateBoundMm (B), direction components (endpoint
	// differences) against the componentwise domain 2B. The exceeded limit
	// is named in the wrapped message.
	ErrOutOfBounds = errors.New("predicates: value outside candidate bound")

	// ErrDegenerateSegment reports a zero-length segment argument.
	ErrDegenerateSegment = errors.New("predicates: degenerate zero-length segment")
)
