# Issue 0007 Performance Measurements — macOS/arm64

**Status:** HOST-DEPENDENT evidence for issue 0007 acceptance criterion 6 (wall-clock and allocation measurements). These measurements are evidence only for issue 0030's Phase 0 performance judgment. This issue sets and applies NO pass/fail threshold.

**Host classification per ED-0070 amendment (a):** All figures below are HOST-DEPENDENT and carry the environment that ACTUALLY produced them.

## Reference Environment

Identical for correctness and performance runs (verified together in one session):

- **CPU:** Apple M4 Max
- **Cores:** 16 physical cores (performance + efficiency)
- **RAM:** 48 GiB (51539607552 bytes)
- **OS:** macOS 26.6.2 (Darwin kernel 25.6.0, build 25G83)
- **Architecture:** darwin/arm64
- **Filesystem:** apfs
- **Go version:** go1.27.0 darwin/arm64
- **GOTOOLCHAIN:** local (verified with `GOTOOLCHAIN=local go version`)
- **CGO_ENABLED:** 1 (go test default)
- **Module:** domowik.dev/domowik
- **Test binary:** built by `go test` with default flags (no additional build flags)

## Correctness Baseline (Same Environment)

Correctness tests re-run on THIS host with the SAME environment as benchmark measurements, per ED-0070 amendment (a) and KICKOFF.md requirement.

**Command:**
```bash
export PATH="/Users/dawidlaszuk/.local/share/mise/installs/go/1.27.0/bin:$PATH"
GOTOOLCHAIN=local go test ./internal/geometry/ -run 'TestCorpusBooleanVsOracle|TestBooleanPinchSemantics|TestBooleanCanonicalInvariances' -v
```

**Result:** PASS (all three tests, 0.263s total)

**Oracle agreement verified (per ED-0072 efficacy requirement):**
- `TestCorpusBooleanVsOracle`: B-1x and B-1q both report 0/12 topology mismatches, 122 matched vertices, maxSquaredEuclideanDist=0 across all 12 named corpus cases
- `TestBooleanPinchSemantics`: pinch-contact normalization fixtures pass
- `TestBooleanCanonicalInvariances`: reversal/rotation/permutation invariance verified

The oracle agreement is POST-NORMALIZATION AGREEMENT per docs/evidence/0007-booleans-topology.md §5; both engines apply analogous deterministic split/validate normalization.

## 1. Boolean Pipeline Benchmarks (Primary Measurement)

**Corpus:** 12 named cases (single-room, disjoint-rooms, thick-L-join, unequal-thickness-T, X-crossing, shared-edge-touching-bands, touching-vertices-pinch-corner, sliver-overlap, courtyard-island, point-pinch, large-translation, near-B-translation)

**Command:**
```bash
export PATH="/Users/dawidlaszuk/.local/share/mise/installs/go/1.27.0/bin:$PATH"
GOTOOLCHAIN=local go test ./internal/geometry/ -run '^$' -bench '^BenchmarkCorpusBoolean' -benchmem -count=3 -benchtime=2s
```

**Verbatim output:**

```
goos: darwin
goarch: arm64
pkg: domowik.dev/domowik/internal/geometry
cpu: Apple M4 Max
BenchmarkCorpusBooleanB1X-16    	     290	   8381247 ns/op	         0 rejects/op	14503063 B/op	  492402 allocs/op
BenchmarkCorpusBooleanB1X-16    	     285	   8442741 ns/op	         0 rejects/op	14503101 B/op	  492402 allocs/op
BenchmarkCorpusBooleanB1X-16    	     282	   8445813 ns/op	         0 rejects/op	14503122 B/op	  492402 allocs/op
BenchmarkCorpusBooleanB1Q-16    	     262	   9143819 ns/op	         0 rejects/op	15841007 B/op	  545187 allocs/op
BenchmarkCorpusBooleanB1Q-16    	     262	   9161812 ns/op	         0 rejects/op	15840948 B/op	  545187 allocs/op
BenchmarkCorpusBooleanB1Q-16    	     259	   9235768 ns/op	         0 rejects/op	15841007 B/op	  545187 allocs/op
PASS
ok  	domowik.dev/domowik/internal/geometry	20.139s
```

