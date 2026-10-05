# Canonical region hit ordering — 2026-09-05

Issue 0047 supplies the missing canonical outer-ring key before entity ID in
native region hit ordering. It does not supply exact-real distance predicates;
issue 0012 retains that obligation. No dependency or document format changes.

## Correctness

`equal_distance_regions_compare_geometry_before_identity` failed against the
original comparator: UUID 1 won instead of UUID 2 despite its later canonical
ring key. It now passes across document-order and endpoint-direction
permutations. The existing identical-region fixture still checks the final UUID
tie-break. All 114 geometry library tests passed in the scoped run.

## Measurements

Host: Apple M4 Max, 16 logical CPUs, 48 GiB RAM, macOS 26.6.2 (25G83).
Toolchain: rustc 1.98.0 (88d9e12ae 2026-08-18), release bench profile.
Criterion: 20 samples, 0.2 s warm-up, 0.5 s measurement per case. These are
confidence intervals on the mean, **not latency percentiles or frame times**.
Other agents paused builds during the measurement window; this is not an
isolated benchmark machine. CPU/native query costs exclude FFI and Flutter.

| Distribution / path | Before | Correct canonical ordering |
|---|---:|---:|
| 10×10 room grid, 220 walls, one-shot build + query | 28.890–29.187 µs | 35.073–35.278 µs |
| Same grid, cached indexed query | 240.71–247.27 ns | 261.67–265.72 ns |
| 32 overlapping unequal regions, build | Not measured | 5.3369–5.3963 µs |
| Same 32 regions, one-shot | Not measured | 6.2234–6.2685 µs |
| Same 32 regions, indexed | Not measured | 854.61–866.89 ns |
| 220 overlapping unequal regions, build | Not measured | 27.362–27.650 µs |
| Same 220 regions, one-shot | Not measured | 31.825–32.117 µs |
| Same 220 regions, indexed | Not measured | 4.2239–4.3147 µs |

The correctness fix has a measured cost: approximately 22% for one-shot grid
queries and 9% for reused grid queries in this short run. Canonical rings are
normalized and allocated at construction, never inside the query comparator.
The editor bridge caches one active-level index so unchanged clicks do not
repeatedly pay the build cost. The existing grid query budget is 100 µs; this
measurement does not establish whole-application performance or allocation
bounds. The overlap distribution deliberately forces the new comparison key;
it is not representative of a residential plan.

Reproduce from the repository root:

```sh
cargo test -p domowik-geometry --lib
cargo bench -p domowik-geometry --bench spatial -- 'spatial/(point-hit/(indexed|one-shot)/grid-10x10|tied-regions)' --sample-size 20 --warm-up-time 0.2 --measurement-time 0.5
```

Baseline was measured before the comparator change in this worktree, against
the source at `60984a4`. The tie-heavy benchmark is new and has no baseline;
historical Criterion comparison output is not evidence for those cases.
