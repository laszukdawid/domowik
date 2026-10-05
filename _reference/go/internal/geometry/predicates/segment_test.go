package predicates

import (
	"math"
	"math/big"
	"math/rand"
	"testing"
)

func mustSetString(s string) *big.Rat {
	r, ok := new(big.Rat).SetString(s)
	if !ok {
		panic("bad rational literal " + s)
	}
	return r
}

func mustIntersect(t *testing.T, p1, p2, q1, q2 Point) SegmentResult {
	t.Helper()
	res, err := IntersectSegments(p1, p2, q1, q2)
	if err != nil {
		t.Fatalf("IntersectSegments(%v,%v,%v,%v) error: %v", p1, p2, q1, q2, err)
	}
	return res
}

func ratFromInt(n int64) *big.Rat { return new(big.Rat).SetInt64(n) }

// TestSegmentClassMatrix covers every SegmentClass with hand-built fixtures
// whose exact parameters are known integers/halves.
func TestSegmentClassMatrix(t *testing.T) {
	cases := []struct {
		name string
		seg  [4]Point
		want SegmentClass
		tT   string // expected T as decimal fraction; "" for absent
		tS   string
	}{
		{"proper-crossing", [4]Point{{0, 0}, {10, 10}, {0, 10}, {10, 0}}, ClassCrossingInterior, "0.5", "0.5"},
		{"endpoint-endpoint-angle", [4]Point{{0, 0}, {4, 0}, {4, 0}, {4, 4}}, ClassTouchEndpoint, "1", "0"},
		{"endpoint-on-interior", [4]Point{{0, 0}, {4, 0}, {2, 0}, {2, 4}}, ClassTouchEndpoint, "0.5", "0"},
		{"interior-on-endpoint", [4]Point{{2, 4}, {2, -4}, {0, 0}, {2, 0}}, ClassTouchEndpoint, "0.5", "1"},
		{"collinear-touch", [4]Point{{0, 0}, {4, 0}, {4, 0}, {8, 0}}, ClassCollinearTouch, "1", "0"},
		{"collinear-overlap-partial", [4]Point{{0, 0}, {4, 0}, {-2, 0}, {2, 0}}, ClassCollinearOverlap, "", ""},
		{"collinear-containment", [4]Point{{0, 0}, {10, 0}, {2, 0}, {8, 0}}, ClassCollinearOverlap, "", ""},
		{"coincident", [4]Point{{0, 0}, {4, 0}, {0, 0}, {4, 0}}, ClassCoincident, "", ""},
		{"disjoint-crossing-lines", [4]Point{{0, 0}, {1, 1}, {5, 0}, {6, 1}}, ClassDisjoint, "", ""},
		{"parallel-disjoint", [4]Point{{0, 0}, {4, 0}, {0, 3}, {4, 3}}, ClassDisjoint, "", ""},
		{"collinear-disjoint", [4]Point{{0, 0}, {4, 0}, {9, 0}, {12, 0}}, ClassDisjoint, "", ""},
	}
	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			got := mustIntersect(t, tc.seg[0], tc.seg[1], tc.seg[2], tc.seg[3])
			if got.Class != tc.want {
				t.Fatalf("class: got %v want %v", got.Class, tc.want)
			}
			if tc.tT == "" {
				if got.T != nil || got.S != nil {
					t.Fatalf("T/S must be nil for %v, got %v/%v", got.Class, got.T, got.S)
				}
				return
			}
			wantT, _ := new(big.Rat).SetString(tc.tT)
			wantS, _ := new(big.Rat).SetString(tc.tS)
			if got.T == nil || got.S == nil || got.T.Cmp(wantT) != 0 || got.S.Cmp(wantS) != 0 {
				t.Fatalf("params: got T=%v S=%v want %v/%v", got.T, got.S, wantT, wantS)
			}
		})
	}

	t.Run("containment-intervals", func(t *testing.T) {
		// q strictly inside p: overlap interval is exactly q's own span.
		res := mustIntersect(t, Point{0, 0}, Point{10, 0}, Point{2, 0}, Point{8, 0})
		if res.Class != ClassCollinearOverlap {
			t.Fatalf("class %v", res.Class)
		}
		if res.OverlapT[0].Cmp(mustSetString("0.2")) != 0 ||
			res.OverlapT[1].Cmp(mustSetString("0.8")) != 0 ||
			res.OverlapS[0].Cmp(ratZero) != 0 ||
			res.OverlapS[1].Cmp(ratOne) != 0 {
			t.Fatalf("containment intervals T=%v..%v S=%v..%v",
				res.OverlapT[0], res.OverlapT[1], res.OverlapS[0], res.OverlapS[1])
		}
	})

	t.Run("overlap-intervals", func(t *testing.T) {
		res := mustIntersect(t, Point{0, 0}, Point{4, 0}, Point{-2, 0}, Point{2, 0})
		if res.Class != ClassCollinearOverlap {
			t.Fatalf("class %v", res.Class)
		}
		wantOT := [2]*big.Rat{ratFromInt(0), mustSetString("0.5")}
		wantOS := [2]*big.Rat{mustSetString("0.5"), ratFromInt(1)}
		for i := 0; i < 2; i++ {
			if res.OverlapT[i] == nil || res.OverlapT[i].Cmp(wantOT[i]) != 0 ||
				res.OverlapS[i] == nil || res.OverlapS[i].Cmp(wantOS[i]) != 0 {
				t.Fatalf("interval %d: got T=%v S=%v", i, res.OverlapT[i], res.OverlapS[i])
			}
		}
	})

	t.Run("coincident-reversed-intervals", func(t *testing.T) {
		res := mustIntersect(t, Point{0, 0}, Point{4, 0}, Point{4, 0}, Point{0, 0})
		if res.Class != ClassCoincident {
			t.Fatalf("class %v", res.Class)
		}
		if res.OverlapT[0].Cmp(ratZero) != 0 || res.OverlapT[1].Cmp(ratOne) != 0 {
			t.Fatalf("OverlapT: %v %v", res.OverlapT[0], res.OverlapT[1])
		}
		// Reversed q maps T=0 to S=1 and T=1 to S=0.
		if res.OverlapS[0].Cmp(ratOne) != 0 || res.OverlapS[1].Cmp(ratZero) != 0 {
			t.Fatalf("reversed OverlapS: %v %v", res.OverlapS[0], res.OverlapS[1])
		}
	})

	t.Run("collinear-touch-endpoint-of-both-sides", func(t *testing.T) {
		// p spans [-2,6] along x, q spans [6,10]: single touch at x=6,
		// which is p's endpoint (T=1) and q's endpoint (S=0).
		res := mustIntersect(t, Point{-2, 0}, Point{6, 0}, Point{6, 0}, Point{10, 0})
		if res.Class != ClassCollinearTouch {
			t.Fatalf("class %v", res.Class)
		}
		wantT := mustSetString("1")
		wantS := mustSetString("0")
		if res.T.Cmp(wantT) != 0 || res.S.Cmp(wantS) != 0 {
			t.Fatalf("params T=%v S=%v", res.T, res.S)
		}
	})
}

