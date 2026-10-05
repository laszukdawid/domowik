# Performance — occupied region and room detection

Host: Apple Silicon (macOS 25.6.0, arm64), `cargo bench --release`, criterion
median of the reported interval. Timings are host-dependent and are NOT a
pass/fail gate (ED-0070); `scripts/verify.sh` compiles the harness on every run
so it cannot rot, and these numbers are the record to compare against, not a
threshold to defend.

## Measured

| Benchmark | Shape | Median |
|---|---|---|
| `orientation/filtered` | non-degenerate triple | **1.12 ns** |
| `orientation/exact-fallback` | collinear triple | **122 ns** |
| `wall_polygon` | one butt-capped rectangle | 27.4 ns |
| `occupied-region/single-room` | 4 walls, 4 joins → 8 rings | 26.4 µs |
| `occupied-region/apartment` | 8 walls, 4 joins → 12 rings | 40.9 µs |
| `occupied-region/grid-4x4` | 10 crossing walls → 10 rings | 65.4 µs |
| `occupied-region/grid-10x10` | 220 walls | 356 µs |
| `plan/single-room` | region **and** rooms | **25.5 µs** |
| `plan/apartment` | region and rooms | 41.2 µs |
| `plan/grid-4x4` | region and rooms | 64.9 µs |

## What correctness cost, 2026-09-01

The exact orientation fallback was wrong — it subtracted coordinates with
ordinary rounding and compensated only the products, so it returned the
OPPOSITE sign on inputs inside the coordinate bound. Fixing it (error-free
subtraction, then an exact expansion sum) moved the fallback from 1.79 ns to
**122 ns**, and moved the shapes that lean on it:

| Shape | Before | After | Change |
|---|---:|---:|---:|
| `plan/single-room` | 14.2 µs | 25.5 µs | +77% |
| `plan/apartment` | 30.3 µs | 41.2 µs | +36% |
| `plan/grid-4x4` | 65.7 µs | 64.9 µs | none |
| `occupied-region/grid-10x10` | 359 µs | 356 µs | none |

The small shapes pay most because they are almost entirely axis-aligned, so
nearly every predicate call lands on the fallback; the large ones are dominated
by the quadratic broad phase instead. 25 µs is still three orders of magnitude
inside a 16 ms interactive frame, and the figure that motivated the rewrite —
Go's unconditional exact path at 12,134 ns — is still a hundred times worse.

**This is the right trade and it is not close.** A predicate that is fast and
sometimes returns the wrong side of a line makes every derived answer — room
areas, unions, hit tests, wall splits — unreliable in a way no amount of speed
compensates for.

## What the numbers say

**Rooms are nearly free once the region is built: 0.2–0.4 µs of the total.**
They are traced from the same boundary as the region, in the reverse direction,
so the splitting and classification that dominate the cost are paid once. This
is why `plan` and `occupied_region` measure within 1% of each other, and why
`occupied_region` computes the rooms and discards them rather than keeping a
second, cheaper path that would have to be kept correct separately.

**The union is quadratic in rings, and visibly so.** 8 rings cost 14.0 µs,
12 rings cost 29.7 µs. Each ring's edges are cut against every other ring's
edges, and each surviving fragment's midpoint is classified against every other
ring. Bounding-box rejection was added before these numbers were taken and is
worth 18–35% on these shapes; it does not change the asymptotics.

**It is nonetheless within budget, for a structural reason rather than a lucky
one.** R5.1.9 scopes recomputation to the join-connected components an edit
actually touches, not the level and never the document. An edit therefore pays
for one cluster — a room or two — which is the 14–30 µs column, roughly three
orders of magnitude inside a 16 ms interactive frame. A whole-plan rebuild is
not on the interactive path.

**Where the next win is NOT.** This document previously said both quadratic
loops were broad-phase problems that issue 0012's spatial index would fix. That
was a guess, and measuring it showed it was wrong.

The index landed on 2026-08-31 and was wired into `boolean::union`'s ring-pair
selection and its point-in-ring containment test. On a 220-wall grid the result
was **364 µs without the index and 365 µs with it** — no gain, and a 3-9%
regression on small plans from the allocation the branch introduced. The change
was reverted.

The reason, once measured, is obvious in hindsight: the bounding-box filter it
replaced is a handful of comparisons per ring PAIR, while the cost is in the
narrow phase — an intersection test per edge pair and an exact point-in-ring
classification per fragment. Finding the neighbours faster does not reduce the
work done with them, because the neighbour SET is identical either way.

A real win would have to cut narrow-phase work: fewer candidate edges per cut,
or a cheaper containment test. Until a corpus from issue 0032 shows a real plan
missing a real budget, guessing again would be the same mistake.

## Not measured

No benchmark here covers a plan at the scale issue 0032 will define, and none
should be read as evidence about one. `grid-4x4` is a stress shape chosen to be
past any real floor plan in crossing density at small size, not a substitute for
a corpus. Nor is the R5.1.9 scoped recomputation measured, because it is not
implemented: the current code rebuilds a whole level per query, and the argument
above is about what the rule REQUIRES, not what is running.
