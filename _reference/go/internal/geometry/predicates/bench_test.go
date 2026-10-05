package predicates

import (
	"fmt"
	"math"
	"math/rand"
	"testing"
)

// Benchmarks are separate from correctness tests (no timing assertions
// anywhere in tests). Corpora are deterministic and named so results can be
// reproduced and recorded in docs/evidence/0006-predicates/BENCHMARKS.md.

type orientCorpus struct {
	name  string
	cases [][6]float64 // ax ay bx by cx cy
	// filterFallbackRate records the fraction of cases whose filter stage
	// was inconclusive at construction time (1 for the hard corpus).
	filterFallbackRate float64
}

// report describes the corpus for benchmark evidence records.
func (c orientCorpus) report(b *testing.B) {
	b.ReportMetric(float64(len(c.cases)), "corpus-size")
	if c.filterFallbackRate > 0 {
		b.ReportMetric(c.filterFallbackRate, "filter-fallback-rate")
	}
}

var (
	benchOrientEasy   = buildOrientCorpus(1, 256)
	benchOrientHard   = buildHardOrientCorpus(2, 256)
	benchOrientRandom = buildClassedOrientCorpus(3, 256)
)

func buildOrientCorpus(seed, n int) orientCorpus {
	g := rand.New(rand.NewSource(int64(seed)))
	c := orientCorpus{name: "easy", cases: make([][6]float64, 0, n)}
	for len(c.cases) < n {
		ax, ay := g.Float64()*1e6, g.Float64()*1e6
		bx, by := g.Float64()*1e6, g.Float64()*1e6
		cx, cy := g.Float64()*1e6, g.Float64()*1e6
		if !finiteWithin(ax, ay, bx, by, cx, cy) {
			continue
		}
		c.cases = append(c.cases, [6]float64{ax, ay, bx, by, cx, cy})
	}
	return c
}

// buildHardOrientCorpus builds exactly n near-collinear triples that force
// the filter into its inconclusive band and exercise the exact fallback.
// Every accepted case is verified with filterSign during construction, so
// filterFallbackRate == 1 by construction. The attempt bound only guards
// against a degenerate construction; a shortfall panics rather than
// silently producing a smaller corpus (the fixed seed makes this stable).
func buildHardOrientCorpus(seed, n int) orientCorpus {
	g := rand.New(rand.NewSource(int64(seed)))
	c := orientCorpus{name: "hard-filter-fail", cases: make([][6]float64, 0, n)}
	const maxAttempts = 1000 * 256
	attempts := 0
	for len(c.cases) < n && attempts < maxAttempts {
		attempts++
		ax, ay := float64(g.Int63n(1<<28)), float64(g.Int63n(1<<28))
		bx, by := ax+float64(g.Int63n(1<<12)), ay+float64(g.Int63n(1<<12))
		eps := math.Ldexp(1+g.Float64(), -g.Intn(24)-20)
		cx := ax + eps*(bx-ax)
		cy := ay + eps*(by-ay)
		if !finiteWithin(ax, ay, bx, by, cx, cy) {
			continue
		}
		if _, ok := filterSign(Point{ax, ay}, Point{bx, by}, Point{cx, cy}); !ok {
			c.cases = append(c.cases, [6]float64{ax, ay, bx, by, cx, cy})
		}
	}
	if len(c.cases) != n {
		panic(fmt.Sprintf("hard corpus incomplete: %d/%d filter-failing cases within %d attempts",
			len(c.cases), n, attempts))
	}
	c.filterFallbackRate = 1 // verified per-case at construction
	return c
}

