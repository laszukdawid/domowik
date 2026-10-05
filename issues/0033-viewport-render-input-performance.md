# 0033: Benchmark viewport, render, and input performance

- **Phase:** 2
- **Priority:** High
- **Status:** Open
- **Depends on:** 0012, 0020, 0024

## Problem

BEHAVIOR-SPEC section 1.7 fixes only qualitative performance scopes. The
interactive editor needs measurable criteria before Phase 2 claims any
performance property. This issue defines those criteria: named reference
devices, a representative document corpus, target resolutions, an interaction
query mix (pan, zoom, fit, grid redraw, hit-test, marquee), frame-time
percentiles p50/p95/p99, input latency, memory and allocation limits, and
culling/grid/fit thresholds.

## Scope

Belongs here: benchmark design and thresholds for viewport rendering, spatial
queries, grid/ruler drawing, zoom/fit operations, and input-to-photon latency
on the Phase 2 GUI; measurement harness requirements; publication of results.

Explicitly excluded: correctness semantics (owned by normative BEHAVIOR-SPEC
sections 1–4; opening behavior and its field ownership are owned by issue
0019 and consumed here as fixed input), core/CLI benchmarks (issues 0012,
0029), GPU surface feasibility itself (issue 0025), and any timing-dependent
assertions inside correctness tests — benchmarks and correctness tests remain
separate artifacts.

## Acceptance criteria

- [ ] Named reference devices (at least one desktop and one mobile class) are
      published with generation and memory class.
- [ ] A representative corpus is published with entity counts, level counts,
      asset-instance counts, and visible-region characteristics per document,
      including opening counts per wall so aperture-cut wall regions
      (BEHAVIOR-SPEC R4.2.2) are represented.
- [ ] Target resolutions are fixed per device class.
- [ ] The query mix is specified with operation weights for pan, zoom,
      fit, grid/ruler redraw, hit testing, and marquee selection.
- [ ] Frame-time thresholds are stated as p50/p95/p99 budgets per device
      class and resolution.
- [ ] Input latency is bounded from input event to next presented frame.
- [ ] Memory and allocation limits are stated per scenario, including steady
      state and worst interaction.
- [ ] Culling, grid-adaptation, and fit-operation thresholds are defined so
      behavior switches are observable and testable.
- [ ] Correctness tests are demonstrably separate from benchmarks; no
      correctness test asserts on timing.
- [ ] The query mix and frame budgets include opening-derived render inputs —
      door/window glyphs, swing arcs, and aperture-cut wall regions per
      BEHAVIOR-SPEC section 4 (semantics owned by issue 0019; budgets owned
      here).
- [ ] Results are reproducible from the published harness configuration.

## Closure

- **Implementation/spec commits:**
- **Follow-up issues:**
