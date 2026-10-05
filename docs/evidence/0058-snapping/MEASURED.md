# Snap query cost — 2026-09-12

A 21-fold larger document costs at most twice as much per query, so snap
evaluation does no document-wide work per pointer move. That is what issue
0058's sixth acceptance criterion asks. The absolute microsecond figures below
were measured on a host carrying background load and are an upper bound, not a
best case.

`snap/query-*` measures one `SnapIndex::query` against a prebuilt index: the
per-pointer-move cost. It excludes the index build, the FFI boundary and
Flutter.

## Measurements

Host: Apple M4 Max, 16 logical CPUs, 48 GiB RAM, macOS 26.6.2 (25G83).
Toolchain: rustc 1.98.0 (88d9e12ae 2026-08-18), release bench profile.
Criterion: 20 samples, 0.5 s warm-up, 1 s measurement per case. These are
confidence intervals on the mean, **not latency percentiles or frame times**.

Host load during the measurement window: 1-minute load average 1.54 rising to
2.37, on a machine also running a Kandji endpoint-security extension (18.9%
CPU), a Virtualization.framework VM (8.6%), Docker Desktop (8.5%) and a
leftover headless Android emulator from issue 0024 (4.1%). No other Domowik
agent was building or benchmarking. **This is not an isolated benchmark
machine**, and the figures should be read accordingly.

The corpus is an n×n grid of rooms sharing walls: 40 walls at 4×4, 220 at
10×10, 840 at 20×20.

| Query | 40 walls | 220 walls | 840 walls | Growth, 40 → 840 |
|---|---:|---:|---:|---:|
| Corner, four walls meeting | 6.2648–6.3263 µs | 8.8105–8.9024 µs | 9.7869–9.9089 µs | 1.56× |
| Mid-span, away from any join | 1.8940–1.9152 µs | 2.3897–2.4441 µs | 3.6894–3.7528 µs | 1.95× |
| Corner with angle targets | 6.4782–6.5817 µs | 9.4692–9.7626 µs | 10.191–10.279 µs | 1.57× |
| Corner with a wall excluded | 6.1960–6.3961 µs | 8.6773–8.7911 µs | 9.7941–9.9941 µs | 1.57× |

## Bounded work

Twenty-one times the walls costs between 1.56× and 1.95× the time. Work
proportional to document size would have cost about 21×. The query pays for the
candidates its broad phase returns, which is what R6.1.1's visible-region bound
and the prebuilt spatial indices are for.

The sub-span ownership path added for R6.1.3 self-exclusion is free within
measurement noise: at 840 walls the excluded query measures 9.7941–9.9941 µs
against 9.7869–9.9089 µs for the same query without exclusions, and the two
intervals overlap across almost their whole width.

## What this does not establish

No frame time, no p95 or p99 latency, and no whole-application performance. The
index build is excluded, and rebuilding it is paid once per document change
rather than per pointer move. Nothing here measures the FFI boundary or the
Flutter gesture path. Issue 0033 owns the interaction budgets this work will
have to respect, and no budget in `docs/BENCHMARK-INTERACTION.md` covers a
pointer move, so these numbers are compared against no threshold.

## Reproduce

From the repository root, at `4fae1ef`:

```sh
cargo bench --locked -p domowik-geometry --bench snap -- 'snap/query' \
  --sample-size 20 --warm-up-time 0.5 --measurement-time 1
```

Raw output and the captured host state are local to the measuring worktree and
are not committed. The `query-with-exclusion` case is new in this issue and has
no baseline, so Criterion's comparison against stored history is not evidence
for it.