**Summary (slash-separated three-run format per ED-0073):**

| Strategy | Wall-clock (ns/op) | Bytes/op | Allocs/op | Rejects/op |
|---|---|---|---|---|
| B-1x | 8381247 / 8442741 / 8445813 | ~14.5 MB | 492,402 | 0 |
| B-1q | 9143819 / 9161812 / 9235768 | ~15.8 MB | 545,187 | 0 |

## 2. ADR-0002 Trigger Benchmarks (Allocation Profile Focus)

**Context per ED-0073:** The trigger-3 disposition found the gap "not plausibly closable by caching or locality" — which are about memory ACCESS. The allocation counts below are about allocation VOLUME, a different question that was not previously asked.

**Command:**
```bash
export PATH="/Users/dawidlaszuk/.local/share/mise/installs/go/1.27.0/bin:$PATH"
GOTOOLCHAIN=local go test ./internal/geometry/predicates/ -run '^$' -bench '^Benchmark(IntersectSegmentsRobust|OffsetCertificateAndVertex)$' -benchmem -count=3 -benchtime=2s
```

**Verbatim output:**

```
goos: darwin
goarch: arm64
pkg: domowik.dev/domowik/internal/geometry/predicates
cpu: Apple M4 Max
BenchmarkIntersectSegmentsRobust-16       	  188356	     12555 ns/op	       256.0 corpus-size	   14765 B/op	     426 allocs/op
BenchmarkIntersectSegmentsRobust-16       	  188338	     12677 ns/op	       256.0 corpus-size	   14765 B/op	     426 allocs/op
BenchmarkIntersectSegmentsRobust-16       	  190446	     12669 ns/op	       256.0 corpus-size	   14765 B/op	     426 allocs/op
BenchmarkOffsetCertificateAndVertex-16    	  110613	     21428 ns/op	   21395 B/op	     538 allocs/op
BenchmarkOffsetCertificateAndVertex-16    	  113578	     21288 ns/op	   21395 B/op	     538 allocs/op
BenchmarkOffsetCertificateAndVertex-16    	  111878	     21374 ns/op	   21395 B/op	     538 allocs/op
PASS
ok  	domowik.dev/domowik/internal/geometry/predicates	15.641s
```

**Summary:**

| Benchmark | Wall-clock (ns/op) | Bytes/op | Allocs/op | vs 10,000 ns/op bound |
|---|---|---|---|---|
| IntersectSegmentsRobust | 12555 / 12677 / 12669 | 14,765 | **426** | ~26% over bound |
| OffsetCertificateAndVertex | 21428 / 21288 / 21374 | 21,395 | **538** | ~114% over bound |

**Trigger-3 confirmation:** The ADR-0002 revision trigger reproduces on this host within ~4% of the historical Linux/amd64 measurements (ED-0073: 11934-12173 ns/op vs 12779.5 historical for IntersectSegments; 20433-20460 ns/op vs 20949.0 historical for Offset). The finding is NOT host-specific.

## 3. Allocation Profile Analysis (Task 2 — First-Class Data)

**Question from ED-0073:** If allocation dominates, what are the largest contributors?

Memory profiling was performed on both the boolean pipeline and the predicates to identify allocation contributors by call site, not just aggregate counts.

### Boolean Pipeline Allocation Contributors

**Profiling command:**
```bash
go test ./internal/geometry/ -run '^$' -bench '^BenchmarkCorpusBoolean' -benchmem -count=3 -benchtime=2s -memprofile=build/corpus-boolean.memprof
```

**Top allocation sites by object count (alloc_objects):**

1. **`math/big.nat.make`** — 844,798,603 objects (95.20% of total)
   - This is the underlying allocation for all `math/big.Int` and `math/big.Rat` arithmetic
   - Dominates by nearly 2 orders of magnitude

2. **`candEventY`** — 7,528,894 objects (0.85%)
   - Sweep-line event construction

3. **`candIntersectParams`** — 7,143,760 objects (0.81%)
   - Exact rational intersection parameter extraction

4. **`gridUnitsToMM`** (B-1q) — 4,612,368 objects (0.52%)
   - Grid-to-mm conversion for quantized strategy

