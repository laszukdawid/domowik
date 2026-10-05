# Phase 0 Performance Criterion — Pre-Registered Gate

Status: **Pre-registered before judgment.** This document defines the Phase 0
performance criterion required by [NUMERICAL-MODEL.md](../../NUMERICAL-MODEL.md)
§11 and issue 0030's define-before-judge acceptance criterion. It was authored
and committed BEFORE opening any recorded benchmark number in
[evidence/0006-predicates/BENCHMARKS.md](../0006-predicates/BENCHMARKS.md) or
[evidence/0007-booleans-topology.md](../0007-booleans-topology.md). No
threshold below was chosen from, checked against, or adjusted after observed
results. Any later change to a threshold must appear as a dated addendum at
the end of this file with its own derivation; in-place edits are prohibited.

## Provenance of this registration

- Author: issue 0030 executor (Ox Alpha), 2026-08-24.
- Inputs used: ADR 0002 gate 1 wording; the project Performance Policy in
  `EXECUTIVE-DECISIONS.md`; NUMERICAL-MODEL §11 evidence-chain ruling
  (producers: 0006 predicate benchmarks, 0007 pipeline wall-clock/allocation
  measurements, H-6 fallback/escalation counts; judge: 0030); the names and
  structure of existing benchmark functions in `internal/geometry/predicates`
  and `internal/geometry` test code; product interaction-latency reasoning
  stated inline. No recorded measurement, table, or timing figure from 0006 or
  0007 evidence was read prior to this registration.
- Reviewer countersignature: recorded in the review log once rev0030 confirms
  the thresholds were registered before judgment and are falsifiable as
  written.

## Scope of the judged workloads

Only the following recorded evidence enters the Phase 0 performance judgment:

1. **W-P — predicate/contact-classification benchmarks (issue 0006).** The
   recorded Go benchmark suites for orientation decisions (filtered,
   adaptive/exact fallback, naive comparison), robust segment intersection
   parameter extraction, unit-direction construction (§4.2), and
   offset-certificate/vertex construction (§4.4), over 0006's named corpora.
2. **W-B — boolean pipeline measurements (issue 0007).** Wall-clock and
   allocation measurements of the B-1 boolean pipeline on the same named
   corpus as its correctness runs, including whatever fixture setup cost its
   methodology declares (declared costs are reported verbatim, not netted out;
   see exclusions).
3. **W-H6 — escalation/fallback counts (both issues).** Recorded frequencies
   of filter→exact fallbacks and any §4.3-grade algebraic escalations on each
   corpus class.

### Explicit exclusions

- **CandFaces and OracleFaces timings are EXCLUDED.** Per ED-0011 the flat
  `CandFaces([][2][2]float64)` API is characterization-only; all prior face
  conformance, determinism, AND performance claims are withdrawn. No face
  timing may support or fail the Phase 0 gate until a structured
  provenance-preserving face input is specified and re-measured under its own
  pre-registered criteria, or face extraction is explicitly rejected.
- **Post-normalization boolean agreement counts are correctness evidence, not
  performance evidence**, and do not substitute for any threshold here.
- Benchmarks of withdrawn or unselected strategies are excluded except as
  comparative context; they cannot satisfy W-B for a different strategy.

## Reference environment

The judged environment is exactly the reference environment named in the
evidence records for the corresponding correctness runs (CPU model, core
count, memory, OS/kernel, frequency scaling policy if recorded) plus the
reference Go toolchain (currently go1.27.0 as reference pin; operative pin
owned by tracker 0008). If the evidence does NOT fully name its environment,
the performance evidence chain is incomplete: the gate is **not judged** and a
rerun obligation is recorded instead. Thresholds below remain fixed
regardless of which hardware is ultimately named; no threshold scales with
observed machine speed.

## Measurement protocol

- Harness: `go test -bench` (Go testing framework), `-count=10` independent
  process runs per benchmark.
- Warm-up: the framework's automatic ramp-up iterations, plus discarding of
  the first of the 10 runs.
- Timing source: the Go runtime monotonic nanosecond timer used by
  `testing.B` (never date/time wall-clock).
- Statistic: **median** ns/op (or s/op) across the 10 runs is the judged
  value; min/max spread is reported alongside as reproducibility context but
  is not itself pass/fail.
- Allocations: `allocs/op` and `bytes/op` recorded as mandatory evidence;
  see memory clause for the only hard bound.
- Reproducibility: the evidence record must state exact commands so any
  reviewer can rerun on comparable hardware.

## Thresholds (fixed at registration)

Derivation anchors, stated before any measurement:

