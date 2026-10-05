# 0022: Decide the numerical geometry model

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0001, 0002

## Problem

Select a coherent coordinate and topology model before writing isolated
predicate/clipping ports. Compare float inputs with adaptive predicates,
integer quantization/snap rounding, and hybrid approaches.

## Acceptance criteria

- [x] Required operations are enumerated: segment intersection/splitting,
      offsets, union/intersection/difference, rings/holes, planar faces, point
      containment, and measurement.
      (`../docs/NUMERICAL-MODEL.md` §2 plus the §2.1 rule-to-operation-to-
      fixture/owner matrix covering R3/R4/R5 incl. R3.4.8, R4.1–R4.3,
      R5.1.9.)
- [x] Document coordinate bounds, precision, scale, overflow, topology
      tolerance, and deterministic normalization are proposed.
      (`../docs/NUMERICAL-MODEL.md` §3–§9: bounds B/D with overflow
      derivations, opening-field coverage (R4.0.4/R4.0.5), gated fast-path
      error model with full derivation, exact discrete conformance vs
      budgeted continuous outputs, canonical normalization under exact
      tests, mm²-consistent R5.3.2 guard; all Candidate pending
      0006/0007/0030.)
- [x] Candidate algorithms/libraries have exact versions, licenses, platform
      support, and provenance RECORDED — with reliance honestly incomplete:
      the `../docs/NUMERICAL-MODEL.md` §10 ledger pins artifacts, versions,
      licenses, and live-checked URLs under four reliance statuses and a
      ledger-before-reliance rule. PINNED-PENDING-STUDY entries (P-1a/b/c)
      are NOT yet relied upon: study logs, adaptation types, and test-vector
      provenance complete at implementation reliance in 0006/0007;
      CITE-VERIFIED literature is unconsulted; P-2 remains quarantined until
      its edition/section pinning at first consultation; B-2 adoption would
      additionally require SBOM/NOTICE/transitive/maintenance records at
      0030. No entry claims completed reliance evidence today.
- [x] Decision identifies which questions 0006 and 0007 must prove and what
      failure triggers revision of ADR 0002.
      (`../docs/NUMERICAL-MODEL.md` §11: delegated obligations, gate
      semantics as a PROPOSED delegation preserving 0006/0007 evidence-only
      roles and 0030's criterion-setting authority pending ADR update, and
      operationalized ADR-0002 triggers.)

## Remaining gates before closure

This issue closes only when ALL of the following hold:

1. **Completed 2026-08-23:** reviewer approval of the candidate-model diff and
   orchestrator integration in main commit `6a9794a`.
2. **Partial evidence consumed 2026-08-24; NOT COMPLETE.** Issue 0006 spike
   evidence was integrated in main lineage `ca460ea` (substantive `43fbd87`,
   generated-only `8b2e137`) plus SBOM refresh `ccec6f8`, and consolidated by
   substantive ledger-metadata commit `ca53786` followed by generated-only
   refresh `49305bc`; the reconciled handoff was integrated by `07c81dd` and
   refreshed for the integration tree by `672c5ae`. P-1a/P-1b provenance
   verdicts are recorded for issue-0006 roles only; P-1c remains explicitly
   UNCONSULTED / NON-RELIED. The P-3 published-vector subcriterion holds only
   while both transcribed triples and their provenance remain in the committed
   tree. Section 4.1/4.2/4.4 gate/certificate results are PARTIAL,
   EVIDENCE-ONLY, and do not ratify gate policy. Section 4.3 canonical
   multiquadratic evaluation and H-0 cost remain OPEN under issue 0030. This
   item makes no threshold/performance judgment, representation choice,
   bounds decision, Android runtime claim, or ratification.
3. Issue 0007 spike evidence: boolean-strategy selection with measured
   coordinate-error AND topology-mismatch rates vs the independent oracle;
   planar-face-extraction evidence.
4. Issue 0030 ratification: measurable Phase 0 performance criterion defined
   BEFORE judging recorded measurements; ratify/revise/reject decision;
   entity numeric bounds and format numeric limits finalized afterward by
   their owning issues.
5. Phase 0 performance-evidence chain executed (resolved by orchestrator
   ruling, recorded in `../docs/NUMERICAL-MODEL.md` §11): 0007 records
   wall-clock/allocation measurements of its boolean/planar-face pipeline on
   the same named corpus/reference environment (evidence only, no threshold);
   0030 defines the Phase 0 performance criterion before judging that
   evidence together with 0006 predicate benchmarks and H-6
   fallback/escalation data. ADR 0002's acceptable-Phase-0-performance
   requirement is judged by no other producer or issue.

Until then the model document stands as Candidate only.

## Reviewed candidate handoff

- **Main repository commit:** `6a9794a`
  (`docs(issue-0022): specify numerical geometry model`).
- **Review:** DeepSeek V4 Flash independently returned PRE-MERGE WORK APPROVED
  after the third full-diff review. The final residual ratification gates were
  then closed: overflow magnitude/notation, strict-fit and error-envelope
  precision, provenance adaptation type, issue 0007 topology-mismatch evidence,
  and issue 0030's define-before-judge criterion. Both repository diffs passed
  `git diff --check` before integration.
- **Disposition:** this is a reviewed Candidate handoff, not issue closure.

## Continuation audit — 2026-08-24

