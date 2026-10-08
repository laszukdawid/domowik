# 0012: Implement spatial index and hit testing

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0011, 0013

## Current disposition — 2026-10-08

Keep partial for numerical/query conformance and representative performance
coverage. Issue 0059 delivered caller-supplied per-instance resolution to the
frontend point-hit and marquee queries, with native-backed PH1/PH2 selection,
deletion and read-only inspection tests. The owner approved declared width ×
depth footprints with instance scale/rotation and axis-aligned bounds; the
geometry helper now constructs that branch. The fallback still follows ED-0106.

Derived-distance and exact-real constructed-geometry guarantees, locality and
certified fast/fallback performance evidence remain open. The new 42 ns
construction and 1.9 μs warm native query samples do not discharge those gates
or 0033's whole-app budgets. See
[the integration evidence](../docs/evidence/0059-marquee/2026-10-08.md).

The disposition below is historical and superseded by this update.

## Historical disposition — 2026-10-05

Keep partial for numerical/query conformance and end-to-end placeholder
inputs.

`crates/domowik-geometry/src/hit.rs` already defines PlaceholderBox, explicit
placeholder snapshots, PH1 hit/marquee tests and declared-extent refusal.
Selection cycling is delivered under 0049. Remaining work is
derived-distance/constructed-geometry guarantees, representative benchmarks
and caller-supplied placeholder resolution/extent integration (0026/0059), not
implementing the class again.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

Maintain a spatial index over entity/derived bounds, plus culling, proximity,
and hit-testing queries. Distinguish committed updates from transient drag
preview queries.

## Acceptance criteria

- [ ] Property tests compare index results with naive queries under random
      operation streams.
- [ ] Hit priority and tolerance match the behavior spec.
- [ ] Benchmarks define dataset distribution, reference hardware, Go version,
      query mix, percentile, allocations, and explicit budgets.
- [ ] Committed updates, visible-region culling, point hits, and preview queries
      are benchmarked separately.

## Progress: 2026-09-07 (exact endpoint distances)

Four integration tests failed against the previous implementation: a rounded radius tie, rounded ranking
tie, squared underflow/overflow, and invalid query admission. Local failure
log: `/private/tmp/domowik-0012-endpoint-red.log`. The repair introduces shared
filtered/exact point predicates as a prerequisite for issue 0058. All four
integration tests now pass. An outward-rounded interval filter handles ordinary
cases; existing dyadic arithmetic resolves overlapping intervals. Independent
integer oracles, fractional-origin cases, subnormals, overflow, signed zero,
invalid inputs and benchmark-path assertions cover the primitives.

Endpoint/join queries bypass the old rounded radial prefilter, and compare
stored point distances exactly. Identical endpoint coordinates resolve equality
without allocating dyadics. Region projection and distance exactness remain
unfinished; neither this change nor its benchmarks establish complete snapping.

On the Apple M4 Max host (16 cores, 48 GiB, macOS 26.6.2, rustc 1.98.0),
indexed corner-hit median confidence intervals were
365.30–370.95 ns (40 walls), 307.73–311.51 ns (220), and 320.12–329.56 ns (840).
These measure the complete indexed core hit query, not Flutter frame time or
p95 latency. Command: `cargo bench --locked -p domowik-geometry --bench spatial
-- 'spatial/point-hit/indexed' --sample-size 20 --warm-up-time 0.5
--measurement-time 1 --verbose`. Local raw output:
`/private/tmp/domowik-0012-endpoint-indexed-final-bench.log`.
Primitive measurements are in `/private/tmp/domowik-0012-distance-bench.log`;
ordinary comparison and radius checks measured about 13 ns and 8 ns respectively.
Full repository verification at commit `4dd59ff` exited 0 with
`=== VERIFICATION PASSED === all stages` and no skipped stages. It included
196 geometry tests, 72 native unit tests, 171 Flutter tests, benchmark
compilation and macOS packaging. Local log:
`/private/tmp/domowik-0012-distance-final-verify.log`.

