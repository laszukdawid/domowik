# Trigger-3 Disposition — NUMERICAL-MODEL §11 (Phase 0 performance failure)

Status: **NON-RATIFYING continuation record.** This document records the
NUMERICAL-MODEL.md §11 revision-trigger 3 assessment that
[RATIFICATION.md](RATIFICATION.md) section 8 and the v3 rerun record leave
open. It changes no threshold, adds no protocol rule, ratifies nothing,
promotes nothing in the numeric domain, and never re-judges the recorded
Phase 0 criterion verdict.

## 1. Question and scope

The registered Phase 0 performance criterion FAILED for the recorded v3
scope on the A3 robust threshold (≤ 10000 ns/op median):

- `BenchmarkIntersectSegmentsRobust` — recorded median **12779.5 ns/op**
- `BenchmarkOffsetCertificateAndVertex` — recorded median **20949.0 ns/op**

([PHASE0-PERFORMANCE-RERUN.md](PHASE0-PERFORMANCE-RERUN.md); these recorded
values are cited as facts and are not re-measured into judgment here.)

Per NUMERICAL-MODEL.md §11, revision trigger 3 fires only when **no
caching/locality strategy per R5.1.9 can plausibly close the gap**. This
document assesses exactly that question for the two breached benchmarks and
records exactly one outcome (§8). The assessment is restricted to
caching/locality strategies; other kinds of engineering headroom are noted
in §7.4 as context only and cannot avert or waive the trigger assessment,
because the registered failure-handling rule admits no such waiver.

## 2. Recorded facts relied upon (citations only)

- Criterion: [PHASE0-PERFORMANCE-CRITERION.md](PHASE0-PERFORMANCE-CRITERION.md)
  (blob `7f4ac272174abf6ef04147dda63794b40be527ae`), including its
  failure-handling rule: outcomes are limited to revising the candidate
  model and rerunning under a new pre-registered criterion, or honestly
  revising ADR 0002's gate status/custom-burden assessment.
- Verdict: [PHASE0-PERFORMANCE-RERUN.md](PHASE0-PERFORMANCE-RERUN.md) —
  FAILED for recorded scope on the two A3 robust medians above; all other
  applicable thresholds met as recorded (A1 max case median ≈ 2.57 ms vs
  250 ms bound; A2 max per-case median 2.5672 ms vs 1 s conservative tail;
  A4 max 13160 KiB vs 524288 KiB). These passing results are cited as
  recorded context only; they do not re-weight or reinterpret the A3
  failures.
- Ratification state: [RATIFICATION.md](RATIFICATION.md) §8 — PARTIAL /
  NOT COMPLETE; numerical model NOT RATIFIED; ADR 0002 remains Proposed;
  H-6 MISSING domains persist; all gates listed there remain open and are
  untouched by this record.

## 3. Diagnostic environment disclosure

All measurements in §5–§6 are **DIAGNOSTIC ONLY**. They explain where time
goes; they are not reruns of the registered protocol, produce no judged
value, and neither confirm nor weaken the recorded v3 verdict.

| Item | Value |
|---|---|
| Host CPU | AMD Ryzen 9 7950X 16-Core Processor, 32 logical CPUs |
| Frequency scaling context | observed `CPU(s) scaling MHz: 78%` via lscpu at profiling time |
| Memory | 61 GiB total |
| OS / kernel | Arch Linux, kernel 7.1.8-arch1-3 |
| Toolchain | go1.27.0 linux/amd64 (`GOTOOLCHAIN=local`) |
| Shell pins | `GOMAXPROCS=32 CGO_ENABLED=1 GOCACHE=/tmp/domowik-0030-go-cache` |
| Date | 2026-08-24 (pprof sample headers 14:24 PDT) |
| Harness | committed evidence-only decomposition file `internal/geometry/predicates/trigger3_evidenceonly_test.go` plus stock `go test -bench -cpuprofile/-memprofile` over the existing registered-selector benchmarks |

Environment differences from the v3 reference environment are irrelevant to
the verdict because no diagnostic number below is compared against any
threshold; they matter only for reproducibility of the attribution
observations.

## 4. Method

1. Diagnostic reproduction: five independent `go test -bench` processes per
   benchmark (`-benchtime=1s -count=1 -cpu=32 -benchmem`, exact
   registered-name selectors), outputs retained under the gitignored
   `build/0030-trigger3-profiling/` (manifest in §9).
