# 0007: Spike clipping and planar room topology in Go

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0022
- **Deferred-closure gate:** 0030 (strategy selection and structured face-input
  disposition under ED-0011)

## Problem

Validate the chosen numerical model for union, intersection, difference,
holes/multipolygons, wall outlines/opening cuts, and the planar arrangement or
face traversal needed for room detection. Do not assume clipping alone solves
room topology.

## Acceptance criteria

- [x] Coordinate quantization/snap rounding, range, overflow, and maximum error
      are measured if integer clipping is used.
- [x] Fill/winding rules, self-intersection policy, holes, deterministic output
      normalization, and invalid input behavior are defined.
- [x] Corpus covers thick-wall L/T/X joins, unequal thickness, shared edges,
      touching vertices, slivers, opening difference, nested faces, and large
      coordinate translations.
- [x] Separate evidence evaluates planar face extraction and exterior/hole
      classification.
- [ ] Topology-mismatch rate versus the independent exact-decision oracle
      (NUMERICAL-MODEL §6) is measured and reported separately from
      coordinate-error budgets, for the selected strategy.
- [x] Wall-clock and allocation measurements of the boolean/planar-face
      pipeline are RECORDED on the same named corpus and reference
      environment as the correctness runs. Measurements are evidence only
      for issue 0030's Phase 0 performance judgment (0030 defines the
      criterion before judging, together with 0006 predicate benchmarks and
      H-6 fallback/escalation data per `../docs/NUMERICAL-MODEL.md` §11);
      this issue sets and applies NO pass/fail threshold.
- [x] Provenance records exact publications and implementation licenses; no
      abstract algorithm is described as having a code license.

## Integrated evidence handoff — 2026-08-24

Branch head `c72e424` was integrated into the main repository by merge commit
`bf17a3b`. Normal, race, shuffle, diff, and full repository verification passed
before handoff and `./scripts/verify.sh` passes on the combined integration
tree.

## Partial-scope evidence handoff — 2026-08-24

Substantive commits `f0dad76` and `d354eb6`, followed by generator-only SBOM
commit `0a0bbcb`, were integrated into the main repository by merge commit
`3cc5f8e`; integration SBOM commit `4356ef0` follows it. The executor and
adversarial reviewer independently passed focused/raw/corpus tests, normal,
race, and test-order-shuffle runs. `./scripts/verify.sh` also exits 0 on the
clean merged main tree.

The issue remains **In Progress (partial scope)**. Boolean/oracle evidence now
records generic pinch-contact normalization on both engines and
post-normalization agreement on 0/12 corpus cases (all 12, including near-B),
122 matched vertices, and zero measured coordinate error. The agreement is
qualified because both engines apply analogous canonicalization; literal
normative pinch fixtures anchor semantics. Input-permutation, raw-cycle,
simple-ring, race, and shuffle coverage is committed.

Under ED-0011, `CandFaces` remains characterization-only: the flat input loses
operand, direction, role, and multiplicity provenance; the seed-1 historical
probe changes pre-`cChain` counts from 12 fragments / 12 directed pieces / 2
cycles to 16 / 10 / 1 under permutation and endpoint reversal. All face
conformance, determinism, and performance claims are withdrawn. `OracleFaces`
remains reference machinery only. B-1q remains nonconforming and
selection-blocking without runtime certify-or-escalate behavior.

Issue 0030 owns the deferred closure: it must define the Phase 0 performance
criterion before judging, select or reject the numerical strategy, and either
ratify a provenance-preserving structured face input or reject candidate face
extraction. Until then, the selected-strategy topology-mismatch and combined
boolean/planar-face performance acceptance criteria above remain unchecked.

## Measurement round integrated 2026-08-31 (ED-0069)

Branch `issue/0007-pipeline-measurement` merged by `f82fafd` with SBOM refresh
`d023478`; full main `./scripts/verify.sh` exits 0 with a clean tree. Evidence
in TRACKED content at `docs/evidence/0007-measurements-macOS-arm64.md`.

**The criterion is satisfied and the reason it was dispatchable is recorded.**
The wall-clock and allocation criterion requires measurements "on the same
named corpus and reference environment as the correctness runs". That was
unanswerable while the measurement-authority question stayed open, because the
historical correctness runs were on the retired Linux/amd64 host. ED-0070
amendment (a) rules the authority per-record and per-quantity, so the clause is
satisfied by re-running BOTH correctness and performance on this host and
recording both with this host's environment — which is what was done. The
committed historical records were not touched (ED-0031 forbids it).

**This issue sets and applies NO pass/fail threshold.** The measurements are
evidence for issue 0030's Phase 0 judgment, which defines the criterion before
judging.

### The H-6 escalation data, and why it reframes the ADR-0002 evidence

`Orientation` implements an adaptive filter (`orientFast`, `orient.go:97`) that
escalates to exact `math/big` arithmetic only when inconclusive. Measured
escalation:

- **0% on non-adversarial corpora** — the filter decides every case, and no
  `big.Rat` is allocated at all.
