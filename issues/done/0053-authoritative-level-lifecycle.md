# 0053: Align authoritative level lifecycle with the reviewed specification

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0010, 0020, 0042

## Problem

Section 2.2 requires at least one level and a new document with elevation-zero
"Level 1". The editor baseline currently uses "Ground", and low-level native
sessions can contain no levels. Before claiming complete level conformance,
audit these distinctions across document validation, file opening, CLI and GUI.
The level-view controller in 0051 must not disguise authoritative gaps.

## Acceptance criteria

- [x] Audit baseline construction and section 2.2–2.3 invariants across all
      entry points; distinguish unpublished low-level builder states from
      valid saved documents, with an explicit compatibility assessment.
- [x] Implement the reviewed baseline and last/nonempty/referenced-level
      deletion constraints without making the baseline undoable or destroying
      existing user documents through a silent migration.
- [x] Test accepted and rejected level lifecycle operations through the core,
      CLI and native GUI boundary, including undo/redo and reopen.
- [x] Full verification passes; remaining authoritative level-field controls
      and validation gaps are either delivered or separately tracked.

## Coordination

Codex owns compatibility decisions, bridge/Flutter integration and full
verification. Claude Opus owns core lifecycle guards and tests; Claude Sonnet
owns CLI baseline/level commands and tests after the shared contract review.
Empty replay snapshots and low-level builders must remain distinct from a
valid materialized plan. Existing documents are never silently renamed.

## Closure

- **Implementation/spec commits:** `26f09fc`.
- **Verification:** `./scripts/verify.sh` exited 0 on `26f09fc` with
  `=== VERIFICATION PASSED === all stages`, including 107 core document
  tests, 127 CLI tests, 50 bridge tests, 131 Flutter tests, the new P16 CLI
  lifecycle probe and clean macOS bundle/runtime/first-frame checks.
  [Compatibility, review and performance evidence](../../docs/evidence/0053-level-lifecycle/VERIFIED.md)
  includes the demonstrated canonical-reordering bypass and its regression.
- **Follow-up issues:** 0054 owns level-field controls and validation warnings.
  Future room/other entity schemas (0010 and their existing issues) must extend
  the blocker inventory; this increment covers the currently implemented types.