Two Ox Alpha executor sessions repeatedly ended with provider `network_error`
and produced no branch diff or valid executor-authored handoff. The paired
reviewer independently audited the integrated evidence, sent requirements to
the active 0006 and 0007 pairs, and rejected closure or ratification. The
failed pair was purged with the worktree preserved clean; resume 0022 after
those issues produce new durable evidence.

The minimal gate ledger is:

- 0006 S1+S3 remains evidence-only; the section 4.3/H-0 canonical
  multiquadratic contract is not implemented and its conservative gates are
  evidence rather than policy.
- 0006's literal published-adversarial-vector criterion remains unresolved
  unless eligible primary-publication vectors are found and provenanced.
- 0007's recorded 2/12 pinch mismatch cannot support a candidate verdict
  until oracle pinch normalization is repaired or characterized and the
  corpus is rerun.
- `CandFaces` has an observed input-order failure before chaining; canonical
  fragment aggregation and traversal need arbitrary segment-permutation and
  endpoint-reversal tests.
- B-1q remains nonconforming without runtime certify-or-escalate behavior;
  B-1x has no proven generic topology repair.
- Issue 0030 must define performance criteria before judgment and owns final
  strategy ratification.
  Issues 0006 and 0007 must produce the recorded evidence and issue 0030 must
  define its criterion before judging and ratifying, revising, or rejecting the
  model. This issue remains `In Progress`; no acceptance criterion or downstream
  gate is represented as final ratification.

## Status corrected by ED-0096 — 2026-09-01

**Blocked.** All four acceptance criteria are met: the required operations are enumerated, the bounds and tolerances are proposed with derivations, and the candidate ledger records versions, licences and provenance. What remains is the owner's ratification, which issue 0030 carries — together with the evidence summary 0030 still owes. Three provisional constants name this gate in their own source per ED-0092, and one of them, `COORD_BOUND_MM`, differs from the candidate bound by a factor of a thousand (1e6 mm enforced against 1e9 mm proposed), so this is a format-observable open question and not a tuning knob.

## Unblocked and partly ratified — 2026-09-04 (ED-0100)

**No longer Blocked.** The owner delegated the decision (2026-09-04) and it was
taken: `coordBoundMm` = 10⁷ mm, `dimensionCapMm` = 10⁶ mm,
`derivedCoordBoundMm` = 1.1×10⁷ mm, topology quantum 10⁻³ mm. FORMAT-SPEC row
L14 moves from Deferred to Ratified and the ED-0092 provisional-constant gate
is closed — the constants no longer name this issue as their gate.

The bound ROSE from the 10⁶ mm previously enforced. The argument for the
smaller value turned out to measure the area routine rather than the domain:
once `signed_double_area` translates to a local origin, the worst-case area
error for a room-sized rectangle is 3.7×10⁻⁹ mm² and is identical at 10⁶, 10⁷
and 10⁹. ±1 km would have excluded campus and site plans for no numerical
benefit.

**Two correctness defects surfaced while deciding**, both in shipped code and
both now carrying regression tests: `orientation` returned Collinear for a
subnormal-scale triangle while documenting exactness for every input, and a
2 µm square at the coordinate bound had exactly zero computed area.

**Why this is not Done.** Five questions remain open and are listed in
NUMERICAL-MODEL §12: stored-coordinate quantization, one "same point" rule to
replace the three that disagree today, split-point persistence, the exact
area/half-even display contract, and the positive-area threshold that decides
R5.1.9 component linking. Issue 0030 carries them.

## Closure — 2026-09-12

**Done.** All four acceptance criteria were already met; what remained was the
gate list under "Remaining gates before closure", and every item in it is now
resolved or superseded.

1. **Reviewer approval and integration** — completed 2026-08-23, `6a9794a`.
2. **0006 spike evidence** — consumed. The §4.3/H-0 remainder it reserved is
   SUPERSEDED by ED-0100, which ruled §§4.2–4.5, 6, 10 and 11 a record of
   alternatives rather than pending decisions. Issue 0006 is Done.
3. **0007 spike evidence** — the boolean-strategy selection and the
   topology-mismatch rate against the §6 oracle name scope that ED-0100
   superseded: there is no selected strategy between B-1 and B-2 because the
   Rust core implements neither, and no §6 oracle to measure against. Issue
   0007 is Done with that criterion left unchecked and its rationale recorded,
   rather than checked off or deleted.
4. **0030 ratification** — taken. ED-0100 ratified the bounds; ED-0103 ratified
   the numeric policy as NUMERICAL-MODEL §12. Entity numeric bounds and format
   numeric limits are final: FORMAT-SPEC row L14 is Ratified and the ED-0092
   provisional-constant gate is closed.
5. **Phase 0 performance-evidence chain** — 0007 recorded its measurements
   (`docs/evidence/0007-measurements-macOS-arm64.md`); 0030 defined its
   criterion before judging and judged. The judgment fired ADR 0002's revision
   trigger 3, the owner moved the core to Rust, and ADR 0002 is Superseded by
   ADR 0008. The chain ran to completion; it did not end in ratification of the
   Go stack, which is a result rather than an omission.

**The model document is no longer Candidate.** Its status header now reads
partly ratified and says which sections are in force (§§3, 12, 13, 14) and
which are retained only as the record of alternatives considered.

**Residual, and where it lives.** Two obligations against BEHAVIOR-SPEC survive
this issue and belong to 0030: the exact area and half-even display contract
(N-4) and exact positive-area component linking (N-5), plus the join-admission
repair in N-2. None is a numerical-model decision — each is implementation
conformance to a rule this issue's model already requires.
