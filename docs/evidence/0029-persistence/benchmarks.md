# Issue 0029 persistence benchmarks (evidence-only)

Status: **Evidence** under the Performance Policy. These numbers are NOT
correctness gates, carry no normative weight, and are separated from the
test suite (`go test -run '^$' -bench` only). Recorded 2026-08-25 for
issue 0029's acceptance criterion on package size, load/save time, peak
memory, and streaming behavior.

## Environment

- CPU: AMD Ryzen 9 7950X 16-Core Processor (-32 GOMAXPROCS)
- OS: Linux (local ext4 workspace)
- Go: `GOTOOLCHAIN=local`, go1.25 toolchain per go.mod directive
- Command: `go test ./internal/persist/ -run '^$' -bench Benchmark -benchtime 20x`
- Inputs: golden corpus packages (`thin-valid.domo`, `portable-valid.domo`)
  plus deterministic in-test syntheses from golden entity fixtures
  (100 and 1000 entities); a 512 KiB deflate-compressed preview entry
  streamed through the full guard pipeline.

## Results

```
BenchmarkSaveThin100Entities-32        	      20	   1535841 ns/op	         1.822 cumulative-alloc-MB/op	      1585 package-bytes
BenchmarkLoadGoldenPortable-32         	      20	    302278 ns/op	     67992 cumulative-alloc-B/op
BenchmarkRoundTripThin1kEntities-32    	      20	   11686297 ns/op	         7.791 cumulative-alloc-MB/op	      9830 package-bytes
BenchmarkStreamingLargeEntry-32        	      20	    651997 ns/op
```

## Observations

- Package size: a 100-entity thin package serializes to ~1.6 KiB; a
  1000-entity round trip to ~9.8 KiB — consistent with the unfurnished /
  lightly furnished size targets discussed in ASSET-STRATEGY §10.9
  (non-normative context).
- Save cost is dominated by S2 full reader-equivalent candidate
  validation (a complete reopen through limits + digests + projection),
  which is the §9.7-mandated price for old-or-new readability.
- Streaming: a ~512 KiB logical preview entry at ~967:1 compression —
  deliberately just under row L15 — passes the incremental ratio guard;
  an 8 MiB zero run (>1000:1) is rejected by the same guard mid-stream,
  demonstrating enforcement before full allocation.
- Memory figures are cumulative allocated bytes per op
  (`runtime.MemStats.TotalAlloc` deltas), an upper-bound proxy for peak
  live heap; they are disclosed as such rather than presented as RSS.

## Coverage notes

- Rows L1/L2 (GiB-scale cumulative caps) are enforced by counters whose
  boundary semantics are carried by the declarative recipes of
  EXPECTATIONS.md; materializing them faithfully exceeds any reasonable
  test budget and is not attempted here.
- Writer thresholds (L31) and artifact policy (L30) are named provisional
  parameters in `internal/persist/limits.go`; no benchmark asserts them.
