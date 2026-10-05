# ADR 0002: Go core with CLI-first delivery

- **Status:** Superseded by [ADR 0008](0008-rust-core-implementation.md) on
  2026-08-30. It never reached Accepted: revision trigger 3 fired when two
  geometry predicates missed the 10,000 ns/op Phase 0 bound, and the owner
  decided the core moves to Rust. Retained in full as the record of the
  reasoning that led there — the Go implementation it describes is frozen at
  `_reference/go/`. The counter-evidence matters and is recorded in ADR 0008:
  the same package's adaptively filtered path ran at 5.479 ns/op with zero
  allocations, so this ADR's stack was not proven incapable of the bound.
- **Date:** 2026-08-22

## Context

The shared core needs document projection, operation handling, computational
geometry, spatial indexing, asset/package management, and headless 2D export.
The same behavior should serve the CLI and Flutter GUI.

Candidates include Go, Rust, C++, and Dart-only. Go aligns with existing team
experience and gives fast builds and straightforward CLI distribution. Its
geometry ecosystem and cross-platform Flutter embedding are weaker than
Rust's.

## Proposed decision

Implement the core and CLI in Go. Keep numerical geometry behind explicit
interfaces. Expose the core to Flutter through the smallest practical native
bridge rather than duplicating behavior in Dart.

## Validation gates

The proposal becomes Accepted only if:

1. Phase 0 numerical spikes demonstrate robust intersections, polygon
   operations, and planar room topology with documented coordinate bounds and
   acceptable performance.
2. Phase 1 benchmarks show document/geometry workloads meet defined targets on
   reference hardware without unacceptable GC/allocation behavior.
3. Before Phase 2 implementation, a spike demonstrates a supported Flutter
   bridge on Linux and Android with explicit ownership, errors, threading,
   cancellation, and ABI packaging. iOS remains unsupported until separately
   validated.

## Failure triggers

- Robust geometry requires maintaining an unreasonable custom numerical stack.
- Mobile Go embedding is unreliable or imposes unacceptable binary/runtime
  cost.
- The bridge forces duplicated domain logic in Dart.

If triggered, reassess Rust for the core or split only the numerical kernel
behind the existing interface. Compile time is one input, not the sole
criterion.

## Validation-gate progress record (2026-08-24, issue 0030)

Status of this ADR: **remains Proposed**; no gate below has fully passed.

### Gate 1 (Phase 0 geometry): NOT PASSED — OPEN

Assessed against the pre-registered
[Phase 0 performance criterion](../evidence/0030-numerical-model/PHASE0-PERFORMANCE-CRITERION.md)
and the [issue-0030 ratification record](../evidence/0030-numerical-model/RATIFICATION.md):

- Robust intersections and polygon operations have PARTIAL correctness
  support (0006 certificate corpora with zero certificate violations;
  0007's 0/12 post-normalization boolean agreement, which carries a
  disclosed analogous-normalizer independence caveat and is anchored by
  literal pinch fixtures and raw pre-normalization cycle pins).
- Planar room topology has NO conforming candidate path: the flat candidate
  face extractor is REJECTED (ED-0011 decision branch); a structured,
  provenance-preserving input contract does not yet exist.
- Performance acceptability is UNPROVEN: recorded 0006/0007 measurements do
  not satisfy the registered protocol (single-run means; one aggregate line
  with operand construction inside the timed loop) and are NOT-JUDGEABLE.
  The gate stays OPEN until a compliant rerun exists.
- Coordinate bounds remain documented proposals, unratified.

Gate 1 passes only when robust intersections, polygon operations, AND planar
room topology each have conformance evidence plus acceptable performance
under a registered criterion.

### Custom maintenance burden (explicit assessment): UNPROVEN SUSTAINABILITY

Accepting the current evidence as a production direction would commit the
project to maintaining, indefinitely and in-tree:

- a custom exact-arithmetic stack (scaled big.Int dyadic paths, big.Rat
  sweep/event arithmetic, outward-rational certificates), while the §4.3
  canonical multiquadratic layer remains ABSENT — irrational decisions
  currently rest on an evidence-grade big.Float(256) fallback that is not
  candidate-compliant;
- quadratic/event-growth risk in the big.Rat sweep, measured high
  allocation counts (~490k–550k allocs per corpus pass), and no per-case
  memory cap or per-invocation escalation budget wired into any runtime;