// buildClassedOrientCorpus deterministically constructs exactly n cases
// (no rejection loop) spanning NAMED bit-pattern classes, all within the
// candidate coordinate policy |coord| <= B:
//
//	0: moderate normals, exponents [-30, +30] (geometric-tie dense)
//	1: far-normal SMALL normals, exponents in [-900, -60]
//	2: upper-range normals, exponents [25, 28] (values < 2^29 < B)
//	3: near-bound values in [B/2, B)
//	4: subnormals down to eta scale
//	5: signed zeros
//
// Out-of-policy maxima (MaxFloat64-scale) are deliberately NOT generated
// here; Orientation rejects them by contract and a benchmark corpus must
// consist of admissible inputs only. Policy rejection is covered by tests.
func buildClassedOrientCorpus(seed, n int) orientCorpus {
	g := rand.New(rand.NewSource(int64(seed)))
	c := orientCorpus{name: "classed-bits", cases: make([][6]float64, 0, n)}
	next := func() float64 {
		switch g.Intn(6) {
		case 0: // moderate scale where geometric ties are dense
			// Max exponent 28 keeps values < 2^29 < B for any mantissa.
			return math.Ldexp(1+g.Float64(), g.Intn(59)-30)
		case 1: // far-normal small magnitudes
			return math.Ldexp(1+g.Float64(), -900+g.Intn(840))
		case 2: // upper-range normals within the bound
			// Max exponent 28 keeps values < 2^29 < B; the [B/2,B) band is
			// covered by class 3.
			return math.Ldexp(1+g.Float64(), 25+g.Intn(4))
		case 3: // near the candidate bound B
			return (CandidateCoordinateBoundMm / 2) * (1 + g.Float64())
		case 4: // subnormal
			return math.Ldexp(g.Float64(), -1074+g.Intn(52))
		default: // signed zero
			return math.Copysign(0, g.Float64()-0.5)
		}
	}
	for i := 0; i < n; i++ {
		var v [6]float64
		for j := range v {
			v[j] = next()
		}
		c.cases = append(c.cases, v)
	}
	return c
}

func benchOrientImpl(b *testing.B, c orientCorpus, f func(a, b, p Point) (Sign, error)) {
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		v := c.cases[i%len(c.cases)]
		sign, err := f(Point{v[0], v[1]}, Point{v[2], v[3]}, Point{v[4], v[5]})
		if err != nil {
			b.Fatal(err)
		}
		benchSinkSign = sign
	}
}

// BenchmarkOrientationFiltered measures ONLY the filter stage of the
// adaptive predicate (decision returned when the error bound is decisive;
// inconclusive cases counted, not escalated). Relationship recorded for the
// evidence file: naive < filtered <= adaptive, where adaptive = filtered +
// exact fallback on inconclusive cases.
func BenchmarkOrientationFiltered(b *testing.B) {
	run := func(c orientCorpus) func(b *testing.B) {
		return func(b *testing.B) {
			inconclusive := 0
			c.report(b)
			b.ReportAllocs()
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				v := c.cases[i%len(c.cases)]
				if _, ok := filterSign(Point{v[0], v[1]}, Point{v[2], v[3]}, Point{v[4], v[5]}); !ok {
					inconclusive++
				}
			}
			b.StopTimer()
			b.ReportMetric(float64(inconclusive)/float64(b.N)*100, "%inconclusive")
		}
	}
	b.Run(benchOrientEasy.name, run(benchOrientEasy))
	b.Run(benchOrientHard.name, run(benchOrientHard))
	b.Run(benchOrientRandom.name, run(benchOrientRandom))
}

// benchSinkSign/benchSinkClass prevent dead-code elimination of benchmarked
// results while costing one global store per iteration.
var (
	benchSinkSign   Sign
	benchSinkClass  SegmentClass
	benchSinkDir    Direction
	benchSinkPoint  Point
	benchSinkBounds OffsetVertexCertBounds
)

func BenchmarkOrientationNaive(b *testing.B) {
	for _, c := range []orientCorpus{benchOrientEasy, benchOrientHard, benchOrientRandom} {
		b.Run(c.name, func(b *testing.B) {
			c.report(b)
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				v := c.cases[i%len(c.cases)]
				benchSinkSign = naiveOrient(v[0], v[1], v[2], v[3], v[4], v[5])
			}
		})
	}
}