- **A1 (interaction anchor).** A committed edit must complete local geometry
  revalidation fast enough to feel synchronous; 100 ms is the conventional
  upper bound for instantaneous-feeling feedback. Phase 0 evaluates small
  normative fixtures (handfuls of walls per case) with no spatial index and no
  caching, so a whole-pipeline per-case budget of **250 ms median** gives ≥
  2.5× headroom against that anchor while remaining far below anything an
  indexed, incremental Phase 1+ implementation could excuse.
- **A2 (tail safety).** Degenerate/adversarial cases may escalate but must not
  blow up: p95 across corpus cases ≤ **1 s**.
- **A3 (predicate non-dominance).** Sign decisions are invoked many times per
  pipeline case; individual robust predicate operations must stay in the
  microsecond range so predicates cannot dominate A1. Robust-path classes:
  median ≤ **10 µs/op** (≥ 10⁵ ops/s). Filtered fast-path classes: median ≤
  **1 µs/op** (≥ 10⁶ ops/s).
- **A4 (memory floor).** Peak resident memory of any single corpus-case
  process < **512 MiB** — a deliberately generous desktop/mobile floor whose
  only purpose is to catch pathological blowups (e.g., runaway exact
  arithmetic), not to optimize.
- **A5 (allocation recording).** Allocation counts are recorded evidence only
  at Phase 0; the hard allocation criterion is deferred to Phase 1 targets
  (ADR 0002 gate 2, tracker 0032). This is a deliberate scope decision,
  registered now, not an omission discovered later.
- **A6 (escalation sanity).** On NON-adversarial fixture corpora, the fraction
  of sign decisions requiring exact fallback beyond the first-level filter ≤
  **20%**. Adversarial/extreme-conditioning corpus classes carry NO fallback
  rate threshold — forcing escalation there is their purpose; their H-6 counts
  feed cost modeling instead. Budget breaches (`geometry-indeterminate`) are
  correctness failures handled by the conformance protocol, not performance
  statistics.

### Pass/fail

The Phase 0 performance criterion is MET iff every applicable threshold above
is satisfied by the recorded evidence judged under the stated statistic and
protocol, with a fully named reference environment. It is FAILED if any
applicable threshold is missed. It is NOT JUDGEABLE if the evidence chain is
incomplete (missing environment naming, missing commands, missing H-6 counts,
or excluded-workload substitution).

### Failure handling

A failure or non-judgeable verdict triggers NUMERICAL-MODEL §11 revision-
trigger assessment (trigger 3): whether a caching/locality strategy (R5.1.9)
plausibly closes the gap. Outcomes are limited to: revise the candidate model
and RERUN the spikes under a new pre-registered criterion, or honestly revise
ADR 0002's gate status/custom-burden assessment. A failure may never be
silently waived, reinterpreted post hoc, or absorbed into a later phase.

## Relationship to open hypotheses

This criterion judges ONLY what the evidence chain records. It does not
certify H-0 (algebraic sign-evaluation cost), H-1 (intersection-parameter
chains beyond C1), H-2 (true hull evaluation bounds), H-3 (boolean vertex
generation bounds), H-4 (face assembly bounds), H-5 (area-chain propagation),
or B-1q-style runtime certify-or-escalate wiring. Any of these left open
after judging remains open in the ratification record regardless of the
performance verdict; open hypotheses block promotion of affected guarantees
to accepted status even when all thresholds pass.

## Addendum 2026-08-24 — protocol corrections and NOT-JUDGEABLE finding

Registered after adversarial review of the registration text; per the header
rule this addendum changes NO threshold value. It corrects protocol wording,
pins workload mappings, and records a verdict about EXISTING evidence.

### Verdict on existing recorded evidence: NOT JUDGEABLE

The recorded 0006/0007 performance evidence cannot be judged against this
criterion:

1. 0006 records single-run (`-count=1`) mean ns/op figures, not the registered
   multi-process median protocol.
2. 0007 reports one aggregate ns/op/allocs line covering all corpus cases with
   operand construction inside the timed region; it contains neither
   per-case medians nor any tail statistic, so A1/A2 are unsatisfiable from
   it as recorded.
3. No RSS-per-case measurement exists for A4; allocation counts cannot stand
   in for resident memory.
4. No qualifying W-H6 runtime counts exist: withdrawn `CandFaces` counters
   are excluded (ED-0011), OracleFaces counts are reference-only for the exact
   oracle, B-1q certify-or-escalate is unimplemented, and no predeclared
   non-adversarial/adversarial partition exists behind recorded fallback
   percentages, so they cannot be classified under A6 retroactively.

Consequence: the Phase 0 performance gate is **OPEN**. It is neither passed
nor failed. Satisfying it requires reruns implementing the protocol below.
No pass/fail judgment may be written from existing numbers.

### Process-independence correction

