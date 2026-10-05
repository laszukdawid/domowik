# Phase 0 Performance Rerun Record — v3

Status: **NON-RATIFYING continuation record.** This document records a
protocol-compliant rerun of the Phase 0 performance workloads under the
pre-registered criterion
[PHASE0-PERFORMANCE-CRITERION.md](PHASE0-PERFORMANCE-CRITERION.md)
(current blob `7f4ac272174abf6ef04147dda63794b40be527ae`, thresholds and
protocol rules untouched). It changes no threshold, adds no protocol rule,
and ratifies nothing.

## Verdict for the recorded scope

Under the registered pass/fail rule, the Phase 0 performance criterion is
**FAILED for the recorded scope of this rerun**: the A3 robust threshold
(≤ 10000 ns/op median) is exceeded by two named benchmarks:

- `BenchmarkIntersectSegmentsRobust` — median 12779.5 ns/op across the ten
  retained runs;
- `BenchmarkOffsetCertificateAndVertex` — median 20949.0 ns/op across the ten
  retained runs.

All other applicable thresholds observed by this rerun were satisfied as
recorded below. Per the criterion's failure-handling rule, this outcome feeds
NUMERICAL-MODEL §11 trigger-3 assessment; the ratification/promotion gate
remains **OPEN** pending that revision disposition. Nothing here promotes any
numeric-domain guarantee or closes ADR 0002 gate 1.

## Identity chain

- Criterion document: [PHASE0-PERFORMANCE-CRITERION.md](PHASE0-PERFORMANCE-CRITERION.md),
  blob sha256 `7f4ac272174abf6ef04147dda63794b40be527ae`.
- Canonical pre-measurement provenance:
  `build/0030-performance-raw-v2/precheck/env-premeasurement-FINAL.txt`;
  canonical snapshot
  `7f8aca35d7dbcdaf7d5ac575cf47f576285e8df75b3e9c59e815c3bc668cdd02`.
- Measured test binary: `build/0030-performance-raw-v2/precheck/geometry.test`,
  sha256 `13c91b1e13fc071327cb9b21795d9d4793d0020ff05a4e7233d5e98eddc0fb8a`,
  asserted equal PRE-RUN and POST-RUN in `build/0030-performance-raw-v3/rss/status.log`;
  no rebuild was performed.
- Countersigned analysis package (provisional, internally consistent):
  - `build/0030-performance-raw-v3/analysis/REPORT.txt`
    sha256 `92854f17a35c19c6344efb386611af3c952fad427c284eca1d722fe2eae7c15d`
  - `build/0030-performance-raw-v3/analysis/PROVENANCE.txt`
    sha256 `5b61917a441bccd5328abdb387e105b3c8b6d820727aade2eb39a6f735090f03`
  - `build/0030-performance-raw-v3/analysis/ANALYZER-HASH.txt`
    sha256 `4bb835e17c85f7bf89fc2ef167535bc00a4fd284930141bee88b9b7a5ce776d5`
- Prior raw roots `build/0030-performance-raw/` (v1) and the partial v2 W-B
  outputs (`build/0030-performance-raw-v2/wb/`) are DIAGNOSTIC/NON-JUDGEABLE
  and preserved untouched. The historical 0006/0007 recorded evidence remains
  NOT JUDGEABLE (single-run means, aggregate boolean timing, no RSS, no
  qualifying H-6 counts); only the v3 rerun is judgeable for its recorded scope.

## Executed protocol (facts only)

- Shell pins for every invocation: `GOTOOLCHAIN=local GOMAXPROCS=32
  CGO_ENABLED=1 GOCACHE=/tmp/domowik-0030-go-cache`.
- W-P, W-B, H-6 suites: one discarded warmup plus ten retained independent
  `go test` processes each; exact registered selectors; flags `-benchmem
  -benchtime=1s -count=1 -cpu=32 -v`; stdout and stderr retained separately in
  `build/0030-performance-raw-v3/{wp,wb,h6}/`; every run exit=0 per status.log.
- RSS suite: GNU time absent on the reference host, so the pre-pinned
  equivalent tool applies — same-process `/proc/self/status` field spelled
  exactly `VmHWM:` (uppercase), value in kB interpreted as KiB, one process per
  case, twelve frozen corpus cases in frozen order, exactly one VmHWM line per
  case, artifacts under `build/0030-performance-raw-v3/rss/`.

## Provisional per-threshold results (from the countersigned REPORT.txt)

These are observations of the recorded data, not an independent verdict beyond
the scope verdict above:

- A3 filtered ≤ 1000 ns/op median: within bound (all
  `BenchmarkOrientationFiltered` subcases).
- A3 robust ≤ 10000 ns/op median: **EXCEEDED** by the two benchmarks named
  above; `BenchmarkOrientationAdaptive` and `BenchmarkUnitDirection` within
  bound.
- A1 ≤ 250 ms/case median: within bound (max courtyard-island ≈ 2.57 ms).
- A2 conservative max-tail ≤ 1 s: within bound (max per-case median 2.5672 ms).
- A6 non-adversarial fallback ≤ 20%: easy = 0.0000 within bound;
  hard-filter-fail and classed-bits are adversarial and carry no bound.
- A4 RSS < 524288 KiB per case: within bound (max 13160 KiB).
- W-B rejects/op = 0 for every case in every retained run (fail-closed check).
- H-6: qualifying runtime counts exist only for the orientation/direction
  production paths. The boolean-pipeline, CandFaces, OracleFaces, and B-1q
  certification domains remain MISSING (absent from the evidence, never zero).

## Scope guard

PARTIAL / NOT COMPLETE remains in force. The numerical model is NOT RATIFIED.
ADR 0002 remains Proposed. All other open gates and hypotheses are unchanged.
No threshold or protocol text of the registered criterion is modified by this
record.