// TestSegmentDifferentialAgainstOracle fuzzes all classes including
// collinear constructions against the independent oracle.
func TestSegmentDifferentialAgainstOracle(t *testing.T) {
	g := rand.New(rand.NewSource(20260823))
	lerp := func(a, b Point, tr *big.Rat) Point {
		ax := new(big.Rat).SetFloat64(a.X)
		ay := new(big.Rat).SetFloat64(a.Y)
		bx := new(big.Rat).SetFloat64(b.X)
		by := new(big.Rat).SetFloat64(b.Y)
		numX := new(big.Rat).Add(ax, new(big.Rat).Mul(tr, new(big.Rat).Sub(bx, ax)))
		numY := new(big.Rat).Add(ay, new(big.Rat).Mul(tr, new(big.Rat).Sub(by, ay)))
		fx, _ := numX.Float64()
		fy, _ := numY.Float64()
		return Point{fx, fy}
	}
	genPoint := func() Point {
		for {
			f := math.Ldexp(1+g.Float64(), g.Intn(14)-7) // ~2^-7..2^7 scale
			return Point{f * float64(g.Intn(64)-32), f * float64(g.Intn(64)-32)}
		}
	}
	collinearSeg := func() (p1, p2, q1, q2 Point) {
		p1 = genPoint()
		p2 = genPoint()
		// q endpoints exactly on p's line via parameter interpolation on
		// dyadic-friendly values.
		q1 = lerp(p1, p2, big.NewRat(int64(g.Intn(5)-2), 4))
		q2 = lerp(p1, p2, big.NewRat(int64(g.Intn(7)-1), 3))
		return
	}

	for i := 0; i < 4000; i++ {
		var p1, p2, q1, q2 Point
		switch g.Intn(3) {
		case 0:
			p1, p2, q1, q2 = genPoint(), genPoint(), genPoint(), genPoint()
		case 1:
			p1, p2, q1, q2 = collinearSeg()
		default:
			p1, p2, q1, q2 = genPoint(), genPoint(), genPoint(), genPoint()
			switch g.Intn(3) {
			case 0:
				q1 = p2 // shared endpoint
			case 1:
				q1 = lerp(p1, p2, mustSetString("0.25"))
			case 2:
				q2 = p1
			}
		}
		if (p1 == p2) || (q1 == q2) {
			continue
		}
		got := mustIntersect(t, p1, p2, q1, q2)
		want := oracleIntersect(t, p1, p2, q1, q2)

		if got.Class != want.class {
			t.Fatalf("case %d class: got %v want %v at %v,%v,%v,%v",
				i, got.Class, want.class, p1, p2, q1, q2)
		}
		if !ratsEqual(got.T, ptrOrNil(want.hasTS, want.t)) ||
			!ratsEqual(got.S, ptrOrNil(want.hasTS, want.s)) {
			t.Fatalf("case %d params: got T=%v S=%v want T=%v S=%v",
				i, got.T, got.S, want.t, want.s)
		}
		if want.overlapT != nil {
			for j := 0; j < 2; j++ {
				if !ratsEqual(got.OverlapT[j], want.overlapT[j]) ||
					!ratsEqual(got.OverlapS[j], want.overlapS[j]) {
					t.Fatalf("case %d interval %d: got T=%v S=%v want T=%v S=%v",
						i, j, got.OverlapT[j], got.OverlapS[j],
						want.overlapT[j], want.overlapS[j])
				}
			}
		}
	}
}

