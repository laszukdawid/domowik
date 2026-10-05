package predicates

import (
	"math"
	"math/big"
)

// float64bits exposes the IEEE-754 bit pattern of f.
func float64bits(f float64) uint64 { return math.Float64bits(f) }

// Exact dyadic machinery used by production escalation paths. Every finite
// binary64 value f decomposes uniquely as f = mant * 2**exp with integer
// mant and exp; these helpers evaluate expressions over such values exactly,
// so their sign decisions are ground truth for the stored inputs.

// dyadic is the exact representation of one float64 value.
type dyadic struct {
	mant *big.Int // signed significand; nil only for zero
	exp  int      // power-of-two exponent such that value = mant * 2^exp
}

// decodeDyadic decomposes f into its exact dyadic form from the IEEE-754 bit
// pattern. It handles normals, subnormals, zeros, and the sign of zero.
func decodeDyadic(f float64) dyadic {
	bits := float64bits(f)
	sign := int64(1)
	if bits>>63 != 0 {
		sign = -1
	}
	frac := int64(bits & 0x000fffffffffffff)
	bexp := int((bits >> 52) & 0x7ff)
	switch {
	case bexp == 0x7ff:
		// Callers reject non-finite inputs before decoding; treated as zero
		// here only to keep this decoder total.
		return dyadic{}
	case bexp == 0:
		if frac == 0 {
			return dyadic{} // covers +0 and -0
		}
		// Subnormal: value = frac * 2^-1074.
		return dyadic{mant: big.NewInt(sign * frac), exp: -1074}
	default:
		mant := frac | 0x0010000000000000
		// Normal: value = mant * 2^(bexp - 1023 - 52).
		return dyadic{mant: big.NewInt(sign * mant), exp: bexp - 1075}
	}
}

// rat returns the value as an exact rational.
func (d dyadic) rat() *big.Rat {
	if d.mant == nil {
		return new(big.Rat)
	}
	r := new(big.Rat).SetInt(d.mant)
	if d.exp >= 0 {
		num := new(big.Int).Lsh(r.Num(), uint(d.exp))
		r.SetFrac(num, big.NewInt(1))
	} else {
		den := new(big.Int).Lsh(big.NewInt(1), uint(-d.exp))
		r.SetFrac(r.Num(), den)
	}
	return r
}

// floatToRat converts a validated finite float to its exact rational value.
func floatToRat(f float64) *big.Rat {
	return decodeDyadic(f).rat()
}

// crossOfDiffs computes the exact cross product
//
//	(bx-ax)*(dy-cy) - (by-ay)*(dx-cx)
//
// of the direction vectors (ax,ay)->(bx,by) and (cx,cy)->(dx,dy).
func crossOfDiffs(ax, ay, bx, by, cx, cy, dx, dy float64) *big.Rat {
	dpx := new(big.Rat).Sub(floatToRat(bx), floatToRat(ax))
	dpy := new(big.Rat).Sub(floatToRat(by), floatToRat(ay))
	dqx := new(big.Rat).Sub(floatToRat(dx), floatToRat(cx))
	dqy := new(big.Rat).Sub(floatToRat(dy), floatToRat(cy))

	t1 := new(big.Rat).Mul(dpx, dqy)
	t2 := new(big.Rat).Mul(dpy, dqx)
	return t1.Sub(t1, t2)
}

// dotOfDiffs computes the exact dot product of the direction vectors
// (ax,ay)->(bx,by) and (cx,cy)->(dx,dy).
func dotOfDiffs(ax, ay, bx, by, cx, cy, dx, dy float64) *big.Rat {
	dpx := new(big.Rat).Sub(floatToRat(bx), floatToRat(ax))
	dpy := new(big.Rat).Sub(floatToRat(by), floatToRat(ay))
	dqx := new(big.Rat).Sub(floatToRat(dx), floatToRat(cx))
	dqy := new(big.Rat).Sub(floatToRat(dy), floatToRat(cy))

	t1 := new(big.Rat).Mul(dpx, dqx)
	t2 := new(big.Rat).Mul(dpy, dqy)
	return t1.Add(t1, t2)
}

// ratCmpInt compares r with the integer n exactly.
func ratCmpInt(r *big.Rat, n int64) int {
	return r.Cmp(new(big.Rat).SetFrac(big.NewInt(n), big.NewInt(1)))
}

// halfULPRat returns the exact rational half-ulp of |f|: the maximum
// commitment error of rounding an ideal value to f under
// round-to-nearest-even. Normal f: 0.5*ulp(f) = 2^(bexp-1076). Zero and
// subnormal f: eta/2 = 2^-1075 (NUMERICAL-MODEL.md section 4 conventions;
// ulp of zero/subnormals is eta = 2^-1074).
func halfULPRat(f float64) *big.Rat {
	bits := float64bits(f)
	bexp := int((bits >> 52) & 0x7ff)
	var e int
	if bexp == 0 {
		e = -1075 // eta/2 for zero and subnormals
	} else {
		e = bexp - 1076 // normal: ilogb(f) - 53
	}
	if e >= 0 {
		return new(big.Rat).SetFrac(
			new(big.Int).Lsh(big.NewInt(1), uint(e)), big.NewInt(1))
	}
	return new(big.Rat).SetFrac(
		big.NewInt(1), new(big.Int).Lsh(big.NewInt(1), uint(-e)))
}