## Progress: 2026-09-07 (large finite marquee corners)

`hit::marquee` under-selected silently for a query large enough to overflow
the narrow-phase crossing parameters. R7.3.2 makes finiteness the only
validity condition on marquee corners, so such a query is valid input;
`arrangement::intersection_param` computed `t`/`u` in binary64, an overflowed
parameter is neither in `[0,1]` nor comparable, and the resulting "no
crossing" propagated as "no intersection". A one-millimetre strip through the
middle of a wall with a `±1e308` extent therefore selected nothing. Confirmed
by diagnostic before the change (`s2 = inf`, `denom = inf`, `t = u = NaN`,
`intersection_param = None`) and reproduced by three of the regressions below,
which fail without the fix.

The witness is a false negative. An enclosing huge query already succeeds
through vertex containment, so that case alone cannot test the repair.

Repaired by clipping each candidate's query to `ENVELOPE ∪ bounds` before the
narrow phase, where `bounds` is the bounding box actually computed for that
candidate's polygon. `S ⊆ bounds ⊆ limit` gives
`S ∩ Q = S ∩ (limit ∩ Q)`: clipping preserves the geometric intersection and
reduces the crossing test's operand magnitudes. Large finite corners remain
valid input. This identity does not make the rounded intersection algorithm
exact or establish that every previous computed answer is preserved.

Scope of the guarantee, stated narrowly on purpose:

- Containment is exact and assumption-free, because it is taken from the
  computed AABB rather than from an error analysis of `wall_polygon`. No
  bound is claimed on that construction: `thickness_mm / 2.0` underflows for a
  subnormal thickness, and `f64::hypot` carries no correctly-rounded guarantee
  across targets.
- Freedom from overflow holds when a candidate's computed bounds lie within
  `DERIVED_COORD_BOUND_MM`. Oversized computed geometry remains eligible
  through the widened limit, but gets no overflow guarantee: its coordinates
  can overflow the crossing test independently of the query.
- **Still not delivered:** exactness. The binary64 crossing parameters, their
  underflow behaviour and the rounded polygon construction are unchanged, and
  remain this issue's obligation. Clipping removes an overflow, nothing more.

Regressions in `hit.rs`: huge vertical strip and its transpose through a
vertical wall (both RED before the fix), corner-order invariance (RED before),
huge band that misses everything, huge enclosing query in key order,
tangent-contact selection with a bounded control that misses, order unchanged
across huge/bounded/ordinary queries and across document order, huge
degenerate point, `clip` producing a zero-width box directly, a wall beyond
the envelope still selected, and a line rectangle keeping rectangle semantics
against point-hit semantics at the same corner.

R7.3.2's own "zero-area" wording is left alone: the degenerate branch still
requires BOTH extents zero, and the classification still reads the original
rectangle, ahead of clipping, which matters because clipping can create a
zero-width box. A line rectangle is mathematically zero-area, but "at its
corner" is single-valued only when the two corners coincide, so which entity
such a drag selects is a normative question and not settled here.

`benches/spatial.rs` gains a `-band` pair per size that actually exercises the
repaired crossing path, alongside the enclosing pair, with selected counts in
the benchmark names and each pair's ordered answers asserted equal outside the
timed section. The pairs measure sensitivity to query magnitude at equal work;
they do not isolate the clip's cost, because every variant now clips. That
would need a run from before the change.

The [marquee checkpoint](../docs/evidence/0059-marquee/PROGRESS.md) records
the named-host measurements, correctness results and outstanding gaps.

## Progress — 2026-08-31 (Rust core)

Delivered in `crates/domowik-geometry`:

- `spatial` — `Aabb` and a uniform-grid `SpatialIndex<T>` with rectangle
  (culling) and radius (proximity) queries. The grid is chosen over an R-tree
  because floor plans are spatially even and change shape on every edit; the
  choice, and its failure mode, are argued in the module's own documentation.
