package predicates

import (
	"errors"
	"fmt"
	"hash/fnv"
	"math"
	"testing"
)

// TestOrientationMatchesStatsPath pairs the instrumented route with the
// public Orientation on every corpus case: identical signs and identical
// (nil) errors are required, proving the counted route is observably the
// production route. Note on impact: the production path now routes
// through orientFastStats(nil), adding two nil-pointer comparisons per
// decision on the hot path; BenchmarkOrientationAdaptive measures that
// shipped path under the registered protocol.
func TestOrientationMatchesStatsPath(t *testing.T) {
	for _, c := range []orientCorpus{benchOrientEasy, benchOrientHard, benchOrientRandom} {
		for i, v := range c.cases {
			a, b, p := Point{v[0], v[1]}, Point{v[2], v[3]}, Point{v[4], v[5]}
			sPub, errPub := Orientation(a, b, p)
			sStats, _, errStats := orientationWithStats(a, b, p)
			if (errPub != nil) != (errStats != nil) {
				t.Fatalf("%s case %d: error divergence %v vs %v", c.name, i, errPub, errStats)
			}
			if sPub != sStats {
				t.Fatalf("%s case %d: sign divergence %v vs %v", c.name, i, sPub, sStats)
			}
		}
	}

	// Invalid inputs must be rejected identically by both routes:
	// same sentinel via errors.Is and same exact message.
	bound := CandidateCoordinateBoundMm
	badCases := []struct {
		name    string
		a, b, p Point
		wantErr error
	}{
		{"nan-x", Point{math.NaN(), 0}, Point{1, 0}, Point{0, 1}, ErrNonFinite},
		{"inf-y", Point{0, math.Inf(1)}, Point{1, 0}, Point{0, 1}, ErrNonFinite},
		{"out-of-bound", Point{bound + 1, 0}, Point{1, 0}, Point{0, 1}, ErrOutOfBounds},
		{"out-of-bound-negative", Point{-(bound + 1), 0}, Point{1, 0}, Point{0, 1}, ErrOutOfBounds},
	}
	for _, tc := range badCases {
		_, errPub := Orientation(tc.a, tc.b, tc.p)
		_, _, errStats := orientationWithStats(tc.a, tc.b, tc.p)
		if errPub == nil || errStats == nil {
			t.Fatalf("%s: expected errors, got %v / %v", tc.name, errPub, errStats)
		}
		if !errors.Is(errPub, tc.wantErr) || !errors.Is(errStats, tc.wantErr) {
			t.Fatalf("%s: sentinel mismatch: %v / %v, want %v",
				tc.name, errPub, errStats, tc.wantErr)
		}
		if errPub.Error() != errStats.Error() {
			t.Fatalf("%s: message divergence: %q vs %q",
				tc.name, errPub.Error(), errStats.Error())
		}
	}
}

// TestPhase0H6Runtime records W-H6 escalation/fallback counts at runtime
// through implemented, observable decision paths only. Assertions pin
// per-call counter invariants and the deterministic fallback totals of
// the fixed-seed corpora; any deviation is behavioral drift that must be
// investigated BEFORE measurement, never silently recorded. Workloads
// without an implemented filtered stage (exact big.Rat boolean pipeline,
// CandFaces, OracleFaces, B-1q certification) have no counter here and
// are reported MISSING in evidence, never as zeros.
//
// Labels are canonical: ordinary-control maps to criterion class
// non-adversarial; adversarial fixtures map to adversarial. This mapping
// is separate from the twelve-case boolean corpus partition.
func TestPhase0H6Runtime(t *testing.T) {
	corpora := []struct {
		name           string
		class          string
		criterionClass string
		c              orientCorpus
		wantFilterToEx int // frozen deterministic expectation
	}{
		{benchOrientEasy.name, "ordinary-control", "non-adversarial",
			benchOrientEasy, 0},
		{benchOrientHard.name, "adversarial", "adversarial",
			benchOrientHard, 256},
		{benchOrientRandom.name, "adversarial", "adversarial",
			benchOrientRandom, 231},
	}
	for _, cc := range corpora {
		var dec, fte int
		h := fnv.New64a()
		for i, v := range cc.c.cases {
			s, st, err := orientationWithStats(
				Point{v[0], v[1]}, Point{v[2], v[3]}, Point{v[4], v[5]})
			if err != nil {
				t.Fatalf("%s: %v", cc.name, err)
			}
			if st.decisions != 1 {
				t.Fatalf("%s case %d: decisions=%d, want exactly 1", cc.name, i, st.decisions)
			}
			if st.filterToExact != 0 && st.filterToExact != 1 {
				t.Fatalf("%s case %d: filterToExact=%d, want 0 or 1", cc.name, i, st.filterToExact)
			}
			dec += st.decisions
			fte += st.filterToExact
			fmt.Fprintf(h, "%d\n", int8(s))
		}
		if dec != len(cc.c.cases) {
			t.Fatalf("%s: aggregate decisions=%d, want %d", cc.name, dec, len(cc.c.cases))
		}
		if fte != cc.wantFilterToEx {
			t.Fatalf("%s: frozen fallback total drift: got %d, want %d; investigate before measuring",
				cc.name, fte, cc.wantFilterToEx)
		}
		t.Logf("H6-CORPUS name=%s class=%s criterionClass=%s cases=%d decisions=%d filterToExact=%d fallbackRate=%.6f signDigest=%016x",
			cc.name, cc.class, cc.criterionClass, len(cc.c.cases), dec, fte,
			float64(fte)/float64(dec), h.Sum64())
	}

	const wantCases = 256
	if len(benchDirs) != wantCases {
		t.Fatalf("direction corpus size %d, want %d", len(benchDirs), wantCases)
	}
	gatedFast := 0
	highPrec := 0
	hd := fnv.New64a()
	for _, d := range benchDirs {
		dir, err := UnitDirection(d[0], d[1])
		if err != nil {
			t.Fatalf("unit direction: %v", err)
		}
		switch dir.Path {
		case PathGatedFast:
			gatedFast++
		case PathHighPrecisionFallback:
			highPrec++
		default:
			t.Fatalf("unknown DirectionPath %v", dir.Path)
		}
		fmt.Fprintf(hd, "%v %.17g %.17g\n", dir.Path, dir.NX, dir.NY)
	}
	if gatedFast+highPrec != wantCases {
		t.Fatalf("direction path counts %d+%d do not sum to %d",
			gatedFast, highPrec, wantCases)
	}
	// Absent paths are reported explicitly as zero so omission can never
	// be confused with missing instrumentation.
	t.Logf("H6-DIRECTION cases=%d criterionClass=non-adversarial gated-fast=%d high-precision-fallback=%d dirDigest=%016x",
		wantCases, gatedFast, highPrec, hd.Sum64())
}