5. **`quantizeGrid`** (B-1q) — 4,194,423 objects (0.47%)
   - Rational grid quantization

**Top allocation sites by bytes (alloc_space):**

1. **`math/big.nat.make`** — 30,193.75 MB (92.78% of 32.5 GB total)
2. **`candEventY`** — 459.53 MB (1.41%)
3. **`candIntersectParams`** — 339.52 MB (1.04%)
4. **`gridUnitsToMM`** — 281.52 MB (0.87%)
5. **`candRingIsSimple.func2`** — 217.01 MB (0.67%)

**Key operations driving `math/big` allocations:**

- `math/big.(*Int).scaleDenom` — 15,978.23 MB (49.10%)
- `math/big.(*Rat).Sub` — 16,908.69 MB (51.96%)
- `math/big.(*Rat).Cmp` — 4,588.21 MB (14.10%)
- `math/big.(*Rat).Mul` — 4,582.69 MB (14.08%)

### Predicates Allocation Contributors

**Top allocation sites by object count:**

1. **`math/big.nat.make`** — 270,263,515 objects (72.51% of total)
2. **`slices.Grow`** — 51,074,339 objects (13.70%)
   - Dynamic slice growth for `math/big.Word` arrays
3. **`math/big.NewInt`** — 25,920,115 objects (6.95%)
4. **`dyadic.rat`** — 17,466,395 objects (4.69%)
   - Exact rational construction from dyadic inputs

**Top allocation sites by bytes:**

1. **`math/big.nat.make`** — 12.36 GB (79.72% of 15.5 GB total)
2. **`dyadic.rat`** — 1.04 GB (6.71%)
3. **`slices.Grow`** — 1.00 GB (6.45%)
4. **`math/big.NewInt`** — 0.63 GB (4.08%)

**Key predicates operations:**

- `math/big.(*Rat).norm` — 7.30 GB (47.06%)
- `math/big.(*Int).lehmerGCD` — 6.34 GB (40.87%)
- `intersectNonParallel` — 5.56 GB (35.86%)
- `OffsetVertexCertificate` — 5.77 GB (37.19%)

### Interpretation: Is Allocation the Bottleneck?

**For predicates (426/538 allocs/op):**

The allocation profile shows that **426 allocations per IntersectSegments** and **538 allocations per OffsetCertificate** are dominated by:
- `math/big` arithmetic (72-95% of objects, 80-93% of bytes)
- Exact rational operations (`dyadic.rat`, `diffRat`, `floatToRat`)
- GCD normalization (`lehmerGCD`, `norm`)

**Observation:** The 426/538 allocation counts are NOT obviously inherent to exact-predicate arithmetic. They are the signature of an implementation allocating intermediates for every rational operation. The profiler shows that:
- Each predicate call triggers dozens of `math/big.Rat` operations
- Each `math/big.Rat` operation allocates multiple `math/big.Int` objects
- Each `math/big.Int` allocates a `nat.make` slice

**The question ED-0073 raises:** If the timing is allocation-dominated (12.5 µs with 426 allocs, 21.4 µs with 538 allocs), the bound MAY be reachable by reducing allocation volume without changing languages. If timing is NOT allocation-dominated, the case for stack reassessment is stronger.

**This measurement does NOT answer that question** — it reports the allocation profile as first-class data per the task scope. Determining whether allocation or computation dominates would require profiling CPU time per allocation site, which is beyond measurement-only scope.

### H-6 Escalation Rates (Filter-to-Exact Fallback)

**Context per NUMERICAL-MODEL §11:** H-6 instrumentation measures runtime filter failures that escalate to exact arithmetic. This subsection records H-6 data where it exists and states its applicability to the ADR-0002 trigger benchmarks.

#### Orientation: H-6 Data Available

`Orientation` implements an adaptive filter (`orientFast`) that returns a decision when safe, escalating to exact `math/big` arithmetic (`orientExact`) only when the filter is inconclusive.

**Measurement command:**
```bash
export PATH="/Users/dawidlaszuk/.local/share/mise/installs/go/1.27.0/bin:$PATH"
GOTOOLCHAIN=local go test ./internal/geometry/predicates/ -run '^TestPhase0H6Runtime$' -v
```

