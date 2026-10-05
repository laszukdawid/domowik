package predicates

import (
	"math"
	"math/big"
	"testing"
)

// Tests for the exact along-axis comparator evidence helpers (NUMERICAL-
// MODEL.md section 3 forms for R4.1.2, R4.1.4, R4.3.3). Expected values are
// computed independently here from first principles on small dyadic
// fixtures.

func TestExactSquaredLength(t *testing.T) {
	// (0,0)->(900,0): m = 810000 exactly.
	m, err := ExactSquaredLength(0, 0, 900, 0)
	if err != nil {
		t.Fatal(err)
	}
	if m.Cmp(new(big.Rat).SetInt64(810000)) != 0 {
		t.Fatalf("m = %v", m)
	}

	// (3,4): m = 25.
	m34, err := ExactSquaredLength(0, 0, 3, 4)
	if err != nil {
		t.Fatal(err)
	}
	if m34.Cmp(new(big.Rat).SetInt64(25)) != 0 {
		t.Fatalf("(3,4) length squared = %v, want 25", m34)
	}

	// 9-40-41 triple scaled by 41: (41,0)->(369,1640): dx=328=8*41,
	// dy=1640=40*41 -> m = 41^2*(64+1600) = 1681*1664.
	m2, err := ExactSquaredLength(41, 0, 369, 1640)
	if err != nil {
		t.Fatal(err)
	}
	want := new(big.Rat).SetInt64(1681 * 1664)
	if m2.Cmp(want) != 0 {
		t.Fatalf("m2 = %v want %v", m2, want)
	}

	// Policy errors.
	if _, err := ExactSquaredLength(0, 0, CandidateCoordinateBoundMm*2, 0); !isErr(err, ErrOutOfBounds) {
		t.Fatalf("oob accepted: %v", err)
	}
	if _, err := ExactSquaredLength(math.NaN(), 0, 1, 1); !isErr(err, ErrNonFinite) {
		t.Fatalf("NaN accepted: %v", err)
	}
}

func TestSignStartFitTouchingCase(t *testing.T) {
	// NM section 3 fixture shape: u = 1/2, L = 900, FULL opening width
	// w = 900: the half-width w/2 = 450 equals the remaining clearance;
	// strict fit must be ZERO (zero clearance rejects), never positive.
	m, err := ExactSquaredLength(0, 0, 900, 0)
	if err != nil {
		t.Fatal(err)
	}
	s, err := SignStartFit(0.5, 900, m)
	if err != nil {
		t.Fatal(err)
	}
	if s != SignZero {
		t.Fatalf("touching case sign %v, want zero", s)
	}

	// Slightly smaller width fits strictly.
	s2, err := SignStartFit(0.5, 899, m)
	if err != nil {
		t.Fatal(err)
	}
	if s2 != SignPositive {
		t.Fatalf("fit case sign %v, want positive", s2)
	}

	// Larger width fails strictly.
	s3, err := SignStartFit(0.5, 901, m)
	if err != nil {
		t.Fatal(err)
	}
	if s3 != SignNegative {
		t.Fatalf("no-fit case sign %v, want negative", s3)
	}
}

func TestSignEndFitSymmetry(t *testing.T) {
	m, _ := ExactSquaredLength(0, 0, 1000, 0)
	// End fit at u is start fit at mirrored position only when widths
	// match: check end fit at u=1 is degenerate-zero for any w? At u=1:
	// 4*0*m - w^2 = -w^2 < 0 for w>0.
	s, err := SignEndFit(1, 5, m)
	if err != nil {
		t.Fatal(err)
	}
	if s != SignNegative {
		t.Fatalf("u=1 end fit = %v, want negative", s)
	}
	s0, err := SignEndFit(0, 5, m)
	if err != nil {
		t.Fatal(err)
	}
	if s0 != SignPositive {
		t.Fatalf("u=0 end fit = %v, want positive (full length remains)", s0)
	}
}

