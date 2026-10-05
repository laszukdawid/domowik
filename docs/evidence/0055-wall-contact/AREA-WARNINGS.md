# Wall area-overlap and query benchmarks

Status: verified checkpoint at `b0bacc2`, 2026-09-06. This report covers the new
non-rejecting wall-area-overlap diagnostics and reusable spatial-query scratch
path. It does not close issue 0055.

The warning pass diagnoses legal positive-area wall overlap, while these
benchmarks measure its cost on complete documents. Scratch-backed local queries
are faster in these fixtures. These are regression measurements, not a GUI
frame-time claim.

## What is measured

`rectangles_overlap` tests ideal centered, butt-ended wall rectangles. It first
uses an outward-rounded floating-point interval filter; uncertain cases use the
exact dyadic separating-axis test. The benchmark checks three ordinary
filtered cases and three exact-boundary or mixed-magnitude witnesses before
timing.

`validation_warnings` builds conservative per-wall bounds, reuses one
`QueryScratch` across its queries, applies the exact rectangle predicate, and
allocates the deterministic warning list. Clean fixtures contain spaced
parallel axes. Warning fixtures contain independent copies of the specified
nonparallel pair. Counts are 0 and 50, 250, and 1,000 warnings for 100, 500,
and 2,000 walls.

The spatial benchmark queries deterministic axis-aligned boxes and checks that
ordinary and scratch-backed queries return the same values and counts. It is a
bulk-query comparison, not a whole-frame measurement.

## Host and method

Runs used the same named host as the adjacent issue 0055 checkpoint:
Apple M4 Max, 48 GiB RAM, macOS 26.6.2, arm64, Rust 1.98.0. Each command used
20 samples, a one-second warm-up, and a two-second measurement window. The
requested quiet window covered project builds and tests, but this sandbox could
not inspect the operating-system process list, so OS-level process quiescence
was not independently verified.

```sh
cargo bench -p domowik-geometry --bench predicates -- wall-overlap \
  --sample-size 20 --warm-up-time 1 --measurement-time 2
cargo bench -p domowik-geometry --bench spatial -- bulk-query \
  --sample-size 20 --warm-up-time 1 --measurement-time 2
cargo bench -p domowik-document --bench session -- validation-warnings/walls \
  --sample-size 20 --warm-up-time 1 --measurement-time 2
```

All three commands exited 0. Raw logs:

- `/private/tmp/domowik-0055-area-predicates-bench.log`
- `/private/tmp/domowik-0055-area-spatial-bench.log`
- `/private/tmp/domowik-0055-area-warnings-bench.log`

## Predicate timings

Criterion reports the lower bound, estimate, and upper bound below, in that
order.

| Case | Interval |
|---|---:|
| Filtered, parallel clear | [69.757, 70.352, 70.793] ns |
| Filtered, parallel overlap | [104.28, 107.40, 110.05] ns |
| Filtered, nonparallel specification fixture | [107.20, 108.65, 109.92] ns |
| Exact, touching | [1.2496, 1.2629, 1.2757] µs |
| Exact, one-ULP separated | [1.2129, 1.2252, 1.2419] µs |
| Exact, mixed subnormal | [1.2917, 1.2955, 1.2994] µs |

One mild high outlier appeared in the nonparallel filtered case. Assertions
classified all six fixtures before timing. Dedicated geometry tests, rather
than the benchmark assertions, verify that uncertain cases take the exact
fallback branch.

## Spatial bulk-query timings

Each row measures the complete batch of 100, 500, or 2,000 local queries.

| Boxes and queries | Ordinary query | Reused scratch |
|---:|---:|---:|
| 100 | [3.8935, 3.9065, 3.9156] µs | [3.1916, 3.2109, 3.2299] µs |
| 500 | [24.198, 24.251, 24.297] µs | [17.017, 17.200, 17.391] µs |
| 2,000 | [124.93, 125.98, 127.31] µs | [66.576, 66.847, 67.118] µs |

The scratch allocation is warmed before measurement by benchmark setup; each
query still allocates its result vector. The benchmark does not measure render
preparation, painting, input dispatch, or a frame budget.

## Whole-document warning traversal

