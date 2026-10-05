# 0050: Preserve distinct native hit features during deduplication

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0012, 0047

## Problem

The native hit query deduplicates adjacent entries by class and entity ID.
Two free endpoints on one short wall have different closest-point keys but
can be merged anyway; inserting an intervening candidate changes whether
both survive. R6.3.2 permits deduplication only when full keys agree and never
merges different classes. First-candidate targeting hides the problem.

## Acceptance criteria

- [x] Distinct endpoint positions survive even when class/entity match;
      truly identical full-key features deduplicate and joined endpoints
      continue to contribute their join node, not member handles.
- [x] Regression tests cover adjacent/interleaved same-entity endpoints,
      document/endpoint order invariance and unchanged first-candidate order.
- [x] A before/fix regression is demonstrated and an endpoint-heavy indexed
      benchmark supplements the existing native query measurements.
- [x] Full verification passes with no dependency or format change.

## Coordination

Claude Opus owns `crates/domowik-geometry/src/hit.rs` and its spatial benchmark.
Codex owns tracker/evidence and integration. Full numerical exactness remains
issue 0012; this corrects feature identity, not binary64 distance arithmetic.

## Closure

- **Implementation/spec commits:** `ff1214d`.
- **Verification:** 118 geometry tests pass. The adjacent-endpoint regression
  was proved red against the old predicate, then green with full-key equality.
  [Controlled measurements](../../docs/evidence/0050-hit-features/MEASURED.md)
  compare old, unguarded and guarded implementations without hiding residual
  costs. `./scripts/verify.sh` exited 0 on `ff1214d` with
  `=== VERIFICATION PASSED === all stages`.
- **Follow-up issues:** 0049 owns selection-level entity projection; it must
  not change the raw native feature inventory to implement click behavior.
