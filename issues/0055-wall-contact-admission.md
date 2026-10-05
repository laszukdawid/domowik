# 0055: Enforce wall contact refusals and semantic overlap warnings

- **Phase:** 2
- **Priority:** High
- **Status:** In Progress
- **Depends on:** 0001, 0037

## Problem

The shared edit algebra does not implement every normative R3.1.2 contact
class or its precedence. A root CLI reproduction on 2026-09-05 creates wall
A `(0,0)→(4000,0)` and then wall B `(2000,0)→(6000,0)`, both 200 mm thick
on the same level. The second command succeeds, reports two splits and two
joins, and materializes four walls, including two identical
`(2000,0)→(4000,0)` axes. `validate --json` reports valid.

R3.1.2 instead requires the entire second command to refuse with
`collinear-overlap` before contact resolution. The existing endpoint-on-interior
classifier induces splits even for collinear axes; the absence of a proper
crossing does not mean no split occurs. This reproduction corrects an initial
review claim that only two unsplit walls would remain.

## Acceptance criteria

- [ ] Implement and test R3.1.2 precedence for full coincidence, positive-length
      collinear overlap and zero-length split-child refusal, including reversed
      axes, endpoint-only touching, different levels and numerical boundaries.
- [ ] Cover wall creation and geometry edits through the shared core, including
      final affected positions and atomic incidental effects; CLI and GUI must
      not maintain separate contact rules.
- [ ] Define and test the admission/compatibility boundary for raw operations,
      compensation and newly admitted external batches. Do not silently change
      replay of historical files or assume a gesture-only guard covers them.
- [x] Surface the non-rejecting `wall-area-overlap` class after final-state
      validation, including disjoint nonparallel axes, with deterministic
      diagnostics separate from errors.
- [x] Rejections preserve document, log, history, dirty state and existing file
      bytes; native-backed GUI and CLI process tests exercise the refusal.
- [ ] New geometric predicates use proven floating filters before exact
      fallback and land with adversarial tests and benchmarks; measure affected
      edit costs and avoid unbounded document-wide pairwise checks per gesture.
- [x] Full repository verification passes, with remaining scope stated honestly.

## Provenance

Original Domowik specification and independently constructed axis example;
no comparable-application source, fixtures or assets consulted.

## Progress

The axis-admission implementation and numerical measurements are recorded in
[the progress evidence](../docs/evidence/0055-wall-contact/PROGRESS.md).
[Area-overlap and spatial-warning measurements](../docs/evidence/0055-wall-contact/AREA-WARNINGS.md)
now cover filtered and exact rectangle paths, bulk scratch queries, and
whole-document warning traversal at 100, 500, and 2,000 walls. The exported
warning entry point is shared by CLI and native bridge callers and remains
separate from rejecting admission errors. The issue remains In Progress:
crossing C1 and the join-policy boundary tracked by 0030, raw-operation
semantics, and intersection construction are not complete. The evidence
distinguishes measured warning traversal from full verification or a full
R3.1.2 conformance claim.
