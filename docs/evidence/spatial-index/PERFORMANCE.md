# Performance — spatial index and hit testing (issue 0012)

Host: Apple Silicon (macOS 25.6.0, arm64), Rust 1.98.0, `cargo bench
--release`, criterion median. Timings are host-dependent and are NOT a
pass/fail gate (ED-0070); `scripts/verify.sh` compiles the harness on every run
so it cannot rot.

## Dataset distribution

`grid(n)`: an n x n arrangement of 4 m x 3 m rooms sharing walls — walls short
relative to the plan, spread evenly, meeting at corners, which is the shape a
real floor plan has.

| Shape | Walls | Stands for |
|---|---:|---|
| `grid(4)` | 40 | A large flat |
| `grid(10)` | 220 | A small office floor |
| `grid(20)` | 840 | Past anything a v0.1 user is expected to draw |

## Measured

| Benchmark | grid(4) | grid(10) | grid(20) |
|---|---:|---:|---:|
| `build` (committed update) | 1.65 µs | 7.3 µs | 26.3 µs |
| `cull` (visible region) | 328 ns | 303 ns | 323 ns |
| `point-hit/one-shot` | 28.5 µs | 29.2 µs | 77.9 µs |
| `point-hit/indexed` | **287 ns** | **229 ns** | **235 ns** |
| `preview` (proximity) | 72 ns | 73 ns | 87 ns |
| `marquee` | 508 ns | 2.38 µs | 9.31 µs |

Budgets, which are REGRESSION ALARMS rather than tuning targets: under 100 µs
for a query at `grid(10)`, under 1 ms for a build. Every row is inside them.

## What the numbers say

**Separating committed updates from preview queries is worth 125x, and the
issue was right to require it.** A one-shot `hit_test` builds both indexes,
answers, and throws them away; at 840 walls that is 78 µs and it GROWS with the
plan. A `HitIndex` built by the last committed update answers in 235 ns and
does not grow — the query touches the cells around the pointer and nothing
else. A drag that rebuilt per pointer move would spend seventy times more on
construction than on the answer.

Both paths are kept, because both are correct uses: a click pays 28 µs once and
needs no lifetime management, while a drag holds the index across a gesture.
`a_reused_index_answers_exactly_as_the_one_shot_call_does` sweeps a grid of
query points and asserts the two agree, because a preview that highlighted one
thing and a click that selected another would be worse than either being slow.

**Culling and proximity are flat in plan size**; hit testing and marquee are
not, for different reasons. `marquee` is deliberately linear: R7.3.2 makes it
return every intersecting entity in marquee-key order, so the result set is
proportional to the plan and no index changes that.

## A negative result, recorded because it corrects an earlier claim

`docs/evidence/wall-joins/PERFORMANCE.md` predicted this index would speed up
`boolean::union`, whose ring-pair selection and containment test are both
quadratic. It was wired in and measured: **364 µs without it and 365 µs with
it** on a 220-wall grid, plus a 3-9% regression on small plans from the branch's
allocation. The change was reverted and that document corrected.

The reason is plain once measured: the bounding-box filter the index replaced
is a few comparisons per ring pair, while the cost lives in the narrow phase —
an intersection test per edge pair, an exact point-in-ring classification per
fragment. Finding the neighbours faster does not reduce the work done with
them, because the neighbour SET is identical either way.

## Not measured

**Allocation counts.** Issue 0012's criterion asks for them, written against
Go's `testing.B`, which reports them for free. Rust's standard library has no
allocation counter, and adding a counting global allocator to the workspace to
satisfy a criterion inherited from another toolchain would be the wrong trade.
Recorded as a known gap rather than quietly dropped.

**Percentiles.** Criterion reports a confidence interval on the mean, not p50
and p99. A percentile budget needs a harness that keeps the sample; that
belongs with the interaction corpus of issue 0032, which owns percentiles,
memory bounds, and thresholds for exactly this reason.
