// Package geometry contains the issue-0007 clipping and planar
// room-topology spike. It is evidence-producing code for tracker issue
// 0007 only: it defines no product API and carries no ratified status.
//
// Exactness policy (see docs/evidence/0007-booleans-topology.md):
//
//   - xnum values live either in Q (ratNum) or in a single fixed
//     single-radicand quadratic field Q(sqrt(d)) with d > 1 squarefree
//     (quadNum). Every value carries its field; combining values with
//     different field tags panics deterministically rather than
//     comparing across extensions.
//   - The oracle (oracle*.go) evaluates stored binary64 inputs exactly
//     through these fields. This is the NUMERICAL-MODEL section 6
//     independent exact-decision oracle restricted to the corpus's
//     rationally representable and single-radicand constructions.
package geometry

import (
	"math"
	"math/big"
	"sync"
)

// xnum is an element of an ordered exact number field used by the
// oracle: the rationals Q or a quadratic extension Q(sqrt(d)) for one
// fixed squarefree integer d > 1. Implementations must panic on any
// operation whose operands carry different field tags.
type xnum interface {
	Add(o xnum) xnum
	Sub(o xnum) xnum
	Mul(o xnum) xnum
	Div(o xnum) xnum
	Neg() xnum
	Sign() int
	IsZero() bool
	// Rat returns the exact rational value when the field is Q.
	Rat() (*big.Rat, bool)
	// sameField reports whether o belongs to the same field.
	sameField(o xnum) bool
}

// xcmp compares two field elements of the same field.
func xcmp(a, b xnum) int { return a.Sub(b).Sign() }

// ratNum implements xnum over big.Rat.
type ratNum struct{ v *big.Rat }

func xr(v *big.Rat) xnum { return ratNum{v} }

// XRatio constructs an exact rational from an integer ratio.
func XRatio(num, den int64) xnum {
	return xr(new(big.Rat).SetFrac64(num, den))
}

// XFloat converts a finite binary64 value to its exact dyadic rational.
// NaN and ±Inf are rejected deterministically with a SpikeError rather
// than relying on library panic behavior; callers treat stored
// coordinates as validated before constructing oracle values.
func XFloat(f float64) (xnum, error) {
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return nil, &SpikeError{"nonfinite-input"}
	}
	r := new(big.Rat)
	r.SetFloat64(f)
	return xr(r), nil
}

func (a ratNum) Add(o xnum) xnum {
	b, ok := o.(ratNum)
	if !ok || !o.sameField(a) {
		panic("geometry: mixed exact fields")
	}
	return xr(new(big.Rat).Add(a.v, b.v))
}

func (a ratNum) Sub(o xnum) xnum {
	b, ok := o.(ratNum)
	if !ok || !o.sameField(a) {
		panic("geometry: mixed exact fields")
	}
	return xr(new(big.Rat).Sub(a.v, b.v))
}

func (a ratNum) Mul(o xnum) xnum {
	b, ok := o.(ratNum)
	if !ok || !o.sameField(a) {
		panic("geometry: mixed exact fields")
	}
	return xr(new(big.Rat).Mul(a.v, b.v))
}

func (a ratNum) Div(o xnum) xnum {
	b, ok := o.(ratNum)
	if !ok || !o.sameField(a) {
		panic("geometry: mixed exact fields")
	}
	if b.v.Sign() == 0 {
		panic("geometry: division by zero")
	}
	return xr(new(big.Rat).Quo(a.v, b.v))
}

func (a ratNum) Neg() xnum { return xr(new(big.Rat).Neg(a.v)) }
func (a ratNum) Sign() int { return a.v.Sign() }
func (a ratNum) IsZero() bool {
	return a.v.Sign() == 0
}
func (a ratNum) Rat() (*big.Rat, bool) { return a.v, true }
func (a ratNum) sameField(o xnum) bool {
	_, ok := o.(ratNum)
	return ok
}

// quadNum implements xnum over p + q*sqrt(d) for one fixed squarefree
// integer d > 1. The field tag is d; values with different d must never
// be combined.
type quadNum struct {
	p, q *big.Rat
	d    int64
}

// XQuad constructs p + q*sqrt(d). d must exceed 1 and be squarefree;
// the caller owns that obligation. The operands are copied: callers
// retain ownership of their big.Rat values.
func XQuad(p, q *big.Rat, d int64) xnum {
	return quadNum{p: new(big.Rat).Set(p), q: new(big.Rat).Set(q), d: d}
}

// XLift lifts an exact rational into the quadratic field Q(sqrt(d)).
// Every mixed rational-plus-surd construction must lift its rational
// terms through this function so all operands share one field tag.
func XLift(r *big.Rat, d int64) xnum {
	return XQuad(r, new(big.Rat).SetInt64(0), d)
}

func (a quadNum) sameField(o xnum) bool {
	b, ok := o.(quadNum)
	return ok && b.d == a.d
}