`go test -count=10` reuses one process and does not yield ten independent
process runs. The registered protocol is therefore: an external shell loop
invoking `go test -bench` once per run as a separate process, one additional
warm-up invocation whose results are discarded entirely, then the judged
statistic computed across the retained process runs. GOMAXPROCS is pinned to
the same value for every invocation and that value is recorded alongside CPU/
toolchain identification.

### Benchmark-to-threshold mapping

| Threshold | Qualifying benchmarks (rerun under protocol) | Notes |
|---|---|---|
| A3 filtered ≤ 1 µs/op | `BenchmarkOrientationFiltered` sub-benchmarks | naive comparisons are context only |
| A3 robust ≤ 10 µs/op | `BenchmarkOrientationAdaptive`, `BenchmarkIntersectSegmentsRobust`, `BenchmarkUnitDirection`, `BenchmarkOffsetCertificateAndVertex` | all robust/exact-capable paths |
| A1 ≤ 250 ms/case median | boolean pipeline per-case timing on the named correctness corpus | operands prebuilt outside the timed region, or operand-construction cost measured separately and reported alongside |
| A2 p95 ≤ 1 s | same population as A1 | definition below |
| A4 < 512 MiB RSS/case | per-case OS-level peak RSS: GNU `time -v`, field `Maximum resident set size (kbytes)`, unit KiB, one process invocation per case | allocations alone never satisfy A4; if GNU time is unavailable in the rerun environment, the rerun plan MUST pin an equivalent tool, field, and unit BEFORE executing, recorded in the evidence |

Any benchmark not listed maps to no threshold and is context only.

### Tail statistic definition

A2's population is the set of per-case judged values (each case's median
across its retained process runs). With fewer than 20 cases in the corpus,
p95 is ill-conditioned; the conservative substitute is the MAXIMUM per-case
median, which is what A2 then judges. This is fixed now, before reruns.

### Corpus partition (predeclared)

- **Non-adversarial class:** BEHAVIOR-SPEC section 3–5 normative fixture cases
  exercised in their plain stored form — rational coordinates within §3
  bounds, no extreme direction conditioning, no deliberately degenerate or
  escalation-forcing variants. Only this class falls under A6's 20% bound.
- **Adversarial class:** 0006's extreme-κ_dir, mixed-scale, minimal-thickness,
  and other deliberately escalation-forcing corpora, plus any fixture variant
  explicitly constructed to force filter failure. These carry NO fallback-rate
  threshold.
- Every corpus case must be assigned to exactly one class IN THE RERUN PLAN
  BEFORE execution. Recorded aggregate percentages without such a partition
  remain unclassifiable context.

### W-H6 qualification rule

Fallback/escalation counts qualify only if produced at runtime by an
implemented path whose output feeds an observable decision on the corpus, with
the partition above applied. Excluded: characterization-only APIs (ED-0011),
oracle/reference implementations, and unimplemented certification paths. If
no qualifying counts exist, A6 is NOT JUDGEABLE (contributing to an overall
NOT JUDGEABLE verdict), never passed by absence of instrumentation.

### Scope guard on downstream promotion

Until the gate closes, nothing in the numeric domain is promoted: coordinate
bound B = 10⁹ mm, derived-coordinate D = B + 10⁶ mm, dimension caps (10⁶ mm),
proposed diagnostics (`coordinate-out-of-range`, `dimension-out-of-range`,
`geometry-indeterminate`, `area-tie-indeterminate`), commitment/error budgets,
and any geometry API guarantees remain PROPOSED/open or explicitly rejected —
never ratified by implication. FORMAT-SPEC L14 stays deferred to its owning
gates and BEHAVIOR-SPEC R3.0.8 deferrals stay unresolved; normative display
semantics are untouched by this criterion. In particular NO public face API or
structured face contract is ratified here; the face-input decision is made
separately in issue 0030's ratification record with its own contracts, tests,
and provenance gate, or an explicit reject/defer entry.

## Addendum 2026-08-24 (2) — W-B/A1 judged-value freeze

The A1 wording permitting either prebuilt operands or separately measured
construction left the judged workload selectable. Frozen now, before any
rerun and independent of results:

- The judged A1 value for each corpus case is the wall-clock of the
  boolean pipeline execution (CandBoolean-class entry point) on PRE-BUILT
  stored-float operands, including ALL conversion, validation, sweep,
  normalization, and terminal commitment inside that execution.
- Deterministic corpus/operand construction happens once outside timing;
  external measurement/report parsing is outside timing.
- Operand-construction cost MAY be measured and reported as secondary
  context only. It can never substitute for, be added to, or be traded
  against the A1 value, and no rerun may choose inclusion or exclusion
  based on observed results.

No threshold value changes in this addendum.