func BenchmarkOrientationAdaptive(b *testing.B) {
	for _, c := range []orientCorpus{benchOrientEasy, benchOrientHard, benchOrientRandom} {
		b.Run(c.name, func(b *testing.B) {
			c.report(b)
			benchOrientImpl(b, c, Orientation)
		})
	}
}

// naiveIntersect approximates a simple-float classifier baseline: it uses
// plain float cross products without any exactness guarantee.
func naiveIntersectClass(p1, p2, q1, q2 Point) SegmentClass {
	cross := func(ax, ay, bx, by float64) float64 { return ax*by - ay*bx }
	denom := cross(p2.X-p1.X, p2.Y-p1.Y, q2.X-q1.X, q2.Y-q1.Y)
	if denom == 0 {
		side := cross(q1.X-p1.X, q1.Y-p1.Y, q2.X-q1.X, q2.Y-q1.Y)
		if side != 0 {
			return ClassDisjoint
		}
		return ClassCollinearOverlap
	}
	wx, wy := q1.X-p1.X, q1.Y-p1.Y
	tv := cross(wx, wy, q2.X-q1.X, q2.Y-q1.Y) / denom
	sv := cross(wx, wy, p2.X-p1.X, p2.Y-p1.Y) / denom
	inUnit := func(x float64) bool { return x >= 0 && x <= 1 }
	if !inUnit(tv) || !inUnit(sv) {
		return ClassDisjoint
	}
	if tv == 0 || tv == 1 || sv == 0 || sv == 1 {
		return ClassTouchEndpoint
	}
	return ClassCrossingInterior
}

var benchSegs = buildSegCorpus(7, 256)

func buildSegCorpus(seed, n int) [][4]Point {
	g := rand.New(rand.NewSource(int64(seed)))
	out := make([][4]Point, 0, n)
	for len(out) < n {
		p := [4]Point{}
		for i := range p {
			p[i] = Point{
				X: (g.Float64() - 0.5) * 2e6,
				Y: (g.Float64() - 0.5) * 2e6,
			}
		}
		if p[0] != p[1] && p[2] != p[3] {
			out = append(out, p)
		}
	}
	return out
}

func BenchmarkIntersectSegmentsNaive(b *testing.B) {
	b.ReportMetric(float64(len(benchSegs)), "corpus-size")
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		s := benchSegs[i%len(benchSegs)]
		benchSinkClass = naiveIntersectClass(s[0], s[1], s[2], s[3])
	}
}

func BenchmarkIntersectSegmentsRobust(b *testing.B) {
	b.ReportMetric(float64(len(benchSegs)), "corpus-size")
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		s := benchSegs[i%len(benchSegs)]
		res, err := IntersectSegments(s[0], s[1], s[2], s[3])
		if err != nil {
			b.Fatal(err)
		}
		benchSinkClass = res.Class
	}
}

var benchDirs = buildDirCorpus(11, 256)

func buildDirCorpus(seed, n int) [][2]float64 {
	g := rand.New(rand.NewSource(int64(seed)))
	out := make([][2]float64, 0, n)
	for len(out) < n {
		dx, dy := g.Float64()*2e6-1e6, g.Float64()*2e6-1e6
		if dx == 0 && dy == 0 {
			continue
		}
		out = append(out, [2]float64{dx, dy})
	}
	return out
}

func BenchmarkUnitDirection(b *testing.B) {
	b.ReportMetric(float64(len(benchDirs)), "corpus-size")
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		d := benchDirs[i%len(benchDirs)]
		dir, err := UnitDirection(d[0], d[1])
		if err != nil {
			b.Fatal(err)
		}
		benchSinkDir = dir
	}
}

func BenchmarkOffsetCertificateAndVertex(b *testing.B) {
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
		v, verr := OffsetVertex(p, 125, 1, dir)
		if verr != nil {
			b.Fatal(verr)
		}
		benchSinkBounds = bounds
		benchSinkPoint = v
	}
}
