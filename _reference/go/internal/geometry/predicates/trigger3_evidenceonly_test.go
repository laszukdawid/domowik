package predicates

// EVIDENCE-ONLY profiling harness for the NUMERICAL-MODEL.md section 11
// trigger-3 disposition (docs/evidence/0030-numerical-model/
// TRIGGER3-DISPOSITION.md). NOT PRODUCTION CODE: these benchmarks exist to
// attribute where time goes inside the two A3 robust-path benchmarks whose
// recorded v3 medians exceeded the registered bound. They carry NO
// threshold, assert nothing about performance, and are excluded from every
// registered workload mapping (criterion addendum: "Any benchmark not listed
// maps to no threshold and is context only").
//
// Provenance (CLEAN-ROOM-POLICY): adapted from this package's committed
// bench_test.go harness patterns (same corpora benchSegs/benchDirs, same
// sink variables, same ReportAllocs discipline). No external source
// consulted. All measured functions are the existing production spike
// implementations, unchanged.

import "testing"

// BenchmarkTrigger3ISRValidateOnly isolates IntersectSegments' four-point
// input-policy validation.
func BenchmarkTrigger3ISRValidateOnly(b *testing.B) {
	b.ReportMetric(float64(len(benchSegs)), "corpus-size")
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		s := benchSegs[i%len(benchSegs)]
		for _, p := range []Point{s[0], s[1], s[2], s[3]} {
			if err := validatePoint(p); err != nil {
				b.Fatal(err)
			}
		}
	}
}

// BenchmarkTrigger3ISRCrossDenomOnly isolates the exact parallelism
// determinant (the first exact decision IntersectSegments makes).
func BenchmarkTrigger3ISRCrossDenomOnly(b *testing.B) {
	b.ReportMetric(float64(len(benchSegs)), "corpus-size")
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		s := benchSegs[i%len(benchSegs)]
		denom := crossOfDiffs(
			s[0].X, s[0].Y, s[1].X, s[1].Y,
			s[2].X, s[2].Y, s[3].X, s[3].Y)
		benchSinkRat = denom
	}
}

// BenchmarkTrigger3ISRNonParallelPath measures the non-parallel resolution
// stage alone over the segment corpus (every corpus entry is non-parallel;
// see TestTrigger3SegmentCorpusClasses below).
func BenchmarkTrigger3ISRNonParallelPath(b *testing.B) {
	b.ReportMetric(float64(len(benchSegs)), "corpus-size")
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		s := benchSegs[i%len(benchSegs)]
		denom := crossOfDiffs(
			s[0].X, s[0].Y, s[1].X, s[1].Y,
			s[2].X, s[2].Y, s[3].X, s[3].Y)
		res := intersectNonParallel(s[0], s[1], s[2], s[3], denom)
		benchSinkClass = res.Class
		benchSinkRat = res.T
	}
}

// BenchmarkTrigger3OCVCertOnly isolates OffsetVertexCertificate.
func BenchmarkTrigger3OCVCertOnly(b *testing.B) {
	b.ReportAllocs()
	p := Point{123456.75, -98765.25}
	for i := 0; i < b.N; i++ {
		d := benchDirs[i%len(benchDirs)]
		dir, err := UnitDirection(d[0], d[1])
		if err != nil {
			b.Fatal(err)
		}
		bounds, cerr := OffsetVertexCertificate(p, 125, 1, dir)
		if cerr != nil {
			b.Fatal(cerr)
		}
		benchSinkBounds = bounds
	}
}

// BenchmarkTrigger3OCVCertOnlyPrebuiltDir removes the UnitDirection call
// from the preceding benchmark by reusing one constructed Direction.
func BenchmarkTrigger3OCVCertOnlyPrebuiltDir(b *testing.B) {
	b.ReportAllocs()
	b.StopTimer()
	p := Point{123456.75, -98765.25}
	dir, err := UnitDirection(benchDirs[0][0], benchDirs[0][1])
	if err != nil {
		b.Fatal(err)
	}
	b.StartTimer()
	for i := 0; i < b.N; i++ {
		bounds, cerr := OffsetVertexCertificate(p, 125, 1, dir)
		if cerr != nil {
			b.Fatal(cerr)
		}
		benchSinkBounds = bounds
	}
}

// BenchmarkTrigger3OCVVertexOnly isolates OffsetVertex.
func BenchmarkTrigger3OCVVertexOnly(b *testing.B) {
	b.ReportAllocs()
	p := Point{123456.75, -98765.25}
	for i := 0; i < b.N; i++ {
		d := benchDirs[i%len(benchDirs)]
		dir, err := UnitDirection(d[0], d[1])
		if err != nil {
			b.Fatal(err)
		}
		v, verr := OffsetVertex(p, 125, 1, dir)
		if verr != nil {
			b.Fatal(verr)
		}
		benchSinkPoint = v
	}
}

// BenchmarkTrigger3OCVValidateDirectionOnly measures one
// validateDirection call on a fixed valid Direction (the structural check
// invoked twice per certified-pair construction).
func BenchmarkTrigger3OCVValidateDirectionOnly(b *testing.B) {
	b.ReportAllocs()
	b.StopTimer()
	dir, err := UnitDirection(benchDirs[0][0], benchDirs[0][1])
	if err != nil {
		b.Fatal(err)
	}
	b.StartTimer()
	for i := 0; i < b.N; i++ {
		if err := validateDirection(dir); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkTrigger3OCVCommitOnly measures the shared commit path
// (commitOffsetVertex) on a prebuilt Direction.
func BenchmarkTrigger3OCVCommitOnly(b *testing.B) {
	b.ReportAllocs()
	b.StopTimer()
	p := Point{123456.75, -98765.25}
	dir, err := UnitDirection(benchDirs[0][0], benchDirs[0][1])
	if err != nil {
		b.Fatal(err)
	}
	b.StartTimer()
	for i := 0; i < b.N; i++ {
		vx, vy, cerr := commitOffsetVertex(p, 125, 1, dir)
		if cerr != nil {
			b.Fatal(cerr)
		}
		benchSinkPoint = Point{X: vx, Y: vy}
	}
}

// bigRatSink mirrors the benchSink pattern for *big.Rat results.
// benchSinkRat prevents dead-code elimination of benchmarked *big.Rat
// results while costing one global store per iteration.
var benchSinkRat = ratZero

// TestTrigger3SegmentCorpusClasses records the class distribution of the
// shared segment benchmark corpus so attribution statements can state which
// IntersectSegments paths the corpus actually exercises.
func TestTrigger3SegmentCorpusClasses(t *testing.T) {
	counts := map[SegmentClass]int{}
	for _, s := range benchSegs {
		res, err := IntersectSegments(s[0], s[1], s[2], s[3])
		if err != nil {
			t.Fatal(err)
		}
		counts[res.Class]++
	}
	for class, n := range counts {
		t.Logf("class %s: %d/%d", class, n, len(benchSegs))
	}
}