func (a quadNum) Add(o xnum) xnum {
	b := o.(quadNum)
	if !o.sameField(a) {
		panic("geometry: mixed quadratic fields")
	}
	return quadNum{p: new(big.Rat).Add(a.p, b.p), q: new(big.Rat).Add(a.q, b.q), d: a.d}
}

func (a quadNum) Sub(o xnum) xnum {
	b := o.(quadNum)
	if !o.sameField(a) {
		panic("geometry: mixed quadratic fields")
	}
	return quadNum{p: new(big.Rat).Sub(a.p, b.p), q: new(big.Rat).Sub(a.q, b.q), d: a.d}
}

func (a quadNum) Mul(o xnum) xnum {
	b := o.(quadNum)
	if !o.sameField(a) {
		panic("geometry: mixed quadratic fields")
	}
	pp := new(big.Rat).Mul(a.p, b.p)
	qq := new(big.Rat).Mul(new(big.Rat).Mul(a.q, b.q), new(big.Rat).SetInt64(a.d))
	pq := new(big.Rat).Mul(a.p, b.q)
	qp := new(big.Rat).Mul(a.q, b.p)
	return quadNum{
		p: new(big.Rat).Add(pp, qq),
		q: new(big.Rat).Add(pq, qp),
		d: a.d,
	}
}

func (a quadNum) Div(o xnum) xnum {
	b := o.(quadNum)
	if !o.sameField(a) {
		panic("geometry: mixed quadratic fields")
	}
	norm := new(big.Rat).Sub(
		new(big.Rat).Mul(b.p, b.p),
		new(big.Rat).Mul(new(big.Rat).Mul(b.q, b.q), new(big.Rat).SetInt64(b.d)))
	if norm.Sign() == 0 {
		panic("geometry: division by zero")
	}
	conj := quadNum{p: new(big.Rat).Set(b.p), q: new(big.Rat).Neg(b.q), d: a.d}
	num := a.Mul(conj).(quadNum)
	return quadNum{
		p: num.p.Quo(num.p, norm),
		q: num.q.Quo(num.q, norm),
		d: a.d,
	}
}

func (a quadNum) Neg() xnum {
	return quadNum{p: new(big.Rat).Neg(a.p), q: new(big.Rat).Neg(a.q), d: a.d}
}

// Sign terminates exactly: zero iff both coefficients vanish; when the
// coefficient signs agree the value's sign is that sign; otherwise the
// dominant term decides by exact comparison of p^2 against q^2*d.
func (a quadNum) Sign() int {
	sp, sq := a.p.Sign(), a.q.Sign()
	switch {
	case sp == 0 && sq == 0:
		return 0
	case sq == 0:
		return sp
	case sp == 0:
		return sq
	case sp == sq:
		return sp
	default:
		e := new(big.Rat).Sub(
			new(big.Rat).Mul(new(big.Rat).Mul(a.q, a.q), new(big.Rat).SetInt64(a.d)),
			new(big.Rat).Mul(a.p, a.p))
		switch {
		case e.Sign() > 0:
			return sq // |q|sqrt(d) dominates
		case e.Sign() < 0:
			return sp // |p| dominates
		default:
			return 0 // unreachable for squarefree d > 1 with rational p,q
		}
	}
}

func (a quadNum) IsZero() bool { return a.p.Sign() == 0 && a.q.Sign() == 0 }

func (a quadNum) Rat() (*big.Rat, bool) {
	if a.q.Sign() != 0 {
		return nil, false
	}
	return a.p, true
}

// Mixed-field arithmetic panics are the deterministic field-tag
// rejection mechanism. GuardMixed converts that rejection into a
// counted error so corpus runs can assert zero silent cross-field
// events while keeping hot-path overhead at zero.
var mixedFieldEvents int64 // guarded by mu

var mixedFieldMu sync.Mutex

// MixedFieldEvents reports how many guarded cross-field operations
// were rejected (H-0/H-3 evidence; expected zero on the corpus).
// Mutex-guarded: safe under concurrent corpus/bench runs.
func MixedFieldEvents() int64 {
	mixedFieldMu.Lock()
	defer mixedFieldMu.Unlock()
	return mixedFieldEvents
}

// GuardMixed runs op, converting a mixed-field deterministic panic
// into a counted error. Race-clean by mutex.
func GuardMixed(op func()) (err error) {
	defer func() {
		if r := recover(); r != nil {
			mixedFieldMu.Lock()
			mixedFieldEvents++
			mixedFieldMu.Unlock()
			err = &SpikeError{"mixed-field-operation"}
		}
	}()
	op()
	return nil
}

// Field validates d > 1 and squarefree, then returns the field tag for
// constructors. Quadratic-field corpus constructions must obtain their
// tag here so non-squarefree radicands are rejected deterministically
// instead of silently breaking the p+q*sqrt(d) basis argument.
func Field(d int64) (int64, error) {
	if d <= 1 {
		return 0, &SpikeError{"invalid-field"}
	}
	r := d
	for f := int64(2); f*f <= r; f++ {
		if r%f == 0 {
			r /= f
			if r%f == 0 {
				return 0, &SpikeError{"field-not-squarefree"}
			}
		}
	}
	return d, nil
}
