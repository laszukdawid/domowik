package predicates

import (
	"math/big"
	"math/bits"
)

// Integer-arithmetic helpers for the exact orientation escalation path.
//
// FMA-independence is structural here: every decision-relevant product is
// formed by INTEGER multiplication (bits.Mul64 fast path, math/big
// general path). Integer operations are never eligible for floating-point
// FMA fusion, so no compiler transformation can change a returned sign.

// intMul sets z = x*y using the bits.Mul64 two-product fast path when both
// operands fit int64, and falls back to math/big multiplication otherwise.
// It returns z.
func intMul(z, x, y *big.Int) *big.Int {
	if x.IsInt64() && y.IsInt64() {
		xi, yi := x.Int64(), y.Int64()
		neg := (xi < 0) != (yi < 0)
		ux, uy := uint64(abs64(xi)), uint64(abs64(yi))
		hi, lo := bits.Mul64(ux, uy)
		z.SetUint64(hi)
		z.Lsh(z, 64)
		z.Or(z, new(big.Int).SetUint64(lo))
		if neg {
			z.Neg(z)
		}
		return z
	}
	return z.Mul(x, y)
}

func abs64(v int64) int64 {
	if v < 0 {
		return -v
	}
	return v
}

// decodeCoord decomposes a finite binary64 coordinate into its exact signed
// significand and exponent: f = mant * 2**exp with |mant| <= 2^53. Zero
// (including signed zero) decodes as mant=0, exp=0, so the integer path is
// total over the validated input domain.
func decodeCoord(f float64) (mant int64, exp int) {
	if f == 0 {
		return 0, 0 // covers +0 and -0
	}
	d := decodeDyadic(f)
	return d.mant.Int64(), d.exp
}

// scaleCoord returns X = mant << shift as a fresh big.Int.
func scaleCoord(mant int64, shift uint) *big.Int {
	return new(big.Int).Lsh(big.NewInt(mant), shift)
}

// orientExactScaled decides the orientation sign through uniformly scaled
// integer arithmetic: every coordinate becomes an exact integer
// X_i = mant_i * 2^(1074+exp_i), which preserves the determinant's sign
// because the shared factor 2^-2148 scales both products equally. This is
// the expansion-free scaled-integer escalation path required by
// NUMERICAL-MODEL.md section 11 obligation 1.
func orientExactScaled(a, b, c Point) Sign {
	coords := [6]float64{a.X, a.Y, b.X, b.Y, c.X, c.Y}
	X := [6]*big.Int{}
	for i, f := range coords {
		mant, exp := decodeCoord(f)
		X[i] = scaleCoord(mant, uint(1074+exp))
	}
	dxab := new(big.Int).Sub(X[2], X[0])
	dyab := new(big.Int).Sub(X[3], X[1])
	dxac := new(big.Int).Sub(X[4], X[0])
	dyac := new(big.Int).Sub(X[5], X[1])

	p1 := intMul(new(big.Int), dxab, dyac)
	p2 := intMul(new(big.Int), dyab, dxac)
	det := p1.Sub(p1, p2)
	return ratSignFromInt(det)
}

func ratSignFromInt(z *big.Int) Sign {
	switch z.Sign() {
	case 1:
		return SignPositive
	case -1:
		return SignNegative
	default:
		return SignZero
	}
}