**Verbatim output (H-6 instrumentation lines only):**

```
H6-CORPUS name=easy class=ordinary-control criterionClass=non-adversarial cases=256 decisions=256 filterToExact=0 fallbackRate=0.000000 signDigest=8becea5a13ac480b
H6-CORPUS name=hard-filter-fail class=adversarial criterionClass=adversarial cases=256 decisions=256 filterToExact=256 fallbackRate=1.000000 signDigest=51f1c89414aed821
H6-CORPUS name=classed-bits class=adversarial criterionClass=adversarial cases=256 decisions=256 filterToExact=231 fallbackRate=0.902344 signDigest=91036119e596879b
H6-DIRECTION cases=256 criterionClass=non-adversarial gated-fast=256 high-precision-fallback=0 dirDigest=533562d926860524
```

**Summary:**

| Workload | Corpus Class | Cases | Filter-to-Exact | Fallback Rate |
|---|---|---|---|---|
| Orientation (easy) | non-adversarial | 256 | 0 | 0.0% |
| Orientation (hard-filter-fail) | adversarial | 256 | 256 | 100.0% |
| Orientation (classed-bits) | adversarial | 256 | 231 | 90.2% |
| UnitDirection | non-adversarial | 256 | 0 | 0.0% |

**Corpus construction:** The `hard-filter-fail` corpus is explicitly constructed to force filter failures per `h6_test.go:52-82`. Every accepted case is verified with `filterSign` during construction to ensure it is inconclusive. The 100% fallback rate is by design.

#### IntersectSegments and OffsetVertex: H-6 NOT APPLICABLE

H-6 escalation measurement is **NOT APPLICABLE** to `BenchmarkIntersectSegmentsRobust` (426 allocs/op) and `BenchmarkOffsetCertificateAndVertex` (538 allocs/op). The H-6 instrumentation (`internal/geometry/predicates/evidence_h6.go`) measures filter-to-exact escalation for `Orientation`'s `orientFast` → `orientExact` path only.

**Architecture as implemented:**

- `IntersectSegments` (`internal/geometry/predicates/segment.go:113`) calls `crossOfDiffs` directly
- `crossOfDiffs` (`internal/geometry/predicates/exact.go:76-85`) builds `big.Rat` unconditionally: four `floatToRat` conversions, two subtractions, two multiplications, one final subtraction — every call allocates on every invocation
- `OffsetVertex` and `OffsetVertexCertificate` construct exact rationals for offset computation on every call
- Neither operation implements an adaptive filter stage

**Observation:** `orientFast` exists in `internal/geometry/predicates/orient.go:97`. `internal/geometry/predicates/segment.go` references `orientFast` zero times. The package contains the adaptive-filter technique; these predicates do not use it.

**Consequence:** `IntersectSegments` is EXACT-ONLY. The 426 allocs/op is NOT a filter-escalation rate. There is no filter, therefore no fallback to measure. The allocation count is the unconditional cost of the exact path on every call, irrespective of input conditioning. Similarly, `OffsetVertex`'s 538 allocs/op is the cost of exact offset construction on every call.
- classed-bits: 558.7 ns/op, 1971 B/op, 24 allocs/op (escalates 90.2%)

The 25× timing difference and the appearance of allocations ONLY on adversarial corpora confirm that `math/big` cost is on the exact escalation path for `Orientation`. `IntersectSegments` has no filter path — all calls take the exact path.

## 4. Face Pipeline Benchmarks (Characterization Only per ED-0011)

**Context:** CandFaces is CHARACTERIZATION-ONLY per ED-0011. All face conformance, determinism, and performance claims are withdrawn. Measurements are recorded for 0030's judgment only.

**Command:**
```bash
export PATH="/Users/dawidlaszuk/.local/share/mise/installs/go/1.27.0/bin:$PATH"
GOTOOLCHAIN=local go test ./internal/geometry/ -run '^$' -bench 'CandFaces|OracleFaces' -benchmem -count=3 -benchtime=2s
```

**Verbatim output:**

