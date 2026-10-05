# Issue 0006 Benchmarks — Robust Predicates

Recorded 2026-08-23. Evidence only: no pass/fail threshold is claimed or
judged here (NUMERICAL-MODEL.md §11 performance-evidence chain; issue 0030
defines the Phase 0 criterion before judging).

## Environment

| Item | Value |
|---|---|
| Go | go1.27.0 (`go version`), GOTOOLCHAIN=local |
| OS | Linux 7.1.8-arch1-3 x86_64 |
| CPU | AMD Ryzen 9 7950X 16-Core Processor |
| Threads used by bench | 32 (GOMAXPROCS) |
| Date | 2026-08-23 |

## Command

```
go test ./internal/geometry/predicates/ -run '^$' -bench . -benchmem -count=1
```

Correctness and benchmarks are separate: `go test ./...` (without `-bench`)
runs the correctness suite; no test contains timing-dependent assertions.

## Corpora (deterministic, fixed seeds; construction in bench_test.go)

| Corpus | Size | Generator | Notes |
|---|---|---|---|
| easy | 256 | seed 1, uniform [0,1e6)^6 | filter decides ~100% |
| hard-filter-fail | 256 | seed 2, near-collinear lattice; every case VERIFIED filter-inconclusive at construction | fallback rate 1.0 by construction |
| classed-bits | 256 | seed 3, six named bit-pattern classes (moderate / far-small normals exp [-900,-60] / upper-range exp [25,28] / near-bound [B/2,B) / subnormals / signed zeros), all within \|coord\| <= B = 1e9 | stresses operand gate |

All corpora contain only admissible inputs (out-of-policy rejection is
tested separately). Benchmark results are assigned to package-level sinks
so no measured work is dead-code eliminated.

## Output (captured; insignificant trailing whitespace normalized for git diff --check)

```
goos: linux
goarch: amd64
pkg: domowik.dev/domowik/internal/geometry/predicates
cpu: AMD Ryzen 9 7950X 16-Core Processor
BenchmarkOrientationFiltered/easy-32      	186622909	         6.641 ns/op	         0 %inconclusive	       0 B/op	       0 allocs/op
BenchmarkOrientationFiltered/hard-filter-fail-32         	272277177	         4.416 ns/op	       100.0 %inconclusive	       0 B/op	       0 allocs/op
BenchmarkOrientationFiltered/classed-bits-32             	243393618	         4.935 ns/op	        90.23 %inconclusive	       0 B/op	       0 allocs/op
BenchmarkOrientationNaive/easy-32                        	901068016	         1.368 ns/op	       256.0 corpus-size	       0 B/op	       0 allocs/op
BenchmarkOrientationNaive/hard-filter-fail-32            	887493052	         1.344 ns/op	       256.0 corpus-size	         1.000 filter-fallback-rate	       0 B/op	       0 allocs/op
BenchmarkOrientationNaive/classed-bits-32                	367950973	         3.225 ns/op	       256.0 corpus-size	       0 B/op	       0 allocs/op
BenchmarkOrientationAdaptive/easy-32                     	61379038	        18.16 ns/op	       0 B/op	       0 allocs/op
BenchmarkOrientationAdaptive/hard-filter-fail-32         	 1768234	       662.8 ns/op	    2344 B/op	      28 allocs/op
BenchmarkOrientationAdaptive/classed-bits-32             	 1450081	       798.2 ns/op	    1971 B/op	      24 allocs/op
BenchmarkIntersectSegmentsNaive-32                       	494008266	         2.423 ns/op	       256.0 corpus-size	       0 B/op	       0 allocs/op
BenchmarkIntersectSegmentsRobust-32                      	   96253	     14027 ns/op	       256.0 corpus-size	   14765 B/op	     426 allocs/op
BenchmarkUnitDirection-32                                	19261251	        66.93 ns/op	       256.0 corpus-size	     160 B/op	       3 allocs/op
BenchmarkOffsetCertificateAndVertex-32                   	   55370	     22216 ns/op	   20912 B/op	     517 allocs/op
PASS
ok  	domowik.dev/domowik/internal/geometry/predicates	20.177s
```

## Interpretation

- **naive < filtered < adaptive**, as expected:
  - Naive orientation: ~1.3–1.4 ns/op (unguarded float formula).
  - Filter-only fast path (`filterSign`, internal measurement): ~4.5–6.7
    ns/op including the §4.1 operand/intermediate gates.
  - Adaptive full API (`Orientation`: validation + filter + integer exact
    fallback when inconclusive): ~18–19 ns/op on easy inputs (validation +
    filter dominate); ~0.66 µs/op (hard corpus) to ~0.80 µs/op
    (classed-bits) when cases escalate to the exact scaled-big.Int path.
- **classed-bits**: ~90% of cases are filter-inconclusive under the literal
  §4.1 operand gate (zeros/subnormals/small exponents escalate — see
  FINDINGS.md §3), so adaptive cost there (~0.80 µs/op) approaches the
  pure-exact cost. This quantifies the interpretation choice for 0030.
- **Segment intersection** robust classifier: ~14.0 µs/op, 426 allocs —
  dominated by big.Rat parameter extraction (exact C1 values are part of
  the contract). The naive float baseline (~2.5 ns/op) is unsound by
  construction and shown only as scale reference.
- **UnitDirection**: ~67 ns/op typical (gated fast path + Rat certificate
  copy); certificate is an outward rational bound, not a heuristic.
- **OffsetVertexCertificate+OffsetVertex**: ~22.2 µs/op, 517 allocs —
  dominated by exact rational term assembly. Suitable for construction-time
  certification, not per-frame hot loops; caching per direction/thickness is
  the obvious follow-up if 0030's Phase 0 criterion demands it.

Reproducibility caveat: absolute numbers are machine-specific;
relative structure (naive < filtered < adaptive; escalation-dominated worst
cases) is the durable evidence. Re-run on any platform with the command
above; divergent DISCRETE outcomes across platforms would be semantic
findings, not benchmark noise (NUMERICAL-MODEL.md §8).