- **90–100% on adversarial corpora** — near-total escalation, which is what a
  robustness corpus of near-degenerate inputs is BUILT to produce.

**`IntersectSegments` and `OffsetVertex`: H-6 NOT APPLICABLE.** They have no
filter. `IntersectSegments` (`segment.go:113`) calls `crossOfDiffs`
(`exact.go:76-85`), which builds `big.Rat` unconditionally on EVERY call
irrespective of input conditioning. `orientFast` exists in the same package and
`segment.go` references it ZERO times. So their 426 and 538 allocs/op are not
escalation rates — they are the unconditional cost of an exact-only path.

The executor discovered this by asking rather than instrumenting production
code to manufacture an escalation figure that would have looked like evidence
and meant nothing for these predicates. Recorded as ED-0073 amendment (a).

**What this does and does not mean.** It does NOT show the 10,000 ns/op bound
is reachable, does not cost a filter for these predicates, and does not resolve
the ADR-0002 stack question — all of which belong to issue 0030's judgment and
to the owner, and the pair was explicitly instructed not to draw the conclusion.
What it establishes is narrower and still decision-relevant: the `math/big`
cost appears either on deliberately adversarial inputs or on a predicate that
does not use the filter technique the package already contains.

Status remains `In Progress (partial scope)`: the topology-mismatch-rate
criterion is open, and the ED-0011 deferred-closure gate on issue 0030 is
unchanged.

## Rust delivery — 2026-08-31

The spike's subject is now IMPLEMENTED rather than validated, which closes its
question from the other side: `crates/domowik-geometry` carries polygon union
with holes and multipolygons (`boolean::union_and_complement`), wall outlines
and join patches (`walls`), and planar face traversal for room detection.

- **Fill/winding, self-intersection, holes, normalization, invalid input** —
  all defined and tested. Exterior rings counter-clockwise, hole rings
  clockwise, R3.2.4 canonical normalization in full, degenerate rings dropped.
- **The corpus** covers L/T/X joins at equal and unequal thickness, shared
  edges, touching vertices, point-pinch cycles, nested faces, islands inside
  courtyards, and collinear runs that wrap a ring's end.
- **Clipping alone does NOT solve room topology**, exactly as this issue
  warned. Rooms are the bounded components of the COMPLEMENT of the occupied
  region, and a void enclosed by several components that touch only at points
  belongs to none of them — found by adversarial review, and now traced from
  the same boundary as the region in the opposite direction.
- **Floating point, not integer clipping.** The quantization criterion is met
  by not being applicable: coordinates are `f64` throughout, with an exact
  orientation predicate and a stated 1 µm weld tolerance (ED-0092). The one
  place a tolerance is scale-dependent — cut-parameter collapse — was made
  spatial after review showed a fixed parameter epsilon loses real features on
  a long edge.

Measurements in `docs/evidence/wall-joins/PERFORMANCE.md`. The ED-0011 strategy
selection this issue gates on remains open under issue 0030.

## Closure — 2026-09-12

**Done.** One remaining criterion is checked from evidence this issue already
recorded; the other names scope that no longer exists. Both are stated
separately below so neither is mistaken for the other.

- **Wall-clock and allocation measurements — CHECKED from recorded evidence.**
  The box lagged the issue's own text. The "Measurement round integrated
  2026-08-31 (ED-0069)" section above states the criterion satisfied, with
  correctness and performance re-run together on one host under ED-0070
  amendment (a) and recorded in tracked content at
  `docs/evidence/0007-measurements-macOS-arm64.md`. Checking the box records
  what was already delivered; it applies no threshold, which this criterion
  explicitly forbids.
- **Topology-mismatch rate versus the §6 oracle — SUPERSEDED SCOPE, left
  unchecked.** The criterion is measured "for the selected strategy" against
  "the independent exact-decision oracle (NUMERICAL-MODEL §6)". ED-0100
  superseded §6 along with §§4.2–4.5, 10 and 11: the oracle and the B-1/B-2
  strategy selection were candidate designs for a Go core that was never built.
  There is no selected strategy to measure and no §6 oracle to measure against.
  The box stays unchecked and the criterion stays in the file unedited, because
  it is an accurate record of what was asked for while the question was live.
- **What replaced it, so the obligation is not simply gone.** The spike's
  subject is implemented rather than validated. `domowik-geometry` carries
  union with holes and multipolygons, wall outlines and join patches, and
  planar face traversal for room detection, with the boolean decisions made by
  an exact predicate rather than compared against an oracle. Correctness,
  determinism and performance for the shipped implementation are summarized in
  NUMERICAL-MODEL §13; provenance in §14.
- **The ED-0011 deferred-closure gate is discharged** by ED-0103, which ratifies
  NUMERICAL-MODEL §12. ED-0012's rejection of the flat `CandFaces` input stands
  and is not revisited; the shipped face traversal is a different construction.
- **Residual, named rather than closed over.** Boundary-edge ownership in the
  merged occupied boundary is decided by ED-0103 Part 2 and implemented under
  issue 0058, which holds exclusive ownership of `boolean.rs` and `walls.rs`
  for that work.