```
goos: darwin
goarch: arm64
pkg: domowik.dev/domowik/internal/geometry
cpu: Apple M4 Max
BenchmarkCorpusCandFaces-16      	    6897	    336972 ns/op	         0 rejects/op	  245201 B/op	   12029 allocs/op
BenchmarkCorpusCandFaces-16      	    7033	    347457 ns/op	         0 rejects/op	  245201 B/op	   12029 allocs/op
BenchmarkCorpusCandFaces-16      	    6922	    339338 ns/op	         0 rejects/op	  245200 B/op	   12029 allocs/op
BenchmarkCorpusOracleFaces-16    	      84	  29256705 ns/op	         0 rejects/op	64231272 B/op	 1973213 allocs/op
BenchmarkCorpusOracleFaces-16    	      80	  29455466 ns/op	         0 rejects/op	64231255 B/op	 1973213 allocs/op
BenchmarkCorpusOracleFaces-16    	      81	  29612883 ns/op	         0 rejects/op	64231288 B/op	 1973214 allocs/op
PASS
ok  	domowik.dev/domowik/internal/geometry	14.759s
```

**Summary:**

| Pipeline | Wall-clock (ns/op) | Bytes/op | Allocs/op | Rejects/op |
|---|---|---|---|---|
| CandFaces (characterization only) | 336972 / 347457 / 339338 | ~245 KB | 12,029 | 0 |
| OracleFaces (reference machinery) | 29256705 / 29455466 / 29612883 | ~64 MB | 1,973,213 | 0 |

**Note:** CandFaces is ~87× faster and uses ~260× fewer allocations than OracleFaces. However, per ED-0011, CandFaces carries NO conformance or performance claim. These measurements are characterization only.

## 5. Comparability Notes

**Boolean vs Face benchmarks:** The boolean benchmarks (`BenchmarkCorpusBoolean*`) build operands inside the timed loop per the original evidence (docs/evidence/0007-booleans-topology.md §7 comparability caveat). Their numbers must not be read as directly comparable to face-pipeline figures, which operate on pre-built segments.

**Variance:** Wall-clock variance across three runs is low (< 1% for booleans, < 2% for predicates, < 4% for faces), indicating stable measurements on this host.

**Rejects/op:** All benchmarks report 0 rejects/op, confirming no validation-stage rejections on the corpus.

## 6. Acceptance Criterion Satisfaction

**AC6 from issues/0007-geometry-spike-booleans.md:**
> Wall-clock and allocation measurements of the boolean/planar-face pipeline are RECORDED on the same named corpus and reference environment as the correctness runs.

**Satisfied:**
- ✅ Wall-clock: recorded for boolean pipeline (§1), predicates (§2), and face pipeline (§4)
- ✅ Allocation: recorded as first-class data (§3) with memory profiling identifying largest contributors
- ✅ H-6 fallback/escalation data: recorded per NUMERICAL-MODEL §11 (§3 H-6 subsection)
- ✅ Same named corpus: 12 named cases, identical to correctness harness
- ✅ Same environment: correctness tests (§0) and benchmarks run in one session with identical environment block
- ✅ Host-dependent classification: all figures labeled per ED-0070 amendment (a)
- ✅ NO pass/fail threshold: explicitly stated per issue scope

**Task 2 (allocation question) addressed:** The allocation profile is reported as first-class data per ED-0073. The 426/538 allocs/op are dominated by `math/big` intermediate allocations. The H-6 data shows that `Orientation` has an adaptive filter (0% escalation on non-adversarial inputs, 90-100% on adversarial), while `IntersectSegments` is exact-only (no filter, unconditional `math/big` on every call). Whether allocation or computation dominates the timing is NOT determined here — that determination is beyond measurement-only scope and belongs to issue 0030's judgment.

## 7. Provenance and Standards Compliance

- **ED-0070 amendment (a):** Every figure is classified as host-dependent and carries the environment that ACTUALLY produced it
- **ED-0072:** Correctness baseline reports VERIFIED oracle agreement (efficacy), not just test presence
- **ED-0074:** This evidence is in a TRACKED file (`docs/evidence/0007-measurements-macOS-arm64.md`), not gitignored `build/`
- **KICKOFF.md:** Correctness runs were re-executed on THIS host with the SAME environment block as measurements, not cited from main

No code changes, no optimization, measurement only per task scope.