2. CPU and allocation profiling (`-cpuprofile`, `-memprofile`,
   `-benchtime=2s`) analyzed with `go tool pprof -top` (flat and cumulative).
3. Stage decomposition via the evidence-only harness: isolated sub-steps of
   each breached op, five processes each, same sink/ReportAllocs discipline
   as the committed benchmarks.
4. Corpus-path check (`TestTrigger3SegmentCorpusClasses`): the shared
   segment corpus is 190 ClassDisjoint + 66 ClassCrossingInterior + 0
   collinear — every case takes the non-parallel path, so ISR attribution
   covers essentially the whole op.
5. Plausibility analysis separating (i) the registered judged statistic —
   per-operation median over cold, rotating-corpus inputs — from (ii) what
   R5.1.9 caching/locality can actually change — which recomputations run at
   document/face scope after an edit.

## 5. Diagnostic reproduction (not a judgment)

Median of five independent process runs:

| Benchmark | Diagnostic median ns/op | Range | allocs/op | B/op |
|---|---|---|---|---|
| BenchmarkIntersectSegmentsRobust | 13131 | 13007–13722 | 426 | 14765 |
| BenchmarkOffsetCertificateAndVertex | 20752 | 20411–21801 | 538 | 21416 |

Consistent with the recorded v3 medians (12779.5 / 20949.0) on this host;
recorded solely to show the profiled binary behaves like the measured one.

Required reduction to reach the 10000 ns/op bound under the registered
judged statistic:

- ISR: from recorded 12779.5 → ≥ **21.75%** cut ((12779.5−10000)/12779.5);
  from diagnostic 13131 → ≥ 23.85% cut.
- OCV: from recorded 20949.0 → ≥ **52.26%** cut; from diagnostic 20752 →
  ≥ 51.81% cut.

## 6. Where the time goes (DIAGNOSTIC attribution)

### 6.1 CPU profiles (cumulative % of samples)

| Attributed function | ISR | OCV |
|---|---|---|
| `math/big.(*Rat).norm` (rational normalization) | 65.3% | 59.4% |
| `math/big.(*Int).lehmerGCD` (inside normalization) | 53.7% | 50.5% |
| `predicates.intersectNonParallel` | 58.5% | — |
| `predicates.crossOfDiffs` (exact determinant) | 26.4% | — |
| `predicates.floatToRat` / `dyadic.rat` | 22.5% | 13.0% (floatToRat) |
| `predicates.OffsetVertexCertificate` | — | 68.3% |
| `predicates.validateDirection` (all callers) | — | 49.2% |
| `predicates.commitOffsetVertex` | — | 32.3% |
| `predicates.OffsetVertex` | — | 16.2% |
| `runtime.mallocgc` (+makeslice/growslice) | ~20% | ~20% |

### 6.2 Allocation profiles

- ISR (alloc_objects): `math/big.nat.make` flat 71.7%; `dyadic.rat` cum
  26.5%; `floatToRat` cum 34.8%; `intersectNonParallel` cum 65.8%.
- OCV (alloc_space): `math/big.nat.make` flat 83.8%;
  `OffsetVertexCertificate` cum 79.5%; `commitOffsetVertex` cum 38.5%.

### 6.3 Evidence-only stage decomposition (medians of five runs)

| Isolated stage | Median ns/op | allocs/op |
|---|---|---|
| ISR: four-point input validation | 17.8 | 0 |
| ISR: exact parallelism determinant alone | 4376 | 146 |
| ISR: non-parallel resolution alone (≈ full op) | 13286 | 426* |
| OCV: UnitDirection (gated fast path) | 74 | 3 |
| OCV: OffsetVertexCertificate (incl. UnitDirection) | 17621 | 423 |
| OCV: validateDirection alone | 3524 | 115 |
| OCV: commitOffsetVertex alone (incl. validation) | 3650 | 115 |
| OCV: OffsetVertex (incl. UnitDirection) | 4012 | 118 |

\* reported by the full-op benchmark; the decomposition reproduces it.

Reading:

- ISR cost IS the non-parallel exact-rational resolution: fresh rational
  construction (`floatToRat`), products/quotients, and above all GCD-based
  normalization of freshly created rationals. Validation is negligible
  (~18 ns). There is no repeated computation inside one cold decision.
