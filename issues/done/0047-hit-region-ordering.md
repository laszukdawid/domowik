# 0047: Order equal-distance region hits by canonical geometry

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0012

## Problem

Native hit ranking compares class, distance and closest point, then jumps
directly to entity ID. R7.1.3 requires a region's canonical outer-ring tuple
before that ID. Overlapping regions at distance zero therefore select by
identity instead of the specified geometry. The existing identical-wall test
cannot reveal the missing key.

## Acceptance criteria

- [x] Unequal region shapes with equal distance/closest point compare by
      canonical geometry before ID; identical geometry still compares by ID.
- [x] Document order and endpoint reversal do not change the ranking.
- [x] Canonical keys are cached at index construction, not rebuilt per query.
- [x] A regression fails on the original comparator; separate build/one-shot/
      indexed query benchmarks include the tie-heavy case.
- [x] Full verification passes with no dependency or document-format change.

## Boundaries

This supplies the missing tuple component, not an exact-arithmetic distance
predicate. Existing f64 distance/projection comparison limitations remain
explicit in issue 0012 and are not discharged by these ordering tests.

## Closure

- **Implementation/spec commits:** `8fad110`.
- **Follow-up issues:** 0045 integrates consistent editor targeting; 0012 owns
  exact distance comparisons and remaining spatial-query guarantees.

Scoped correctness and measured construction/query costs are recorded in
`docs/evidence/0047-hit-ordering/MEASURED.md`. Full `./scripts/verify.sh`
passed against `8fad110`, including compiled benchmark targets.