| Walls | Clean, spaced axes | Independent warning pairs |
|---:|---:|---:|
| 100 | [8.9726, 9.0377, 9.1065] µs | [14.980, 15.040, 15.090] µs (50 warnings) |
| 500 | [42.707, 42.886, 43.049] µs | [72.389, 72.622, 72.818] µs (250 warnings) |
| 2,000 | [175.62, 179.74, 185.16] µs | [290.86, 292.28, 293.66] µs (1,000 warnings) |

Fixtures are constructed and sanity-checked outside timed closures. Each
document validates, and every fixture pair is `AxisContact::Disjoint`; the
warning path measures positive-area rectangle overlap rather than refusing axis
contact. The warning function is read-only and recomputes the current final
document, so callers can refresh diagnostics after an accepted thickness or
position edit. The editor caches the result for one accepted revision to avoid
duplicate traversal. CLI and native bridge callers use the exported
`validation_warnings(&Document)` entry point; warnings remain separate from
admission errors.

## Native warning pipeline

The native timing harness ran one ignored release test with 31 samples per
cell. The reported median, p95, and maximum are empirical order statistics for
those samples; they are not population-tail guarantees.

| Path | 100 walls | 500 walls | 2,000 walls |
|---|---:|---:|---:|
| Mutation | median 78.291, p95 141.292, max 182.709 µs | median 179.708, p95 207.583, max 211.042 µs | median 685.583, p95 764.708, max 771.333 µs |
| Undo | median 49.583, p95 56.667, max 57.583 µs | median 171.875, p95 192.333, max 194.291 µs | median 676.875, p95 698.250, max 707.500 µs |
| Redo | median 38.167, p95 42.500, max 43.708 µs | median 173.833, p95 198.416, max 206.209 µs | median 690.792, p95 718.958, max 733.250 µs |
| Cached state | median 7.917, p95 8.042, max 9.125 µs | median 37.917, p95 48.000, max 62.458 µs | median 153.875, p95 166.125, max 176.459 µs |

The fixture contains isolated parallel pairs, producing 50, 250, or 1,000
warnings. Mutation changes only one wall's height through raw operations;
undo and redo act on that accepted change. Each size and operation has a
warning-count preflight outside measurement.

Mutation, undo, and redo include serialization and buffer freeing. They exclude
fixture construction and handle close. They do not measure the gesture builder
or Flutter. Cached-state timing measures the already cached revision path.

Command, run from `spikes/0024-flutter-rust-bridge`, and raw output:

```sh
cargo test --release --test warning_pipeline warning_pipeline_timing \
  -- --ignored --nocapture
```

The command exited 0. Raw log: `/private/tmp/domowik-0055-area-native-timing.log`.

## Verification state and remaining scope

The workspace test run exited 0 (`cargo test --workspace --locked`), covering
127 document tests, 135 CLI tests, 165 geometry unit tests, four overlap-oracle
tests, and two axis-oracle tests. The full Flutter ABI10 run also exited 0:
analysis passed and all 154 tests passed. ABI10 corrected an incompatible wire
change: `plan_edit` formerly returned raw UUID bytes and now returns a JSON
summary. Execute, history, and warning fields remain additive, and persisted
files are unchanged. The final bridge unit run passed 57 tests, including
thickness-only warning changes through edit/undo/redo and the shared cache
counter. The C harness passed 114 checks under ASan/UBSan; the Dart FFI harness
passed 85 checks. These commands exited 0.

The canonical verifier also exited 0 on `b0bacc2`, ending with
`=== VERIFICATION PASSED === all stages`, with no skipped stages:

```sh
PATH="/Users/dawidlaszuk/develop/flutter/bin:$PATH" ./scripts/verify.sh
```

Raw local log: `/private/tmp/domowik-0055-area-verify.log`. This includes
dependency/tracker checks, benchmark compilation, the native harnesses,
Flutter analysis and all 154 tests, viewport measurement harnesses, and the
clean universal macOS bundle/signature and built-application probes. The
opt-in native timing test is intentionally ignored in ordinary test runs;
its separate successful measurement is recorded above. This existing-checkout
pass does not settle issue 0057's missing-archive first-build failure or
issue 0056's intermittent SDK module failure.

Issue 0055 still owes crossing C1 and the join-policy boundary tracked by issue
0030, plus remaining raw-operation and intersection-construction conformance
work. These area-warning measurements do not prove those behaviors.

No GPL application source, fixtures, or assets informed this work. The
predicates and query traversal are independently implemented from Domowik's
specification. No new external dependency was added.