func TestSignOverlapForm(t *testing.T) {
	m, _ := ExactSquaredLength(0, 0, 1000, 0)
	// Two identical openings: spans fully overlap, so the form is
	// NEGATIVE (no positive-length gap).
	s, err := SignOverlapForm(0.5, 0.5, 200, 200, m)
	if err != nil {
		t.Fatal(err)
	}
	if s != SignNegative {
		t.Fatalf("coincident centers: %v, want negative", s)
	}
	// Separated far beyond their widths: a real GAP exists between the
	// spans, so |du|*L > (w1+w2)/2 and the form is POSITIVE. (Sign
	// convention of the form: negative = spans overlap with positive
	// length, zero = exactly touching, positive = separated.)
	s2, err := SignOverlapForm(0.1, 0.9, 50, 50, m)
	if err != nil {
		t.Fatal(err)
	}
	if s2 != SignPositive {
		t.Fatalf("separated openings: %v, want positive", s2)
	}
	// Exactly touching spans with dyadic-exact u values:
	// du = 0.25-0.75 = -0.5, du*L = 500 = (500+500)/2 -> exact zero.
	s3, err := SignOverlapForm(0.25, 0.75, 500, 500, m)
	if err != nil {
		t.Fatal(err)
	}
	if s3 != SignZero {
		t.Fatalf("touching spans: %v, want zero", s3)
	}
}

func TestRatStrictlyBetween(t *testing.T) {
	lo := new(big.Rat).SetInt64(1)
	hi := new(big.Rat).SetInt64(2)
	if !RatStrictlyBetween(mustSetString("1.5"), lo, hi) {
		t.Fatal("midpoint not between")
	}
	if RatStrictlyBetween(lo, lo, hi) {
		t.Fatal("boundary counted as strictly between")
	}
	if RatStrictlyBetween(hi, lo, hi) {
		t.Fatal("upper boundary counted as strictly between")
	}
}

// TestAlongAxisFormsAgainstIndependentOracle cross-checks the quadratic
// forms against direct rational computation with independently written
// expressions over oracle-decoded inputs.
func TestAlongAxisFormsAgainstIndependentOracle(t *testing.T) {
	g := newCoordGen(60606)
	for i := 0; i < 500; i++ {
		ax, ay := float64(g.rng.Int63n(2000)), float64(g.rng.Int63n(2000))
		bx, by := float64(g.rng.Int63n(2000)), float64(g.rng.Int63n(2000))
		u := g.rng.Float64()
		w := float64(g.rng.Int63n(3000)) / 2

		mProd, err := ExactSquaredLength(ax, ay, bx, by)
		if err != nil {
			t.Fatal(err)
		}
		// Independent oracle m via oracle-decoded rationals.
		dxO := new(big.Rat).Sub(oracleRat(t, bx), oracleRat(t, ax))
		dyO := new(big.Rat).Sub(oracleRat(t, by), oracleRat(t, ay))
		mOracle := new(big.Rat).Add(
			new(big.Rat).Mul(dxO, dxO),
			new(big.Rat).Mul(dyO, dyO))
		if mProd.Cmp(mOracle) != 0 {
			t.Fatalf("m mismatch: %v vs %v", mProd, mOracle)
		}

		// Independent start-fit evaluation.
		uo := oracleRat(t, u)
		wo := oracleRat(t, w)
		want := new(big.Rat).Mul(uo, uo)
		want.Mul(want, mOracle)
		want.Mul(want, new(big.Rat).SetInt64(4))
		want.Sub(want, new(big.Rat).Mul(wo, wo))

		gotSign, err := SignStartFit(u, w, mProd)
		if err != nil {
			t.Fatal(err)
		}
		if signToOracle(gotSign) != want.Sign() {
			t.Fatalf("start fit mismatch: got %v want %d", gotSign, want.Sign())
		}
	}
}

func TestEvidenceHelperPolicyErrors(t *testing.T) {
	m, _ := ExactSquaredLength(0, 0, 100, 0)
	if _, err := SignStartFit(-0.25, 10, m); err == nil {
		t.Fatal("negative u accepted")
	}
	if _, err := SignStartFit(1.25, 10, m); err == nil {
		t.Fatal("u > 1 accepted")
	}
	if _, err := SignStartFit(math.NaN(), 10, m); !isErr(err, ErrNonFinite) {
		t.Fatal("NaN u accepted")
	}
	if _, err := SignStartFit(0.5, -1, m); err == nil {
		t.Fatal("negative width accepted")
	}
	if _, err := SignOverlapForm(0.5, 0.5, math.Inf(1), 1, m); !isErr(err, ErrNonFinite) {
		t.Fatal("Inf width accepted")
	}
}
