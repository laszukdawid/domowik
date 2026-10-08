# 0032: Geometry interaction benchmark corpus

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0013
- **Deferred-closure gates:** 0013 (scoped recomputation), 0036 (reference-host CI runner)

## Current disposition — 2026-10-08

Keep partial for missing interaction workloads and locality validation.

`crates/domowik-bench/` has generated corpora, measured scenarios and locality
tests. RoomRecord now exists, so missing record-churn measurements are
unstarted benchmark work, not blocked on an absent entity. Add
create/update/delete/cross-level record workloads and prove real recomputation
locality with 0013; keep timings separate from correctness and refresh
named-host evidence.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

Performance is a release criterion, not optional polish. Wall creation, editing,
splitting, joining, and deletion trigger geometry resolution and derived-data
recomputation whose cost must be measured before any interactive or CLI claim.
No named corpus, reference environment, metrics, or thresholds exist yet. This
issue defines them so behavior work (issues 0001–0003 and 0019) stays free of
invented numbers.

## Scope

Define and implement a deterministic benchmark corpus for core geometry
interaction workloads:

- Named reference hardware/environment for every published number (CPU class,
  memory, OS, build mode); results are meaningless without it.
- Representative generated plan corpora at multiple scales with a defined
  operation mix (create with incidental splits/joins, endpoint/joint drags,
  body drags, detachment, deletion, numeric edits), all clean-room generated
  from the normative rules in `../docs/BEHAVIOR-SPEC.md` section 3.
- Room-detection workloads generated from the normative rules in
  `../docs/BEHAVIOR-SPEC.md` section 5: per-level extraction of the bounded
  faces of the free-space complement induced by the occupied region,
  including holed/nested faces, non-convex faces, point-pinched arrangements
  resolved into separate simple-ring faces, unjoined positively-overlapping
  wall clusters, multi-room plans at multiple face counts, and room record
  churn (create/update/delete including cross-level `levelId` moves driving
  candidacy reclassification, exclusive-status changes, and tie-break
  outcomes). No numbers live here; thresholds are follow-up deliverables of
  this issue.
- Opening workloads clean-room generated from the normative
  `../docs/BEHAVIOR-SPEC.md` section 4 (2026-08-23, issue 0019): opening
  create/edit/delete, standalone and cascade deletion, hosted-wall reversal
  with the R4.3.1 field map, shorten/lengthen revalidation (R4.3.2), split
  remap and straddle rejection (R4.3.3), aperture subtraction recomputation
  (R4.2.2), and overlap-matrix validation (R4.1.4). Invalidation-locality
  checks for opening batches mirror the host's join-connected-component
  scope (BEHAVIOR-SPEC R4.3.7).
- Percentile metrics per operation class: p50/p95/p99 wall-clock latency.
- Peak memory and allocation limits per operation class and per corpus scale.
- Invalidation-locality checks verifying recomputation after an accepted batch
  is confined to the union of the pre-edit and post-edit join-connected
  components of the touched walls together with the faces reached through
  their transitive component closure: DISTINCT components are linked by
  spatial containment or by positive-area intersection/overlap of their
  occupied regions over both states (BEHAVIOR-SPEC R3.4.8/R5.1.9). Whole-level
  scope is acceptable
  only when the proven closure reaches every face of the level; unjustified
  whole-level or any document-wide wall/face invalidation must fail these
  checks.
- Explicit separation of benchmarks from correctness tests: tests must not use
  timing-dependent assertions; benchmark failures never substitute for failing
  conformance vectors.
- Numeric acceptance thresholds and failure thresholds recorded here once
  measured; they are follow-up deliverables of this issue, not inputs.

Excluded: rendering/GPU frame budgets (Phase 2/3 spike issues), sync/network
throughput, asset pipeline performance, and any change to normative
BEHAVIOR-SPEC semantics (sections 1–5; opening behavior is owned by issue
0019, room behavior by issue 0002, and both are consumed here as fixed input).