- separate oracle/reference machinery (BooleanUnion, OracleFaces) that must
  evolve in lockstep with candidates to preserve independence;
- quantized-strategy complexity (B-1q) that would ADD nonconforming
  pinned-grid machinery unless runtime certify-or-escalate exists first;
- duplicated normalization logic whose structural analogy between engines
  currently weakens the independence of correctness evidence.

Conclusion: the evidenced slice stays within "adaptive predicates +
classified constructions + one boolean strategy", but overall maintenance
burden and performance suitability are UNPROVEN. This is recorded as an
active risk against gate 1 feeding the failure-trigger review below; it does
not by itself trigger stack reassessment while the open gates remain
plausibly closable under NUMERICAL-MODEL §11's trigger framework.

Gates 2 and 3 remain untouched and unassessed.

## Revision-trigger 3 assessment (2026-08-24, issue 0030) — RECOMMENDATION ONLY

The Phase 0 performance criterion FAILED for the recorded v3 scope on the
A3 robust threshold, and the NUMERICAL-MODEL §11 trigger-3 assessment
([TRIGGER3-DISPOSITION.md](../evidence/0030-numerical-model/TRIGGER3-DISPOSITION.md))
found no caching/locality strategy per R5.1.9 plausibly closes the gap:
the judged statistic is a per-operation median over cold rotating-corpus
inputs, which invocation-level locality cannot reduce, and even a maximal
caching hypothesis leaves `BenchmarkOffsetCertificateAndVertex` above the
bound while `BenchmarkIntersectSegmentsRobust` offers no cacheable
redundancy at all. Revision trigger 3 therefore FIRES.

RECOMMENDATION to the owner: reassess the Go/float core per this ADR's
failure triggers — either stack reassessment (core language or numerical
kernel split behind the existing interface) or a revised candidate model
judged under a new pre-registered criterion with protocol-compliant reruns.
This ADR's Status remains **Proposed**; this record makes NO stack decision,
passes no gate, and changes no failure-trigger text. Gate 1 remains NOT
PASSED/OPEN and all other recorded states are unchanged.

## Validation-gate progress record (2026-08-25, issue 0024 spike)

Status of this ADR: **remains Proposed**; gate 3 remains OPEN.

### Gate 3 (Flutter bridge): NOT PASSED — Linux slice executed, Android deferred

The issue-0024 spike ([spikes/0024-flutter-go-bridge](../../spikes/0024-flutter-go-bridge/README.md))
demonstrated a minimal native bridge on **Linux amd64 only**, in a nested
spike module outside the main build graph:

- Flutter 3.47.1 desktop app + tests load an ABI-appropriate Go c-shared
  library via dart:ffi `DynamicLibrary.open` and drive a stateful
  internal/document Session op through a guarded C ABI (opaque int64
  handles; no Go pointers cross the boundary; malloc'd, registry-tracked,
  caller-owned response buffers with refused double/foreign frees).
- Panic containment on every export, sticky per-handle error retrieval,
  callback delivery with re-entry, per-handle serialization with
  cross-handle parallelism, prompt cancellation, and buffer/handle leak
  checks are all covered by three independent harnesses: Go (`-race`), a
  plain-C dlopen harness on pthreads, and Dart FFI (tests plus a real
  release-mode app run that exited 0).
- Measurements recorded evidence-only: `.so` ≈5.0 MB unstripped / ≈3.5 MB
  stripped; cold/warm cgo builds ≈2.1 s/≈0.19 s; nullary export ≈42 ns;
  single-op round-trip ≈141 µs at low history degrading superlinearly with
  session history (finding shared with gate 2 concerns); 20-op batching
  ≈9.4 µs/op-equivalent. Environment fully disclosed in the spike README.

Gate 3 does NOT pass because:

- **Android was assessed-and-deferred**: this host has no Android SDK, NDK,
  Gradle, or emulator/device; nothing Android-related was executed or
  simulated. The gap is recorded verbatim rather than bridged by
  cross-build-only claims.
- **iOS remains unsupported** per this ADR's own text.
- Spike findings (core history-depth cost growth, absent runtime shutdown
  story for c-shared, seam-level validation semantics) are unresolved and
  feed gates 1–2 risk records above.

No normative section of this ADR changes; no gate status advances beyond
this record.