- OCV decomposes as certificate construction ≈ 17.6 µs (~85%) plus vertex
  commitment ≈ 4.0 µs. Within that, `validateDirection` runs **three times
  per benchmark iteration** (once directly in `OffsetVertexCertificate`,
  once inside its internal F4 `commitOffsetVertex` re-check, once inside
  `OffsetVertex`'s `commitOffsetVertex`), at ≈ 3.5 µs per call:
  3 × 3524 ns ≈ 10.6 µs ≈ **51%** of the diagnostic op. The remaining
  ≈ 10 µs is the six-term certificate arithmetic itself (exact rationals,
  half-ulp bounds, outward square roots) — intrinsic exact-arithmetic work.
- `UnitDirection` itself is cheap (74 ns diagnostic; recorded within bound)
  and is NOT part of either breach.

## 7. Plausibility analysis: can R5.1.9 caching/locality close the gap?

### 7.1 Statistic versus locality — the decisive separation

The judged A3 statistic is the **median ns/op of a single cold operation**
over a rotating corpus of distinct inputs. Caching changes *which*
operations run again; R5.1.9 locality governs *which faces recompute* after
an edit at document/face scope. Both mechanisms act on **invocation
counts**, not on the cost of an operation that does run on fresh inputs.
A per-operation median over cold inputs is therefore structurally beyond
what caching/locality can reduce: no cache hit occurs inside a single cold
op, and reducing how often the pipeline invokes predicates elsewhere cannot
change the measured per-op value without changing what is measured — which
the registered protocol forbids post hoc.

### 7.2 Per-benchmark arithmetic

ISR (needs ≥ 21.75–23.85% cut): the profile shows >50% of samples inside
GCD normalization of freshly built rationals and ~20% in allocation of new
big.Int backing stores, with zero intra-op repeated work (validation ≈
18 ns; the rest is one pass of fresh exact-rational arithmetic). There is
no redundancy for a cache to eliminate and no locality structure inside one
decision for R5.1.9 to exploit. A credible mechanism by which caching/locality
removes a quarter of this op's cost does not exist.

OCV (needs ≥ 51.81–52.26% cut): take the most favorable caching hypothesis
available — memoize direction validity once at construction so the three
per-op structural validations collapse to zero additional runtime calls.
That removes at most ≈ 10.6 µs:

- removing the two redundant re-validations (keeping first validation):
  20752 − ~7050 ≈ 13.7 µs → still **~37% above the bound**;
- removing ALL three validations (implausible anyway: fail-closed boundary
  contract checks): 20752 − 10572 ≈ **10.18 µs — still above 10000**, with
  the ≈ 10 µs intrinsic certificate rational arithmetic untouched.

Even the maximally favorable hypothesis cannot close OCV's gap, let alone
with margin; and R5.1.9's actual subject matter (face-recomputation scope)
is further from this metric still.

### 7.3 What R5.1.9 locality actually offers

R5.1.9 bounds WHICH faces recompute after edits (transitive containment/
overlap closure instead of document-wide work). That is a real Phase 1+
architecture lever for end-to-end latency, but it operates above the
predicate layer entirely and maps to no mechanism that reduces either
breached per-op median. It cannot close this gap and could not waive the
failure even if pipeline-level latency felt acceptable, because the
registered criterion judges the recorded per-op values as recorded.

### 7.4 Context outside the trigger-3 lane (no waiver implied)

The profile suggests ordinary constant-factor engineering headroom exists
(scaled-integer paths analogous to `orientExactScaled`, deduplicated
validation inside certified-pair construction, fewer intermediate
rational normalizations). Such work belongs to any future REVISED candidate
model and would require new pre-registered criteria and full protocol-compliant
reruns before any judgment. It is NOT a caching/locality strategy per
R5.1.9, so it does not avert trigger 3; it is recorded here only so the
owner's reassessment decision has an honest picture of the option space.

## 8. Outcome (exactly one)

**(b) NOT PLAUSIBLY CLOSABLE.** No caching/locality strategy per R5.1.9 can
plausibly close the recorded A3 robust gap for either breached benchmark:
ISR offers no cacheable redundancy in a cold single-op statistic, and OCV
remains above the bound even under a maximal caching hypothesis that
eliminates all structural re-validation. Revision trigger 3 therefore FIRES
as a matter of this assessment.

Consequence, per the registered failure-handling rule: the ADR-0002
revision-trigger assessment is recorded as a **RECOMMENDATION ONLY** in
[docs/decisions/0002-go-core-cli-first.md](../../decisions/0002-go-core-cli-first.md).
Any stack reassessment (core language or numerical-kernel split behind the
existing interface) is an OWNER decision; this record decides nothing about
stacks. Alternatively the owner may direct a revised candidate model with a
new pre-registered criterion and protocol-compliant reruns; both paths stay
open until the owner chooses.

## 9. Artifact manifest (diagnostic, gitignored `build/0030-trigger3-profiling/`)

```
# sha256sum build/0030-trigger3-profiling/*
6d046b2e3357beac15e01f0f8204bacd3e750e1aef5a90a565890326c61f58d0  isr.cpu
a3f2c92c57374734e67da1ab3cc0d50baa559cc0570c4cf8f982110308ff3053  isr.mem
08b85b645085082f41fec50cccfe5d898db0868aa05d84f2c021205496defcbc  ocv.cpu
c52c25a5d18a44118ebb4457715d779c490774935ec537866006b3de2f837ba6  ocv.mem
f5e2d11ad0b750d4d898806bf4d655279cadb5d88f508818b42253ed8dc0e7b6  isr-profile.out
159dee581239057c175a9fd83c21db0342b45c7e3843f164382b723674b79215  ocv-profile.out
8cd7c7ebc5489859e8b5d61324f5e57ea16524aa966ef2566528020d7dcc36fa  diag-run1.out
1fac879ad8757836412a7a4040c4ade8c47650a4907a73b71d2e6cbb4569290d  diag-run2.out
4024763bb9fe230d9d42e19104ec07d0edfa2db7163d9d0179bd5cd6f549beaa  diag-run3.out
445b5386a55fc4d6119d2e7b27cca1ac605ff5a8686e17021c7f74eb45632386  diag-run4.out
aea607caf703673b30ceb43ffe09a9ccfc20278f5d299f7e3105fcb96129f444  diag-run5.out
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  diag-run1.err
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  diag-run2.err
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  diag-run3.err
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  diag-run4.err
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  diag-run5.err
1526bf73c90aeb8f30eab686a85f041b47ab87a6b7b787ea8de35ccebe590631  decomp-run1.out
ad1186bf4a2588c3d6b0728bed770a168ce99624afb7e92bbc45ffe6fc161611  decomp-run2.out
48f9ef8104649bc664e01ea516f099f1e257db089d1a4a0c3b3e7bfb26eb4408  decomp-run3.out
14b428dfd4db04ff5a4ae5dda7f6616c1618d06a04ecbc60e9b17681526e3e66  decomp-run4.out
3b9745ee3be82a88a47d27dda5af7dec5bad6f1c974d17559a6068321b7ab98a  decomp-run5.out
104324e07b9264d71515c455faa363bbbaa2c56688bb2a66293556cfbd0cc1c6  SHA256SUMS.txt
```

Reproduction commands (diagnostic):

```sh
export GOTOOLCHAIN=local GOMAXPROCS=32 CGO_ENABLED=1 GOCACHE=/tmp/domowik-0030-go-cache
go test ./internal/geometry/predicates/ -run '^$' \
  -bench 'BenchmarkIntersectSegmentsRobust|BenchmarkOffsetCertificateAndVertex' \
  -benchmem -benchtime=1s -count=1 -cpu=32
go test ./internal/geometry/predicates/ -run '^$' -bench 'Trigger3' \
  -benchmem -benchtime=1s -count=1 -cpu=32
go test ./internal/geometry/predicates/ -run 'TestTrigger3SegmentCorpusClasses' -v
```

Harness provenance (CLEAN-ROOM-POLICY): the decomposition file is adapted
solely from this repository's committed `internal/geometry/predicates/
bench_test.go` patterns (corpora `benchSegs`/`benchDirs`, sink variables,
ReportAllocs discipline) exercising unchanged production functions. No
external source was consulted.

## 10. Scope guard

This record does NOT: modify any registered threshold or protocol rule;
promote or ratify NUMERICAL-MODEL or any §6 item; edit BEHAVIOR-SPEC;
implement production geometry APIs; claim any H-register item closed (H-0,
H-1, H-2, H-5, H-6 and the H-3 partial state all remain open as recorded);
edit `issues/` or `EXECUTIVE-DECISIONS.md`; or decide any stack question.
The numerical model remains NOT RATIFIED, ADR 0002 remains Proposed, gate 1
remains NOT PASSED/OPEN, PARTIAL / NOT COMPLETE stays in force, and the
ratification/promotion gate remains open pending the owner's decision on
this recommendation.
