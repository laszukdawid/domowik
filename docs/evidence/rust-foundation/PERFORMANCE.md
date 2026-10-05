# Rust core — measured performance

**Date:** 2026-08-30
**Host:** Apple M4 Max, darwin/arm64, Rust 1.98.0, `--release`
(`lto = "fat"`, `codegen-units = 1`)
**Harness:** criterion, `crates/domowik-geometry/benches/predicates.rs`

## Results

| Benchmark | Time | Note |
|-----------|------|------|
| `orientation/filtered` | **1.137 ns/op** | the common, non-degenerate case |
| `orientation/exact-fallback` | **1.757 ns/op** | collinear input; filter cannot decide |
| `wall_polygon` | 2.553 ns/op | four corners from a centre line |

Criterion's reported intervals were [1.1333, 1.1413], [1.7542, 1.7596] and
[2.5499, 2.5563] ns respectively.

## Comparison with the frozen Go implementation — stated carefully

The honest comparison is **like for like**:

| Predicate | Go (frozen) | Rust | Ratio |
|-----------|-------------|------|-------|
| Orientation, filtered path | 5.479 ns/op, 0 allocs | 1.137 ns/op | ~4.8x |

That is the same predicate on the same class of input, and it is the only
directly comparable pair.

**The 12,134 ns/op figure that triggered this rewrite is NOT comparable to any
number above**, and conflating them would overstate the result by three orders
of magnitude. That measurement was `BenchmarkIntersectSegmentsRobust` — a
different, more complex predicate (segment intersection, not orientation) which
had **no adaptive filter at all** and called `big.Rat` unconditionally, at 426
allocations per call. Segment intersection is not yet ported, so it has no Rust
counterpart to compare against.

What the numbers do support:

1. The filtered orientation path is ~4.8x faster than its Go equivalent.
2. **The exact fallback costs 1.76 ns, not microseconds.** This is the load-
   bearing result. It uses error-free transformations — `two_product` built on
   `f64::mul_add`, and `two_sum` — so the determinant's sign is recovered
   exactly with no allocation and no arbitrary-precision arithmetic. The Go
   implementation's exact path allocated hundreds of times per call.
3. Both paths sit roughly four orders of magnitude inside the Phase 0 criterion
   of 10,000 ns/op that the Go predicates missed.

## Why the fallback being cheap matters more than the filter being fast

A filtered predicate is only as good as its worst case. If the fallback is
expensive, an adversarial or merely unlucky input — collinear walls, a grid,
anything axis-aligned, which in floor plans is *most* input — drops onto the
slow path and the average collapses. At 1.76 ns the fallback is within 55% of
the filter, so degenerate input costs almost nothing.

This is the substance behind the claim recorded in
`docs/decisions/0008-rust-core-implementation.md`: **the win is the adaptive
filter and the error-free transformations, not the language.** A Rust port that
reached for arbitrary-precision arithmetic on the fallback path would have
reproduced the Go implementation's problem faithfully.

## Reproducing

```sh
cargo bench -p domowik-geometry
```

`scripts/verify.sh` compiles the harness on every run (`cargo bench --no-run`)
so the measurement path cannot rot, but does not run the timings: they are
host-dependent and do not belong in a pass/fail gate (ED-0070).