func ptrOrNil(has bool, r *big.Rat) *big.Rat {
	if !has {
		return nil
	}
	return r
}

// TestSegmentReversalInvariance checks the documented transformation laws:
// reversing p maps T -> 1-T and preserves the class; swapping operand order
// swaps T<->S symmetrically.
func TestSegmentReversalInvariance(t *testing.T) {
	base := mustIntersect(t, Point{0, 0}, Point{10, 0}, Point{4, -4}, Point{4, 4})
	revP := mustIntersect(t, Point{10, 0}, Point{0, 0}, Point{4, -4}, Point{4, 4})
	if base.Class != revP.Class || base.Class != ClassCrossingInterior {
		t.Fatalf("classes %v / %v", base.Class, revP.Class)
	}
	oneMinusT := new(big.Rat).Sub(ratOne, base.T)
	if revP.T.Cmp(oneMinusT) != 0 || revP.S.Cmp(base.S) != 0 {
		t.Fatalf("reversal params: got T=%v S=%v want T=%v S=%v",
			revP.T, revP.S, oneMinusT, base.S)
	}

	swap := mustIntersect(t, Point{4, -4}, Point{4, 4}, Point{0, 0}, Point{10, 0})
	if swap.Class != ClassCrossingInterior || swap.T.Cmp(base.S) != 0 || swap.S.Cmp(base.T) != 0 {
		t.Fatalf("operand swap params: got T=%v S=%v", swap.T, swap.S)
	}

	// Collinear overlap reversal: interval transforms to [1-b, 1-a].
	ov := mustIntersect(t, Point{0, 0}, Point{4, 0}, Point{-2, 0}, Point{2, 0})
	ovRev := mustIntersect(t, Point{4, 0}, Point{0, 0}, Point{-2, 0}, Point{2, 0})
	if ovRev.Class != ClassCollinearOverlap {
		t.Fatalf("class %v", ovRev.Class)
	}
	w0 := new(big.Rat).Sub(ratOne, ov.OverlapT[1])
	w1 := new(big.Rat).Sub(ratOne, ov.OverlapT[0])
	if ovRev.OverlapT[0].Cmp(w0) != 0 || ovRev.OverlapT[1].Cmp(w1) != 0 {
		t.Fatalf("reversed overlap T: %v %v want [%v,%v]",
			ovRev.OverlapT[0], ovRev.OverlapT[1], w0, w1)
	}
}
