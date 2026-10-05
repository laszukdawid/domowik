package geometry

import "testing"

// Benchmarks run the SAME named corpus as the correctness harness
// (AC6): identical inputs, environment, and build for wall-clock and
// allocation evidence. No thresholds exist here; tracker 0030 judges
// the recorded numbers.
//
// Inclusion lists (stated for the evidence document): boolean
// benchmarks cover the FULL named corpus including the near-B case;
// CandFaces benchmarks exclude boolOnly cases per the documented
// axis-aligned integral domain narrowing. The oracle benchmark is
// reference machinery, not a production path.

// benchBooleans runs the timed loop WITHOUT b.Fatal: deterministic
// per-case candidate rejections are counted and reported via
// ReportMetric so raw evidence for the full corpus is emitted even
// when a case rejects. Correctness diagnostics stay in tests.
func benchBooleans(b *testing.B, pol CandPolicy) {
	rejects := 0
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		for _, tc := range buildCorpus() {
			ca, cb := candRectsHalved(tc.rects)
			if _, err := CandBoolean(candMemUnion, pol, ca, cb); err != nil {
				rejects++
			}
		}
	}
	b.ReportMetric(float64(rejects), "rejects/op")
}

func BenchmarkCorpusBooleanB1X(b *testing.B) { benchBooleans(b, polX) }

func BenchmarkCorpusBooleanB1Q(b *testing.B) { benchBooleans(b, polQ) }

func BenchmarkCorpusCandFaces(b *testing.B) {
	b.ReportAllocs()
	cases := make([][][2][2]float64, 0)
	for _, tc := range buildCorpus() {
		if tc.boolOnly {
			continue // outside documented candidate-face domain
		}
		cases = append(cases, rectsToSegs(tc.rects))
	}
	b.ResetTimer()
	faceRejects := 0
	for i := 0; i < b.N; i++ {
		for _, segs := range cases {
			if _, err := CandFaces(segs); err != nil {
				faceRejects++
			}
		}
	}
	b.ReportMetric(float64(faceRejects), "rejects/op")
}

// BenchmarkPhase0BooleanB1X is the Phase 0 performance-criterion A1/A2
// workload: one sub-benchmark per named corpus case, B-1x policy only.
// All deterministic operand conversion and building happens before
// b.ResetTimer; the timed region is exactly the CandBoolean call
// (conversion, validation, sweep, normalization, terminal commitment).
// Per-case medians are judged by tracker 0030 under the registered
// protocol; no threshold exists in this file. Any nonzero rejects/op
// value must be surfaced in evidence as candidate rejections, never
// silently treated as valid measured work.
func BenchmarkPhase0BooleanB1X(b *testing.B) {
	for _, tc := range buildCorpus() {
		tc := tc
		ca, cb := candRectsHalved(tc.rects)
		b.Run(tc.name, func(b *testing.B) {
			b.ReportAllocs()
			rejects := 0
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				if _, err := CandBoolean(candMemUnion, polX, ca, cb); err != nil {
					rejects++
				}
			}
			b.StopTimer()
			b.ReportMetric(float64(rejects), "rejects/op")
		})
	}
}

func BenchmarkCorpusOracleFaces(b *testing.B) {
	// Oracle extraction is correctness machinery, not a production
	// path; measured once per iteration for reference only.
	b.ReportAllocs()
	cases := make([][]oSeg, 0)
	for _, tc := range buildCorpus() {
		var segs []oSeg
		for _, sg := range rectsToSegs(tc.rects) {
			ax, _ := XFloat(sg[0][0])
			ay, _ := XFloat(sg[0][1])
			bx, _ := XFloat(sg[1][0])
			by, _ := XFloat(sg[1][1])
			segs = append(segs, oSeg{a: oPt{x: ax, y: ay}, b: oPt{x: bx, y: by}})
		}
		cases = append(cases, segs)
	}
	oracleRejects := 0
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		for _, segs := range cases {
			if _, err := OracleFaces(segs); err != nil {
				oracleRejects++
			}
		}
	}
	b.ReportMetric(float64(oracleRejects), "rejects/op")
}