## Acceptance criteria

- [x] Reference hardware/environment is named and fixed for reported runs.
- [x] Corpus scales and operation mix are defined and reproducibly generated.
- [ ] Room-detection and record-churn workloads are included per the section 5
      scope above, with p50/p95/p99 latency reported per class and scale.
      **Partial.** Room DETECTION is measured at every scale and on named
      topology fixtures (holed, nested, non-convex, point-pinched, unjoined
      overlapping). Room RECORD churn measurements remain outstanding. The persisted
      RoomRecord entity and create/update/delete commands now exist; add
      cross-level moves and candidacy/status/tie-break workload coverage. The
      earlier absent-entity explanation is superseded, not a closure gate.
- [x] p50/p95/p99 latency is reported per operation class and scale.
- [x] Peak memory and allocation counts are bounded per operation class.
- [ ] Invalidation-locality checks pass on every measured operation class.
      **They do not, and the failure is the finding.** The checks are
      implemented and run; `walls::plan` is the only derived-geometry entry
      point and takes a whole level, so on the multi-cluster corpora every
      gesture exceeds the closure R3.4.8 permits. Scoped recomputation is an
      acceptance criterion of issue 0013. A canary test asserts the violation
      still exists and FAILS the day it is fixed.
- [x] Benchmarks are separated from correctness tests; no timing assertions in
      tests.
- [x] Measured thresholds are recorded as acceptance criteria with explicit
      failure thresholds. Not enforced in CI — they are calibrated to one named
      host and CI is not that host; a pinned reference runner is owed.
- [x] Opening operation classes from BEHAVIOR-SPEC section 4 are represented
      in the corpus, operation mix, and metrics. Split remap and straddle
      rejection (R4.3.3) are absent because they are unimplemented, not
      because the corpus lacks the shape.

## Progress — 2026-09-02 (corpus, metrics, locality checks)

Delivered, with results in
[../docs/BENCHMARK-INTERACTION.md](../docs/BENCHMARK-INTERACTION.md) and the
decision in ED-0098:

- `domowik_geometry::locality` — join-connected component partition, the
  R5.1.9 link relations (containment, positive-area intersection; point and
  edge contact deliberately DO NOT link), the transitive closure over the
  union of the pre- and post-edit states with wall ids carrying identity
  across the edit, and the faces the closure admits. With its benchmark, per
  AGENTS.md.
- `crates/domowik-bench` — nine corpora generated deterministically from the
  normative §3/§4 rules (six generated scales plus three topology fixtures),
  fifteen gesture classes, nearest-rank percentiles, a counting allocator, and
  a Markdown reporting binary. `--smoke` runs in `verify.sh`.
- The wall-edit batch algebra was extracted out of the file commands into
  `add_wall_operations`, `set_wall_operations` and `remove_wall_operations`,
  so the harness measures the PRODUCT's code rather than a copy of it. Five
  gesture classes still have no product command and are marked `harness` in
  every table.

Measured facts worth carrying forward:

- **The gesture is not the cost.** On `wings-l` fourteen of fifteen classes
  fall within 1.62–1.68 ms p50 against a 1.53 ms whole-level rebuild; on
  `campus-xl` all fifteen fall within 3.70–3.76 ms against 2.57 ms.
- **Recomputation is not confined.** `campus-xl` / `wall-create-free` is
  permitted 1 wall and 0 faces and recomputes 330 walls and 84 faces.
- **Locality is vacuous on an ordinary floor plan**, because a fully joined
  level is ONE component. It constrains multi-cluster levels only, which is
  why the corpus contains both families.

## Closure

- **Implementation/spec commits:** see the `feat(bench)` commit adding
  `crates/domowik-bench` and `domowik_geometry::locality`.
- **Follow-up issues:** scoped recomputation (issue 0013) and a pinned
  reference-host runner for threshold enforcement (issue 0036). Room-record
  churn measurements remain owned here; the entity exists.