- `hit` — R7.1.3 hit classes and R6.3.1's total ranking; R7.3 marquee
  selection; `HitIndex` for reuse across a gesture.

### Acceptance criteria

- [x] Property tests compare index results with naive queries under random
      operation streams. Deterministic sweeps rather than a randomised
      generator: `a_query_agrees_with_a_naive_scan` and
      `a_proximity_query_agrees_with_a_naive_scan` cover a grid of query
      rectangles and radii, and `hit_test_agrees_with_a_naive_scan_over_a_sweep_of_points`
      does the same for hit testing. A sweep is reproducible where a seeded
      generator is only reproducible if nothing about it ever changes.
- [ ] Hit priority and tolerance match the behavior spec. Class precedence,
      join-node deduplication and active-level eligibility are implemented.
      Issue 0047 supplies the previously missing canonical region tuple.
      Distances and closest-point projections still use binary64 arithmetic,
      not the exact-real comparisons required by the normative specification;
      near-boundary membership and nearly tied distances need filtered/exact
      predicates and adversarial tests before this criterion can be checked.
- [x] Committed updates, visible-region culling, point hits, and preview
      queries are benchmarked separately. `benches/spatial.rs`, one group each.
- [ ] Benchmarks define dataset distribution, reference hardware, Go version,
      query mix, percentile, allocations, and explicit budgets. Distribution,
      hardware, toolchain version, query mix and budgets are in
      `docs/evidence/spatial-index/PERFORMANCE.md`. **Percentiles and
      allocation counts are NOT** — criterion reports a confidence interval on
      the mean, and Rust has no allocation counter in its standard library.
      Adding a counting global allocator to satisfy a criterion written against
      Go's `testing.B` would be the wrong trade; issue 0032 owns percentiles
      and memory bounds for the interaction corpus.

### Not delivered

- **Exact distance/projection comparisons.** The current f64 implementation
  avoids square roots but that does not make a squared-distance comparison
  exact. Issue 0047 corrects one missing ordering component, not this numerical
  guarantee. Any replacement requires proven floating filters, exact fallback,
  independent-oracle tests and benchmarks under the project geometry policy.
- **`placeholder-box` hits** (R7.1.3 class 4). **Delivered 2026-09-12** on
  `finalize/0012-placeholder`, as a bounded slice of this issue assigned to
  the issue-0059 worker; the rest of this issue's gaps are untouched.
  `HitClass::PlaceholderBox` ranks last and carries R7.1.3's own key for the
  class, exact min corner then stable instance ID, with no distance term.
  `placeholder_box` builds the §9.6 fallback per ED-0106. `hit_test_with` and
  `marquee_with` accept the candidates; the existing `hit_test` and `marquee`
  delegate with an empty snapshot, so no caller changed.
  Candidacy is NOT decided here: the caller passes a per-instance resolution
  snapshot, which keeps `domowik-geometry` free of any asset dependency and
  refuses the "every v0.1 instance is degraded" shortcut. The
  descriptor-dimensions branch of §9.6 stays with issue 0026, along with the
  resolution itself. The seven §9.6 inputs still missing, with owners, are
  inventoried in
  [unresolved R9.6 inputs](../docs/evidence/0012-placeholder/UNRESOLVED-R9.6.md).
- **Cycling state.** R7.2.2's repeated-click advance and R7.2.3's eleven reset
  conditions are interaction state, not geometry. The ranking this delivers is
  what cycling walks, and it is total so the walk is deterministic; the walk
  itself belongs with a front end.
- **Transient drag-preview INVALIDATION.** `HitIndex` distinguishes the two
  query kinds, but nothing yet decides when a preview index is stale. That is
  R5.1.9's scoped recomputation, still open in issue 0013.

Status is In Progress (partial scope) rather than Done: numerical guarantees,
benchmark evidence, placeholder hits and the cycling layer remain incomplete.
