# 0002: Behavior specification for room topology and identity

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0001

## Problem

Define room topology, automatic detection, persisted metadata, and identity
reconciliation. Resolve the current architectural question: room polygons and
areas are derived, while user names/styles need stable persistence across wall
edits.

## Acceptance criteria

- [x] Input arrangement, intersection splitting, bounded faces, exterior,
      holes, nesting, degenerate regions, and opening behavior are specified.
      (`BEHAVIOR-SPEC.md` R5.1.1–R5.1.9; openings excluded from enclosure by
      R5.1.5 with the abstract F36 vector.)
- [x] Authoritative room metadata is separated from derived polygon and area.
      (R5.2.1 record fields; R5.3.1/R5.3.5 exact-real derived area and
      non-authority; R5.1.8 face-identity prohibition.)
- [x] Deterministic split, merge, disappearance, recreation, and orphan rules
      are illustrated with IDs and operation batches. (R5.2.4–R5.2.7;
      fixtures F28–F31, F33, F34, F37a–j.)
- [x] Manual room input is either explicitly excluded from v0.1 or modeled
      separately from automatic topology. (Excluded: R5.1.7,
      `manual-room-geometry-unsupported`.)
- [x] Area boundary and rounding have coordinate examples. (Base arrangement
      B4 ring/area; R5.3.2 ties-to-even examples incl. the F26 tie at
      30,245,000 mm² → `30.24 m²`; fixed m² exception to display preference.)
- [x] Normative rules have study-log entries and a provenance review.
      (Fourteen study-log entries including superseded rounds;
      luna0002 approval recorded 2026-08-23 after four review rounds.)

## Closure

- **Main repository commit:** `3ab0c51`
- **Review:** Ox Alpha worker and GPT-5.6 Luna adversarial reviewer completed
  four review rounds (15 + 9 + 4 findings, then confirmation) and Luna
  approved provenance and semantics on 2026-08-23. Independent Fraction-exact
  verification of all fixture vectors (112 checks) is recorded in the issue
  handoff, not committed to this repository.
